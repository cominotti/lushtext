// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over the sliced protected install planner
//! (`sliced-protected-install`, design D5): K7, K8b, K10 and K11, and the
//! one-step induction that proves K1–K6, K8 and K9
//! (`close-protected-install-proof-strength`, designs D1–D3).
//!
//! **K7** ranges over up to [`MAX_RANGES`] recorded ranges of 0..=15 bytes and
//! a clear budget of 4..=7. **K8b** over every `u64` paragraph length and
//! density parameters that cannot bind on one paragraph. **K10** over every
//! `u64` input. **K11** (`should_panic`) over one undelimited 12-byte
//! paragraph of one 2-, 3- or 4-byte scalar, `head_cap` 1..=3 and a tail
//! budget of 4..=7.
//!
//! # The step harnesses (`step_*`)
//!
//! Domain: every document of at most [`DOC_BYTES`] bytes of whole scalars of
//! `a`, `é`, `€`, `𝄞`, `\r`, `\n` and U+2029 (so `\r\n` too), every fold
//! plan of up to [`STEP_RANGES`] ranges in strictly increasing paragraph order
//! with `head_chars` 0..=3 (a superset of C2's plans; the planner does not
//! read `tail_chars`), every tail budget of 4..=7 bytes and every visible
//! budget of 1..=8 bytes with 1..=3 characters or none.
//!
//! [`well_formed`] (design D2) states which cursors a plan can reach: a safe
//! offset, the paragraph count that matches it, the ranges before
//! `next_range` behind it and the rest ahead, and per phase a paragraph start
//! (`ParagraphStart`), a point of a folded paragraph's tail at or after its
//! head (`InTail`), or that paragraph's content end (`AtDelimiter`). It reads
//! the text with its own delimiter scanner, not `next_paragraph_break` or
//! `first_delimiter_in`.
//!
//! The predicate reads the text through [`Document`], tables the harness
//! builds in one pass over the bytes (the paragraph, start, safety, content
//! end and content scalars of every point), so a symbolic offset is looked
//! up rather than rescanned: the rescanning form took 27 local minutes and
//! up to 6.2 GiB, the tables 15 minutes and at most 4.5 GiB.
//!
//! The induction: `step_initial_cursor_is_well_formed` proves that
//! `ProtectedInstallCursor::new()` is well-formed and that a well-formed
//! cursor at the text's end plans nothing. The three step harnesses prove that
//! one call of the production `plan_protected_install` from any well-formed
//! cursor with text left returns a slice that ends at a safe point (K3), is
//! homogeneous (K4), meets its bound (K5, hence K9; a visible slice is also
//! maximal: it stops only at the text's end, before the next fold, or when
//! its next unit would exceed the budget) and its phase rules (K6, K8), and
//! leaves a well-formed cursor at `slice.end` (K2). The hidden-tail
//! step has two harnesses: a tail slice, and the `continue` into a visible
//! slice when the cursor sits on the tail's delimiter. A step either
//! advances the offset (K1) or, for an empty head, moves the phase from
//! `ParagraphStart` to `InTail`, so `(offset, phase)` decreases
//! lexicographically towards the end and every plan terminates. Every plan
//! of the domain is therefore a sequence of proved steps from offset 0 to the
//! text's end: the properties hold for all of them, not for a sample.
//!
//! The whole-plan form these replace (every step of a plan unrolled at once)
//! did not fit CBMC: at 10, 6 and 4 bytes it exhausted 16 GiB or ran past 20
//! minutes (`docs/next/formal-verification.md`, "Sliced protected install").
//! The property tests (`tests/properties/protected_install.rs`) still cover
//! production budgets and multi-megabyte tails, which this domain does not.
//!
//! Compiled only under `cfg(kani)` (`make kani`, shards
//! `core-protected-install`, `core-protected-install-step` and
//! `core-protected-install-step-visible`).

