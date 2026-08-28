// SPDX-License-Identifier: GPL-3.0-or-later

//! # The document-load workflow
//!
//! What happens between the user naming a file — through the Open dialog, the
//! recent-documents popover, a sidebar row, a session restore, or a
//! reopen-with-a-different-encoding — and that file's text appearing in a tab.
//! One workflow, one stage order, seven points where control leaves and comes
//! back.
//!
//! This directory is a **per-workflow role home**: `ui/editor_page/` hosts eight
//! workflows and the role file names `policy.rs` and `evidence.rs` are fixed at
//! one each per workflow, so this workflow keeps its roles in a subdirectory
//! whose `mod.rs` — this file — is the facade. It is the second adopter of the
//! shape slot 3a introduced for [`super::save`]. Together the two changes
//! dissolved `load_save.rs`, the 1,795-line file that used to hold both
//! workflows.
//!
//! ## Role table
//!
//! | Module | Role | Owns |
//! | --- | --- | --- |
//! | this file | narrative facade | the entry points and this narration |
//! | `admission` | coordination | identity rotation, the planning probe, the process-wide coordinator, the queue, the idle drain, exactly-once charge release |
//! | `execution` | coordination | acceptance, the four-phase installation state machine, the final projection |
//! | `retirement` | coordination | cancellation and disposal: the payload, the charge, the partial buffer, the identity |
//! | [`policy`] | pure policy | the chunked-versus-direct threshold, the slice budgets and the paragraph-boundary rule, phase and abort classification, the freshness predicates, the failure-state rule |
//! | [`evidence`] | evidence | [`LoadEvidence`], the one typed value observers read |
//! | `test_policy` | test policy | the workflow's single `test-utils` override value |
//!
//! ## Stages
//!
//! 1. **Accept the request.** [`LushtextEditorPage::load_file_async`],
//!    [`load_file_async_with_encoding`](LushtextEditorPage::load_file_async_with_encoding),
//!    and
//!    `load_file_async_with_planning_terminal`
//!    are the only ways in; they differ only in whether an encoding is forced
//!    and whether an external owner is waiting on the planning terminal. The
//!    window may first call `set_file_path_for_pending_load`, which belongs to
//!    the shared identity group in `super::document_identity` rather than to
//!    this workflow, so duplicate detection works before any content exists.
//!    Delegates to `admission::begin_load_request`.
//! 2. **Park, or rotate identity.** A request arriving during final projection
//!    or during a live installation is parked as the single latest intent and
//!    the current work is asked to stop; bounded cleanup restarts it. Otherwise
//!    the editor's load identity is rotated — a fresh generation and a fresh
//!    cancellation token — and captured **once** as one
//!    `policy::LoadRequestTicket`, which every later stage validates as a unit.
//! 3. **Plan.** A compact worker probe reads metadata only and produces the
//!    plan whose `transient_weight` the shared budget is spent against. **No
//!    document text has been read yet.**
//! 4. **Admit under a shared byte budget.** `admission`'s drain retires
//!    requests whose ticket no longer describes their editor, re-prioritises the
//!    survivors by which tab the user is looking at, and admits as many as the
//!    transient budget and the disposal lane allow.
//! 5. **Read and decode on a worker.** The bytes are read, decoded in the
//!    requested or detected encoding, and handed back as a payload the disposal
//!    lane owns, so freeing it later never blocks the GTK thread.
//! 6. **Install.** A stale completion publishes nothing. A fresh one enters the
//!    buffer either in one turn or in bounded main-loop slices, decided by
//!    `policy::requires_chunked_install`. **This is where decoded bytes enter
//!    the buffer.** Every slice boundary, inserting and clearing alike, ends on
//!    a paragraph boundary; [`policy`] records why that is a performance
//!    contract with a user-visible failure mode rather than refactorable detail.
//! 7. **Publish.** Size, size class, canonical identity, encoding, BOM, health
//!    findings, and mtime are adopted; the cursor is restored; the file monitor
//!    starts; local history is seeded; and the tab reports `Loaded`.
//! 8. **Retire, when asked to stop.** `retirement` gives back the payload, the
//!    admission charge, the partially installed buffer, and the load identity.
//!    Cancellation is user-visible and clears the buffer; disposal is silent.
//!    Both leave `installation_incomplete` set until a retry installs one exact
//!    payload, because the emptied buffer must never be saved over the file.
//!
//! ## Where control leaves, and where it comes back
//!
//! - **Stage 2 → 3, planning worker.** `spawn_blocking_then` leaves the GTK
//!   thread; control resumes in the planning completion closure inside
//!   `admission::begin_load_request`, which either submits or publishes the
//!   failure.
//! - **Stage 3 → 4, idle drain.** Submitting schedules a
//!   `glib::idle_add_local_once` and returns; control resumes in
//!   `admission::drain`.
//! - **Stage 4, disposal-capacity wakeup.** When the disposal lane has no room
//!   for the decoded body, the grant is returned to the queue and a capacity
//!   wakeup is armed; control resumes in `admission::schedule_drain` once the
//!   lane's epoch advances.
//! - **Stage 4 → 5 → 6, read worker.** A second `spawn_blocking_then` performs
//!   the read and decode; control resumes in
//!   `execution::accept_admitted_load_outcome`. **This is where a stale load is
//!   refused**, against the ticket from stage 2.
//! - **Stage 6, per slice.** A chunked installation yields on a 1 ms timeout
//!   between slices; control resumes in `execution::run_install_slice`, which
//!   re-checks freshness before every slice and can divert to stage 8.
//! - **Stage 8, cancelled cleanup.** Cancellation does not clear the buffer in
//!   one turn either; control resumes per slice in
//!   `retirement::run_cancelled_clear_slice`, **which is where a cancelled
//!   load's content is cleared**, and which restarts a parked request when it
//!   finishes.
//! - **Charge release.** Dropping a `TransientLoadPermit` posts
//!   `glib::idle_add_once`; control resumes in `admission::release_on_main`,
//!   which re-arms this lane's drain and the save lane's. Every terminal
//!   converges here, so worker failure, stale completion, cancellation,
//!   disposal, and success all release exactly once.
//!
//! ## One freshness seam, validated as a unit
//!
//! `policy::LoadRequestTicket` carries `{load_generation, cancel_token}`. It
//! is built once in stage 2 and checked by `is_current(&editor)` at the planning
//! completion, at every drain, and at the read completion. It needs no `*Facts`
//! companion because every clause it compares is live editor state against
//! dispatch-time expectation.
//!
//! Token **identity** matters as much as the flag: each request installs a fresh
//! `Arc`, so an older worker cannot be un-cancelled by a newer request clearing
//! the token. The installation state machine deliberately uses the weaker
//! `policy::installation_is_current` instead — an installation is already the
//! newest request's own work, so it re-reads the editor's current token rather
//! than comparing identity.
//!
//! ## State this workflow shares with others
//!
//! | State | Ownership from this workflow's side |
//! | --- | --- |
//! | `imp().load*`, `load_tracking`, `cancel_load`, `dispose_load_resources` | **owned here.** The save workflow calls `cancel_load` when a Save As pre-empts an in-flight load; it does not touch the state |
//! | `imp().restore.*` and the restore-position group | cross-cutting editor-page state with five owning workflows; lives in `super::restore_position`. Load **calls** `apply_restore_position` from its publish stage and owns none of it |
//! | `set_file_path`, `set_file_path_with_canonical`, `set_file_path_for_pending_load`, `size_check`, `reapply_language` | shared document identity and metadata in `super::document_identity`, also used by rename, minimap, encoding, accessibility, and local history. Load **calls** the provisional-path operation from the window side before stage 1 and owns none of the group. Load's publish stage writes `size_check`, `file_size`, and the canonical path, jointly with save's accept terminal |
//! | `ui::plain_disposal` and its file-load reservation lane | cross-cutting (slot 7). Load reserves and owns; it does not restructure the lane |
//! | `model/file_load.rs` | domain, and **staying in `model/`**: `services/editor_io.rs` depends on it, so relocating it under `ui/` would invert dependency direction |
//! | `model/buffer_replacement.rs`, `ui::buffer_snapshot` | cross-cutting (slots 4 and 7); load reads their shared thresholds and owns neither |
//! | the editor I/O service's read, decode, and encoding-detection path | services; behavior unchanged by this migration, and its load-side `test-utils` overrides stay there because the service owns the behavior |

