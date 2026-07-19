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

use gtk_lush_tasks::spawn_blocking_then;
use gtk4;
use gtk4::glib;
use gtk4::subclass::prelude::ObjectSubclassIsExt;
use sourceview5::prelude::*;

use crate::model::encoding::{
    DocumentEncoding, FileHealthFinding, FileHealthFindingKind, FileHealthSeverity,
};
use crate::model::file_load::{SYNCHRONOUS_INSTALL_THRESHOLD_BYTES, next_install_boundary};
use crate::model::save_admission::SaveAdmissionPriority;
use crate::services::file_limits::FileSizeCheck;
use crate::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use crate::services::{editor_io, filesystem::metadata as fs_metadata};
use crate::ui::buffer_snapshot;

use super::load_runtime::{self, TransientLoadPermit};
use super::save_runtime::{self, SavePayloadPermit, SaveSubmission};
use super::{
    BufferReplacementOutcome, BufferReplacementRequest, BufferReplacementTicket,
    BufferReplacementWorkflow, EditorLoadState, EditorSaveError, LushtextEditorPage,
};
use editor_io::EditorLoadError;

pub(super) type SaveCallback = Box<dyn FnOnce(Result<(), EditorSaveError>)>;

/// Temporary view flags captured while chunked snapshotting makes the editor read-only.
#[derive(Clone, Copy)]
struct ViewInteractivityState {
    editable: bool,
    cursor_visible: bool,
}

#[derive(Clone, Copy)]
struct SaveCompletionTicket {
    save_generation: u64,
    path_generation: u64,
    load_generation: u64,
    edit_generation: u64,
    close_session_identity: Option<u64>,
}

impl SaveCompletionTicket {
    fn capture(editor: &LushtextEditorPage, close_session_identity: Option<u64>) -> Self {
        Self {
            save_generation: editor.imp().save.generation.get(),
            path_generation: editor.imp().local_history.path_generation.get(),
            load_generation: editor.imp().load_generation.get(),
            edit_generation: editor.imp().local_history.edit_generation.get(),
            close_session_identity,
        }
    }

    fn is_current(self, editor: &LushtextEditorPage) -> bool {
        editor.is_saving()
            && editor.imp().save.generation.get() == self.save_generation
            && editor.imp().local_history.path_generation.get() == self.path_generation
            && editor.imp().load_generation.get() == self.load_generation
            && editor.imp().local_history.edit_generation.get() == self.edit_generation
            && self.close_session_identity.is_none_or(|identity| {
                editor
                    .root()
                    .and_then(|root| root.downcast::<crate::ui::window::LushtextWindow>().ok())
                    .is_some_and(|window| window.close_save_session_is_current(identity))
            })
    }
}

/// Request-bound state that must survive snapshotting until the write consumes it.
struct AdmittedSaveContext {
    ticket: SaveCompletionTicket,
    allow_lossy: bool,
    permit: SavePayloadPermit,
}