use super::{
    MAX_SLICE_BYTES, MIN_TAIL_SLICE_BYTES, Phase, ProtectedInstallCursor, ProtectedInstallLimits,
    ProtectedSlice, SliceBudgetUnattainable, SliceKind, clear_ranges_for_turn,
    plan_protected_install, plan_with,
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

// ---------------------------------------------------------------------------
// One-step harnesses (close-protected-install-proof-strength, design D1-D3).
// ---------------------------------------------------------------------------

/// The document domain of the step harnesses: at most this many bytes.
const DOC_BYTES: usize = 8;
/// The points of a document: every offset from 0 to [`DOC_BYTES`].
const POINTS: usize = DOC_BYTES + 1;
/// The fold plans' ranges.
const STEP_RANGES: usize = 2;

/// C3's alphabet: one scalar of every UTF-8 length and every GTK paragraph
/// delimiter (`\r` and `\n` also form `\r\n`).
const SCALARS: [&[u8]; 7] = [
    b"a",
    "\u{e9}".as_bytes(),
    "\u{20ac}".as_bytes(),
    "\u{1d11e}".as_bytes(),
    b"\r",
    b"\n",
    "\u{2029}".as_bytes(),
];

/// The byte length of the GTK paragraph delimiter starting at `at`, or 0.
/// The harnesses' own reading, independent of `next_paragraph_break` and
/// `first_delimiter_in`.
fn delimiter_length(bytes: &[u8], at: usize) -> usize {
    match bytes[at] {
        b'\n' => 1,
        b'\r' if at + 1 < bytes.len() && bytes[at + 1] == b'\n' => 2,
        b'\r' => 1,
        0xE2 if at + 2 < bytes.len() && bytes[at + 1] == 0x80 && bytes[at + 2] == 0xA9 => 3,
        _ => 0,
    }
}

/// Any document of the domain, read once into per-point tables, so the
/// predicates below look a symbolic offset up instead of rescanning the
/// text for it.
struct Document {
    bytes: [u8; DOC_BYTES],
    len: usize,
    /// The length of the delimiter starting at each point, or 0.
    delimiter: [u8; POINTS],
    /// The delimiters wholly before each point: the paragraph it lies in.
    paragraph: [u8; POINTS],
    /// Whether each point starts a paragraph.
    starts: [bool; POINTS],
    /// Whether each point is a safe slice end: the text's end, or a scalar
    /// boundary that is not between `\r` and `\n`.
    safe: [bool; POINTS],
    /// The start of the paragraph each point lies in.
    paragraph_start: [u8; POINTS],
    /// The content end (its delimiter's start, or the text's end) of the
    /// paragraph each content point lies in.
    content_end: [u8; POINTS],
    /// The content scalars (delimiters excluded) before each point.
    chars: [u8; POINTS],
}

impl Document {
    /// Any string of at most [`DOC_BYTES`] bytes of whole [`SCALARS`].
    fn any() -> Self {
        let mut bytes = [0u8; DOC_BYTES];
        let mut len = 0;
        let mut step = 0;
        while step < DOC_BYTES {
            let more: bool = kani::any();
            if !more {
                break;
            }
            let choice: usize = kani::any();
            kani::assume(choice < SCALARS.len());
            let scalar = SCALARS[choice];
            kani::assume(len + scalar.len() <= DOC_BYTES);
            let mut index = 0;
            while index < scalar.len() {
                bytes[len + index] = scalar[index];
                index += 1;
            }
            len += scalar.len();
            step += 1;
        }
        Self::read(bytes, len)
    }

    fn read(bytes: [u8; DOC_BYTES], len: usize) -> Self {
        let mut document = Self {
            bytes,
            len,
            delimiter: [0; POINTS],
            paragraph: [0; POINTS],
            starts: [false; POINTS],
            safe: [false; POINTS],
            paragraph_start: [0; POINTS],
            content_end: [0; POINTS],
            chars: [0; POINTS],
        };
        let text = &bytes[..len];
        let mut paragraph = 0u8;
        let mut start = 0u8;
        // Bytes of the current delimiter still to come.
        let mut inside = 0;
        let mut chars = 0u8;
        let mut at = 0;
        while at < POINTS {
            document.paragraph[at] = paragraph;
            document.paragraph_start[at] = start;
            document.chars[at] = chars;
            document.starts[at] = at <= len && inside == 0 && at == usize::from(start);
            document.safe[at] = at == len || (at < len && inside == 0 && text[at] & 0xC0 != 0x80);
            if at < len {
                if inside > 0 {
                    inside -= 1;
                    if inside == 0 {
                        paragraph += 1;
                        start = at as u8 + 1;
                    }
                } else {
                    let length = delimiter_length(text, at);
                    document.delimiter[at] = length as u8;
                    if length == 0 {
                        if text[at] & 0xC0 != 0x80 {
                            chars += 1;
                        }
                    } else if length == 1 {
                        paragraph += 1;
                        start = at as u8 + 1;
                    } else {
                        inside = length - 1;
                    }
                }
            }
            at += 1;
        }
        let mut end = len as u8;
        let mut at = POINTS;
        while at > 0 {
            at -= 1;
            if at >= len {
                end = len as u8;
            } else if document.delimiter[at] > 0 {
                end = at as u8;
            }
            document.content_end[at] = end;
        }
        document
    }

    /// The length of the delimiter starting at `at`, or 0.
    fn delimiter_at(&self, at: usize) -> usize {
        usize::from(self.delimiter[at])
    }

    /// The paragraph `at` lies in.
    fn paragraph_of(&self, at: usize) -> u64 {
        u64::from(self.paragraph[at])
    }

    /// The start of the paragraph `at` lies in.
    fn paragraph_start_of(&self, at: usize) -> usize {
        usize::from(self.paragraph_start[at])
    }

    /// The content end of the paragraph `at` lies in.
    fn content_end_of(&self, at: usize) -> usize {
        usize::from(self.content_end[at])
    }

    /// The content scalars in `from..to` (delimiters excluded).
    fn chars_between(&self, from: usize, to: usize) -> u64 {
        u64::from(self.chars[to] - self.chars[from])
    }

    fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    /// Where a folded paragraph starting at `start` shows its head up to:
    /// after `head_chars` scalars, or at the content end if that comes first.
    fn head_end(&self, start: usize, head_chars: u64) -> usize {
        let end = self.content_end_of(start);
        let mut found = end;
        let mut at = POINTS;
        while at > 0 {
            at -= 1;
            // The first byte of the (`head_chars` + 1)-th scalar: a lead
            // byte, since a scalar's continuation bytes count the same.
            if at >= start
                && at < end
                && self.bytes[at] & 0xC0 != 0x80
                && self.chars_between(start, at) == head_chars
            {
                found = at;
            }
        }
        found
    }

    /// The end of the visible unit at the paragraph start or delimiter `at`:
    /// past the paragraph's delimiter, or the text's end.
    fn unit_end(&self, at: usize) -> usize {
        let content_end = self.content_end_of(at);
        content_end + self.delimiter_at(content_end)
    }
}

/// Any fold plan: up to [`STEP_RANGES`] ranges in strictly increasing
/// paragraph order, `head_chars` 0..=3. A superset of C2's plans; the planner
/// does not read `tail_chars`.
fn any_fold_plan() -> ([FoldRange; STEP_RANGES], usize) {
    let count: usize = kani::any();
    kani::assume(count <= STEP_RANGES);
    let mut ranges = [FoldRange {
        paragraph: 0,
        head_chars: 0,
        tail_chars: 1,
    }; STEP_RANGES];
    let mut index = 0;
    while index < count {
        let paragraph: u64 = kani::any();
        let head_chars: u64 = kani::any();
        kani::assume(paragraph <= DOC_BYTES as u64 && head_chars <= 3);
        if index > 0 {
            kani::assume(paragraph > ranges[index - 1].paragraph);
        }
        ranges[index] = FoldRange {
            paragraph,
            head_chars,
            tail_chars: 1,
        };
        index += 1;
    }
    (ranges, count)
}

/// Any visible-slice budget: 1..=8 bytes, and 1..=3 characters or none.
fn any_step_budget() -> SliceBudget {
    let bytes: u64 = kani::any();
    let chars: u64 = kani::any();
    let limited: bool = kani::any();
    kani::assume((1..=8).contains(&bytes) && (1..=3).contains(&chars));
    SliceBudget {
        bytes,
        chars: if limited { chars } else { u64::MAX },
    }
}

/// Any tail budget of 4..=7 bytes.
fn any_tail_limits() -> ProtectedInstallLimits {
    let tail: usize = kani::any();
    kani::assume((4..=7).contains(&tail));
    ProtectedInstallLimits::new(tail, MIN_TAIL_SLICE_BYTES).unwrap()
}

/// Design D2: the cursors the induction ranges over.
///
/// 1. `offset` is a safe point of the text; 2. `paragraph` counts the
/// delimiters wholly before it; 3. the ranges before `next_range` are behind
/// the cursor (or are the folded paragraph being finished, in `InTail` and
/// `AtDelimiter`) and the rest ahead of it; 4. `ParagraphStart` is at a
/// paragraph start (or the text's end), `InTail` in the content of the
/// folded paragraph `ranges[next_range − 1]` at or after its head's end, and
/// `AtDelimiter` at that paragraph's content end.
fn well_formed(document: &Document, ranges: &[FoldRange], cursor: &ProtectedInstallCursor) -> bool {
    let offset = cursor.offset;
    if offset > document.len || !document.safe[offset] {
        return false;
    }
    let paragraph = document.paragraph_of(offset);
    if cursor.paragraph != paragraph || cursor.next_range > ranges.len() {
        return false;
    }
    let finishing = matches!(cursor.phase, Phase::InTail | Phase::AtDelimiter);
    let mut index = 0;
    while index < ranges.len() {
        let range = ranges[index].paragraph;
        let placed = if index < cursor.next_range {
            range < paragraph || (finishing && range == paragraph)
        } else {
            range > paragraph || (!finishing && range == paragraph)
        };
        if !placed {
            return false;
        }
        index += 1;
    }
    match cursor.phase {
        Phase::ParagraphStart => document.starts[offset] || offset == document.len,
        Phase::InTail | Phase::AtDelimiter => {
            if cursor.next_range == 0 || ranges[cursor.next_range - 1].paragraph != paragraph {
                return false;
            }
            if cursor.phase == Phase::AtDelimiter {
                return document.content_end_of(offset) == offset;
            }
            let start = document.paragraph_start_of(offset);
            offset >= document.head_end(start, ranges[cursor.next_range - 1].head_chars)
        }
    }
}

/// One step's inputs: a document, a fold plan, a well-formed cursor in
/// `phase` with text left to plan, and the budgets.
struct Step {
    document: Document,
    ranges: [FoldRange; STEP_RANGES],
    count: usize,
    cursor: ProtectedInstallCursor,
    budget: SliceBudget,
    limits: ProtectedInstallLimits,
}

impl Step {
    fn any(phase: Phase) -> Self {
        let document = Document::any();
        let (ranges, count) = any_fold_plan();
        let offset: usize = kani::any();
        let next_range: usize = kani::any();
        kani::assume(offset < document.len && next_range <= count);
        let cursor = ProtectedInstallCursor {
            offset,
            paragraph: document.paragraph_of(offset),
            next_range,
            phase,
        };
        kani::assume(well_formed(&document, &ranges[..count], &cursor));
        Self {
            document,
            ranges,
            count,
            cursor,
            budget: any_step_budget(),
            limits: any_tail_limits(),
        }
    }

    fn ranges(&self) -> &[FoldRange] {
        &self.ranges[..self.count]
    }

    /// The production planner, one call.
    fn plan(&self) -> (ProtectedSlice, ProtectedInstallCursor) {
        let mut after = self.cursor;
        let slice = plan_protected_install(
            as_text(self.document.bytes()),
            self.ranges(),
            &mut after,
            self.budget,
            self.limits,
        );
        // K2: a slice whenever text is left.
        assert!(slice.is_some());
        (slice.unwrap(), after)
    }

    /// What every step guarantees: K2's contiguity, K3, and the induction.
    fn check_common(&self, slice: ProtectedSlice, after: &ProtectedInstallCursor) {
        assert!(after.offset == slice.end);
        assert!(slice.end >= self.cursor.offset && slice.end <= self.document.len);
        assert!(self.document.safe[slice.end]);
        assert!(well_formed(&self.document, self.ranges(), after));
    }

    /// K1 and K4–K6 of a hidden-tail slice.
    fn check_hidden_tail(&self, slice: ProtectedSlice, after: &ProtectedInstallCursor) {
        let document = &self.document;
        let start = self.cursor.offset;
        let length = slice.end - start;
        let budget = self.limits.tail_slice_bytes();
        assert!(slice.kind == SliceKind::HiddenTail);
        assert!(slice.end > start);
        // K4: no delimiter byte, inside the folded paragraph being finished,
        // and `last_of_tail` exactly at its content end.
        let tail_end = document.content_end_of(start);
        assert!(slice.end <= tail_end);
        assert!(after.next_range == self.cursor.next_range);
        assert!(after.paragraph == self.ranges()[after.next_range - 1].paragraph);
        assert!(slice.last_of_tail == (slice.end == tail_end));
        // K5.
        assert!(length <= budget);
        if slice.last_of_tail {
            assert!(after.phase == Phase::AtDelimiter);
        } else {
            assert!(length + 3 >= budget);
            // K6.
            assert!(after.phase == Phase::InTail);
        }
        kani::cover!(!slice.last_of_tail && length + 1 == budget);
        kani::cover!(!slice.last_of_tail && length + 2 == budget);
        kani::cover!(!slice.last_of_tail && length + 3 == budget);
        kani::cover!(slice.last_of_tail && length == budget);
        kani::cover!(slice.last_of_tail && slice.end == document.len);
        kani::cover!(slice.last_of_tail && slice.end < document.len);
        kani::cover!(
            start + budget < document.len
                && document.bytes[start + budget] == b'\n'
                && document.bytes[start + budget - 1] == b'\r'
        );
    }

    /// K1, K4 and K5 of a visible slice, and its maximality.
    fn check_visible(&self, slice: ProtectedSlice, after: &ProtectedInstallCursor) {
        let document = &self.document;
        let start = self.cursor.offset;
        assert!(slice.kind == SliceKind::Visible && !slice.last_of_tail);
        assert!(slice.end > start);
        // K4: whole paragraphs, ending at a paragraph start or the text's end.
        let last = document.paragraph_of(slice.end);
        let ends_at_start = document.starts[slice.end];
        assert!(slice.end == document.len || ends_at_start);
        assert!(after.phase == Phase::ParagraphStart && after.paragraph == last);
        assert!(after.next_range == self.cursor.next_range);
        // No folded content: every paragraph whose content the slice holds is
        // before the next folded paragraph.
        let next_fold = self
            .ranges()
            .get(self.cursor.next_range)
            .map(|range| range.paragraph);
        if let Some(fold) = next_fold {
            assert!(last < fold || (last == fold && ends_at_start));
        }
        // K5: within the budget, unless the slice is exactly one unit (from
        // `AtDelimiter`, the finished paragraph's delimiter).
        let length = (slice.end - start) as u64;
        let chars = document.chars_between(start, slice.end);
        let one_unit = slice.end == document.unit_end(start);
        assert!(one_unit || (length <= self.budget.bytes && chars <= self.budget.chars));
        // Maximal, as the ordinary slicer cuts: a slice that stops before the
        // text's end and before the next fold could not take its next unit
        // (`PacedSlice::admit` refuses exactly when either total would exceed
        // its budget). Added after proof strength (task 3.3): without it, a
        // planner that cut one paragraph per slice, or overcounted a unit's
        // bytes, satisfied every other assertion.
        if slice.end < document.len && next_fold != Some(last) {
            let next = document.unit_end(slice.end);
            let next_bytes = (next - start) as u64;
            let next_chars = document.chars_between(start, next);
            assert!(next_bytes > self.budget.bytes || next_chars > self.budget.chars);
        }
        if slice.end < document.len {
            let before_fold = next_fold == Some(last);
            let fits_bytes = (document.unit_end(slice.end) - start) as u64 <= self.budget.bytes;
            let cut = start + self.budget.bytes as usize;
            kani::cover!(before_fold);
            kani::cover!(!before_fold && !fits_bytes);
            kani::cover!(!before_fold && fits_bytes);
            kani::cover!(
                cut < document.len
                    && document.bytes[cut] == b'\n'
                    && document.bytes[cut - 1] == b'\r'
            );
        }
        kani::cover!(slice.end == document.len);
    }
}

/// The induction's base: the cursor a session starts with is well-formed
/// for every document and fold plan of the domain, and a well-formed cursor
/// at the text's end plans nothing (K2's `None`), in every phase.
#[kani::proof]
#[kani::unwind(10)]
fn step_initial_cursor_is_well_formed() {
    let document = Document::any();
    let (ranges, count) = any_fold_plan();
    assert!(well_formed(
        &document,
        &ranges[..count],
        &ProtectedInstallCursor::new()
    ));
    let phase = match kani::any::<u8>() {
        0 => Phase::ParagraphStart,
        1 => Phase::InTail,
        _ => Phase::AtDelimiter,
    };
    let next_range: usize = kani::any();
    kani::assume(next_range <= count);
    let len = document.len;
    let mut end = ProtectedInstallCursor {
        offset: len,
        paragraph: document.paragraph_of(len),
        next_range,
        phase,
    };
    kani::assume(well_formed(&document, &ranges[..count], &end));
    let planned = plan_protected_install(
        as_text(document.bytes()),
        &ranges[..count],
        &mut end,
        any_step_budget(),
        any_tail_limits(),
    );
    assert!(planned.is_none());
    kani::cover!(len > 0 && phase == Phase::InTail);
}

/// The hidden-tail step, from any well-formed `InTail` cursor not at the
/// tail's end: a tail slice (K1, K3–K6).
#[kani::proof]
#[kani::unwind(10)]
fn step_hidden_tail_slices_are_safe_and_bounded() {
    let step = Step::any(Phase::InTail);
    kani::assume(step.document.delimiter_at(step.cursor.offset) == 0);
    let (slice, after) = step.plan();
    step.check_common(slice, &after);
    step.check_hidden_tail(slice, &after);
}

/// The hidden-tail step from a well-formed `InTail` cursor at the tail's end
/// (a delimiter): the planner's `continue` into the visible slice that starts
/// with the folded paragraph's delimiter (K1, K3–K5).
#[kani::proof]
#[kani::unwind(10)]
fn step_hidden_tail_end_continues_into_a_visible_slice() {
    let step = Step::any(Phase::InTail);
    kani::assume(step.document.delimiter_at(step.cursor.offset) > 0);
    let (slice, after) = step.plan();
    step.check_common(slice, &after);
    step.check_visible(slice, &after);
}

/// The head step, from any well-formed `ParagraphStart` cursor at the next
/// folded paragraph: the head ends at the `head_chars`-th scalar or the
/// content end (K8), holds no delimiter (K4), and is followed by the tail.
#[kani::proof]
#[kani::unwind(10)]
fn step_head_ends_at_the_fold_point() {
    let step = Step::any(Phase::ParagraphStart);
    let next = step.cursor.next_range;
    kani::assume(next < step.count && step.ranges[next].paragraph == step.cursor.paragraph);
    let (slice, after) = step.plan();
    step.check_common(slice, &after);
    let document = &step.document;
    let start = step.cursor.offset;
    assert!(slice.kind == SliceKind::Head && !slice.last_of_tail);
    assert!(slice.end == document.head_end(start, step.ranges[next].head_chars));
    assert!(slice.end <= document.content_end_of(start));
    // K1: an empty head still moves the phase forward.
    assert!(after.phase == Phase::InTail && after.next_range == next + 1);
    assert!(after.paragraph == step.cursor.paragraph);
    kani::cover!(slice.end == start);
    kani::cover!(slice.end > start && slice.end < document.content_end_of(start));
    kani::cover!(slice.end > start && slice.end == document.content_end_of(start));
}

/// The visible step, from any well-formed `ParagraphStart` cursor not at the
/// next folded paragraph, or any well-formed `AtDelimiter` cursor: whole
/// paragraphs under the budget (or exactly one unit), none folded (K1, K4,
/// K5).
#[kani::proof]
#[kani::unwind(10)]
fn step_visible_slices_hold_whole_unfolded_paragraphs() {
    let at_delimiter: bool = kani::any();
    let step = Step::any(if at_delimiter {
        Phase::AtDelimiter
    } else {
        Phase::ParagraphStart
    });
    let next = step.cursor.next_range;
    kani::assume(
        at_delimiter || next == step.count || step.ranges[next].paragraph != step.cursor.paragraph,
    );
    let (slice, after) = step.plan();
    step.check_common(slice, &after);
    step.check_visible(slice, &after);
    kani::cover!(at_delimiter && slice.end < step.document.len);
}
