//! Status-line formatting helpers for the virtual pet.

use super::pet::animated_expression;
use super::state::PetState;

pub(crate) const ENERGY_BAR_LENGTH: usize = 10;
pub(crate) const FILLED_BAR_CHAR: char = '●';
pub(crate) const EMPTY_BAR_CHAR: char = '○';

/// Compact token count: raw / x.xK / x.xxM.
pub(crate) fn format_token_count(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.2}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}K", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

pub(crate) fn generate_energy_bar(energy: f64) -> String {
    let energy = if energy.is_nan() {
        0.0
    } else {
        energy.clamp(0.0, 100.0)
    };
    let filled = ((energy / 100.0) * ENERGY_BAR_LENGTH as f64).round() as usize;
    let filled = filled.min(ENERGY_BAR_LENGTH);
    let empty = ENERGY_BAR_LENGTH - filled;
    format!(
        "{}{}",
        FILLED_BAR_CHAR.to_string().repeat(filled),
        EMPTY_BAR_CHAR.to_string().repeat(empty)
    )
}

/// Default ccpet-style line1 composite.
///
/// Uses a stable (non-cycling) expression so the footer does not flicker and
/// snapshot tests stay deterministic. Optional frame-based animation is still
/// available via [`animated_expression`] for richer UIs.
pub(crate) fn format_pet_line(state: &PetState, _frame_index: u64) -> String {
    let expression = animated_expression(state, /*frame_index*/ 0, /*emoji_enabled*/ true);
    // Prefer the energy-derived static face (frame 0 of the happy cycle is (^_^)).
    // For non-happy states frame 0 also matches the static expression.
    let bar = generate_energy_bar(state.energy);
    let energy_value = format!("{:.2}", state.energy);
    let accum = format_token_count(state.accumulated_tokens);
    let lifetime = format_token_count(state.total_lifetime_tokens);
    format!("{expression} {bar} {energy_value} ({accum}) 💖{lifetime}")
}

pub(crate) fn format_session_cost_usd(cost: f64) -> String {
    // Match ccpet label style: "Cost: $x.xx"
    if cost < 0.01 && cost > 0.0 {
        format!("Cost: ${cost:.4}")
    } else {
        format!("Cost: ${cost:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use chrono::Utc;

    #[test]
    fn token_count_formatting() {
        assert_eq!(format_token_count(42), "42");
        assert_eq!(format_token_count(1500), "1.5K");
        assert_eq!(format_token_count(2_500_000), "2.50M");
    }

    #[test]
    fn energy_bar_full_and_empty() {
        assert_eq!(generate_energy_bar(100.0), "●".repeat(10));
        assert_eq!(generate_energy_bar(0.0), "○".repeat(10));
        assert_eq!(generate_energy_bar(50.0), format!("{}{}", "●".repeat(5), "○".repeat(5)));
    }

    #[test]
    fn pet_line_contains_expression() {
        let state = PetState::new_default(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
        let line = format_pet_line(&state, 0);
        assert!(line.contains("(^_^)") || line.contains("(^o^)") || line.contains("(^v^)"));
        assert!(line.contains('💖'));
    }
}
