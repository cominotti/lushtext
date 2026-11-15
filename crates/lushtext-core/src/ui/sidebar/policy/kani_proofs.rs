// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses for the sidebar's collapse step policy
//! (`slice-large-directory-collapse` design D9, shard `core-sidebar-collapse`).
//!
//! Domain: subtrees of the collapsing directory's store plus at most one
//! expanded descendant store, every `u32` store length up to 2^20 rows laid
//! out consistently (the descendant inside its parent's span), and two band
//! shapes:
//!
//! - **small bands** (C1–C4 and C5's unbound half): an optional held row, a
//!   run of at most [`SMALL_RUN`] held rows and an optional held row after it,
//!   anywhere in the list. That places held rows before, inside and after
//!   every range a step can pick, so every branch but the bound removal is
//!   reached: a bound removal needs a store whose every row is held, more
//!   than `DIRECT_COLLAPSE_ROWS` of them.
//! - **a held store** (C1, C2 and C5's bound half): the deepest store is
//!   exactly [`HELD_RUN`] rows, one more than `DIRECT_COLLAPSE_ROWS`, every
//!   one of them held, with an optional held row before and after it, so a
//!   bound removal takes one row. Bands with a held run placed independently
//!   of the stores did not fit: a symbolic run length up to 40 ran CBMC for
//!   16 minutes and out of a 14 GiB address space, and a run of exactly 33 or
//!   40 rows still ran past 11 minutes (2026-11-15,
//!   `docs/next/formal-verification.md`). A bound removal of more than one
//!   row, up to a whole unit (a held store of 64 rows or more), is covered by
//!   the property tests in `policy.rs`, not here.
//!
//! The GTK side reads the real band of about 207 rows (ledger A1) and sorts it
//! through `collapse_band`, which the property tests cover.

use super::{
    COLLAPSE_UNBOUND_CHUNK_ROWS, CollapseFacts, CollapseStep, CollapseStore, DIRECT_COLLAPSE_ROWS,
    SIDEBAR_REALIZED_ROWS_PER_FRAME, next_collapse_step,
};

/// Rows of a small band's contiguous held run.
const SMALL_RUN: u32 = 4;

/// The concrete held run of the bound harness: one row above
/// `DIRECT_COLLAPSE_ROWS`.
const HELD_RUN: u32 = DIRECT_COLLAPSE_ROWS + 1;

/// Held positions a band can hold: the held run and the two outliers.
const MAX_BAND: usize = HELD_RUN as usize + 2;

/// Rows per store: far past any directory the budget admits per band, small
/// enough that no sum overflows.
const MAX_ROWS: u32 = 1 << 20;

/// A consistent subtree: the directory at `directory`, its store from the next
/// row, and with `nested` a descendant store listed under the parent's row
/// `at`, one level down.
fn any_stores(nested: bool) -> ([CollapseStore; 2], usize) {
    let directory: u32 = kani::any();
    let length: u32 = kani::any();
    kani::assume(directory < MAX_ROWS && length < MAX_ROWS);
    let first_row = directory + 1;
    if !nested {
        let root = CollapseStore {
            depth: 0,
            first_row,
            length,
            span: length,
        };
        return ([root, root], 1);
    }
    let at: u32 = kani::any();
    let rows: u32 = kani::any();
    kani::assume(at < length && rows < MAX_ROWS);
    let root = CollapseStore {
        depth: 0,
        first_row,
        length,
        span: length + rows,
    };
    let child = CollapseStore {
        depth: 1,
        first_row: first_row + at + 1,
        length: rows,
        span: rows,
    };
    ([root, child], 2)
}

