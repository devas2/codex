//! Status-line formatting helpers for the virtual pet.

use ratatui::prelude::Stylize;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;

use super::pet::animated_expression;
use super::state::PetState;

pub(crate) const ENERGY_BAR_LENGTH: usize = 10;
pub(crate) const FILLED_BAR_CHAR: char = '●';
pub(crate) const EMPTY_BAR_CHAR: char = '○';

// ccpet-inspired fixed colors for pet line segments.
fn style_expression() -> Style {
    Style::default().light_yellow().bold()
}
fn style_energy_bar() -> Style {
    Style::default().light_green()
}
fn style_energy_value() -> Style {
    Style::default().light_cyan()
}
fn style_accumulated() -> Style {
    // slate gray (#778899) ≈ gray
    Style::default().gray()
}
fn style_lifetime() -> Style {
    // magenta / hot pink for 💖 lifetime
    Style::default().light_magenta()
}

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

/// Colored segments of the ccpet-style pet status line (line 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PetLineParts {
    pub expression: String,
    pub energy_bar: String,
    pub energy_value: String,
    /// Parenthesized remainder tokens toward next energy, e.g. `(171.6K)`.
    pub accumulated: String,
    /// Heart + lifetime total, e.g. `💖171.6K`.
    pub lifetime: String,
}

impl PetLineParts {
    pub(crate) fn from_state(state: &PetState, frame_index: u64) -> Self {
        let expression = animated_expression(state, frame_index, /*emoji_enabled*/ true);
        let energy_bar = generate_energy_bar(state.energy);
        let energy_value = format!("{:.2}", state.energy);
        let accumulated = format!("({})", format_token_count(state.accumulated_tokens));
        let lifetime = format!("💖{}", format_token_count(state.total_lifetime_tokens));
        Self {
            expression,
            energy_bar,
            energy_value,
            accumulated,
            lifetime,
        }
    }

    pub(crate) fn join_plain(&self) -> String {
        format!(
            "{} {} {} {} {}",
            self.expression, self.energy_bar, self.energy_value, self.accumulated, self.lifetime
        )
    }
}

/// Default ccpet-style line1 composite.
///
/// Uses a stable (non-cycling) expression so the footer does not flicker and
/// snapshot tests stay deterministic. Optional frame-based animation is still
/// available via [`animated_expression`] for richer UIs.
pub(crate) fn format_pet_line(state: &PetState, frame_index: u64) -> String {
    // Prefer the energy-derived static face (frame 0 of the happy cycle is (^_^)).
    PetLineParts::from_state(state, frame_index).join_plain()
}

/// Pet line parts for multi-color status rendering (stable frame 0).
pub(crate) fn pet_line_parts(state: &PetState) -> PetLineParts {
    PetLineParts::from_state(state, /*frame_index*/ 0)
}

/// Multi-color pet status line (expression / bar / energy / accum / lifetime).
pub(crate) fn format_pet_line_colored(state: &PetState) -> Line<'static> {
    let parts = pet_line_parts(state);
    Line::from(vec![
        Span::styled(parts.expression, style_expression()),
        Span::raw(" "),
        Span::styled(parts.energy_bar, style_energy_bar()),
        Span::raw(" "),
        Span::styled(parts.energy_value, style_energy_value()),
        Span::raw(" "),
        Span::styled(parts.accumulated, style_accumulated()),
        Span::raw(" "),
        Span::styled(parts.lifetime, style_lifetime()),
    ])
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

    #[test]
    fn pet_line_parts_are_separately_colored() {
        let state = PetState::new_default(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
        let line = format_pet_line_colored(&state);
        // expression, space, bar, space, energy, space, accum, space, lifetime = 9 spans
        assert_eq!(line.spans.len(), 9);
        assert_ne!(line.spans[0].style.fg, line.spans[2].style.fg);
        assert_ne!(line.spans[2].style.fg, line.spans[4].style.fg);
        assert_ne!(line.spans[4].style.fg, line.spans[6].style.fg);
        assert_ne!(line.spans[6].style.fg, line.spans[8].style.fg);
    }
}
