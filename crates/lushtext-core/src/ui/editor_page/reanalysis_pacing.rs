// SPDX-License-Identifier: GPL-3.0-or-later

//! The paced re-analysis of an edit (`bound-context-flip-keystrokes`, C6c).
//!
//! **Cross-cutting coordination, no role** (`WFR-DOCUMENT-SCALE`), beside
//! `document_scale.rs`, whose edit after-handlers start it; the pure rule is
//! `model::reanalysis_pacing`.
//!
//! After an edit in a highlighted document, GtkSourceView's context engine
//! re-analyses from the edited line in one first update of up to 10 ms at
//! `G_PRIORITY_HIGH_IDLE` and, when that is not enough, in a
//! `G_PRIORITY_LOW` worker of 30 ms slices (ledger A69). A key that opens a
//! comment with no closer after it re-analyses the rest of the document, and
//! in JavaScript and TypeScript an ordinary key re-analyses most of what
//! follows it (ledger A85).
//!
//! So the edit's after-handler arms one **step** idle at
//! `G_PRIORITY_HIGH_IDLE - 1`, which dispatches before the first update (A85).
//! Each step calls `gtk_source_buffer_ensure_highlight` on an empty range at a
//! bound it chooses: the engine analyses from its invalid point up to the line
//! after the bound, reports the range in one `highlight-updated` and tags
//! nothing (A85). A report that ends before the bound means the analysis
//! merged with the previous one, and the first update finds nothing left. A
//! report that reaches the bound continues the session with a step sized by
//! the pure rule. Once the session has spent the engine's own first-update
//! allowance without merging, syntax highlighting is removed for the session
//! (`DocumentScale::with_edit_reanalysis_reduction`): removing the language
//! removes the pending first update and worker (A85) and changes no text
//! (A27). The reduction is announced through the reduced-features notice like
//! every other.
//!
//! The steps never run while a load installation or a buffer replacement owns
//! the buffer (the watch's own condition), and a session ends without a
//! downgrade when one starts, when the language goes, or when the editor goes.

use std::cell::Cell;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;
use sourceview5::prelude::*;

use crate::model::reanalysis_pacing::{
    PacingNext, PacingSession, StepReport, after_step_within, reanalysis_allowance_us,
};

use super::LushtextEditorPage;

/// Per-editor pacing state.
#[derive(Default)]
pub struct ReanalysisPacingState {
    /// The active session, `None` when the analysis is complete.
    session: Cell<Option<PacingSession>>,
    /// Where the next step's bound is measured from: the earliest line start
    /// of an edit the session has not analysed past.
    from: Cell<i32>,
    /// Whether a step idle is queued.
    queued: Cell<bool>,
    /// Advanced when a session is cancelled, so a queued step of an older
    /// session does nothing.
    generation: Cell<u64>,
    /// Whether the `highlight-updated` handler records into `report` (only
    /// during a step).
    capturing: Cell<bool>,
    /// The first `highlight-updated` range reported during the current step.
    report: Cell<Option<(i32, i32)>>,
    sessions: Cell<u64>,
    steps: Cell<u64>,
    longest_step_us: Cell<u64>,
    merged: Cell<u64>,
    downgrades: Cell<u64>,
}

/// Observable pacing state, for tests.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReanalysisPacingEvidence {
    /// Sessions started.
    pub sessions: u64,
    /// Steps run.
    pub steps: u64,
    /// The longest step, µs.
    pub longest_step_us: u64,
    /// Sessions that ended because the analysis merged.
    pub merged: u64,
    /// Sessions that removed syntax highlighting.
    pub downgrades: u64,
    /// Whether a session is active.
    pub active: bool,
}

/// The priority of a step: one above the context engine's first update
/// (`G_PRIORITY_HIGH_IDLE`), so every step dispatches before it (ledger A85).
fn step_priority() -> glib::Priority {
    glib::Priority::from(glib::ffi::G_PRIORITY_HIGH_IDLE - 1)
}

/// The bound one step analyses to: `from + chars`, at the start of the next
/// line (the engine analyses whole lines), or the buffer end.
fn step_bound(buffer: &sourceview5::Buffer, from: i32, chars: u32) -> gtk4::TextIter {
    let chars = i32::try_from(chars).unwrap_or(i32::MAX);
    let mut iter = buffer.iter_at_offset(from.saturating_add(chars));
    if !iter.starts_line() {
        iter.forward_line();
    }
    iter
}

impl LushtextEditorPage {
    /// Observable pacing state.
    #[must_use]
    pub fn reanalysis_pacing_evidence(&self) -> ReanalysisPacingEvidence {
        let state = &self.imp().reanalysis_pacing;
        ReanalysisPacingEvidence {
            sessions: state.sessions.get(),
            steps: state.steps.get(),
            longest_step_us: state.longest_step_us.get(),
            merged: state.merged.get(),
            downgrades: state.downgrades.get(),
            active: state.session.get().is_some(),
        }
    }

