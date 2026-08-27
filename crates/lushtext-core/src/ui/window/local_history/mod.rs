// SPDX-License-Identifier: GPL-3.0-or-later

//! # The local-history workflow
//!
//! Per-document snapshots the user can browse and restore: a baseline captured
//! the moment a clean document becomes modified, periodic snapshots while it
//! stays modified, a safety snapshot before any restore, and one taken by every
//! successful save. **Two stage orders** — capture, and browse/restore — over one
//! sidecar record.
//!
//! ## The two-directory decision
//!
//! This workflow spans two directories of owned code, and the fixed role names
//! `policy.rs` and `evidence.rs` are one each **per workflow**, so it cannot own
//! role files in both. It is resolved on the **coordination/presentation line**,
//! the split slot 3b used for the recent-documents surface:
//!
//! - **`ui/window/local_history/` is the canonical role home** — this directory.
//!   It holds the facade, all coordination, the workflow's single `policy.rs`,
//!   and its single `evidence.rs`. Browse, preview, restore, undo, lineage
//!   migration, and action enablement are coordination: they sequence bounded
//!   work and own the durable record.
//! - **`ui/editor_page/local_history.rs` is a called surface**, whose ownership
//!   is recorded in its own module doc. It is per-tab *presentation-adjacent
//!   capture*: it watches one buffer's clean/modified transitions and drives
//!   captures for it. It owns no policy and no evidence of its own — it calls
//!   [`policy`] for its freshness tickets and projects into [`evidence`].
//!
//! Two `policy.rs` files for one row was the alternative, and it is exactly what
//! the convention forbids.
//!
//! ## Role table
//!
//! | Module | Role | Owns |
//! | --- | --- | --- |
//! | this file | narrative facade | this narration and the workflow's own entry operations. The 31 stage operations the window calls are declared by the coordination module that owns each stage, not re-exported one-by-one through here |
//! | `journal` | coordination | the sidecar record: listing a lineage, its recovery diagnostics, lineage migration after a rename, and browse-action enablement |
//! | `preview_execution` | coordination | the browser dialog, one-active/one-latest body reads, and the bounded preview install |
//! | `restore_execution` | coordination | the safety capture, the buffer replacement, and the undo affordance |
//! | [`policy`] | pure policy | viewer geometry, which snapshots are shown, the preview install plan, both capture freshness tickets, and row presentation |
//! | [`evidence`] | evidence | [`LocalHistoryEvidence`], the one typed value observers read |
//! | `test_policy` | test-only policy | the workflow's single timing/limit value, entirely behind `#[cfg(feature = "test-utils")]` |
//!
//! Two execution-shaped coordination jobs in one stage order means **both** take
//! a stage-order qualifier; neither is a stable sibling renamed for symmetry.
//!
//! ## Stage order A: capture
//!
//! 1. **A clean document becomes modified.** The capture surface's
//!    `modified-changed` handler admits only a saved, browseable, non-suppressed
//!    buffer.
//! 2. **Capture the baseline** — the *last clean* text, which is what makes a
//!    "before edits" snapshot meaningful. One process-wide permit admits a single
//!    document-sized automatic payload at a time; a contended editor enqueues a
//!    **weak** waiter rather than a body.
//! 3. **Arm the periodic timer**, and re-arm it after each capture while the
//!    document stays modified and file-backed.
//! 4. **Persist, or hand the text back.** A failed baseline returns its text to
//!    its original cycle only if that cycle is still current, and spends one
//!    retry-budget unit.
//!
//! ## Stage order B: browse and restore
//!
//! 5. **List the lineage** on a worker, publishing any recovery diagnostics.
//!    Legacy empty baselines are hidden only when there is evidence of the old
//!    bug that created them.
//! 6. **Present the browser**, or an empty status page when nothing is visible.
//! 7. **Preview one snapshot.** Selection submits to a one-active/one-latest
//!    coordinator; the body read takes a disposal reservation, or defers behind a
//!    capacity wakeup; installation is direct under 1 MiB and otherwise
//!    paragraph-aligned slices.
//! 8. **Restore.** The **current** buffer is captured and persisted as a
//!    `RestoreSafety` snapshot *first* — a restore never destroys what it
//!    replaces — then the historical body is installed through the bounded
//!    buffer-replacement workflow, and the captured body becomes the undo
//!    affordance.
//! 9. **Undo the restore**, installing the retained body back through the same
//!    replacement workflow.
//!
//! ## Where control leaves, and where it comes back
//!
//! Sixteen deferred inversions, where the census recorded six. By stage:
//!
//! - **A2, admission drain.** Releasing the automatic-capture permit posts
//!   `MainContext::invoke`; control resumes in the capture surface's waiter drain,
//!   which admits the next weak waiter that is still eligible.
//! - **A2 → A4, baseline worker.** Control resumes in a **failure-only**
//!   completion, validated by `policy::baseline_capture_is_current`.
//! - **A3, periodic timer.** A `SupersedingTimer`; control resumes in the capture
//!   surface's periodic run.
//! - **A3 → A4, chunked snapshot then persist worker.** Two more inversions,
//!   the second validated by `policy::periodic_capture_is_current`.
//! - **B5, listing worker**, twice: once for the active editor and once for an
//!   explicit path, which additionally opens the document first.
//! - **B7, capacity wakeup**, resuming in `retry_preview_admission`.
//! - **B7, body-read worker**, resuming in `finish_preview_load`, which accepts
//!   only the current generation and starts any queued latest read.
//! - **B7, install slice**, an idle or timeout source resuming in
//!   `run_preview_install_slice`.
//! - **B8, restore capacity wakeup** and **B8, chunked undo capture**, then the
//!   **safety-capture worker**, then the **buffer-replacement terminal**.
//! - **B9, the undo path's own replacement terminal.**
//! - **Lineage migration worker**, resuming in a completion that warns on
//!   failure without blocking the rename that triggered it.
//!
//! ## State this workflow shares with others
//!
//! | State | Ownership from this workflow's side |
//! | --- | --- |
//! | `imp().local_history.*` on the editor page | **owned here.** Migrated save reads `editor_generation` as its `SaveCompletionTicket` freshness identity, and migrated load reads `automatic_capture_suppressed` when it restores installation state; both go through named accessors on this workflow rather than field reaches |
//! | `automatic_capture_suppressed` during a bounded replacement | suspended and exactly restored by `WFR-BUFFER-REPLACEMENT`'s guard, which this workflow calls |
//! | `model::buffer_replacement` — the direct/sliced threshold and `next_replacement_boundary` | **cross-cutting and called, never copied.** The preview installer uses the same paragraph-boundary arithmetic as the replacement workflow and the load workflow |
//! | `ui/editor_page/buffer_replacement/` | called through `replace_buffer_bounded` for both restore and undo; its session, guard, and terminal are not this workflow's |
//! | `services/local_history_service.rs` | a service, shared with migrated `WFR-DOCUMENT-SAVE`, which captures a snapshot on every successful save. Behavior unchanged |
//! | `services/recovery_metadata.rs` | a service, shared with all three durable slot-4 rows |
//! | `resolve_notes_for_editor`, `dismiss_editor_notifications` | owned by `WFR-NOTES-BOOKMARKS` (slot 5) and the notification workflow. Called from two restore terminals, never absorbed |
//! | `migration_ledger` | cross-cutting; the rename migration records a tracked kind through it |

