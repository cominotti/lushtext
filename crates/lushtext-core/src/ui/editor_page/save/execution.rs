// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination: the document-save workflow's execution job.
//!
//! Execution is the admitted half — capture, dispatch, write, accept. It begins
//! only after [`admission`](super::admission) has granted byte capacity, and it
//! is the only place the workflow touches document-sized text or the durable
//! write boundary.
//!
//! The stage body that actually writes the user's bytes is
//! [`write_snapshot_async`]. It is deliberately one function rather than several:
//! the worker closure it builds is a single ordered contract — format, write,
//! canonicalize, capture history, decide the text's disposition — and splitting
//! it would put a `spawn_blocking_then` boundary in the middle of a durable
//! write. The facade narrates it as one stage and names where control resumes.
//!
//! Two freshness guards operate here and they are different seams.
//! [`super::policy::QueuedSaveTicket`] decides whether a *queued* request may
//! still be admitted; [`SaveCompletionTicket`] decides whether a *completed*
//! worker result may still mutate the editor. Both stay.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk_lush_tasks::spawn_blocking_then;
use gtk4::subclass::prelude::ObjectSubclassIsExt;
use sourceview5::prelude::*;

use crate::model::encoding::{
    DocumentEncoding, FileHealthFinding, FileHealthFindingKind, FileHealthSeverity,
};
use crate::services::file_limits::FileSizeCheck;
use crate::services::{editor_io, filesystem::metadata as fs_metadata};
use crate::ui::buffer_snapshot;
use crate::ui::editor_page::{
    BufferReplacementOutcome, BufferReplacementRequest, BufferReplacementTicket,
    BufferReplacementWorkflow, EditorLoadState, EditorSaveError, LushtextEditorPage,
};

use super::admission::{self, SavePayloadPermit};
use super::policy::{
    QueuedSaveTicket, SaveCaptureMode, SaveTextDisposition, SaveWriteClassification,
    classify_saved_text, save_capture_mode,
};
use super::{SaveCallback, evidence};

/// Temporary view flags captured while a save makes the editor read-only.
///
/// The load workflow keeps its own equivalent inside `LoadInstallationState`.
/// They are deliberately not shared: each workflow owns the flags it suspended,
/// so neither has to reach into the other's residual state.
#[derive(Clone, Copy)]
pub(super) struct SaveViewInteractivity {
    editable: bool,
    cursor_visible: bool,
}

impl SaveViewInteractivity {
    fn suspend(editor: &LushtextEditorPage) -> Self {
        let view = editor.source_view();
        let captured = Self {
            editable: view.is_editable(),
            cursor_visible: view.is_cursor_visible(),
        };
        view.set_editable(false);
        view.set_cursor_visible(false);
        captured
    }

    fn restore(self, editor: &LushtextEditorPage) {
        editor.source_view().set_editable(self.editable);
        editor.source_view().set_cursor_visible(self.cursor_visible);
    }
}