    /// Record the step's `highlight-updated` report (from `constructed`).
    pub(super) fn setup_reanalysis_pacing(&self) {
        let buffer = self.buffer();
        let editor_weak = self.downgrade();
        let id = buffer.connect_highlight_updated(move |_, start, end| {
            if let Some(editor) = editor_weak.upgrade() {
                let state = &editor.imp().reanalysis_pacing;
                if state.capturing.get() && state.report.get().is_none() {
                    state.report.set(Some((start.offset(), end.offset())));
                }
            }
        });
        self.imp().document_scale.signals.track(&buffer, id);
    }

    /// Whether an edit now re-analyses through the engine: a language is
    /// attached, highlighting is on, and the session scale allows it.
    fn reanalysis_applies(&self) -> bool {
        let buffer = self.buffer();
        buffer.language().is_some()
            && buffer.is_highlight_syntax()
            && self.document_scale().syntax_enabled
    }

    /// Pace the re-analysis of an edit whose earliest changed character is
    /// at `offset` (from the edit after-handlers, after the watch step).
    pub(super) fn pace_reanalysis_after_edit(&self, offset: i32) {
        if !self.reanalysis_applies() {
            return;
        }
        let buffer = self.buffer();
        let mut line_start = buffer.iter_at_offset(offset);
        line_start.set_line_offset(0);
        let state = &self.imp().reanalysis_pacing;
        if state.session.get().is_none() {
            state.session.set(Some(PacingSession::START));
            state.from.set(line_start.offset());
            state.sessions.set(state.sessions.get().saturating_add(1));
        } else {
            state.from.set(state.from.get().min(line_start.offset()));
        }
        if state.queued.get() {
            return;
        }
        state.queued.set(true);
        let generation = state.generation.get();
        let editor_weak = self.downgrade();
        glib::idle_add_local_full(step_priority(), move || {
            let Some(editor) = editor_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if editor.run_reanalysis_step(generation) {
                glib::ControlFlow::Continue
            } else {
                editor.imp().reanalysis_pacing.queued.set(false);
                glib::ControlFlow::Break
            }
        });
    }

    /// End any session without a downgrade (a load or replacement took the
    /// buffer, the language went, a new document session began). A queued
    /// step of it does nothing.
    pub(crate) fn cancel_reanalysis_pacing(&self) {
        let state = &self.imp().reanalysis_pacing;
        state.session.set(None);
        state.generation.set(state.generation.get().wrapping_add(1));
    }

    /// Run one step of the session of `generation`; `true` keeps the idle.
    fn run_reanalysis_step(&self, generation: u64) -> bool {
        let state = &self.imp().reanalysis_pacing;
        if state.generation.get() != generation {
            return false;
        }
        let Some(session) = state.session.get() else {
            return false;
        };
        if !self.reanalysis_applies()
            || self.load_projection_suspended()
            || self.buffer_replacement_in_progress()
        {
            state.session.set(None);
            return false;
        }
        let buffer = self.buffer();
        let bound = step_bound(&buffer, state.from.get(), session.step_chars());
        state.report.set(None);
        state.capturing.set(true);
        let started = glib::monotonic_time();
        buffer.ensure_highlight(&bound, &bound);
        let took_us = u64::try_from(glib::monotonic_time().saturating_sub(started)).unwrap_or(0);
        state.capturing.set(false);
        state.steps.set(state.steps.get().saturating_add(1));
        state
            .longest_step_us
            .set(state.longest_step_us.get().max(took_us));
        let report = match state.report.take() {
            // The report ends one character before the next line it stopped
            // at (`refresh_range`); a report short of the bound merged.
            Some((start, end)) if !bound.is_end() && end.saturating_add(1) >= bound.offset() => {
                state.from.set(end);
                StepReport::ReachedBound {
                    analysed_chars: u64::try_from(end.saturating_sub(start)).unwrap_or(0),
                    took_us,
                }
            }
            _ => StepReport::Merged,
        };
        match after_step_within(session, report, reanalysis_allowance_us()) {
            PacingNext::Continue(next) => {
                state.session.set(Some(next));
                true
            }
            PacingNext::Finished => {
                state.session.set(None);
                state.merged.set(state.merged.get().saturating_add(1));
                false
            }
            PacingNext::Downgrade => {
                state.session.set(None);
                state
                    .downgrades
                    .set(state.downgrades.get().saturating_add(1));
                let previous = self.document_scale();
                self.adopt_document_scale(previous, previous.with_edit_reanalysis_reduction());
                false
            }
        }
    }
}