pub mod evidence;
mod journal;
pub mod policy;
mod preview_execution;
mod restore_execution;
#[cfg(feature = "test-utils")]
pub mod test_policy;

use crate::ui::editor_page::LushtextEditorPage;

use super::LushtextWindow;

#[cfg(feature = "test-utils")]
pub use evidence::LocalHistoryEvidence;
pub use evidence::{LocalHistoryPreviewInstallEvidence, local_history_preview_install_evidence};

#[cfg(feature = "test-utils")]
pub use crate::services::local_history_service::set_local_history_preview_read_delay_for_test;
#[cfg(feature = "test-utils")]
pub use test_policy::{
    set_local_history_baseline_delay_for_test, set_local_history_baseline_failures_for_test,
    set_local_history_preview_install_delay_for_test,
};

impl LushtextWindow {
    /// Open the local-history browser for the active saved document.
    ///
    /// Stage 5 as the user reaches it, from `win.show-local-history`, the editor
    /// context menu, or the command palette. Refuses with a status message rather
    /// than an empty dialog when the document is unsaved or too large to browse.
    pub(super) fn show_local_history_dialog(&self) {
        self.list_local_history_for_active_editor();
    }

    /// Open local history for an explicit saved file path.
    ///
    /// Stage 5 as the **sidebar context menu** reaches it — an entry point the
    /// census cell omitted. The lineage is listed before the tab is opened, so an
    /// ineligible file never disturbs the user's tabs.
    pub(super) fn show_local_history_for_path(&self, path: &std::path::Path) {
        self.list_local_history_for_path(path);
    }
}

impl LushtextEditorPage {
    /// This editor's local-history disposal generation.
    ///
    /// A cheap accessor rather than a whole [`LocalHistoryEvidence`] read:
    /// migrated `WFR-DOCUMENT-SAVE` reads it once per save completion to build
    /// its `SaveCompletionTicket`, and it is identical by construction because
    /// both read the same cell. `ui/editor_page/save/mod.rs` documented this
    /// group as slot 4's to own; this is how save keeps reading it without a
    /// field reach.
    #[must_use]
    pub(crate) fn local_history_editor_generation(&self) -> u64 {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        self.imp().local_history.editor_generation.get()
    }

    /// This editor's local-history path generation.
    #[must_use]
    pub(crate) fn local_history_path_generation(&self) -> u64 {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        self.imp().local_history.path_generation.get()
    }

    /// This editor's local-history edit generation.
    #[must_use]
    pub(crate) fn local_history_edit_generation(&self) -> u64 {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        self.imp().local_history.edit_generation.get()
    }

    /// Suspend automatic capture and report what it was before.
    ///
    /// The paired half of [`Self::set_local_history_capture_suppressed`]. Migrated
    /// `WFR-DOCUMENT-LOAD` and `WFR-BUFFER-REPLACEMENT` both suspend capture
    /// around their own buffer mutations and restore exactly what they found, so
    /// they take and give back the previous value through these two operations
    /// rather than reaching into this workflow's state group.
    pub(crate) fn suspend_local_history_capture(&self) -> bool {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        self.imp()
            .local_history
            .automatic_capture_suppressed
            .replace(true)
    }

    /// Restore automatic-capture suspension to a previously observed value.
    pub(crate) fn set_local_history_capture_suppressed(&self, suppressed: bool) {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        self.imp()
            .local_history
            .automatic_capture_suppressed
            .set(suppressed);
    }
}
