// SPDX-License-Identifier: GPL-3.0-or-later

//! # The draft-recovery workflow
//!
//! What protects the user's unsaved work. Not "what saves their file" — that is
//! the document-save workflow — but what exists so that closing a laptop lid,
//! losing power, or a crash does not lose an hour of typing in a tab that was
//! never saved. **Three stage orders** over one durable record: autosave, restore,
//! and orphan cleanup.
//!
//! It is also the workflow with the sharpest failure mode in the tree. Autosave
//! writes over the previous recovery body, so a pass that writes the *wrong*
//! bytes destroys the very thing it exists to protect — and orphan cleanup
//! deletes user content by design. Nearly every guard below is there because of
//! one of those two facts.
//!
//! This directory is a **per-workflow role home**; `ui/window/` hosts 15 rows and
//! `policy.rs` / `evidence.rs` are fixed at one each per workflow. A prefixed
//! `draft_policy.rs` was not available: pure policy is reached by the mutation
//! scope through `ui/**/policy.rs`, and a prefixed name would leave that scope —
//! which for *this* workflow's decisions would be a real loss of coverage.
//!
//! ## Role table
//!
//! | Module | Role | Owns |
//! | --- | --- | --- |
//! | this file | narrative facade | this narration, the workflow's own entry operations, and the test-seam surface. The 34 stage operations the window calls are declared by the coordination module that owns each stage, not re-exported one-by-one through here |
//! | `journal` | coordination | the manifest and bodies, the mutation-serialization gate, tombstones, deletes, **and orphan cleanup** |
//! | `admission` | coordination | preload demotion, the one-at-a-time lazy restore queue, disposal reservations, restore accounting |
//! | `autosave_execution` | coordination | the autosave and close-flush pipelines: collect, snapshot, write, commit |
//! | `restore_execution` | coordination | installing a recovered body and its inline alerts |
//! | `retirement` | coordination | handing leftover eager preload bodies to a worker while keeping the compact markers restore still needs |
//! | `seams` | seam value objects | the restore Ticket/Facts pair and predicate, the pipeline's candidate/completion/accepted values, and the worker-boundary payloads |
//! | [`policy`] | pure policy | candidate eligibility, autosave admission, snapshot freshness, failure reporting, cleanup continuation and backoff, and the mutation-intent epoch allocator |
//! | [`evidence`] | evidence | [`DraftEvidence`], the one typed value observers read |
//! | `test_policy` | test-only policy | the workflow's single timing/limit/fault value, entirely behind `#[cfg(feature = "test-utils")]` |
//!
//! Two execution-shaped coordination jobs, so **both** take a stage-order
//! qualifier; neither is a stable sibling renamed for symmetry.
//!
//! ## Stage order A: autosave
//!
//! 1. **A clean tab becomes dirty**, arming a 750 ms first-dirty timer — sooner
//!    than the always-running 5 s tick, because brand-new unsaved work is the
//!    most valuable and least protected.
//! 2. **The tick admits, or marks pending.** With the lane already owned it sets
//!    a flag and returns; it does **not** queue, so a burst of ticks during one
//!    long pass cannot fan out.
//! 3. **Collect candidates.** `policy::draft_candidate_is_eligible` decides.
//!    Its `installation_incomplete` term is a data-safety guard, not an
//!    optimisation: a cancelled load installation empties the buffer and clears
//!    `modified` without clearing `draft_dirty`, so one keystroke afterwards
//!    would otherwise make a near-empty buffer look ordinary and the pass would
//!    write it over a draft holding real work. Each candidate is assigned a
//!    `DraftMutationIntent` **before** any document-sized work, so a later delete
//!    can invalidate it.
//! 4. **Snapshot and write one at a time.** The next candidate is admitted from
//!    inside the previous one's completion, which is what bounds the lane to one
//!    complete body however many tabs are dirty.
//! 5. **Commit once, then accept per completion.** Durability covers only the
//!    generation that was captured: a tab edited again during the write stays
//!    dirty for a later pass.
//!
//! ## Stage order B: restore
//!
//! 6. **Startup delivers the manifest and preload graph** from the session
//!    workflow's read, through `adopt_startup_draft_records`.
//! 7. **Per tab, take a preloaded body or queue a lazy read.** Taking one moves
//!    it under a *replacement* disposal reservation out of the aggregate permit;
//!    with no headroom, **every** eager body is demoted to a compact marker before
//!    returning, so GTK never owns an unguarded recovery body.
//! 8. **Admit one lazy body at a time**, or arm a capacity wakeup.
//! 9. **Validate, then install.** `draft_restore_is_current(ticket, facts)` is
//!    checked when the worker returns **and again** inside the replacement's
//!    terminal, because the bounded install spans turns.
//! 10. **Publish.** The baseline is seeded, the buffer marked modified, and the
//!     restored-draft inline alert offers Discard and Save.
//!
//! ## Stage order C: orphan cleanup
//!
//! 11. **Two seconds after restore**, release eager preloads and begin — but only
//!     if startup **trusted** the manifest. Cleanup deletes user content, so
//!     untrusted metadata refuses outright rather than guessing.
//! 12. **Inspect, then execute on a worker** under the manifest write lock, with
//!     the manifest reloaded, the same `TargetWriteGuard` atomic replacement uses
//!     acquired, and the **inode rechecked before deleting** — because manifest
//!     serialization alone is insufficient: an autosave may finish replacing the
//!     body before it acquires the manifest lock.
//! 13. **Merge exact fingerprints only**, never replace live state, so an
//!     autosave accepted while the worker ran survives.
//! 14. **Continue or back off**, per `policy::orphan_cleanup_follow_up`.
//!
//! ## Where control leaves, and where it comes back
//!
//! Seventeen deferred inversions, where the census recorded seven worker
//! handoffs. The ten it missed are timers, polls, capacity wakeups, chunked
//! snapshots, and the replacement terminal:
//!
//! - **A1/A2, two timers.** The first-dirty `SupersedingTimer` and the 5 s
//!   repeating tick both resume in `autosave_execution`'s `autosave_tick`.
//! - **A4, chunked snapshot**, resuming in the capture's `finish_snapshot`
//!   closure, which re-validates with `policy::captured_snapshot_is_current`.
//! - **A4, body worker**, resuming in a completion that admits the next candidate.
//! - **A5, manifest worker**, resuming in a completion that accepts matching
//!   generations.
//! - **Close flush: a lane-drain poll**, a **chunked snapshot**, a **body
//!   worker**, a **manifest worker**, and a final **`wait_for_draft_mutations_then`
//!   poll** before the caller's `on_done` — five more.
//! - **B8, capacity wakeup**, resuming in `drive_lazy_draft_restore_queue`.
//! - **B9, body-resolve worker**, resuming in `finish_draft_restore`.
//! - **B9, the bounded buffer-replacement terminal**, resuming in the closure that
//!   calls `finish_applied_draft`.
//! - **Delete worker**, resuming in a completion that retires the tombstone only
//!   if its intent is still current.
//! - **C11/C14, two cleanup timers** and **C12, the cleanup worker** — three more.
//!
//! ## State this workflow shares with others
//!
//! | State | Ownership from this workflow's side |
//! | --- | --- |
//! | `session.close_safety_inflight`, `session.close_safety_bypass` | **genuinely shared** with `WFR-SESSION-RESTORE`: one close-safety pass runs this workflow's flush and the session save together, and the bypass releases the final close only after both. Both project them; neither owns them |
//! | the session file, `collect_session` | owned by `WFR-SESSION-RESTORE`. Called on every manifest commit, because the manifest records which session tabs a draft belongs to |
//! | `drafts.manifest`, `manifest_authority`, `preloaded` | **owned here.** The session workflow's startup read produces them and hands them over through `adopt_startup_draft_records`, one named operation rather than three field writes from another workflow's file |
//! | `ui/window/startup_data.rs` — the startup format-upgrade gate | **owned by neither** this row nor session restore. Its census home is `WFR-NOTES-BOOKMARKS`; it *calls* `start_autosave_timer` |
//! | `imp().load.installation_incomplete` | owned by migrated `WFR-DOCUMENT-LOAD`, read through its `has_incomplete_load_installation()` operation. This is the data-safety guard in stage 3 |
//! | `ui/editor_page/buffer_replacement/` | called through `replace_buffer_bounded` to install a recovered body; its session, guard, and terminal are not this workflow's |
//! | `imp().local_history.*` | owned by `WFR-LOCAL-HISTORY`. A restored draft seeds its baseline through that workflow's named operation |
//! | `ui/buffer_snapshot` and its chunked threshold | cross-cutting (slot 7). This workflow supplies its own byte budget and never duplicates the threshold |
//! | `ui/plain_disposal` | cross-cutting (10 workflows). Every recovery body this workflow owns is `DisposalOwned` |
//! | `services/draft_service.rs`, `services/recovery_metadata.rs` | services, behaviorally unchanged. The six load-side `test-utils` overrides in `services/editor_io.rs` are shared with save and load and **stay in the service** |

