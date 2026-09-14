// SPDX-License-Identifier: GPL-3.0-or-later

//! Headless GTK responsiveness evidence for weighted plain-data disposal.

use crate::common::{ensure_gtk_init, wait_until};
use lushtext_core::ui::plain_disposal::{
    aggregate_pressure_evidence_for_test, hold_disposal_capacity_for_test,
    hold_progress_disposal_capacity_for_test, plain_disposal_evidence,
};

#[test]
fn test_aggregate_disposal_pressure_returns_immediately_and_keeps_gtk_alive() {
    ensure_gtk_init();

    let evidence = aggregate_pressure_evidence_for_test();
    assert_eq!(evidence.producers, 4);
    assert_eq!(evidence.immediate_full_outcomes, 5);
    assert_eq!(evidence.producer_pending_high_water, 1);
    assert_eq!(evidence.producer_retry_high_water, 1);
    assert_eq!(evidence.teardown_cancellations, 1);
    assert_eq!(evidence.gtk_heartbeat_turns, 1);
    assert_eq!(evidence.running_high_water, 2);
    assert_eq!(evidence.queued_high_water, 2);
    assert_eq!(evidence.retained_bytes_high_water, 4 * 1024 * 1024);
    assert_eq!(evidence.completed_jobs, 8);
    assert_eq!(evidence.producer_terminals, 13);
    assert_eq!(evidence.final_pending_jobs, 0);
    assert_eq!(evidence.preadmitted_worker_drops, 1);

    eprintln!(
        "plain-disposal-pressure-evidence producers={} immediate_full={} pending_high_water={} retry_high_water={} teardown_cancellations={} gtk_heartbeats={} running_high_water={} queued_high_water={} retained_bytes_high_water={} completed_jobs={} producer_terminals={} final_pending={} preadmitted_worker_drops={}",
        evidence.producers,
        evidence.immediate_full_outcomes,
        evidence.producer_pending_high_water,
        evidence.producer_retry_high_water,
        evidence.teardown_cancellations,
        evidence.gtk_heartbeat_turns,
        evidence.running_high_water,
        evidence.queued_high_water,
        evidence.retained_bytes_high_water,
        evidence.completed_jobs,
        evidence.producer_terminals,
        evidence.final_pending_jobs,
        evidence.preadmitted_worker_drops,
    );
}

#[test]
fn test_production_disposal_lane_reserves_guarded_replacement_headroom() {
    ensure_gtk_init();

    let limits = plain_disposal_evidence().ordinary.limits;
    assert_eq!(limits.replacement_job_headroom, 1);
    assert!(limits.worker_limit + limits.queued_job_limit >= 8);
}

#[test]
fn test_recovery_progress_capacity_is_independent_from_ordinary_owners() {
    ensure_gtk_init();

    let ordinary_hold = hold_disposal_capacity_for_test();
    let progress_hold = hold_progress_disposal_capacity_for_test();
    let limits = plain_disposal_evidence().progress.limits;
    let snapshot = plain_disposal_evidence().progress.snapshot;

    assert_eq!(limits.retained_byte_limit, 72 * 1024 * 1024);
    assert_eq!(limits.replacement_job_headroom, 1);
    assert!(snapshot.overweight_exclusive);

    drop(progress_hold);
    drop(ordinary_hold);
}

// --- The lane's three surface proofs (delta 2) -------------------------------
//
// `WFR-PLAIN-DISPOSAL` is a cross-cutting lane, not a workflow: it owes the
// evidence surface but no facade, no coordination role names, and no policy
// module. These three proofs are the same ones a migrated workflow's surface
// owes, adapted to a lane whose state lives in process-wide atomics that worker
// threads advance.

