// SPDX-License-Identifier: GPL-3.0-or-later

//! The sliced install plan of a Protected View body
//! (`sliced-protected-install`, C3, design D5).
//!
//! C2 installs a protected body with each folded paragraph's tail hidden at
//! insertion (`ui/protected_fold.rs`), but a tail longer than the slice budget
//! went in in **one** GTK turn: 23–28 ms for a 1.9M-character tail, 1.3 s for
//! 100 MB, against `B_SETTLE_US`. This module is the GTK-free planner that
//! lets a hidden tail go in over several turns:
//!
//! - a [`SliceKind::Visible`] slice holds whole paragraphs and ends on a
//!   paragraph boundary (or at a folded paragraph's start), exactly as the
//!   ordinary slicer [`next_slice_boundary`](super::buffer_replacement::next_slice_boundary) cuts them;
//! - a [`SliceKind::Head`] slice is a folded paragraph's visible head, up to
//!   C2's fold point (at most `HEAD_CAP` characters). The executor installs it
//!   **after** the paragraph's tail, at a left-gravity mark at the paragraph
//!   start (ledger A76), so no tail slice re-lays out the head (A77);
//! - [`SliceKind::HiddenTail`] slices cut the tail at most
//!   [`ProtectedInstallLimits::tail_slice_bytes`] apart, rounded down to a
//!   scalar boundary. A slice never contains a delimiter, so the visible
//!   paragraph structure is unchanged by slicing.
//!
//! The executor separates consecutive tail slices with left-gravity marks
//! (A76), so the btree never merges and copies them (A36), and records them as
//! the boundary ledger the clear plan ([`clear_ranges_for_turn`]) follows,
//! paragraph by paragraph from each paragraph's start.
//!
//! # Domain of the proofs
//!
//! Kani (`protected_install/kani_proofs.rs`) proves the clear plan (K7),
//! C2's fold of one paragraph of any `u64` length (K8b), the slice-budget
//! derivation's freedom from panics and its admissibility (K10), and,
//! `should_panic`, that removing the rounding guard splits a scalar (K11),
//! in shard `core-protected-install`. The planner's whole-plan properties
//! (K1–K6, K8, K9: progress, exact bytes, safe slice ends, homogeneity, the
//! slice bounds, the visible/hidden switch, the head's end, the slice count)
//! are proved by a one-step induction (shards `core-protected-install-step`
//! and `core-protected-install-step-visible`)
//! over every document of **at most 8 bytes** of `a`, `é`, `€`, `𝄞`, `\r`,
//! `\n` and U+2029, every fold plan of up to two ranges in paragraph order
//! with `head_chars` 0..=3, every tail budget of 4..=7 bytes and every
//! visible budget of 1..=8 bytes with 1..=3 characters or none: the initial
//! cursor is well-formed, and one call from any well-formed cursor returns a
//! slice with those properties and a well-formed cursor. Outside that domain
//! (production budgets, multi-megabyte tails) they are property-tested
//! (`tests/properties/protected_install.rs`); that file lists which
//! properties it asserts.

#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]

#[cfg(kani)]
mod kani_proofs;

use super::buffer_replacement::{
    PacedSlice, ParagraphLength, REPLACEMENT_INSERT_SLICE_BYTES, SliceBudget,
};
use super::paragraph_boundary::next_paragraph_break;
use super::protected_fold::{FoldRange, fold_byte_offset};

/// The most UTF-8 bytes one scalar takes.
pub const MAX_UTF8_SCALAR_BYTES: usize = 4;

/// The smallest tail or clear slice [`ProtectedInstallLimits::new`] accepts:
/// one scalar of any length, so every slice advances.
///
/// Kind: external fact (UTF-8's longest scalar), kept exact.
pub const MIN_TAIL_SLICE_BYTES: usize = MAX_UTF8_SCALAR_BYTES;

/// The largest slice [`ProtectedInstallLimits::for_settle_budget`] derives:
/// the ordinary install slice, `REPLACEMENT_INSERT_SLICE_BYTES` (256 KiB).
///
/// Kind: cap, a power of two.
pub const MAX_SLICE_BYTES: usize = REPLACEMENT_INSERT_SLICE_BYTES;

/// The settle budget the production slices are derived from: `B_SETTLE_US`.
pub const B_SETTLE_US: u64 = super::responsiveness_budget::B_SETTLE_US;

