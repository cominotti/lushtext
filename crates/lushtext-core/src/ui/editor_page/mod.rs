// SPDX-License-Identifier: GPL-3.0-or-later

//! Editor page widget — one tab's content: GtkSourceView + search bar.

// Private implementation module (GObject pattern).
mod imp;

use crate::services::file_limits::FileSizeCheck;
use crate::services::{async_task, editor_io};
use crate::ui::info_bar::LushtextInfoBar;
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
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
                    editor.apply_restore_position();
                    editor.notify_estimated_memory_changed();
                    // Update mtime baseline after load and dismiss any
                    // "externally changed" bar that triggered this reload.
                    if let Some(ref path) = *editor.imp().file_path.borrow() {
                        editor.update_last_known_mtime(path);
                    }
                    editor.info_bar().dismiss_all();
                    // Fire the one-shot load-completed callback. Used by the
                    // window to defer draft recovery until after file content
                    // is loaded, preventing the race where load overwrites
                    // draft content.
                    if let Some(cb) = editor.imp().load_completed_callback.take() {
                        cb();
                    }
                }
                Err(LoadError::Cancelled) => {}
                Err(e) => {
                    tracing::error!("{}", e);
                    editor.info_bar().show_load_error(&e.to_string());
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

    pub fn info_bar(&self) -> &LushtextInfoBar {
        self.imp().info_bar.as_ref()
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

    // --- Draft state ---

    pub fn draft_dirty(&self) -> bool {
        self.imp().draft_dirty.get()
    }

    pub fn set_draft_dirty(&self, dirty: bool) {
        self.imp().draft_dirty.set(dirty);
    }

    pub fn draft_id(&self) -> Option<String> {
        self.imp().draft_id.borrow().clone()
    }

    pub fn set_draft_id(&self, id: String) {
        *self.imp().draft_id.borrow_mut() = Some(id);
    }

    pub fn is_draft_restored(&self) -> bool {
        self.imp().draft_restored.get()
    }

    pub fn set_draft_restored(&self, restored: bool) {
        self.imp().draft_restored.set(restored);
    }

    // --- Session restore: cursor/scroll position ---

    /// Store a cursor and scroll position to apply after the next async
    /// file load completes. Values are consumed by `apply_restore_position`.
    pub fn set_restore_position(&self, cursor_line: u32, cursor_col: u32, scroll_line: u32) {
        self.imp().restore_cursor_line.set(Some(cursor_line));
        self.imp().restore_cursor_col.set(Some(cursor_col));
        self.imp().restore_scroll_line.set(Some(scroll_line));
    }

    /// Read the current cursor position as (line, column).
    pub fn cursor_position(&self) -> (u32, u32) {
        let buffer = self.buffer();
        let iter = buffer.iter_at_mark(&buffer.get_insert());
        (iter.line() as u32, iter.line_offset() as u32)
    }

    /// Read the line number at the top of the visible scroll area.
    pub fn visible_top_line(&self) -> u32 {
        let view = self.source_view();
        let Some(vadj) = view.vadjustment() else {
            return 0;
        };
        let (iter, _line_top) = view.line_at_y(vadj.value() as i32);
        iter.line() as u32
    }

    /// Apply stored cursor/scroll position after a file load, then clear
    /// the stored values. No-op if no position was stored.
    fn apply_restore_position(&self) {
        let line = self.imp().restore_cursor_line.take();
        let col = self.imp().restore_cursor_col.take();
        let scroll_line = self.imp().restore_scroll_line.take();

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
            // scroll_to_mark queues the scroll for the next layout pass,
            // so it works even before the view is mapped.
            let mark = buffer.create_mark(None, &iter, true);
            self.source_view()
                .scroll_to_mark(&mark, 0.0, true, 0.0, 0.0);
            buffer.delete_mark(&mark);
        }
    }

    // --- File monitoring ---

    /// Start watching the file for external modifications. Creates a
    /// `gio::FileMonitor` and connects its `changed` signal with a 500ms
    /// generation-counter debounce. Only shows the info bar when the file's
    /// mtime actually differs from the last known value (filters noise from
    /// build tools and atomic-write patterns).
    pub fn start_file_monitor(&self) {
        self.stop_file_monitor();
        let Some(ref path) = *self.imp().file_path.borrow() else {
            return;
        };
        self.update_last_known_mtime(path);

        let file = gio::File::for_path(path);
        let monitor = match file.monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Failed to start file monitor: {e}");
                return;
            }
        };

        let editor_weak = self.downgrade();
        monitor.connect_changed(move |_, _, _, event| {
            // Only react to real content changes, not attribute-only updates.
            if !matches!(
                event,
                gio::FileMonitorEvent::Changed | gio::FileMonitorEvent::Created
            ) {
                return;
            }
            let Some(editor) = editor_weak.upgrade() else {
                return;
            };

            let generation = editor.imp().monitor_generation.get().wrapping_add(1);
            editor.imp().monitor_generation.set(generation);

            let editor_weak = editor.downgrade();
            glib::timeout_add_local_once(Duration::from_millis(500), move || {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                if editor.imp().monitor_generation.get() != generation {
                    return;
                }
                // Compare mtime to confirm a real change occurred.
                let Some(ref path) = *editor.imp().file_path.borrow() else {
                    return;
                };
                let current_mtime = editor_io::mtime_secs(path);
                if current_mtime != editor.imp().last_known_mtime.get() {
                    editor.info_bar().show_externally_changed();
                }
            });
        });

        *self.imp().file_monitor.borrow_mut() = Some(monitor);
    }

    /// Stop watching the file. Cancels the monitor and clears state.
    pub fn stop_file_monitor(&self) {
        if let Some(monitor) = self.imp().file_monitor.take() {
            monitor.cancel();
        }
    }

    /// Record the file's current mtime as the baseline for change detection.
    fn update_last_known_mtime(&self, path: &Path) {
        self.imp().last_known_mtime.set(editor_io::mtime_secs(path));
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
                        // Update mtime baseline after save so the file monitor
                        // doesn't misinterpret our own write as an external change.
                        if let Some(ref path) = *editor.imp().file_path.borrow() {
                            editor.update_last_known_mtime(path);
                        }
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
