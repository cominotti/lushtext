// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over the Protected View fold policy (design D12 of
//! `long-line-protected-view`, ids H1–H2s).
//!
//! Domain of the sequence harnesses: every sequence of at most
//! [`MAX_PARAGRAPHS`] (6) paragraphs whose lengths are arbitrary `u64`
//! values, and every threshold tuple [`FoldThresholds::new`] accepts with
//! `r_fold` from 1 to 3 and arbitrary `u64` `head_cap`, `n_dense` and
//! `v_batch`. Six paragraphs with `r_fold ≤ 3` reach a sequence shorter than
//! one window, warm-up, full and evicting windows, and two separate dense
//! runs (each covered below). The ring is [`HarnessFoldState`] (capacity 4),
//! the production code with a buffer small enough that CBMC does not encode a
//! 256-element array. Window sums are computed here, never by the
//! production code under proof, in `u64`: a window holds at most `r_fold`
//! values of at most `head_cap`, and `FoldThresholds::new` requires
//! `r_fold · head_cap` to fit (Kani checks every addition for overflow).
//!
//! Compiled only under `cfg(kani)` (`make kani`, shard `core-protected-fold`).

use super::{
    A28_SAFE_CHARS, FoldLocation, FoldRange, FoldState, FoldThresholds, HEAD_CAP, L_PROTECT,
    N_HEAD, N_INSPECT, N_VISIBLE, dense_visible, fold_byte_offset, fold_finish, fold_step,
    length_cap, window_is_over_budget,
};

/// The most paragraphs a sequence harness folds.
const MAX_PARAGRAPHS: usize = 6;

/// The largest `r_fold` the harnesses range over.
const MAX_HARNESS_R_FOLD: usize = 3;

/// The ring the harnesses step: capacity 4, at least `MAX_HARNESS_R_FOLD`.
type HarnessFoldState = FoldState<4>;

fn any_thresholds() -> FoldThresholds {
    let r_fold: usize = kani::any();
    kani::assume((1..=MAX_HARNESS_R_FOLD).contains(&r_fold));
    any_thresholds_for(r_fold)
}

/// Every accepted threshold tuple with this `r_fold`. The sequence harnesses
/// run once per `r_fold` from 1 to 3 (their `_r1`, `_r2`, `_r3` instances,
/// together the whole domain): a concrete window size keeps the ring indices
/// concrete, and with a symbolic one H2 alone took 20 minutes.
fn any_thresholds_for(r_fold: usize) -> FoldThresholds {
    let head_cap: u64 = kani::any();
    let n_dense: u64 = kani::any();
    let v_batch: u64 = kani::any();
    // Redundant with `FoldThresholds::new`'s checked products `r_fold ·
    // n_dense ≤ v_batch` and `r_fold · head_cap ≤ u64::MAX` (so the domain is
    // unchanged): stated additively, they spare the solver from reasoning
    // through a 64-bit multiplier.
    let mut dense_window = 0u128;
    let mut head_window = 0u128;
    let mut member = 0;
    while member < r_fold {
        dense_window += u128::from(n_dense);
        head_window += u128::from(head_cap);
        member += 1;
    }
    kani::assume(dense_window <= u128::from(v_batch));
    kani::assume(head_window <= u128::from(u64::MAX));
    let thresholds = FoldThresholds::new(head_cap, n_dense, v_batch, r_fold);
    kani::assume(thresholds.is_some());
    thresholds.unwrap()
}

fn any_sequence() -> ([u64; MAX_PARAGRAPHS], usize) {
    let count: usize = kani::any();
    kani::assume(count <= MAX_PARAGRAPHS);
    (kani::any(), count)
}

/// What one run of the production fold over `lengths[..count]` shows.
struct Shown {
    /// Each paragraph's visible characters (its length when not folded).
    visible: [u64; MAX_PARAGRAPHS],
    /// How many ranges the run emitted.
    folded: usize,
}

