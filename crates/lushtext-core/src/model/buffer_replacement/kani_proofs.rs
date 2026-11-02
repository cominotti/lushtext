// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over the two-budget slice-end decision, [`PacedSlice`]
//! (`bound-post-open-settle`, design D6, review R15).
//!
//! Domain: every sequence of at most [`MAX_PARAGRAPHS`] paragraphs with
//! symbolic `(bytes, visible characters)` lengths, and every pair of budgets.
//! The harnesses cut the whole sequence into slices the way the installers
//! do, one [`PacedSlice`] per slice, and check every slice. The text scanner
//! that turns a document into those lengths is `paragraph_boundary`'s, pinned
//! by C1's per-character harnesses and its Pango agreement test, not here.
//!
//! Compiled only under `cfg(kani)` (`make kani`, shard
//! `core-post-open-settle`).

use super::{PacedSlice, ParagraphLength, SliceBudget, V_SLICE, batch_exceeds_slice};
use crate::model::paragraph_boundary::ProfileSummary;

/// The longest paragraph sequence the harnesses range over.
const MAX_PARAGRAPHS: usize = 6;

fn any_paragraphs() -> ([ParagraphLength; MAX_PARAGRAPHS], usize) {
    let lengths: [ParagraphLength; MAX_PARAGRAPHS] = core::array::from_fn(|_| ParagraphLength {
        bytes: kani::any(),
        chars: kani::any(),
    });
    let count: usize = kani::any();
    kani::assume(count <= MAX_PARAGRAPHS);
    (lengths, count)
}

fn any_budget() -> SliceBudget {
    SliceBudget {
        bytes: kani::any(),
        chars: kani::any(),
    }
}

/// Cut `paragraphs` into slices under `budget`, checking each slice with
/// `check(slice, first, end)` (`first..end` are the paragraph indices it
/// holds), and return how many paragraphs and bytes the slices covered.
fn cut(
    paragraphs: &[ParagraphLength],
    budget: SliceBudget,
    mut check: impl FnMut(&PacedSlice, usize, usize),
) -> (usize, u64) {
    let mut first = 0;
    let mut covered_bytes = 0u64;
    while first < paragraphs.len() {
        let mut slice = PacedSlice::new(budget);
        let mut end = first;
        while end < paragraphs.len() && slice.admit(paragraphs[end]) {
            end += 1;
        }
        // Every slice is non-empty, so the cut makes progress.
        assert!(end > first);
        check(&slice, first, end);
        covered_bytes = covered_bytes.saturating_add(slice.bytes());
        first = end;
    }
    (first, covered_bytes)
}

#[kani::proof]
#[kani::unwind(8)]
fn paced_slices_cover_the_paragraphs_exactly() {
    let (lengths, count) = any_paragraphs();
    let paragraphs = &lengths[..count];
    let (covered, bytes) = cut(paragraphs, any_budget(), |slice, first, end| {
        assert_eq!(slice.paragraphs() as usize, end - first);
    });
    assert_eq!(covered, count);
    let total = paragraphs
        .iter()
        .fold(0u64, |sum, paragraph| sum.saturating_add(paragraph.bytes));
    assert_eq!(bytes, total);
    kani::cover!(count == MAX_PARAGRAPHS, "the longest sequence");
}

#[kani::proof]
#[kani::unwind(8)]
fn paced_slices_hold_both_budgets_unless_one_paragraph() {
    let (lengths, count) = any_paragraphs();
    let paragraphs = &lengths[..count];
    let budget = any_budget();
    cut(paragraphs, budget, |slice, first, end| {
        let alone = end - first == 1;
        assert!(alone || (slice.bytes() <= budget.bytes && slice.chars() <= budget.chars));
        // A slice ends only where the next paragraph would break a budget:
        // the cut is the longest prefix the budgets allow.
        if end < paragraphs.len() {
            let next = paragraphs[end];
            assert!(
                slice.bytes().saturating_add(next.bytes) > budget.bytes
                    || slice.chars().saturating_add(next.chars) > budget.chars
            );
        }
        kani::cover!(
            alone && slice.chars() > budget.chars,
            "an over-budget paragraph goes alone"
        );
        kani::cover!(end - first == 3, "a three-paragraph slice");
    });
}

/// The byte budget alone does not bound the visible characters: dropping the
/// second budget lets a slice of several paragraphs exceed `V_SLICE`.
#[kani::proof]
#[kani::unwind(8)]
#[kani::should_panic]
fn paced_slices_without_the_visible_budget_exceed_v_slice() {
    let (lengths, count) = any_paragraphs();
    let paragraphs = &lengths[..count];
    let bytes_only = SliceBudget {
        bytes: kani::any(),
        chars: u64::MAX,
    };
    cut(paragraphs, bytes_only, |slice, first, end| {
        assert!(end - first == 1 || slice.chars() <= V_SLICE);
    });
}

#[kani::proof]
fn the_paced_budget_follows_batch_density() {
    let summary = ProfileSummary {
        total_bytes: kani::any(),
        longest_paragraph_chars: kani::any(),
        worst_batch_chars: kani::any(),
    };
    let budget = SliceBudget::for_profile(summary);
    assert_eq!(
        batch_exceeds_slice(summary),
        summary.worst_batch_chars > V_SLICE
    );
    if batch_exceeds_slice(summary) {
        assert_eq!(budget, SliceBudget::PACED);
    } else {
        assert_eq!(budget, SliceBudget::BYTES_ONLY);
    }
}
