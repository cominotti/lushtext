// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain-Rust policy bounding a user-visible list by retained bytes and by
//! per-turn time instead of by a count (`bound-lists-by-budget-not-count`,
//! design D2).
//!
//! **Cross-cutting pure policy with four owning workflows**, `WFR-SEARCH-REPLACE`
//! (the workspace search worker charges each match), `WFR-WORKSPACE-TREE`
//! (the sidebar applies a directory's rows in time-sliced batches) and
//! `WFR-NOTES-BOOKMARKS` (Browse Notes captures open-editor bookmarks in
//! time-sliced turns under one retained-byte budget,
//! `bound-notes-browser-by-budget` D4) and `WFR-COMMAND-PALETTE` (the category
//! view sizes its ranked rows with [`rows_within`],
//! `drill-down-palette-categories` D3). It is shared
//! rather than placed beside either one, as `model/attention_refresh.rs` is:
//! `.agents/rules/workflow-convention.md` counts owning workflows, and a forked
//! copy of the admission or continuation rule could drift while both halves
//! still read as correct.
//!
//! It owns two decisions:
//!
//! - **Admission against a retained-byte budget.** [`RetainedBudget::admit`]
//!   retains an item only while the running charge stays within the limit, and
//!   the first refusal latches: every later item is refused too. What is
//!   retained is therefore always a **prefix** of the delivery order, so a list
//!   has no hole the user cannot see, "showing X of Y" stays true, and F4
//!   navigation stays contiguous.
//! - **Turn continuation.** [`turn_continues`] lets a GTK turn take one more
//!   unit of work while the turn's elapsed time is under
//!   [`LIST_TURN_SLICE_US`], and always lets it take the first, so every turn
//!   makes progress.
//!
//! It also owns the one wording helper those lists' "showing X of Y" notices
//! share, [`grouped_thousands`], so the three notices print counts alike.
//!
//! The clock is read by the GTK caller (`glib::monotonic_time`) and passed in,
//! so the module stays pure and integer only. Kani proves it
//! (`list_budget/kani_proofs.rs`, shard `core-list-budget`): B1 the charge
//! never exceeds the limit, B2 the retained items are a prefix and the refusal
//! latches, B3 no loss against a count cap (`limit >= n * w` retains any `n`
//! items of at most `w` bytes), B4 a counter-example one byte below that, B5
//! [`rows_within`] is the equal-charge prefix the budget retains, T1
//! every turn makes progress, and T2 a turn stops at the first unit started at
//! or after the slice, so the guarded work costs at most the slice plus one
//! unit.

#![forbid(clippy::float_arithmetic)]

use super::responsiveness_budget::B_INTERACT_US;

#[cfg(kani)]
mod kani_proofs;

/// The time one list turn may spend before it yields, microseconds.
///
/// Half the interaction budget: a turn leaves the other half of `B_INTERACT_US`
/// for the frame and the input that waits on it. The work already started when
/// the slice ends finishes, so a turn's guarded work costs at most the slice
/// plus one unit (Kani T2); each workflow bounds its unit by a count cap and
/// measures it.
///
/// Kind: out: time; `B_INTERACT_US / 2`, a time budget (`docs/thresholds.md`).
pub const LIST_TURN_SLICE_US: u64 = B_INTERACT_US / 2;

/// What [`RetainedBudget::admit`] decided for one item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Admission {
    /// The item fits: it is retained and its charge counted.
    Retained,
    /// The item, and every later one, is not retained.
    Limited,
}

/// A retained-byte budget with a latching refusal.
///
/// `charged` never exceeds `limit` (B1). The first refused item latches the
/// budget `Limited`, so the retained items are always a prefix of the order in
/// which they were offered (B2).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetainedBudget {
    limit: u64,
    charged: u64,
    limited: bool,
}

impl RetainedBudget {
    /// An empty budget that retains at most `limit` bytes.
    #[must_use]
    pub const fn new(limit: u64) -> Self {
        Self {
            limit,
            charged: 0,
            limited: false,
        }
    }

    /// Offer one item charging `charge` bytes.
    ///
    /// Retained only while nothing was refused before and
    /// `charged + charge <= limit` (a sum that would overflow `u64` is
    /// refused rather than wrapped).
    pub fn admit(&mut self, charge: u64) -> Admission {
        if self.limited {
            return Admission::Limited;
        }
        match self.charged.checked_add(charge) {
            Some(next) if next <= self.limit => self.charged = next,
            _ => {
                self.limited = true;
                return Admission::Limited;
            }
        }
        Admission::Retained
    }