/// What one KiB of hidden tail costs to insert in one GTK turn with the
/// accessibility bridge on and a listener attached, in nanoseconds: the
/// slowest rate C2's verdict measured for the one-turn tail insert
/// (`docs/next/long-line-documents.md`, "C2 … Verdict": a 1.9M-character tail
/// at 31–49 ms with AT on, which is at most 26 µs per KiB, the plain rate
/// being 12–15 µs per KiB, and the 100 MB install 1.28–1.37 s, 13 µs per
/// KiB). Kind: measured cost, kept as measured.
pub const PROTECTED_TAIL_COST_NS_PER_KIB: u64 = 26_000;

/// What one KiB of hidden tail costs to delete in one GTK turn, in
/// nanoseconds, AT on: the A76 probe deletes a range between two boundary
/// marks in a time that does not grow with the range's neighbours, and the
/// lane's sliced clear measured at most this rate (C3 verdict). Kind: measured
/// cost, kept as measured.
pub const PROTECTED_CLEAR_COST_NS_PER_KIB: u64 = 26_000;

/// The share of `B_SETTLE_US` a slice's own cost may take, as a margin over
/// that cost in percent: 100 % keeps half of the budget for the rest of the
/// dispatch (the load completion, the line validation the slice causes), as
/// `SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES` does.
pub const PROTECTED_SLICE_MARGIN_PERCENT: u64 = 100;

/// Bytes of hidden tail one install turn inserts.
///
/// [`ProtectedInstallLimits::for_settle_budget`] over `B_SETTLE_US`,
/// [`PROTECTED_TAIL_COST_NS_PER_KIB`] and a 100 % margin gives 315,076 bytes,
/// above [`MAX_SLICE_BYTES`], so the cap binds: 256 KiB (a unit test pins it).
/// Kind: cap, a power of two.
pub const PROTECTED_TAIL_SLICE_BYTES: usize = 256 * 1024;

/// Bytes of hidden tail one clear turn deletes (derived like
/// [`PROTECTED_TAIL_SLICE_BYTES`], from [`PROTECTED_CLEAR_COST_NS_PER_KIB`]).
/// Kind: cap, a power of two.
pub const PROTECTED_CLEAR_SLICE_BYTES: usize = 256 * 1024;

/// Bytes of a hidden tail one install turn scans for the paragraph's end
/// before the tail's first slice goes in (`paragraph_content_end`).
///
/// The executor inserts a sliced paragraph's delimiter before its tail, so
/// the paragraph is never the buffer's last line while its tail grows: GTK
/// maps a point below the layout to the last line's end, and with a hidden
/// run on that line every drawn frame walks it (ledger A77; 182 s of stalls
/// installing 100 MB with the delimiter last). A memchr scan covers 32 MiB in
/// a few milliseconds.
///
/// Kind: cap, a power of two.
pub const PROTECTED_TAIL_SCAN_BYTES: usize = 32 * 1024 * 1024;

/// Where the paragraph whose content continues at `from` ends: `Ok` with
/// its content end (the first paragraph delimiter at or after `from`, or the
/// text's end), when it lies within `limit` bytes; otherwise `Err` with the
/// offset the next scan resumes from.
///
/// # Errors
///
/// The resume offset, when no paragraph end lies within `limit` bytes.
pub fn paragraph_content_end(text: &str, from: usize, limit: usize) -> Result<usize, usize> {
    let bytes = text.as_bytes();
    let window_end = from.saturating_add(limit).min(bytes.len());
    match first_delimiter_in(bytes, from, window_end) {
        Some(end) => Ok(end),
        None if window_end == bytes.len() => Ok(bytes.len()),
        // A U+2029 may straddle the window's end: resume two bytes back.
        None => Err(window_end.saturating_sub(2).max(from + 1)),
    }
}

/// The production limits.
pub const PRODUCTION_LIMITS: ProtectedInstallLimits =
    match ProtectedInstallLimits::new(PROTECTED_TAIL_SLICE_BYTES, PROTECTED_CLEAR_SLICE_BYTES) {
        Some(limits) => limits,
        None => panic!("the production protected slice sizes are below MIN_TAIL_SLICE_BYTES"),
    };

/// Whether a body hiding `hidden_chars` characters must be installed through
/// the sliced executor: when its hidden bytes, at most four per character,
/// could exceed one [`PROTECTED_TAIL_SLICE_BYTES`] slice. A smaller body's
/// tails fit one tail slice, so a direct install stays within the same bound.
#[must_use]
pub const fn hidden_tail_requires_slicing(hidden_chars: u64) -> bool {
    hidden_chars.saturating_mul(MAX_UTF8_SCALAR_BYTES as u64) > PROTECTED_TAIL_SLICE_BYTES as u64
}

