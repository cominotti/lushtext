// SPDX-License-Identifier: GPL-3.0-or-later

//! The Protected View fold: which characters of each paragraph stay visible.
//!
//! A protected document is installed with every paragraph's **tail** under an
//! invisible, non-editable tag, so GtkTextView never lays out more than a
//! bounded head of any paragraph, and no 2000 px validation batch (ledger
//! A31) lays out more than [`V_BATCH`] visible characters. This module is the
//! pure, GTK-free policy that decides the heads; the GTK executor
//! (`ui/protected_fold.rs`, `long-line-protected-view` task 3) inserts each
//! head with `insert` and each tail with `insert_with_tags(fold_tag)`.
//!
//! Two production stages, both named so a proof can call one alone:
//!
//! 1. [`length_cap`]: every paragraph shows at most `head_cap` characters,
//!    `head_cap = min(N_VISIBLE, N_HEAD, A28_SAFE_CHARS)` ([`HEAD_CAP`]).
//! 2. [`density_cap`]: a paragraph is **dense** when some window of `r_fold`
//!    consecutive paragraphs containing it has a length-capped sum over
//!    `v_batch` ([`window_is_over_budget`]); a dense paragraph shows
//!    [`dense_visible`] `= min(c, n_dense)` of its length-capped count `c`,
//!    every other paragraph shows `c`. No other paragraph is folded.
//!
//! # Streaming
//!
//! The policy is a step function over the paragraph lengths in document order
//! ([`fold_step`], then [`fold_finish`]), with state bounded by `r_fold`
//! ([`FoldState`]), independent of the document. A paragraph's decision needs
//! the windows that start up to `r_fold − 1` paragraphs after it, so the step
//! emits paragraph `s` when paragraph `s + r_fold − 1` arrives, and
//! [`fold_finish`] emits the rest. Only folded paragraphs produce a
//! [`FoldRange`]; the plan is one compact range per folded paragraph, whose
//! retained size is [`FoldRange::RETAINED_BYTES`] each, the figure an install
//! charges to its transient admission.
//!
//! The lengths are the ones C1's `ParagraphProfileAccumulator`
//! ([`paragraph_boundary`](super::paragraph_boundary)) closes: characters
//! (Unicode scalars, which `GtkTextIter` counts), delimiter excluded, the last
//! paragraph included (an empty document has one, empty, paragraph). A worker
//! that profiles a body therefore feeds each closed length, in the same pass,
//! to [`fold_step`] on one [`ProductionFoldState`] beside the profile's own
//! `ParagraphLengthFold`, collects the returned ranges, and calls
//! [`fold_finish`] when the accumulator finishes (`long-line-protected-view`
//! task 2.2 wires it; [`plan_folds`] is the same loop over a slice of
//! lengths). The plan is wrap-independent, so toggling word wrap never
//! re-folds.
//!
//! # Units
//!
//! Characters only; `r_fold` alone carries the pixel relationship (rows per
//! 2000 px validation batch). Integer-only, and on the whole-pixel table at
//! ceiling 0 (`.agents/rules/workflow-convention.md`).
//!
//! # Verification
//!
//! Kani proves the density bound, the window lemma, the production threshold
//! preconditions, the fold shape, the UTF-8 safety of [`fold_byte_offset`],
//! the column mapping and slicing independence (`protected_fold/kani_proofs.rs`,
//! shard `core-protected-fold`), and that `length_cap` alone breaks the
//! density bound; each harness states its domain. The property tests in
//! `crates/lushtext-core/tests/properties/protected_fold.rs` are the evidence
//! at large sizes and at the production thresholds.

#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]

#[cfg(kani)]
mod kani_proofs;

use super::document_scale::{L_PROTECT, N_VISIBLE, R_BATCH, V_BATCH};