/// Strictly ascending held positions: an optional row, a run of `run_len`
/// rows from any start, and an optional row after it.
fn band_around(run_len: u32) -> ([u32; MAX_BAND], usize) {
    let mut band = [0u32; MAX_BAND];
    let mut len = 0;
    let before: Option<u32> = kani::any();
    let run_start: u32 = kani::any();
    let after: Option<u32> = kani::any();
    kani::assume(run_start < 3 * MAX_ROWS);
    if let Some(row) = before {
        kani::assume(row < run_start);
        band[len] = row;
        len += 1;
    }
    for offset in 0..run_len {
        band[len] = run_start + offset;
        len += 1;
    }
    if let Some(row) = after {
        kani::assume(row >= run_start + run_len && row < 4 * MAX_ROWS);
        band[len] = row;
        len += 1;
    }
    (band, len)
}

/// A small band: a run of at most [`SMALL_RUN`] rows and the two outliers.
fn any_small_band() -> ([u32; MAX_BAND], usize) {
    let run_len: u32 = kani::any();
    kani::assume(run_len <= SMALL_RUN);
    band_around(run_len)
}

fn held_within(band: &[u32], start: u32, count: u32) -> bool {
    band.iter()
        .any(|position| *position >= start && *position - start < count)
}

/// Held positions in `start..start + count`; the band is strictly ascending,
/// so `count` of them means every row of the range is held.
fn held_count(band: &[u32], start: u32, count: u32) -> u32 {
    let mut held = 0;
    for position in band {
        if *position >= start && *position - start < count {
            held += 1;
        }
    }
    held
}

/// C1 and C2 on a bound removal, C5's bound half: a bound removal takes the
/// head of a store whose every row is held, exactly `min(unit, rows above the
/// direct rows)` of them, and leaves the direct rows.
fn assert_bound_removal(
    stores: &[CollapseStore],
    band: &[u32],
    store: usize,
    start: u32,
    count: u32,
) {
    let facts = stores[store];
    assert!(count > 0 && count <= SIDEBAR_REALIZED_ROWS_PER_FRAME);
    assert!(start + count <= facts.length);
    assert!(facts.length - count >= DIRECT_COLLAPSE_ROWS);
    assert_eq!(start, 0);
    assert_eq!(
        count,
        (facts.length - DIRECT_COLLAPSE_ROWS).min(SIDEBAR_REALIZED_ROWS_PER_FRAME)
    );
    // Every row of the store is held: no unbound row was left.
    assert_eq!(
        held_count(band, facts.first_row, facts.length),
        facts.length
    );
}

/// C1, C2 on an unbound removal, C5's unbound half: it covers no held
/// position and at most one chunk, and takes the whole tail of its range up
/// to a chunk (the row after it ends the store or is held, and a short
/// removal starts at the store's start or right after a held row), so no
/// range after it is left unbound.
fn assert_unbound_removal(
    stores: &[CollapseStore],
    band: &[u32],
    store: usize,
    start: u32,
    count: u32,
) {
    let facts = stores[store];
    assert!(count > 0 && count <= COLLAPSE_UNBOUND_CHUNK_ROWS);
    assert!(start + count <= facts.length);
    assert!(!held_within(band, facts.first_row + start, count));
    let end = facts.first_row + start + count;
    assert!(
        start + count == facts.length || held_within(band, end, 1),
        "the removal ends the store or stops at a held row"
    );
    if count < COLLAPSE_UNBOUND_CHUNK_ROWS {
        assert!(
            start == 0 || held_within(band, facts.first_row + start - 1, 1),
            "a short removal takes its whole range"
        );
    }
}