/// Fold `lengths[..count]` with the production `fold_step` and `fold_finish`,
/// applying each range as it is emitted (a plan array indexed by a symbolic
/// length cost the solver a mux per range). With `check_shape`, every range
/// is also checked as emitted: inside the sequence, after the previous one,
/// head plus tail equal to the length, a positive tail, and a head of at
/// least `min(len, n_dense) ≥ 1`.
fn fold_run(
    lengths: &[u64; MAX_PARAGRAPHS],
    count: usize,
    thresholds: FoldThresholds,
    check_shape: bool,
) -> Shown {
    let mut state = HarnessFoldState::new(thresholds).unwrap();
    let mut shown = Shown {
        visible: *lengths,
        folded: 0,
    };
    let mut previous: Option<u64> = None;
    let mut apply = |range: FoldRange, shown: &mut Shown| {
        let paragraph = range.paragraph as usize;
        assert!(paragraph < count);
        if check_shape {
            if let Some(previous) = previous {
                assert!(range.paragraph > previous);
            }
            let length = lengths[paragraph];
            assert!(range.head_chars.checked_add(range.tail_chars) == Some(length));
            assert!(range.tail_chars > 0);
            assert!(range.head_chars >= length.min(thresholds.n_dense()));
            assert!(range.head_chars >= 1);
        }
        previous = Some(range.paragraph);
        shown.visible[paragraph] = range.head_chars;
        shown.folded += 1;
    };
    let mut index = 0;
    while index < count {
        if let Some(range) = fold_step(&mut state, lengths[index]) {
            apply(range, &mut shown);
        }
        index += 1;
    }
    for range in fold_finish(state) {
        apply(range, &mut shown);
    }
    shown
}

/// The windows of the sequence: `(start, size)` of the `n − r + 1` windows of
/// `r` paragraphs, or the whole sequence as one window when it is shorter.
fn window_count(count: usize, r_fold: usize) -> (usize, usize) {
    if count < r_fold {
        (usize::from(count > 0), count)
    } else {
        (count - r_fold + 1, r_fold)
    }
}

fn window_sum(values: &[u64; MAX_PARAGRAPHS], start: usize, size: usize) -> u64 {
    let mut sum = 0u64;
    let mut index = start;
    while index < start + size {
        sum += values[index];
        index += 1;
    }
    sum
}

/// Which paragraphs the specification calls dense: members of a window whose
/// `head_cap`-capped sum exceeds `v_batch`, computed independently of the
/// production stream.
fn reference_dense(
    lengths: &[u64; MAX_PARAGRAPHS],
    count: usize,
    thresholds: FoldThresholds,
) -> [bool; MAX_PARAGRAPHS] {
    let mut capped = [0u64; MAX_PARAGRAPHS];
    let mut index = 0;
    while index < count {
        capped[index] = length_cap(lengths[index], thresholds);
        index += 1;
    }
    let mut dense = [false; MAX_PARAGRAPHS];
    let (windows, size) = window_count(count, thresholds.r_fold());
    let mut start = 0;
    while start < windows {
        if window_sum(&capped, start, size) > thresholds.v_batch() {
            let mut member = start;
            while member < start + size {
                dense[member] = true;
                member += 1;
            }
        }
        start += 1;
    }
    dense
}