/// **PROVISIONAL** (measured by `long-line-protected-view` task 1.3): the
/// largest visible head whose first layout, wrap on and wrap off, and whose
/// re-layout on a word-wrap toggle each fit `B_settle` per dispatch.
///
/// Until the lane measures it, this is design D1's linear extrapolation of
/// ledger A23 (a 200,000-character paragraph's first layout costs 177–217 ms
/// wrapped, so about 15,000 characters fit 16 ms). It is not the measured
/// value, and task 1.3 replaces it.
pub const N_HEAD: u64 = 15_000;

/// Pango's advance of the view's monospace font at the default zoom, in Pango
/// units (1/1024 px), as ledger A28 measured it (9011 units per character).
pub const A28_MEASURED_ADVANCE_UNITS: u64 = 9_011;

/// The largest zoom percentage the window offers (`ui/window/zoom.rs`'s
/// `ZOOM_MAX`, 400). Zoom scales the monospace font size, so it scales the
/// advance. A compile-time assertion in `zoom.rs` that its range stays within
/// `R_FOLD_MIN_ZOOM_PERCENT..=A28_MAX_ZOOM_PERCENT` belongs to the first
/// change of this programme that edits that GTK module (it needs the
/// accessibility and visual lanes).
pub const A28_MAX_ZOOM_PERCENT: u64 = 400;

/// **PROVISIONAL** (task 1.3 records the measured margin): the longest visible
/// head whose Pango x positions stay below the `int32` overflow of ledger A28
/// at the **largest** supported zoom.
///
/// A28 measured the overflow at 238,319 characters for the default advance
/// (`i32::MAX / 9011`). The overflow length shrinks as the advance grows, so
/// the binding zoom is the largest one, 400 %, where the advance is about
/// `4 · 9011 = 36,044` units and the overflow falls at
/// `floor(i32::MAX / 36,044)` = 59,579 characters (the smallest zoom only
/// lengthens it). The value assumes the advance scales linearly with the font
/// size; hinting can round a scaled advance up by a fraction of a pixel, which
/// task 1.3's measurement at 400 % settles. It is not the binding term of
/// [`HEAD_CAP`] today ([`N_HEAD`] is).
pub const A28_SAFE_CHARS: u64 =
    (i32::MAX as u64) / (A28_MEASURED_ADVANCE_UNITS * A28_MAX_ZOOM_PERCENT / 100);

/// Rows of one 2000 px validation batch the density window covers: C1's
/// [`R_BATCH`] (224), with no edge row added.
///
/// `long-line-protected-view` task 1.2: GtkTextView validates whole lines from
/// a line start until the batch reaches 2000 px (ledger A31: each step grows
/// `upper` by 2000 px plus at most one line, 2016 px = 112 × 18 px at the
/// default font), so a batch holds at most `ceil(2000 / h)` lines of minimum
/// row height `h`, the row straddling the 2000 px edge included: C0's 112 is
/// exactly `ceil(2000 / 18)`, and its double, 224, covers `ceil(2000 / 9)` =
/// 223 at the 50 % minimum zoom. Even a 2000 px window starting at an
/// arbitrary pixel (the widget test of task 3.4) touches at most
/// `ceil((2000 + h − 1) / h)` rows, 113 at 18 px and 224 at 9 px. So
/// `R_BATCH` already counts the partially covered edge row, and `R_FOLD + 1`
/// would only make `R_FOLD · N_DENSE` exceed `V_BATCH`. The argument rests on
/// a 9 px minimum row height at the 50 % zoom, the assumption C0's doubling
/// already made.
pub const R_FOLD: usize = R_BATCH;

/// The smallest zoom percentage [`R_FOLD`] covers (`ui/window/zoom.rs`'s
/// `ZOOM_MIN`, 50; see [`A28_MAX_ZOOM_PERCENT`] for the pending assertion).
pub const R_FOLD_MIN_ZOOM_PERCENT: u64 = 50;

