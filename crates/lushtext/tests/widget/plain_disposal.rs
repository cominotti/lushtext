// SPDX-License-Identifier: GPL-3.0-or-later

//! Headless GTK responsiveness evidence for weighted plain-data disposal.

use crate::common::ensure_gtk_init;
use lushtext_core::ui::plain_disposal::{
    aggregate_pressure_evidence_for_test, hold_disposal_capacity_for_test,
    hold_progress_disposal_capacity_for_test, limits_for_test, progress_lane_snapshot_for_test,
    progress_limits_for_test,
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

    let limits = limits_for_test();
    assert_eq!(limits.replacement_job_headroom, 1);
    assert!(limits.worker_limit + limits.queued_job_limit >= 8);
}

#[test]
fn test_recovery_progress_capacity_is_independent_from_ordinary_owners() {
    ensure_gtk_init();

    let ordinary_hold = hold_disposal_capacity_for_test();
    let progress_hold = hold_progress_disposal_capacity_for_test();
    let limits = progress_limits_for_test();
    let snapshot = progress_lane_snapshot_for_test();

    assert_eq!(limits.retained_byte_limit, 72 * 1024 * 1024);
    assert_eq!(limits.replacement_job_headroom, 1);
    assert!(snapshot.overweight_exclusive);

    drop(progress_hold);
    drop(ordinary_hold);
}