/// H1 (stream half): the production fold gives every window the two facts
/// the window lemma (H1w) needs. For every paragraph, its visible count is at
/// most its length-capped count (so at most `head_cap`); and for every window
/// of `r_fold` consecutive paragraphs (the whole sequence when shorter) whose
/// capped sum is over `v_batch`, every member shows at most `n_dense`.
/// Domain: the module's sequence domain, one `r_fold` per instance.
///
/// With H1w (any `r_fold ≤ 3` counts with these two facts sum to at most
/// `v_batch`), every window holds at most `v_batch` visible characters. The
/// monolithic form, asserting each window's visible sum here, did not finish
/// in 24 minutes: it made the solver relate overlapping 64-bit sums inside
/// one formula. Split, each half is checked in seconds.
fn check_density_bound(r_fold: usize) {
    let thresholds = any_thresholds_for(r_fold);
    let (lengths, count) = any_sequence();
    let shown = fold_run(&lengths, count, thresholds, false);
    let visible = shown.visible;
    let mut capped = [0u64; MAX_PARAGRAPHS];
    let mut index = 0;
    while index < count {
        capped[index] = length_cap(lengths[index], thresholds);
        assert!(visible[index] <= capped[index]);
        index += 1;
    }
    let (windows, size) = window_count(count, r_fold);
    let mut start = 0;
    while start < windows {
        if window_is_over_budget(window_sum(&capped, start, size), thresholds.v_batch()) {
            let mut member = start;
            while member < start + size {
                assert!(visible[member] <= thresholds.n_dense());
                member += 1;
            }
        }
        start += 1;
    }
    // A sequence shorter than one window exists only when `r_fold > 1`.
    if r_fold > 1 {
        kani::cover!(count > 0 && count < r_fold, "shorter than one window");
    }
    kani::cover!(count > r_fold + 1, "evicting windows");
    kani::cover!(shown.folded > 0, "a paragraph is folded");
    kani::cover!(count > 0 && shown.folded == 0, "nothing is folded");
}

/// density_bound_holds_for_every_window with `r_fold = 1`; see `check_density_bound`.
#[kani::proof]
#[kani::unwind(7)]
fn density_bound_holds_for_every_window_r1() {
    check_density_bound(1);
}

/// density_bound_holds_for_every_window with `r_fold = 2`; see `check_density_bound`.
#[kani::proof]
#[kani::unwind(7)]
fn density_bound_holds_for_every_window_r2() {
    check_density_bound(2);
}

/// density_bound_holds_for_every_window with `r_fold = 3`; see `check_density_bound`.
#[kani::proof]
#[kani::unwind(7)]
fn density_bound_holds_for_every_window_r3() {
    check_density_bound(3);
}

/// H1w: the window lemma. For any `r_fold ≤ 3` length-capped counts `c_i ≤
/// head_cap` and visible counts `v_i ≤ c_i`, where every `v_i ≤ n_dense` when
/// `window_is_over_budget(Σ c_i)`: the visible sum is at most `v_batch`
/// (`r_fold · n_dense ≤ v_batch` when over budget, `Σ v_i ≤ Σ c_i ≤ v_batch`
/// otherwise). With H1's two facts this is the density bound for every window
/// of every sequence H1 covers; neither half depends on the sequence length,
/// so with step (3) of `fold_step`'s argument it extends to every length.
#[kani::proof]
#[kani::unwind(4)]
fn an_over_budget_window_fits_once_dense() {
    let thresholds = any_thresholds();
    let capped: [u64; MAX_HARNESS_R_FOLD] = kani::any();
    let shown: [u64; MAX_HARNESS_R_FOLD] = kani::any();
    let mut capped_sum = 0u64;
    let mut index = 0;
    while index < thresholds.r_fold() {
        kani::assume(capped[index] <= thresholds.head_cap());
        kani::assume(shown[index] <= capped[index]);
        capped_sum += capped[index];
        index += 1;
    }
    let over = window_is_over_budget(capped_sum, thresholds.v_batch());
    let mut shown_sum = 0u64;
    let mut index = 0;
    while index < thresholds.r_fold() {
        if over {
            kani::assume(shown[index] <= thresholds.n_dense());
        }
        shown_sum += shown[index];
        index += 1;
    }
    assert!(shown_sum <= thresholds.v_batch());
    // The production dense count is one such `v_i`.
    let mut index = 0;
    while index < thresholds.r_fold() {
        let dense = dense_visible(capped[index], thresholds.n_dense());
        assert!(dense <= capped[index] && dense <= thresholds.n_dense());
        index += 1;
    }
    kani::cover!(over, "over budget");
    kani::cover!(!over && capped_sum > 0, "within budget");
}