/// Characters a dense paragraph keeps visible: `V_BATCH / R_BATCH` (250), so
/// `R_FOLD` dense paragraphs fill at most one batch.
// `R_BATCH` (224) is a usize that fits a u64, so the cast is lossless.
pub const N_DENSE: u64 = V_BATCH / (R_BATCH as u64);

/// The most characters any paragraph shows: `min(N_VISIBLE, N_HEAD,
/// A28_SAFE_CHARS)`.
pub const HEAD_CAP: u64 = min_u64(min_u64(N_VISIBLE, N_HEAD), A28_SAFE_CHARS);

/// The Line Inspector's window, in characters: C0's bound
/// `min(N_visible, L_protect_stall)` = 500 (`docs/next/long-line-documents.md`,
/// "Measured values"). It must not exceed `min(HEAD_CAP, L_PROTECT)`, checked
/// at compile time below and by Kani.
pub const N_INSPECT: u64 = 500;

/// The ring capacity of [`ProductionFoldState`]: the largest `r_fold`
/// [`FoldThresholds::new`] accepts.
pub const MAX_R_FOLD: usize = 256;

// The production thresholds must satisfy the preconditions: evaluating the
// constant here makes the build fail when a C1 constant, `N_HEAD` or
// `A28_SAFE_CHARS` breaks them.
const _: FoldThresholds = FoldThresholds::PRODUCTION;
const _: () = assert!(N_INSPECT <= min_u64(HEAD_CAP, L_PROTECT));
const _: () = assert!(R_FOLD <= MAX_R_FOLD);

const fn min_u64(a: u64, b: u64) -> u64 {
    if a < b { a } else { b }
}

/// The fold policy's thresholds, validated.
///
/// The preconditions every proof assumes are `n_dense ≥ 1`,
/// `n_dense ≤ head_cap`, `1 ≤ r_fold ≤ MAX_R_FOLD`,
/// `r_fold · n_dense ≤ v_batch` and `r_fold · head_cap ≤ u64::MAX` (checked
/// multiplications): the fourth makes a window of dense paragraphs fit one
/// batch, and the fifth lets a window's capped sum live in a `u64`, which
/// halves the adders the Kani models carry (with `u128` sums H1 did not
/// finish in 24 minutes).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FoldThresholds {
    head_cap: u64,
    n_dense: u64,
    v_batch: u64,
    r_fold: usize,
}

impl FoldThresholds {
    /// The shipped thresholds: [`HEAD_CAP`], [`N_DENSE`], [`V_BATCH`] and
    /// [`R_FOLD`]. A module-level constant evaluates it, so the build fails
    /// when they break the preconditions.
    pub const PRODUCTION: Self = match Self::new(HEAD_CAP, N_DENSE, V_BATCH, R_FOLD) {
        Some(thresholds) => thresholds,
        None => panic!("the production fold thresholds break the fold preconditions"),
    };

    /// The thresholds, or `None` unless `n_dense ≥ 1`, `n_dense ≤ head_cap`,
    /// `1 ≤ r_fold ≤ MAX_R_FOLD`, `r_fold · n_dense ≤ v_batch` and
    /// `r_fold · head_cap` fits a `u64`, both without overflow.
    #[must_use]
    pub const fn new(head_cap: u64, n_dense: u64, v_batch: u64, r_fold: usize) -> Option<Self> {
        if n_dense == 0 || n_dense > head_cap || r_fold == 0 || r_fold > MAX_R_FOLD {
            return None;
        }
        // `r_fold` is at most `MAX_R_FOLD`, so the cast is lossless.
        if (r_fold as u64).checked_mul(head_cap).is_none() {
            return None;
        }
        match (r_fold as u64).checked_mul(n_dense) {
            Some(dense_window) if dense_window <= v_batch => Some(Self {
                head_cap,
                n_dense,
                v_batch,
                r_fold,
            }),
            _ => None,
        }
    }

    /// The most characters any paragraph shows.
    #[must_use]
    pub const fn head_cap(self) -> u64 {
        self.head_cap
    }

