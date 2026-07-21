//! Theme-derived styling for the configurable footer statusline.

use ratatui::prelude::Stylize;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;

use super::status_line_setup::StatusLineItem;
use crate::render::highlight::foreground_style_for_scopes;

const STATUS_LINE_SEPARATOR: &str = " · ";
const STATUS_LINE_COLOR_SATURATION_PERCENT: u16 = 85;
const STATUS_LINE_COLOR_BRIGHTNESS_PERCENT: u16 = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StatusLineAccent {
    Model,
    Path,
    Branch,
    State,
    /// Virtual-pet composite (ccpet yellow expression).
    Pet,
    /// Session cost (ccpet gold).
    Cost,
    /// Session input tokens (ccpet green).
    Input,
    /// Session output tokens (ccpet yellow).
    Output,
    /// Cached tokens (ccpet sandy).
    Cached,
    /// Cache-write tokens.
    CacheWrite,
    /// Total / used aggregate (ccpet white).
    Total,
    /// Context metrics (ccpet cyan).
    ContextMetric,
    Limit,
    Metadata,
    Mode,
    Thread,
    Progress,
}

impl StatusLineAccent {
    fn for_item(item: StatusLineItem) -> Self {
        match item {
            StatusLineItem::ModelName
            | StatusLineItem::ModelWithReasoning
            | StatusLineItem::Reasoning => Self::Model,
            StatusLineItem::CurrentDir | StatusLineItem::ProjectRoot => Self::Path,
            StatusLineItem::GitBranch
            | StatusLineItem::PullRequestNumber
            | StatusLineItem::BranchChanges => Self::Branch,
            StatusLineItem::Status => Self::State,
            StatusLineItem::Pet => Self::Pet,
            StatusLineItem::SessionCost => Self::Cost,
            StatusLineItem::TotalInputTokens => Self::Input,
            StatusLineItem::TotalOutputTokens => Self::Output,
            StatusLineItem::CachedTokens => Self::Cached,
            StatusLineItem::CacheWriteTokens => Self::CacheWrite,
            StatusLineItem::UsedTokens => Self::Total,
            StatusLineItem::ContextRemaining
            | StatusLineItem::ContextUsed
            | StatusLineItem::ContextWindowSize => Self::ContextMetric,
            StatusLineItem::FiveHourLimit | StatusLineItem::WeeklyLimit => Self::Limit,
            StatusLineItem::CodexVersion | StatusLineItem::SessionId => Self::Metadata,
            StatusLineItem::FastMode | StatusLineItem::RawOutput => Self::Mode,
            StatusLineItem::Permissions => Self::Mode,
            StatusLineItem::ApprovalMode => Self::Mode,
            StatusLineItem::ThreadTitle | StatusLineItem::WorkspaceHeadline => Self::Thread,
            StatusLineItem::TaskProgress => Self::Progress,
        }
    }

    fn scopes(self) -> &'static [&'static str] {
        match self {
            Self::Model => &["entity.name.type", "support.type", "variable"],
            Self::Path => &["string", "markup.underline.link"],
            Self::Branch => &["entity.name.function", "entity.name.tag"],
            Self::State => &["keyword.control", "keyword"],
            Self::Total | Self::Progress => &["constant.numeric", "constant"],
            Self::Pet => &["markup.heading", "entity.name.section"],
            Self::Cost => &["string.regexp", "constant.character"],
            Self::Input => &["markup.inserted", "string"],
            Self::Output => &["markup.changed", "constant.language"],
            Self::Cached | Self::CacheWrite => &["entity.name.tag", "support.type"],
            Self::ContextMetric => &["support.function", "variable.parameter"],
            Self::Limit => &["constant.language", "storage.type"],
            Self::Metadata => &["comment", "constant.other"],
            Self::Mode => &["storage.modifier", "keyword.operator"],
            Self::Thread => &["markup.heading", "entity.name.section"],
        }
    }

    /// Fixed vivid colors inspired by ccpet defaults (ANSI / light palette).
    fn fallback_style(self) -> Style {
        match self {
            Self::Model | Self::State | Self::Mode => Style::default().cyan(),
            Self::Path => Style::default().green(),
            Self::Branch | Self::Limit | Self::Thread => Style::default().magenta(),
            Self::Progress => Style::default().green(),
            // ccpet: PET_EXPRESSION yellow bright bold
            Self::Pet => Style::default().light_yellow().bold(),
            // ccpet COST gold → yellow
            Self::Cost => Style::default().yellow().bold(),
            // SESSION_INPUT green
            Self::Input => Style::default().light_green(),
            // SESSION_OUTPUT yellow
            Self::Output => Style::default().light_yellow(),
            // SESSION_CACHED sandy → light red / yellow-ish; use light red for contrast
            Self::Cached => Style::default().light_red(),
            Self::CacheWrite => Style::default().red(),
            // SESSION_TOTAL white
            Self::Total => Style::default().white().bold(),
            // CONTEXT_* cyan family
            Self::ContextMetric => Style::default().light_cyan(),
            Self::Metadata => Style::default().dark_gray(),
        }
    }

    /// Pet / session metrics keep fixed vivid colors so they don't collapse to one
    /// theme scope (ccpet's multi-color status bar).
    fn prefers_fixed_color(self) -> bool {
        matches!(
            self,
            Self::Pet
                | Self::Cost
                | Self::Input
                | Self::Output
                | Self::Cached
                | Self::CacheWrite
                | Self::Total
                | Self::ContextMetric
        )
    }
}

