// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses for the list budget (`bound-lists-by-budget-not-count`,
//! design D2, shard `core-list-budget`).
//!
//! Domain: every `u64` limit and charge, sequences of at most
//! [`MAX_SEQUENCE`] offers (B1, B2) or worst-case items (B3, B4), every `u64`
//! elapsed time and every unit count (T1), and turns of at most
//! [`MAX_SEQUENCE`] units of any `u64` cost below 2^32 µs (T2).

use super::*;

/// Offers per harness. The properties are per step, so a short sequence
/// covers every transition the latch can take (fits before, refused once,
/// refused after).
const MAX_SEQUENCE: usize = 4;

/// B1: the charge never exceeds the limit, and it only grows by retained
/// charges.
#[kani::proof]
#[kani::unwind(5)]
fn the_charge_never_exceeds_the_limit() {
    let mut budget = RetainedBudget::new(kani::any());
    for _ in 0..MAX_SEQUENCE {
        let before = budget.charged();
        let charge: u64 = kani::any();
        let admission = budget.admit(charge);
        assert!(budget.charged() <= budget.limit());
        match admission {
            Admission::Retained => assert_eq!(budget.charged(), before + charge),
            Admission::Limited => assert_eq!(budget.charged(), before),
        }
    }
}

/// B2: the retained items are a prefix of the offers: once an item is
/// refused, every later one is refused, whatever its charge.
#[kani::proof]
#[kani::unwind(5)]
fn the_retained_items_are_a_prefix() {
    let mut budget = RetainedBudget::new(kani::any());
    let mut refused = false;
    for _ in 0..MAX_SEQUENCE {
        let admission = budget.admit(kani::any());
        if refused {
            assert_eq!(admission, Admission::Limited);
        }
        if admission == Admission::Limited {
            refused = true;
            kani::cover!(true, "a refusal latches");
        }
        assert_eq!(budget.is_limited(), refused);
    }
}

/// B3: no loss against a count cap. A limit of at least `n * w` retains any
/// `n` items each charging at most `w`.
#[kani::proof]
#[kani::unwind(5)]
fn a_budget_of_n_times_w_loses_nothing() {
    let w: u64 = kani::any();
    let n: usize = kani::any();
    kani::assume(n <= MAX_SEQUENCE);
    let Some(floor) = w.checked_mul(n as u64) else {
        return;
    };
    let limit: u64 = kani::any();
    kani::assume(limit >= floor);
    let mut budget = RetainedBudget::new(limit);
    for _ in 0..n {
        let charge: u64 = kani::any();
        kani::assume(charge <= w);
        assert_eq!(budget.admit(charge), Admission::Retained);
    }
    kani::cover!(n == MAX_SEQUENCE, "a full sequence is retained");
}

/// B4: the counter-example. One byte below `n * w`, the `n`-th worst-case
/// item is refused, so asserting it retained fails; kept failing by design.
#[kani::proof]
#[kani::should_panic]
#[kani::unwind(5)]
fn a_budget_one_byte_below_n_times_w_refuses_the_last_item() {
    let w: u64 = kani::any();
    kani::assume(w > 0);
    let n: usize = kani::any();
    kani::assume(n >= 1 && n <= MAX_SEQUENCE);
    let Some(floor) = w.checked_mul(n as u64) else {
        return;
    };
    let mut budget = RetainedBudget::new(floor - 1);
    for _ in 0..n {
        assert_eq!(budget.admit(w), Admission::Retained);
    }
}

/// T1: every turn takes its first unit, whatever the clock says.
#[kani::proof]
fn every_turn_makes_progress() {
    assert!(turn_continues(kani::any(), 0));
}

/// T2: a guarded turn stops at the first unit started at or after the slice,
/// so its guarded work costs less than the slice plus its largest unit.
#[kani::proof]
#[kani::unwind(5)]
fn a_turn_stops_at_the_first_unit_after_the_slice() {
    let mut elapsed = 0u64;
    let mut done = 0usize;
    let mut largest = 0u64;
    while done < MAX_SEQUENCE && turn_continues(elapsed, done) {
        let cost: u64 = kani::any();
        kani::assume(cost < 1 << 32);
        largest = largest.max(cost);
        elapsed += cost;
        done += 1;
    }
    assert!(done >= 1);
    assert!(elapsed < LIST_TURN_SLICE_US + largest);
    if done < MAX_SEQUENCE {
        assert!(elapsed >= LIST_TURN_SLICE_US);
        kani::cover!(true, "a turn yields at the slice");
    }
}
