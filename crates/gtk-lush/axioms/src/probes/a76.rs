// SPDX-License-Identifier: MIT OR Apache-2.0

//! A76: a zero-width **left-gravity** mark between two runs of text keeps the
//! `GtkTextBuffer` btree from merging their character segments, which it
//! otherwise does by copying both strings (A36).
//!
//! Structural half (observed through mark offsets and tags):
//!
//! - text inserted at a left-gravity mark's iterator lands **after** the mark,
//!   and text inserted at a right-gravity mark's iterator lands **before** it;
//! - text inserted at a left-gravity mark placed at the **start** of a tagged
//!   run lands before the run and carries none of its tags (GTK's insertion
//!   point skips zero-width left-gravity segments, the mark and a toggle-off,
//!   and stops before a toggle-on).
//!
//! Cost half (segment structure is private, so, like A36, it is observed by
//! cost only): appending `SLICES` tagged slices one after another, each at
//! the run's end, costs per append a time that **grows** with the run when
//! nothing separates the slices (the toggles between two same-tag runs cancel
//! and the two segments merge), and stays **flat** when each slice is followed
//! by a left-gravity mark. Deleting the slices again, one range between two
//! marks at a time **from the start** (each collapsed mark deleted with its
//! range), also stays flat; from the end the per-range cost grows with the
//! text before the range (recorded, not claimed).
//!
//! `sliced-protected-install` (C3) separates the hidden-tail slices of a
//! protected paragraph with such marks (design D3), inserts a deferred head
//! at a left-gravity mark at the paragraph's start (design D2), and clears a
//! paragraph's ranges from its start (design D4).

use std::time::Instant;

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{ParagraphKind, invisible_tag, paragraph};
use crate::{AxiomId, Observation};

/// Bytes of one appended slice (ASCII, so also characters).
pub const SLICE_BYTES: usize = 64 * 1024;
/// Appended slices per run: an 8 MiB run.
pub const SLICES: usize = 128;
/// Slices averaged at each end of a run when comparing costs.
const WINDOW: usize = 8;
/// The last window's median may be at most this many times the first's for
/// the cost to count as flat.
pub const FLAT_RATIO: u128 = 2;
/// The last window's median must be at least this many times the first's for
/// the cost to count as growing.
pub const GROWTH_RATIO: u128 = 3;

/// One slice of ASCII text.
#[must_use]
pub fn slice_text() -> String {
    paragraph(ParagraphKind::Ascii, SLICE_BYTES, 76)
}

/// Append `SLICES` copies of `slice` to an empty buffer, each tagged with
/// `tag` by `insert_with_tags` at a right-gravity end mark, followed by a
/// left-gravity boundary mark when `separate` holds. Returns each append's
/// microseconds and the boundary marks (empty without separation).
#[must_use]
pub fn append_tagged_slices(
    buffer: &gtk4::TextBuffer,
    tag: &gtk4::TextTag,
    slice: &str,
    separate: bool,
) -> (Vec<u128>, Vec<gtk4::TextMark>) {
    buffer.set_text("");
    let end = buffer.create_mark(None, &buffer.end_iter(), false);
    let mut costs = Vec::with_capacity(SLICES);
    let mut marks = Vec::new();
    for _ in 0..SLICES {
        let started = Instant::now();
        let mut at = buffer.iter_at_mark(&end);
        buffer.insert_with_tags(&mut at, slice, &[tag]);
        if separate {
            marks.push(buffer.create_mark(None, &at, true));
        }
        costs.push(started.elapsed().as_micros());
    }
    buffer.delete_mark(&end);
    (costs, marks)
}

/// Delete the slices between consecutive `marks` from the end backwards, one
/// range per call, deleting the range's end mark with it (it is collapsed
/// onto the range's start). Returns each deletion's microseconds.
#[must_use]
pub fn delete_ranges_backwards(buffer: &gtk4::TextBuffer, marks: &[gtk4::TextMark]) -> Vec<u128> {
    let mut costs = Vec::with_capacity(marks.len());
    for index in (0..marks.len()).rev() {
        let started = Instant::now();
        let mut start = if index == 0 {
            buffer.start_iter()
        } else {
            buffer.iter_at_mark(&marks[index - 1])
        };
        let mut end = buffer.iter_at_mark(&marks[index]);
        buffer.delete(&mut start, &mut end);
        buffer.delete_mark(&marks[index]);
        costs.push(started.elapsed().as_micros());
    }
    costs
}

/// Delete the slices between consecutive `marks` from the start forwards,
/// one range per call, and each range's start mark once the range is gone. Returns each deletion's microseconds.
#[must_use]
pub fn delete_ranges_forwards(buffer: &gtk4::TextBuffer, marks: &[gtk4::TextMark]) -> Vec<u128> {
    let mut costs = Vec::with_capacity(marks.len());
    for (index, mark) in marks.iter().enumerate() {
        let started = Instant::now();
        let mut start = buffer.start_iter();
        let mut end = buffer.iter_at_mark(mark);
        buffer.delete(&mut start, &mut end);
        if index > 0 {
            buffer.delete_mark(&marks[index - 1]);
        }
        costs.push(started.elapsed().as_micros());
    }
    if let Some(last) = marks.last() {
        buffer.delete_mark(last);
    }
    costs
}

fn median(values: &[u128]) -> u128 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted.get(sorted.len() / 2).copied().unwrap_or(0)
}