/// Which visual row a status-line item belongs on (ccpet-style multi-line layout).
///
/// Rows are emitted only when they have content, so a pet-only selection still
/// renders as a single line.
///
/// Layout mirrors ccpet:
/// 1. pet expression / energy
/// 2. `Input · Output · Cached · Total` (session token breakdown only)
/// 3. cost + context-window metrics
/// 4. model / path / git / other ambient context
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum StatusLineRow {
    /// Pet expression / energy bar.
    Pet = 0,
    /// Session token breakdown: Input / Output / Cached / Total.
    SessionTokens = 1,
    /// Estimated cost + context window stats.
    CostAndContext = 2,
    /// Model, path, git, limits, and other ambient context.
    Ambient = 3,
}

impl StatusLineRow {
    fn for_item(item: StatusLineItem) -> Self {
        match item {
            StatusLineItem::Pet => Self::Pet,
            StatusLineItem::TotalInputTokens
            | StatusLineItem::TotalOutputTokens
            | StatusLineItem::CachedTokens
            | StatusLineItem::CacheWriteTokens
            | StatusLineItem::UsedTokens => Self::SessionTokens,
            StatusLineItem::SessionCost
            | StatusLineItem::ContextRemaining
            | StatusLineItem::ContextUsed
            | StatusLineItem::ContextWindowSize => Self::CostAndContext,
            _ => Self::Ambient,
        }
    }
}

pub(crate) fn status_line_from_segments<I>(
    segments: I,
    use_theme_colors: bool,
) -> Option<Line<'static>>
where
    I: IntoIterator<Item = (StatusLineItem, String)>,
{
    // Single-line helper (preview / unit tests): preserve flat " · " joining.
    status_line_from_segments_with_resolver(segments, use_theme_colors, |accent| {
        foreground_style_for_scopes(accent.scopes())
    })
}

/// Build a multi-line status surface (pet / usage / context), ccpet-style.
pub(crate) fn status_lines_from_segments<I>(
    segments: I,
    use_theme_colors: bool,
) -> Option<Vec<Line<'static>>>
where
    I: IntoIterator<Item = (StatusLineItem, String)>,
{
    status_lines_from_segments_with_pet(
        segments,
        /*pet_row*/ None,
        use_theme_colors,
    )
}

/// Like [`status_lines_from_segments`], but injects a pre-colored pet row as line 1
/// when `pet_row` is `Some` (and any `StatusLineItem::Pet` text segments are skipped).
pub(crate) fn status_lines_from_segments_with_pet<I>(
    segments: I,
    pet_row: Option<Line<'static>>,
    use_theme_colors: bool,
) -> Option<Vec<Line<'static>>>
where
    I: IntoIterator<Item = (StatusLineItem, String)>,
{
    status_lines_from_segments_with_resolver(
        segments,
        pet_row,
        use_theme_colors,
        |accent| foreground_style_for_scopes(accent.scopes()),
    )
}