/// H1c: the production thresholds pass `FoldThresholds::new`, `head_cap` is
/// at most each of its terms, and the Line Inspector window fits
/// `min(head_cap, L_PROTECT)`. Concrete.
#[kani::proof]
fn production_thresholds_satisfy_the_preconditions() {
    let production = FoldThresholds::PRODUCTION;
    assert!(
        FoldThresholds::new(
            production.head_cap(),
            production.n_dense(),
            production.v_batch(),
            production.r_fold(),
        ) == Some(production)
    );
    assert!(production.head_cap() == HEAD_CAP);
    assert!(HEAD_CAP <= N_VISIBLE && HEAD_CAP <= N_HEAD && HEAD_CAP <= A28_SAFE_CHARS);
    assert!(N_INSPECT <= HEAD_CAP && N_INSPECT <= L_PROTECT);
}

/// H1x (`should_panic`): `length_cap` alone, under H1's window assertion,
/// breaks the density bound. H1 proves the production stages free of
/// overflow and panics on the same domain, so the only failure reachable here
/// is the window assertion.
#[kani::proof]
#[kani::unwind(7)]
#[kani::should_panic]
fn density_bound_fails_without_the_density_fold() {
    let thresholds = any_thresholds();
    let (lengths, count) = any_sequence();
    let mut visible = [0u64; MAX_PARAGRAPHS];
    let mut index = 0;
    while index < count {
        visible[index] = length_cap(lengths[index], thresholds);
        index += 1;
    }
    let (windows, size) = window_count(count, thresholds.r_fold());
    let mut start = 0;
    while start < windows {
        assert!(window_sum(&visible, start, size) <= thresholds.v_batch());
        start += 1;
    }
}

/// H2: only tails are hidden, and only where needed. For every emitted range,
/// head plus tail is the paragraph's length (lengths exclude delimiters, so no
/// range covers one), the tail is positive, and a non-empty paragraph keeps
/// at least `min(len, n_dense) ≥ 1` characters; ranges are in strictly
/// increasing paragraph order inside the sequence; and every paragraph shows
/// exactly what the specification says (`min(capped, n_dense)` when it lies in
/// an over-budget window, its `head_cap`-capped length otherwise), so a
/// paragraph of at most `head_cap` characters in no over-budget window is
/// never folded. Domain: the module's sequence domain.
fn check_fold_shape(r_fold: usize) {
    let thresholds = any_thresholds_for(r_fold);
    let (lengths, count) = any_sequence();
    let visible = fold_run(&lengths, count, thresholds, true).visible;
    let dense = reference_dense(&lengths, count, thresholds);
    let mut paragraph = 0;
    while paragraph < count {
        let capped = length_cap(lengths[paragraph], thresholds);
        let expected = if dense[paragraph] {
            capped.min(thresholds.n_dense())
        } else {
            capped
        };
        assert!(visible[paragraph] == expected);
        paragraph += 1;
    }
    if r_fold > 1 {
        kani::cover!(
            count > 0 && count < r_fold && dense[0],
            "a short, over-budget document"
        );
    }
    kani::cover!(count == r_fold && dense[0], "one full, over-budget window");
    // Two dense runs separated by an undense paragraph need a gap no window
    // of an over-budget run covers, which six paragraphs leave only for
    // `r_fold ≤ 2`.
    if r_fold <= 2 {
        kani::cover!(
            count == MAX_PARAGRAPHS && dense[0] && !dense[2] && dense[5],
            "two separate dense runs"
        );
    }
    kani::cover!(
        count > 0 && !dense[0] && lengths[0] > thresholds.head_cap(),
        "folded by the length cap alone"
    );
    kani::cover!(
        count > 0
            && dense[0]
            && lengths[0] > thresholds.n_dense()
            && lengths[0] <= thresholds.head_cap(),
        "folded by the density cap alone"
    );
    kani::cover!(
        count > 0 && dense[0] && lengths[0] <= thresholds.n_dense(),
        "dense but short enough to stay whole"
    );
}