mod admission;
mod autosave_execution;
pub mod evidence;
mod journal;
pub mod policy;
mod restore_execution;
mod retirement;
pub(super) mod seams;
#[cfg(feature = "test-utils")]
pub mod test_policy;

use std::time::Duration;

use crate::services::draft_service;

pub use seams::DraftFlushError;

#[cfg(feature = "test-utils")]
pub use test_policy::{
    fail_next_draft_mutations_for_test, set_automatic_draft_limit_for_test,
    set_draft_manifest_completion_delay_for_test, set_draft_mutation_delays_for_test,
    set_draft_restore_delay_for_test, set_first_dirty_autosave_delay_for_test,
    set_lazy_draft_read_delay_for_test, set_next_draft_body_disposal_probe_for_test,
    set_orphan_cleanup_delays_for_test,
};

/// Conservative pre-read reservation for one maximum automatic draft body.
pub(super) const DRAFT_RESTORE_DISPOSAL_RESERVATION_BYTES: u64 =
    draft_service::MAX_AUTOMATIC_DRAFT_BYTES.saturating_add(1024 * 1024);

/// The first-dirty autosave debounce, honouring any test override.
pub(super) fn first_dirty_autosave_debounce() -> Duration {
    #[cfg(feature = "test-utils")]
    {
        Duration::from_millis(test_policy::first_dirty_autosave_delay_ms())
    }
    #[cfg(not(feature = "test-utils"))]
    {
        Duration::from_millis(policy::FIRST_DIRTY_AUTOSAVE_DEBOUNCE_MS)
    }
}

