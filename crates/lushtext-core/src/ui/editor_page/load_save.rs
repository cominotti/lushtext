// SPDX-License-Identifier: GPL-3.0-or-later

//! File load/save and restore-position flows for one editor tab.
//!
//! This stays in the driving-adapter layer because it mutates `GtkSourceView`
//! widgets directly, but the extraction keeps `mod.rs` focused on the public
//! facade while this file owns the async file-I/O choreography.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gtk4::subclass::prelude::ObjectSubclassIsExt;
use gtk4::{self, glib};
use sourceview5::prelude::*;

use crate::model::encoding::{
    DocumentEncoding, FileHealthFinding, FileHealthFindingKind, FileHealthSeverity,
};
use crate::services::file_limits::FileSizeCheck;
use crate::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use crate::services::{async_task, editor_io};

use super::{LushtextEditorPage, SaveError};
use editor_io::LoadError;

type SaveCallback = Box<dyn FnOnce(Result<(), SaveError>)>;
type ChunkedCallback = Rc<RefCell<Option<Box<dyn FnOnce(String)>>>>;

/// Temporary view flags captured while chunked snapshotting makes the editor read-only.
#[derive(Clone, Copy)]
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
        self.load_file_async_with_encoding(path, None);
    }

    /// Start loading a file asynchronously, optionally forcing a reopen encoding.
    pub fn load_file_async_with_encoding(&self, path: &Path, reopen_as: Option<DocumentEncoding>) {
        let file_path = path.to_path_buf();
        self.imp().file_path.replace(Some(file_path.clone()));
        self.imp().canonical_file_path.borrow_mut().take();

        self.imp()
            .cancel_token
            .borrow()
            .store(true, Ordering::Release);
        let cancel = Arc::new(AtomicBool::new(false));
        self.imp().cancel_token.replace(cancel.clone());
        let load_generation = self.imp().load_generation.get().wrapping_add(1);
        self.imp().load_generation.set(load_generation);

        async_task::spawn_blocking_then(
            self.clone(),
            move || editor_io::load_text_file_with_encoding(&file_path, &cancel, reopen_as),
            move |editor, result| {
                editor.apply_load_result_if_current(load_generation, result);
            },
        );
    }

    /// Apply one background load result only if it still belongs to the newest load.
    ///
    /// Background reads can finish after a later `load_file_async` or
    /// `cancel_load` call. Keeping the generation check inside this helper
    /// makes that stale-result guard easy to exercise without a timing race.
    fn apply_load_result_if_current(
        &self,
        load_generation: u64,
        result: Result<editor_io::LoadResult, LoadError>,
    ) -> bool {
        if self.imp().load_generation.get() != load_generation {
            return false;
        }
        match result {
            Ok(loaded) => {
                self.imp().file_size.set(Some(loaded.size));
                self.imp().size_check.set(loaded.size_check);
                self.imp()
                    .canonical_file_path
                    .replace(loaded.canonical_path);
                self.imp().evicted.set(false);
                self.set_document_encoding_state(loaded.encoding_state);
                self.set_has_bom(loaded.has_bom);
                self.set_file_health(loaded.file_health);
                self.set_minimap_tracking_suspended(true);
                self.apply_loaded_content(&loaded.content, loaded.size_check);
                self.set_minimap_tracking_suspended(false);
                self.clear_modified_line_marks();
                self.apply_restore_position();
                self.notify_estimated_memory_changed();
                self.imp().monitor.last_known_mtime.set(loaded.mtime);
                self.clear_inline_notification();
                self.seed_local_history_from_loaded_content(&loaded.content);
                if self
                    .file_health()
                    .iter()
                    .any(|finding| finding.kind == FileHealthFindingKind::MixedLineEndings)
                {
                    self.emit_inline_notification_with_warning_action(
                        InlineActionNotification {
                            style: InlineNotificationStyle::Warning,
                            title: "Mixed Line Endings Detected".to_string(),
                            body: format!(
                                "This document opened with mixed line endings. Normalize future saves to {}.",
                                self.save_line_ending().label()
                            ),
                            primary_button: Some("_Normalize…".to_string()),
                            secondary_button: None,
                        },
                        super::imp::PendingWarningAction::NormalizeLineEndings,
                    );
                }
                self.refresh_minimap();
                if let Some(callback) = self.imp().load.load_completed_callback.take() {
                    callback();
                }
                for callback in self.imp().load.file_loaded_callbacks.borrow().iter() {
                    callback();
                }
            }
            Err(LoadError::Cancelled) => {}
            Err(error) => {
                tracing::error!("{error}");
                let error_text = error.to_string();
                self.emit_inline_notification(InlineActionNotification {
                    style: InlineNotificationStyle::Error,
                    title: "Could Not Open File".to_string(),
                    body: error_text.clone(),
                    primary_button: Some("_Retry".to_string()),
                    secondary_button: None,
                });
                if let Some(callback) = self.imp().load.load_failed_callback.take() {
                    callback(error_text);
                }
            }
        }
        true
    }

    /// Apply the same post-load size policy without blocking on a giant fixture.
    ///
    /// Widget tests use this `test-utils` seam for UI-observable large-file
    /// capability states that would otherwise require loading tens of megabytes
    /// through `GtkTextBuffer` just to cross a threshold.
    #[cfg(feature = "test-utils")]
    pub fn apply_loaded_content_for_test(&self, content: &str, reported_size: u64) {
        let size_check = FileSizeCheck::classify(reported_size);
        self.imp().file_size.set(Some(reported_size));
        self.imp().size_check.set(size_check);
        self.imp().evicted.set(false);
        self.apply_loaded_content(content, size_check);
        self.seed_local_history_from_loaded_content(content);
        self.notify_estimated_memory_changed();
        self.refresh_minimap();
    }

    /// Cancel any in-progress file load. Safe to call even if no load is active.
    pub fn cancel_load(&self) {
        self.imp()
            .cancel_token
            .borrow()
            .store(true, Ordering::Release);
        self.imp()
            .load_generation
            .set(self.imp().load_generation.get().wrapping_add(1));
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

    /// Test-only seam for the live-buffer snapshot policy.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn save_uses_chunked_snapshot_for_test(&self) -> bool {
        self.live_buffer_requires_chunked_snapshot()
    }

    /// Return the active load generation for stale-callback regression tests.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn load_generation_for_test(&self) -> u64 {
        self.imp().load_generation.get()
    }

    /// Return the active cancellation token so tests can prove token identity rotation.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn load_cancel_token_for_test(&self) -> Arc<AtomicBool> {
        self.imp().cancel_token.borrow().clone()
    }

    /// Apply a synthetic load result through the production stale-generation gate.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn apply_load_result_for_test(
        &self,
        load_generation: u64,
        result: Result<editor_io::LoadResult, LoadError>,
    ) -> bool {
        self.apply_load_result_if_current(load_generation, result)
    }

    /// Set the file path (used by Save As) and refresh syntax highlighting.
    pub fn set_file_path(&self, path: &Path) {
        self.set_file_path_with_canonical(path, None);
    }

    /// Set the display path and known canonical target as one coherent identity.
    pub(crate) fn set_file_path_with_canonical(
        &self,
        path: &Path,
        canonical_path: Option<PathBuf>,
    ) {
        self.imp().file_path.replace(Some(path.to_path_buf()));
        self.imp().canonical_file_path.replace(canonical_path);
        if self.imp().size_check.get().syntax_enabled() {
            self.reapply_language();
        }
        self.schedule_minimap_refresh();
    }

    /// Clear a provisional file identity after the first load fails.
    ///
    /// Failed desktop activation can leave an editor visible with an inline
    /// error, but it must not behave as if the path was successfully opened.
    pub(crate) fn clear_file_path_after_failed_load(&self) {
        self.imp().file_path.replace(None);
        self.imp().canonical_file_path.borrow_mut().take();
        self.buffer().set_language(None::<&sourceview5::Language>);
        self.schedule_minimap_refresh();
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
        if self.imp().save.inflight.get() {
            callback(Err(SaveError::SaveInProgress));
            return;
        }

        self.imp().save.inflight.set(true);
        let view = self.source_view().clone();
        let restore_state = ViewInteractivityState {
            editable: view.is_editable(),
            cursor_visible: view.is_cursor_visible(),
        };
        view.set_editable(false);
        view.set_cursor_visible(false);

        if self.live_buffer_requires_chunked_snapshot() {
            let editor = self.clone();
            snapshot_buffer_text_async(self.buffer(), move |text| {
                editor.write_snapshot_async(path, text, restore_state, callback);
            });
            return;
        }

        let buffer = self.buffer();
        let text = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .to_string();
        self.write_snapshot_async(path, text, restore_state, callback);
    }

    /// Store a cursor and scroll position to apply after the next async load.
    pub fn set_restore_position(&self, cursor_line: u32, cursor_col: u32, scroll_line: u32) {
        self.imp().restore.cursor_line.set(Some(cursor_line));
        self.imp().restore.cursor_col.set(Some(cursor_col));
        self.imp().restore.scroll_line.set(Some(scroll_line));
    }

    /// Read the current cursor position as (line, column).
    #[must_use]
    #[expect(
        clippy::cast_sign_loss,
        reason = "GtkTextIter line and line_offset values are non-negative i32 coordinates"
    )]
    pub fn cursor_position(&self) -> (u32, u32) {
        let buffer = self.buffer();
        let iter = buffer.iter_at_mark(&buffer.get_insert());
        (iter.line() as u32, iter.line_offset() as u32)
    }

    /// Read the line number at the top of the visible scroll area.
    #[must_use]
    #[expect(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "Cursor offsets and scroll positions are derived from non-negative GTK coordinates and persisted as u32 session data"
    )]
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
        restore_view_state: ViewInteractivityState,
        callback: SaveCallback,
    ) {
        self.prepare_local_history_for_save();
        let was_modified_before_save = self.buffer().is_modified();
        let metadata = self.document_encoding_state();
        let formatting_overrides = self.formatting_overrides();
        let allow_lossy = self.take_lossy_save_once();
        let history_availability = self.local_history_availability();

        async_task::spawn_blocking_then(
            self.clone(),
            move || {
                let formatted_text =
                    editor_io::apply_save_formatting_overrides(&text, formatting_overrides);
                let should_update_buffer = formatted_text != text;
                let (size, mtime) = editor_io::write_document_to_path(
                    &path,
                    &formatted_text,
                    metadata.save_encoding,
                    metadata.save_line_ending,
                    allow_lossy,
                )?;
                let canonical_path = path.canonicalize().ok();

                if history_availability.allows_browsing() {
                    let data_dir = crate::services::json_store::data_dir();
                    if let Err(error) = crate::services::local_history_service::capture_snapshot_for_path(
                        &data_dir,
                        &path,
                        &formatted_text,
                        crate::model::local_history::LocalHistorySnapshotOrigin::Save,
                        crate::services::local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
                    ) {
                        tracing::warn!(
                            "Saved {}, but local-history snapshot capture failed: {error}",
                            path.display()
                        );
                    }
                }

                Ok::<_, SaveError>((
                    size,
                    mtime,
                    canonical_path,
                    history_availability
                        .allows_automatic_capture()
                        .then(|| formatted_text.clone()),
                    should_update_buffer.then_some(formatted_text),
                ))
            },
            move |editor, result| {
                editor
                    .source_view()
                    .set_editable(restore_view_state.editable);
                editor
                    .source_view()
                    .set_cursor_visible(restore_view_state.cursor_visible);
                editor.imp().save.inflight.set(false);

                match result {
                    Ok((size, mtime, canonical_path, clean_text, saved_buffer_text)) => {
                        if let Some(saved_buffer_text) = saved_buffer_text {
                            editor.replace_buffer_after_save_formatting(&saved_buffer_text);
                        }
                        editor.buffer().set_modified(false);
                        editor.imp().file_size.set(Some(size));
                        editor.imp().size_check.set(FileSizeCheck::classify(size));
                        let mut state = editor.document_encoding_state();
                        state.opened_encoding = state.save_encoding;
                        state.detected_line_ending = state.save_line_ending;
                        state.decode_confidence = crate::model::encoding::DecodeConfidence::Exact;
                        editor.set_document_encoding_state(state);
                        let has_bom = state.save_encoding.writes_bom();
                        editor.set_has_bom(has_bom);
                        editor.imp().canonical_file_path.replace(canonical_path);
                        let mut findings: Vec<FileHealthFinding> = editor
                            .file_health()
                            .into_iter()
                            .filter(|finding| {
                                !matches!(
                                    finding.kind,
                                    FileHealthFindingKind::LowConfidenceDecode
                                        | FileHealthFindingKind::MixedLineEndings
                                        | FileHealthFindingKind::Utf8Bom
                                )
                            })
                            .collect();
                        if has_bom && state.save_encoding == DocumentEncoding::Utf8Bom {
                            findings.insert(
                                0,
                                FileHealthFinding {
                                    kind: FileHealthFindingKind::Utf8Bom,
                                    severity: FileHealthSeverity::Info,
                                    title: "UTF-8 BOM detected".to_string(),
                                    body:
                                        "This document will be saved with a UTF-8 byte-order mark."
                                            .to_string(),
                                },
                            );
                        }
                        editor.set_file_health(findings);
                        editor.notify_estimated_memory_changed();
                        editor.imp().monitor.last_known_mtime.set(mtime);
                        editor.clear_modified_line_marks();
                        editor.refresh_minimap();
                        editor.complete_local_history_after_save_success(clean_text);
                        callback(Ok(()));
                    }
                    Err(error) => {
                        editor.buffer().set_modified(was_modified_before_save);
                        editor.complete_local_history_after_save_failure();
                        callback(Err(error));
                    }
                }
            },
        );
    }

    /// Mirror save-time EditorConfig rewrites back into the live buffer.
    fn replace_buffer_after_save_formatting(&self, saved_text: &str) {
        let buffer = self.buffer();
        let cursor_offset = buffer.iter_at_mark(&buffer.get_insert()).offset();
        self.set_minimap_tracking_suspended(true);
        buffer.begin_irreversible_action();
        buffer.set_text(saved_text);
        buffer.end_irreversible_action();
        let mut iter = buffer.start_iter();
        iter.forward_chars(cursor_offset.min(buffer.end_iter().offset()));
        buffer.place_cursor(&iter);
        buffer.set_modified(false);
        self.set_minimap_tracking_suspended(false);
    }

    /// Decide whether save snapshotting should yield through the main loop.
    ///
    /// GtkTextBuffer exposes a cheap character count, not a UTF-8 byte count.
    /// Multiplying by four is a conservative upper bound that sends large or
    /// unknown-size buffers through chunked capture without first copying the
    /// whole text just to measure it.
    fn live_buffer_requires_chunked_snapshot(&self) -> bool {
        let char_count = u64::try_from(self.buffer().char_count()).unwrap_or(u64::MAX);
        char_count.saturating_mul(4) >= LARGE_SAVE_SNAPSHOT_THRESHOLD
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "GtkSource buffers are ref-counted GObjects, so pass-by-value keeps the async snapshot helper idiomatic"
)]
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
