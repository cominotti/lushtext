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
//! - **The notice.** The reduced features are reported outside the text
//!   view's allocation path: one announced status-bar message per newly
//!   reduced feature, published when the tab is the window's selected one,
//!   and a persistent row in the document properties. Never an inline alert:
//!   showing or hiding one above the editor reallocates the text view, and
//!   GTK then lays out every visible paragraph again, 300-760 ms for one of
//!   hundreds of thousands of characters (the 10.4 lane, 2026-10-30).

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_signals::SignalBag;
use gtk4::glib;
use gtk4::prelude::*;
use sourceview5::prelude::*;
use std::cell::Cell;

use crate::model::document_scale::{DocumentScale, ReducedFeatures, SyntaxLanguage};
use crate::model::paragraph_boundary::{
    DocumentProfile, ParagraphDelimiter, ProfileConfig, ProfileSummary, paragraph_chars_from_line,
};
use crate::model::paragraph_profile_watch::{
    DeleteFacts, InsertFacts, ProfileWatchState, WatchPlan, complete, plan_delete, plan_insert,
};
use crate::ui::accessibility as a11y;
use crate::ui::responsiveness_probe::PostOpenSpan;
use crate::ui::status_bar::MessageKind;
use crate::ui::window::LushtextWindow;

use super::LushtextEditorPage;
use super::language_attach::RequestedLanguage;

/// Most characters of the notice (and so of its announcement).
///
/// Kind: presentation cap; 320 rounded up to a power of two
/// (`power-of-two-thresholds` D12): the longest notice outside Protected View
/// is 229 characters (a unit test pins it), and the separate 256-character
/// announcement cap is unchanged.
pub const REDUCED_FEATURES_NOTICE_LIMIT: usize = 512;

/// Where the reduced-features notice is.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum NoticePhase {
    /// Nothing to announce.
    #[default]
    Idle,
    /// Waiting until the tab is the window's selected one.
    Pending,
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
    /// The scale and watch state from just before the edit that entered
    /// Protected View (`long-line-protected-view` design D2): what the one
    /// bounded entry undo restores. `None` unless the session entered by edit.
    before_protected_entry: Cell<Option<(DocumentScale, ProfileWatchState)>>,
    notice: Cell<NoticePhase>,
    /// Test evidence: cumulative watch line reads and steps.
    reads: Cell<u64>,
    fast_steps: Cell<u64>,
    window_steps: Cell<u64>,
    /// Paragraphs the per-paragraph wrap gate unwrapped and rewrapped
    /// (`paragraph_wrap.rs`), cumulative.
    unwraps: Cell<u64>,
    rewraps: Cell<u64>,
    /// Buffer and info-bar handlers, disconnected on dispose.
    pub(crate) signals: SignalBag,
}

impl DocumentScaleState {
    /// Count one paragraph the wrap gate unwrapped.
    pub(super) fn record_unwrap(&self) {
        self.unwraps.set(self.unwraps.get().saturating_add(1));
    }

    /// Count one paragraph the wrap gate let wrap again.
    pub(super) fn record_rewrap(&self) {
        self.rewraps.set(self.rewraps.get().saturating_add(1));
    }
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
    /// Whether the reduced-features notice waits for its tab to be selected.
    pub notice_pending: bool,
    /// The presentation reductions the notice has announced this session.
    pub notice_announced: ReducedFeatures,
    /// Paragraphs the per-paragraph wrap gate unwrapped, cumulative.
    pub paragraphs_unwrapped: u64,
    /// Paragraphs the gate let wrap again, cumulative.
    pub paragraphs_rewrapped: u64,
}

