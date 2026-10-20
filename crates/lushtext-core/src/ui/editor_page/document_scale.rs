// SPDX-License-Identifier: GPL-3.0-or-later

//! The document scale policy as one editor tab applies it.
//!
//! **Cross-cutting coordination, no role** (`WFR-DOCUMENT-SCALE`). It is not a
//! workflow with its own stage order: the document-load publish stage, the
//! save accept terminal and the buffer-replacement terminals call it, and the
//! buffer's own `insert-text` and `delete-range` signals drive it.
//!
//! - **The session scale.** [`LushtextEditorPage::document_scale`] is the
//!   effective policy: a load starts a document session from the worker
//!   profile; a save and every installed body only **join** into it
//!   (`model::document_scale::DocumentScale::join`), so a presentation
//!   feature never comes back on within the session, while the byte tiers
//!   follow the latest loaded or saved bytes.
//! - **The edit-time watch.** An `insert-text` and a `delete-range`
//!   after-handler gather the facts of each edit (line counts and the lines of
//!   the edit's iters; the buffer's lines depend on its edit history, ledger
//!   A42) and run the GTK-free core
//!   `model::paragraph_profile_watch`: it names the lines to read (at most
//!   `2 · R_BATCH`, read through `chars_in_line`, which walks segments, not
//!   characters, ledger A43), and this module applies whatever downgrade the
//!   new profile implies **before the handler returns** — before GTK's
//!   validation and highlight idles can lay out the grown paragraph (ledger
//!   A44; A29 is the context-engine stall it avoids). Clearing the language
//!   removes tags and changes no text (A27). The handlers stand down while a
//!   load installation or a buffer replacement owns the buffer, because those
//!   installers deliver their profile from a worker.
//! - **The gates.** Syntax highlighting (the language is cleared, not only
//!   hidden), bracket matching, invisible characters, automatic search
//!   highlighting and the minimap are applied from the scale at load
//!   publication and at every downgrade.
//! - **The notice.** One reduced-features inline alert per session, below
//!   every recovery and error alert: it waits while another alert is shown and
//!   is shown again only for an additionally reduced feature.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_signals::SignalBag;
use gtk4::glib;
use gtk4::prelude::*;
use sourceview5::prelude::*;
use std::cell::Cell;

use crate::model::document_scale::{DocumentScale, ReducedFeatures};
use crate::model::paragraph_boundary::{
    DocumentProfile, ParagraphDelimiter, ProfileConfig, ProfileSummary, paragraph_chars_from_line,
};
use crate::model::paragraph_profile_watch::{
    DeleteFacts, InsertFacts, ProfileWatchState, WatchPlan, complete, plan_delete, plan_insert,
};
use crate::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use crate::ui::accessibility as a11y;

use super::LushtextEditorPage;

/// Title of the reduced-features notice.
pub const REDUCED_FEATURES_NOTICE_TITLE: &str = "Some Features Are Off for This Document";

/// Most characters of the notice body (and so of its announcement).
const REDUCED_FEATURES_NOTICE_BODY_LIMIT: usize = 320;

/// Where the reduced-features notice is.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum NoticePhase {
    /// Nothing to show.
    #[default]
    Idle,
    /// Waiting for the info bar to be free.
    Pending,
    /// Shown on the info bar.
    Showing,
}

/// Per-tab document scale state.
#[derive(Default)]
pub struct DocumentScaleState {
    /// The effective scale: the session meet of presentation features, the
    /// byte tiers of the latest loaded or saved bytes.
    scale: Cell<DocumentScale>,
    /// The live edit-time profile (session high-waters plus the carried
    /// density bound).
    watch: Cell<ProfileWatchState>,
    /// Bytes of the latest load or save, for the byte tiers.
    total_bytes: Cell<u64>,
    /// Line count recorded by the before-handlers of one edit.
    lines_before_edit: Cell<u64>,
    /// The presentation reductions the notice has already announced.
    announced: Cell<ReducedFeatures>,
    notice: Cell<NoticePhase>,
    /// Test evidence: cumulative watch line reads and steps.
    reads: Cell<u64>,
    fast_steps: Cell<u64>,
    window_steps: Cell<u64>,
    /// Buffer and info-bar handlers, disconnected on dispose.
    pub(crate) signals: SignalBag,
}

