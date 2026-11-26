// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses for the list budget (`bound-lists-by-budget-not-count`,
//! design D2, shard `core-list-budget`).
//!
//! Domain: every `u64` limit and charge, sequences of at most
//! [`MAX_SEQUENCE`] offers (B1, B2) or worst-case items (B3, B4), every `u64`
//! elapsed time and every unit count (T1), and turns of at most
//! [`MAX_SEQUENCE`] units of any `u64` cost below 2^32 µs (T2); every `u64`
//! limit with a zero charge, every `u64` limit at seven concrete charges (B5's
//! closed form, a recorded restriction), and results of at most
//! [`MAX_SEQUENCE`] rows (B5's agreement with the budget); every `u32`
//! length (G1, R2) and every `u32` length and held span (R1).

use super::{
    Admission, LIST_RETIREMENT_UNIT_ROWS, LIST_TURN_SLICE_US, RetainedBudget,
    for_each_unheld_range, next_growth, retirement_unit_cut, rows_within, staged_first_rows,
    turn_continues,
};

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

/// B5, zero charge: a charge of zero bounds nothing, for every `u64` limit.
#[kani::proof]
fn rows_within_a_zero_charge_bounds_nothing() {
    assert_eq!(rows_within(kani::any(), 0), usize::MAX);
}

/// The charges B5's closed form is checked at: the small ones, the palette's
/// 36-byte ranked row, a page, one past `u32`, and `u64::MAX`.
const CLOSED_FORM_CHARGES: [u64; 7] = [1, 2, 3, 36, 4_096, (1 << 32) + 1, u64::MAX];

/// B5, closed form: `rows_within` is the largest `r` with
/// `r * charge <= limit`, with no overflow (checked arithmetic), for **every**
/// `u64` limit at each charge of [`CLOSED_FORM_CHARGES`].
///
/// **Restricted domain (charge):** with a symbolic charge the 64-bit division
/// did not finish, neither for the closed form over every `u64` (16 minutes)
/// or under `limit < 2^32, charge < 2^16` (12 minutes), nor for the bare
/// equality with `limit / charge` over every `u64` (15 minutes) (2026-11-07,
/// `drill-down-palette-categories` task 2.1); a concrete divisor keeps the
/// limit symbolic over every `u64`. A symbolic charge is covered by
/// `rows_within_agrees_with_the_budget` (every charge, results of at most
/// four rows) and by the unit and property tests.
#[kani::proof]
#[kani::unwind(8)]
fn rows_within_is_the_largest_fitting_prefix() {
    let limit: u64 = kani::any();
    for charge in CLOSED_FORM_CHARGES {
        let rows = rows_within(limit, charge) as u64;
        let used = rows
            .checked_mul(charge)
            .expect("r * charge cannot overflow");
        assert!(used <= limit);
        match rows
            .checked_add(1)
            .and_then(|next| next.checked_mul(charge))
        {
            Some(next) => assert!(next > limit),
            None => kani::cover!(true, "the next row would overflow u64"),
        }
    }
}

/// B5, agreement: for results of at most `MAX_SEQUENCE` rows, `rows_within`
/// equals the number of `charge`-byte offers a `RetainedBudget` retains.
#[kani::proof]
#[kani::unwind(6)]
fn rows_within_agrees_with_the_budget() {
    let limit: u64 = kani::any();
    let charge: u64 = kani::any();
    kani::assume(charge > 0);
    let rows = rows_within(limit, charge);
    kani::assume(rows <= MAX_SEQUENCE);
    let mut budget = RetainedBudget::new(limit);
    let mut retained = 0usize;
    for _ in 0..=MAX_SEQUENCE {
        if budget.admit(charge) == Admission::Retained {
            retained += 1;
        }
    }
    assert_eq!(retained, rows);
    kani::cover!(rows == MAX_SEQUENCE, "a full sequence fits");
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

/// G1 (`lighten-notes-browser-rows` D5): from its first listing, a growing
/// first publish lists strictly more rows each step, never more than the
/// list, and reaches every row in at most eight steps.
#[kani::proof]
#[kani::unwind(10)]
fn growth_reaches_every_row_in_at_most_eight_steps() {
    let len: u32 = kani::any();
    let mut listed = staged_first_rows(len);
    assert!(listed <= len);
    let mut steps = 0u32;
    while let Some(next) = next_growth(listed, len) {
        assert!(next > listed && next <= len);
        listed = next;
        steps += 1;
        assert!(steps <= 8);
    }
    assert_eq!(listed, len);
    kani::cover!(steps == 8, "a long list grows in eight steps");
}

/// R1 (`lighten-notes-browser-rows` D4): a retirement's first turn removes
/// only rows nobody holds, in ranges that stay valid when removed in order
/// (descending, disjoint), and leaves every held row; with the held rows they
/// cover the list. Domain: every `u32` length and zero, one or two held
/// positions, strictly ascending and below the length (the core
/// [`for_each_unheld_range`]; `unheld_ranges`' filter, sort and dedup are
/// the standard library's, covered by the property tests).
#[kani::proof]
#[kani::unwind(5)]
fn the_unheld_ranges_keep_every_held_row() {
    let len: u32 = kani::any();
    let held: [u32; 2] = [kani::any(), kani::any()];
    let count: usize = kani::any();
    kani::assume(count <= 2);
    kani::assume(count < 1 || held[0] < len);
    kani::assume(count < 2 || (held[0] < held[1] && held[1] < len));
    let held = &held[..count];
    let mut ranges = [(0u32, 0u32); 3];
    let mut emitted = 0usize;
    for_each_unheld_range(len, held, |start, count| {
        assert!(emitted < 3);
        ranges[emitted] = (start, count);
        emitted += 1;
    });
    let ranges = &ranges[..emitted];
    let mut covered = 0u64;
    let mut previous_start = u32::MAX;
    for &(start, count) in ranges {
        assert!(count > 0);
        assert!(u64::from(start) + u64::from(count) <= u64::from(len));
        assert!(start + count <= previous_start);
        previous_start = start;
        for &row in held {
            assert!(row < start || row >= start + count);
        }
        covered += u64::from(count);
    }
    let kept = u64::try_from(held.len()).unwrap_or(u64::MAX);
    assert_eq!(covered + kept, u64::from(len));
    kani::cover!(ranges.len() == 3, "rows go after, between and before");
}

/// R2: a retirement unit removes at least one and at most
/// `LIST_RETIREMENT_UNIT_ROWS` rows from a non-empty list, and ends on empty.
#[kani::proof]
fn a_retirement_unit_removes_at_most_its_rows() {
    let len: u32 = kani::any();
    match retirement_unit_cut(len) {
        None => assert_eq!(len, 0),
        Some(next) => {
            assert!(next < len);
            assert!(len - next <= LIST_RETIREMENT_UNIT_ROWS);
        }
    }
}
