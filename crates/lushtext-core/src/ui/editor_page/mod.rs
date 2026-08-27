// SPDX-License-Identifier: GPL-3.0-or-later

//! Editor page widget — one tab's content: GtkSourceView, minimap, and search bar.
//!
//! The public wrapper type and its small facade stay here, while file I/O,
//! search-bar choreography, and external file monitoring live in dedicated
//! sibling modules to keep this driving adapter easier to navigate.

mod accessibility;
mod bookmarks;
mod buffer_replacement;
mod document_identity;
mod focus_mode;
// Private implementation module. GTK's GObject bindings split custom widgets
// into an `imp` struct for instance data/lifecycle hooks and this public wrapper
// module for the type-safe API.
mod imp;
mod invisibles;
pub mod load;
pub(crate) mod local_history;
mod minimap;
mod monitor;
mod overscroll;
mod restore_position;
pub mod save;
mod search;
mod style_scheme;

use crate::model::bookmark::BookmarkRecord;
use crate::model::encoding::{
    DocumentEncoding, DocumentEncodingState, FileHealthFinding, InvisibleCharactersMode, LineEnding,
};
use crate::model::formatting_overrides::FormattingOverrides;
use crate::services::notifications::InlineActionNotification;
use crate::ui::info_bar::LushtextInfoBar;
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

pub use crate::services::editor_io::EditorSaveError;
#[cfg(feature = "test-utils")]
pub use crate::ui::buffer_snapshot::{
    BufferSnapshotCancelReason, BufferSnapshotCountersForTest, BufferSnapshotHandle,
    BufferSnapshotMetrics, BufferSnapshotOutcome, BufferSnapshotStateForTest,
    BufferSnapshotTestEdit, BufferSnapshotTestMutation, BufferSnapshotTestTrigger,
    buffer_snapshot_counters_for_test, coalesce_snapshot_payload_for_test,
    snapshot_buffer_text_async_for_test, snapshot_payload_metrics_for_test,
};
pub use bookmarks::{
    BookmarkEditError, BookmarkEditOutcome, BookmarkNavigationDirection, BookmarkToggleState,
};
pub use buffer_replacement::{
    BufferReplacementCancelReason, BufferReplacementMetrics, BufferReplacementTerminalDiagnostic,
    BufferReplacementTicket, BufferReplacementWorkflow, ReplacementPhase,
};
#[cfg(feature = "test-utils")]
pub use buffer_replacement::{BufferReplacementEvidence, BufferReplacementTestOutcome};
pub(crate) use buffer_replacement::{BufferReplacementOutcome, BufferReplacementRequest};
pub(crate) use focus_mode::{approximate_char_width, readable_column_margin};
pub use imp::{EditorLoadState, PendingWarningAction};
pub use load::{LoadEvidence, LoadInstallPhase, LoadOutcome};
#[cfg(feature = "test-utils")]
pub use load::{
    set_next_load_body_disposal_probe_for_test, set_next_load_disposal_reservation_weight_for_test,
};
#[cfg(feature = "test-utils")]
pub use minimap::MinimapAnalysisSnapshot;
pub(crate) use minimap::{
    MinimapAdjustmentDiagnostics, MinimapNativeSliderDiagnostics, MinimapTextViewRect,
};
pub use minimap::{
    MinimapAvailability, MinimapMarkerBounds, MinimapMarkerKind, MinimapProjectedBounds,
};

