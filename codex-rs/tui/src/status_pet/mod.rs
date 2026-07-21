//! Status-line virtual pet (ccpet port) with Codex token pricing.
//!
//! Separate from ambient image pets under [`crate::pets`].

mod format;
mod pet;
mod pricing;
mod state;
mod storage;

use std::path::Path;
use std::path::PathBuf;

use chrono::Utc;

use crate::token_usage::TokenUsage;

pub(crate) use format::format_pet_line;
pub(crate) use format::format_pet_line_colored;
pub(crate) use format::format_session_cost_usd;
pub(crate) use format::format_token_count;
pub(crate) use pricing::estimate_session_cost_usd;
pub(crate) use state::PetState;

/// Owns the live pet, session cost, and disk persistence for the status line.
#[derive(Debug)]
pub(crate) struct StatusPetController {
    codex_home: PathBuf,
    state: PetState,
    /// Estimated USD for the current thread (recomputed from token totals).
    session_cost_usd: f64,
    animation_frame: u64,
    dirty: bool,
}

impl StatusPetController {
    pub(crate) fn load(codex_home: &Path) -> Self {
        let now = Utc::now();
        let mut state = storage::load_state(codex_home).unwrap_or_else(|| PetState::new_default(now));
        pet::apply_time_decay(&mut state, now);
        pet::update_expression(&mut state);
        let animation_frame = storage::load_animation_frame(codex_home);
        let mut ctrl = Self {
            codex_home: codex_home.to_path_buf(),
            state,
            session_cost_usd: 0.0,
            animation_frame,
            dirty: true,
        };
        ctrl.persist_if_dirty();
        ctrl
    }

    /// Apply decay, feed any new session tokens, recompute cost, and persist.
    pub(crate) fn on_token_usage(&mut self, model: &str, total_usage: &TokenUsage) {
        let now = Utc::now();
        pet::apply_time_decay(&mut self.state, now);

        let billable = total_usage.billable_tokens().max(0) as u64;
        let last = self.state.last_fed_session_billable;
        if billable > last {
            let delta = billable - last;
            pet::feed(&mut self.state, delta, now);
            self.state.last_fed_session_billable = billable;
        } else if billable < last {
            // New thread / reset totals — reset feed cursor without draining energy.
            self.state.last_fed_session_billable = billable;
        }

        self.session_cost_usd = estimate_session_cost_usd(model, total_usage);
        self.animation_frame = self.animation_frame.wrapping_add(1);
        self.dirty = true;
        self.persist_if_dirty();
    }

    /// Re-apply decay when rendering without a new token event.
    pub(crate) fn prepare_for_display(&mut self) {
        let now = Utc::now();
        let before = self.state.energy;
        pet::apply_time_decay(&mut self.state, now);
        if (self.state.energy - before).abs() > 1e-9 {
            self.dirty = true;
            self.persist_if_dirty();
        }
        self.animation_frame = self.animation_frame.wrapping_add(1);
        storage::save_animation_frame(&self.codex_home, self.animation_frame);
    }

    pub(crate) fn pet_status_line(&mut self) -> String {
        self.prepare_for_display();
        format_pet_line(&self.state, /*frame_index*/ 0)
    }

    /// Colored pet row for the multi-line status surface.
    pub(crate) fn pet_status_line_colored(&mut self) -> ratatui::text::Line<'static> {
        self.prepare_for_display();
        format_pet_line_colored(&self.state)
    }

    pub(crate) fn session_cost_line(&self) -> Option<String> {
        if self.session_cost_usd <= 0.0 {
            // Still show $0.00 once any session activity may exist; omit only if never updated.
            // Callers can show when token totals > 0.
        }
        Some(format_session_cost_usd(self.session_cost_usd))
    }

    pub(crate) fn session_cost_usd(&self) -> f64 {
        self.session_cost_usd
    }

    pub(crate) fn set_session_cost_from_usage(&mut self, model: &str, total_usage: &TokenUsage) {
        self.session_cost_usd = estimate_session_cost_usd(model, total_usage);
    }

    pub(crate) fn state(&self) -> &PetState {
        &self.state
    }

    pub(crate) fn reset(&mut self) {
        let now = Utc::now();
        let _ = storage::move_to_graveyard(&self.codex_home, &self.state);
        pet::reset_to_new(&mut self.state, now);
        self.session_cost_usd = 0.0;
        self.dirty = true;
        self.persist_if_dirty();
    }

    pub(crate) fn status_summary(&mut self) -> String {
        self.prepare_for_display();
        let s = self.state();
        let vitality = if pet::is_dead(s) { "dead" } else { "alive" };
        format!(
            "Status pet: {} {} ({vitality})  energy={:.2}  lifetime={}  session_cost=${:.4}  born={}",
            s.animal_emoji(),
            s.pet_name,
            s.energy,
            format_token_count(s.total_lifetime_tokens),
            self.session_cost_usd,
            s.birth_time.format("%Y-%m-%d"),
        )
    }

    fn persist_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        if storage::save_state(&self.codex_home, &self.state).is_ok() {
            self.dirty = false;
        }
        storage::save_animation_frame(&self.codex_home, self.animation_frame);
    }
}
