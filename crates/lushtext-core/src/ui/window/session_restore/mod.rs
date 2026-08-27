// SPDX-License-Identifier: GPL-3.0-or-later

//! # The session-restore workflow
//!
//! What happens to the user's window layout across a restart: which documents
//! come back, in what order, which one they end up looking at, and what is
//! written back when they close. **Two stage orders** — persisting the session
//! file, and restoring from it — sharing one durable record.
//!
//! This directory is a **per-workflow role home**. `ui/window/` hosts 15 rows,
//! and `policy.rs` / `evidence.rs` are fixed at one each per workflow, so they
//! cannot be shared; a prefixed `session_policy.rs` was not available either,
//! because pure policy is reached by the mutation scope through
//! `ui/**/policy.rs` and a prefixed name would leave that scope.
//!
//! ## Role table
//!
//! | Module | Role | Owns |
//! | --- | --- | --- |
//! | this file | narrative facade | this narration and the workflow's own entry operations. The 27 stage operations the window calls are declared by the coordination module that owns each stage, not re-exported one-by-one through here |
//! | `journal` | coordination | the session file: collection, the debounced and synchronous writes, the close-time merge, the startup read-back, and failure state |
//! | `admission` | coordination | the bounded-turn runtime, planning permits, turn re-arming, terminal and cancellation |
//! | `execution` | coordination | mounting one admitted page and settling the final selection |
//! | [`policy`] | pure policy | the bounded-turn admission policy, plus the journal's pure half: tab identity, the close-time merge, the preload-graph fit, and the recovery summary |
//! | [`evidence`] | evidence | [`SessionRestoreEvidence`], the one typed value observers read |
//!
//! ## Stage order A: persist
//!
//! 1. **Collect.** `collect_session` walks the tab view into compact
//!    `SessionTab` descriptors. A *failed* file-backed placeholder keeps its path
//!    rather than degrading to untitled, so a transient mount or permission
//!    failure stays retryable next session.
//! 2. **Debounce.** An ordinary edit or tab change schedules a 500 ms write, and
//!    the debounce generation is this record's mutual-exclusion gate.
//! 3. **Write, or merge first.** If startup has not yet published its
//!    descriptors, the persisted file is loaded and **merged** rather than
//!    overwritten — otherwise closing during a still-running restore would delete
//!    every document the restore had not reached. If recovery evidence cannot be
//!    preserved safely, the close is refused rather than the file replaced.
//! 4. **Record the outcome.** Failure sets a retryable banner keyed to the
//!    generation, and a late success may only clear a failure that is not newer
//!    than itself.
//!
//! ## Stage order B: restore
//!
//! 5. **Read the record back**, together with the draft records, in one worker
//!    pass — the descriptors and the draft manifest have to agree. The startup
//!    preload graph is fitted to its disposal reservation on the worker, and the
//!    draft half is handed over through the draft workflow's own named operation.
//! 6. **Plan one bounded turn.** `plan_turn` admits at most four pages per turn
//!    and at most two concurrent background file-planning operations, preserving
//!    persisted order.
//! 7. **Mount each admitted descriptor.** A file-backed one reserves a permit and
//!    hands its planning terminal to the load workflow; an untitled one opens a
//!    fresh tab and adopts its draft.
//! 8. **Re-arm while there is more to do**, then settle. The final selection is
//!    user-first: if the user picked a tab while the restore ran, nothing here
//!    overrides it.
//! 9. **Publish the terminal exactly once**, retain the generation's counters as
//!    a last-restore outcome record, and hand off to draft orphan cleanup.
//!
//! ## Where control leaves, and where it comes back
//!
//! - **A2 → A3, debounce.** `Debounce::schedule` returns; control resumes in the
//!   debounce closure 500 ms later, which re-checks both restore state and
//!   descriptor readiness before writing.
//! - **A3 → A4, worker write.** `spawn_blocking_then` leaves the GTK thread;
//!   control resumes in the completion closure that records or clears failure.
//! - **A3 → A4, close write.** `save_session_for_close_async` inverts the same
//!   way and resumes in a completion that calls the caller's `on_done`.
//! - **B5, capacity wakeup.** With no progress-disposal headroom for the preload
//!   graph, the read arms a wakeup and returns. Control resumes in
//!   `journal`'s `start_startup_journal_read`, retried against the same cancel
//!   token so a superseded startup cannot restart it.
//! - **B5, worker read.** `spawn_blocking_then` leaves the GTK thread; control
//!   resumes in the completion that installs the records and begins the restore.
//! - **B6, per-turn idle.** Every turn is a `glib::idle_add_local_once`; control
//!   resumes in `admission`'s `run_scheduled_session_restore_turn`.
//! - **B7 → B8, the load workflow's planning terminal.** This is the inversion
//!   the whole sequencer is built around. Control leaves through
//!   `load_file_async_with_planning_terminal` and resumes in
//!   `release_session_restore_plan_permit`, which is the **only** thing that
//!   lets the next document open. Every load terminal either carries a parked
//!   request's planning owner into a restart or releases it, and no path drops
//!   one — `release_permit` counts exactly those releases.
//!
//! Seven inversions where the census recorded one.
//!
//! ## State this workflow shares with others
//!
//! | State | Ownership from this workflow's side |
//! | --- | --- |
//! | `session.close_safety_inflight`, `session.close_safety_bypass` | **genuinely shared** with the draft-recovery workflow: one close-safety pass runs draft flush and session save together, and the bypass releases the final close only after both. Both workflows project them; neither owns them |
//! | `session.next_close_save_identity`, `session.active_close_save_identity` | **owned by migrated `WFR-DOCUMENT-SAVE`**, driven end to end by `ui/window/dialogs.rs`. They live in this state group for historical reasons and are not read here |
//! | `session.tab_projection_publications`, and the projection batch | owned by the tab workflow. This workflow **opens and closes** a batch around a restore generation and reports the aggregate; it does not own the projection |
//! | `drafts.manifest`, `drafts.manifest_authority`, `drafts.preloaded` | owned by `WFR-DRAFT-RECOVERY`. The startup read produces them and hands them over through `adopt_startup_draft_records` rather than writing three fields from another workflow's file |
//! | `ui/window/startup_data.rs` — the startup format-upgrade gate | **owned by neither** this row nor drafts. Its census home is `WFR-NOTES-BOOKMARKS`; it *calls* `load_session_and_drafts` and shares no state group here. This workflow only reads its `completed` flag |
//! | `ui/editor_page/restore_position.rs` | cross-cutting with **five** owning workflows, this one among them. Called through `set_restore_position`, never absorbed |
//! | the load workflow's `imp().load*`, `cancel_load`, `dispose_load_resources` | owned by `WFR-DOCUMENT-LOAD`. Reached only through named operations, and `load_evidence()` rather than field reaches |
//! | `services/session_service.rs`, `services/draft_service.rs`, `services/recovery_metadata.rs` | services, shared and behaviorally unchanged. `recovery_metadata` is shared with all three durable slot-4 rows |

