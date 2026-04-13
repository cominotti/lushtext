// SPDX-License-Identifier: GPL-3.0-or-later

//! File load/save and restore-position flows for one editor tab.
//!
//! This stays in the driving-adapter layer because it mutates `GtkSourceView`
//! widgets directly, but the extraction keeps `mod.rs` focused on the public
//! facade while this file owns the async file-I/O choreography.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use gtk4::subclass::prelude::ObjectSubclassIsExt;
use gtk4::{self, glib};
use sourceview5::prelude::*;

use crate::services::file_limits::FileSizeCheck;
use crate::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use crate::services::{async_task, editor_io};

use super::{LushtextEditorPage, SaveError};
use editor_io::LoadError;

type SaveCallback = Box<dyn FnOnce(Result<(), SaveError>)>;
type ChunkedCallback = Rc<RefCell<Option<Box<dyn FnOnce(String)>>>>;

/// Temporary view flags captured while chunked snapshotting makes the editor read-only.
struct ViewInteractivityState {
    editable: bool,
    cursor_visible: bool,
}

/// Files at or above 10MB use chunked snapshotting to avoid a long
/// single-frame pause when copying GtkTextBuffer content to a String.
const LARGE_SAVE_SNAPSHOT_THRESHOLD: u64 = 10_000_000;
/// Characters per slice when chunking large buffer snapshots. 64k chars
/// completes in under 1ms on the GTK main thread, keeping frame times stable.
const SAVE_SNAPSHOT_CHUNK_CHARS: i32 = 64 * 1024;

impl LushtextEditorPage {
    /// Start loading a file asynchronously. Sets the file path immediately
    /// so duplicate detection works before content arrives.
    pub fn load_file_async(&self, path: &Path) {
        let file_path = path.to_path_buf();
        self.imp().file_path.replace(Some(file_path.clone()));

        self.imp().cancel_token.store(false, Ordering::Release);
        let cancel = self.imp().cancel_token.clone();

        async_task::spawn_blocking_then(
            self.clone(),
            move || editor_io::load_text_file(&file_path, &cancel),
            |editor, result| match result {
                Ok(loaded) => {
                    editor.imp().file_size.set(Some(loaded.size));
                    editor.imp().size_check.set(loaded.size_check);
                    editor.imp().evicted.set(false);
                    editor.apply_loaded_content(&loaded.content, loaded.size_check);
                    editor.apply_restore_position();
                    editor.notify_estimated_memory_changed();
                    editor.imp().monitor.last_known_mtime.set(loaded.mtime);
                    editor.clear_inline_notification();
                    if let Some(callback) = editor.imp().draft.load_completed_callback.take() {
                        callback();
                    }
                }
                Err(LoadError::Cancelled) => {}
                Err(error) => {
                    tracing::error!("{error}");
                    editor.emit_inline_notification(InlineActionNotification {
                        style: InlineNotificationStyle::Error,
                        title: "Could Not Open File".to_string(),
                        body: error.to_string(),
                        primary_button: Some("_Retry".to_string()),
                        secondary_button: None,
                    });
                }
            },
        );
    }

    /// Cancel any in-progress file load. Safe to call even if no load is active.
    pub fn cancel_load(&self) {
        self.imp().cancel_token.store(true, Ordering::Release);
    }

    /// Apply freshly loaded file content and its size-based feature gates.
    fn apply_loaded_content(&self, content: &str, check: FileSizeCheck) {
        let buffer = self.buffer();
        buffer.begin_irreversible_action();
        buffer.set_text(content);
        if check.undo_enabled() {
            buffer.end_irreversible_action();
        }
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
    #[must_use]
    pub fn size_check(&self) -> FileSizeCheck {
        self.imp().size_check.get()
    }

    /// Set the file path (used by Save As) and refresh syntax highlighting.
    pub fn set_file_path(&self, path: &Path) {
        self.imp().file_path.replace(Some(path.to_path_buf()));
        if self.imp().size_check.get().syntax_enabled() {
            self.reapply_language();
        }
    }

    /// Detect and apply syntax language from the current file path.
    fn reapply_language(&self) {
        let buffer = self.buffer();
        if let Some(ref file_path) = *self.imp().file_path.borrow() {
            let lang_manager = sourceview5::LanguageManager::default();
            if let Some(language) = lang_manager.guess_language(file_path.to_str(), None::<&str>) {
                buffer.set_language(Some(&language));
            }
        }
    }

    /// Save the file asynchronously on a background thread.
    pub fn save_file_async<F: FnOnce(Result<(), SaveError>) + 'static>(&self, callback: F) {
        let Some(path) = self.imp().file_path.borrow().clone() else {
            callback(Err(SaveError::NoPath));
            return;
        };
        self.save_file_async_to_path(path, callback);
    }

