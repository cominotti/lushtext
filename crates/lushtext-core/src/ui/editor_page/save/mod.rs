// SPDX-License-Identifier: GPL-3.0-or-later

//! # The document-save workflow
//!
//! What happens when the user presses Ctrl+S, picks a Save As destination, or
//! confirms "Save" in a close-with-changes dialog. One workflow, one stage
//! order, five points where control leaves and comes back.
//!
//! This directory is a **per-workflow role home**: `ui/editor_page/` hosts eight
//! workflows and the role file names `policy.rs` and `evidence.rs` are fixed at
//! one each per workflow, so this workflow keeps its roles in a subdirectory
//! whose `mod.rs` — this file — is the facade. A prefixed `save_policy.rs` was
//! not available: pure policy is reached by the mutation scope through
//! `ui/**/policy.rs`, and a prefixed name would leave that scope.
//!
//! ## Role table
//!
//! | Module | Role | Owns |
//! | --- | --- | --- |
//! | this file | narrative facade | the three entry points, the destination-adoption step, and this narration |
//! | `admission` | coordination | the process-wide coordinator, the queue stage, the idle drain, exactly-once charge release |
//! | `execution` | coordination | view suspension, buffer capture, the worker write, completion acceptance |
//! | [`policy`] | pure policy | byte-weighted admission accounting, the `QueuedSaveTicket` seam and its predicate, the saved-text disposition, the write classification |
//! | [`evidence`] | evidence | [`SaveEvidence`], the one typed value observers read |
//!
//! ## Stages
//!
//! 1. **Accept the request.** [`LushtextEditorPage::save_file_async`],
//!    `save_file_async_to_path`,
//!    and
//!    `save_file_async_for_close`
//!    are the only ways in. Each names the user's intent and differs only in
//!    where the destination comes from and what gates the save. Delegates to
//!    `admission::queue_save_request`.
//! 2. **Queue under compact ownership.** The request is refused outright if the
//!    editor cannot honour it — a load in progress with no explicit destination,
//!    an incomplete load installation, a buffer replacement, or an existing
//!    save. Otherwise the save generation advances, the editor is marked saving,
//!    and one `QueuedSaveTicket` is built **once** and carried from here on.
//!    No document text has been copied yet: only compact scalars are queued.
//! 3. **Admit under a shared byte budget.** `admission::drain` retires queued
//!    requests whose ticket no longer describes their editor, then admits as
//!    many as the shared payload budget allows.
//! 4. **Capture the buffer.** `execution::begin_admitted_save` revalidates the
//!    ticket, suspends the view, and copies the text — in one turn, or in
//!    main-loop slices for a buffer over the shared chunked threshold.
//! 5. **Write durably on a worker.** Save formatting is applied, the bytes are
//!    written through the atomic temp-then-rename boundary, the canonical path
//!    is resolved, and a local-history snapshot is captured.
//! 6. **Accept, mirror back, and settle.** A stale completion publishes nothing.
//!    A fresh one marks the tab clean and adopts the written size, encoding,
//!    mtime, and canonical identity — except when save formatting rewrote the
//!    text, in which case the buffer is mirrored back **first**, because a clean
//!    tab whose visible text differs from disk is a lost edit.
//!
//!    A **failed** write is classified here rather than flattened, by
//!    `execution::classify_write_error` into
//!    [`policy::SaveWriteClassification`], and the distinction is load-bearing:
//!    a failure *before* the rename leaves the previous bytes intact and the tab
//!    modified, while a failure *after* it means the new bytes are on disk and
//!    only their durability is unconfirmed. Reporting the second as a lost save
//!    would tell the user to redo work that is already written.
//!
//! ## Where control leaves, and where it comes back
//!
//! - **Stage 2 → 3, idle drain.** Queueing schedules a `glib::idle_add_local_once`
//!   and returns. Control resumes in `admission::drain`.
//! - **Stage 4, chunked capture.** A buffer over the threshold yields between
//!   slices. Control resumes in the snapshot callback installed by
//!   `begin_admitted_save`, which either continues to stage 5 or finishes
//!   without a write.
//! - **Stage 5 → 6, worker write.** `spawn_blocking_then` leaves the GTK thread.
//!   Control resumes in the completion closure inside
//!   `execution::write_snapshot_async`.
//! - **Stage 6, mirror-back.** When formatting rewrote the text, acceptance
//!   inverts a second time through the bounded buffer-replacement workflow.
//!   Control resumes in that replacement's terminal callback, which is the only
//!   place the tab is marked clean on this path.
//! - **Charge release.** Dropping a `SavePayloadPermit` posts
//!   `glib::idle_add_once`; control resumes in `admission::release_on_main`,
//!   which re-arms the drain. Every terminal converges here, so cancellation,
//!   worker failure, stale completion, and success all release exactly once.
//!
//! ## Two freshness seams, deliberately distinct
//!
//! `QueuedSaveTicket` + `QueuedSaveFacts` + `queued_save_is_current` decide
//! whether a *queued* request may still be admitted. `SaveCompletionTicket`
//! decides whether a *completed* worker result may still mutate the editor.
//! They guard different windows and both stay.
//!
//! The queued ticket carries `explicit_destination` — the user's intent — and
//! **not** `cancel_pending_load`, which names only one consequence of it.
//!
//! Two separate things make the old defect hard to reintroduce, and they are
//! worth keeping distinct. **The ticket is what gives type safety**: the
//! freshness predicate now takes `QueuedSaveTicket` and `QueuedSaveFacts`
//! rather than five positional scalars, so a value can no longer arrive in a
//! parameter that names it something else — the mismatched call that used to be
//! invisible is now a type error. **The derivation is what gives readability**:
//! `policy::save_may_preempt_pending_load` is `bool -> bool` and proves
//! nothing to the compiler, but it puts the inference in the code where a reader
//! can see it instead of leaving it implied by a shared boolean.
//!
//! The two failure modes this closes are asymmetric and both bad: a plain save
//! that wrongly claims an explicit destination skips the path comparison
//! protecting it from writing a stale target, and a Save As that stops
//! pre-empting the pending load races a load into a just-saved buffer.
//!
//! ## State this workflow shares with others
//!
//! Most of this is read-only for save. Two rows are not, and they are marked,
//! because "reads but does not own" would be false for them.
//!
//! | State | Ownership from this workflow's side |
//! | --- | --- |
//! | `imp().load*`, `cancel_load`, the pending-load cancellation it triggers | read and invoked only; owned by the document-load workflow in `ui/editor_page/load/`, migrated by slot 3b |
//! | `imp().restore.*` and the restore-position group | never touched; cross-cutting editor-page state with five owning workflows, in `ui/editor_page/restore_position.rs` |
//! | `imp().size_check`, `file_size`, `load_state`, encoding state, `monitor.last_known_mtime` | **written by the accept terminal**, which adopts what the write actually produced. Owned jointly with load, which writes the same fields from its own terminal |
//! | `file_path`, `canonical_file_path` | shared editor-page identity, in `ui/editor_page/document_identity.rs`. The accept terminal **replaces the canonical path** with the one the write resolved, and Save As additionally **adopts the new display path** through `adopt_saved_destination`. Otherwise read-only here; the rename, open, and load flows own it too |
//! | `imp().local_history.*`, draft and session records | the local-history and draft/session recovery workflows (slot 4) |
//! | `ui::buffer_snapshot` and its chunked threshold, `ui::plain_disposal` | cross-cutting (slot 7) |
//! | the editor I/O service and the durable atomic-write boundary it writes through | services; behavior unchanged by this migration |

