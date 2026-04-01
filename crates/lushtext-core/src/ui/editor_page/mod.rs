// SPDX-License-Identifier: GPL-3.0-or-later

//! Editor page widget — one tab's content: GtkSourceView + search bar.

mod imp;

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

    /// Load a file into the editor buffer, detecting language from the path.
    pub fn load_file(&self, path: &Path) -> anyhow::Result<()> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path.display(), e))?;

        let buffer = self.buffer();
        buffer.begin_irreversible_action();
        buffer.set_text(&content);
        buffer.end_irreversible_action();
        buffer.set_modified(false);

        let start = buffer.start_iter();
        buffer.place_cursor(&start);

        let lang_manager = sourceview5::LanguageManager::default();
        if let Some(language) = lang_manager.guess_language(Some(&path.display().to_string()), None)
        {
            buffer.set_language(Some(&language));
        }

        self.imp().file_path.replace(Some(path.to_path_buf()));
        Ok(())
    }

    /// Save the buffer contents back to the file.
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