fn status_line_from_segments_with_resolver<I, F>(
    segments: I,
    use_theme_colors: bool,
    theme_style_for_accent: F,
) -> Option<Line<'static>>
where
    I: IntoIterator<Item = (StatusLineItem, String)>,
    F: Fn(StatusLineAccent) -> Option<Style>,
{
    let mut spans = Vec::new();
    for (item, text) in segments {
        if !spans.is_empty() {
            spans.push(STATUS_LINE_SEPARATOR.dim());
        }
        let style = style_for_status_item(item, use_theme_colors, &theme_style_for_accent);
        spans.push(Span::styled(text, style));
    }
    (!spans.is_empty()).then(|| Line::from(spans))
}

fn style_for_status_item<F>(
    item: StatusLineItem,
    use_theme_colors: bool,
    theme_style_for_accent: &F,
) -> Style
where
    F: Fn(StatusLineAccent) -> Option<Style>,
{
    let accent = StatusLineAccent::for_item(item);
    let style = if use_theme_colors && accent.prefers_fixed_color() {
        // Keep ccpet-like multi-color pet/session metrics (do not collapse via theme).
        accent.fallback_style()
    } else if use_theme_colors {
        soften_status_line_style(
            theme_style_for_accent(accent).unwrap_or_else(|| accent.fallback_style()),
        )
    } else {
        Style::default().dim()
    };
    if item == StatusLineItem::PullRequestNumber {
        style.underlined()
    } else {
        style
    }
}

fn status_lines_from_segments_with_resolver<I, F>(
    segments: I,
    pet_row: Option<Line<'static>>,
    use_theme_colors: bool,
    theme_style_for_accent: F,
) -> Option<Vec<Line<'static>>>
where
    I: IntoIterator<Item = (StatusLineItem, String)>,
    F: Fn(StatusLineAccent) -> Option<Style>,
{
    let mut pet_spans = Vec::new();
    let mut session_token_spans = Vec::new();
    let mut cost_and_context_spans = Vec::new();
    let mut ambient_spans = Vec::new();

    for (item, text) in segments {
        // Prefer the pre-colored pet row when provided.
        if item == StatusLineItem::Pet && pet_row.is_some() {
            continue;
        }
        let spans = match StatusLineRow::for_item(item) {
            StatusLineRow::Pet => &mut pet_spans,
            StatusLineRow::SessionTokens => &mut session_token_spans,
            StatusLineRow::CostAndContext => &mut cost_and_context_spans,
            StatusLineRow::Ambient => &mut ambient_spans,
        };
        if !spans.is_empty() {
            spans.push(STATUS_LINE_SEPARATOR.dim());
        }
        let style = style_for_status_item(item, use_theme_colors, &theme_style_for_accent);
        spans.push(Span::styled(text, style));
    }

    let mut lines = Vec::new();
    if let Some(pet_row) = pet_row {
        lines.push(pet_row);
    } else if !pet_spans.is_empty() {
        lines.push(Line::from(pet_spans));
    }
    for spans in [
        session_token_spans,
        cost_and_context_spans,
        ambient_spans,
    ] {
        if !spans.is_empty() {
            lines.push(Line::from(spans));
        }
    }
    (!lines.is_empty()).then_some(lines)
}

fn soften_status_line_style(mut style: Style) -> Style {
    if let Some(fg) = style.fg {
        style.fg = Some(soften_status_line_color(fg));
    }
    style
}

#[allow(clippy::disallowed_methods)]
fn soften_status_line_color(color: Color) -> Color {
    match color {
        Color::Rgb(r, g, b) => {
            let luma = weighted_luma(r, g, b);
            Color::Rgb(
                soften_rgb_channel(r, luma),
                soften_rgb_channel(g, luma),
                soften_rgb_channel(b, luma),
            )
        }
        Color::LightRed => Color::Red,
        Color::LightGreen => Color::Green,
        Color::LightYellow => Color::Yellow,
        Color::LightBlue => Color::Blue,
        Color::LightMagenta => Color::Magenta,
        Color::LightCyan => Color::Cyan,
        Color::White => Color::Gray,
        Color::Reset
        | Color::Black
        | Color::Red
        | Color::Green
        | Color::Yellow
        | Color::Blue
        | Color::Magenta
        | Color::Cyan
        | Color::Gray
        | Color::DarkGray
        | Color::Indexed(_) => color,
    }
}

fn weighted_luma(r: u8, g: u8, b: u8) -> u16 {
    (77 * u16::from(r) + 150 * u16::from(g) + 29 * u16::from(b)) / 256
}