struct SaveWriteOutcome {
    size: u64,
    mtime: Option<u64>,
    canonical_path: Option<PathBuf>,
    clean_text: Option<crate::ui::plain_disposal::DisposalOwned<String>>,
    formatted_text: Option<crate::ui::plain_disposal::DisposalOwned<String>>,
    retain_formatted_as_clean: bool,
    permit: Option<SavePayloadPermit>,
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
    loaded: Option<editor_io::LoadResult>,
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
        && (editor.imp().load_generation.get() != generation
            || editor.imp().cancel_token.borrow().load(Ordering::Acquire))
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
        || editor.imp().load_generation.get() != generation
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
        self.imp().cancel_token.replace(cancel.clone());
        let load_generation = self.imp().load_generation.get().wrapping_add(1);
        self.imp().load_generation.set(load_generation);

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
        self.apply_load_outcome(load_generation, result, error_state, None)
    }

    pub(super) fn accept_admitted_load_outcome(
        &self,
        load_generation: u64,
        result: Result<editor_io::LoadResult, EditorLoadError>,
        error_state: EditorLoadState,
        permit: TransientLoadPermit,
    ) {
        let _ = self.apply_load_outcome(load_generation, result, error_state, Some(permit));
    }

    fn apply_load_outcome(
        &self,
        load_generation: u64,
        result: Result<editor_io::LoadResult, EditorLoadError>,
        error_state: EditorLoadState,
        permit: Option<TransientLoadPermit>,
    ) -> bool {
        if self.imp().load_generation.get() != load_generation
            || self.imp().cancel_token.borrow().load(Ordering::Acquire)
        {
            return false;
        }
        match result {
            Ok(loaded) => {
                if self.requires_chunked_install(loaded.content.len()) {
                    self.start_chunked_install(load_generation, loaded, permit);
                } else {
                    self.install_loaded_direct(loaded, permit);
                }
            }
            Err(EditorLoadError::Cancelled) => {}
            Err(error) => {
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
        }
        true
    }

    fn install_loaded_direct(
        &self,
        loaded: editor_io::LoadResult,
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
        loaded: editor_io::LoadResult,
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
        loaded: editor_io::LoadResult,
        restore: LoadInstallationState,
    ) {
        let editor_io::LoadResult {
            content,
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
        self.imp().evicted.set(false);
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
        self.seed_local_history_from_loaded_content(content);
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
        for callback in self.imp().load.file_loaded_callbacks.borrow().iter() {
            callback();
        }
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
    #[cfg(feature = "test-utils")]
    pub fn apply_loaded_content_for_test(&self, content: &str, reported_size: u64) {
        let size_check = FileSizeCheck::classify(reported_size);
        self.install_loaded_direct(
            editor_io::LoadResult {
                content: content.to_string(),
                size: reported_size,
                size_check,
                canonical_path: self.canonical_file_path(),
                mtime: self.imp().monitor.last_known_mtime.get(),
                encoding_state: self.document_encoding_state(),
                has_bom: self.has_bom(),
                file_health: self.file_health(),
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
            .load_generation
            .set(self.imp().load_generation.get().wrapping_add(1));
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
            .load_generation
            .set(self.imp().load_generation.get().wrapping_add(1));
    }

    fn cancel_noninstall_load_resources(&self) {
        self.imp()
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
        self.imp().load_generation.get() == generation
            && Arc::ptr_eq(&self.imp().cancel_token.borrow(), cancel)
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

    /// Reset process-wide admission state between isolated widget cases.
    #[cfg(feature = "test-utils")]
    pub fn reset_transient_load_admission_for_test(&self) {
        load_runtime::reset_for_test();
    }

    /// Process-wide scalar transient-save accounting for widget proofs.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn transient_save_admission_snapshot_for_test(
        &self,
    ) -> crate::model::save_admission::SaveAdmissionSnapshot {
        save_runtime::snapshot_for_test()
    }

    /// Reset process-wide save admission state between isolated widget cases.
    #[cfg(feature = "test-utils")]
    pub fn reset_transient_save_admission_for_test(&self) {
        save_runtime::reset_for_test();
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

    /// Test-only seam for the live-buffer snapshot policy.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn save_uses_chunked_snapshot_for_test(&self) -> bool {
        self.live_buffer_requires_chunked_snapshot()
    }

    /// Whether save currently owns a chunked snapshot lifecycle.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn save_snapshot_inflight_for_test(&self) -> bool {
        self.imp().save.snapshot.borrow().is_some()
    }

    /// Pause the next chunked save snapshot after its first captured slice.
    #[cfg(feature = "test-utils")]
    pub fn pause_next_save_snapshot_for_test(&self) {
        self.imp().save.snapshot_test_mutation.set(Some(
            buffer_snapshot::BufferSnapshotTestMutation {
                trigger: buffer_snapshot::BufferSnapshotTestTrigger::AfterSlice(1),
                edit: buffer_snapshot::BufferSnapshotTestEdit::Pause,
            },
        ));
    }

    /// Resume a save snapshot paused by [`Self::pause_next_save_snapshot_for_test`].
    #[cfg(feature = "test-utils")]
    pub fn resume_save_snapshot_for_test(&self) {
        if let Some(snapshot) = self.imp().save.snapshot.borrow().as_ref() {
            snapshot.resume_for_test();
        }
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
        result: Result<editor_io::LoadResult, EditorLoadError>,
    ) -> bool {
        if self.imp().load_generation.get() == load_generation {
            self.imp()
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
        if self.imp().load_generation.get() == load_generation {
            self.imp()
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

    /// Save the file asynchronously on a background thread.
    pub fn save_file_async<F: FnOnce(Result<(), EditorSaveError>) + 'static>(&self, callback: F) {
        let Some(path) = self.imp().file_path.borrow().clone() else {
            callback(Err(EditorSaveError::NoPath));
            return;
        };
        self.queue_save_request(path, false, SaveAdmissionPriority::Ordinary, None, callback);
    }

    /// Save the current buffer to an explicit path without mutating the tracked path first.
    ///
    /// Save As may race the original file load after the user has already edited
    /// the visible buffer. A queued or pre-install load is cancelled before the
    /// snapshot so its stale result cannot replace the newly saved destination.
    pub(crate) fn save_file_async_to_path<F: FnOnce(Result<(), EditorSaveError>) + 'static>(
        &self,
        path: PathBuf,
        callback: F,
    ) {
        self.queue_save_request(path, true, SaveAdmissionPriority::Ordinary, None, callback);
    }

    /// Queue a file-backed save that gates the current close session.
    pub(crate) fn save_file_async_for_close<F: FnOnce(Result<(), EditorSaveError>) + 'static>(
        &self,
        close_session_identity: u64,
        callback: F,
    ) {
        let Some(path) = self.imp().file_path.borrow().clone() else {
            callback(Err(EditorSaveError::NoPath));
            return;
        };
        self.queue_save_request(
            path,
            false,
            SaveAdmissionPriority::Close,
            Some(close_session_identity),
            callback,
        );
    }

    fn queue_save_request<F: FnOnce(Result<(), EditorSaveError>) + 'static>(
        &self,
        path: PathBuf,
        cancel_pending_load: bool,
        priority: SaveAdmissionPriority,
        close_session_identity: Option<u64>,
        callback: F,
    ) {
        let callback: SaveCallback = Box::new(callback);
        if self.imp().load_state.get() == EditorLoadState::Loading {
            if cancel_pending_load {
                self.cancel_load();
            } else {
                callback(Err(EditorSaveError::LoadInProgress));
                return;
            }
        }
        if self.imp().load.installation_incomplete.get() {
            callback(Err(EditorSaveError::IncompleteLoadInstallation));
            return;
        }
        if self.buffer_replacement_in_progress() {
            callback(Err(EditorSaveError::LoadInProgress));
            return;
        }
        if self.imp().save.inflight.get() {
            callback(Err(EditorSaveError::SaveInProgress));
            return;
        }

        if cancel_pending_load {
            self.cancel_load();
        }
        // Publish queued ownership before yielding so duplicate saves and an
        // already-planned eviction pass revalidate this page as protected.
        let generation = self.imp().save.generation.get().wrapping_add(1);
        self.imp().save.generation.set(generation);
        self.imp().save.inflight.set(true);
        self.notify_memory_policy_changed();

        // Consent belongs to this generation: cancellation must discard it
        // instead of allowing unrelated later content to save lossily.
        let allow_lossy = self.take_lossy_save_once();

        save_runtime::submit(
            self,
            generation,
            SaveSubmission {
                path,
                cancel_pending_load,
                priority,
                close_session_identity,
                allow_lossy,
                callback,
            },
        );
    }

    pub(super) fn queued_save_is_current(
        &self,
        generation: u64,
        path: &Path,
        explicit_destination: bool,
        required_modified: bool,
        close_session_identity: Option<u64>,
    ) -> bool {
        if !self.is_saving()
            || self.imp().save.generation.get() != generation
            || (required_modified && !self.is_modified())
            || (!explicit_destination && self.file_path().as_deref() != Some(path))
        {
            return false;
        }

        close_session_identity.is_none_or(|identity| {
            self.root()
                .and_then(|root| root.downcast::<crate::ui::window::LushtextWindow>().ok())
                .is_some_and(|window| window.close_save_session_is_current(identity))
        })
    }

    pub(super) fn finish_queued_save_without_admission(&self, generation: u64) {
        if self.imp().save.generation.get() != generation {
            return;
        }
        self.imp().save.inflight.set(false);
        self.notify_memory_policy_changed();
        self.refresh_accessibility_metadata();
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "The admitted boundary keeps every compact freshness field explicit before any document payload is captured"
    )]
    pub(super) fn begin_admitted_save(
        &self,
        generation: u64,
        path: PathBuf,
        cancel_pending_load: bool,
        required_modified: bool,
        close_session_identity: Option<u64>,
        allow_lossy: bool,
        permit: SavePayloadPermit,
        callback: SaveCallback,
    ) {
        if !self.queued_save_is_current(
            generation,
            &path,
            cancel_pending_load,
            required_modified,
            close_session_identity,
        ) {
            self.finish_queued_save_without_admission(generation);
            callback(Err(EditorSaveError::SnapshotCancelled));
            return;
        }

        self.cancel_load();
        let ticket = SaveCompletionTicket::capture(self, close_session_identity);
        let admitted = AdmittedSaveContext {
            ticket,
            allow_lossy,
            permit,
        };
        let view = self.source_view().clone();
        let restore_state = ViewInteractivityState {
            editable: view.is_editable(),
            cursor_visible: view.is_cursor_visible(),
        };
        view.set_editable(false);
        view.set_cursor_visible(false);
        self.refresh_accessibility_metadata();

        if self.live_buffer_requires_chunked_snapshot() {
            let editor_weak = self.downgrade();
            let snapshot_callback = move |outcome| {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                editor.imp().save.snapshot.take();
                match outcome {
                    buffer_snapshot::BufferSnapshotOutcome::Captured(text) => {
                        editor.write_snapshot_async(path, text, restore_state, admitted, callback);
                    }
                    buffer_snapshot::BufferSnapshotOutcome::Cancelled(_)
                    | buffer_snapshot::BufferSnapshotOutcome::ExceededLimit { .. } => {
                        editor.finish_save_snapshot_without_write(restore_state, callback);
                    }
                }
            };
            #[cfg(feature = "test-utils")]
            let snapshot = buffer_snapshot::snapshot_buffer_text_async_for_test(
                self.buffer().upcast::<gtk4::TextBuffer>(),
                None,
                self.imp().save.snapshot_test_mutation.take(),
                snapshot_callback,
            );
            #[cfg(not(feature = "test-utils"))]
            let snapshot =
                buffer_snapshot::snapshot_buffer_text_async(self.buffer(), snapshot_callback);
            self.imp().save.snapshot.replace(Some(snapshot));
            return;
        }

        let buffer = self.buffer();
        let text = buffer_snapshot::BufferSnapshotPayload::direct(
            buffer_snapshot::snapshot_buffer_text_direct(&buffer),
        );
        self.write_snapshot_async(path, text, restore_state, admitted, callback);
    }

    /// Restore the view after a chunked snapshot ends without coherent text.
    fn finish_save_snapshot_without_write(
        &self,
        restore_state: ViewInteractivityState,
        callback: SaveCallback,
    ) {
        self.source_view().set_editable(restore_state.editable);
        self.source_view()
            .set_cursor_visible(restore_state.cursor_visible);
        self.imp().save.inflight.set(false);
        self.notify_memory_policy_changed();
        self.refresh_accessibility_metadata();
        callback(Err(EditorSaveError::SnapshotCancelled));
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
        text: buffer_snapshot::BufferSnapshotPayload,
        restore_view_state: ViewInteractivityState,
        admitted: AdmittedSaveContext,
        callback: SaveCallback,
    ) {
        let AdmittedSaveContext {
            ticket,
            allow_lossy,
            permit,
        } = admitted;
        self.prepare_local_history_for_save();
        let was_modified_before_save = self.buffer().is_modified();
        let metadata = self.document_encoding_state();
        let formatting_overrides = self.formatting_overrides();
        let history_availability = self.live_local_history_availability();

        spawn_blocking_then(
            self.clone(),
            move || {
                let text = text.into_guarded_string_on_worker();
                let formatted_text = editor_io::apply_save_formatting_overrides_borrowed(
                    text.as_str(),
                    formatting_overrides,
                );
                let should_update_buffer = formatted_text.as_ref() != text.as_str();
                let write_result = editor_io::write_document_to_path(
                    &path,
                    formatted_text.as_ref(),
                    metadata.save_encoding,
                    metadata.save_line_ending,
                    allow_lossy,
                )?;
                let size = write_result.bytes_written;
                let mtime = write_result.modified_at_secs;
                let canonical_path = fs_metadata::canonical_path(&path).ok();

                if history_availability.allows_browsing() {
                    let data_dir = crate::services::json_store::data_dir();
                    if let Err(error) = crate::services::local_history_service::capture_snapshot_for_path(
                        &data_dir,
                        &path,
                        formatted_text.as_ref(),
                        crate::model::local_history::LocalHistorySnapshotOrigin::Save,
                        crate::services::local_history_service::LocalHistoryCapturePolicy::DeduplicateLatest,
                    ) {
                        tracing::warn!(
                            "Saved {}, but local-history snapshot capture failed: {error}",
                            path.display()
                        );
                    }
                }

                let retain_formatted_as_clean =
                    should_update_buffer && history_availability.allows_automatic_capture();
                let (clean_text, formatted_text) = if should_update_buffer {
                    let formatted_text = formatted_text.into_owned();
                    (
                        None,
                        Some(text.map_preserving_reservation(|_| formatted_text)),
                    )
                } else {
                    drop(formatted_text);
                    if history_availability.allows_automatic_capture() {
                        (Some(text), None)
                    } else {
                        drop(text.into_inner_on_worker());
                        (None, None)
                    }
                };
                Ok::<_, EditorSaveError>(SaveWriteOutcome {
                    size,
                    mtime,
                    canonical_path,
                    clean_text,
                    formatted_text,
                    retain_formatted_as_clean,
                    permit: Some(permit),
                })
            },
            move |editor, result| match result {
                Ok(mut outcome) => {
                    if !ticket.is_current(&editor) {
                        editor.finish_save_formatting_without_acceptance(
                            restore_view_state,
                            callback,
                        );
                        return;
                    }
                    let Some(formatted_text) = outcome.formatted_text.take() else {
                        editor.finish_accepted_save(outcome, restore_view_state, None, callback);
                        return;
                    };
                    let cursor_offset = editor
                        .buffer()
                        .iter_at_mark(&editor.buffer().get_insert())
                        .offset();
                    let freshness_editor = editor.downgrade();
                    let terminal_editor = editor.downgrade();
                    let completed_body = Rc::new(RefCell::new(None));
                    let completed_body_for_request = Rc::clone(&completed_body);
                    let completed_body_for_terminal = Rc::clone(&completed_body);
                    editor.replace_buffer_bounded(
                        BufferReplacementRequest::new_guarded(
                            BufferReplacementTicket {
                                workflow: BufferReplacementWorkflow::SaveFormatting,
                                generation: ticket.save_generation,
                            },
                            formatted_text,
                            move |_| {
                                freshness_editor
                                    .upgrade()
                                    .is_some_and(|editor| ticket.is_current(&editor))
                            },
                            move |replacement| {
                                let Some(editor) = terminal_editor.upgrade() else {
                                    return;
                                };
                                match replacement {
                                    BufferReplacementOutcome::Complete {
                                        ticket:
                                            BufferReplacementTicket {
                                                workflow: BufferReplacementWorkflow::SaveFormatting,
                                                generation,
                                            },
                                        ..
                                    } if generation == ticket.save_generation
                                        && ticket.is_current(&editor) =>
                                    {
                                        if outcome.retain_formatted_as_clean {
                                            outcome.clean_text =
                                                completed_body_for_terminal.borrow_mut().take();
                                        }
                                        editor.finish_accepted_save(
                                            outcome,
                                            restore_view_state,
                                            Some(cursor_offset),
                                            callback,
                                        );
                                    }
                                    _ => editor.finish_save_formatting_without_acceptance(
                                        restore_view_state,
                                        callback,
                                    ),
                                }
                            },
                        )
                        .return_guarded_body_on_complete(move |body| {
                            completed_body_for_request.replace(Some(body));
                        }),
                    );
                }
                Err(error) => {
                    if !ticket.is_current(&editor) {
                        editor.finish_save_formatting_without_acceptance(
                            restore_view_state,
                            callback,
                        );
                        return;
                    }
                    editor.restore_view_after_save(restore_view_state);
                    editor.buffer().set_modified(was_modified_before_save);
                    editor.complete_local_history_after_save_failure();
                    editor.refresh_accessibility_metadata();
                    callback(Err(error));
                }
            },
        );
    }

    fn restore_view_after_save(&self, restore: ViewInteractivityState) {
        self.source_view().set_editable(restore.editable);
        self.source_view()
            .set_cursor_visible(restore.cursor_visible);
        self.imp().save.inflight.set(false);
        self.notify_memory_policy_changed();
    }

    fn finish_save_formatting_without_acceptance(
        &self,
        restore: ViewInteractivityState,
        callback: SaveCallback,
    ) {
        self.restore_view_after_save(restore);
        self.buffer().set_modified(true);
        self.complete_local_history_after_save_failure();
        self.refresh_accessibility_metadata();
        callback(Err(EditorSaveError::SnapshotCancelled));
    }

    fn finish_accepted_save(
        &self,
        mut outcome: SaveWriteOutcome,
        restore: ViewInteractivityState,
        cursor_offset: Option<i32>,
        callback: SaveCallback,
    ) {
        self.restore_view_after_save(restore);
        let buffer = self.buffer();
        if let Some(cursor_offset) = cursor_offset {
            let mut iter = buffer.start_iter();
            iter.forward_chars(cursor_offset.min(buffer.end_iter().offset()));
            buffer.place_cursor(&iter);
        }
        buffer.set_modified(false);
        self.imp().file_size.set(Some(outcome.size));
        self.imp()
            .size_check
            .set(FileSizeCheck::classify(outcome.size));
        self.imp().load_state.set(EditorLoadState::Loaded);
        self.imp().latest_load_failed.set(false);
        let mut state = self.document_encoding_state();
        state.opened_encoding = state.save_encoding;
        state.detected_line_ending = state.save_line_ending;
        state.decode_confidence = crate::model::encoding::DecodeConfidence::Exact;
        self.set_document_encoding_state(state);
        let has_bom = state.save_encoding.writes_bom();
        self.set_has_bom(has_bom);
        self.imp()
            .canonical_file_path
            .replace(outcome.canonical_path);
        let mut findings: Vec<FileHealthFinding> = self
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
                    body: "This document will be saved with a UTF-8 byte-order mark.".to_string(),
                },
            );
        }
        self.set_file_health(findings);
        self.notify_memory_policy_changed();
        self.imp().monitor.last_known_mtime.set(outcome.mtime);
        self.clear_modified_line_marks();
        self.refresh_minimap();
        self.complete_local_history_after_save_success(outcome.clean_text.take());
        self.refresh_accessibility_metadata();
        // Close-save progression may synchronously queue the next editor. The
        // consumed payload must leave shared accounting before that callback
        // can trigger another admission pass.
        drop(outcome.permit.take());
        callback(Ok(()));
    }

    /// Decide whether save snapshotting should yield through the main loop.
    fn live_buffer_requires_chunked_snapshot(&self) -> bool {
        buffer_snapshot::buffer_requires_chunked_snapshot(&self.buffer())
    }
}