/// Freshness of one dispatched write, checked before its result may be accepted.
#[derive(Clone, Copy)]
pub(super) struct SaveCompletionTicket {
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
            load_generation: editor.imp().load_tracking.generation.get(),
            edit_generation: editor.imp().local_history.edit_generation.get(),
            close_session_identity,
        }
    }

    fn is_current(self, editor: &LushtextEditorPage) -> bool {
        editor.is_saving()
            && editor.imp().save.generation.get() == self.save_generation
            && editor.imp().local_history.path_generation.get() == self.path_generation
            && editor.imp().load_tracking.generation.get() == self.load_generation
            && editor.imp().local_history.edit_generation.get() == self.edit_generation
            && self
                .close_session_identity
                .is_none_or(|identity| admission::close_save_session_is_current(editor, identity))
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

/// Begin one admitted save: revalidate, suspend the view, then capture text.
///
/// Control leaves this function in one of two ways. A buffer under the chunked
/// threshold is captured inline and continues straight into
/// [`write_snapshot_async`]. A buffer over it yields to the main loop, and
/// control resumes in the snapshot callback installed here.
pub(super) fn begin_admitted_save(
    editor: &LushtextEditorPage,
    ticket: QueuedSaveTicket,
    allow_lossy: bool,
    permit: SavePayloadPermit,
    callback: SaveCallback,
) {
    if !admission::queued_save_ticket_is_current(editor, &ticket) {
        admission::finish_queued_save_without_admission(editor, ticket.save_generation);
        callback(Err(EditorSaveError::SnapshotCancelled));
        return;
    }

    let path = ticket.path.clone();
    let close_session_identity = ticket.close_session_identity;

    editor.cancel_load();
    // Re-check the incomplete-installation gate **after** cancelling, not only
    // at the queue stage.
    //
    // `cancel_load` on a live bounded installation deliberately empties the
    // buffer in slices and sets `installation_incomplete`; the buffer at this
    // instant holds a half-installed decode of the file. The queue stage checks
    // this gate, but a load can start *between* queueing and admission — the
    // shared byte budget can hold a queued save for as long as another save is
    // writing, and no load entry point gates on `is_saving()`. Capturing text
    // here without re-checking would write that partial decode over the user's
    // file. Refuse exactly as the queue stage does, and let the retry come from
    // the user once the load settles.
    if editor.imp().load.installation_incomplete.get() {
        admission::finish_queued_save_without_admission(editor, ticket.save_generation);
        callback(Err(EditorSaveError::IncompleteLoadInstallation));
        return;
    }
    evidence::record_admitted_ticket(editor, ticket);
    let admitted = AdmittedSaveContext {
        ticket: SaveCompletionTicket::capture(editor, close_session_identity),
        allow_lossy,
        permit,
    };
    let restore_state = SaveViewInteractivity::suspend(editor);
    editor.refresh_accessibility_metadata();

    if buffer_snapshot::buffer_requires_chunked_snapshot(&editor.buffer()) {
        let editor_weak = editor.downgrade();
        let snapshot_callback = move |outcome| {
            let Some(editor) = editor_weak.upgrade() else {
                return;
            };
            editor.imp().save.snapshot.take();
            match outcome {
                buffer_snapshot::BufferSnapshotOutcome::Captured(text) => {
                    write_snapshot_async(&editor, path, text, restore_state, admitted, callback);
                }
                buffer_snapshot::BufferSnapshotOutcome::Cancelled(_)
                | buffer_snapshot::BufferSnapshotOutcome::ExceededLimit { .. } => {
                    finish_save_snapshot_without_write(&editor, restore_state, callback);
                }
            }
        };
        #[cfg(feature = "test-utils")]
        let snapshot = buffer_snapshot::snapshot_buffer_text_async_for_test(
            editor.buffer().upcast::<gtk4::TextBuffer>(),
            None,
            editor.imp().save.snapshot_test_mutation.take(),
            snapshot_callback,
        );
        #[cfg(not(feature = "test-utils"))]
        let snapshot =
            buffer_snapshot::snapshot_buffer_text_async(editor.buffer(), snapshot_callback);
        editor.imp().save.snapshot.replace(Some(snapshot));
        return;
    }

    let buffer = editor.buffer();
    let text = buffer_snapshot::BufferSnapshotPayload::direct(
        buffer_snapshot::snapshot_buffer_text_direct(&buffer),
    );
    write_snapshot_async(editor, path, text, restore_state, admitted, callback);
}

/// Restore the view after a chunked snapshot ends without coherent text.
fn finish_save_snapshot_without_write(
    editor: &LushtextEditorPage,
    restore_state: SaveViewInteractivity,
    callback: SaveCallback,
) {
    restore_state.restore(editor);
    evidence::clear_admitted_ticket(editor);
    editor.imp().save.inflight.set(false);
    editor.notify_memory_policy_changed();
    editor.refresh_accessibility_metadata();
    callback(Err(EditorSaveError::SnapshotCancelled));
}

/// Spawn the background write and restore any temporary view state afterwards.
///
/// Control leaves the GTK thread inside `spawn_blocking_then` and resumes in the
/// completion closure below. When save formatting rewrote the text, control
/// inverts a second time through the bounded buffer-replacement workflow, and
/// resumes in the replacement's terminal callback — which is where the tab is
/// finally marked clean, because the saved bytes and the live buffer must agree
/// first.
fn write_snapshot_async(
    editor: &LushtextEditorPage,
    path: PathBuf,
    text: buffer_snapshot::BufferSnapshotPayload,
    restore_view_state: SaveViewInteractivity,
    admitted: AdmittedSaveContext,
    callback: SaveCallback,
) {
    let AdmittedSaveContext {
        ticket,
        allow_lossy,
        permit,
    } = admitted;
    editor.prepare_local_history_for_save();
    let was_modified_before_save = editor.buffer().is_modified();
    let metadata = editor.document_encoding_state();
    let formatting_overrides = editor.formatting_overrides();
    let history_availability = editor.live_local_history_availability();

    spawn_blocking_then(
        editor.clone(),
        move || {
            let text = text.into_guarded_string_on_worker();
            let formatted_text = editor_io::apply_save_formatting_overrides_borrowed(
                text.as_str(),
                formatting_overrides,
            );
            let formatting_changed = formatted_text.as_ref() != text.as_str();
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

            let disposition = classify_saved_text(
                formatting_changed,
                history_availability.allows_automatic_capture(),
            );
            let retain_formatted_as_clean = matches!(
                disposition,
                SaveTextDisposition::MirrorFormattedIntoBuffer {
                    retain_as_clean: true
                }
            );
            let (clean_text, formatted_text) = match disposition {
                SaveTextDisposition::MirrorFormattedIntoBuffer { .. } => {
                    let formatted_text = formatted_text.into_owned();
                    (
                        None,
                        Some(text.map_preserving_reservation(|_| formatted_text)),
                    )
                }
                SaveTextDisposition::RetainCapturedAsClean => {
                    drop(formatted_text);
                    (Some(text), None)
                }
                SaveTextDisposition::RetireCaptured => {
                    drop(formatted_text);
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
        move |editor, result| {
            // A stale completion publishes nothing, however the write ended.
            if !ticket.is_current(&editor) {
                finish_save_formatting_without_acceptance(&editor, restore_view_state, callback);
                return;
            }
            match result {
                Ok(mut outcome) => {
                    let Some(formatted_text) = outcome.formatted_text.take() else {
                        evidence::record_formatting_rewrite(&editor, false);
                        finish_accepted_save(&editor, outcome, restore_view_state, None, callback);
                        return;
                    };
                    evidence::record_formatting_rewrite(&editor, true);
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
                        BufferReplacementRequest::new_guarded_returning_body_on_complete(
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
                                        evidence::record_mirror_back_completed(&editor);
                                        finish_accepted_save(
                                            &editor,
                                            outcome,
                                            restore_view_state,
                                            Some(cursor_offset),
                                            callback,
                                        );
                                    }
                                    _ => finish_save_formatting_without_acceptance(
                                        &editor,
                                        restore_view_state,
                                        callback,
                                    ),
                                }
                            },
                            move |body| {
                                completed_body_for_request.replace(Some(body));
                            },
                        ),
                    );
                }
                Err(error) => {
                    evidence::record_write_classification(&editor, classify_write_error(&error));
                    restore_view_after_save(&editor, restore_view_state);
                    editor.buffer().set_modified(was_modified_before_save);
                    editor.complete_local_history_after_save_failure();
                    editor.refresh_accessibility_metadata();
                    callback(Err(error));
                }
            }
        },
    );
}

