// SPDX-License-Identifier: GPL-3.0-or-later

//! File load and restore-position flows for one editor tab.
//!
//! **This file is mid-migration and its name is stale.** The document-save
//! workflow that used to share it has migrated to its own role home at
//! [`super::save`], which is where Ctrl+S, Save As, and close-with-changes now
//! live. What remains here is the document-load half, plus one group of
//! cross-cutting state, and it has **not** been migrated to the workflow
//! readability convention yet — that is slot 3b
//! (`migrate-document-load-workflow-readability`), which dissolves this file.
//! The name is deliberately unchanged until then: renaming a file two
//! consecutive changes touch is churn next to a durable write path.
//!
//! Two things here are **not** load state, and slot 3b must not absorb them:
//!
//! - The **restore-position group** (`set_restore_position`, `cursor_position`,
//!   `visible_top_line`, `apply_restore_position`) is cross-cutting editor-page
//!   state with five owning workflows — session restore, editor find, notes and
//!   bookmarks, load, and the window's tab handling. Cross-cutting eligibility
//!   counts owning workflows, so it stays in a shared `ui/editor_page/` location.
//! - Document identity and metadata (`set_file_path`,
//!   `set_file_path_with_canonical`, `size_check`) are shared with the rename,
//!   minimap, encoding, accessibility, and local-history paths.
//!
//! This stays in the driving-adapter layer because it mutates `GtkSourceView`
//! widgets directly.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gtk_lush_tasks::spawn_blocking_then;
use gtk4;
use gtk4::glib;
use gtk4::subclass::prelude::ObjectSubclassIsExt;
use sourceview5::prelude::*;

use crate::model::encoding::{DocumentEncoding, FileHealthFindingKind};
use crate::model::file_load::{SYNCHRONOUS_INSTALL_THRESHOLD_BYTES, next_install_boundary};
use crate::services::editor_io;
use crate::services::file_limits::FileSizeCheck;
use crate::services::notifications::{InlineActionNotification, InlineNotificationStyle};

use super::load_runtime::{self, GuardedLoadResult, TransientLoadPermit};
use super::{EditorLoadState, LushtextEditorPage};
use editor_io::EditorLoadError;

/// Temporary view flags captured while chunked snapshotting makes the editor read-only.
#[derive(Clone, Copy)]
struct ViewInteractivityState {
    editable: bool,
    cursor_visible: bool,
}

/// Projection and view flags restored after one complete or cancelled install.
#[derive(Clone, Copy)]
struct LoadInstallationState {
    view: ViewInteractivityState,
    minimap_tracking_suspended: bool,
    history_capture_suppressed: bool,
    projection_suspended: bool,
}

/// Latest load request held while a prior partial buffer is cleared in slices.
pub(crate) struct PendingFileLoad {
    path: PathBuf,
    reopen_as: Option<DocumentEncoding>,
    planning_terminal: Option<Box<dyn FnOnce()>>,
}

