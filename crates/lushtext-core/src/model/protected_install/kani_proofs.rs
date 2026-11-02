// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over the sliced protected install planner
//! (`sliced-protected-install`, design D5): K7, K8b, K10 and K11.
//!
//! **K7** ranges over up to [`MAX_RANGES`] recorded ranges of 0..=15 bytes and
//! a clear budget of 4..=7. **K8b** over every `u64` paragraph length and
//! density parameters that cannot bind on one paragraph. **K10** over every
//! `u64` input. **K11** (`should_panic`) over one undelimited 12-byte
//! paragraph of one 2-, 3- or 4-byte scalar, `head_cap` 1..=3 and a tail
//! budget of 4..=7.
//!
//! The whole-plan harnesses K1–K6, K8 and K9 (every planner property over
//! every document of a few bytes) did not fit CBMC: at 10, 6 and 4 bytes they
//! exhausted 16 GiB in propositional reduction or ran past 20 minutes, with
//! C2's real fold in front of the planner and without it. Their properties are
//! property-tested instead, at small and production budgets on every input
//! (`tests/properties/protected_install.rs`), with the K9 bound's
//! counterexample pinned by a unit test; rewriting them as one-step harnesses
//! over a symbolic cursor is recorded in `docs/next/formal-verification.md`.
//!
//! Compiled only under `cfg(kani)` (`make kani`, shard
//! `core-protected-install`).

use super::{
    MAX_SLICE_BYTES, MIN_TAIL_SLICE_BYTES, ProtectedInstallCursor, ProtectedInstallLimits,
    SliceBudgetUnattainable, clear_ranges_for_turn, plan_with,
};
use crate::model::buffer_replacement::SliceBudget;
use crate::model::protected_fold::{FoldRange, FoldState, FoldThresholds, fold_finish, fold_step};

/// K7's recorded ranges.
const MAX_RANGES: usize = 4;

fn as_text(bytes: &[u8]) -> &str {
    // Every caller passes whole UTF-8 scalars.
    unsafe { core::str::from_utf8_unchecked(bytes) }
}

fn any_limits() -> ProtectedInstallLimits {
    let tail: usize = kani::any();
    let clear: usize = kani::any();
    kani::assume((4..=7).contains(&tail) && (4..=7).contains(&clear));
    ProtectedInstallLimits::new(tail, clear).unwrap()
}

/// K7: repeated clear turns over one paragraph's recorded ranges delete them
/// from the first forwards, each range exactly once, at most the clear budget
/// per turn unless one range alone exceeds it, as many as fit, and stop when
/// none is left.
#[kani::proof]
#[kani::unwind(6)]
fn protected_clear_plan_covers_the_buffer_once() {
    let sizes: [u64; MAX_RANGES] = kani::any();
    let count: usize = kani::any();
    kani::assume(count <= MAX_RANGES);
    let mut index = 0;
    while index < count {
        kani::assume(sizes[index] <= 15);
        index += 1;
    }
    let limits = any_limits();
    let mut first = 0;
    let mut turns = 0;
    let mut deleted = 0u64;
    while first < count {
        let taken = clear_ranges_for_turn(&sizes[first..count], limits);
        assert!(taken >= 1 && first + taken <= count);
        let mut bytes = 0;
        let mut range = first;
        while range < first + taken {
            bytes += sizes[range];
            range += 1;
        }
        assert!(taken == 1 || bytes <= limits.clear_slice_bytes() as u64);
        // Maximal: the next range would not have fit.
        if first + taken < count {
            assert!(bytes + sizes[first + taken] > limits.clear_slice_bytes() as u64);
        }
        deleted += bytes;
        first += taken;
        turns += 1;
    }
    assert!(clear_ranges_for_turn(&sizes[..0], limits) == 0);
    let mut total = 0;
    index = 0;
    while index < count {
        total += sizes[index];
        index += 1;
    }
    assert!(deleted == total && turns <= count);
    kani::cover!(turns >= 2 && turns < count);
}

/// C2's fold of one paragraph whose content is `content` (density not
/// binding): `head_cap` (1..=3) characters visible when the paragraph is
/// longer, no fold otherwise.
fn giant_fold(content: &[u8]) -> ([FoldRange; 1], usize) {
    let head_cap: u64 = kani::any();
    kani::assume((1..=3).contains(&head_cap));
    let chars = scalars_in(content);
    let range = FoldRange {
        paragraph: 0,
        head_chars: head_cap,
        tail_chars: chars.saturating_sub(head_cap),
    };
    ([range], usize::from(chars > head_cap))
}

fn scalars_in(bytes: &[u8]) -> u64 {
    let mut count = 0;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] & 0xC0 != 0x80 {
            count += 1;
        }
        index += 1;
    }
    count
}