/// The median of the first and of the last `WINDOW` costs.
#[must_use]
pub fn ends(costs: &[u128]) -> (u128, u128) {
    let window = WINDOW.min(costs.len());
    (
        median(&costs[..window]),
        median(&costs[costs.len() - window..]),
    )
}

/// Whether the per-operation cost stays flat across the run.
#[must_use]
pub fn is_flat(costs: &[u128]) -> bool {
    let (first, last) = ends(costs);
    last <= first.max(1).saturating_mul(FLAT_RATIO)
}

/// Whether the per-operation cost grows across the run.
#[must_use]
pub fn is_growing(costs: &[u128]) -> bool {
    let (first, last) = ends(costs);
    last >= first.max(1).saturating_mul(GROWTH_RATIO)
}

/// Probe A76.
#[must_use]
pub fn probe_a76() -> Observation {
    Recorder::run(AxiomId::new(76), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let buffer = gtk4::TextBuffer::new(None);
        let tag = invisible_tag(&buffer);

        // Gravity.
        buffer.set_text("abc");
        let left = buffer.create_mark(None, &buffer.end_iter(), true);
        let right = buffer.create_mark(None, &buffer.end_iter(), false);
        let mut at = buffer.iter_at_mark(&left);
        buffer.insert(&mut at, "XYZ");
        let left_offset = buffer.iter_at_mark(&left).offset();
        let right_offset = buffer.iter_at_mark(&right).offset();
        recorder.measure(
            "gravity",
            format!("left mark at {left_offset}, right mark at {right_offset} after 3+3"),
        );
        recorder.axiom(
            left_offset == 3 && right_offset == 6,
            "text inserted at a left-gravity mark lands after it, and before a right-gravity \
             mark at the same place",
        )?;
        buffer.delete_mark(&left);
        buffer.delete_mark(&right);

        // A deferred head at a left-gravity mark at the start of a tagged run.
        buffer.set_text("p\n");
        let start = buffer.create_mark(None, &buffer.iter_at_offset(2), true);
        let end = buffer.create_mark(None, &buffer.end_iter(), false);
        let mut at = buffer.iter_at_mark(&end);
        buffer.insert_with_tags(&mut at, "TAIL", &[&tag]);
        let mut at = buffer.iter_at_mark(&start);
        buffer.insert(&mut at, "head");
        let text = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .to_string();
        let head_tagged = (2..6).any(|offset| buffer.iter_at_offset(offset).has_tag(&tag));
        let tail_tagged = (6..10).all(|offset| buffer.iter_at_offset(offset).has_tag(&tag));
        recorder.measure(
            "deferred_head",
            format!("text {text:?}, head tagged {head_tagged}, tail tagged {tail_tagged}"),
        );
        recorder.axiom(
            text == "p\nheadTAIL" && !head_tagged && tail_tagged,
            "text inserted at a left-gravity mark at the start of a tagged run lands before the \
             run, untagged, and the run keeps its tag",
        )?;
        buffer.delete_mark(&start);
        buffer.delete_mark(&end);

        // Cost: appends with and without separation (each in its own buffer,
        // so one run's freed memory does not shape the other's), then
        // deletions.
        let slice = slice_text();
        let control = gtk4::TextBuffer::new(None);
        let control_tag = invisible_tag(&control);
        let (merged, _) = append_tagged_slices(&control, &control_tag, &slice, false);
        control.set_text("");
        let (separated, marks) = append_tagged_slices(&buffer, &tag, &slice, true);
        let complete = usize::try_from(buffer.char_count()).unwrap_or(0) == SLICES * SLICE_BYTES;
        let one_range = buffer.start_iter().has_tag(&tag) && {
            let mut toggle = buffer.start_iter();
            !toggle.forward_to_tag_toggle(Some(&tag)) || toggle.is_end()
        };
        let deletions = delete_ranges_backwards(&buffer, &marks);
        let emptied = buffer.char_count() == 0;
        // The same deletions forwards, from the start, for comparison.
        let (_, forward_marks) = append_tagged_slices(&buffer, &tag, &slice, true);
        let forwards = delete_ranges_forwards(&buffer, &forward_marks);
        recorder.measure(
            "delete_us_between_marks_forwards",
            format!("first/last medians {:?}, all {forwards:?}", ends(&forwards)),
        );
        recorder.measure(
            "append_us_without_marks",
            format!("first/last medians {:?}, all {merged:?}", ends(&merged)),
        );
        recorder.measure(
            "append_us_with_left_marks",
            format!(
                "first/last medians {:?}, all {separated:?}",
                ends(&separated)
            ),
        );
        recorder.measure(
            "delete_us_between_marks_backwards",
            format!(
                "first/last medians {:?}, all {deletions:?}",
                ends(&deletions)
            ),
        );
        recorder.control(
            complete && one_range && emptied && buffer.char_count() == 0,
            "control: the separated run holds every slice, is one tagged range, and both \
             deletion orders empty the buffer",
        )?;
        recorder.control(
            is_growing(&merged),
            "control (A36): without separation the per-append cost grows with the run",
        )?;
        recorder.axiom(
            is_flat(&separated),
            "with a left-gravity mark after each slice the per-append cost stays flat",
        )?;
        recorder.axiom(
            is_flat(&forwards),
            "deleting the slices between marks from the start, each collapsed mark with its \
             range, costs a flat time per range",
        )
    })
}
