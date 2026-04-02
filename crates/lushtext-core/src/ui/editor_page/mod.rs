// SPDX-License-Identifier: GPL-3.0-or-later

//! Editor page widget — one tab's content: GtkSourceView + search bar.

mod imp;

use crate::services::async_task;
use crate::services::file_limits::FileSizeCheck;
use glib::subclass::prelude::ObjectSubclassIsExt;
use glib::Object;
use gtk4::prelude::*;
use sourceview5::prelude::*;
use std::path::Path;
use std::sync::atomic::Ordering;

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
    ///
    /// Checks file size before reading and applies feature gates:
    /// - >500MB: refuses to open (returns error message)
    /// - >50MB: disables undo history
    /// - >10MB: disables syntax highlighting
    /// - >1MB: shows large-file toast via the returned `FileSizeCheck`
    pub fn load_file_async(&self, path: &Path) {
        let file_path = path.to_path_buf();
        self.imp().file_path.replace(Some(file_path.clone()));

        // Reset cancel token for this load
        self.imp().cancel_token.store(false, Ordering::Release);
        let cancel = self.imp().cancel_token.clone();

        async_task::spawn_blocking_then(
            self.clone(),
            move || {
                // Check cancellation before doing I/O
                if cancel.load(Ordering::Acquire) {
                    return Err("Load cancelled".to_string());
                }

                let meta = std::fs::metadata(&file_path)
                    .map_err(|e| format!("Cannot stat {}: {}", file_path.display(), e))?;
                let size = meta.len();
                let check = FileSizeCheck::classify(size);

                if check == FileSizeCheck::TooLarge {
                    return Err(format!(
                        "{} is too large to edit ({} MB). Consider a pager like `less`.",
                        file_path.display(),
                        size / 1_000_000
                    ));
                }

                // Check cancellation after size check (user may have closed tab)
                if cancel.load(Ordering::Acquire) {
                    return Err("Load cancelled".to_string());
                }

                let content = std::fs::read_to_string(&file_path)
                    .map_err(|e| format!("Failed to read {}: {}", file_path.display(), e))?;

                Ok((content, size, check))
            },
            |editor, result| match result {
                Ok((content, size, check)) => {
                    editor.imp().file_size.set(Some(size));
                    editor.imp().size_check.set(check);
                    editor.apply_loaded_content(&content, check);
                }
                Err(e) => {
                    if e != "Load cancelled" {
                        tracing::error!("{}", e);
                    }
                }
            },
        );
    }

    /// Cancel any in-progress file load. Safe to call even if no load is active.
    pub fn cancel_load(&self) {
        self.imp().cancel_token.store(true, Ordering::Release);
    }

    fn apply_loaded_content(&self, content: &str, check: FileSizeCheck) {
        let buffer = self.buffer();
        buffer.begin_irreversible_action();
        buffer.set_text(content);
        if check.undo_enabled() {
            buffer.end_irreversible_action();
        }
        // If undo is disabled, we intentionally do NOT call end_irreversible_action(),
        // keeping the buffer permanently in irreversible mode.
        buffer.set_modified(false);

        let start = buffer.start_iter();
        buffer.place_cursor(&start);

        if check.syntax_enabled() {
            self.reapply_language();
        } else {
            buffer.set_language(None::<&sourceview5::Language>);
            buffer.set_highlight_syntax(false);
        }
    }

    /// The size classification from the last file load.
    pub fn size_check(&self) -> FileSizeCheck {
        self.imp().size_check.get()
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

    /// Save the file asynchronously on a background thread.
    /// Calls `callback` on the main thread with the result.
    pub fn save_file_async<F: FnOnce(Result<(), String>) + 'static>(&self, callback: F) {
        let path = match self.imp().file_path.borrow().clone() {
            Some(p) => p,
            None => {
                callback(Err("No file path set".to_string()));
                return;
            }
        };
        let buffer = self.buffer();
        let text = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .to_string();

        async_task::spawn_blocking_then(
            self.clone(),
            move || {
                std::fs::write(&path, &text)
                    .map(|_| text.len() as u64)
                    .map_err(|e| format!("Failed to write {}: {}", path.display(), e))
            },
            move |editor, result| match result {
                Ok(size) => {
                    editor.imp().file_size.set(Some(size));
                    editor.buffer().set_modified(false);
                    callback(Ok(()));
                }
                Err(e) => callback(Err(e)),
            },
        );
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
