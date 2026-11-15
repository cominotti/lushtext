// SPDX-License-Identifier: MIT OR Apache-2.0

//! A79: mapping a point **below** a `GtkTextView`'s layout to an iterator
//! (`gtk_text_view_get_iter_at_location`) costs time proportional to the
//! **hidden** bytes of the buffer's **last line**: GTK takes the last line,
//! asks for the iterator at the line's full byte count as a visible index
//! (`gtk_text_layout_get_iter_at_position`), and
//! `gtk_text_iter_set_visible_line_index`, never reaching that many visible
//! bytes, keeps stepping at the end iterator, one byte of the implicit final
//! newline per step. When a delimiter follows the hidden run, the last line
//! is the empty one after it and the lookup is flat.
//!
//! GtkSourceView's gutter and below-text layer ask for exactly that point on
//! every drawn frame whenever the visible area extends below the text (a
//! short document, the end-of-document overscroll), so a document whose last
//! paragraph is folded and undelimited pays it on every frame
//! (`sliced-protected-install`, C3: 3.5 s frames recovering a 33.5M-character
//! draft on `main`).

use std::time::Instant;

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{ParagraphKind, TextViewFixture, invisible_tag, paragraph};
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Hidden sizes of the last line, in MiB.
pub const HIDDEN_MIB: [usize; 3] = [1, 2, 4];
const MIB: usize = 1024 * 1024;
/// Rounds over [`HIDDEN_MIB`]; each size keeps its fastest lookup. A lookup
/// is one sample, so a preemption on a shared host inflated one size's cost
/// enough to break the growth check (`rust-1-100-adoption`, 2026-11-14:
/// `[155077, 113729, 287920]` µs, against about `[50000, 100000, 200000]` in
/// isolation). Preemption only adds time, so the minimum is the cost.
const ROUNDS: usize = 3;

/// The microseconds of one lookup of a point 2000 px below the text.
#[must_use]
pub fn below_layout_lookup_us(fixture: &TextViewFixture) -> u128 {
    let started = Instant::now();
    let _ = fixture.view.iter_at_location(0, 100_000);
    started.elapsed().as_micros()
}

/// Fill the buffer with a short visible head and `mib` MiB hidden after it,
/// followed by `"\n"` when `delimited`.
pub fn install(fixture: &TextViewFixture, tag: &gtk4::TextTag, mib: usize, delimited: bool) {
    let buffer = &fixture.buffer;
    buffer.set_text("head ");
    let mut end = buffer.end_iter();
    let hidden = paragraph(ParagraphKind::Ascii, 64 * 1024, 79).repeat(mib * MIB / (64 * 1024));
    buffer.insert_with_tags(&mut end, &hidden, &[tag]);
    if delimited {
        let mut end = buffer.end_iter();
        buffer.insert(&mut end, "\n");
    }
    settle(LAYOUT_SETTLE);
}

/// Probe A79.
#[must_use]
pub fn probe_a79() -> Observation {
    Recorder::run(AxiomId::new(79), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let fixture = TextViewFixture::new(gtk4::WrapMode::None);
        let _shown = Presented::checked(
            recorder,
            &fixture.scrolled,
            "control: the text view was presented",
        )?;
        let tag = invisible_tag(&fixture.buffer);
        let mut last = [u128::MAX; HIDDEN_MIB.len()];
        let mut delimited = [u128::MAX; HIDDEN_MIB.len()];
        let mut rounds = Vec::with_capacity(ROUNDS);
        for _ in 0..ROUNDS {
            let mut round = Vec::with_capacity(HIDDEN_MIB.len());
            for (index, mib) in HIDDEN_MIB.into_iter().enumerate() {
                install(&fixture, &tag, mib, false);
                let undelimited = below_layout_lookup_us(&fixture);
                install(&fixture, &tag, mib, true);
                let with_delimiter = below_layout_lookup_us(&fixture);
                last[index] = last[index].min(undelimited);
                delimited[index] = delimited[index].min(with_delimiter);
                round.push((undelimited, with_delimiter));
            }
            rounds.push(round);
        }
        fixture.buffer.set_text("");
        recorder.measure(
            "below_layout_lookup_us",
            format!(
                "hidden {HIDDEN_MIB:?} MiB, fastest of {ROUNDS} rounds: last line {last:?}, \
                 delimited {delimited:?}"
            ),
        );
        recorder.measure(
            "rounds_us",
            format!("(last line, delimited) per size and round: {rounds:?}"),
        );
        let grows = last[2] >= 3 * last[0].max(1) && last[2] >= 1_000;
        let flat = delimited.iter().all(|us| *us < 1_000);
        recorder.axiom(
            grows,
            "a point below the layout, on a last line with a hidden run, costs a lookup growing \
             with the hidden bytes",
        )?;
        recorder.axiom(
            flat,
            "with a delimiter after the hidden run, the same lookup stays under a millisecond",
        )
    })
}
