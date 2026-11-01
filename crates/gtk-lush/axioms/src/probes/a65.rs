// SPDX-License-Identifier: MIT OR Apache-2.0

//! A65: `gtk_text_buffer_insert_with_tags` of a run carrying an `invisible`
//! tag, appended to a short visible head in the same paragraph, never adds
//! the run to the paragraph's layout: once the view has validated, the
//! paragraph's display height (wrapping on) and the layout width (wrapping
//! off) equal those of the head alone, at every tail size. The run is never
//! visible between the insertion and the tagging, because both happen in one
//! call with no main-loop turn between them.
//!
//! The cost of that one call, and of the first location query after it (the
//! view's validation of the paragraph), is recorded per MiB of tail: the
//! input to `sliced-protected-install` (C3)'s adoption gate. Hidden text
//! still costs what is paid per character of the paragraph rather than per
//! laid-out character: the btree insert, and log attributes over the whole
//! paragraph (A26).
//!
//! Protected View (C2) inserts a giant paragraph's tail this way (design D1,
//! D13).

use std::time::Instant;

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{ParagraphKind, TextViewFixture, invisible_tag, paragraph};
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Characters in the visible head.
pub const HEAD_CHARS: usize = 2_000;
/// Tail sizes, in MiB of ASCII text.
pub const TAIL_MIB: [usize; 3] = [1, 4, 16];
const MIB: usize = 1024 * 1024;
const TILE_CHARS: usize = 64 * 1024;

/// The visible head.
#[must_use]
pub fn head() -> String {
    paragraph(ParagraphKind::Ascii, HEAD_CHARS, 65)
}

/// A tail of `mib` MiB of ASCII words with no line terminator, built from a
/// repeated 64 Ki-character tile so a large tail is cheap to make.
#[must_use]
pub fn tail(mib: usize) -> String {
    let tile = paragraph(ParagraphKind::Ascii, TILE_CHARS, 66);
    tile.repeat(mib * MIB / TILE_CHARS)
}

/// The paragraph's display height and the horizontal layout width (the
/// hadjustment's upper bound), after the view validated it.
#[must_use]
pub fn paragraph_extent(fixture: &TextViewFixture) -> (i32, f64) {
    let start = fixture.buffer.start_iter();
    let (_, height) = fixture.view.line_yrange(&start);
    (height, fixture.scrolled.hadjustment().upper())
}

/// Replace the buffer with the head alone, then insert `tail` hidden with
/// one `insert_with_tags`. Returns the insert's and the first location
/// query's microseconds.
#[must_use]
pub fn insert_hidden_tail(
    fixture: &TextViewFixture,
    tag: &gtk4::TextTag,
    tail: &str,
) -> (u128, u128) {
    let mut end = fixture.buffer.end_iter();
    let started = Instant::now();
    fixture.buffer.insert_with_tags(&mut end, tail, &[tag]);
    let insert_us = started.elapsed().as_micros();
    let start = fixture.buffer.start_iter();
    let started = Instant::now();
    let _ = fixture.view.iter_location(&start);
    let _ = fixture.view.line_yrange(&start);
    (insert_us, started.elapsed().as_micros())
}

fn per_mib(micros: u128, mib: usize) -> u128 {
    micros / u128::try_from(mib.max(1)).unwrap_or(1)
}

/// Probe A65.
#[must_use]
pub fn probe_a65() -> Observation {
    Recorder::run(AxiomId::new(65), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let fixture = TextViewFixture::new(gtk4::WrapMode::WordChar);
        let _shown = Presented::checked(
            recorder,
            &fixture.scrolled,
            "control: the text view was presented",
        )?;
        let tag = invisible_tag(&fixture.buffer);
        let head = head();
        let mut holds_wrapped = true;
        let mut holds_unwrapped = true;
        for wrap in [gtk4::WrapMode::WordChar, gtk4::WrapMode::None] {
            fixture.view.set_wrap_mode(wrap);
            fixture.buffer.set_text(&head);
            settle(LAYOUT_SETTLE);
            let reference = paragraph_extent(&fixture);
            let mut rows = Vec::new();
            for mib in TAIL_MIB {
                let tail = tail(mib);
                fixture.buffer.set_text(&head);
                settle(LAYOUT_SETTLE);
                let (insert_us, validate_us) = insert_hidden_tail(&fixture, &tag, &tail);
                settle(LAYOUT_SETTLE);
                let extent = paragraph_extent(&fixture);
                let chars = usize::try_from(fixture.buffer.char_count()).unwrap_or(0);
                let complete = chars == HEAD_CHARS + tail.len();
                let same = if wrap == gtk4::WrapMode::None {
                    super::same_value(extent.1, reference.1)
                } else {
                    extent.0 == reference.0
                };
                if !(same && complete) {
                    if wrap == gtk4::WrapMode::None {
                        holds_unwrapped = false;
                    } else {
                        holds_wrapped = false;
                    }
                }
                rows.push(format!(
                    "{mib} MiB: insert {insert_us} us ({} us/MiB), first location {validate_us} \
                     us ({} us/MiB), extent {extent:?}, chars {chars}",
                    per_mib(insert_us, mib),
                    per_mib(validate_us, mib)
                ));
                fixture.buffer.set_text("");
                settle(LAYOUT_SETTLE);
            }
            let key = if wrap == gtk4::WrapMode::None {
                "wrap_off"
            } else {
                "wrap_on"
            };
            recorder.measure(
                key,
                format!("head-only extent {reference:?}; {}", rows.join(" | ")),
            );
        }
        recorder.axiom(
            holds_wrapped,
            "with wrapping on, a paragraph whose tail was inserted hidden is exactly as tall as \
             its head alone, at every tail size",
        )?;
        recorder.axiom(
            holds_unwrapped,
            "with wrapping off, the layout is exactly as wide as the head alone, at every tail \
             size",
        )
    })
}