    /// Bytes charged by the retained items.
    #[must_use]
    pub const fn charged(&self) -> u64 {
        self.charged
    }

    /// The budget's limit, bytes.
    #[must_use]
    pub const fn limit(&self) -> u64 {
        self.limit
    }

    /// Whether an item has been refused (the latch).
    #[must_use]
    pub const fn is_limited(&self) -> bool {
        self.limited
    }
}

/// Whether a list turn may start one more unit of work.
///
/// True for the first unit (`units_done == 0`, so every turn makes progress,
/// T1) and while `elapsed_us < LIST_TURN_SLICE_US`; false from the first unit
/// at or after the slice (T2).
#[must_use]
pub const fn turn_continues(elapsed_us: u64, units_done: usize) -> bool {
    units_done == 0 || elapsed_us < LIST_TURN_SLICE_US
}

/// Rows one unit of a sliced realized-row retirement removes
/// (`lighten-notes-browser-rows` D4): the rows left once the rows GTK never
/// realized are gone are its realized band (about 205, ledger A1), each torn
/// down at about 70 µs (measured in task 1.4), so a unit costs about 1.1 ms and
/// the slice plus one unit stays well within `B_INTERACT_US`.
///
/// Kind: cap; a power of two, rounded down.
pub const LIST_RETIREMENT_UNIT_ROWS: u32 = 16;

/// The rows of a list of `len` rows that nobody holds a row object for, given
/// the held positions (GTK's realized band, its focus row, the selection):
/// `(start, count)` ranges after the highest held row, between consecutive
/// held rows, and before the lowest, **from the last to the first**, so
/// removing them in this order keeps every earlier start valid
/// (`lighten-notes-browser-rows` D4). Removing rows GTK holds no object for
/// tears nothing down (ledger A110), so a retirement's first turn removes
/// them all in a few emissions and leaves only the held rows for its units.
/// With no held row the whole list is one range. Held positions at or past
/// `len` (a stale map) and repeats are ignored; `held` need not be sorted.
#[must_use]
pub fn unheld_ranges(len: u32, held: &[u32]) -> Vec<(u32, u32)> {
    let mut held: Vec<u32> = held
        .iter()
        .copied()
        .filter(|position| *position < len)
        .collect();
    held.sort_unstable();
    held.dedup();
    let mut ranges = Vec::with_capacity(held.len() + 1);
    for_each_unheld_range(len, &held, |start, count| ranges.push((start, count)));
    ranges
}

/// The core of [`unheld_ranges`]: calls `emit(start, count)` for each range,
/// last first, given held positions already strictly ascending and each below
/// `len`. It allocates nothing, so the Kani harness proves it directly: CBMC
/// cannot afford the standard library's sort or a growing `Vec` over symbolic
/// input, and the property tests cover `unheld_ranges`' filter, sort and
/// dedup instead.
pub fn for_each_unheld_range(len: u32, held: &[u32], mut emit: impl FnMut(u32, u32)) {
    let mut end = len;
    for &position in held.iter().rev() {
        let start = position + 1;
        if start < end {
            emit(start, end - start);
        }
        end = position;
    }
    if end > 0 {
        emit(0, end);
    }
}

/// Rows a lazy list's first publish of a source lists in the frame that
/// first shows it (`lighten-notes-browser-rows` D5): it covers the tallest
/// viewport the notes browser (a 700 sp dialog) and the palette (400 px of
/// results) allow, about 20 rows at the smallest row height, so the frame
/// realizes these rows only rather than GTK's whole band of about 205 (A1).
///
/// Kind: coverage window; a power of two, rounded up.
pub const LIST_FIRST_ROWS: u32 = 32;

/// Rows each later frame adds while a first publish grows towards
/// [`LIST_GROWTH_BAND_ROWS`]: each step realizes this many rows, measured at
/// about 0.16 ms of layout a row (task 6: 64 rows took 10.3–10.6 ms, which
/// with a GL frame's 3–6 ms of paint leaves no margin under `B_SETTLE_US`),
/// so a growth frame takes about 5 ms of layout.
///
/// Kind: cap; a power of two, rounded down.
pub const LIST_GROWTH_STEP_ROWS: u32 = 32;