glib::wrapper! {
    // Generate the public GObject wrapper for `LushtextEditorPage`.
    // `ObjectSubclass` links to the private implementation, `@extends` declares
    // the GTK class chain, and `@implements` lists the GTK interfaces.
    /// Public GTK widget for one editor tab.
    ///
    /// The wrapper exposes the type-safe facade around the private GObject
    /// implementation so window workflows can reach editor, search, minimap,
    /// file, draft, and bookmark state without depending on template internals.
    pub struct LushtextEditorPage(ObjectSubclass<imp::LushtextEditorPage>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextEditorPage {
    #[must_use]
    pub fn new() -> Self {
        Object::builder().build()
    }

    #[must_use]
    ///
    /// # Panics
    ///
    /// Panics if the template wiring ever swaps the editor buffer away from
    /// the expected `sourceview5::Buffer` type.
    pub fn buffer(&self) -> sourceview5::Buffer {
        // GtkSourceView exposes the generic GtkTextBuffer API. This checked
        // GObject cast confirms source-specific behavior is available.
        self.source_view()
            .buffer()
            .downcast::<sourceview5::Buffer>()
            .expect("source view buffer is always a sourceview5::Buffer")
    }

    #[must_use]
    pub fn source_view(&self) -> &sourceview5::View {
        self.imp().source_view.as_ref()
    }

    /// Return the style-scheme ID currently applied to this editor buffer.
    ///
    /// Widget tests use this to verify that transparency swaps the buffer onto
    /// the derived opacity-aware scheme instead of fading the whole widget.
    #[must_use]
    pub fn applied_style_scheme_id(&self) -> Option<String> {
        self.imp().applied_style_scheme_id.borrow().clone()
    }

    /// Return the current document-surface opacity used by this editor tab.
    ///
    /// The minimap intentionally stays opaque, so this value only describes
    /// the main editor surface.
    #[must_use]
    pub fn content_background_opacity(&self) -> f64 {
        self.imp().document_surface_opacity.get()
    }

    /// Return the opacity used for the minimap surface.
    ///
    /// The minimap remains on an explicitly opaque path even when the editor
    /// document surface becomes translucent.
    #[must_use]
    pub fn minimap_background_opacity(&self) -> f64 {
        1.0
    }

    #[must_use]
    pub fn info_bar(&self) -> &LushtextInfoBar {
        self.imp().info_bar.as_ref()
    }

    #[must_use]
    pub fn file_path(&self) -> Option<std::path::PathBuf> {
        self.imp().file_path.borrow().clone()
    }

    /// Background-resolved canonical path for duplicate-tab reconciliation.
    #[must_use]
    pub(crate) fn canonical_file_path(&self) -> Option<std::path::PathBuf> {
        self.imp().canonical_file_path.borrow().clone()
    }

    /// On-disk size in bytes, populated after async load completes.
    #[must_use]
    pub fn file_size(&self) -> Option<u64> {
        self.imp().file_size.get()
    }

    /// Current file-load lifecycle state for this tab.
    #[must_use]
    pub fn load_state(&self) -> EditorLoadState {
        self.imp().load_state.get()
    }

    /// Whether the newest accepted load attempt failed for this visible buffer.
    #[must_use]
    pub(crate) fn latest_load_failed(&self) -> bool {
        self.imp().latest_load_failed.get()
    }

    /// Current encoding and line-ending facts for this tab.
    #[must_use]
    pub fn document_encoding_state(&self) -> DocumentEncodingState {
        self.imp().document_metadata.encoding_state.get()
    }

    /// Replace the current encoding and line-ending facts for this tab.
    pub fn set_document_encoding_state(&self, state: DocumentEncodingState) {
        self.imp().document_metadata.encoding_state.set(state);
    }

    /// Current "opened as" encoding for the active buffer content.
    #[must_use]
    pub fn opened_encoding(&self) -> DocumentEncoding {
        self.document_encoding_state().opened_encoding
    }

    /// Current save encoding policy for the next write.
    #[must_use]
    pub fn save_encoding(&self) -> DocumentEncoding {
        self.document_encoding_state().save_encoding
    }

    /// Update the save encoding policy while keeping the current open facts.
    pub fn set_save_encoding(&self, save_encoding: DocumentEncoding) {
        let mut state = self.document_encoding_state();
        state.save_encoding = save_encoding;
        self.set_document_encoding_state(state);
    }

    /// Advance the per-editor lossy-encoding request generation.
    pub(crate) fn advance_lossy_analysis_generation(&self) -> u32 {
        if let Some(snapshot) = self.imp().document_metadata.lossy_analysis_snapshot.take() {
            snapshot.dispose();
        }
        let generation = self
            .imp()
            .document_metadata
            .lossy_analysis_generation
            .get()
            .wrapping_add(1);
        self.imp()
            .document_metadata
            .lossy_analysis_generation
            .set(generation);
        generation
    }

    /// Current generation for lossy-encoding analysis requests.
    #[must_use]
    pub(crate) fn lossy_analysis_generation(&self) -> u32 {
        self.imp().document_metadata.lossy_analysis_generation.get()
    }

    /// Current line-ending state detected during the last load.
    #[must_use]
    pub fn detected_line_ending(&self) -> LineEnding {
        self.document_encoding_state().detected_line_ending
    }

    /// Current line-ending style selected for the next save.
    #[must_use]
    pub fn save_line_ending(&self) -> LineEnding {
        self.document_encoding_state().save_line_ending
    }

    /// Update the next-save line-ending policy while keeping other metadata.
    pub fn set_save_line_ending(&self, save_line_ending: LineEnding) {
        let mut state = self.document_encoding_state();
        state.save_line_ending = save_line_ending;
        self.set_document_encoding_state(state);
    }

    /// Whether the loaded on-disk representation carried a byte-order mark.
    #[must_use]
    pub fn has_bom(&self) -> bool {
        self.imp().document_metadata.has_bom.get()
    }

    /// Record whether the active on-disk representation carries a byte-order mark.
    pub fn set_has_bom(&self, has_bom: bool) {
        self.imp().document_metadata.has_bom.set(has_bom);
    }

    /// Current encoding-adjacent file-health findings for this tab.
    #[must_use]
    pub fn file_health(&self) -> Vec<FileHealthFinding> {
        self.imp().document_metadata.file_health.borrow().clone()
    }

    /// Replace the current file-health findings for this tab.
    pub fn set_file_health(&self, findings: Vec<FileHealthFinding>) {
        *self.imp().document_metadata.file_health.borrow_mut() = findings;
    }

    /// Current invisible-character visibility mode for this tab.
    #[must_use]
    pub fn invisible_characters_mode(&self) -> InvisibleCharactersMode {
        self.imp().document_metadata.invisible_mode.get()
    }

    /// Update the per-tab invisible-character visibility mode.
    pub fn set_invisible_characters_mode(&self, mode: InvisibleCharactersMode) {
        self.imp().document_metadata.invisible_mode.set(mode);
    }

    /// Allow exactly one lossy save attempt to proceed for this tab.
    pub fn arm_lossy_save_once(&self) {
        self.imp().document_metadata.allow_lossy_save_once.set(true);
    }

    /// Consume the one-shot lossy-save permission for this tab.
    #[must_use]
    pub fn take_lossy_save_once(&self) -> bool {
        self.imp()
            .document_metadata
            .allow_lossy_save_once
            .replace(false)
    }

    /// Route the shared warning-bar primary action to one editor-specific workflow.
    pub fn set_pending_warning_action(&self, action: Option<imp::PendingWarningAction>) {
        self.imp().document_metadata.warning_action.set(action);
    }

    /// Consume the current editor-specific warning-bar action, if any.
    #[must_use]
    pub fn take_pending_warning_action(&self) -> Option<imp::PendingWarningAction> {
        self.imp().document_metadata.warning_action.replace(None)
    }

    #[must_use]
    pub fn title(&self) -> String {
        self.imp()
            .file_path
            .borrow()
            .as_ref()
            .and_then(|path| path.file_name())
            .map_or_else(
                || "Untitled".to_string(),
                |name| name.to_string_lossy().into_owned(),
            )
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.buffer().is_modified()
    }

    /// Evict buffer content to free memory. The tab reloads from disk when re-focused.
    pub fn evict(&self) {
        let generation = self.imp().residency.policy_generation.get();
        let editor_weak = self.downgrade();
        self.replace_buffer_bounded(BufferReplacementRequest::new(
            BufferReplacementTicket {
                workflow: BufferReplacementWorkflow::MemoryEviction,
                generation,
            },
            String::new(),
            move |editor| {
                editor.imp().residency.policy_generation.get() == generation
                    && !editor.is_evicted()
                    && !editor.is_saving()
                    && editor.load_state() == EditorLoadState::Loaded
                    && !editor.imp().latest_load_failed.get()
            },
            move |outcome| {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                if !matches!(
                    outcome,
                    BufferReplacementOutcome::Complete {
                        ticket: BufferReplacementTicket {
                            workflow: BufferReplacementWorkflow::MemoryEviction,
                            generation: current_generation,
                        },
                        ..
                    } if current_generation == generation
                ) {
                    return;
                }
                editor.imp().residency.evicted.set(true);
                editor.buffer().set_modified(false);
                editor.release_local_history_residency_for_eviction();
                editor.clear_modified_line_marks();
                editor.refresh_minimap();
                editor.notify_memory_policy_changed();
                editor.refresh_accessibility_metadata();
            },
        ));
    }

    #[must_use]
    pub fn is_evicted(&self) -> bool {
        self.imp().residency.evicted.get()
    }

    /// Whether current content can be dropped and reconstructed from disk.
    #[must_use]
    pub(crate) fn eligible_for_memory_eviction(&self, active: bool) -> bool {
        !active
            && !self.is_evicted()
            && !self.is_modified()
            && !self.is_saving()
            && self.load_state() == EditorLoadState::Loaded
            && !self.latest_load_failed()
            && self.file_path().is_some()
    }

    #[must_use]
    /// Return a conservative O(1) estimate of this editor's live buffer residency.
    ///
    /// This GTK-main-thread query reads only scalar buffer metadata, never
    /// copies document text, and uses accepted file bytes as a floor. Evicted
    /// pages report only their fixed bookkeeping estimate.
    pub fn estimated_live_buffer_bytes(&self) -> u64 {
        if self.is_evicted() {
            return crate::model::editor_memory::EVICTED_EDITOR_BOOKKEEPING_BYTES;
        }
        #[cfg(feature = "test-utils")]
        if let Some(bytes) = self.imp().residency.estimate_override.get() {
            return bytes;
        }

        // GtkTextBuffer maintains char_count as scalar metadata. Reading it is
        // O(1), so accounting never copies or scans document text on edits.
        let character_count = u64::try_from(self.buffer().char_count()).unwrap_or(0);
        crate::model::editor_memory::estimate_live_editor_bytes(
            character_count,
            self.file_size(),
            self.is_evicted(),
        )
    }

    /// Current least-recently-used generation assigned by the owning window.
    #[must_use]
    pub(crate) fn memory_access_generation(&self) -> u64 {
        self.imp().residency.access_generation.get()
    }

    /// Current residency and eviction-eligibility generation.
    #[must_use]
    pub(crate) fn memory_policy_generation(&self) -> u64 {
        self.imp().residency.policy_generation.get()
    }

    /// Assign a window-wide access generation and invalidate stale decisions.
    pub(crate) fn mark_memory_accessed(&self, generation: u64) {
        self.imp().residency.access_generation.set(generation);
        self.notify_memory_policy_changed();
    }

    /// Install the page-to-window callback for residency or safety changes.
    ///
    /// It runs on the GTK main thread after edits, modified-state changes,
    /// load/save transitions, eviction, or access updates. A new callback
    /// replaces the previous window observer.
    pub fn connect_memory_policy_changed<F: Fn() + 'static>(&self, f: F) {
        *self.imp().memory_changed_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_inline_notification<F: Fn(InlineActionNotification) + 'static>(&self, f: F) {
        *self.imp().notification_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Advance policy freshness and notify the window without copying text.
    pub(crate) fn notify_memory_policy_changed(&self) {
        self.imp()
            .residency
            .policy_generation
            .set(self.imp().residency.policy_generation.get().wrapping_add(1));
        if let Some(ref callback) = *self.imp().memory_changed_callback.borrow() {
            callback();
        }
    }

    /// Override scalar residency for deterministic window-policy tests.
    #[cfg(feature = "test-utils")]
    pub fn set_memory_estimate_for_test(&self, bytes: Option<u64>) {
        self.imp().residency.estimate_override.set(bytes);
        self.notify_memory_policy_changed();
    }

    pub fn emit_inline_notification(&self, notification: InlineActionNotification) {
        self.set_pending_warning_action(None);
        if let Some(ref callback) = *self.imp().notification_callback.borrow() {
            callback(notification);
        } else {
            self.info_bar().render_notification(Some(&notification));
        }
    }

    /// Emit one inline notification while keeping a specific warning action wired.
    pub fn emit_inline_notification_with_warning_action(
        &self,
        notification: InlineActionNotification,
        action: imp::PendingWarningAction,
    ) {
        self.set_pending_warning_action(Some(action));
        if let Some(ref callback) = *self.imp().notification_callback.borrow() {
            callback(notification);
        } else {
            self.info_bar().render_notification(Some(&notification));
        }
    }

    pub fn clear_inline_notification(&self) {
        self.info_bar().render_notification(None);
    }

    #[must_use]
    pub fn notification_owner_id(&self) -> usize {
        self.as_ptr() as usize
    }

    #[must_use]
    pub fn draft_dirty(&self) -> bool {
        self.imp().draft.draft_dirty.get()
    }

    pub fn set_draft_dirty(&self, dirty: bool) {
        if dirty {
            self.imp()
                .draft
                .dirty_generation
                .set(self.imp().draft.dirty_generation.get().wrapping_add(1));
        }
        self.imp().draft.draft_dirty.set(dirty);
    }

    #[must_use]
    pub(crate) fn draft_dirty_generation(&self) -> u64 {
        self.imp().draft.dirty_generation.get()
    }

    #[must_use]
    pub fn draft_id(&self) -> Option<String> {
        self.imp().draft.draft_id.borrow().clone()
    }

    pub fn set_draft_id(&self, id: String) {
        *self.imp().draft.draft_id.borrow_mut() = Some(id);
    }

    #[must_use]
    pub fn is_draft_restored(&self) -> bool {
        self.imp().draft.draft_restored.get()
    }

    pub fn set_draft_restored(&self, restored: bool) {
        self.imp().draft.draft_restored.set(restored);
    }

    /// Whether automatic crash recovery is currently behind this dirty buffer.
    #[must_use]
    pub(crate) fn automatic_recovery_limited(&self) -> bool {
        self.imp().draft.automatic_recovery_limited.get()
    }

    /// Track the document-scoped automatic-recovery limit warning state.
    pub(crate) fn set_automatic_recovery_limited(&self, limited: bool) {
        self.imp().draft.automatic_recovery_limited.set(limited);
    }

    /// Current file-load generation used to reject stale lazy draft completions.
    #[must_use]
    pub(crate) fn load_generation(&self) -> u64 {
        self.imp().load_tracking.generation.get()
    }

    /// Apply EditorConfig formatting overrides and update the view.
    pub fn apply_editorconfig_overrides(&self, overrides: FormattingOverrides) {
        if let Some(line_ending) = overrides.line_ending {
            self.set_save_line_ending(line_ending);
        }
        if let Some(save_encoding) = overrides.save_encoding {
            self.set_save_encoding(save_encoding);
        }
        self.imp().formatting_overrides.set(overrides);
        imp::apply_formatting_settings(&self.imp().source_view, &self.imp().settings, overrides);
    }

    /// Clear all EditorConfig overrides and fall back to GSettings values.
    pub fn clear_editorconfig_overrides(&self) {
        self.apply_editorconfig_overrides(FormattingOverrides::default());
    }

    /// Current formatting overrides (for status-bar indicator).
    #[must_use]
    pub fn formatting_overrides(&self) -> FormattingOverrides {
        self.imp().formatting_overrides.get()
    }

    /// Register a callback fired when bookmark state changes and should be persisted.
    pub fn connect_bookmarks_changed<F: Fn() + 'static>(&self, f: F) {
        *self.imp().bookmarks.changed_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Register a callback fired when the user activates a bookmark gutter mark.
    pub fn connect_bookmark_activated<F: Fn(BookmarkRecord) + 'static>(&self, f: F) {
        bookmarks::connect_bookmark_activated(self, f);
    }

    /// Snapshot the current live bookmark projection into pure model records.
    #[must_use]
    pub fn bookmark_records(&self) -> Vec<BookmarkRecord> {
        bookmarks::bookmark_records(self)
    }

    /// Snapshot live bookmarks without exceeding a retained request-byte budget.
    #[must_use]
    pub(crate) fn bookmark_records_bounded_by_retained_bytes(
        &self,
        max_records: usize,
        max_retained_bytes: u64,
    ) -> (Vec<BookmarkRecord>, u64, bool) {
        bookmarks::bookmark_records_bounded_by_retained_bytes(self, max_records, max_retained_bytes)
    }

    /// Snapshot the live bookmark projection generation for async race guards.
    #[must_use]
    pub fn bookmark_change_generation(&self) -> u64 {
        bookmarks::bookmark_change_generation(self)
    }

    /// Replace the live bookmark projection with freshly loaded sidecar records.
    pub fn load_bookmarks(&self, bookmarks: &[BookmarkRecord]) {
        bookmarks::load_bookmarks(self, bookmarks);
    }

    /// Apply loaded sidecar bookmarks only when the live projection stayed unchanged.
    #[must_use]
    pub fn load_bookmarks_if_generation_matches(
        &self,
        bookmarks: &[BookmarkRecord],
        expected_generation: u64,
    ) -> bool {
        bookmarks::load_bookmarks_if_generation_matches(self, bookmarks, expected_generation)
    }

    /// Clear all live bookmark marks for the current file identity.
    pub fn clear_bookmarks(&self) {
        bookmarks::clear_bookmarks(self);
    }

    /// Toggle the bookmark on the active cursor line.
    #[must_use]
    pub fn toggle_bookmark_at_cursor(&self) -> BookmarkToggleState {
        bookmarks::toggle_bookmark_at_cursor(self)
    }

    /// Update the label for the bookmark on the active cursor line.
    #[must_use]
    pub fn set_bookmark_label_at_cursor(&self, label: Option<String>) -> Option<BookmarkRecord> {
        bookmarks::set_bookmark_label_at_cursor(self, label)
    }

    /// Update an existing bookmark by stable ID using a user-facing 1-based line.
    ///
    /// # Errors
    ///
    /// Returns a validation error if the bookmark ID no longer exists, the
    /// target line is outside the buffer, or another bookmark already owns the
    /// target line.
    pub fn update_bookmark(
        &self,
        id: &crate::model::bookmark::BookmarkId,
        label: Option<String>,
        target_line: u32,
    ) -> Result<BookmarkEditOutcome, BookmarkEditError> {
        bookmarks::update_bookmark(self, id, label, target_line)
    }

    /// Return the bookmark on the active cursor line, if one exists.
    #[must_use]
    pub fn current_bookmark(&self) -> Option<BookmarkRecord> {
        bookmarks::current_bookmark(self)
    }

    /// Return the bookmark whose live mark currently occupies a zero-based buffer line.
    #[must_use]
    pub fn bookmark_at_line(&self, line: u32) -> Option<BookmarkRecord> {
        bookmarks::bookmark_at_line(self, line)
    }

    /// Activate a bookmark by zero-based live buffer line and notify listeners.
    #[must_use]
    pub fn activate_bookmark_at_line(&self, line: u32) -> Option<BookmarkRecord> {
        bookmarks::activate_bookmark_at_line(self, line)
    }

    /// Jump to the next or previous bookmark in the active file, wrapping around.
    #[must_use]
    pub fn navigate_bookmark(
        &self,
        direction: BookmarkNavigationDirection,
    ) -> Option<BookmarkRecord> {
        bookmarks::navigate_bookmark(self, direction)
    }

    /// Notify persistence listeners that bookmark state changed.
    pub(crate) fn emit_bookmarks_changed(&self) {
        bookmarks::emit_bookmarks_changed(self);
    }

    /// Reconcile bookmark line numbers after the user edits the buffer.
    #[must_use]
    pub(crate) fn reconcile_bookmarks_after_edit(&self) -> bool {
        bookmarks::reconcile_bookmarks_after_edit(self)
    }

    /// Install bookmark gutter attributes on the source view.
    pub(crate) fn setup_bookmark_projection(&self) {
        bookmarks::setup_bookmark_projection(self);
    }
}

impl Default for LushtextEditorPage {
    fn default() -> Self {
        Self::new()
    }
}
