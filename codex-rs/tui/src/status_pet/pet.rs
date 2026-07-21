//! Pet energy, decay, feed, and expression state machine (ported from ccpet).

use chrono::DateTime;
use chrono::Utc;

use super::state::PetState;

pub(crate) const INITIAL_ENERGY: f64 = 100.0;
pub(crate) const TOKENS_PER_ENERGY: u64 = 1_000_000;
/// Energy points lost per minute (~3 days from 100 → 0).
pub(crate) const DECAY_PER_MINUTE: f64 = 0.0231;
pub(crate) const MINIMUM_DECAY_MINUTES: f64 = 1.0;

pub(crate) const THRESHOLD_HAPPY: f64 = 80.0;
pub(crate) const THRESHOLD_HUNGRY: f64 = 40.0;
pub(crate) const THRESHOLD_SICK: f64 = 10.0;

pub(crate) const EXPRESSION_HAPPY: &str = "(^_^)";
pub(crate) const EXPRESSION_HUNGRY: &str = "(o_o)";
pub(crate) const EXPRESSION_SICK: &str = "(u_u)";
pub(crate) const EXPRESSION_DEAD: &str = "(x_x)";

const ANIMATED_HAPPY: &[&str] = &["(^_^)", "(^o^)", "(^_^)", "(^v^)"];
const ANIMATED_HUNGRY: &[&str] = &["(o_o)", "(O_O)", "(o_o)", "(-_-)"];
const ANIMATED_SICK: &[&str] = &["(u_u)", "(T_T)", "(u_u)", "(>_<)"];
const ANIMATED_DEAD: &[&str] = &["(x_x)", "(X_X)", "(x_x)", "(+_+)"];

/// Feed the pet with a positive token delta and update energy.
pub(crate) fn feed(state: &mut PetState, tokens: u64, now: DateTime<Utc>) {
    if tokens == 0 {
        return;
    }
    let new_accum = state.accumulated_tokens.saturating_add(tokens);
    let energy_to_add = new_accum / TOKENS_PER_ENERGY;
    let remaining = new_accum % TOKENS_PER_ENERGY;

    state.accumulated_tokens = remaining;
    state.last_feed_time = now;
    state.total_tokens_consumed = state.total_tokens_consumed.saturating_add(tokens);
    state.total_lifetime_tokens = state.total_lifetime_tokens.saturating_add(tokens);

    if energy_to_add > 0 {
        add_energy(state, energy_to_add as f64, now);
    } else {
        update_expression(state);
    }
}

/// Apply wall-clock energy decay since the last decay/feed anchor.
pub(crate) fn apply_time_decay(state: &mut PetState, now: DateTime<Utc>) {
    let last = state
        .last_decay_time
        .unwrap_or(state.last_feed_time);
    let minutes = (now - last).num_milliseconds() as f64 / 60_000.0;
    if minutes < MINIMUM_DECAY_MINUTES {
        return;
    }
    let decay = minutes * DECAY_PER_MINUTE;
    if decay > 0.0 {
        decrease_energy(state, decay);
        state.last_decay_time = Some(now);
    }
}

pub(crate) fn add_energy(state: &mut PetState, amount: f64, now: DateTime<Utc>) {
    if amount <= 0.0 || amount.is_nan() {
        return;
    }
    state.energy = (state.energy + amount).min(100.0);
    state.last_feed_time = now;
    state.last_decay_time = Some(now);
    update_expression(state);
}

pub(crate) fn decrease_energy(state: &mut PetState, amount: f64) {
    if amount <= 0.0 || amount.is_nan() {
        return;
    }
    state.energy = (state.energy - amount).max(0.0);
    update_expression(state);
}

pub(crate) fn is_dead(state: &PetState) -> bool {
    state.energy == 0.0
}

pub(crate) fn update_expression(state: &mut PetState) {
    state.expression = expression_for_energy(state.energy).to_string();
}

