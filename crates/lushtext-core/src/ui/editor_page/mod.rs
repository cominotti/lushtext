// SPDX-License-Identifier: GPL-3.0-or-later

//! Editor page widget — one tab's content: GtkSourceView + search bar.
//!
//! The public wrapper type and its small facade stay here, while file I/O,
//! search-bar choreography, and external file monitoring live in dedicated
//! sibling modules to keep this driving adapter easier to navigate.

mod annotations;
mod bookmarks;
mod imp;
mod load_save;
mod monitor;
mod search;

use crate::model::annotation::{AnnotationId, AnnotationRecord, AnnotationStyle};
use crate::model::bookmark::BookmarkRecord;
use crate::model::formatting_overrides::FormattingOverrides;
use crate::services::notifications::InlineActionNotification;
use crate::ui::info_bar::LushtextInfoBar;
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

pub use crate::services::editor_io::SaveError;
pub use annotations::AnnotationEditSelection;
pub use bookmarks::{BookmarkNavigationDirection, BookmarkToggleState};

glib::wrapper! {
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
    pub fn buffer(&self) -> sourceview5::Buffer {
        self.source_view()
            .buffer()
            .downcast::<sourceview5::Buffer>()
            .expect("source view buffer is always a sourceview5::Buffer")
    }

    #[must_use]
    pub fn source_view(&self) -> &sourceview5::View {
        self.imp().source_view.as_ref()
    }

    #[must_use]
    pub fn info_bar(&self) -> &LushtextInfoBar {
        self.imp().info_bar.as_ref()
    }

    #[must_use]
    pub fn file_path(&self) -> Option<std::path::PathBuf> {
        self.imp().file_path.borrow().clone()
    }

    /// On-disk size in bytes, populated after async load completes.
    #[must_use]
    pub fn file_size(&self) -> Option<u64> {
        self.imp().file_size.get()
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
        self.imp().evicted.set(true);
        let buffer = self.buffer();
        buffer.begin_irreversible_action();
        buffer.set_text("");
        buffer.end_irreversible_action();
        buffer.set_modified(false);
        self.notify_estimated_memory_changed();
    }

    #[must_use]
    pub fn is_evicted(&self) -> bool {
        self.imp().evicted.get()
    }

    #[must_use]
    pub fn estimated_buffer_bytes(&self) -> u64 {
        if self.is_evicted() {
            return 0;
        }

        self.file_size().map_or(0, |size| {
            size.saturating_mul(self.size_check().estimated_buffer_multiplier())
        })
    }

    pub fn connect_estimated_memory_changed<F: Fn(u64) + 'static>(&self, f: F) {
        *self.imp().memory_changed_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_inline_notification<F: Fn(InlineActionNotification) + 'static>(&self, f: F) {
        *self.imp().notification_callback.borrow_mut() = Some(Box::new(f));
    }

    fn notify_estimated_memory_changed(&self) {
        if let Some(ref callback) = *self.imp().memory_changed_callback.borrow() {
            callback(self.estimated_buffer_bytes());
        }
    }

    pub fn emit_inline_notification(&self, notification: InlineActionNotification) {
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
        self.imp().draft.draft_dirty.set(dirty);
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

    /// Apply EditorConfig formatting overrides and update the view.
    pub fn apply_editorconfig_overrides(&self, overrides: FormattingOverrides) {
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

    /// Register a callback fired after every successful file load or reload.
    pub fn connect_file_loaded<F: Fn() + 'static>(&self, f: F) {
        *self.imp().load.file_loaded_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Register a callback fired when bookmark state changes and should be persisted.
    pub fn connect_bookmarks_changed<F: Fn() + 'static>(&self, f: F) {
        *self.imp().bookmarks.changed_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Register a callback fired when annotation state changes and should be persisted.
    pub fn connect_annotations_changed<F: Fn() + 'static>(&self, f: F) {
        *self.imp().annotations.changed_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Snapshot the current live bookmark projection into pure model records.
    #[must_use]
    pub fn bookmark_records(&self) -> Vec<BookmarkRecord> {
        bookmarks::bookmark_records(self)
    }

    /// Replace the live bookmark projection with freshly loaded sidecar records.
    pub fn load_bookmarks(&self, bookmarks: &[BookmarkRecord]) {
        bookmarks::load_bookmarks(self, bookmarks);
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

    /// Return the bookmark on the active cursor line, if one exists.
    #[must_use]
    pub fn current_bookmark(&self) -> Option<BookmarkRecord> {
        bookmarks::current_bookmark(self)
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

    /// Snapshot the current live annotation projection into pure model records.
    #[must_use]
    pub fn annotation_records(&self) -> Vec<AnnotationRecord> {
        annotations::annotation_records(self)
    }

    /// Replace the live annotation projection with freshly loaded sidecar records.
    pub fn load_annotations(&self, annotations: &[AnnotationRecord]) {
        annotations::load_annotations(self, annotations);
    }

    /// Clear all live annotations for the current file identity.
    pub fn clear_annotations(&self) {
        annotations::clear_annotations(self);
    }

    /// Create a new annotation from the current selection (or current line).
    #[must_use]
    pub fn create_annotation_from_selection(
        &self,
        note_text: String,
        style: AnnotationStyle,
    ) -> AnnotationRecord {
        annotations::create_annotation_from_selection(self, note_text, style)
    }

    /// Update an existing annotation's note body and presentation style.
    #[must_use]
    pub fn update_annotation(
        &self,
        annotation_id: &AnnotationId,
        note_text: String,
        style: AnnotationStyle,
    ) -> Option<AnnotationRecord> {
        annotations::update_annotation(self, annotation_id, note_text, style)
    }

    /// Delete an existing annotation from the live editor state.
    #[must_use]
    pub fn delete_annotation(&self, annotation_id: &AnnotationId) -> bool {
        annotations::delete_annotation(self, annotation_id)
    }

    /// Return the annotation currently covering the cursor line, if one exists.
    #[must_use]
    pub fn current_annotation(&self) -> Option<AnnotationRecord> {
        annotations::current_annotation(self)
    }

    /// Find a specific annotation by ID in the current live projection.
    #[must_use]
    pub fn annotation_by_id(&self, annotation_id: &AnnotationId) -> Option<AnnotationRecord> {
        annotations::annotation_by_id(self, annotation_id)
    }

    /// Record an annotation that should reopen once the next file load finishes.
    pub fn set_pending_annotation_focus(&self, annotation_id: Option<AnnotationId>) {
        annotations::set_pending_annotation_focus(self, annotation_id);
    }

    /// Consume the pending annotation focus request after load completes.
    #[must_use]
    pub fn take_pending_annotation_focus(&self) -> Option<AnnotationId> {
        annotations::take_pending_annotation_focus(self)
    }

    /// Return the selected annotation-editing context for the current cursor state.
    #[must_use]
    pub fn annotation_edit_selection(&self) -> AnnotationEditSelection {
        annotations::annotation_edit_selection(self)
    }

    /// Notify persistence listeners that annotation state changed.
    pub(crate) fn emit_annotations_changed(&self) {
        annotations::emit_annotations_changed(self);
    }

    /// Reconcile annotation ranges after the user edits the buffer.
    #[must_use]
    pub(crate) fn reconcile_annotations_after_edit(&self) -> bool {
        annotations::reconcile_annotations_after_edit(self)
    }

    /// Refresh annotation highlight colors and visibility after theme or settings changes.
    pub(crate) fn refresh_annotation_highlights(&self) {
        annotations::refresh_annotation_highlights(self);
    }

    /// Toggle whether annotation highlights are applied to the current buffer.
    pub(crate) fn set_annotation_highlights_visible(&self, visible: bool) {
        annotations::set_annotation_highlights_visible(self, visible);
    }
}

impl Default for LushtextEditorPage {
    fn default() -> Self {
        Self::new()
    }
}