impl PendingFileLoad {
    fn finish_planning(mut self) {
        if let Some(callback) = self.planning_terminal.take() {
            callback();
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LoadInstallPhase {
    ClearingExisting,
    Installing,
    ClearingCancelled,
    Finalizing,
}

#[derive(Clone, Copy)]
enum AbortDisposition {
    Cancel,
    Dispose,
}

/// Bound deletion work by characters so even four-byte Unicode stays within
/// the byte-oriented installation slice policy.
const CLEAR_SLICE_CHARS: i32 = 64 * 1024;

/// One generation-bound GTK installation retaining decoded text and admission.
pub(crate) struct ChunkedLoadInstall {
    editor: glib::WeakRef<LushtextEditorPage>,
    buffer: sourceview5::Buffer,
    end_mark: Option<gtk4::TextMark>,
    loaded: Option<GuardedLoadResult>,
    byte_offset: usize,
    generation: u64,
    source_id: Option<glib::SourceId>,
    permit: Option<TransientLoadPermit>,
    restore: LoadInstallationState,
    phase: LoadInstallPhase,
    terminal: bool,
    slice_count: u64,
}

fn schedule_install_slice(session: &Rc<RefCell<ChunkedLoadInstall>>) {
    let session_for_source = Rc::clone(session);
    let source_id = glib::timeout_add_local_once(Duration::from_millis(1), move || {
        run_install_slice(&session_for_source);
    });
    session.borrow_mut().source_id = Some(source_id);
}

fn run_install_slice(session: &Rc<RefCell<ChunkedLoadInstall>>) {
    let (editor, generation, phase) = {
        let mut state = session.borrow_mut();
        state.source_id.take();
        if state.terminal {
            return;
        }
        (state.editor.upgrade(), state.generation, state.phase)
    };
    let Some(editor) = editor else {
        abort_chunked_install(session, AbortDisposition::Dispose);
        return;
    };
    if phase != LoadInstallPhase::ClearingCancelled
        && (editor.imp().load_tracking.generation.get() != generation
            || editor
                .imp()
                .load_tracking
                .cancel_token
                .borrow()
                .load(Ordering::Acquire))
    {
        abort_chunked_install(session, AbortDisposition::Cancel);
        return;
    }

    match phase {
        LoadInstallPhase::ClearingExisting => run_existing_clear_slice(session),
        LoadInstallPhase::Installing => run_text_insert_slice(session),
        LoadInstallPhase::ClearingCancelled => run_cancelled_clear_slice(session),
        LoadInstallPhase::Finalizing => {}
    }
}

fn delete_buffer_slice(buffer: &sourceview5::Buffer) -> bool {
    let remaining = buffer.char_count();
    if remaining <= 0 {
        return true;
    }
    let mut start = buffer.start_iter();
    let mut end = start;
    let _ = end.forward_chars(remaining.min(CLEAR_SLICE_CHARS));
    // GTK text layout validates whole paragraphs, so a deletion that stops
    // inside a line would re-lay-out the shrinking remainder on every turn.
    // Extending to the next line start deletes each paragraph exactly once.
    if !end.is_end() && !end.starts_line() {
        let _ = end.forward_line();
    }
    buffer.delete(&mut start, &mut end);
    buffer.char_count() == 0
}

fn run_existing_clear_slice(session: &Rc<RefCell<ChunkedLoadInstall>>) {
    let buffer = session.borrow().buffer.clone();
    let cleared = delete_buffer_slice(&buffer);
    if session.borrow().phase != LoadInstallPhase::ClearingExisting {
        return;
    }
    if cleared {
        let mark = buffer.create_mark(None, &buffer.end_iter(), false);
        let mut state = session.borrow_mut();
        if state.phase != LoadInstallPhase::ClearingExisting || state.terminal {
            buffer.delete_mark(&mark);
            return;
        }
        state.end_mark = Some(mark);
        state.phase = LoadInstallPhase::Installing;
    }
    schedule_install_slice(session);
}

fn run_text_insert_slice(session: &Rc<RefCell<ChunkedLoadInstall>>) {
    let (buffer, mark, start, loaded) = {
        let mut state = session.borrow_mut();
        let Some(mark) = state.end_mark.clone() else {
            drop(state);
            abort_chunked_install(session, AbortDisposition::Cancel);
            return;
        };
        let Some(loaded) = state.loaded.take() else {
            drop(state);
            abort_chunked_install(session, AbortDisposition::Cancel);
            return;
        };
        (state.buffer.clone(), mark, state.byte_offset, loaded)
    };
    let end = next_install_boundary(&loaded.content, start);
    let content_len = loaded.content.len();
    let mut iter = buffer.iter_at_mark(&mark);
    // GtkTextBuffer emits signals synchronously. Keep the session unborrowed so
    // a reentrant cancellation can move it to bounded cleanup without panicking.
    buffer.insert(&mut iter, &loaded.content[start..end]);

    let reached_end = {
        let mut state = session.borrow_mut();
        if state.terminal || state.phase != LoadInstallPhase::Installing {
            drop(state);
            drop(loaded);
            return;
        }
        state.byte_offset = end;
        state.slice_count = state.slice_count.saturating_add(1);
        state.loaded = Some(loaded);
        end == content_len
    };

    if reached_end {
        finish_chunked_install(session);
    } else {
        schedule_install_slice(session);
    }
}

fn finish_chunked_install(session: &Rc<RefCell<ChunkedLoadInstall>>) {
    let (editor, buffer, mark, source, loaded, restore, permit, slice_count, generation) = {
        let mut state = session.borrow_mut();
        if state.terminal {
            return;
        }
        state.phase = LoadInstallPhase::Finalizing;
        (
            state.editor.upgrade(),
            state.buffer.clone(),
            state.end_mark.take(),
            state.source_id.take(),
            state.loaded.take(),
            state.restore,
            state.permit.take(),
            state.slice_count,
            state.generation,
        )
    };
    if let Some(source) = source {
        source.remove();
    }
    if let Some(editor) = editor.as_ref() {
        editor.imp().load.finalizing.set(true);
        editor.imp().load.dispose_during_finalization.set(false);
    }
    if let Some(mark) = mark {
        buffer.delete_mark(&mark);
    }
    let Some(editor) = editor else {
        drop(loaded);
        drop(permit);
        return;
    };
    editor.imp().load.installation_slice_count.set(slice_count);
    if editor.imp().load.dispose_during_finalization.get()
        || editor.imp().load_tracking.generation.get() != generation
    {
        buffer.end_irreversible_action();
        session.borrow_mut().terminal = true;
        clear_installation_owner(&editor, session);
        editor.finish_load_finalization(permit);
        drop(loaded);
        return;
    }
    let Some(loaded) = loaded else {
        session.borrow_mut().terminal = true;
        clear_installation_owner(&editor, session);
        editor.finish_load_finalization(permit);
        return;
    };
    // Retain admission through every final projection and callback. Dropping
    // it afterward schedules one coalesced queue drain.
    editor.complete_loaded_installation(loaded, restore);
    session.borrow_mut().terminal = true;
    clear_installation_owner(&editor, session);
    editor.finish_load_finalization(permit);
}

fn abort_chunked_install(session: &Rc<RefCell<ChunkedLoadInstall>>, disposition: AbortDisposition) {
    let (editor, buffer, mark, source, loaded, permit) = {
        let mut state = session.borrow_mut();
        if state.terminal {
            return;
        }
        if state.phase == LoadInstallPhase::Finalizing {
            return;
        }
        if matches!(disposition, AbortDisposition::Cancel)
            && state.phase == LoadInstallPhase::ClearingCancelled
        {
            return;
        }
        if matches!(disposition, AbortDisposition::Dispose) {
            state.terminal = true;
        } else {
            state.phase = LoadInstallPhase::ClearingCancelled;
        }
        let permit = if matches!(disposition, AbortDisposition::Dispose) {
            state.permit.take()
        } else {
            None
        };
        (
            state.editor.upgrade(),
            state.buffer.clone(),
            state.end_mark.take(),
            state.source_id.take(),
            state.loaded.take(),
            permit,
        )
    };
    if let Some(source) = source {
        source.remove();
    }
    if let Some(mark) = mark {
        buffer.delete_mark(&mark);
    }
    // Release decoded text before either disposal releases admission or
    // cancellation begins bounded cleanup of the partial GTK buffer.
    drop(loaded);
    if matches!(disposition, AbortDisposition::Dispose) {
        if let Some(editor) = editor {
            clear_installation_owner(&editor, session);
        }
        drop(buffer);
        drop(permit);
        return;
    }
    let Some(editor) = editor else {
        abort_chunked_install(session, AbortDisposition::Dispose);
        return;
    };
    editor.imp().load.installation_incomplete.set(true);
    schedule_install_slice(session);
}

fn run_cancelled_clear_slice(session: &Rc<RefCell<ChunkedLoadInstall>>) {
    let (editor, buffer) = {
        let state = session.borrow();
        (state.editor.upgrade(), state.buffer.clone())
    };
    let Some(editor) = editor else {
        abort_chunked_install(session, AbortDisposition::Dispose);
        return;
    };
    if !delete_buffer_slice(&buffer) {
        schedule_install_slice(session);
        return;
    }

    let (restore, permit, slice_count) = {
        let mut state = session.borrow_mut();
        if state.terminal || state.phase != LoadInstallPhase::ClearingCancelled {
            return;
        }
        state.terminal = true;
        (state.restore, state.permit.take(), state.slice_count)
    };
    clear_installation_owner(&editor, session);
    buffer.end_irreversible_action();
    buffer.set_modified(false);
    editor.imp().load.installation_slice_count.set(slice_count);
    editor.restore_load_installation_state(restore);
    if editor.imp().load.user_cancel_pending.replace(false) {
        editor.finish_user_cancelled_load();
    } else {
        editor.refresh_accessibility_metadata();
    }
    let pending = editor.imp().load.pending_load.take();
    drop(permit);
    if let Some(pending) = pending {
        editor.load_file_async_with_encoding_and_planning_terminal(
            &pending.path,
            pending.reopen_as,
            pending.planning_terminal,
        );
    }
}

fn clear_installation_owner(
    editor: &LushtextEditorPage,
    session: &Rc<RefCell<ChunkedLoadInstall>>,
) {
    let is_current = editor
        .imp()
        .load
        .installation
        .borrow()
        .as_ref()
        .is_some_and(|current| Rc::ptr_eq(current, session));
    if is_current {
        editor.imp().load.installation.take();
    }
}

impl LushtextEditorPage {
    /// Start loading a file asynchronously. Sets the file path immediately
    /// so duplicate detection works before content arrives.
    pub fn load_file_async(&self, path: &Path) {
        self.load_file_async_with_encoding_and_planning_terminal(path, None, None);
    }

    /// Start loading a file asynchronously, optionally forcing a reopen encoding.
    pub fn load_file_async_with_encoding(&self, path: &Path, reopen_as: Option<DocumentEncoding>) {
        self.load_file_async_with_encoding_and_planning_terminal(path, reopen_as, None);
    }

    /// Start one load whose background planning admission is owned externally.
    pub(crate) fn load_file_async_with_planning_terminal<F>(&self, path: &Path, on_terminal: F)
    where
        F: FnOnce() + 'static,
    {
        self.load_file_async_with_encoding_and_planning_terminal(
            path,
            None,
            Some(Box::new(on_terminal)),
        );
    }

    fn load_file_async_with_encoding_and_planning_terminal(
        &self,
        path: &Path,
        reopen_as: Option<DocumentEncoding>,
        planning_terminal: Option<Box<dyn FnOnce()>>,
    ) {
        if self.imp().load.finalizing.get() {
            if let Some(replaced) = self.imp().load.pending_load.replace(Some(PendingFileLoad {
                path: path.to_path_buf(),
                reopen_as,
                planning_terminal,
            })) {
                replaced.finish_planning();
            }
            self.cancel_noninstall_load_resources();
            return;
        }
        let installation = self.imp().load.installation.borrow().clone();
        if let Some(session) = installation {
            if let Some(replaced) = self.imp().load.pending_load.replace(Some(PendingFileLoad {
                path: path.to_path_buf(),
                reopen_as,
                planning_terminal,
            })) {
                replaced.finish_planning();
            }
            self.imp()
                .load_tracking
                .cancel_token
                .borrow()
                .store(true, Ordering::Release);
            load_runtime::cancel_for_editor(self);
            abort_chunked_install(&session, AbortDisposition::Cancel);
            return;
        }
        let file_path = path.to_path_buf();
        let previous_load_state = self.imp().load_state.get();
        let error_state = if previous_load_state == EditorLoadState::Loaded {
            EditorLoadState::Loaded
        } else {
            EditorLoadState::Failed
        };
        self.cancel_noninstall_load_resources();
        self.imp()
            .load
            .planning_terminal_callback
            .replace(planning_terminal);
        self.imp().file_path.replace(Some(file_path.clone()));
        self.imp().canonical_file_path.borrow_mut().take();
        self.imp().file_size.set(None);
        self.imp().load_state.set(EditorLoadState::Loading);
        self.imp().latest_load_failed.set(false);
        self.notify_memory_policy_changed();
        self.refresh_accessibility_metadata();
        self.stop_file_monitor();

        let cancel = Arc::new(AtomicBool::new(false));
        self.imp()
            .load_tracking
            .cancel_token
            .replace(cancel.clone());
        let load_generation = self.imp().load_tracking.generation.get().wrapping_add(1);
        self.imp().load_tracking.generation.set(load_generation);

        let editor_weak = self.downgrade();
        let cancel_for_plan = Arc::clone(&cancel);
        spawn_blocking_then(
            editor_weak,
            move || editor_io::plan_text_file(&file_path, &cancel_for_plan),
            move |editor_weak, result| {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                if !editor.load_request_is_current(load_generation, &cancel) {
                    editor.finish_load_planning();
                    return;
                }
                match result {
                    Ok(plan) => load_runtime::submit(
                        &editor,
                        load_generation,
                        plan,
                        reopen_as,
                        cancel,
                        error_state,
                    ),
                    Err(error) => {
                        editor.apply_load_result_if_current(
                            load_generation,
                            Err(error),
                            error_state,
                        );
                    }
                }
                editor.finish_load_planning();
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
        result: Result<editor_io::LoadResult, EditorLoadError>,
        error_state: EditorLoadState,
    ) -> bool {
        if self.imp().load_tracking.generation.get() != load_generation
            || self
                .imp()
                .load_tracking
                .cancel_token
                .borrow()
                .load(Ordering::Acquire)
        {
            return false;
        }
        match result {
            Ok(editor_io::LoadResult { metadata, content }) => {
                let weight = u64::try_from(content.capacity()).unwrap_or(u64::MAX);
                let Some(mut reservation) = crate::ui::plain_disposal::try_reserve_for_gtk(weight)
                else {
                    return false;
                };
                reservation.shrink_to(weight);
                self.install_guarded_load(
                    load_generation,
                    GuardedLoadResult {
                        metadata,
                        content: reservation.own(content),
                    },
                    None,
                );
            }
            Err(EditorLoadError::Cancelled) => {}
            Err(error) => self.publish_load_error(&error, error_state),
        }
        true
    }

    pub(super) fn accept_admitted_load_outcome(
        &self,
        load_generation: u64,
        result: Result<GuardedLoadResult, EditorLoadError>,
        error_state: EditorLoadState,
        permit: TransientLoadPermit,
    ) {
        if self.imp().load_tracking.generation.get() != load_generation
            || self
                .imp()
                .load_tracking
                .cancel_token
                .borrow()
                .load(Ordering::Acquire)
        {
            return;
        }
        match result {
            Ok(loaded) => self.install_guarded_load(load_generation, loaded, Some(permit)),
            Err(EditorLoadError::Cancelled) => {}
            Err(error) => self.publish_load_error(&error, error_state),
        }
    }

    fn install_guarded_load(
        &self,
        load_generation: u64,
        loaded: GuardedLoadResult,
        permit: Option<TransientLoadPermit>,
    ) {
        if self.requires_chunked_install(loaded.content.len()) {
            self.start_chunked_install(load_generation, loaded, permit);
        } else {
            self.install_loaded_direct(loaded, permit);
        }
    }

    fn publish_load_error(&self, error: &EditorLoadError, error_state: EditorLoadState) {
        tracing::error!("{error}");
        let error_text = error.to_string();
        self.imp().load_state.set(error_state);
        self.imp().latest_load_failed.set(true);
        self.notify_memory_policy_changed();
        self.emit_inline_notification(InlineActionNotification {
            style: InlineNotificationStyle::Error,
            title: "Could Not Open File".to_string(),
            body: error_text.clone(),
            primary_button: Some("_Retry".to_string()),
            secondary_button: None,
        });
        self.refresh_accessibility_metadata();
        if error_state == EditorLoadState::Loaded {
            self.start_file_monitor();
        }
        if let Some(callback) = self.imp().load.load_failed_callback.take() {
            callback(error_text);
        }
    }

    fn install_loaded_direct(
        &self,
        loaded: GuardedLoadResult,
        permit: Option<TransientLoadPermit>,
    ) {
        self.imp().load.installation_slice_count.set(0);
        let restore = self.begin_load_installation(false);
        let buffer = self.buffer();
        self.imp().load.finalizing.set(true);
        self.imp().load.dispose_during_finalization.set(false);
        buffer.begin_irreversible_action();
        buffer.set_text(&loaded.content);
        if self.imp().load.dispose_during_finalization.get() {
            buffer.end_irreversible_action();
            self.finish_load_finalization(permit);
            return;
        }
        self.complete_loaded_installation(loaded, restore);
        self.finish_load_finalization(permit);
    }

    fn start_chunked_install(
        &self,
        generation: u64,
        loaded: GuardedLoadResult,
        permit: Option<TransientLoadPermit>,
    ) {
        let restore = self.begin_load_installation(true);
        let buffer = self.buffer();
        buffer.begin_irreversible_action();
        let session = Rc::new(RefCell::new(ChunkedLoadInstall {
            editor: self.downgrade(),
            buffer,
            end_mark: None,
            loaded: Some(loaded),
            byte_offset: 0,
            generation,
            source_id: None,
            permit,
            restore,
            phase: LoadInstallPhase::ClearingExisting,
            terminal: false,
            slice_count: 0,
        }));
        self.imp().load.installation_slice_count.set(0);
        self.imp()
            .load
            .installation
            .replace(Some(Rc::clone(&session)));
        schedule_install_slice(&session);
    }

    fn requires_chunked_install(&self, incoming_bytes: usize) -> bool {
        let existing_bytes = u64::try_from(self.buffer().char_count())
            .unwrap_or(u64::MAX)
            .saturating_mul(4);
        incoming_bytes > SYNCHRONOUS_INSTALL_THRESHOLD_BYTES
            || existing_bytes
                > u64::try_from(SYNCHRONOUS_INSTALL_THRESHOLD_BYTES).unwrap_or(u64::MAX)
    }

    fn begin_load_installation(&self, suspend_minimap_projection: bool) -> LoadInstallationState {
        self.invalidate_minimap_analysis_content();
        let imp = self.imp();
        let view = self.source_view();
        let restore = LoadInstallationState {
            view: ViewInteractivityState {
                editable: view.is_editable(),
                cursor_visible: view.is_cursor_visible(),
            },
            minimap_tracking_suspended: imp.minimap.tracking_suspended.replace(true),
            history_capture_suppressed: imp
                .local_history
                .automatic_capture_suppressed
                .replace(true),
            projection_suspended: imp.load.projection_suspended.replace(true),
        };
        if suspend_minimap_projection {
            self.suspend_minimap_projection();
        }
        if self.search_bar().search_context().is_some() {
            self.search_bar().detach();
        }
        view.set_editable(false);
        view.set_cursor_visible(false);
        self.buffer().set_highlight_syntax(false);
        self.refresh_accessibility_metadata();
        restore
    }

    fn complete_loaded_installation(
        &self,
        loaded: GuardedLoadResult,
        restore: LoadInstallationState,
    ) {
        let GuardedLoadResult {
            metadata: loaded,
            content,
        } = loaded;
        let editor_io::LoadMetadata {
            size,
            size_check,
            canonical_path,
            mtime,
            encoding_state,
            has_bom,
            file_health,
        } = loaded;
        let buffer = self.buffer();
        if size_check.undo_enabled() {
            buffer.end_irreversible_action();
        }
        buffer.set_modified(false);
        buffer.place_cursor(&buffer.start_iter());

        self.imp().file_size.set(Some(size));
        self.imp().size_check.set(size_check);
        self.imp().canonical_file_path.replace(canonical_path);
        self.imp().latest_load_failed.set(false);
        self.imp().load.installation_incomplete.set(false);
        self.imp().residency.evicted.set(false);
        self.set_document_encoding_state(encoding_state);
        self.set_has_bom(has_bom);
        self.set_file_health(file_health);
        if size_check.syntax_enabled() {
            self.reapply_language();
            buffer.set_highlight_syntax(true);
        } else {
            buffer.set_language(None::<&sourceview5::Language>);
            buffer.set_highlight_syntax(false);
        }
        self.clear_modified_line_marks();
        self.apply_restore_position();
        self.imp().monitor.last_known_mtime.set(mtime);
        self.clear_inline_notification();
        self.seed_local_history_from_guarded_loaded_content(content);
        self.restore_load_installation_state(restore);
        self.imp().load_state.set(EditorLoadState::Loaded);
        self.notify_memory_policy_changed();
        self.start_file_monitor();
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
        self.refresh_accessibility_metadata();
        self.imp().load.load_failed_callback.borrow_mut().take();
        if let Some(callback) = self.imp().load.load_completed_callback.take() {
            callback();
        }
        // Snapshot the callbacks and drop the borrow before invoking, so a
        // file-loaded callback that re-enters `connect_file_loaded` (a
        // `borrow_mut`) cannot panic the GTK thread. Callbacks registered during
        // invocation land in the temporarily-empty live vec and are appended
        // after the originals, preserving invocation order.
        let callbacks = std::mem::take(&mut *self.imp().load.file_loaded_callbacks.borrow_mut());
        for callback in &callbacks {
            callback();
        }
        let mut slot = self.imp().load.file_loaded_callbacks.borrow_mut();
        let newly_registered = std::mem::replace(&mut *slot, callbacks);
        slot.extend(newly_registered);
    }

    fn restore_load_installation_state(&self, restore: LoadInstallationState) {
        self.imp()
            .load
            .projection_suspended
            .set(restore.projection_suspended);
        self.imp()
            .local_history
            .automatic_capture_suppressed
            .set(restore.history_capture_suppressed);
        self.set_minimap_tracking_suspended(restore.minimap_tracking_suspended);
        self.source_view().set_editable(restore.view.editable);
        self.source_view()
            .set_cursor_visible(restore.view.cursor_visible);
        if self.is_search_visible() && self.search_bar().search_context().is_none() {
            self.search_bar().attach(&self.buffer(), self.source_view());
        }
        self.refresh_minimap();
    }

    /// Release finalization ownership and start only the newest deferred reload.
    fn finish_load_finalization(&self, permit: Option<TransientLoadPermit>) {
        self.imp().load.finalizing.set(false);
        let disposed = self.imp().load.dispose_during_finalization.replace(false);
        let pending = self.imp().load.pending_load.take();
        drop(permit);
        if !disposed && let Some(pending) = pending {
            self.load_file_async_with_encoding(&pending.path, pending.reopen_as);
        }
    }

    /// Apply the same post-load size policy without blocking on a giant fixture.
    ///
    /// Widget tests use this `test-utils` seam for UI-observable large-file
    /// capability states that would otherwise require loading tens of megabytes
    /// through `GtkTextBuffer` just to cross a threshold.
    ///
    /// # Panics
    ///
    /// Panics when the test process has already exhausted the guarded disposal
    /// capacity required to own the synthetic body.
    #[cfg(feature = "test-utils")]
    pub fn apply_loaded_content_for_test(&self, content: &str, reported_size: u64) {
        let size_check = FileSizeCheck::classify(reported_size);
        let content = content.to_string();
        let weight = u64::try_from(content.capacity()).unwrap_or(u64::MAX);
        let reservation = crate::ui::plain_disposal::try_reserve_for_gtk(weight)
            .expect("test load body should acquire disposal capacity");
        self.install_loaded_direct(
            GuardedLoadResult {
                metadata: editor_io::LoadMetadata {
                    size: reported_size,
                    size_check,
                    canonical_path: self.canonical_file_path(),
                    mtime: self.imp().monitor.last_known_mtime.get(),
                    encoding_state: self.document_encoding_state(),
                    has_bom: self.has_bom(),
                    file_health: self.file_health(),
                },
                content: reservation.own(content),
            },
            None,
        );
    }

    /// Cancel any in-progress file load. Safe to call even if no load is active.
    pub fn cancel_load(&self) {
        if self.imp().load.finalizing.get() {
            // Final projection owns the main thread and has no cancellable
            // payload work left. A cancel can still withdraw a reload queued
            // reentrantly by an earlier callback.
            if let Some(pending) = self.imp().load.pending_load.take() {
                pending.finish_planning();
            }
            return;
        }
        let was_loading = self.imp().load_state.get() == EditorLoadState::Loading;
        let installation_active = self.imp().load.installation.borrow().is_some();
        if let Some(pending) = self.imp().load.pending_load.take() {
            pending.finish_planning();
        }
        self.imp()
            .load
            .user_cancel_pending
            .set(was_loading && installation_active);
        self.cancel_current_load_resources(AbortDisposition::Cancel);
        self.imp()
            .load_tracking
            .generation
            .set(self.imp().load_tracking.generation.get().wrapping_add(1));
        if was_loading && !installation_active {
            self.finish_user_cancelled_load();
        }
    }

    fn finish_user_cancelled_load(&self) {
        self.imp().load_state.set(EditorLoadState::Failed);
        self.imp().latest_load_failed.set(true);
        self.emit_inline_notification(InlineActionNotification {
            style: InlineNotificationStyle::Error,
            title: "Loading Cancelled".to_string(),
            body: "The document was not fully loaded. Retry when you are ready.".to_string(),
            primary_button: Some("_Retry".to_string()),
            secondary_button: None,
        });
        self.notify_memory_policy_changed();
        self.refresh_accessibility_metadata();
    }

    /// Tear down queued, admitted, or installing load state without UI feedback.
    ///
    /// Disposal is not a user cancellation surface: the page is leaving the
    /// widget hierarchy, so a retry notification would be invisible and stale.
    pub(super) fn dispose_load_resources(&self) {
        if self.imp().load.finalizing.get() {
            self.imp().load.dispose_during_finalization.set(true);
        }
        if let Some(pending) = self.imp().load.pending_load.take() {
            pending.finish_planning();
        }
        self.imp().load.user_cancel_pending.set(false);
        self.cancel_current_load_resources(AbortDisposition::Dispose);
        self.imp()
            .load_tracking
            .generation
            .set(self.imp().load_tracking.generation.get().wrapping_add(1));
    }

    fn cancel_noninstall_load_resources(&self) {
        self.imp()
            .load_tracking
            .cancel_token
            .borrow()
            .store(true, Ordering::Release);
        load_runtime::cancel_for_editor(self);
        self.finish_load_planning();
    }

    fn finish_load_planning(&self) {
        if let Some(callback) = self.imp().load.planning_terminal_callback.take() {
            callback();
        }
    }

    fn cancel_current_load_resources(&self, disposition: AbortDisposition) {
        self.cancel_noninstall_load_resources();
        let installation = self.imp().load.installation.borrow().clone();
        if let Some(session) = installation {
            abort_chunked_install(&session, disposition);
        }
    }

    pub(super) fn load_request_is_current(
        &self,
        generation: u64,
        cancel: &Arc<AtomicBool>,
    ) -> bool {
        self.imp().load_tracking.generation.get() == generation
            && Arc::ptr_eq(&self.imp().load_tracking.cancel_token.borrow(), cancel)
            && !cancel.load(Ordering::Acquire)
    }

    /// Whether document-amplifying callbacks must ignore load installation edits.
    #[must_use]
    pub(crate) fn load_projection_suspended(&self) -> bool {
        self.imp().load.projection_suspended.get() || self.buffer_replacement_projection_suspended()
    }

    /// Whether load installation currently suppresses amplifying projections.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn load_projection_suspended_for_test(&self) -> bool {
        self.load_projection_suspended()
    }

    /// Process-wide scalar transient-load accounting for widget proofs.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn transient_load_admission_snapshot_for_test(
        &self,
    ) -> crate::model::file_load::FileLoadAdmissionSnapshot {
        load_runtime::snapshot_for_test()
    }

    /// Whether the process-wide load queue is polling for disposal capacity.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn transient_load_disposal_wakeup_armed_for_test(&self) -> bool {
        load_runtime::disposal_wakeup_armed_for_test()
    }

    /// Reset process-wide admission state between isolated widget cases.
    #[cfg(feature = "test-utils")]
    pub fn reset_transient_load_admission_for_test(&self) {
        load_runtime::reset_for_test();
    }

    /// Number of GTK slices completed by the newest chunked installation.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn load_installation_slice_count_for_test(&self) -> u64 {
        self.imp().load.installation_slice_count.get()
    }

    /// Whether a chunked installation currently retains decoded text.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn load_installation_active_for_test(&self) -> bool {
        self.imp().load.installation.borrow().is_some()
    }