/// Drain both lanes to a terminal and confirm quiescence through the surface.
///
/// **This is the load-bearing helper for proof 1.** The lane's counters are
/// process-wide atomics mutated by worker threads, so "unchanged state" is not a
/// property of the reader's control flow: a job admitted by an unrelated test in
/// the same process can advance `running_jobs` between two reads that the test
/// believes are adjacent. Quiescing first is what makes read-to-read identity a
/// real claim rather than a race that usually passes.
///
/// Returns whether quiescence was reached within the budget, so a caller can
/// degrade to monotonicity rather than assert an unsound equality.
fn quiesce_lanes(budget: std::time::Duration) -> bool {
    // Uses the **shared** `wait_until` rather than a private poll loop. Its
    // mechanism is load-bearing and is the opposite of the obvious one: it
    // sleeps briefly and *then* drains every ready main-loop source, which is
    // what dispatches `spawn_blocking_then`'s low-priority `idle_add_once`
    // completion. A hand-rolled loop that drains first and sleeps after — which
    // this helper was — starves exactly the terminal it is waiting for.
    //
    // `wait_until` panics on timeout; this wrapper catches that so a caller can
    // degrade to a monotonic assertion instead of claiming an identity the lane
    // never reached.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        wait_until(budget, || plain_disposal_evidence().is_quiesced());
    }))
    .is_ok()
        && plain_disposal_evidence().is_quiesced()
}

/// Proof 1 of 3 — **reentrancy / side-effect freedom**.
///
/// Reads are taken after each operation that mutates the state the surface
/// reports, and repeated reads of unchanged state must be identical. Identity is
/// asserted **only** with the lane quiesced; where a worker can still advance a
/// counter the assertion is **monotonicity**, which is the distinction slot 7a's
/// no-retry widget lane caught as an unsound assertion whose panic read exactly
/// like a production defect.
#[test]
fn test_plain_disposal_evidence_reads_stay_side_effect_free_across_lane_mutation() {
    ensure_gtk_init();

    assert!(
        quiesce_lanes(std::time::Duration::from_secs(10)),
        "the lane must reach a terminal before identity may be asserted"
    );

    // Quiesced: two reads must be byte-identical, including every high-water
    // mark and cumulative counter.
    let quiet = plain_disposal_evidence();
    assert!(quiet.is_quiesced());
    assert_eq!(
        plain_disposal_evidence(),
        quiet,
        "repeated reads of a quiesced lane must be identical"
    );
    assert_eq!(plain_disposal_evidence(), quiet, "and again");

    // Saturating the lane takes the admission lock and mutates live counters.
    {
        let _hold = hold_disposal_capacity_for_test();
        let held = plain_disposal_evidence();
        assert!(
            !held.ordinary.is_quiesced(),
            "an outstanding hold must be visible as non-quiesced"
        );
        // Not quiesced, so identity is not claimed. What must hold is that
        // reading did not itself admit, release, or retire anything: the
        // cumulative admission count is unchanged across two adjacent reads.
        let again = plain_disposal_evidence();
        assert_eq!(
            again.ordinary.snapshot.admitted_jobs, held.ordinary.snapshot.admitted_jobs,
            "reading admitted a job; the hold blocks the lane, so nothing can \
             advance this counter between two adjacent reads and the comment \
             above claims equality rather than monotonicity"
        );
        assert_eq!(
            again.ordinary.limits, held.ordinary.limits,
            "fixed ceilings cannot move while the lane is held"
        );
    }

    // Back to quiet: identity is claimable again, and the high-water marks the
    // hold produced must have survived the quiesce rather than being reset.
    assert!(quiesce_lanes(std::time::Duration::from_secs(10)));
    let after = plain_disposal_evidence();
    assert!(after.is_quiesced());
    assert_eq!(plain_disposal_evidence(), after);
    assert!(
        after.ordinary.snapshot.admitted_jobs >= quiet.ordinary.snapshot.admitted_jobs,
        "cumulative counters are monotonic across a quiesce"
    );
}