use std::path::PathBuf;

use gtk4::subclass::prelude::ObjectSubclassIsExt;

use crate::services::editor_io::EditorSaveError;
use crate::ui::editor_page::LushtextEditorPage;

pub(crate) mod admission;
pub mod evidence;
mod execution;
pub mod policy;

pub use evidence::SaveEvidence;

use policy::SaveAdmissionPriority;

/// The completion callback one save request carries to its terminal.
pub(crate) type SaveCallback = Box<dyn FnOnce(Result<(), EditorSaveError>)>;

impl LushtextEditorPage {
    /// Save this tab to the path it already tracks.
    ///
    /// Stage 1 for a plain Ctrl+S. The destination is not explicit, so a load in
    /// progress refuses the save rather than pre-empting it.
    pub fn save_file_async<F: FnOnce(Result<(), EditorSaveError>) + 'static>(&self, callback: F) {
        let Some(path) = self.imp().file_path.borrow().clone() else {
            callback(Err(EditorSaveError::NoPath));
            return;
        };
        admission::queue_save_request(
            self,
            path,
            false,
            SaveAdmissionPriority::Ordinary,
            None,
            Box::new(callback),
        );
    }

    /// Save this tab's buffer to a destination the user named.
    ///
    /// Stage 1 for Save As. The tracked path is deliberately **not** mutated
    /// here: the editor adopts the new identity only after the durable write
    /// reports success, through
    /// [`adopt_saved_destination`](Self::adopt_saved_destination).
    pub(crate) fn save_file_async_to_path<F: FnOnce(Result<(), EditorSaveError>) + 'static>(
        &self,
        path: PathBuf,
        callback: F,
    ) {
        admission::queue_save_request(
            self,
            path,
            true,
            SaveAdmissionPriority::Ordinary,
            None,
            Box::new(callback),
        );
    }

    /// Save this tab to its tracked path as part of one close session.
    ///
    /// Stage 1 for close-with-changes. The close session identity travels in the
    /// ticket so a save whose close transaction was superseded publishes
    /// nothing.
    pub(crate) fn save_file_async_for_close<F: FnOnce(Result<(), EditorSaveError>) + 'static>(
        &self,
        close_session_identity: u64,
        callback: F,
    ) {
        let Some(path) = self.imp().file_path.borrow().clone() else {
            callback(Err(EditorSaveError::NoPath));
            return;
        };
        admission::queue_save_request(
            self,
            path,
            false,
            SaveAdmissionPriority::Close,
            Some(close_session_identity),
            Box::new(callback),
        );
    }

    /// Adopt a destination this workflow has just written successfully.
    ///
    /// The final step of Save As, called by the window once the durable write
    /// reported success. It exists so the window states the workflow step it
    /// wants rather than re-reading and re-mutating editor save identity inline.
    pub(crate) fn adopt_saved_destination(&self, path: &std::path::Path) {
        let canonical_path = self.canonical_file_path();
        self.set_file_path_with_canonical(path, canonical_path);
    }

    /// Whether this tab has a background save in progress.
    #[must_use]
    pub fn is_saving(&self) -> bool {
        self.imp().save.inflight.get()
    }
}