/// folds_hide_only_tails_and_only_where_needed with `r_fold = 1`; see `check_fold_shape`.
#[kani::proof]
#[kani::unwind(7)]
fn folds_hide_only_tails_and_only_where_needed_r1() {
    check_fold_shape(1);
}

/// folds_hide_only_tails_and_only_where_needed with `r_fold = 2`; see `check_fold_shape`.
#[kani::proof]
#[kani::unwind(7)]
fn folds_hide_only_tails_and_only_where_needed_r2() {
    check_fold_shape(2);
}

/// folds_hide_only_tails_and_only_where_needed with `r_fold = 3`; see `check_fold_shape`.
#[kani::proof]
#[kani::unwind(7)]
fn folds_hide_only_tails_and_only_where_needed_r3() {
    check_fold_shape(3);
}

/// H2b: `fold_byte_offset` never splits a scalar. Domain: every valid UTF-8
/// string of at most 8 bytes containing no paragraph delimiter (`\n`, `\r`,
/// U+2029), and every `head_chars` up to its character count.
///
/// The harness counts scalars by their lead bytes instead of decoding with
/// `chars()`: three decoding passes over 8 symbolic bytes ran CBMC out of its
/// 14 GiB address space.
#[kani::proof]
#[kani::unwind(9)]
fn fold_byte_offset_never_splits_a_scalar() {
    let bytes: [u8; 8] = kani::any();
    let len: usize = kani::any();
    kani::assume(len <= bytes.len());
    let Ok(paragraph) = core::str::from_utf8(&bytes[..len]) else {
        kani::assume(false);
        return;
    };
    let mut chars = 0u64;
    let mut widths = [false; 5];
    let mut index = 0;
    while index < len {
        let byte = bytes[index];
        kani::assume(byte != b'\n' && byte != b'\r');
        // U+2029 is E2 80 A9; valid UTF-8 puts both continuation bytes inside.
        kani::assume(
            !(byte == 0xE2
                && index + 2 < len
                && bytes[index + 1] == 0x80
                && bytes[index + 2] == 0xA9),
        );
        if byte & 0xC0 != 0x80 {
            chars += 1;
            let width = if byte < 0x80 {
                1
            } else if byte >= 0xF0 {
                4
            } else if byte >= 0xE0 {
                3
            } else {
                2
            };
            widths[width] = true;
        }
        index += 1;
    }
    let head_chars: u64 = kani::any();
    kani::assume(head_chars <= chars);
    let offset = fold_byte_offset(paragraph, head_chars);
    assert!(offset <= len);
    assert!(paragraph.is_char_boundary(offset));
    let mut prefix_chars = 0u64;
    let mut prefix = 0;
    while prefix < offset {
        if bytes[prefix] & 0xC0 != 0x80 {
            prefix_chars += 1;
        }
        prefix += 1;
    }
    assert!(prefix_chars == head_chars);
    kani::cover!(widths[1], "a 1-byte scalar");
    kani::cover!(widths[2], "a 2-byte scalar");
    kani::cover!(widths[3], "a 3-byte scalar");
    kani::cover!(widths[4], "a 4-byte scalar");
    kani::cover!(
        head_chars > 0 && head_chars < chars,
        "a split inside the text"
    );
}