/// Observable document-scale state of one editor, for tests and automation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DocumentScaleEvidence {
    /// The effective scale.
    pub scale: DocumentScale,
    /// The watched session high-water of the longest paragraph.
    pub longest_paragraph_chars: u64,
    /// The watched session high-water of the worst density window.
    pub worst_batch_chars: u64,
    /// Buffer line-length reads the watch has made.
    pub watch_line_reads: u64,
    /// Watch steps that took the fast path.
    pub watch_fast_steps: u64,
    /// Watch steps that recomputed density windows.
    pub watch_window_steps: u64,
    /// Whether the reduced-features notice is waiting for the info bar.
    pub notice_pending: bool,
    /// Whether the reduced-features notice is on the info bar.
    pub notice_showing: bool,
}

/// The delimiter that ends GTK line `line`, `None` on the last line.
fn line_delimiter(buffer: &sourceview5::Buffer, line: i32) -> Option<ParagraphDelimiter> {
    let mut next = buffer.iter_at_line(line.checked_add(1)?)?;
    if next.line() != line + 1 || !next.backward_char() {
        return None;
    }
    match next.char() {
        '\n' => {
            let mut before = next;
            if before.backward_char() && before.char() == '\r' {
                Some(ParagraphDelimiter::CrLf)
            } else {
                Some(ParagraphDelimiter::Lf)
            }
        }
        '\r' => Some(ParagraphDelimiter::Cr),
        '\u{2029}' => Some(ParagraphDelimiter::ParagraphSeparator),
        _ => None,
    }
}

/// The paragraph length of GTK line `line`, from `chars_in_line` (ledger
/// A43: segment walks, not character walks).
fn read_paragraph_chars(buffer: &sourceview5::Buffer, line: u64) -> u64 {
    let Ok(line) = i32::try_from(line) else {
        return 0;
    };
    let Some(iter) = buffer.iter_at_line(line) else {
        return 0;
    };
    let chars = u64::try_from(iter.chars_in_line()).unwrap_or(0);
    paragraph_chars_from_line(chars, line_delimiter(buffer, line))
}

fn line_count(buffer: &sourceview5::Buffer) -> u64 {
    u64::try_from(buffer.line_count()).unwrap_or(0)
}

fn line_of(iter: &gtk4::TextIter) -> u64 {
    u64::try_from(iter.line()).unwrap_or(0)
}

impl LushtextEditorPage {
    /// The effective document scale of this tab.
    #[must_use]
    pub fn document_scale(&self) -> DocumentScale {
        self.imp().document_scale.scale.get()
    }

    /// Observable document-scale state.
    #[must_use]
    pub fn document_scale_evidence(&self) -> DocumentScaleEvidence {
        let state = &self.imp().document_scale;
        let watch = state.watch.get();
        DocumentScaleEvidence {
            scale: state.scale.get(),
            longest_paragraph_chars: watch.longest_paragraph_chars(),
            worst_batch_chars: watch.worst_batch_chars(),
            watch_line_reads: state.reads.get(),
            watch_fast_steps: state.fast_steps.get(),
            watch_window_steps: state.window_steps.get(),
            notice_pending: state.notice.get() == NoticePhase::Pending,
            notice_showing: state.notice.get() == NoticePhase::Showing,
        }
    }

    /// The scale an incoming body would leave this tab with, without
    /// applying it: its presentation features joined into the session, the
    /// current byte tiers kept.
    pub(crate) fn scale_for_incoming_profile(&self, profile: &DocumentProfile) -> DocumentScale {
        DocumentScale::for_profile(profile.summary()).join(self.document_scale())
    }