/// Proof 2 of 3 — **the disposed stage**.
///
/// A lane has no `TemplateChild`, so the disposed-widget rule needs its analogue
/// named rather than skipped: the stage is a **torn-down owner whose pending job
/// is cancelled**. The surface must keep answering honestly through that
/// transition instead of reporting a job that no longer has an owner.
#[test]
fn test_plain_disposal_evidence_answers_honestly_through_owner_teardown() {
    ensure_gtk_init();
    assert!(quiesce_lanes(std::time::Duration::from_secs(10)));

    let before = plain_disposal_evidence();

    // A hold is an owner with an outstanding reservation; dropping it is this
    // lane's teardown.
    let owner = hold_disposal_capacity_for_test();
    let during = plain_disposal_evidence();
    assert!(!during.ordinary.is_quiesced());
    assert_eq!(
        during.ordinary.limits, before.ordinary.limits,
        "teardown must not move a fixed ceiling"
    );
    drop(owner);

    // After teardown the surface must return to an honest quiesced answer rather
    // than stranding the reservation it was reporting.
    assert!(
        quiesce_lanes(std::time::Duration::from_secs(10)),
        "a torn-down owner must not strand a reservation"
    );
    let after = plain_disposal_evidence();
    assert!(after.is_quiesced());
    assert_eq!(after.ordinary.snapshot.running_jobs, 0);
    assert_eq!(after.ordinary.snapshot.queued_jobs, 0);
    assert_eq!(
        plain_disposal_evidence(),
        after,
        "repeated reads after teardown must stay identical"
    );
}

/// Proof 3 of 3 — **non-materialization**.
///
/// Reading the surface must not admit, reserve, release, retire, spawn, or
/// advance any counter it reports. Proved in both extremes — empty and saturated
/// — with every admission counter, retained-byte total, high-water mark, and
/// terminal count compared before and after **each** read.
///
/// This is the proof that would have failed had
/// `aggregate_pressure_evidence_for_test` been folded into the surface: that
/// accessor spawns threads and saturates the lane, so a single read would move
/// nearly every field asserted below.
#[test]
fn test_plain_disposal_evidence_reads_materialize_no_lane_state() {
    ensure_gtk_init();
    assert!(quiesce_lanes(std::time::Duration::from_secs(10)));

    // Empty extreme.
    let baseline = plain_disposal_evidence();
    for _ in 0..5 {
        let observed = plain_disposal_evidence();
        assert_eq!(
            observed, baseline,
            "reading an empty lane must change nothing at all"
        );
    }

    // Saturated extreme. Identity is not claimed here — a worker may still be
    // running — so each field that a read could plausibly disturb is compared
    // across two adjacent reads instead.
    {
        let _hold = hold_disposal_capacity_for_test();
        let _progress_hold = hold_progress_disposal_capacity_for_test();

        let first = plain_disposal_evidence();
        let second = plain_disposal_evidence();

        for (lane, a, b) in [
            ("ordinary", first.ordinary, second.ordinary),
            ("progress", first.progress, second.progress),
        ] {
            assert_eq!(a.limits, b.limits, "{lane}: reading moved a fixed ceiling");
            assert_eq!(
                a.snapshot.admitted_jobs, b.snapshot.admitted_jobs,
                "{lane}: reading admitted a job"
            );
            assert_eq!(
                a.snapshot.running_high_water, b.snapshot.running_high_water,
                "{lane}: reading advanced the running high-water mark"
            );
            assert_eq!(
                a.snapshot.queued_high_water, b.snapshot.queued_high_water,
                "{lane}: reading advanced the queued high-water mark"
            );
            assert_eq!(
                a.snapshot.retained_bytes_high_water, b.snapshot.retained_bytes_high_water,
                "{lane}: reading advanced the retained-byte high-water mark"
            );
            assert_eq!(
                a.snapshot.overweight_exclusive, b.snapshot.overweight_exclusive,
                "{lane}: reading changed overweight exclusivity"
            );
            assert_eq!(
                a.snapshot.replacement_headroom_active, b.snapshot.replacement_headroom_active,
                "{lane}: reading borrowed replacement headroom"
            );
        }
    }

    assert!(quiesce_lanes(std::time::Duration::from_secs(10)));
}
