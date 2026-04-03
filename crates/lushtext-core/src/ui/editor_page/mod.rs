// SPDX-License-Identifier: GPL-3.0-or-later

//! Editor page widget — one tab's content: GtkSourceView + search bar.

// Private implementation module (GObject pattern).
mod imp;

use crate::services::file_limits::FileSizeCheck;
use crate::services::{async_task, editor_io};
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use sourceview5::prelude::*;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::Duration;

pub use crate::services::editor_io::SaveError;
use editor_io::LoadError;

// glib::wrapper! generates the public wrapper type for this widget.
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
            move || editor_io::load_text_file(&file_path, &cancel),
            |editor, result| match result {
                Ok((content, size, check)) => {
                    editor.imp().file_size.set(Some(size));
                    editor.imp().size_check.set(check);
                    editor.imp().evicted.set(false);
                    editor.apply_loaded_content(&content, check);
                    editor.notify_estimated_memory_changed();
                }
                Err(LoadError::Cancelled) => {}
                Err(e) => {
                    tracing::error!("{}", e);
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
    /// based on the new filename's extension, unless syntax was disabled
    /// for large files.
    pub fn set_file_path(&self, path: &Path) {
        self.imp().file_path.replace(Some(path.to_path_buf()));
        if self.imp().size_check.get().syntax_enabled() {
            self.reapply_language();
        }
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
    ///
    /// Sets `modified(false)` optimistically before the write so the tab
    /// title loses its dot immediately. On write failure the flag is rolled back.
    pub fn save_file_async<F: FnOnce(Result<(), SaveError>) + 'static>(&self, callback: F) {
        let path = match self.imp().file_path.borrow().clone() {
            Some(p) => p,
            None => {
                callback(Err(SaveError::NoPath));
                return;
            }
        };
        let callback: SaveCallback = Box::new(callback);

        if self.file_size().unwrap_or_default() >= LARGE_SAVE_SNAPSHOT_THRESHOLD {
            let view = self.source_view().clone();
            let restore_state = (view.is_editable(), view.is_cursor_visible());
            view.set_editable(false);
            view.set_cursor_visible(false);

            let editor = self.clone();
            snapshot_buffer_text_async(self.buffer(), move |text| {
                editor.write_snapshot_async(path, text, Some(restore_state), callback);
            });
            return;
        }

        let buffer = self.buffer();
        // GString → String copy is required: GString is not Send, so the
        // background thread needs an owned String. With the live buffer still
        // resident, save can briefly peak around 3x the file size in memory.
        let text = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .to_string();
        self.write_snapshot_async(path, text, None, callback);
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
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_string())
    }

    pub fn is_modified(&self) -> bool {
        self.buffer().is_modified()
    }

    /// Evict buffer content to free memory. Clears the buffer text and marks
    /// the tab as evicted. The tab will reload from disk when re-focused.
    pub fn evict(&self) {
        // Set evicted flag BEFORE clearing buffer so that the modified-changed
        // signal handler (wire_modified_indicator) can skip title updates
        // for evicted tabs, avoiding a cosmetic title flash.
        self.imp().evicted.set(true);
        let buffer = self.buffer();
        buffer.begin_irreversible_action();
        buffer.set_text("");
        buffer.end_irreversible_action();
        buffer.set_modified(false);
        self.notify_estimated_memory_changed();
    }

    pub fn is_evicted(&self) -> bool {
        self.imp().evicted.get()
    }

    pub fn estimated_buffer_bytes(&self) -> u64 {
        if self.is_evicted() {
            return 0;
        }

        self.file_size()
            .map(|size| size.saturating_mul(self.size_check().estimated_buffer_multiplier()))
            .unwrap_or(0)
    }

    pub fn connect_estimated_memory_changed<F: Fn(u64) + 'static>(&self, f: F) {
        *self.imp().memory_changed_callback.borrow_mut() = Some(Box::new(f));
    }

    fn notify_estimated_memory_changed(&self) {
        if let Some(ref callback) = *self.imp().memory_changed_callback.borrow() {
            callback(self.estimated_buffer_bytes());
        }
    }

    pub fn toggle_search(&self) {
        let revealer = &self.imp().search_revealer;
        let visible = revealer.reveals_child();
        revealer.set_reveal_child(!visible);
        if !visible {
            self.imp().search_bar.search_entry().grab_focus();
        } else {
            self.imp().source_view.grab_focus();
        }
    }

    fn write_snapshot_async(
        &self,
        path: PathBuf,
        text: String,
        restore_view_state: Option<(bool, bool)>,
        callback: SaveCallback,
    ) {
        // Optimistic: clear the modified dot before the async write completes.
        self.buffer().set_modified(false);

        async_task::spawn_blocking_then(
            self.clone(),
            move || editor_io::write_snapshot_to_path(path, text),
            move |editor, result| {
                if let Some((editable, cursor_visible)) = restore_view_state {
                    editor.source_view().set_editable(editable);
                    editor.source_view().set_cursor_visible(cursor_visible);
                }

                match result {
                    Ok(size) => {
                        editor.imp().file_size.set(Some(size));
                        editor.imp().size_check.set(FileSizeCheck::classify(size));
                        editor.notify_estimated_memory_changed();
                        callback(Ok(()));
                    }
                    Err(e) => {
                        // Rollback: restore the modified dot on write failure.
                        editor.buffer().set_modified(true);
                        callback(Err(e));
                    }
                }
            },
        );
    }
}

impl Default for LushtextEditorPage {
    fn default() -> Self {
        Self::new()
    }
}

type SaveCallback = Box<dyn FnOnce(Result<(), SaveError>)>;
type ChunkedCallback = Rc<RefCell<Option<Box<dyn FnOnce(String)>>>>;

/// Files at or above 10MB use chunked snapshotting to avoid a long
/// single-frame pause when copying GtkTextBuffer content to a String.
const LARGE_SAVE_SNAPSHOT_THRESHOLD: u64 = 10_000_000;
/// Characters per slice when chunking large buffer snapshots. 64k chars
/// completes in <1ms on the main thread, within a 16ms frame budget.
const SAVE_SNAPSHOT_CHUNK_CHARS: i32 = 64 * 1024;

fn snapshot_buffer_text_async<F: FnOnce(String) + 'static>(
    buffer: sourceview5::Buffer,
    callback: F,
) {
    let text = Rc::new(RefCell::new(String::new()));
    let callback: ChunkedCallback = Rc::new(RefCell::new(Some(Box::new(callback))));
    snapshot_buffer_text_chunk(buffer.clone(), buffer.start_iter(), text, callback);
}

fn snapshot_buffer_text_chunk(
    buffer: sourceview5::Buffer,
    start: gtk4::TextIter,
    text: Rc<RefCell<String>>,
    callback: ChunkedCallback,
) {
    let mut end = start;
    if !end.forward_chars(SAVE_SNAPSHOT_CHUNK_CHARS) {
        end = buffer.end_iter();
    }

    let chunk = buffer.text(&start, &end, true);
    text.borrow_mut().push_str(chunk.as_str());

    if end == buffer.end_iter() {
        if let Some(callback) = callback.borrow_mut().take() {
            callback(std::mem::take(&mut *text.borrow_mut()));
        }
        return;
    }

    // 1ms timeout yields to the GTK main loop between chunks so the UI
    // stays responsive. Using idle_add would starve rendering because idle
    // callbacks run continuously when there are no higher-priority events.
    glib::timeout_add_local_once(Duration::from_millis(1), move || {
        snapshot_buffer_text_chunk(buffer, end, text, callback);
    });
}