    /// Start a new document session from a load's worker profile.
    ///
    /// Open and external reload re-profile the document from scratch. An
    /// eviction reload keeps the session floor (design Open Question 3's
    /// default): it reloads byte-identical clean content, and features a
    /// session lost do not come back because memory pressure evicted the tab.
    pub(crate) fn begin_document_scale_session(
        &self,
        loaded: DocumentScale,
        summary: ProfileSummary,
        keep_floor: bool,
    ) {
        let state = &self.imp().document_scale;
        let previous = state.scale.get();
        let (scale, watch) = if keep_floor {
            (previous.join(loaded), state.watch.get().absorb(summary))
        } else {
            state.announced.set(ReducedFeatures::NONE);
            if state.notice.get() == NoticePhase::Pending {
                state.notice.set(NoticePhase::Idle);
            }
            (loaded, ProfileWatchState::from_profile(summary))
        };
        state.total_bytes.set(summary.total_bytes);
        state.watch.set(watch);
        state.scale.set(scale);
        self.apply_document_scale_gates();
        self.request_reduced_features_notice();
    }

    /// Join a saved document's byte size: byte tiers follow it, presentation
    /// features never come back on.
    pub(crate) fn join_saved_document_scale(&self, saved_bytes: u64) {
        let state = &self.imp().document_scale;
        state.total_bytes.set(saved_bytes);
        let previous = state.scale.get();
        self.adopt_document_scale(
            previous,
            previous.join(DocumentScale::for_total_bytes(saved_bytes)),
        );
    }

    /// Join a whole-buffer replacement's worker profile into the session.
    pub(crate) fn join_installed_profile(&self, profile: &DocumentProfile) {
        let state = &self.imp().document_scale;
        state.watch.set(state.watch.get().absorb(profile.summary()));
        let previous = state.scale.get();
        self.adopt_document_scale(previous, self.scale_for_incoming_profile(profile));
    }

    /// Store `next` and apply whatever it turned off since `previous`.
    fn adopt_document_scale(&self, previous: DocumentScale, next: DocumentScale) {
        self.imp().document_scale.scale.set(next);
        if next == previous {
            return;
        }
        if previous.syntax_enabled && !next.syntax_enabled {
            self.clear_language_for_scale();
        }
        if previous.bracket_matching_enabled && !next.bracket_matching_enabled {
            self.buffer().set_highlight_matching_brackets(false);
        }
        if previous.invisibles_allowed && !next.invisibles_allowed {
            self.apply_invisible_characters_mode();
        }
        if previous.search_highlight_allowed && !next.search_highlight_allowed {
            self.search_bar().set_highlight_allowed(false);
        }
        if previous.minimap_allowed && !next.minimap_allowed {
            self.retire_minimap_for_scale();
        }
        self.notify_memory_policy_changed();
        self.refresh_accessibility_metadata();
        if next
            .reduced
            .newly_reduced_presentation(previous.reduced)
            .any_presentation()
        {
            self.request_reduced_features_notice();
        }
    }

    /// Retire the minimap instance at once (a gated document never meets a
    /// `GtkSourceMap`), then let the debounced refresh publish the reduced
    /// availability and its feedback.
    fn retire_minimap_for_scale(&self) {
        self.suspend_minimap_projection();
        self.schedule_minimap_refresh();
    }

    /// Clear the language and its highlighting: removing a language removes
    /// its tags and changes no text (ledger A27).
    fn clear_language_for_scale(&self) {
        let buffer = self.buffer();
        buffer.set_highlight_syntax(false);
        if buffer.language().is_some() {
            buffer.set_language(None::<&sourceview5::Language>);
        }
    }

    /// Prepare the buffer before a body with `profile` enters it: a body the
    /// session may not highlight or map never meets the context engine or a
    /// `GtkSourceMap`.
    pub(crate) fn prepare_for_incoming_profile(&self, profile: &DocumentProfile) {
        let incoming = self.scale_for_incoming_profile(profile);
        if !incoming.syntax_enabled {
            self.clear_language_for_scale();
        }
        if !incoming.minimap_allowed {
            self.retire_minimap_for_scale();
        }
    }

    /// Apply every gated feature from the current scale (load publication).
    pub(crate) fn apply_document_scale_gates(&self) {
        let scale = self.document_scale();
        let buffer = self.buffer();
        if scale.syntax_enabled {
            self.reapply_language();
            buffer.set_highlight_syntax(true);
        } else {
            self.clear_language_for_scale();
        }
        buffer.set_highlight_matching_brackets(scale.bracket_matching_enabled);
        self.search_bar()
            .set_highlight_allowed(scale.search_highlight_allowed);
        self.apply_invisible_characters_mode();
        if !scale.minimap_allowed {
            self.retire_minimap_for_scale();
        }
        self.refresh_accessibility_metadata();
    }