    /// The characters a dense paragraph keeps visible.
    #[must_use]
    pub const fn n_dense(self) -> u64 {
        self.n_dense
    }

    /// The most visible characters one window may hold.
    #[must_use]
    pub const fn v_batch(self) -> u64 {
        self.v_batch
    }

    /// Consecutive paragraphs in one window.
    #[must_use]
    pub const fn r_fold(self) -> usize {
        self.r_fold
    }
}

/// Stage 1: the characters a paragraph of `paragraph_chars` may show at all,
/// `min(paragraph_chars, head_cap)`.
#[must_use]
pub const fn length_cap(paragraph_chars: u64, thresholds: FoldThresholds) -> u64 {
    min_u64(paragraph_chars, thresholds.head_cap)
}

/// Whether a window whose length-capped characters sum to `capped_sum` is over
/// the batch budget. A window holds at most `r_fold` capped counts, so the sum
/// fits a `u64` under the `r_fold · head_cap` precondition.
#[must_use]
pub const fn window_is_over_budget(capped_sum: u64, v_batch: u64) -> bool {
    capped_sum > v_batch
}

/// The characters a dense paragraph of `capped` length-capped characters
/// shows: `min(capped, n_dense)`.
#[must_use]
pub const fn dense_visible(capped: u64, n_dense: u64) -> u64 {
    min_u64(capped, n_dense)
}

/// Stage 2: the characters a paragraph shows, from its length-capped count and
/// whether it lies in an over-budget window.
#[must_use]
pub const fn density_cap(capped: u64, dense: bool, thresholds: FoldThresholds) -> u64 {
    if dense {
        dense_visible(capped, thresholds.n_dense)
    } else {
        capped
    }
}

/// One folded paragraph: its first `head_chars` characters stay visible and
/// the following `tail_chars` (up to, never including, its delimiter) carry
/// the fold tag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FoldRange {
    /// The paragraph's index in the document (GTK line number).
    pub paragraph: u64,
    /// Visible characters at the start of the paragraph.
    pub head_chars: u64,
    /// Hidden characters after the head; positive in every planned range.
    pub tail_chars: u64,
}

/// Where a column of a folded paragraph lies.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FoldLocation {
    /// A visible column (`column < head_chars`).
    Visible(u64),
    /// A column `head_chars + offset`, `offset ≤ tail_chars`: inside the
    /// hidden tail, or (`offset == tail_chars`) the paragraph end just before
    /// its delimiter. For a range with no tail, the paragraph end is
    /// `Hidden(0)`.
    Hidden(u64),
    /// Beyond the paragraph's end (`column > head_chars + tail_chars`).
    PastEnd,
}

impl FoldRange {
    /// Bytes one planned range retains, the per-range charge an install's
    /// transient admission takes for the plan.
    pub const RETAINED_BYTES: u64 = std::mem::size_of::<Self>() as u64;

    /// Where `column` (a character offset from the paragraph start) lies.
    ///
    /// Total over every `u64` column and every range, overflow-free: Kani
    /// proves (`fold_location_is_total_and_round_trips`), for any range whose
    /// `head_chars + tail_chars` fits a `u64` and any column, that the result
    /// is `Visible` iff `column < head_chars`, that a `Hidden` offset plus the
    /// head is the column, that [`FoldLocation::column_of`] maps it back for
    /// every column up to the paragraph length, and `PastEnd` iff the column
    /// is beyond it.
    #[must_use]
    pub const fn locate(self, column: u64) -> FoldLocation {
        if column < self.head_chars {
            return FoldLocation::Visible(column);
        }
        let offset = column - self.head_chars;
        if offset <= self.tail_chars {
            FoldLocation::Hidden(offset)
        } else {
            FoldLocation::PastEnd
        }
    }
}