use std::path::Path;

use gtk4::subclass::prelude::ObjectSubclassIsExt;

use crate::model::encoding::DocumentEncoding;
use crate::ui::editor_page::LushtextEditorPage;

use super::save;

pub(crate) mod admission;
pub mod evidence;
mod execution;
pub mod policy;
mod retirement;
#[cfg(feature = "test-utils")]
mod test_policy;

pub use evidence::LoadEvidence;
pub use policy::{LoadInstallPhase, LoadOutcome};

// Two request-lifetime types the editor page's `imp` struct stores directly.
pub(crate) use admission::PendingFileLoad;
pub(crate) use execution::ChunkedLoadInstall;

#[cfg(feature = "test-utils")]
pub use test_policy::{
    set_next_load_body_disposal_probe_for_test, set_next_load_disposal_reservation_weight_for_test,
};

impl LushtextEditorPage {
    /// Start loading a file asynchronously.
    ///
    /// Stage 1 for the Open dialog, the recent-documents popover, sidebar row
    /// activation, and the window's reload paths.
    pub fn load_file_async(&self, path: &Path) {
        admission::begin_load_request(self, path, None, None);
    }

    /// Start loading a file asynchronously, forcing a reopen encoding.
    ///
    /// Stage 1 for "Reopen with encoding". The forced encoding travels with the
    /// request rather than being applied to the buffer afterwards, so a failed
    /// decode leaves the previous content intact.
    pub fn load_file_async_with_encoding(&self, path: &Path, reopen_as: Option<DocumentEncoding>) {
        admission::begin_load_request(self, path, reopen_as, None);
    }