/// The automatic-recovery byte limit, honouring any test override.
pub(super) fn automatic_draft_limit() -> u64 {
    #[cfg(feature = "test-utils")]
    {
        test_policy::automatic_draft_limit_bytes()
    }
    #[cfg(not(feature = "test-utils"))]
    {
        draft_service::MAX_AUTOMATIC_DRAFT_BYTES
    }
}

pub(super) fn orphan_cleanup_start_delay() -> Duration {
    #[cfg(feature = "test-utils")]
    {
        Duration::from_millis(test_policy::orphan_cleanup_start_delay_ms())
    }
    #[cfg(not(feature = "test-utils"))]
    {
        policy::ORPHAN_CLEANUP_START_DELAY
    }
}

/// Map a policy-computed follow-up delay onto its test override, when one is set.
///
/// Only the *unmodified* base delay is overridable: a backoff-multiplied delay is
/// left alone so a test cannot accidentally erase the backoff it is exercising.
pub(super) fn orphan_cleanup_followup_delay(delay: Duration) -> Duration {
    #[cfg(feature = "test-utils")]
    {
        if delay == policy::ORPHAN_CLEANUP_FOLLOWUP_DELAY {
            return Duration::from_millis(test_policy::orphan_cleanup_followup_delay_ms());
        }
    }
    delay
}

/// Sleep only when a test has actually configured a delay.
///
/// The guard matters: `thread::sleep(0)` still yields to the scheduler, which can
/// perturb ordering on the default path where no delay is configured at all. Kept
/// as one helper rather than six copies because the migration lost this guard from
/// exactly one of the six sites while the other five never had it — a single
/// implementation is what stops that drifting again.
#[cfg(feature = "test-utils")]
fn sleep_configured_test_delay(delay_ms: u64) {
    if delay_ms > 0 {
        std::thread::sleep(Duration::from_millis(delay_ms));
    }
}

pub(super) fn delay_orphan_cleanup_worker_for_test() {
    #[cfg(feature = "test-utils")]
    sleep_configured_test_delay(test_policy::orphan_cleanup_worker_delay_ms());
}

pub(super) fn delay_draft_restore_for_test() {
    #[cfg(feature = "test-utils")]
    sleep_configured_test_delay(test_policy::restore_delay_ms());
}

pub(super) fn delay_draft_body_for_test() {
    #[cfg(feature = "test-utils")]
    sleep_configured_test_delay(test_policy::body_delay_ms());
}

pub(super) fn delay_draft_manifest_for_test() {
    #[cfg(feature = "test-utils")]
    sleep_configured_test_delay(test_policy::manifest_delay_ms());
}

pub(super) fn delay_draft_manifest_completion_for_test() {
    #[cfg(feature = "test-utils")]
    sleep_configured_test_delay(test_policy::manifest_completion_delay_ms());
}

pub(super) fn delay_draft_delete_for_test() {
    #[cfg(feature = "test-utils")]
    sleep_configured_test_delay(test_policy::delete_delay_ms());
}

pub(super) fn fail_next_draft_body_for_test() -> anyhow::Result<()> {
    #[cfg(feature = "test-utils")]
    if test_policy::take_body_failure() {
        anyhow::bail!("injected draft body failure");
    }
    Ok(())
}

pub(super) fn fail_next_draft_manifest_for_test() -> anyhow::Result<()> {
    #[cfg(feature = "test-utils")]
    if test_policy::take_manifest_failure() {
        anyhow::bail!("injected draft manifest failure");
    }
    Ok(())
}

pub(super) fn fail_next_draft_delete_for_test() -> anyhow::Result<()> {
    #[cfg(feature = "test-utils")]
    if test_policy::take_delete_failure() {
        anyhow::bail!("injected draft delete failure");
    }
    Ok(())
}

pub(super) fn attach_draft_body_disposal_probe(
    owner: crate::ui::plain_disposal::DisposalOwned<String>,
) -> crate::ui::plain_disposal::DisposalOwned<String> {
    #[cfg(feature = "test-utils")]
    let owner = test_policy::attach_body_disposal_probe(owner);
    owner
}

/// Re-export for the window imp's state group.
pub(super) use policy::{DraftMutationIntent, DraftMutationOrder};
pub(crate) use seams::DraftRestoreTicket;
