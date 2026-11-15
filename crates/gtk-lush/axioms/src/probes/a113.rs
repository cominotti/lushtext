// SPDX-License-Identifier: MIT OR Apache-2.0

//! A113: a `GtkTextView`'s aligned scroll to its **first line**
//! (`scroll_to_iter` or `scroll_to_mark` with `use_align` and `yalign` 0)
//! puts that line's first character box at the top edge of the view, so the
//! vertical adjustment comes to rest at the view's top margin plus the box's
//! location, **never at `lower`**: the top margin scrolls out of sight. GTK
//! moves there through `gtk_adjustment_animate_to_value`, so a scrolled
//! window that animates its adjustments gets there over several
//! `value-changed` emissions (measured here, not asserted).
//!
//! LushText puts a reattached editor view back at its kept top line
//! (`ui/editor_page/view_attachment.rs`). For the first line it sets the value
//! to `lower` itself instead of asking GTK for the aligned scroll, so a tab
//! resting at its top comes back resting there, its top-anchor repair armed
//! (`diagnose-ci-only-reflow-guard-failure`).

use gtk4::prelude::*;

use super::LAYOUT_SETTLE;
use super::text::TextViewFixture;
use crate::observation::Recorder;
use crate::session::{Presented, settle, wait_until};
use crate::{AxiomId, Observation};

/// Lines in the probe's document.
pub const DOCUMENT_LINES: u32 = 400;
/// The view's top margin, in pixels (LushText's editor uses 6).
pub const TOP_MARGIN: i32 = 6;

/// An unwrapped text view holding [`DOCUMENT_LINES`] short lines, with a
/// [`TOP_MARGIN`] top margin.
#[must_use]
pub fn document_view() -> TextViewFixture {
    let fixture = TextViewFixture::new(gtk4::WrapMode::None);
    fixture.view.set_top_margin(TOP_MARGIN);
    let text: String = (0..DOCUMENT_LINES)
        .map(|line| format!("line {line}\n"))
        .collect();
    fixture.buffer.set_text(&text);
    fixture
}

/// Scroll `fixture` to its first line with GTK's aligned scroll, through an
/// iterator (`by_mark` false) or a mark (`by_mark` true).
pub fn scroll_to_first_line(fixture: &TextViewFixture, by_mark: bool) {
    let mut start = fixture.buffer.start_iter();
    if by_mark {
        let mark = fixture.buffer.create_mark(None, &start, true);
        fixture.view.scroll_to_mark(&mark, 0.0, true, 0.0, 0.0);
        fixture.buffer.delete_mark(&mark);
    } else {
        fixture.view.scroll_to_iter(&mut start, 0.0, true, 0.0, 0.0);
    }
}

/// Count `value-changed` emissions of `adjustment` from now on.
fn count_value_changes(adjustment: &gtk4::Adjustment) -> std::rc::Rc<std::cell::Cell<u32>> {
    let count = std::rc::Rc::new(std::cell::Cell::new(0));
    let counter = std::rc::Rc::clone(&count);
    adjustment.connect_value_changed(move |_| counter.set(counter.get() + 1));
    count
}

/// Probe A113.
#[must_use]
pub fn probe_a113() -> Observation {
    Recorder::run(AxiomId::new(113), |recorder| {
        let fixture = document_view();
        let _shown = Presented::checked(
            recorder,
            &fixture.scrolled,
            "control: the text view was presented",
        )?;
        let vadjustment = fixture.scrolled.vadjustment();
        let laid_out = wait_until(std::time::Duration::from_secs(5), || {
            vadjustment.upper() > vadjustment.page_size() * 2.0 && vadjustment.page_size() > 0.0
        });
        settle(LAYOUT_SETTLE);
        recorder.measure("upper", vadjustment.upper());
        recorder.measure("page_size", vadjustment.page_size());
        recorder.control(
            laid_out
                && fixture.view.top_margin() == TOP_MARGIN
                && (vadjustment.value() - vadjustment.lower()).abs() <= 0.5,
            "control: the view is laid out, taller than its page, has its top margin, and \
             rests at lower",
        )?;
        let first = fixture.view.iter_location(&fixture.buffer.start_iter()).y();
        recorder.measure("first_char_location_y", first);
        let expected = f64::from(TOP_MARGIN + first);

        for (by_mark, key, moves_key, check) in [
            (
                false,
                "value_after_scroll_to_iter",
                "value_changes_scroll_to_iter",
                "scroll_to_iter to the first line rests at the top margin plus the first \
                 character's location, not at lower",
            ),
            (
                true,
                "value_after_scroll_to_mark",
                "value_changes_scroll_to_mark",
                "scroll_to_mark to the first line rests at the top margin plus the first \
                 character's location, not at lower",
            ),
        ] {
            vadjustment.set_value(vadjustment.lower());
            settle(LAYOUT_SETTLE);
            let moves = count_value_changes(&vadjustment);
            scroll_to_first_line(&fixture, by_mark);
            // The move may be animated: wait until it has come to rest.
            let _ = wait_until(std::time::Duration::from_secs(3), || {
                (vadjustment.value() - expected).abs() <= 0.5
            });
            settle(LAYOUT_SETTLE * 3);
            recorder.measure(key, vadjustment.value());
            recorder.measure(moves_key, moves.get());
            recorder.axiom(
                (vadjustment.value() - expected).abs() <= 0.5
                    && vadjustment.value() > vadjustment.lower() + 0.5,
                check,
            )?;
        }
        Ok(())
    })
}
