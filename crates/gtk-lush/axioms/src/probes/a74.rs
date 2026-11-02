// SPDX-License-Identifier: MIT OR Apache-2.0

//! A74: a change of a wrapping `GtkTextView`'s text width (here, a left
//! gutter growing) re-lays out **every** paragraph, offscreen ones included:
//! once the view settles, the last paragraph's position equals that of a view
//! built at the new width from the start, and differs from its position before
//! the change. The time the view spends revalidating is recorded (cost
//! evidence).
//!
//! Protected View (C2b) therefore never resizes its gutter at an entry or an
//! exit: the fold-marker column is allocated at a fixed width when the editor
//! is built (`protected-view-inspector-and-surfaces` review S4).

use std::time::{Duration, Instant};

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{ParagraphKind, TextViewFixture, paragraph};
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Paragraphs in the fixture: far more than one screen holds.
pub const PARAGRAPHS: usize = 60;
/// Characters per paragraph: several wrapped rows each.
pub const PARAGRAPH_CHARS: usize = 2_000;
/// The gutter's width before the change, in pixels.
pub const NARROW_GUTTER: i32 = 8;
/// The gutter's width after the change, in pixels.
pub const WIDE_GUTTER: i32 = 96;
/// Most settles to wait for the view's idle validation to finish.
const MAX_SETTLES: u32 = 50;

/// The fixture's text.
#[must_use]
pub fn text() -> String {
    (0..PARAGRAPHS)
        .map(|index| {
            paragraph(
                ParagraphKind::Ascii,
                PARAGRAPH_CHARS,
                74 + u64::try_from(index).unwrap_or(0),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Install (once) a left gutter on `view` and give it `width`.
pub fn set_gutter_width(view: &gtk4::TextView, width: i32) {
    let gutter = view.gutter(gtk4::TextWindowType::Left).unwrap_or_else(|| {
        let gutter = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        view.set_gutter(gtk4::TextWindowType::Left, Some(&gutter));
        gutter.upcast()
    });
    gutter.set_size_request(width, -1);
}

/// The last paragraph's buffer y and height.
#[must_use]
pub fn last_paragraph_extent(fixture: &TextViewFixture) -> (i32, i32) {
    let last = fixture.buffer.end_iter();
    fixture.view.line_yrange(&last)
}

/// Settle until the last paragraph's extent stops moving (the view's idle
/// validation reached it), returning the extent and the time it took.
#[must_use]
pub fn settle_validated(fixture: &TextViewFixture) -> ((i32, i32), Duration) {
    let started = Instant::now();
    let mut last = last_paragraph_extent(fixture);
    let mut stable = 0;
    for _ in 0..MAX_SETTLES {
        settle(LAYOUT_SETTLE);
        let now = last_paragraph_extent(fixture);
        stable = if now == last { stable + 1 } else { 0 };
        last = now;
        if stable >= 2 {
            break;
        }
    }
    (last, started.elapsed())
}

fn fixture_at(
    recorder: &mut Recorder,
    gutter: i32,
    text: &str,
) -> Result<(TextViewFixture, Presented), crate::observation::Stop> {
    let fixture = TextViewFixture::new(gtk4::WrapMode::WordChar);
    set_gutter_width(&fixture.view, gutter);
    let shown = Presented::checked(
        recorder,
        &fixture.scrolled,
        "control: the text view was presented",
    )?;
    fixture.buffer.set_text(text);
    Ok((fixture, shown))
}

/// Probe A74.
#[must_use]
pub fn probe_a74() -> Observation {
    Recorder::run(AxiomId::new(74), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let text = text();
        let (changed, _shown) = fixture_at(recorder, NARROW_GUTTER, &text)?;
        let (before, first_validation) = settle_validated(&changed);
        set_gutter_width(&changed.view, WIDE_GUTTER);
        let (after, revalidation) = settle_validated(&changed);
        let (fresh, _fresh_shown) = fixture_at(recorder, WIDE_GUTTER, &text)?;
        let (reference, _) = settle_validated(&fresh);
        recorder.measure(
            "extents",
            format!(
                "last paragraph (y, height): before {before:?}, after {after:?}, fresh view at \
                 the new width {reference:?}; first validation {} ms, revalidation after the \
                 width change {} ms",
                first_validation.as_millis(),
                revalidation.as_millis()
            ),
        );
        recorder.control(
            before != reference,
            "control: the two widths lay the document out differently",
        )?;
        recorder.axiom(
            after == reference,
            "after the text width changes, the last (offscreen) paragraph sits exactly where a \
             view built at the new width puts it: every paragraph was laid out again",
        )
    })
}