/// K10: for every `u64` input, the derivation never panics, and an
/// `Ok(bytes)` is in `MIN_TAIL_SLICE_BYTES..=MAX_SLICE_BYTES` and accepted by
/// `ProtectedInstallLimits::new`, so it composes with the planner's K5.
///
/// That the value fits the budget with its margin and is the largest that
/// does is **property-tested** over every `u64` input
/// (`tests/properties/protected_install.rs`,
/// `settle_budget_derivation_fits_and_is_the_largest`), not proved: stated
/// here, CBMC's 128-bit multiplications and divisions by symbolic divisors
/// did not finish in 20 minutes.
#[kani::proof]
fn settle_budget_derivation_fits_and_never_overflows() {
    let budget_us: u64 = kani::any();
    let cost: u64 = kani::any();
    let margin: u64 = kani::any();
    match ProtectedInstallLimits::for_settle_budget(budget_us, cost, margin) {
        Ok(bytes) => {
            assert!((MIN_TAIL_SLICE_BYTES..=MAX_SLICE_BYTES).contains(&bytes));
            assert!(ProtectedInstallLimits::new(bytes, bytes).is_some());
            kani::cover!(bytes < MAX_SLICE_BYTES);
            kani::cover!(bytes == MAX_SLICE_BYTES);
        }
        Err(SliceBudgetUnattainable) => kani::cover!(true),
    }
}

/// The rounding with the guard removed: the raw candidate end.
fn unrounded(_bytes: &[u8], _floor: usize, candidate: usize) -> usize {
    candidate
}

/// K11: the same stepper with the tail rounding removed ends a tail slice
/// inside a scalar, so the K3 assertion fails. Pins the guard from both
/// sides: K3 fails if production stops rounding; this fails (no
/// counterexample) if the stepper stops honouring its rounding argument.
///
/// Domain: one undelimited 12-byte paragraph of one 2-, 3- or 4-byte
/// scalar (`é`, `€`, `𝄞`), `head_cap` 1..=3, tail budget 4..=7. A
/// counterexample needs only one candidate end inside one scalar, so the
/// string is concrete per width and only the width, `head_cap` and the
/// budget are symbolic, and only the first two slices are planned (a symbolic
/// 8-byte string ran for over 10 minutes; a concrete string with the whole
/// plan exhausted 16 GiB in CBMC's propositional reduction).
/// The covers report the failure reached inside a scalar of each width.
#[kani::proof]
#[kani::should_panic]
#[kani::unwind(13)]
fn unrounded_tail_end_splits_a_scalar() {
    const TWO: [u8; 12] = [
        0xC3, 0xA9, 0xC3, 0xA9, 0xC3, 0xA9, 0xC3, 0xA9, 0xC3, 0xA9, 0xC3, 0xA9,
    ];
    const THREE: [u8; 12] = [
        0xE2, 0x82, 0xAC, 0xE2, 0x82, 0xAC, 0xE2, 0x82, 0xAC, 0xE2, 0x82, 0xAC,
    ];
    const FOUR: [u8; 12] = [
        0xF0, 0x9D, 0x84, 0x9E, 0xF0, 0x9D, 0x84, 0x9E, 0xF0, 0x9D, 0x84, 0x9E,
    ];
    let width: usize = kani::any();
    kani::assume((2..=4).contains(&width));
    let bytes: &[u8] = match width {
        2 => &TWO,
        3 => &THREE,
        _ => &FOUR,
    };
    let len = bytes.len();
    let text = as_text(bytes);
    let (ranges, count) = giant_fold(bytes);
    let limits = any_limits();
    let mut cursor = ProtectedInstallCursor::new();
    // The head, then the first tail slice: its unrounded end is the head's
    // end plus the budget, inside a scalar whenever the budget is not a
    // multiple of the width.
    let mut calls = 0;
    while calls < 2 {
        let Some(slice) = plan_with(
            text,
            &ranges[..count],
            &mut cursor,
            SliceBudget::BYTES_ONLY,
            limits,
            unrounded,
        ) else {
            break;
        };
        let splits = slice.end != len && bytes[slice.end] & 0xC0 == 0x80;
        kani::cover!(splits && width == 2);
        kani::cover!(splits && width == 3);
        kani::cover!(splits && width == 4);
        // The K3 assertion.
        assert!(!splits);
        calls += 1;
    }
}

/// K8b: C2's real `fold_step`/`fold_finish` fold a lone paragraph of any
/// `u64` length, with density parameters that cannot bind on it
/// (`v_batch ≥ head_cap`, `r_fold` 1..=2), to exactly `min(length, head_cap)`
/// visible characters: the range the giant-paragraph harnesses build
/// (`giant_fold`).
#[kani::proof]
#[kani::unwind(4)]
fn one_paragraph_fold_shows_its_head_cap() {
    let length: u64 = kani::any();
    let head_cap: u64 = kani::any();
    let n_dense: u64 = kani::any();
    let v_batch: u64 = kani::any();
    let r_fold: usize = kani::any();
    kani::assume((1..=2).contains(&r_fold));
    kani::assume(head_cap >= 1 && n_dense >= 1 && n_dense <= head_cap);
    kani::assume(
        v_batch >= head_cap && n_dense.checked_mul(2).is_some_and(|twice| v_batch >= twice),
    );
    let thresholds = FoldThresholds::new(head_cap, n_dense, v_batch, r_fold);
    kani::assume(thresholds.is_some());
    let mut state = FoldState::<4>::new(thresholds.unwrap()).unwrap();
    let mut found = fold_step(&mut state, length);
    for range in fold_finish(state) {
        assert!(found.is_none());
        found = Some(range);
    }
    match found {
        Some(range) => {
            assert!(length > head_cap);
            assert!(range.paragraph == 0 && range.head_chars == head_cap);
            assert!(range.tail_chars == length - head_cap);
        }
        None => assert!(length <= head_cap),
    }
    kani::cover!(found.is_some());
}
