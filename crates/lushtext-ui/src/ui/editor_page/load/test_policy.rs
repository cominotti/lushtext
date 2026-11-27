// SPDX-License-Identifier: GPL-3.0-or-later

//! The document-load workflow's single test-policy value.
//!
//! Everything a test may override about this workflow lives in one place, and
//! the whole module is behind `#[cfg(feature = "test-utils")]` so a production
//! build compiles no override storage at all. Adding a second module-level
//! static for the next overridable knob is the regression this module exists to
//! prevent.
//!
//! **What is deliberately not here.** `services/editor_io.rs` owns the load
//! delay, the payload-load delay, the processing-chunk counters, and the
//! transient-weight override, because the service owns the behavior those
//! change. Mirroring them into a second value in `ui/` would fork one policy
//! across two workflows; slot 3a recorded the same decision for the save side.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;

use crate::ui::plain_disposal::{DisposalOwned, DisposalProbeSlot};

/// Test-only overrides for one process's document-load workflow.
struct LoadTestPolicy {
    /// Observes the worker thread that finally retires a decoded load body.
    body_disposal_probe: DisposalProbeSlot,
    /// Replaces the next conservative disposal reservation weight, so a test can
    /// exercise capacity refusal without allocating a giant GTK fixture.
    next_disposal_reservation_weight: AtomicU64,
}

impl LoadTestPolicy {
    const fn new() -> Self {
        Self {
            body_disposal_probe: DisposalProbeSlot::new(),
            next_disposal_reservation_weight: AtomicU64::new(0),
        }
    }
}

static POLICY: LoadTestPolicy = LoadTestPolicy::new();

/// Observe the worker thread that finally retires the next decoded load body.
pub fn set_next_load_body_disposal_probe_for_test(sender: Sender<std::thread::ThreadId>) {
    POLICY.body_disposal_probe.set(sender);
}

/// Override one conservative reservation without allocating a giant GTK fixture.
pub fn set_next_load_disposal_reservation_weight_for_test(weight: u64) {
    POLICY
        .next_disposal_reservation_weight
        .store(weight, Ordering::Release);
}

/// Attach the pending body-disposal probe, if a test armed one.
pub(super) fn attach_body_disposal_probe(owner: DisposalOwned<String>) -> DisposalOwned<String> {
    POLICY.body_disposal_probe.attach(owner)
}

/// Take a one-shot reservation-weight override, if a test armed one.
pub(super) fn take_disposal_reservation_weight_override() -> Option<u64> {
    let weight = POLICY
        .next_disposal_reservation_weight
        .swap(0, Ordering::AcqRel);
    (weight > 0).then_some(weight)
}
