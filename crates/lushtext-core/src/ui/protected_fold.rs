// SPDX-License-Identifier: GPL-3.0-or-later

//! Cross-cutting coordination (no role): the Protected View fold executor
//! (`long-line-protected-view` design D1, task 3.1).
//!
//! Every whole-content install of a protected body inserts its text through
//! [`insert_folded`], which hides each folded paragraph's tail **in the same
//! dispatch as its head**: the head with `insert`, the tail with
//! `insert_with_tags` and the fold tag (`invisible`, not `editable`). No
//! unfolded giant paragraph therefore exists in a `GtkTextView` at any
//! dispatch boundary, so GTK never lays one out (ledger A23, A65). The plan
//! comes from the worker that profiled the body
//! (`model::protected_fold::FoldPlan`, carried by `DocumentProfile::fold_plan`).
//!
//! Callers: the load install (`ui/editor_page/load/execution.rs`) and the
//! bounded buffer replacement (`ui/editor_page/buffer_replacement/execution.rs`,
//! which serves draft recovery, set-aside open, local-history restore and
//! Undo Restore, and eviction reload).
//!
//! **One hidden-tail insert per main-loop iteration, process-wide.** A tail
//! longer than the slice budget goes in in one call: the declared envelope
//! `sliced-protected-install` (C3) measures. GLib dispatches every ready
//! source of the same priority in one iteration, so two documents' install
//! turns (a session restore, two recovered drafts) would otherwise run their
//! tail inserts back to back in one dispatch: twice the envelope. Every
//! protected install therefore asks [`tail_insert_admitted`] before a turn and
//! reschedules itself when another document already inserted a tail in this
//! iteration; a high-priority idle re-admits at the start of the next one.

use std::cell::Cell;

use gtk4::prelude::*;

use crate::model::paragraph_boundary::next_paragraph_break;
use crate::model::protected_fold::{FoldPlan, fold_byte_offset};

thread_local! {
    static TAIL_INSERTED_THIS_ITERATION: Cell<bool> = const { Cell::new(false) };
}

/// Whether a protected install may run a turn that can insert a hidden tail
/// now: no other tail went in during this main-loop iteration (module docs).
pub(crate) fn tail_insert_admitted() -> bool {
    !TAIL_INSERTED_THIS_ITERATION.with(Cell::get)
}

/// Record a hidden-tail insert; the next iteration starts with a
/// `G_PRIORITY_HIGH` idle that re-admits, before any default-priority turn.
fn note_tail_inserted() {
    if !TAIL_INSERTED_THIS_ITERATION.with(|inserted| inserted.replace(true)) {
        glib::idle_add_local_full(glib::Priority::HIGH, || {
            TAIL_INSERTED_THIS_ITERATION.with(|inserted| inserted.set(false));
            glib::ControlFlow::Break
        });
    }
}

/// The name of the fold tag in a buffer's tag table.
pub const FOLD_TAG_NAME: &str = "lushtext-protected-fold";

/// The buffer's fold tag, created on first use: `invisible` (the tail is
/// never laid out, A24/A65) and not `editable`.
pub(crate) fn fold_tag(buffer: &gtk4::TextBuffer) -> gtk4::TextTag {
    let table = buffer.tag_table();
    if let Some(tag) = table.lookup(FOLD_TAG_NAME) {
        return tag;
    }
    let tag = gtk4::TextTag::builder()
        .name(FOLD_TAG_NAME)
        .invisible(true)
        .editable(false)
        .build();
    table.add(&tag);
    tag
}

/// Insert `text` at `iter` with the plan's tails hidden.
///
/// `text` must start at the start of paragraph `first_paragraph` of the body
/// the plan was made for (every install slice does: slices end after a
/// paragraph delimiter). Runs of unfolded paragraphs go in one `insert`; a
/// folded paragraph goes in as its head, then its tail under the fold tag,
/// then its delimiter. Returns the index of the paragraph after the last one
/// `text` completes (a paragraph `text` leaves open at its end keeps its
/// index, so the next slice continues it: a slice never ends mid-paragraph,
/// so this happens only for the body's last paragraph).
pub(crate) fn insert_folded(
    buffer: &gtk4::TextBuffer,
    iter: &mut gtk4::TextIter,
    text: &str,
    first_paragraph: u64,
    plan: &FoldPlan,
) -> u64 {
    let tag = fold_tag(buffer);
    let ranges = plan.ranges();
    // The first planned range at or after this slice.
    let mut next_range = ranges.partition_point(|range| range.paragraph < first_paragraph);
    let mut paragraph = first_paragraph;
    let mut pending_start = 0; // start of the not-yet-inserted unfolded run
    let mut cursor = 0;
    loop {
        let folded_here = ranges
            .get(next_range)
            .filter(|range| range.paragraph == paragraph)
            .copied();
        let found = next_paragraph_break(text, cursor);
        let content_end = found.map_or(text.len(), |found| found.content_end);
        if let Some(range) = folded_here {
            // Flush the unfolded run before this paragraph, then head, tail.
            if pending_start < cursor {
                buffer.insert(iter, &text[pending_start..cursor]);
            }
            let content = &text[cursor..content_end];
            let split = cursor + fold_byte_offset(content, range.head_chars);
            if cursor < split {
                buffer.insert(iter, &text[cursor..split]);
            }
            if split < content_end {
                // The hidden tail goes in in one call (the declared envelope
                // C3's gate measures); the measurement build records it.
                let started_us = glib::monotonic_time();
                buffer.insert_with_tags(iter, &text[split..content_end], &[&tag]);
                crate::ui::responsiveness_probe::record_span(
                    "protected_tail_insert",
                    started_us,
                    glib::monotonic_time(),
                );
                note_tail_inserted();
            }
            pending_start = content_end;
            next_range += 1;
        }
        match found {
            Some(found) => {
                cursor = found.next_start;
                paragraph += 1;
                if cursor >= text.len() {
                    break;
                }
            }
            None => break,
        }
    }
    if pending_start < text.len() {
        buffer.insert(iter, &text[pending_start..]);
    }
    paragraph
}
