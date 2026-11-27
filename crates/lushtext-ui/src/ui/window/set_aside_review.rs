// SPDX-License-Identifier: GPL-3.0-or-later

//! Ask the user to review the drafts set-aside area once it grows past the
//! soft retention bound.
//!
//! **Cross-cutting coordination with no role**, in the shape
//! `attention_refresh.rs` uses: it is reached from `WFR-SESSION-RESTORE`'s
//! terminal (startup restore settled) and from `WFR-DRAFT-RECOVERY`'s autosave
//! tick (a body was newly set aside), and it owns neither workflow's state.
//! Its pure half is `services::draft_service::set_aside_retention`, which
//! decides the bound and whether a notice is due.
//!
//! Crossing the bound **never deletes anything**. It publishes one window
//! status warning that names the count and size and points to the
//! `app.review-preserved-drafts` action, which opens `Preferences > Data` at
//! the Preserved Drafts group. The rate limit is in memory only, in the
//! application's [`SetAsideReviewState`]: no acknowledgment is persisted, so
//! a launch that starts over the bound notifies once more.

use gtk_lush_tasks::spawn_blocking_then_weak;
use gtk4::glib;
use gtk4::prelude::*;

use crate::app::LushtextApplication;
use crate::ui::preferences::{drafts_phrase, lower_bound_prefix};
use lushtext_services::coordination::notifications::NotificationSeverity;
use lushtext_services::persistence::draft_service::set_aside;
use lushtext_services::persistence::draft_service::set_aside_retention::{
    SetAsideTotals, notice_due,
};
use lushtext_services::persistence::json_store;

use super::LushtextWindow;

/// Lifecycle phase of the process's in-memory set-aside retention notice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SetAsideReviewPhase {
    /// The once-per-process startup evaluation has not yet run.
    #[default]
    Unstarted,
    /// Startup evaluation ran and no background scan is currently running.
    Idle {
        /// Totals the most recent notice named, if any.
        last_notified: Option<SetAsideTotals>,
    },
    /// A background scan of the set-aside area is currently running.
    Evaluating {
        /// Totals the most recent notice named, if any.
        last_notified: Option<SetAsideTotals>,
    },
}

impl SetAsideReviewPhase {
    #[must_use]
    const fn startup_evaluated(self) -> bool {
        !matches!(self, Self::Unstarted)
    }

    #[must_use]
    const fn last_notified(self) -> Option<SetAsideTotals> {
        match self {
            Self::Unstarted => None,
            Self::Idle { last_notified } | Self::Evaluating { last_notified } => last_notified,
        }
    }
}

/// The process's in-memory set-aside review state, owned by the application.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SetAsideReviewState {
    /// Algebraic lifecycle phase (startup evaluation, idle, or scanning).
    pub phase: SetAsideReviewPhase,
    /// `set_aside::placements()` when the last evaluation started.
    pub placements_seen: u64,
    /// The user ran the review action in this process.
    pub reviewed: bool,
    /// Notices published in this process (evidence for tests and diagnostics).
    pub notices_published: u64,
}

impl SetAsideReviewState {
    /// Whether an off-GTK evaluation is currently in flight.
    #[must_use]
    pub const fn evaluation_inflight(&self) -> bool {
        matches!(self.phase, SetAsideReviewPhase::Evaluating { .. })
    }

    fn begin_startup(&mut self) {
        if matches!(self.phase, SetAsideReviewPhase::Unstarted) {
            self.phase = SetAsideReviewPhase::Idle {
                last_notified: None,
            };
        }
    }

    fn start_evaluation(&mut self, placements_seen: u64) {
        let last_notified = self.phase.last_notified();
        self.phase = SetAsideReviewPhase::Evaluating { last_notified };
        self.placements_seen = placements_seen;
    }

    fn finish_evaluation(&mut self, notified: Option<SetAsideTotals>) {
        let last_notified = notified.or_else(|| self.phase.last_notified());
        self.phase = SetAsideReviewPhase::Idle { last_notified };
        if notified.is_some() {
            self.notices_published = self.notices_published.saturating_add(1);
        }
    }
}

/// The review notice: names the count and size and the review action, in the
/// wording the Preserved Drafts summary row uses.
fn review_notice_text(totals: SetAsideTotals) -> String {
    format!(
        "LushText is keeping {}{} ({}). Review them with Review Preserved Drafts.",
        lower_bound_prefix(totals),
        drafts_phrase(totals.count),
        glib::format_size(totals.bytes)
    )
}

impl LushtextWindow {
    fn lushtext_application(&self) -> Option<LushtextApplication> {
        self.application().and_downcast::<LushtextApplication>()
    }

    /// The once-per-process evaluation after the first window's startup
    /// restore settles. Later windows and later restores do nothing here.
    pub(crate) fn evaluate_preserved_drafts_after_startup(&self) {
        let Some(app) = self.lushtext_application() else {
            return;
        };
        if app.set_aside_review_state().phase.startup_evaluated() {
            return;
        }
        app.update_set_aside_review_state(SetAsideReviewState::begin_startup);
        self.evaluate_preserved_drafts();
    }

    /// Re-evaluate after a body was newly set aside since the last
    /// evaluation. Called from the autosave tick; costs one atomic load when
    /// nothing was placed.
    pub(crate) fn evaluate_preserved_drafts_if_placed(&self) {
        let Some(app) = self.lushtext_application() else {
            return;
        };
        let state = app.set_aside_review_state();
        if state.phase.startup_evaluated()
            && !state.reviewed
            && set_aside::placements() != state.placements_seen
        {
            self.evaluate_preserved_drafts();
        }
    }

    /// Scan the set-aside area off GTK and publish a notice when one is due.
    fn evaluate_preserved_drafts(&self) {
        let Some(app) = self.lushtext_application() else {
            return;
        };
        if matches!(
            app.set_aside_review_state().phase,
            SetAsideReviewPhase::Evaluating { .. }
        ) {
            return;
        }
        app.update_set_aside_review_state(|state| {
            state.start_evaluation(set_aside::placements());
        });
        // The application, not this window, receives the result: the window
        // may close first, and the in-flight flag must still clear.
        spawn_blocking_then_weak(
            &app,
            || {
                set_aside::totals(&json_store::data_dir())
                    .map_err(|error| tracing::warn!("Could not scan preserved drafts: {error}"))
                    .ok()
            },
            |app, totals| {
                let state = app.set_aside_review_state();
                let window = app.active_window().and_downcast::<LushtextWindow>();
                let notice = totals
                    .filter(|_| notice_due(totals, state.phase.last_notified(), state.reviewed))
                    .zip(window);
                let notified = notice.as_ref().map(|(totals, _)| *totals);
                app.update_set_aside_review_state(|state| {
                    state.finish_evaluation(notified);
                });
                if let Some((totals, window)) = notice {
                    window.publish_status_message(
                        &review_notice_text(totals),
                        NotificationSeverity::Warning,
                    );
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notice_names_the_count_the_size_and_the_review_action() {
        let text = review_notice_text(SetAsideTotals {
            count: 120,
            bytes: 2_000,
            complete: true,
        });
        assert!(text.starts_with("LushText is keeping 120 preserved drafts ("));
        assert!(text.ends_with("Review them with Review Preserved Drafts."));
        let partial = review_notice_text(SetAsideTotals {
            count: 10_000,
            bytes: 1,
            complete: false,
        });
        assert!(partial.contains("at least 10,000 preserved drafts"));
    }
}