impl FoldLocation {
    /// The column this location of `range` stands for, or `None` past the
    /// end (or for a hidden offset whose column would overflow a `u64`, which
    /// [`FoldRange::locate`] never returns).
    #[must_use]
    pub const fn column_of(self, range: FoldRange) -> Option<u64> {
        match self {
            Self::Visible(column) => Some(column),
            Self::Hidden(offset) => range.head_chars.checked_add(offset),
            Self::PastEnd => None,
        }
    }
}

/// The byte offset in `paragraph` (one paragraph's text, without its
/// delimiter) after its first `head_chars` characters: where the executor
/// splits the head from the tail. A `head_chars` at or beyond the paragraph's
/// character count is its byte length.
///
/// Always a scalar boundary: Kani proves (`fold_byte_offset_never_splits_a_scalar`),
/// for every valid UTF-8 string of at most 8 bytes with no paragraph
/// delimiter and every `head_chars` up to its character count, that the
/// offset is a char boundary and the prefix before it holds exactly
/// `head_chars` characters. Costs at most `4 · head_chars` byte reads, never
/// the paragraph's length.
#[must_use]
pub fn fold_byte_offset(paragraph: &str, head_chars: u64) -> usize {
    // A scalar starts at every byte that is not a UTF-8 continuation byte
    // (`10xxxxxx`); counting lead bytes avoids decoding, which also keeps the
    // Kani model small.
    let bytes = paragraph.as_bytes();
    let mut remaining = head_chars;
    let mut offset = 0;
    while offset < bytes.len() {
        if bytes[offset] & 0xC0 != 0x80 {
            if remaining == 0 {
                return offset;
            }
            remaining -= 1;
        }
        offset += 1;
    }
    bytes.len()
}

/// The streaming fold's state: the last paragraphs not yet emitted (at most
/// `r_fold − 1` between steps), the sum of their length-capped counts, the
/// next paragraph index, and the end of the paragraphs covered by an
/// over-budget window.
///
/// `CAPACITY` bounds the ring; production uses [`ProductionFoldState`]
/// (`MAX_R_FOLD`), and the Kani harnesses a ring of 4 so CBMC does not encode
/// a 256-element array. Each step touches O(1) ring entries (one written, at
/// most one evicted), so stepping costs the same for every `r_fold`.
#[derive(Clone, Debug)]
pub struct FoldState<const CAPACITY: usize> {
    thresholds: FoldThresholds,
    /// Raw paragraph lengths, oldest at `oldest`, in ring order.
    ring: [u64; CAPACITY],
    held: usize,
    oldest: usize,
    write: usize,
    /// Sum of the length-capped counts of the held paragraphs.
    capped_sum: u64,
    next_index: u64,
    /// Every paragraph below this index lies in an over-budget window that
    /// has completed (the end, exclusive, of the latest one).
    dense_until: u64,
}

/// The production fold state (ring capacity [`MAX_R_FOLD`]).
pub type ProductionFoldState = FoldState<MAX_R_FOLD>;

impl<const CAPACITY: usize> FoldState<CAPACITY> {
    /// An empty state (no paragraph seen), or `None` when `thresholds.r_fold`
    /// exceeds `CAPACITY`.
    #[must_use]
    pub const fn new(thresholds: FoldThresholds) -> Option<Self> {
        if thresholds.r_fold > CAPACITY {
            return None;
        }
        Some(Self {
            thresholds,
            ring: [0; CAPACITY],
            held: 0,
            oldest: 0,
            write: 0,
            capped_sum: 0,
            next_index: 0,
            dense_until: 0,
        })
    }

    /// The thresholds this state folds with.
    #[must_use]
    pub const fn thresholds(&self) -> FoldThresholds {
        self.thresholds
    }

    /// Paragraphs stepped so far.
    #[must_use]
    pub const fn paragraphs(&self) -> u64 {
        self.next_index
    }

    /// The length-capped sum of the held paragraphs: the running sum.
    #[cfg(not(kani))]
    const fn window_capped_sum(&self) -> u64 {
        self.capped_sum
    }