    // ─── The edit-time watch ─────────────────────────────────────────────

    /// Whether the watch must stand down: an installer owns the buffer and
    /// delivers the profile from its worker.
    fn document_scale_watch_suspended(&self) -> bool {
        self.load_projection_suspended() || self.buffer_replacement_in_progress()
    }

    /// Install the buffer handlers of the edit-time watch (from `constructed`).
    pub(super) fn setup_document_scale_watch(&self) {
        let buffer = self.buffer();
        let signals = &self.imp().document_scale.signals;
        // Before-handlers record the line count only.
        let editor_weak = self.downgrade();
        let id = buffer.connect_insert_text(move |buffer, _, _| {
            if let Some(editor) = editor_weak.upgrade() {
                editor
                    .imp()
                    .document_scale
                    .lines_before_edit
                    .set(line_count(buffer));
            }
        });
        signals.track(&buffer, id);
        let editor_weak = self.downgrade();
        let id = buffer.connect_delete_range(move |buffer, _, _| {
            if let Some(editor) = editor_weak.upgrade() {
                editor
                    .imp()
                    .document_scale
                    .lines_before_edit
                    .set(line_count(buffer));
            }
        });
        signals.track(&buffer, id);

        let editor_weak = self.downgrade();
        let id = buffer.connect_local("insert-text", true, move |values| {
            let editor = editor_weak.upgrade()?;
            let location = values.get(1)?.get::<gtk4::TextIter>().ok()?;
            let text = values.get(2)?.get::<String>().ok()?;
            editor.watch_insertion(&location, &text);
            None
        });
        signals.track(&buffer, id);
        let editor_weak = self.downgrade();
        let id = buffer.connect_local("delete-range", true, move |values| {
            let editor = editor_weak.upgrade()?;
            let start = values.get(1)?.get::<gtk4::TextIter>().ok()?;
            editor.watch_deletion(&start);
            None
        });
        signals.track(&buffer, id);

        // The notice waits for the info bar to be free.
        let bar = self.info_bar().clone();
        let editor_weak = self.downgrade();
        let id = bar.connect_visible_notify(move |bar| {
            if !bar.is_visible()
                && let Some(editor) = editor_weak.upgrade()
            {
                editor.info_bar_hidden_for_notice();
            }
        });
        signals.track(&bar, id);
    }

    /// The `insert-text` after-handler: `end` is at the end of the run.
    fn watch_insertion(&self, end: &gtk4::TextIter, run: &str) {
        if run.is_empty() || self.document_scale_watch_suspended() {
            return;
        }
        let buffer = self.buffer();
        let run_chars = i32::try_from(run.chars().count()).unwrap_or(i32::MAX);
        let start = buffer.iter_at_offset(end.offset().saturating_sub(run_chars));
        let mut before = start;
        let between_cr_and_lf =
            before.backward_char() && before.char() == '\r' && end.char() == '\n';
        let facts = InsertFacts {
            run,
            line_count_before: self.imp().document_scale.lines_before_edit.get(),
            line_count_after: line_count(&buffer),
            start_line: line_of(&start),
            end_line: line_of(end),
            between_cr_and_lf,
        };
        let state = self.imp().document_scale.watch.get();
        let plan = plan_insert(state, ProfileConfig::PRODUCTION, &facts);
        self.complete_watch_step(&buffer, state, &plan);
    }

    /// The `delete-range` after-handler: `start` is the deletion point.
    fn watch_deletion(&self, start: &gtk4::TextIter) {
        if self.document_scale_watch_suspended() {
            return;
        }
        let buffer = self.buffer();
        let facts = DeleteFacts {
            line_count_before: self.imp().document_scale.lines_before_edit.get(),
            line_count_after: line_count(&buffer),
            line: line_of(start),
        };
        let plan = plan_delete(ProfileConfig::PRODUCTION, facts);
        let state = self.imp().document_scale.watch.get();
        self.complete_watch_step(&buffer, state, &plan);
    }