/// The start of a run an `insert-text` after-handler just saw inserted:
/// `end` is the iter the default handler revalidated to the run's end.
///
/// Shared by the edit-time watch and the minimap's modified-line marks, which
/// both need the run's first and last buffer lines (ledger A42).
pub(super) fn inserted_run_start(
    buffer: &sourceview5::Buffer,
    end: &gtk4::TextIter,
    run: &str,
) -> gtk4::TextIter {
    let run_chars = i32::try_from(run.chars().count()).unwrap_or(i32::MAX);
    buffer.iter_at_offset(end.offset().saturating_sub(run_chars))
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
pub(super) fn read_paragraph_chars(buffer: &sourceview5::Buffer, line: u64) -> u64 {
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

/// The before-handlers' one job: the line count before the edit.
fn record_lines_before_edit(
    editor_weak: &glib::WeakRef<LushtextEditorPage>,
    buffer: &sourceview5::Buffer,
) {
    if let Some(editor) = editor_weak.upgrade() {
        editor
            .imp()
            .document_scale
            .lines_before_edit
            .set(line_count(buffer));
    }
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
            notice_announced: state.announced.get(),
            paragraphs_unwrapped: state.unwraps.get(),
            paragraphs_rewrapped: state.rewraps.get(),
        }
    }

    /// Whether the document is dense: some validation batch's worth of
    /// paragraphs holds more than `V_SLICE` visible characters
    /// (`bound-post-open-settle`, D6 and D9). Read from the live watch.
    pub(crate) fn is_dense_document(&self) -> bool {
        let state = &self.imp().document_scale;
        crate::model::buffer_replacement::batch_exceeds_slice(
            state.watch.get().summary(state.total_bytes.get()),
        )
    }

    /// The language class the total-size syntax threshold depends on
    /// (`bound-post-open-settle`): the requested language when known, else
    /// the buffer's.
    pub(crate) fn syntax_language(&self) -> SyntaxLanguage {
        match self.requested_language() {
            RequestedLanguage::Id(id) => SyntaxLanguage::from_language_id(Some(&id)),
            RequestedLanguage::Plain => SyntaxLanguage::PlainText,
            RequestedLanguage::Unknown => SyntaxLanguage::from_language_id(
                self.buffer()
                    .language()
                    .map(|language| language.id())
                    .as_deref(),
            ),
        }
    }

    /// The scale an incoming body would leave this tab with, without
    /// applying it: its presentation features joined into the session, the
    /// current byte tiers kept.
    pub(crate) fn scale_for_incoming_profile(&self, profile: &DocumentProfile) -> DocumentScale {
        DocumentScale::for_profile_in_with(
            profile.summary(),
            self.syntax_language(),
            crate::model::document_scale::markdown_chunked_cap(),
        )
        .join(self.document_scale())
    }

    /// Start a new document session from a load's worker profile.
    ///
    /// Open and external reload re-profile the document from scratch. An
    /// eviction reload keeps the session floor (design Open Question 3's
    /// default): it reloads byte-identical clean content, and features a
    /// session lost do not come back because memory pressure evicted the tab.
    pub(crate) fn begin_document_scale_session(&self, summary: ProfileSummary, keep_floor: bool) {
        let loaded = DocumentScale::for_profile_in_with(
            summary,
            self.syntax_language(),
            crate::model::document_scale::markdown_chunked_cap(),
        );
        self.cancel_reanalysis_pacing();
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
    pub(super) fn adopt_document_scale(&self, previous: DocumentScale, next: DocumentScale) {
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
        // Announces only a newly reduced feature; refreshes the properties
        // row for any change.
        self.request_reduced_features_notice();
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
        self.cancel_language_attach();
        self.cancel_reanalysis_pacing();
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
            self.request_language_attach();
            let _span = PostOpenSpan::begin("post_open_highlight_attach");
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
            record_lines_before_edit(&editor_weak, buffer);
        });
        signals.track(&buffer, id);
        let editor_weak = self.downgrade();
        let id = buffer.connect_delete_range(move |buffer, _, _| {
            record_lines_before_edit(&editor_weak, buffer);
        });
        signals.track(&buffer, id);

        let editor_weak = self.downgrade();
        let id = buffer.connect_local("insert-text", true, move |values| {
            let editor = editor_weak.upgrade()?;
            let location = values.get(1)?.get::<gtk4::TextIter>().ok()?;
            let text = values.get(2)?.get::<&str>().ok()?;
            editor.watch_insertion(&location, text);
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
    }

    /// The `insert-text` after-handler: `end` is at the end of the run.
    fn watch_insertion(&self, end: &gtk4::TextIter, run: &str) {
        if run.is_empty() || self.document_scale_watch_suspended() {
            return;
        }
        let buffer = self.buffer();
        let start = inserted_run_start(&buffer, end, run);
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
        self.pace_reanalysis_after_edit(start.offset());
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
        self.pace_reanalysis_after_edit(start.offset());
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
        // The fast path (ordinary typing) reads one line and allocates nothing.
        let single;
        let window: Vec<u64>;
        let reads: &[u64] = if let WatchPlan::FastPath { touched_line, .. } = plan {
            single = [read_paragraph_chars(buffer, *touched_line)];
            &single
        } else {
            window = plan
                .lines_to_read()
                .map(|line| read_paragraph_chars(buffer, line))
                .collect();
            &window
        };
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
        let next = complete(state, ProfileConfig::PRODUCTION, plan, reads);
        scale_state.watch.set(next);
        let previous = scale_state.scale.get();
        let candidate = DocumentScale::for_profile_in_with(
            next.summary(scale_state.total_bytes.get()),
            self.syntax_language(),
            crate::model::document_scale::markdown_chunked_cap(),
        );
        // Presentation features meet; the byte tiers stay those of the latest
        // load or save (an edit does not toggle undo, design Open Question 4).
        self.adopt_document_scale(previous, candidate.join(previous));
        if !previous.protected_view && scale_state.scale.get().protected_view {
            // This edit made the document protected: enter Protected View in
            // this same emission, before any validation idle runs (design D2),
            // keeping what the entry undo restores.
            scale_state
                .before_protected_entry
                .set(Some((previous, state)));
            let lines: Vec<u64> = match plan {
                WatchPlan::FastPath { touched_line, .. } => vec![*touched_line],
                _ => plan.lines_to_read().collect(),
            };
            let grown: Vec<(u64, u64)> = lines.into_iter().zip(reads.iter().copied()).collect();
            self.enter_protected_view_from_edit(&grown);
        }
    }

    /// Restore the scale and watch from just before the edit that entered
    /// Protected View, and re-apply every gate from it: the entry undo
    /// (design D2) removes exactly the content that edit added, so this is
    /// the one exception to the monotone session (C1's requirement, MODIFIED
    /// by `long-line-protected-view`). Returns false when no edit entered.
    pub(crate) fn restore_document_scale_before_protected_entry(&self) -> bool {
        let state = &self.imp().document_scale;
        let Some((scale, watch)) = state.before_protected_entry.take() else {
            return false;
        };
        state.watch.set(watch);
        state.scale.set(scale);
        self.apply_document_scale_gates();
        if scale.minimap_allowed {
            self.schedule_minimap_refresh();
        }
        self.notify_memory_policy_changed();
        self.request_reduced_features_notice();
        true
    }

    // ─── The reduced-features notice ─────────────────────────────────────

    /// Queue the notice when the scale reduces a presentation feature it has
    /// not announced yet, and refresh the document properties either way.
    fn request_reduced_features_notice(&self) {
        let state = &self.imp().document_scale;
        if state
            .scale
            .get()
            .reduced
            .newly_reduced_presentation(state.announced.get())
            .any_presentation()
        {
            state.notice.set(NoticePhase::Pending);
        }
        // An idle, so the load or edit that changed the scale finishes first.
        let editor_weak = self.downgrade();
        glib::idle_add_local_once(move || {
            if let Some(editor) = editor_weak.upgrade() {
                editor.flush_reduced_features_notice();
            }
        });
    }

    /// Announce a pending notice and refresh the document properties, when
    /// this tab is its window's selected one (the window also calls this on
    /// every tab selection, so a background tab announces when selected).
    pub(crate) fn flush_reduced_features_notice(&self) {
        let Some(window) = self
            .root()
            .and_then(|root| root.downcast::<LushtextWindow>().ok())
        else {
            return;
        };
        if !window.is_active_editor(self) {
            return;
        }
        window.refresh_status_bar();
        let state = &self.imp().document_scale;
        if state.notice.get() != NoticePhase::Pending {
            return;
        }
        state.notice.set(NoticePhase::Idle);
        let reduced = state.scale.get().reduced;
        // The Protected View alert replaces this notice for a protected
        // document, so it never shows both (`protected-view-inspector-and-surfaces`).
        if self.is_protected_view() {
            state.announced.set(reduced.presentation_only());
            return;
        }
        let Some(description) = reduced.describe() else {
            return;
        };
        let message = a11y::bounded_announcement_text(&description, REDUCED_FEATURES_NOTICE_LIMIT);
        window.publish_status_message(&message, MessageKind::Warning);
        state.announced.set(reduced.presentation_only());
    }
}

#[cfg(test)]
mod notice_limit_tests {
    use super::REDUCED_FEATURES_NOTICE_LIMIT;
    use crate::ui::accessibility as a11y;

    /// No loss against `main` (`power-of-two-thresholds` D12): `main` passed a
    /// 320-character notice through whole; the limit rounds up to 512, and the
    /// longest notice the app ships stays whole.
    #[test]
    fn no_loss_the_reduced_features_notice_keeps_every_character_main_kept() {
        let notice = "n".repeat(320);
        assert_eq!(
            a11y::bounded_announcement_text(&notice, REDUCED_FEATURES_NOTICE_LIMIT),
            notice.as_str()
        );
        let longest = crate::model::document_scale::tests::longest_unprotected_notice();
        assert!(longest <= REDUCED_FEATURES_NOTICE_LIMIT, "{longest}");
        assert_eq!(REDUCED_FEATURES_NOTICE_LIMIT, 512);
    }
}