/// No slice size of at least [`MIN_TAIL_SLICE_BYTES`] fits the settle budget
/// at the given cost.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SliceBudgetUnattainable;

/// The tail and clear slice sizes of a protected install, validated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtectedInstallLimits {
    tail_slice_bytes: usize,
    clear_slice_bytes: usize,
}

impl ProtectedInstallLimits {
    /// The limits, or `None` when either size is below
    /// [`MIN_TAIL_SLICE_BYTES`].
    #[must_use]
    pub const fn new(tail_slice_bytes: usize, clear_slice_bytes: usize) -> Option<Self> {
        if tail_slice_bytes < MIN_TAIL_SLICE_BYTES || clear_slice_bytes < MIN_TAIL_SLICE_BYTES {
            return None;
        }
        Some(Self {
            tail_slice_bytes,
            clear_slice_bytes,
        })
    }

    /// The most bytes one hidden-tail slice holds.
    #[must_use]
    pub const fn tail_slice_bytes(self) -> usize {
        self.tail_slice_bytes
    }

    /// The most tail bytes one clear turn deletes (at least one recorded
    /// range, whatever its size).
    #[must_use]
    pub const fn clear_slice_bytes(self) -> usize {
        self.clear_slice_bytes
    }

    /// The largest slice, in bytes, whose predicted cost plus
    /// `margin_percent` of it fits `settle_budget_us`, capped at
    /// [`MAX_SLICE_BYTES`]; `Err` when even [`MIN_TAIL_SLICE_BYTES`] does not
    /// fit.
    ///
    /// The predicted cost of `b` bytes is `ceil(b · cost_ns_per_kib / 1024)`
    /// ns, and the margin multiplies it by `(100 + margin_percent) / 100`,
    /// rounded up. Integer-only and saturating: Kani proves
    /// (`settle_budget_derivation_fits_and_never_overflows`, every `u64`
    /// input) that it never panics and that an `Ok` value is within
    /// [`MIN_TAIL_SLICE_BYTES`]`..=`[`MAX_SLICE_BYTES`] and accepted by
    /// [`Self::new`]; that an `Ok` value fits the budget and is the largest
    /// that does (or the cap), and that `Err` occurs exactly when
    /// [`MIN_TAIL_SLICE_BYTES`] does not fit, is property-tested over every
    /// `u64` input.
    /// It does not prove the cost is what GTK pays; the lane measures that.
    ///
    /// # Errors
    ///
    /// [`SliceBudgetUnattainable`] when no slice of at least
    /// [`MIN_TAIL_SLICE_BYTES`] fits the budget at this cost.
    pub const fn for_settle_budget(
        settle_budget_us: u64,
        cost_ns_per_kib: u64,
        margin_percent: u64,
    ) -> Result<usize, SliceBudgetUnattainable> {
        let budget_ns = settle_budget_us.saturating_mul(1000) as u128;
        // Cost with margin, in ns·KiB⁻¹ scaled by 100: `bytes` fit when
        // `ceil(ceil(bytes · c / 1024) · (100 + m) / 100) ≤ budget`.
        let cap = MAX_SLICE_BYTES as u128;
        let c = cost_ns_per_kib as u128;
        let m = 100 + margin_percent as u128;
        // The largest whole ns cost with margin that fits: `x` with
        // `ceil(x · m / 100) ≤ budget`, i.e. `x ≤ floor(budget · 100 / m)`.
        let max_cost = budget_ns * 100 / m;
        // The largest bytes with `ceil(bytes · c / 1024) ≤ max_cost`, i.e.
        // `bytes · c ≤ max_cost · 1024`.
        // A zero cost lets any slice fit: the cap.
        let bytes = match (max_cost * 1024).checked_div(c) {
            Some(candidate) if candidate < cap => candidate,
            _ => cap,
        };
        if bytes < MIN_TAIL_SLICE_BYTES as u128 {
            Err(SliceBudgetUnattainable)
        } else {
            // `bytes` is at most `cap`, a `usize`.
            Ok(bytes as usize)
        }
    }
}

/// What one slice of a protected install is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SliceKind {
    /// Whole visible paragraphs, inserted untagged where the install appends.
    Visible,
    /// A folded paragraph's visible head. The executor marks the paragraph's
    /// start here and inserts the head there once the tail is in.
    Head,
    /// Part of a folded paragraph's tail, inserted with the fold tag and
    /// followed by a boundary mark.
    HiddenTail,
}