    /// Start one load whose background planning admission is owned externally.
    ///
    /// Stage 1 for session restore, which sequences how many documents may be
    /// planning at once and counts the terminals to decide when to open the
    /// next. The terminal is released exactly once on every path, including the
    /// ones that park or discard the request.
    pub(crate) fn load_file_async_with_planning_terminal<F>(&self, path: &Path, on_terminal: F)
    where
        F: FnOnce() + 'static,
    {
        admission::begin_load_request(self, path, None, Some(Box::new(on_terminal)));
    }

    /// Cancel any in-progress file load. Safe to call even if no load is active.
    ///
    /// Stage 8 as the user reaches it, and the operation the save workflow calls
    /// when a Save As pre-empts a load it does not depend on.
    pub fn cancel_load(&self) {
        retirement::cancel_load(self);
    }

    /// Tear down queued, admitted, or installing load state without UI feedback.
    ///
    /// Stage 8 as the widget hierarchy reaches it.
    pub(super) fn dispose_load_resources(&self) {
        retirement::dispose_load_resources(self);
    }

    /// Register a callback fired after every successful file load or reload.
    ///
    /// Notes and focus indexing both need the same "a real file just finished
    /// loading" hook, so this list is fan-out friendly and survives reloads.
    pub fn connect_file_loaded<F: Fn() + 'static>(&self, f: F) {
        self.imp()
            .load
            .file_loaded_callbacks
            .borrow_mut()
            .push(Box::new(f));
    }

    /// Register the one-shot terminal for the load request the window is about
    /// to start.
    ///
    /// The window uses this to finish opening a tab — recording recent history,
    /// closing a canonical duplicate, checking for a draft — only once content
    /// has actually arrived. It is one-shot by design: stage 7 takes the
    /// callback, so a later reload of the same tab does not re-run tab setup.
    pub(crate) fn connect_load_completed_once<F: FnOnce() + 'static>(&self, f: F) {
        self.imp()
            .load
            .load_completed_callback
            .replace(Some(Box::new(f)));
    }

    /// Register the one-shot terminal for a load request that fails.
    ///
    /// Window-level open flows undo provisional tab and path state from here,
    /// rather than guessing at failure before the background read has settled.
    pub(crate) fn connect_load_failed_once<F: FnOnce(String) + 'static>(&self, f: F) {
        self.imp()
            .load
            .load_failed_callback
            .replace(Some(Box::new(f)));
    }

    /// Whether document-amplifying callbacks must ignore installation edits.
    ///
    /// Deliberately wider than the evidence surface's `projection_suspended`:
    /// this also reports the bounded buffer-replacement workflow's suspension,
    /// because a projection must stand down for either.
    #[must_use]
    pub(crate) fn load_projection_suspended(&self) -> bool {
        self.imp().load.projection_suspended.get() || self.buffer_replacement_projection_suspended()
    }

    /// Whether this buffer holds a partially installed or cleared load.
    ///
    /// A cancelled or aborted installation leaves the buffer holding neither the
    /// old document nor the new one, and leaves this true until a retry installs
    /// one exact content. Any workflow that would **publish** the buffer must
    /// refuse while it holds: the document-save workflow does so with
    /// `EditorSaveError::IncompleteLoadInstallation`, and the draft-recovery
    /// workflow skips the tab rather than writing a partial buffer over a draft
    /// that still holds real unsaved work.
    ///
    /// This is a cheap accessor over the one cell rather than a whole
    /// [`evidence::LoadEvidence`] read, because its callers consult it once per
    /// tab per pass — identical by construction, since both read the same cell.
    #[must_use]
    /// Whether a load installation was interrupted partway.
    ///
    /// Widened from `pub(crate)` so a widget test can assert the draft-candidate
    /// contract through the production predicate rather than a proxy: this flag is
    /// what makes autosave skip a tab, and the tab/close regression test needs to
    /// feed it to `drafts::policy::draft_candidate_is_eligible`.
    pub fn has_incomplete_load_installation(&self) -> bool {
        self.imp().load.installation_incomplete.get()
    }
}