/// H2c: `FoldRange::locate` is total and round-trips. Domain: any range whose
/// head plus tail fits a `u64` (folded or not) and any `u64` column;
/// loop-free.
#[kani::proof]
fn fold_location_is_total_and_round_trips() {
    let range = FoldRange {
        paragraph: kani::any(),
        head_chars: kani::any(),
        tail_chars: kani::any(),
    };
    let Some(length) = range.head_chars.checked_add(range.tail_chars) else {
        kani::assume(false);
        return;
    };
    let column: u64 = kani::any();
    let location = range.locate(column);
    assert!(matches!(location, FoldLocation::Visible(_)) == (column < range.head_chars));
    if let FoldLocation::Visible(visible) = location {
        assert!(visible == column);
    }
    if let FoldLocation::Hidden(offset) = location {
        assert!(offset.checked_add(range.head_chars) == Some(column));
        kani::cover!(offset < range.tail_chars, "inside the hidden tail");
    }
    assert!((location == FoldLocation::PastEnd) == (column > length));
    if column <= length {
        assert!(location.column_of(range) == Some(column));
    } else {
        assert!(location.column_of(range).is_none());
    }
    kani::cover!(matches!(location, FoldLocation::Visible(_)), "visible");
    kani::cover!(matches!(location, FoldLocation::Hidden(_)), "hidden");
    kani::cover!(location == FoldLocation::PastEnd, "past the end");
    kani::cover!(
        range.tail_chars == 0 && column == length,
        "end of an unfolded paragraph"
    );
}

/// H2s: the plan does not depend on install slicing. One run equals two runs
/// split at any index with the state carried across. Domain: the module's
/// sequence domain and any split index up to the sequence length.
///
/// The two runs step in lockstep over the same lengths, the sliced one
/// replacing its state with the carried copy when it reaches the split, and
/// every emission and the finish are compared. A sequential form (whole run,
/// then a second run starting at the split) asks the solver to prove two
/// differently unrolled copies equal and did not finish in 27 minutes.
fn check_slicing_independence(r_fold: usize) {
    let thresholds = any_thresholds_for(r_fold);
    let (lengths, count) = any_sequence();
    let split: usize = kani::any();
    kani::assume(split <= count);
    let mut whole = HarnessFoldState::new(thresholds).unwrap();
    let mut sliced = HarnessFoldState::new(thresholds).unwrap();
    let mut folded = false;
    let mut index = 0;
    while index < count {
        if index == split {
            // The install turn boundary: the next turn continues from a copy.
            let carried = sliced.clone();
            drop(sliced);
            sliced = carried;
        }
        let emitted = fold_step(&mut whole, lengths[index]);
        assert!(fold_step(&mut sliced, lengths[index]) == emitted);
        folded |= emitted.is_some();
        index += 1;
    }
    if split == count {
        let carried = sliced.clone();
        drop(sliced);
        sliced = carried;
    }
    let mut whole_rest = fold_finish(whole);
    let mut sliced_rest = fold_finish(sliced);
    let mut remaining = MAX_HARNESS_R_FOLD;
    while remaining > 0 {
        let emitted = whole_rest.next();
        assert!(sliced_rest.next() == emitted);
        folded |= emitted.is_some();
        remaining -= 1;
    }
    assert!(whole_rest.next().is_none() && sliced_rest.next().is_none());
    kani::cover!(
        split > 0 && split < count && folded,
        "a split inside a folded plan"
    );
}

/// fold_plan_is_independent_of_install_slicing with `r_fold = 1`; see `check_slicing_independence`.
#[kani::proof]
#[kani::unwind(7)]
fn fold_plan_is_independent_of_install_slicing_r1() {
    check_slicing_independence(1);
}

/// fold_plan_is_independent_of_install_slicing with `r_fold = 2`; see `check_slicing_independence`.
#[kani::proof]
#[kani::unwind(7)]
fn fold_plan_is_independent_of_install_slicing_r2() {
    check_slicing_independence(2);
}

/// fold_plan_is_independent_of_install_slicing with `r_fold = 3`; see `check_slicing_independence`.
#[kani::proof]
#[kani::unwind(7)]
fn fold_plan_is_independent_of_install_slicing_r3() {
    check_slicing_independence(3);
}