/// One planned slice: the text from the previous slice's end to `end`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtectedSlice {
    /// Byte offset just after the slice.
    pub end: usize,
    /// What the slice is.
    pub kind: SliceKind,
    /// For a hidden-tail slice, whether it ends its paragraph's tail.
    pub last_of_tail: bool,
}

/// Where the planner is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    /// At the start of paragraph `paragraph`.
    ParagraphStart,
    /// Inside a folded paragraph's tail.
    InTail,
    /// At the delimiter of a folded paragraph (its content is installed).
    AtDelimiter,
}

/// The planner's position in one body: a small `Copy` value an install
/// session keeps between turns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtectedInstallCursor {
    offset: usize,
    paragraph: u64,
    next_range: usize,
    phase: Phase,
}

impl Default for ProtectedInstallCursor {
    fn default() -> Self {
        Self::new()
    }
}

impl ProtectedInstallCursor {
    /// The start of a body.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            offset: 0,
            paragraph: 0,
            next_range: 0,
            phase: Phase::ParagraphStart,
        }
    }

    /// Bytes planned so far.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// Whether the planner is inside a hidden tail.
    #[must_use]
    pub const fn in_tail(&self) -> bool {
        matches!(self.phase, Phase::InTail)
    }
}

/// The rounding production applies to a hidden-tail slice end: down to a
/// scalar boundary, never below `floor`, and never between `\r` and `\n`.
fn round_down_to_safe_boundary(bytes: &[u8], floor: usize, candidate: usize) -> usize {
    let mut end = candidate.min(bytes.len());
    while end > floor
        && end < bytes.len()
        && (is_continuation(bytes[end]) || (bytes[end] == b'\n' && bytes[end - 1] == b'\r'))
    {
        end -= 1;
    }
    end
}

const fn is_continuation(byte: u8) -> bool {
    byte & 0xC0 == 0x80
}

/// The first byte in `bytes[from..to]` that starts a GTK paragraph delimiter
/// (`\n`, `\r`, or U+2029), read byte by byte so a slice end inside a scalar
/// cannot panic here.
fn first_delimiter_in(bytes: &[u8], from: usize, to: usize) -> Option<usize> {
    let to = to.min(bytes.len());
    let mut index = from;
    while index < to {
        #[cfg(not(kani))]
        {
            index += memchr::memchr3(b'\n', b'\r', 0xE2, &bytes[index..to])?;
        }
        match bytes[index] {
            b'\n' | b'\r' => return Some(index),
            0xE2 if bytes.get(index + 1) == Some(&0x80) && bytes.get(index + 2) == Some(&0xA9) => {
                return Some(index);
            }
            _ => index += 1,
        }
    }
    None
}

/// The next slice of `text`, folded by `ranges` (C2's plan, in paragraph
/// order), or `None` once the body is planned. Visible slices follow
/// `budget` exactly as [`next_slice_boundary`](super::buffer_replacement::next_slice_boundary) does for a body without folds;
/// hidden-tail slices are at most `limits.tail_slice_bytes()` long.
///
/// K1–K6, K8 and K9 are proved by the one-step induction over the domain
/// stated in the module docs (documents of at most 8 bytes), and
/// property-tested beyond it.
#[must_use]
pub fn plan_protected_install(
    text: &str,
    ranges: &[FoldRange],
    cursor: &mut ProtectedInstallCursor,
    budget: SliceBudget,
    limits: ProtectedInstallLimits,
) -> Option<ProtectedSlice> {
    plan_with(
        text,
        ranges,
        cursor,
        budget,
        limits,
        round_down_to_safe_boundary,
    )
}

