// SPDX-License-Identifier: GPL-3.0-or-later

//! Editor page widget — one tab's content: GtkSourceView + search bar.

mod imp;

use crate::services::async_task;
use glib::subclass::prelude::ObjectSubclassIsExt;
use glib::Object;
use gtk4::prelude::*;
use sourceview5::prelude::*;
use std::path::Path;

glib::wrapper! {
    pub struct LushtextEditorPage(ObjectSubclass<imp::LushtextEditorPage>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextEditorPage {
    pub fn new() -> Self {
        Object::builder().build()
    }

    /// Start loading a file asynchronously. Sets the file path immediately
    /// so duplicate detection works before content arrives.
    pub fn load_file_async(&self, path: &Path) {
        let file_path = path.to_path_buf();
        self.imp().file_path.replace(Some(file_path.clone()));

        async_task::spawn_blocking_then(
            self.clone(),
            move || {
                std::fs::read_to_string(&file_path)
                    .map_err(|e| format!("Failed to read {}: {}", file_path.display(), e))
            },
            |editor, result| match result {
                Ok(content) => {
                    editor.imp().file_size.set(Some(content.len() as u64));
                    editor.apply_loaded_content(&content);
                }
                Err(e) => tracing::error!("{}", e),
            },
        );
    }

    fn apply_loaded_content(&self, content: &str) {
        let buffer = self.buffer();
        buffer.begin_irreversible_action();
        buffer.set_text(content);
        buffer.end_irreversible_action();
        buffer.set_modified(false);

        let start = buffer.start_iter();
        buffer.place_cursor(&start);

        self.reapply_language();
    }

    /// Set the file path (used by save-as). Updates syntax highlighting
    /// based on the new filename's extension.
    pub fn set_file_path(&self, path: &Path) {
        self.imp().file_path.replace(Some(path.to_path_buf()));
        self.reapply_language();
    }

    /// Detect and apply syntax language from the current file path.
    fn reapply_language(&self) {
        let buffer = self.buffer();
        if let Some(ref fp) = *self.imp().file_path.borrow() {
            let lang_manager = sourceview5::LanguageManager::default();
            if let Some(language) = lang_manager.guess_language(fp.to_str(), None) {
                buffer.set_language(Some(&language));
            }
        }
    }

    pub fn save_file(&self) -> anyhow::Result<()> {
        let path = self
            .imp()
            .file_path
            .borrow()
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No file path set"))?;

        let buffer = self.buffer();
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
        std::fs::write(&path, text.as_str())
            .map_err(|e| anyhow::anyhow!("Failed to write {}: {}", path.display(), e))?;
        self.imp().file_size.set(Some(text.len() as u64));
        buffer.set_modified(false);
        Ok(())
    }

    pub fn buffer(&self) -> sourceview5::Buffer {
        self.source_view()
            .buffer()
            .downcast::<sourceview5::Buffer>()
            .expect("source view buffer is always a sourceview5::Buffer")
    }

    pub fn source_view(&self) -> &sourceview5::View {
        self.imp().source_view.as_ref()
    }

    pub fn file_path(&self) -> Option<std::path::PathBuf> {
        self.imp().file_path.borrow().clone()
    }

    /// On-disk size in bytes, populated after async load completes.
    /// `None` for untitled tabs or before the load finishes.
    pub fn file_size(&self) -> Option<u64> {
        self.imp().file_size.get()
    }

    pub fn title(&self) -> String {
        self.imp()
            .file_path
            .borrow()
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string())
    }

    pub fn is_modified(&self) -> bool {
        self.buffer().is_modified()
    }

    pub fn toggle_search(&self) {
        let revealer = &self.imp().search_revealer;
        let visible = revealer.reveals_child();
        revealer.set_reveal_child(!visible);
        if !visible {
            self.imp().search_bar.search_entry().grab_focus();
        }
    }
}

impl Default for LushtextEditorPage {
    fn default() -> Self {
        Self::new()
    }
}