    /// The length-capped sum of the held paragraphs, added up from the ring in
    /// paragraph order: the form the Kani harnesses check.
    ///
    /// Production keeps the O(1) running sum above; recomputing costs
    /// `O(r_fold)` per step. Under Kani `r_fold` is at most 3, and the direct
    /// sum is the same expression the harnesses' own window sums build, where
    /// relating the running sum (one addition and one subtraction per step) to
    /// a direct sum kept H1 above 24 minutes. `running_sum_equals_direct_sum`
    /// and the property tests pin the two forms equal.
    #[cfg(kani)]
    fn window_capped_sum(&self) -> u64 {
        let mut sum = 0u64;
        let mut position = self.oldest;
        let mut taken = 0;
        while taken < self.held {
            sum += length_cap(self.ring[position], self.thresholds);
            position = self.advance(position);
            taken += 1;
        }
        sum
    }

    /// The direct sum of [`window_capped_sum`](Self::window_capped_sum)'s
    /// Kani form, for the tests that pin the running sum to it.
    #[cfg(test)]
    fn direct_capped_sum(&self) -> u64 {
        let mut sum = 0u64;
        let mut position = self.oldest;
        for _ in 0..self.held {
            sum += length_cap(self.ring[position], self.thresholds);
            position = self.advance(position);
        }
        sum
    }

    const fn advance(&self, position: usize) -> usize {
        let next = position + 1;
        if next == self.thresholds.r_fold {
            0
        } else {
            next
        }
    }

    /// Remove the oldest held paragraph and return its decision.
    fn emit_oldest(&mut self) -> Option<FoldRange> {
        let length = self.ring[self.oldest];
        // `held ≥ 1` and `next_index ≥ held`: the oldest held paragraph's index.
        let paragraph = self.next_index - self.held as u64;
        self.oldest = self.advance(self.oldest);
        self.held -= 1;
        let capped = length_cap(length, self.thresholds);
        self.capped_sum -= capped;
        let dense = paragraph < self.dense_until;
        let visible = density_cap(capped, dense, self.thresholds);
        (visible < length).then_some(FoldRange {
            paragraph,
            head_chars: visible,
            tail_chars: length - visible,
        })
    }
}

impl ProductionFoldState {
    /// An empty production-capacity state; every validated threshold fits it.
    #[must_use]
    pub const fn with_thresholds(thresholds: FoldThresholds) -> Self {
        match Self::new(thresholds) {
            Some(state) => state,
            // `FoldThresholds::new` bounds `r_fold` by `MAX_R_FOLD`, this
            // state's capacity.
            None => unreachable!(),
        }
    }
}

