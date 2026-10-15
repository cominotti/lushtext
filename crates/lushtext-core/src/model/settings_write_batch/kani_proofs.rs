// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harness over the settings writer's batch algebra.
//!
//! The algebra is the per-key state machine `KeyWriteState`: the batch only
//! finds a key's state, and keys never interact, so the machine is what is
//! proved, with no allocation in the model (a harness over the whole batch,
//! with its vector of keys, exhausted CBMC's memory at two keys and two
//! steps). Domain: three values, an arbitrary backend value, and every
//! sequence of [`STEPS`] operations, each a record, an external change, or a
//! flush whose snapshot may be followed by a newer record before it is
//! acknowledged. After every operation the harness checks that:
//!
//! - a flush never drops the latest value (it is pending, or it is what the
//!   backend will hold: the in-flight value, else the persisted one), even
//!   when it is written while a flush is in flight;
//! - a pending value never equals that baseline, so an acknowledged value is
//!   never re-emitted;
//! - an empty state flushes nothing.
//!
//! The batch's lookup and counters are covered by the unit tests in
//! `settings_write_batch.rs`. Compiled only under `cfg(kani)` (`make kani`).

use super::{KeyWriteState, RecordOutcome};

/// Operations per sequence.
const STEPS: usize = 6;
/// Values are `0..VALUES`.
const VALUES: u8 = 3;

fn any_below(bound: u8) -> u8 {
    let value: u8 = kani::any();
    kani::assume(value < bound);
    value
}

#[kani::proof]
#[kani::unwind(7)]
fn a_flush_never_drops_the_latest_value_or_re_emits_a_persisted_one() {
    let mut state: KeyWriteState<u8> = KeyWriteState::default();
    // The backend's value, as the writer would read it.
    let mut backend = any_below(VALUES);
    // The newest value written, if any.
    let mut latest: Option<u8> = None;
    for _ in 0..STEPS {
        let op = any_below(3);
        let value = any_below(VALUES);
        if op == 0 {
            let seed = backend;
            let outcome = state.record(value, || seed);
            assert!((outcome == RecordOutcome::Pending) == state.pending().is_some());
            latest = Some(value);
        } else if op == 1 {
            // Another process wrote the key; the backend holds it now.
            backend = value;
            state.observe_persisted(value);
            if latest.is_none() || state.pending().is_none() {
                latest = Some(value);
            }
        } else {
            // A flush: take the pending value in flight, write it, maybe
            // record a newer value before the flush is acknowledged, then
            // acknowledge it. An empty state flushes nothing.
            let baseline_before = state.in_flight().or(state.persisted()).copied();
            let flushed = state.begin_flush();
            if let Some(flushed) = flushed {
                assert!(baseline_before != Some(flushed));
                backend = flushed;
            }
            if kani::any() {
                let newer = any_below(VALUES);
                let seed = backend;
                let _ = state.record(newer, || seed);
                latest = Some(newer);
            }
            if let Some(flushed) = flushed {
                state.acknowledge(&flushed);
                assert!(state.in_flight().is_none());
            }
        }
        // What the backend will hold once nothing more is written.
        let baseline = state.in_flight().or(state.persisted());
        if let Some(newest) = latest {
            let kept = state.pending() == Some(&newest)
                || (state.pending().is_none() && baseline == Some(&newest));
            assert!(kept);
        }
        if let Some(pending) = state.pending() {
            assert!(baseline != Some(pending));
        }
        kani::cover!(state.pending().is_some(), "a value is pending");
    }
}