/// [`plan_protected_install`] with the tail rounding as a parameter, so the
/// `should_panic` harness can remove the guard through the same stepper.
/// Production passes only [`round_down_to_safe_boundary`].
fn plan_with(
    text: &str,
    ranges: &[FoldRange],
    cursor: &mut ProtectedInstallCursor,
    budget: SliceBudget,
    limits: ProtectedInstallLimits,
    rounding: fn(&[u8], usize, usize) -> usize,
) -> Option<ProtectedSlice> {
    let bytes = text.as_bytes();
    loop {
        if cursor.offset >= bytes.len() {
            return None;
        }
        match cursor.phase {
            Phase::InTail => {
                let start = cursor.offset;
                let window_end = start
                    .saturating_add(limits.tail_slice_bytes)
                    .min(bytes.len());
                if let Some(content_end) = first_delimiter_in(bytes, start, window_end) {
                    cursor.phase = Phase::AtDelimiter;
                    if content_end == start {
                        continue;
                    }
                    cursor.offset = content_end;
                    return Some(ProtectedSlice {
                        end: content_end,
                        kind: SliceKind::HiddenTail,
                        last_of_tail: true,
                    });
                }
                let end = if window_end == bytes.len() {
                    window_end
                } else {
                    rounding(bytes, start, window_end)
                };
                let last = end == bytes.len() || first_delimiter_in(bytes, end, end + 1).is_some();
                if last {
                    cursor.phase = Phase::AtDelimiter;
                }
                cursor.offset = end;
                return Some(ProtectedSlice {
                    end,
                    kind: SliceKind::HiddenTail,
                    last_of_tail: last,
                });
            }
            Phase::ParagraphStart
                if ranges
                    .get(cursor.next_range)
                    .is_some_and(|range| range.paragraph == cursor.paragraph) =>
            {
                let range = ranges[cursor.next_range];
                let start = cursor.offset;
                // Walks at most the head's `head_chars` scalars.
                let head_end = start + fold_byte_offset(&text[start..], range.head_chars);
                // A plan that does not match the text never moves a slice
                // past the paragraph's delimiter.
                let head_end = first_delimiter_in(bytes, start, head_end).unwrap_or(head_end);
                cursor.next_range += 1;
                cursor.phase = Phase::InTail;
                cursor.offset = head_end;
                return Some(ProtectedSlice {
                    end: head_end,
                    kind: SliceKind::Head,
                    last_of_tail: false,
                });
            }
            Phase::ParagraphStart | Phase::AtDelimiter => {
                return Some(visible_slice(text, ranges, cursor, budget));
            }
        }
    }
}

/// The Unicode scalars in `bytes` (valid UTF-8): its bytes that are not
/// continuation bytes. A plain byte loop, which the Kani harnesses can unwind
/// (`str::chars().count()` reads words at unaligned addresses).
fn scalar_count(bytes: &[u8]) -> u64 {
    let mut count = 0;
    for &byte in bytes {
        if !is_continuation(byte) {
            count += 1;
        }
    }
    count
}

/// Whole paragraphs from the cursor under `budget`, stopping before the next
/// folded paragraph. At a folded paragraph's delimiter the delimiter counts as
/// that paragraph's last bytes.
fn visible_slice(
    text: &str,
    ranges: &[FoldRange],
    cursor: &mut ProtectedInstallCursor,
    budget: SliceBudget,
) -> ProtectedSlice {
    let next_fold = ranges.get(cursor.next_range).map(|range| range.paragraph);
    let mut slice = PacedSlice::new(budget);
    let mut end = cursor.offset;
    let mut paragraph = cursor.paragraph;
    let mut at_delimiter = cursor.phase == Phase::AtDelimiter;
    while end < text.len() {
        if !at_delimiter && Some(paragraph) == next_fold && slice.paragraphs() > 0 {
            break;
        }
        let (content_end, next_start) = next_paragraph_break(text, end)
            .map_or((text.len(), text.len()), |found| {
                (found.content_end, found.next_start)
            });
        let length = ParagraphLength {
            bytes: (next_start - end) as u64,
            chars: scalar_count(&text.as_bytes()[end..content_end]),
        };
        if !slice.admit(length) {
            break;
        }
        end = next_start;
        if next_start > content_end {
            paragraph += 1;
        }
        at_delimiter = false;
    }
    cursor.offset = end;
    cursor.paragraph = paragraph;
    cursor.phase = Phase::ParagraphStart;
    ProtectedSlice {
        end,
        kind: SliceKind::Visible,
        last_of_tail: false,
    }
}

/// How many of the recorded ranges of one protected paragraph, counted from
/// its first, one clear turn deletes: as many as fit
/// `limits.clear_slice_bytes()` together, and at least one. `ranges` holds
/// each range's bytes in buffer order.
///
/// From the first, because deleting a range costs time that grows with the
/// text before it in its paragraph and not with the text after it (ledger
/// A76: 6–42 µs per 64 KiB range from the start, against 46 µs growing to
/// 600 µs from the end of an 8 MiB paragraph).
#[must_use]
pub fn clear_ranges_for_turn(ranges: &[u64], limits: ProtectedInstallLimits) -> usize {
    let budget = limits.clear_slice_bytes as u64;
    let mut taken = 0;
    let mut bytes = 0u64;
    for size in ranges {
        let next = bytes.saturating_add(*size);
        if taken > 0 && next > budget {
            break;
        }
        bytes = next;
        taken += 1;
    }
    taken
}

#[cfg(test)]
mod tests;
