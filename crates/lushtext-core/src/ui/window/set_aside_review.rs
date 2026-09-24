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
use crate::services::draft_service::set_aside;
use crate::services::draft_service::set_aside_retention::{SetAsideTotals, notice_due};
use crate::services::json_store;
use crate::services::notifications::NotificationSeverity;
use crate::ui::preferences::{drafts_phrase, lower_bound_prefix};

use super::LushtextWindow;

/// The process's in-memory set-aside review state, owned by the application.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SetAsideReviewState {
    /// The once-per-process evaluation after the first startup restore ran.
    pub startup_evaluated: bool,
    /// `set_aside::placements()` when the last evaluation started.
    pub placements_seen: u64,
    /// An evaluation is running off GTK.
    pub evaluation_inflight: bool,
    /// The totals the last notice named.
    pub last_notified: Option<SetAsideTotals>,
    /// The user ran the review action in this process.
    pub reviewed: bool,
    /// Notices published in this process (evidence for tests and diagnostics).
    pub notices_published: u64,
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
        if app.set_aside_review_state().startup_evaluated {
            return;
        }
        app.update_set_aside_review_state(|state| state.startup_evaluated = true);
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
        if state.startup_evaluated
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
        if app.set_aside_review_state().evaluation_inflight {
            return;
        }
        app.update_set_aside_review_state(|state| {
            state.evaluation_inflight = true;
            state.placements_seen = set_aside::placements();
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
                    .filter(|_| notice_due(totals, state.last_notified, state.reviewed))
                    .zip(window);
                app.update_set_aside_review_state(|state| {
                    state.evaluation_inflight = false;
                    if let Some((totals, _)) = &notice {
                        state.last_notified = Some(*totals);
                        state.notices_published = state.notices_published.saturating_add(1);
                    }
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
