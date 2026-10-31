// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over the paced restyle plan, [`next_restyle_range`], and
//! the settle predicate, [`settle_stage`] (`bound-post-open-settle`, design
//! D1 and D9, review R21).
//!
//! Domain: documents of at most [`MAX_PARAGRAPHS`] paragraphs with symbolic
//! visible-character lengths, every visible span, and every budget. The GTK
//! adapter reads real paragraph lengths from the buffer; these harnesses do
//! not range over text.
//!
//! Compiled only under `cfg(kani)` (`make kani`, shard
//! `core-post-open-settle`).

use super::{
    ParagraphSpan, RestyleCursor, SETTLE_QUIET_US, SettleFacts, SettleStage, next_restyle_range,
    settle_stage,
};

/// The longest document the harnesses range over.
const MAX_PARAGRAPHS: u32 = 6;

struct Plan {
    lengths: [u64; MAX_PARAGRAPHS as usize],
    count: u32,
    visible: ParagraphSpan,
    v_slice: u64,
}

fn any_plan() -> Plan {
    let lengths: [u64; MAX_PARAGRAPHS as usize] = kani::any();
    let count: u32 = kani::any();
    kani::assume(count <= MAX_PARAGRAPHS);
    let visible = ParagraphSpan {
        first: kani::any(),
        end: kani::any(),
    };
    kani::assume(visible.first <= visible.end && visible.end <= count);
    Plan {
        lengths,
        count,
        visible,
        v_slice: kani::any(),
    }
}

/// Run the plan to its end, calling `check(index, range, chars)` for every
/// range, and return a bit mask of the paragraphs handed out and how many
/// ranges there were.
fn run(plan: &Plan, mut check: impl FnMut(u32, ParagraphSpan, u64)) -> (u32, u32) {
    let chars = |i: u32| plan.lengths[i as usize];
    let mut cursor = RestyleCursor::new(plan.visible, plan.count);
    let mut covered = 0u32;
    let mut ranges = 0u32;
    while let Some((next, range)) = next_restyle_range(cursor, plan.v_slice, chars) {
        // Every range makes progress inside the document.
        assert!(!range.is_empty() && range.end <= plan.count);
        let mut sum = 0u64;
        let mut i = range.first;
        while i < range.end {
            // No paragraph is handed out twice.
            assert!(covered & (1 << i) == 0);
            covered |= 1 << i;
            sum = sum.saturating_add(chars(i));
            i += 1;
        }
        check(ranges, range, sum);
        ranges += 1;
        // Termination: at most one range per paragraph.
        assert!(ranges <= plan.count);
        cursor = next;
    }
    (covered, ranges)
}

#[kani::proof]
#[kani::unwind(8)]
fn restyle_ranges_cover_the_document_exactly_once() {
    let plan = any_plan();
    let (covered, _) = run(&plan, |_, _, _| {});
    assert_eq!(covered, (1u32 << plan.count) - 1);
    kani::cover!(plan.count == MAX_PARAGRAPHS, "the longest document");
}

#[kani::proof]
#[kani::unwind(8)]
fn restyle_ranges_hold_v_slice_unless_one_paragraph() {
    let plan = any_plan();
    run(&plan, |_, range, chars| {
        assert!(range.len() == 1 || chars <= plan.v_slice);
        kani::cover!(
            range.len() == 1 && chars > plan.v_slice,
            "an over-budget paragraph alone"
        );
        kani::cover!(range.len() == 3, "a three-paragraph range");
    });
}

#[kani::proof]
#[kani::unwind(8)]
fn restyle_ranges_take_the_visible_paragraphs_first_then_alternate() {
    let plan = any_plan();
    let visible = plan.visible;
    let mut visible_done = visible.is_empty();
    let mut last_side_below: Option<bool> = None;
    let mut above_left = visible.first;
    let mut below_left = plan.count - visible.end;
    run(&plan, |_, range, _| {
        let inside = range.first >= visible.first && range.end <= visible.end;
        if !visible_done {
            // Until every visible paragraph is out, ranges stay inside them,
            // forwards.
            assert!(inside);
            if range.end == visible.end {
                visible_done = true;
            }
            return;
        }
        assert!(!inside || visible.is_empty());
        let below = range.first >= visible.end;
        assert!(below || range.end <= visible.first);
        // Alternation: a side repeats only when the other is exhausted.
        if let Some(previous_below) = last_side_below
            && previous_below == below
        {
            assert!(if below {
                above_left == 0
            } else {
                below_left == 0
            });
        }
        if below {
            below_left -= range.len();
        } else {
            above_left -= range.len();
        }
        last_side_below = Some(below);
    });
    kani::cover!(
        !visible.is_empty() && visible.first > 0 && visible.end < plan.count,
        "outside on both sides"
    );
}

#[kani::proof]
fn settled_needs_every_clause() {
    let facts = SettleFacts {
        open_complete: kani::any(),
        language_attached: kani::any(),
        post_open_work_pending: kani::any(),
        last_mark_us: kani::any(),
        last_sentinel_us: kani::any(),
    };
    let now: i64 = kani::any();
    let stage = settle_stage(facts, now);
    if stage == SettleStage::Settled {
        assert!(facts.open_complete && facts.language_attached && !facts.post_open_work_pending);
        assert!(facts.last_sentinel_us.is_some());
        if let (Some(mark), Some(sentinel)) = (facts.last_mark_us, facts.last_sentinel_us) {
            assert!(sentinel >= mark && now.saturating_sub(mark) >= SETTLE_QUIET_US);
        }
    }
    kani::cover!(stage == SettleStage::Settled, "settled");
    kani::cover!(stage == SettleStage::Quieting, "quieting");
}