/// Classify one failed write against the durable-write contract.
///
/// `BeforeRename` means the previous bytes are intact and the document must stay
/// modified. `AfterRename` means the new bytes are on disk but the directory
/// sync did not complete, which is a durability warning and never a lost save.
const fn classify_write_error(error: &EditorSaveError) -> SaveWriteClassification {
    match error {
        EditorSaveError::DurabilityUnconfirmed { .. } => {
            SaveWriteClassification::DurabilityUnconfirmed
        }
        _ => SaveWriteClassification::FailedBeforeRename,
    }
}

fn restore_view_after_save(editor: &LushtextEditorPage, restore: SaveViewInteractivity) {
    restore.restore(editor);
    evidence::clear_admitted_ticket(editor);
    editor.imp().save.inflight.set(false);
    editor.notify_memory_policy_changed();
}

fn finish_save_formatting_without_acceptance(
    editor: &LushtextEditorPage,
    restore: SaveViewInteractivity,
    callback: SaveCallback,
) {
    restore_view_after_save(editor, restore);
    editor.buffer().set_modified(true);
    editor.complete_local_history_after_save_failure();
    editor.refresh_accessibility_metadata();
    callback(Err(EditorSaveError::SnapshotCancelled));
}

fn finish_accepted_save(
    editor: &LushtextEditorPage,
    mut outcome: SaveWriteOutcome,
    restore: SaveViewInteractivity,
    cursor_offset: Option<i32>,
    callback: SaveCallback,
) {
    restore_view_after_save(editor, restore);
    let buffer = editor.buffer();
    if let Some(cursor_offset) = cursor_offset {
        let mut iter = buffer.start_iter();
        iter.forward_chars(cursor_offset.min(buffer.end_iter().offset()));
        buffer.place_cursor(&iter);
    }
    buffer.set_modified(false);
    editor.imp().file_size.set(Some(outcome.size));
    editor
        .imp()
        .size_check
        .set(FileSizeCheck::classify(outcome.size));
    editor.imp().load_state.set(EditorLoadState::Loaded);
    editor.imp().latest_load_failed.set(false);
    let mut state = editor.document_encoding_state();
    state.opened_encoding = state.save_encoding;
    state.detected_line_ending = state.save_line_ending;
    state.decode_confidence = crate::model::encoding::DecodeConfidence::Exact;
    editor.set_document_encoding_state(state);
    let has_bom = state.save_encoding.writes_bom();
    editor.set_has_bom(has_bom);
    editor
        .imp()
        .canonical_file_path
        .replace(outcome.canonical_path);
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
                body: "This document will be saved with a UTF-8 byte-order mark.".to_string(),
            },
        );
    }
    editor.set_file_health(findings);
    editor.notify_memory_policy_changed();
    editor.imp().monitor.last_known_mtime.set(outcome.mtime);
    editor.clear_modified_line_marks();
    editor.refresh_minimap();
    editor.complete_local_history_after_save_success(outcome.clean_text.take());
    editor.refresh_accessibility_metadata();
    evidence::record_write_classification(editor, SaveWriteClassification::Accepted);
    // Close-save progression may synchronously queue the next editor. The
    // consumed payload must leave shared accounting before that callback
    // can trigger another admission pass.
    drop(outcome.permit.take());
    callback(Ok(()));
}