mod admission;
pub mod evidence;
mod execution;
mod journal;
pub mod policy;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;

use super::LushtextWindow;

pub(crate) use admission::SessionRestoreRuntime;

impl LushtextWindow {
    /// Cancel scheduled restore ownership without publishing widgets during dispose.
    ///
    /// Stage 9 as the widget hierarchy reaches it. The projection batch is
    /// *cancelled* rather than ended, because publishing a tab projection while
    /// the window is being torn down would rebuild against dying widgets.
    pub(super) fn cancel_session_restore_for_dispose(&self) {
        self.cancel_session_restore_runtime(false);
    }

    /// Whether a bounded session restore currently owns this window.
    ///
    /// A cheap accessor over the one cell rather than a whole
    /// [`SessionRestoreEvidence`] read: the readiness poll consults it on every
    /// automation predicate evaluation, and it is identical by construction
    /// because both read `session.restoring`.
    #[must_use]
    pub(crate) fn session_restore_in_progress(&self) -> bool {
        self.imp().session.restoring.get()
    }

    /// Whether draft or session close-safety work is already running.
    ///
    /// A cheap accessor over the one shared flag rather than a whole
    /// [`SessionRestoreEvidence`] read: the close path consults it inside a guard,
    /// and it is identical by construction because both read the same cell.
    #[must_use]
    pub(crate) fn close_safety_in_progress(&self) -> bool {
        self.imp().session.close_safety_inflight.get()
    }

    /// Start a supplied restore generation through production admission policy.
    ///
    /// A counted actuation seam: stage B5's startup read is the only production
    /// producer of a `SessionData`, and no headless test can reach it without
    /// staging a whole app-data directory and a startup gate.
    #[cfg(feature = "test-utils")]
    pub fn restore_session_for_test(&self, session: crate::model::session::SessionData) {
        self.begin_session_restore(session, false);
    }

    /// Cancel the active generation through the production ownership path.
    ///
    /// A counted actuation seam for the same reason: cancellation is reached from
    /// dispose and from a superseding startup, neither of which a widget test can
    /// drive while asserting the cancelled generation's counters.
    #[cfg(feature = "test-utils")]
    pub fn cancel_session_restore_for_test(&self) {
        self.cancel_session_restore_runtime(false);
    }
}
