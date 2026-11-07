// SPDX-License-Identifier: MIT OR Apache-2.0

//! A100: `GtkSourceSearchContext::forward` is cheap **inside a completed
//! scan** and expensive **before it**.
//!
//! - Once `occurrences-count` reads a count (the asynchronous scan ended), a
//!   `forward` from the start of a 262,144-line buffer to its only match, on
//!   the last line, costs microseconds: it walks the context's match tag.
//! - On a context that has not scanned, the same `forward` scans
//!   synchronously up to the match: milliseconds, growing with the distance.
//! - It honours `visible-only` (A33): a match inside hidden text is not
//!   returned by default.
//! - After an edit, `occurrences-count` reads `-1` until the context's
//!   rescan of the edited region ends, and `notify::occurrences-count` fires
//!   when that rescan ends, as it does when the first scan ends.
//!
//! LushText's minimap reads search markers only from a completed scan and
//! re-arms on that notify (`bin-minimap-markers-per-row` D5), so no minimap
//! dispatch scans synchronously.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{invisible_tag, record_text_toolkit, time_us};
use crate::session::{REALIZE_BUDGET, flush_events, wait_until};
use crate::{AxiomId, Observation};

/// The searched word.
pub const NEEDLE: &str = "needle";
/// Lines of the sparse buffer.
pub const SPARSE_LINES: usize = 1 << 18;

/// A buffer of `lines` short lines whose last line holds the only
/// [`NEEDLE`].
#[must_use]
pub fn sparse_buffer(lines: usize) -> sourceview5::Buffer {
    let mut text = String::with_capacity(lines * 8);
    for line in 0..lines.saturating_sub(1) {
        text.push_str("row ");
        text.push_str(&(line % 1000).to_string());
        text.push('\n');
    }
    text.push_str(NEEDLE);
    text.push('\n');
    let buffer = sourceview5::Buffer::new(None);
    buffer.set_text(&text);
    buffer
}

/// A context searching `buffer` for [`NEEDLE`], without wrap-around.
#[must_use]
pub fn needle_search(buffer: &sourceview5::Buffer) -> sourceview5::SearchContext {
    let settings = sourceview5::SearchSettings::new();
    settings.set_search_text(Some(NEEDLE));
    settings.set_wrap_around(false);
    sourceview5::SearchContext::new(buffer, Some(&settings))
}

/// Microseconds of one `forward` from the buffer start, and the line it found.
#[must_use]
pub fn forward_from_start_us(context: &sourceview5::SearchContext) -> (Option<i32>, u128) {
    let start = context.buffer().start_iter();
    let (found, elapsed) = time_us(|| context.forward(&start));
    (found.map(|(start, _, _)| start.line()), elapsed)
}

/// Probe A100.
#[must_use]
pub fn probe_a100() -> Observation {
    Recorder::run(AxiomId::new(100), |recorder| {
        record_text_toolkit(recorder);

        let unscanned_buffer = sparse_buffer(SPARSE_LINES);
        let unscanned = needle_search(&unscanned_buffer);
        let (unscanned_line, unscanned_us) = forward_from_start_us(&unscanned);
        recorder.measure("unscanned_forward_us", unscanned_us);

        let buffer = sparse_buffer(SPARSE_LINES);
        let context = needle_search(&buffer);
        let notifies = Rc::new(Cell::new(0u32));
        {
            let notifies = notifies.clone();
            context.connect_occurrences_count_notify(move |_| notifies.set(notifies.get() + 1));
        }
        let scanned = wait_until(REALIZE_BUDGET * 4, || context.occurrences_count() >= 0);
        recorder.control(scanned, "control: the first scan ended")?;
        let notifies_after_scan = notifies.get();
        let (scanned_line, scanned_us) = forward_from_start_us(&context);
        recorder.measure("scanned_forward_us", scanned_us);
        recorder.measure("notifies_when_first_scan_ends", notifies_after_scan);

        notifies.set(0);
        let mut at = buffer.iter_at_line(10).unwrap_or_else(|| buffer.end_iter());
        buffer.insert(&mut at, "x");
        let count_after_edit = context.occurrences_count();
        recorder.measure("count_right_after_edit", count_after_edit);
        let rescanned = wait_until(REALIZE_BUDGET, || context.occurrences_count() >= 0);
        flush_events();
        recorder.measure("notifies_when_rescan_ends", notifies.get());

        let hidden_buffer = sourceview5::Buffer::new(None);
        hidden_buffer.set_text("visible\nhidden needle\n");
        let tag = invisible_tag(hidden_buffer.upcast_ref());
        if let (Some(start), Some(end)) =
            (hidden_buffer.iter_at_line(1), hidden_buffer.iter_at_line(2))
        {
            hidden_buffer.apply_tag(&tag, &start, &end);
        }
        let hidden = needle_search(&hidden_buffer);
        let hidden_found = hidden.forward(&hidden_buffer.start_iter()).is_some();
        recorder.measure("hidden_match_found_by_default", hidden_found);

        let last = i32::try_from(SPARSE_LINES - 1).unwrap_or(i32::MAX);
        recorder.control(
            unscanned_line == Some(last) && scanned_line == Some(last),
            "control: both walks found the match on the last line",
        )?;
        recorder.control(rescanned, "control: the rescan after the edit ended")?;
        recorder.axiom(
            scanned_us < 2_000 && unscanned_us > scanned_us.saturating_mul(10),
            "forward is cheap inside a completed scan and scans synchronously before it",
        )?;
        recorder.axiom(!hidden_found, "forward skips hidden text by default")?;
        recorder.axiom(
            notifies_after_scan >= 1 && count_after_edit == -1 && notifies.get() >= 1,
            "occurrences-count reads -1 after an edit and notifies when the rescan ends",
        )
    })
}
