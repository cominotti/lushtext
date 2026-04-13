// SPDX-License-Identifier: GPL-3.0-or-later

//! Editor page widget — one tab's content: GtkSourceView + search bar.
//!
//! The public wrapper type and its small facade stay here, while file I/O,
//! search-bar choreography, and external file monitoring live in dedicated
//! sibling modules to keep this driving adapter easier to navigate.

mod imp;
mod load_save;
mod monitor;
mod search;

use crate::model::formatting_overrides::FormattingOverrides;
use crate::services::notifications::InlineActionNotification;
use crate::ui::info_bar::LushtextInfoBar;
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

pub use crate::services::editor_io::SaveError;

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
}

impl Default for LushtextEditorPage {
    fn default() -> Self {
        Self::new()
    }
}
