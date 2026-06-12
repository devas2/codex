//! Startup swimming-dragon animation.

use std::time::Duration;

use color_eyre::eyre::Result;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use tokio::time::Instant;
use tokio::time::sleep;

use crate::tui::Tui;

const DRAGON: &str = "<=={====>";
const ANIMATION_DURATION: Duration = Duration::from_secs(3);
const FRAME_INTERVAL: Duration = Duration::from_millis(50);

pub(crate) async fn run(tui: &mut Tui) -> Result<()> {
    let total_frames = (ANIMATION_DURATION.as_millis() / FRAME_INTERVAL.as_millis()).max(1);
    let started_at = Instant::now();
    let mut frame_idx = 0;
    while started_at.elapsed() < ANIMATION_DURATION {
        tui.draw(/*height*/ 3, |frame| {
            let area = frame.area();
            let row = Rect::new(area.x, area.y + area.height / 2, area.width, 1);
            Paragraph::new(dragon_frame_line(area.width, frame_idx, total_frames))
                .render(row, frame.buffer_mut());
        })?;
        frame_idx += 1;
        sleep(FRAME_INTERVAL).await;
    }
    tui.terminal.clear()?;
    Ok(())
}

fn dragon_frame_line(width: u16, frame_idx: u128, total_frames: u128) -> Line<'static> {
    let width = width as usize;
    let dragon_width = DRAGON.len();
    let travel = width.saturating_sub(dragon_width);
    let x = if total_frames <= 1 {
        travel
    } else {
        (travel as u128 * frame_idx.min(total_frames - 1) / (total_frames - 1)) as usize
    };
    vec![" ".repeat(x).into(), DRAGON.green().bold()].into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragon_starts_left_and_ends_right() {
        let first = dragon_frame_line(
            /*width*/ 20, /*frame_idx*/ 0, /*total_frames*/ 3,
        );
        let last = dragon_frame_line(
            /*width*/ 20, /*frame_idx*/ 2, /*total_frames*/ 3,
        );

        assert_eq!(first.spans[0].content.as_ref(), "");
        assert_eq!(last.spans[0].content.as_ref(), " ".repeat(11));
    }
}