    /// Save the current buffer to an explicit path without mutating the tracked path first.
    pub(crate) fn save_file_async_to_path<F: FnOnce(Result<(), SaveError>) + 'static>(
        &self,
        path: PathBuf,
        callback: F,
    ) {
        let callback: SaveCallback = Box::new(callback);

        if self.file_size().unwrap_or_default() >= LARGE_SAVE_SNAPSHOT_THRESHOLD {
            let view = self.source_view().clone();
            let restore_state = ViewInteractivityState {
                editable: view.is_editable(),
                cursor_visible: view.is_cursor_visible(),
            };
            view.set_editable(false);
            view.set_cursor_visible(false);

            let editor = self.clone();
            snapshot_buffer_text_async(self.buffer(), move |text| {
                editor.write_snapshot_async(path, text, Some(restore_state), callback);
            });
            return;
        }

        let buffer = self.buffer();
        let text = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .to_string();
        self.write_snapshot_async(path, text, None, callback);
    }

    /// Store a cursor and scroll position to apply after the next async load.
    pub fn set_restore_position(&self, cursor_line: u32, cursor_col: u32, scroll_line: u32) {
        self.imp().restore.cursor_line.set(Some(cursor_line));
        self.imp().restore.cursor_col.set(Some(cursor_col));
        self.imp().restore.scroll_line.set(Some(scroll_line));
    }

    /// Read the current cursor position as (line, column).
    #[must_use]
    #[expect(clippy::cast_sign_loss)] // GTK line/offset are non-negative i32
    pub fn cursor_position(&self) -> (u32, u32) {
        let buffer = self.buffer();
        let iter = buffer.iter_at_mark(&buffer.get_insert());
        (iter.line() as u32, iter.line_offset() as u32)
    }

    /// Read the line number at the top of the visible scroll area.
    #[must_use]
    #[expect(clippy::cast_sign_loss, clippy::cast_possible_truncation)] // GTK i32/f64 -> u32
    pub fn visible_top_line(&self) -> u32 {
        let view = self.source_view();
        let Some(vadj) = view.vadjustment() else {
            return 0;
        };
        let (iter, _line_top) = view.line_at_y(vadj.value() as i32);
        iter.line() as u32
    }

    /// Apply stored cursor/scroll position after a file load, then clear it.
    fn apply_restore_position(&self) {
        let line = self.imp().restore.cursor_line.take();
        let col = self.imp().restore.cursor_col.take();
        let scroll_line = self.imp().restore.scroll_line.take();

        let buffer = self.buffer();

        if let Some(line) = line
            && let Some(mut iter) = buffer.iter_at_line(line as i32)
        {
            if let Some(col) = col {
                iter.forward_chars(col as i32);
            }
            buffer.place_cursor(&iter);
        }

        if let Some(scroll_line) = scroll_line
            && let Some(iter) = buffer.iter_at_line(scroll_line as i32)
        {
            let mark = buffer.create_mark(None, &iter, true);
            self.source_view()
                .scroll_to_mark(&mark, 0.0, true, 0.0, 0.0);
            buffer.delete_mark(&mark);
        }
    }

    /// Spawn the background write and restore any temporary view state afterwards.
    fn write_snapshot_async(
        &self,
        path: PathBuf,
        text: String,
        restore_view_state: Option<ViewInteractivityState>,
        callback: SaveCallback,
    ) {
        self.buffer().set_modified(false);

        async_task::spawn_blocking_then(
            self.clone(),
            move || editor_io::write_snapshot_to_path(&path, &text),
            move |editor, result| {
                if let Some(restore) = restore_view_state {
                    editor.source_view().set_editable(restore.editable);
                    editor
                        .source_view()
                        .set_cursor_visible(restore.cursor_visible);
                }

                match result {
                    Ok((size, mtime)) => {
                        editor.imp().file_size.set(Some(size));
                        editor.imp().size_check.set(FileSizeCheck::classify(size));
                        editor.notify_estimated_memory_changed();
                        editor.imp().monitor.last_known_mtime.set(mtime);
                        callback(Ok(()));
                    }
                    Err(error) => {
                        editor.buffer().set_modified(true);
                        callback(Err(error));
                    }
                }
            },
        );
    }
}

#[allow(clippy::needless_pass_by_value)] // GObject: reference-counted, pass-by-value is idiomatic
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

    // A 1ms timeout yields back to the GTK main loop between slices so large
    // saves stay responsive without starving rendering like a tight idle loop would.
    glib::timeout_add_local_once(Duration::from_millis(1), move || {
        snapshot_buffer_text_chunk(buffer, end, text, callback);
    });
}