/// Step the fold with the next paragraph's length (characters, delimiter
/// excluded), returning the range of the paragraph it can now decide, if that
/// paragraph is folded.
///
/// Paragraph `s` is decided when paragraph `s + r_fold − 1` arrives; the last
/// `r_fold − 1` paragraphs (or a whole document shorter than one window) are
/// decided by [`fold_finish`]. Ranges come out in paragraph order.
///
/// **The density bound.** Every window of `r_fold` consecutive paragraphs (the
/// whole sequence when it is shorter) shows at most `v_batch` characters, and
/// every paragraph at most `head_cap`. For any window `W`: (1) if its
/// length-capped sum is at most `v_batch`, its visible sum, never above the
/// capped one, is too; (2) otherwise every paragraph of `W` is dense, so each
/// shows at most `n_dense` and `W` at most `r_fold · n_dense ≤ v_batch`; (3) a
/// 2000 px validation batch, in either wrap mode, covers at most `r_fold`
/// consecutive paragraphs, because every paragraph occupies at least one row
/// of at least the smallest row height ([`R_FOLD`]), so it lies inside some
/// window and holds at most `v_batch` visible characters. The plan reads no
/// wrap mode, so a wrap toggle never re-folds; task 3.4's widget test checks
/// the one-row-per-paragraph lemma against GtkTextView's laid-out rows in both
/// modes.
///
/// **Proved domain.** Kani proves the bound
/// (`density_bound_holds_for_every_window`), the fold shape
/// (`folds_hide_only_tails_and_only_where_needed`: head plus tail is the
/// length, a non-empty paragraph keeps at least `min(len, n_dense) ≥ 1`
/// characters, a paragraph of at most `head_cap` characters in no over-budget
/// window is never folded, ranges are ordered and disjoint) and slicing
/// independence (`fold_plan_is_independent_of_install_slicing`) over every
/// sequence of up to 6 paragraphs of arbitrary `u64` lengths and every
/// threshold tuple [`FoldThresholds::new`] accepts with `r_fold` from 1 to 3;
/// `an_over_budget_window_fits_once_dense` proves step (2) for any `r_fold ≤ 3`
/// capped lengths. Steps (1)–(2) do not depend on the sequence length or on
/// `r_fold`, so the bound extends to every length; step (3) is the toolkit
/// lemma above.
pub fn fold_step<const CAPACITY: usize>(
    state: &mut FoldState<CAPACITY>,
    paragraph_chars: u64,
) -> Option<FoldRange> {
    let thresholds = state.thresholds;
    state.ring[state.write] = paragraph_chars;
    state.write = state.advance(state.write);
    state.held += 1;
    // At most `r_fold` capped counts, each at most `head_cap`, and
    // `r_fold · head_cap` fits a `u64` (`FoldThresholds::new`).
    state.capped_sum += length_cap(paragraph_chars, thresholds);
    state.next_index = state.next_index.saturating_add(1);
    if state.held < thresholds.r_fold {
        return None;
    }
    // A full window: the `r_fold` paragraphs before `next_index`.
    if window_is_over_budget(state.window_capped_sum(), thresholds.v_batch) {
        state.dense_until = state.next_index;
    }
    state.emit_oldest()
}

/// Close the fold: decide every paragraph [`fold_step`] has not, and return
/// their ranges, in paragraph order, as an iterator (at most `r_fold − 1`
/// paragraphs, or the whole document when it is shorter than one window,
/// which then forms the one window).
#[must_use]
pub fn fold_finish<const CAPACITY: usize>(mut state: FoldState<CAPACITY>) -> FoldFinish<CAPACITY> {
    // The held paragraphs are the whole document when it is shorter than one
    // window, which is then its one window. Otherwise they are the tail of the
    // last full window, whose capped sum is at least theirs, so when they are
    // over budget that window was too and they are already dense: no
    // shortness test is needed.
    if window_is_over_budget(state.window_capped_sum(), state.thresholds.v_batch) {
        state.dense_until = state.next_index;
    }
    FoldFinish { state }
}

/// The ranges [`fold_finish`] decides, in paragraph order.
#[derive(Clone, Debug)]
pub struct FoldFinish<const CAPACITY: usize> {
    state: FoldState<CAPACITY>,
}

impl<const CAPACITY: usize> Iterator for FoldFinish<CAPACITY> {
    type Item = FoldRange;

    fn next(&mut self) -> Option<FoldRange> {
        while self.state.held > 0 {
            if let Some(range) = self.state.emit_oldest() {
                return Some(range);
            }
        }
        None
    }
}

/// The whole plan for a sequence of paragraph lengths: [`fold_step`] over each
/// length, then [`fold_finish`]. For tests and callers that already hold every
/// length; a worker streams instead.
#[must_use]
pub fn plan_folds(paragraph_chars: &[u64], thresholds: FoldThresholds) -> Vec<FoldRange> {
    let mut state = ProductionFoldState::with_thresholds(thresholds);
    let mut plan: Vec<FoldRange> = paragraph_chars
        .iter()
        .filter_map(|&length| fold_step(&mut state, length))
        .collect();
    plan.extend(fold_finish(state));
    plan
}

#[cfg(test)]
mod tests;