/// Rows past which a growing first publish lists the rest in one emission:
/// GTK realizes about 205 rows (A1), so rows past this many are never
/// realized by the listing and cost no row widget.
///
/// Kind: coverage window; a power of two, rounded up.
pub const LIST_GROWTH_BAND_ROWS: u32 = 256;

/// Rows a first publish of `len` rows lists at once.
#[must_use]
pub const fn staged_first_rows(len: u32) -> u32 {
    if len < LIST_FIRST_ROWS {
        len
    } else {
        LIST_FIRST_ROWS
    }
}

/// The rows a growing first publish lists after one more frame, from
/// `listed` of `len`: [`LIST_GROWTH_STEP_ROWS`] more until
/// [`LIST_GROWTH_BAND_ROWS`], then every row; `None` once every row is
/// listed. From [`staged_first_rows`] it reaches `len` in at most eight
/// steps (Kani G1).
#[must_use]
pub const fn next_growth(listed: u32, len: u32) -> Option<u32> {
    if listed >= len {
        return None;
    }
    if listed >= LIST_GROWTH_BAND_ROWS {
        return Some(len);
    }
    let stepped = listed.saturating_add(LIST_GROWTH_STEP_ROWS);
    let capped = if stepped < LIST_GROWTH_BAND_ROWS {
        stepped
    } else {
        LIST_GROWTH_BAND_ROWS
    };
    Some(if capped < len { capped } else { len })
}

/// The length a list of `len` rows has after one more retirement unit: the
/// unit is removed from the end. `None` when nothing is left.
#[must_use]
pub const fn retirement_unit_cut(len: u32) -> Option<u32> {
    if len == 0 {
        None
    } else {
        Some(len.saturating_sub(LIST_RETIREMENT_UNIT_ROWS))
    }
}

/// The number of equal-charge rows a budget of `limit` bytes retains: the
/// largest `n` with `n * charge <= limit`.
///
/// It states [`RetainedBudget::admit`]'s prefix rule for equal charges, so a
/// list that ranks rows of one fixed charge (the command palette's category
/// view, `drill-down-palette-categories` D3) can size its cut without offering
/// every row. A `charge` of zero bounds nothing and returns `usize::MAX`; a
/// quotient above `usize::MAX` saturates. Kani B5 proves the zero-charge case
/// for every `u64` limit, the closed form (`r * charge <= limit < (r + 1) *
/// charge`) for every `u64` limit at seven concrete charges (1, 2, 3, 36,
/// 4,096, 2^32 + 1, `u64::MAX`), and its agreement with [`RetainedBudget`] for
/// every charge and results of at most four rows; a symbolic 64-bit charge in
/// the closed form did not finish (`kani_proofs.rs`).
#[must_use]
pub fn rows_within(limit: u64, charge: u64) -> usize {
    if charge == 0 {
        return usize::MAX;
    }
    usize::try_from(limit / charge).unwrap_or(usize::MAX)
}