fn soften_rgb_channel(channel: u8, luma: u16) -> u8 {
    let channel = u16::from(channel);
    let softened = (channel * STATUS_LINE_COLOR_SATURATION_PERCENT
        + luma * (100 - STATUS_LINE_COLOR_SATURATION_PERCENT)
        + 50)
        / 100;

    ((softened * STATUS_LINE_COLOR_BRIGHTNESS_PERCENT + 50) / 100) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use ratatui::style::Modifier;

    fn line_text(line: &Line<'static>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    #[test]
    fn status_line_segments_preserve_order_and_plain_text() {
        let line = status_line_from_segments_with_resolver(
            [
                (StatusLineItem::ModelName, "gpt-5".to_string()),
                (StatusLineItem::CurrentDir, "/repo".to_string()),
                (StatusLineItem::GitBranch, "main".to_string()),
            ],
            /*use_theme_colors*/ true,
            |_| None,
        )
        .expect("status line");

        assert_eq!(line_text(&line), "gpt-5 · /repo · main");
        assert_eq!(line.spans[0].style.fg, Some(Color::Cyan));
        assert!(!line.spans[0].style.add_modifier.contains(Modifier::DIM));
        assert_eq!(line.spans[2].style.fg, Some(Color::Green));
        assert!(!line.spans[2].style.add_modifier.contains(Modifier::DIM));
        assert_eq!(line.spans[4].style.fg, Some(Color::Magenta));
        assert!(!line.spans[4].style.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn status_line_segments_dim_separators_and_use_theme_styles_first() {
        let line = status_line_from_segments_with_resolver(
            [
                (StatusLineItem::ModelName, "gpt-5".to_string()),
                (StatusLineItem::ContextUsed, "Context 12% used".to_string()),
            ],
            /*use_theme_colors*/ true,
            |accent| match accent {
                StatusLineAccent::Model => Some(Style::default().red()),
                _ => None,
            },
        )
        .expect("status line");

        assert_eq!(line.spans[0].style.fg, Some(Color::Red));
        assert!(!line.spans[0].style.add_modifier.contains(Modifier::DIM));
        assert!(line.spans[1].style.add_modifier.contains(Modifier::DIM));
        // Context metrics use fixed ccpet-like cyan (not theme Usage green).
        assert_eq!(line.spans[2].style.fg, Some(Color::LightCyan));
        assert!(!line.spans[2].style.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    #[allow(clippy::disallowed_methods)]
    fn status_line_segments_soften_rgb_theme_styles_without_dimming_text() {
        let line = status_line_from_segments_with_resolver(
            [(StatusLineItem::ModelName, "gpt-5".to_string())],
            /*use_theme_colors*/ true,
            |_| Some(Style::default().fg(Color::Rgb(255, 0, 0))),
        )
        .expect("status line");

        assert_eq!(line.spans[0].style.fg, Some(Color::Rgb(228, 11, 11)));
        assert!(!line.spans[0].style.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn status_line_segments_can_disable_theme_colors() {
        let line = status_line_from_segments_with_resolver(
            [
                (StatusLineItem::ModelName, "gpt-5".to_string()),
                (StatusLineItem::GitBranch, "main".to_string()),
            ],
            /*use_theme_colors*/ false,
            |_| Some(Style::default().red()),
        )
        .expect("status line");

        // Non-fixed accents dim when theme colors are off.
        assert_eq!(line_text(&line), "gpt-5 · main");
        assert_eq!(line.spans[0].style.fg, None);
        assert!(line.spans[0].style.add_modifier.contains(Modifier::DIM));
        assert!(line.spans[1].style.add_modifier.contains(Modifier::DIM));
        assert_eq!(line.spans[2].style.fg, None);
        assert!(line.spans[2].style.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn pull_request_number_uses_link_style() {
        let line = status_line_from_segments_with_resolver(
            [(StatusLineItem::PullRequestNumber, "PR #20252".to_string())],
            /*use_theme_colors*/ false,
            |_| None,
        )
        .expect("status line");

        assert_eq!(line.spans[0].style.fg, None);
        assert!(line.spans[0].style.add_modifier.contains(Modifier::DIM));
        assert!(
            line.spans[0]
                .style
                .add_modifier
                .contains(Modifier::UNDERLINED)
        );
    }

    #[test]
    fn status_line_segments_return_none_when_empty() {
        assert_eq!(
            status_line_from_segments_with_resolver(
                Vec::<(StatusLineItem, String)>::new(),
                /*use_theme_colors*/ true,
                |_| None,
            ),
            None
        );
    }
}
