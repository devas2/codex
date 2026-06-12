//! linux.do latest-topic refresh scheduling.

use super::*;
use codex_config::types::LinuxDoLatestConfig;

pub(super) struct LinuxDoLatestRefreshState {
    enabled: bool,
    refresh_interval: Duration,
    last_attempt_at: Option<Instant>,
    in_flight: bool,
}

impl LinuxDoLatestRefreshState {
    pub(super) fn from_config(config: &LinuxDoLatestConfig) -> Self {
        Self {
            enabled: config.enabled && !cfg!(test),
            refresh_interval: Duration::from_secs(config.refresh_interval_secs.max(1)),
            last_attempt_at: None,
            in_flight: false,
        }
    }

    fn next_refresh_delay(&self) -> Duration {
        self.last_attempt_at.map_or(Duration::ZERO, |attempt_at| {
            self.refresh_interval.saturating_sub(attempt_at.elapsed())
        })
    }
}

impl App {
    pub(super) fn maybe_refresh_linux_do_latest(&mut self, tui: &mut tui::Tui, force: bool) {
        if !self.linux_do_latest.enabled
            || self.linux_do_latest.in_flight
            || !tui.is_terminal_focused()
        {
            return;
        }

        let delay = self.linux_do_latest.next_refresh_delay();
        if !force && delay > Duration::ZERO {
            tui.frame_requester().schedule_frame_in(delay);
            return;
        }

        self.linux_do_latest.in_flight = true;
        self.linux_do_latest.last_attempt_at = Some(Instant::now());
        self.chat_widget
            .set_linux_do_latest_line(Some(crate::linux_do_latest::loading_line()));

        let codex_home = self.config.codex_home.to_path_buf();
        let app_event_tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            let result = crate::linux_do_latest::fetch_latest_via_webview(codex_home)
                .await
                .map_err(|err| err.to_string());
            app_event_tx.send(AppEvent::LinuxDoLatestLoaded { result });
        });
    }

    pub(super) fn handle_linux_do_latest_loaded(
        &mut self,
        tui: &mut tui::Tui,
        result: Result<crate::linux_do_latest::LinuxDoLatestFetchOutcome, String>,
    ) {
        self.linux_do_latest.in_flight = false;
        self.linux_do_latest.last_attempt_at = Some(Instant::now());

        match result {
            Ok(crate::linux_do_latest::LinuxDoLatestFetchOutcome::Post(post)) => {
                self.chat_widget.set_linux_do_latest_line(Some(
                    crate::linux_do_latest::line_for_post(&post, chrono::Utc::now()),
                ));
            }
            Ok(crate::linux_do_latest::LinuxDoLatestFetchOutcome::Busy) => {
                self.chat_widget
                    .set_linux_do_latest_line(Some(crate::linux_do_latest::busy_line()));
            }
            Err(message) => {
                self.chat_widget
                    .set_linux_do_latest_line(Some(crate::linux_do_latest::error_line(&message)));
            }
        }

        if self.linux_do_latest.enabled && tui.is_terminal_focused() {
            tui.frame_requester()
                .schedule_frame_in(self.linux_do_latest.refresh_interval);
        }
    }
}