    /// Current admitted payload charge retained by GTK installation.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn load_installation_weight_for_test(&self) -> Option<u64> {
        self.imp()
            .load
            .installation
            .borrow()
            .as_ref()
            .and_then(|session| {
                session
                    .borrow()
                    .permit
                    .as_ref()
                    .map(TransientLoadPermit::weight)
            })
    }

    /// The size classification from the last file load.
    #[must_use]
    pub fn size_check(&self) -> FileSizeCheck {
        self.imp().size_check.get()
    }

    /// Return the active load generation for stale-callback regression tests.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn load_generation_for_test(&self) -> u64 {
        self.imp().load_tracking.generation.get()
    }

    /// Return the active cancellation token so tests can prove token identity rotation.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn load_cancel_token_for_test(&self) -> Arc<AtomicBool> {
        self.imp().load_tracking.cancel_token.borrow().clone()
    }

    /// Apply a synthetic load result through the production stale-generation gate.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn apply_load_result_for_test(
        &self,
        load_generation: u64,
        result: Result<editor_io::LoadResult, EditorLoadError>,
    ) -> bool {
        if self.imp().load_tracking.generation.get() == load_generation {
            self.imp()
                .load_tracking
                .cancel_token
                .replace(Arc::new(AtomicBool::new(false)));
        }
        self.apply_load_result_if_current(load_generation, result, EditorLoadState::Failed)
    }

    /// Apply a synthetic reload failure that preserves the prior loaded buffer.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn apply_reload_error_for_test(
        &self,
        load_generation: u64,
        error: EditorLoadError,
    ) -> bool {
        if self.imp().load_tracking.generation.get() == load_generation {
            self.imp()
                .load_tracking
                .cancel_token
                .replace(Arc::new(AtomicBool::new(false)));
        }
        self.apply_load_result_if_current(load_generation, Err(error), EditorLoadState::Loaded)
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
        self.advance_local_history_path_generation();
        self.imp().file_path.replace(Some(path.to_path_buf()));
        self.imp().canonical_file_path.replace(canonical_path);
        self.imp().load_state.set(EditorLoadState::Loaded);
        if self.imp().size_check.get().syntax_enabled() {
            self.reapply_language();
        }
        self.schedule_minimap_refresh();
        self.notify_memory_policy_changed();
        self.refresh_accessibility_metadata();
    }

    /// Set a provisional path before an async load result is available.
    pub(crate) fn set_file_path_for_pending_load(&self, path: &Path) {
        self.advance_local_history_path_generation();
        self.imp().file_path.replace(Some(path.to_path_buf()));
        self.imp().canonical_file_path.borrow_mut().take();
        self.imp().file_size.set(None);
        self.imp().load_state.set(EditorLoadState::Loading);
        self.imp().latest_load_failed.set(false);
        if self.imp().size_check.get().syntax_enabled() {
            self.reapply_language();
        }
        self.schedule_minimap_refresh();
        self.notify_memory_policy_changed();
        self.refresh_accessibility_metadata();
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
}