    /// Read exactly the lines `plan` names, complete the step, and apply any
    /// downgrade before the signal emission returns.
    fn complete_watch_step(
        &self,
        buffer: &sourceview5::Buffer,
        state: ProfileWatchState,
        plan: &WatchPlan<'_>,
    ) {
        if matches!(plan, WatchPlan::Nothing) {
            return;
        }
        let reads: Vec<u64> = plan
            .lines_to_read()
            .map(|line| read_paragraph_chars(buffer, line))
            .collect();
        let scale_state = &self.imp().document_scale;
        scale_state
            .reads
            .set(scale_state.reads.get().saturating_add(reads.len() as u64));
        match plan {
            WatchPlan::FastPath { .. } => scale_state
                .fast_steps
                .set(scale_state.fast_steps.get().saturating_add(1)),
            WatchPlan::ReadWindow(_) => scale_state
                .window_steps
                .set(scale_state.window_steps.get().saturating_add(1)),
            WatchPlan::Nothing => {}
        }
        let next = complete(state, ProfileConfig::PRODUCTION, plan, &reads);
        scale_state.watch.set(next);
        let previous = scale_state.scale.get();
        let candidate = DocumentScale::for_profile(next.summary(scale_state.total_bytes.get()));
        // Presentation features meet; the byte tiers stay those of the latest
        // load or save (an edit does not toggle undo, design Open Question 4).
        self.adopt_document_scale(previous, candidate.join(previous));
    }

    // ─── The reduced-features notice ─────────────────────────────────────

    /// Queue the notice when the scale reduces a presentation feature it has
    /// not announced yet.
    fn request_reduced_features_notice(&self) {
        let state = &self.imp().document_scale;
        let reduced = state.scale.get().reduced;
        if !reduced
            .newly_reduced_presentation(state.announced.get())
            .any_presentation()
        {
            return;
        }
        if state.notice.get() != NoticePhase::Showing {
            state.notice.set(NoticePhase::Pending);
        }
        // An idle, so recovery and error alerts the same turn publishes rank
        // first and the notice waits behind them.
        let editor_weak = self.downgrade();
        glib::idle_add_local_once(move || {
            if let Some(editor) = editor_weak.upgrade() {
                editor.flush_reduced_features_notice();
            }
        });
    }

    /// Another inline alert replaced the notice: show it again later.
    pub(super) fn note_inline_notification_replaced(&self) {
        let state = &self.imp().document_scale;
        if state.notice.get() == NoticePhase::Showing {
            state.notice.set(NoticePhase::Pending);
        }
    }

    fn info_bar_hidden_for_notice(&self) {
        let state = &self.imp().document_scale;
        match state.notice.get() {
            // Dismissed or cleared: dismissing never re-enables anything.
            NoticePhase::Showing => state.notice.set(NoticePhase::Idle),
            NoticePhase::Pending => {
                let editor_weak = self.downgrade();
                glib::idle_add_local_once(move || {
                    if let Some(editor) = editor_weak.upgrade() {
                        editor.flush_reduced_features_notice();
                    }
                });
            }
            NoticePhase::Idle => {}
        }
    }

    /// Show the pending notice if the info bar is free.
    fn flush_reduced_features_notice(&self) {
        let state = &self.imp().document_scale;
        if state.notice.get() != NoticePhase::Pending || self.info_bar().is_visible() {
            return;
        }
        let reduced = state.scale.get().reduced;
        let Some(description) = reduced.describe() else {
            state.notice.set(NoticePhase::Idle);
            return;
        };
        let body =
            a11y::bounded_announcement_text(&description, REDUCED_FEATURES_NOTICE_BODY_LIMIT)
                .into_owned();
        self.emit_inline_notification(InlineActionNotification {
            style: InlineNotificationStyle::Warning,
            title: REDUCED_FEATURES_NOTICE_TITLE.to_string(),
            body,
            primary_button: None,
            secondary_button: None,
        });
        state.announced.set(reduced.presentation_only());
        state.notice.set(NoticePhase::Showing);
    }
}