/// C1, C2, C4 and C5's unbound half on one step of any consistent subtree
/// under a small band: the step makes progress (it removes at least one row
/// of the store it names, inside that store, or collapses a descendant or the
/// directory); an unbound removal covers no held position, at most one chunk,
/// and the whole tail of its range; and the directory collapses only once its
/// subtree is at most `DIRECT_COLLAPSE_ROWS` rows. Every step but the last
/// therefore strictly lowers "rows listed plus stores listed", and a collapse
/// ends within `ceil(unbound rows / chunk)` chunk steps per range plus the
/// bound steps, the bound design D2 states.
#[kani::proof]
#[kani::unwind(8)]
fn every_step_makes_bounded_progress() {
    let nested: bool = kani::any();
    let (stores, count) = any_stores(nested);
    let stores = &stores[..count];
    let (band, band_len) = any_small_band();
    let band = &band[..band_len];
    match next_collapse_step(&CollapseFacts { stores, band }) {
        CollapseStep::CollapseRow => {
            // C4.
            assert!(stores[0].span <= DIRECT_COLLAPSE_ROWS);
        }
        CollapseStep::CollapseDescendant(index) => {
            assert!(index > 0 && index < stores.len());
            assert!(stores[index].span <= DIRECT_COLLAPSE_ROWS);
            kani::cover!(true, "a drained descendant collapses");
        }
        CollapseStep::RemoveUnbound {
            store,
            start,
            count,
        } => {
            assert_unbound_removal(stores, band, store, start, count);
            kani::cover!(true, "an unbound removal");
            kani::cover!(count == COLLAPSE_UNBOUND_CHUNK_ROWS, "a full chunk");
        }
        CollapseStep::RemoveBound {
            store,
            start,
            count,
        } => assert_bound_removal(stores, band, store, start, count),
    }
}

/// C3: while a descendant store is listed, no step removes rows from, or
/// collapses, its ancestor.
#[kani::proof]
#[kani::unwind(8)]
fn a_descendant_goes_before_its_ancestor() {
    let (stores, count) = any_stores(true);
    let stores = &stores[..count];
    let (band, band_len) = any_small_band();
    let band = &band[..band_len];
    match next_collapse_step(&CollapseFacts { stores, band }) {
        CollapseStep::RemoveUnbound { store, .. } | CollapseStep::RemoveBound { store, .. } => {
            assert_eq!(store, 1);
        }
        CollapseStep::CollapseDescendant(index) => assert_eq!(index, 1),
        // The whole subtree is small: one direct call removes both.
        CollapseStep::CollapseRow => assert!(stores[0].span <= DIRECT_COLLAPSE_ROWS),
    }
}

/// C1, C2 and C5's bound half: the deepest store of any consistent subtree,
/// [`HELD_RUN`] rows long with every row held (and a held row before and
/// after it or not), loses exactly one row, the head of its store, in a bound
/// removal: `min(unit, rows above the direct rows)`, leaving the direct rows.
/// Each bound step of a held store therefore takes what its bound allows.
#[kani::proof]
#[kani::unwind(36)]
fn each_step_takes_what_its_bound_allows() {
    let nested: bool = kani::any();
    let (stores, count) = any_stores(nested);
    let stores = &stores[..count];
    let deepest = count - 1;
    let target = stores[deepest];
    kani::assume(target.length == HELD_RUN);
    let mut band = [0u32; MAX_BAND];
    let mut len = 0;
    let before: Option<u32> = kani::any();
    let after: Option<u32> = kani::any();
    if let Some(row) = before {
        kani::assume(row < target.first_row);
        band[len] = row;
        len += 1;
    }
    for offset in 0..HELD_RUN {
        band[len] = target.first_row + offset;
        len += 1;
    }
    if let Some(row) = after {
        kani::assume(row >= target.first_row + HELD_RUN);
        band[len] = row;
        len += 1;
    }
    let band = &band[..len];
    let step = next_collapse_step(&CollapseFacts { stores, band });
    assert_eq!(
        step,
        CollapseStep::RemoveBound {
            store: deepest,
            start: 0,
            count: (HELD_RUN - DIRECT_COLLAPSE_ROWS).min(SIDEBAR_REALIZED_ROWS_PER_FRAME),
        }
    );
    if let CollapseStep::RemoveBound {
        store,
        start,
        count,
    } = step
    {
        assert_bound_removal(stores, band, store, start, count);
    }
    kani::cover!(nested, "a held descendant store");
}