pub(crate) fn expression_for_energy(energy: f64) -> &'static str {
    if energy >= THRESHOLD_HAPPY {
        EXPRESSION_HAPPY
    } else if energy >= THRESHOLD_HUNGRY {
        EXPRESSION_HUNGRY
    } else if energy >= THRESHOLD_SICK {
        EXPRESSION_SICK
    } else {
        EXPRESSION_DEAD
    }
}

/// Animated (or static) expression, optionally prefixed with the animal emoji.
pub(crate) fn animated_expression(state: &PetState, frame_index: u64, emoji_enabled: bool) -> String {
    let frames = if state.energy >= THRESHOLD_HAPPY {
        ANIMATED_HAPPY
    } else if state.energy >= THRESHOLD_HUNGRY {
        ANIMATED_HUNGRY
    } else if state.energy >= THRESHOLD_SICK {
        ANIMATED_SICK
    } else {
        ANIMATED_DEAD
    };
    let base = frames[(frame_index as usize) % frames.len()];
    if emoji_enabled {
        format!("{}{base}", state.animal_emoji())
    } else {
        base.to_string()
    }
}

/// Replace the current pet with a freshly born randomized one (caller handles graveyard).
pub(crate) fn reset_to_new(state: &mut PetState, now: DateTime<Utc>) {
    *state = PetState::new_random(now);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 20, 12, 0, 0).unwrap()
    }

    fn fresh_state() -> PetState {
        PetState::new_default(fixed_now())
    }

    #[test]
    fn feed_one_million_tokens_adds_one_energy() {
        let mut state = fresh_state();
        state.energy = 50.0;
        feed(&mut state, TOKENS_PER_ENERGY, fixed_now());
        assert!((state.energy - 51.0).abs() < 1e-9);
        assert_eq!(state.accumulated_tokens, 0);
        assert_eq!(state.total_lifetime_tokens, TOKENS_PER_ENERGY);
    }

    #[test]
    fn feed_accumulates_remainder() {
        let mut state = fresh_state();
        state.energy = 10.0;
        feed(&mut state, 500_000, fixed_now());
        assert!((state.energy - 10.0).abs() < 1e-9);
        assert_eq!(state.accumulated_tokens, 500_000);
        feed(&mut state, 500_000, fixed_now());
        assert!((state.energy - 11.0).abs() < 1e-9);
        assert_eq!(state.accumulated_tokens, 0);
    }

    #[test]
    fn energy_caps_at_100() {
        let mut state = fresh_state();
        state.energy = 99.0;
        feed(&mut state, TOKENS_PER_ENERGY * 5, fixed_now());
        assert!((state.energy - 100.0).abs() < 1e-9);
    }

    #[test]
    fn decay_reduces_energy() {
        let mut state = fresh_state();
        state.energy = 100.0;
        let later = fixed_now() + chrono::Duration::minutes(100);
        apply_time_decay(&mut state, later);
        let expected = 100.0 - 100.0 * DECAY_PER_MINUTE;
        assert!((state.energy - expected).abs() < 1e-6);
    }

    #[test]
    fn no_decay_under_one_minute() {
        let mut state = fresh_state();
        state.energy = 100.0;
        let later = fixed_now() + chrono::Duration::seconds(30);
        apply_time_decay(&mut state, later);
        assert!((state.energy - 100.0).abs() < 1e-9);
    }

    #[test]
    fn expression_thresholds() {
        assert_eq!(expression_for_energy(90.0), EXPRESSION_HAPPY);
        assert_eq!(expression_for_energy(50.0), EXPRESSION_HUNGRY);
        assert_eq!(expression_for_energy(15.0), EXPRESSION_SICK);
        assert_eq!(expression_for_energy(5.0), EXPRESSION_DEAD);
        assert_eq!(expression_for_energy(0.0), EXPRESSION_DEAD);
    }

    #[test]
    fn is_dead_only_at_zero() {
        let mut state = fresh_state();
        state.energy = 5.0;
        assert!(!is_dead(&state));
        state.energy = 0.0;
        assert!(is_dead(&state));
    }
}
