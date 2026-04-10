// SPDX-License-Identifier: GPL-3.0-or-later

//! Editor page widget — one tab's content: GtkSourceView + search bar.

// Private implementation module (GObject pattern).
mod imp;

use crate::model::formatting_overrides::FormattingOverrides;
use crate::services::file_limits::FileSizeCheck;
use crate::services::notifications::{InlineActionNotification, InlineNotificationStyle};
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
                Ok(loaded) => {
                    editor.imp().file_size.set(Some(loaded.size));
                    editor.imp().size_check.set(loaded.size_check);
                    editor.imp().evicted.set(false);
                    editor.apply_loaded_content(&loaded.content, loaded.size_check);
                    editor.apply_restore_position();
                    editor.notify_estimated_memory_changed();
                    // Mtime baseline from the metadata already read on the
                    // background thread — no extra stat() on the main thread.
                    editor.imp().last_known_mtime.set(loaded.mtime);
                    editor.clear_inline_notification();
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
                    editor.emit_inline_notification(InlineActionNotification {
                        style: InlineNotificationStyle::Error,
                        title: "Could Not Open File".to_string(),
                        body: e.to_string(),
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
        self.save_file_async_to_path(path, callback);
    }

    /// Save the current buffer to an explicit path without mutating the
    /// editor's tracked file path first. Used by Save As so tab identity only
    /// changes after the write succeeds.
    pub(crate) fn save_file_async_to_path<F: FnOnce(Result<(), SaveError>) + 'static>(
        &self,
        path: PathBuf,
        callback: F,
    ) {
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

    pub fn notification_owner_id(&self) -> usize {
        self.as_ptr() as usize
    }

    /// Open the search bar in find-only mode.
    /// If already open, refocuses the search entry.
    pub fn show_search(&self) {
        self.open_search_bar(false);
    }

    /// Open the search bar in find-and-replace mode.
    /// If already open, switches to replace mode and refocuses.
    pub fn show_replace(&self) {
        self.open_search_bar(true);
    }

    /// Close the search bar, restore the cursor if the user didn't navigate,
    /// detach the SearchContext, and return focus to the editor.
    ///
    /// Order matters: detach + collapse the bar BEFORE restoring the cursor,
    /// so `place_cursor` runs against the full viewport height. Restoring
    /// while the bar is still visible would scroll against a shorter viewport,
    /// and the subsequent bar collapse would make the view appear to jump.
    pub fn hide_search(&self) {
        let imp = self.imp();
        let navigated = imp.search_bar.has_navigated();

        imp.search_bar.detach();
        imp.search_revealer.set_reveal_child(false);

        if !navigated {
            self.restore_pre_search_cursor();
        }

        imp.source_view.grab_focus();
    }

    /// Access the search bar widget (for window-level next/prev delegation).
    pub fn search_bar(&self) -> &crate::ui::search_bar::LushtextSearchBar {
        &self.imp().search_bar
    }

    /// Whether the search bar is currently visible.
    pub fn is_search_visible(&self) -> bool {
        self.imp().search_revealer.reveals_child()
    }

    /// Common logic for opening the search bar.
    fn open_search_bar(&self, replace_mode: bool) {
        let imp = self.imp();
        let search_bar = &imp.search_bar;
        let revealer = &imp.search_revealer;

        let was_visible = revealer.reveals_child();
        if !was_visible {
            // Save the cursor position before search so Escape can restore it.
            self.save_pre_search_cursor();

            // Attach SearchContext to the editor's buffer and view.
            search_bar.attach(&self.buffer(), self.source_view());
            revealer.set_reveal_child(true);

            // Pre-fill from selection if any.
            let buffer = self.buffer();
            if let Some((start, end)) = buffer.selection_bounds() {
                let text = buffer.text(&start, &end, true);
                if !text.is_empty() {
                    search_bar.search_entry().set_text(text.as_str());
                }
            }
        }

        search_bar.set_replace_mode(replace_mode);

        // Focus the search entry and select all text so typing replaces it.
        let entry = search_bar.search_entry();
        entry.grab_focus();
        entry.select_region(0, -1);
    }

    /// Save the current cursor position as a TextMark for later restoration.
    fn save_pre_search_cursor(&self) {
        let buffer = self.buffer();
        let iter = buffer.iter_at_mark(&buffer.get_insert());
        // Use a left-gravity mark so it stays at the original position
        // even if text is inserted at that point during search.
        let mark = buffer.create_mark(Some("pre-search-cursor"), &iter, true);
        // Keep the mark alive — it's owned by the buffer. We'll delete it in restore.
        let _ = mark;
    }

    /// Restore the cursor to the pre-search position.
    ///
    /// Does NOT call `scroll_mark_onscreen` — the caller is responsible
    /// for ensuring the viewport is at its final size before scrolling.
    /// In `hide_search`, `grab_focus()` handles this naturally.
    fn restore_pre_search_cursor(&self) {
        let buffer = self.buffer();
        if let Some(mark) = buffer.mark("pre-search-cursor") {
            let iter = buffer.iter_at_mark(&mark);
            buffer.place_cursor(&iter);
            buffer.delete_mark(&mark);
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

    // --- EditorConfig overrides ---

    /// Apply EditorConfig formatting overrides and update the view.
    /// Called after background EditorConfig resolution completes.
    pub fn apply_editorconfig_overrides(&self, overrides: FormattingOverrides) {
        self.imp().formatting_overrides.set(overrides);
        imp::apply_formatting_settings(&self.imp().source_view, &self.imp().settings, overrides);
    }

    /// Clear all EditorConfig overrides and fall back to GSettings values.
    /// Called when the `use-editorconfig` toggle is disabled.
    pub fn clear_editorconfig_overrides(&self) {
        self.apply_editorconfig_overrides(FormattingOverrides::default());
    }

    /// Current formatting overrides (for status bar indicator).
    pub fn formatting_overrides(&self) -> FormattingOverrides {
        self.imp().formatting_overrides.get()
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
        // Mtime baseline is set by load_file_async's then-callback (from the
        // metadata already read on the background thread). No extra stat() here.

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
                let Some(ref path) = *editor.imp().file_path.borrow() else {
                    return;
                };
                let last_known = editor.imp().last_known_mtime.get();
                // Skip if no baseline yet (load still in progress).
                if last_known.is_none() {
                    return;
                }
                // Compare mtime on a background thread to avoid blocking the
                // GTK main thread with a stat() syscall (slow on NFS/FUSE).
                let path = path.clone();
                async_task::spawn_blocking_then(
                    editor.clone(),
                    move || editor_io::mtime_secs(&path),
                    move |editor, current_mtime| {
                        if current_mtime != last_known {
                            editor.emit_inline_notification(InlineActionNotification {
                                style: InlineNotificationStyle::Warning,
                                title: "File Has Changed on Disk".to_string(),
                                body: "The file was modified by another program.".to_string(),
                                primary_button: Some("_Discard Changes and Reload".to_string()),
                                secondary_button: None,
                            });
                        }
                    },
                );
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
                    Ok((size, mtime)) => {
                        editor.imp().file_size.set(Some(size));
                        editor.imp().size_check.set(FileSizeCheck::classify(size));
                        editor.notify_estimated_memory_changed();
                        // Mtime baseline from the background write — prevents
                        // the file monitor from misinterpreting our own write
                        // as an external change.
                        editor.imp().last_known_mtime.set(mtime);
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