/// `count` with comma thousands separators ("16,384"): how every
/// budget-limited list notice prints its counts ("Showing X of Y"), so the
/// workspace tree, workspace search and Browse Notes read alike.
#[must_use]
pub fn grouped_thousands(count: usize) -> String {
    let digits = count.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn counts_are_grouped_in_thousands() {
        assert_eq!(grouped_thousands(0), "0");
        assert_eq!(grouped_thousands(999), "999");
        assert_eq!(grouped_thousands(1_000), "1,000");
        assert_eq!(grouped_thousands(128 * 1024), "131,072");
        assert_eq!(grouped_thousands(1_234_567), "1,234,567");
    }

    #[test]
    fn rows_within_is_the_equal_charge_prefix() {
        assert_eq!(rows_within(0, 36), 0);
        assert_eq!(rows_within(35, 36), 0);
        assert_eq!(rows_within(36, 36), 1);
        assert_eq!(rows_within(71, 36), 1);
        assert_eq!(rows_within(72, 36), 2);
        assert_eq!(rows_within(8 * 1024 * 1024, 36), 233_016);
        assert_eq!(rows_within(u64::MAX, 1), usize::MAX);
    }

    #[test]
    fn a_zero_charge_bounds_nothing() {
        assert_eq!(rows_within(0, 0), usize::MAX);
        assert_eq!(rows_within(17, 0), usize::MAX);
    }

    #[test]
    fn the_slice_is_half_the_interaction_budget() {
        assert_eq!(LIST_TURN_SLICE_US, 4_000);
    }

    #[test]
    fn an_item_that_fits_exactly_is_retained() {
        let mut budget = RetainedBudget::new(10);
        assert_eq!(budget.admit(4), Admission::Retained);
        assert_eq!(budget.admit(6), Admission::Retained);
        assert_eq!(budget.charged(), 10);
        assert!(!budget.is_limited());
    }

    #[test]
    fn the_first_refusal_latches_even_for_a_later_small_item() {
        let mut budget = RetainedBudget::new(10);
        assert_eq!(budget.admit(8), Admission::Retained);
        assert_eq!(budget.admit(3), Admission::Limited);
        assert_eq!(budget.admit(1), Admission::Limited, "no best-fit hole");
        assert_eq!(budget.admit(0), Admission::Limited);
        assert_eq!(budget.charged(), 8);
        assert!(budget.is_limited());
    }

    #[test]
    fn a_saturating_charge_is_refused_not_wrapped() {
        let mut budget = RetainedBudget::new(u64::MAX - 1);
        assert_eq!(budget.admit(u64::MAX - 1), Admission::Retained);
        assert_eq!(budget.admit(u64::MAX), Admission::Limited);
        assert_eq!(budget.charged(), u64::MAX - 1);
    }

    #[test]
    fn a_zero_limit_retains_only_free_items() {
        let mut budget = RetainedBudget::new(0);
        assert_eq!(budget.admit(0), Admission::Retained);
        assert_eq!(budget.admit(1), Admission::Limited);
    }

    #[test]
    fn every_turn_takes_its_first_unit() {
        assert!(turn_continues(u64::MAX, 0));
        assert!(turn_continues(0, 0));
    }

    #[test]
    fn a_turn_stops_at_the_first_unit_at_or_after_the_slice() {
        assert!(turn_continues(LIST_TURN_SLICE_US - 1, 1));
        assert!(!turn_continues(LIST_TURN_SLICE_US, 1));
        assert!(!turn_continues(LIST_TURN_SLICE_US + 1, 7));
    }

    #[test]
    fn the_unheld_ranges_leave_only_the_held_rows() {
        // The realized band in the middle and the focus row at the top.
        let band: Vec<u32> = (5_000..=5_204).chain([0]).collect();
        assert_eq!(
            unheld_ranges(16 * 1024, &band),
            [(5_205, 16 * 1024 - 5_205), (1, 4_999)]
        );
        // A band at the top removes nothing before it.
        let top: Vec<u32> = (0..=204).collect();
        assert_eq!(unheld_ranges(16 * 1024, &top), [(205, 16 * 1024 - 205)]);
        // No held row: everything goes.
        assert_eq!(unheld_ranges(128 * 1024, &[]), [(0, 128 * 1024)]);
        // A stale position past the end is ignored, repeats too.
        assert_eq!(unheld_ranges(10, &[40, 3, 3]), [(4, 6), (0, 3)]);
        assert_eq!(unheld_ranges(0, &[0]), []);
    }

    #[test]
    fn a_first_publish_grows_to_the_band_then_lists_the_rest() {
        assert_eq!(staged_first_rows(10), 10);
        assert_eq!(staged_first_rows(16 * 1024), 32);
        let mut listed = staged_first_rows(16 * 1024);
        let mut steps = Vec::new();
        while let Some(next) = next_growth(listed, 16 * 1024) {
            steps.push(next);
            listed = next;
        }
        assert_eq!(steps, [64, 96, 128, 160, 192, 224, 256, 16 * 1024]);
        assert_eq!(next_growth(64, 80), Some(80));
        assert_eq!(next_growth(100, 100), None);
        assert_eq!(next_growth(300, 1_000), Some(1_000));
    }

    #[test]
    fn a_unit_removes_at_most_its_rows_from_the_end() {
        assert_eq!(LIST_RETIREMENT_UNIT_ROWS, 16);
        assert_eq!(retirement_unit_cut(205), Some(189));
        assert_eq!(retirement_unit_cut(16), Some(0));
        assert_eq!(retirement_unit_cut(3), Some(0));
        assert_eq!(retirement_unit_cut(0), None);
    }

    proptest! {
        /// The unheld ranges are disjoint, descending, hold no held row,
        /// and with the held rows cover the list.
        #[test]
        fn the_unheld_ranges_cover_every_unheld_row(
            len in 0u32..5_000,
            held in prop::collection::vec(0u32..6_000, 0..8),
        ) {
            let ranges = unheld_ranges(len, &held);
            let mut covered = 0u32;
            let mut previous_start = u32::MAX;
            for &(start, count) in &ranges {
                prop_assert!(count > 0 && start + count <= len);
                prop_assert!(start + count <= previous_start);
                previous_start = start;
                prop_assert!(!held.iter().any(|row| *row >= start && *row < start + count));
                covered += count;
            }
            let mut kept: Vec<u32> = held.iter().copied().filter(|row| *row < len).collect();
            kept.sort_unstable();
            kept.dedup();
            prop_assert_eq!(covered + u32::try_from(kept.len()).expect("few held rows"), len);
        }

        /// A first publish reaches the full length in at most eight growth
        /// steps, each listing more rows than the last.
        #[test]
        fn growth_reaches_every_row(len in 0u32..200_000) {
            let mut listed = staged_first_rows(len);
            let mut steps = 0;
            while let Some(next) = next_growth(listed, len) {
                prop_assert!(next > listed && next <= len);
                listed = next;
                steps += 1;
            }
            prop_assert_eq!(listed, len);
            prop_assert!(steps <= 8);
        }

        /// Units reach an empty list in `ceil(len / unit)` steps.
        #[test]
        fn units_empty_the_list(len in 0u32..5_000) {
            let mut left = len;
            let mut steps = 0u32;
            while let Some(next) = retirement_unit_cut(left) {
                prop_assert!(left - next <= LIST_RETIREMENT_UNIT_ROWS);
                left = next;
                steps += 1;
            }
            prop_assert_eq!(steps, len.div_ceil(LIST_RETIREMENT_UNIT_ROWS));
        }

        /// B1 and B2 over random sequences: the charge stays within the limit,
        /// the retained items are a prefix, and the latch holds.
        #[test]
        fn admission_keeps_a_prefix_within_the_limit(
            limit in 0u64..10_000,
            charges in prop::collection::vec(0u64..2_000, 0..64),
        ) {
            let mut budget = RetainedBudget::new(limit);
            let mut refused = false;
            let mut retained_sum = 0u64;
            for charge in charges {
                let admission = budget.admit(charge);
                prop_assert!(budget.charged() <= limit);
                if refused {
                    prop_assert_eq!(admission, Admission::Limited);
                }
                match admission {
                    Admission::Retained => retained_sum += charge,
                    Admission::Limited => refused = true,
                }
                prop_assert_eq!(budget.charged(), retained_sum);
                prop_assert_eq!(budget.is_limited(), refused);
            }
        }

        /// B5 on random values: `rows_within` agrees with offering equal
        /// charges to a `RetainedBudget`.
        #[test]
        fn rows_within_agrees_with_the_budget(
            limit in 0u64..20_000,
            charge in 1u64..2_000,
        ) {
            let rows = rows_within(limit, charge);
            let mut budget = RetainedBudget::new(limit);
            let mut retained = 0usize;
            while budget.admit(charge) == Admission::Retained {
                retained += 1;
            }
            prop_assert_eq!(rows, retained);
        }

        /// B3: no loss against a count cap at its worst charge.
        #[test]
        fn a_budget_of_n_times_w_retains_any_n_items_of_at_most_w(
            w in 0u64..100_000,
            charges in prop::collection::vec(0u64..100_000, 0..64),
        ) {
            let n = charges.len() as u64;
            let mut budget = RetainedBudget::new(n * w);
            for charge in charges {
                prop_assert_eq!(budget.admit(charge.min(w)), Admission::Retained);
            }
        }

        /// T1 and T2 over random unit costs: the loop the policy guards always
        /// makes progress, never starts a unit at or after the slice, and so
        /// costs at most the slice plus one unit.
        #[test]
        fn a_guarded_turn_costs_at_most_the_slice_plus_one_unit(
            costs in prop::collection::vec(0u64..10_000, 1..64),
        ) {
            let mut elapsed = 0u64;
            let mut done = 0usize;
            while done < costs.len() && turn_continues(elapsed, done) {
                elapsed += costs[done];
                done += 1;
            }
            prop_assert!(done >= 1);
            let largest = costs.iter().copied().max().unwrap_or(0);
            prop_assert!(elapsed < LIST_TURN_SLICE_US + largest);
            if done < costs.len() {
                prop_assert!(elapsed >= LIST_TURN_SLICE_US);
            }
        }
    }
}