/// Whether this editor's save capture is yielding through the main loop.
pub(super) fn capture_in_flight(editor: &LushtextEditorPage) -> bool {
    editor.imp().save.snapshot.borrow().is_some()
}

/// Which capture mode this editor's buffer would take right now.
///
/// The threshold belongs to the cross-cutting buffer-snapshot workflow; this
/// only names its current answer for the save workflow's observers. While a save
/// is in flight the view is read-only, so the classification cannot drift from
/// the mode the in-flight capture actually took.
///
/// Reading evidence must not require the workflow to be in a particular stage,
/// and a disposed page is a legitimate observation point — a teardown test
/// asks precisely whether the save released its permit. GTK4 clears template
/// children in `dispose()`, before Rust's `Drop`, so the source view is read
/// through `try_get()` here rather than the panicking accessor. A page with no
/// buffer left has nothing to capture, which is `Direct`.
pub(super) fn capture_mode(editor: &LushtextEditorPage) -> SaveCaptureMode {
    let Some(view) = editor.imp().source_view.try_get() else {
        return SaveCaptureMode::Direct;
    };
    save_capture_mode(buffer_snapshot::buffer_requires_chunked_snapshot(
        &view.buffer(),
    ))
}

#[cfg(feature = "test-utils")]
impl LushtextEditorPage {
    /// Pause the next chunked save snapshot after its first captured slice.
    ///
    /// Preserved actuation seam: the pause point is inside a main-loop slice
    /// that no user gesture can land on deterministically.
    pub fn pause_next_save_snapshot_for_test(&self) {
        self.imp().save.snapshot_test_mutation.set(Some(
            buffer_snapshot::BufferSnapshotTestMutation {
                trigger: buffer_snapshot::BufferSnapshotTestTrigger::AfterSlice(1),
                edit: buffer_snapshot::BufferSnapshotTestEdit::Pause,
            },
        ));
    }

    /// Resume a save snapshot paused by [`Self::pause_next_save_snapshot_for_test`].
    ///
    /// Preserved actuation seam, paired with the pause above.
    pub fn resume_save_snapshot_for_test(&self) {
        if let Some(snapshot) = self.imp().save.snapshot.borrow().as_ref() {
            snapshot.resume_for_test();
        }
    }
}
