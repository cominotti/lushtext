// SPDX-License-Identifier: MIT OR Apache-2.0

//! A34: with `cursor-visible` false, `GtkTextView`'s `move-cursor` (what the
//! arrow, Page and Home/End keys emit) moves **only the viewport** and leaves
//! the insert mark where it was; with `gtk-keynav-use-caret` set (caret
//! browsing), the same `move-cursor` moves the insert mark.
//!
//! The protected view (C2) turns the cursor off so navigation keys scroll a
//! giant-paragraph document without walking the caret through it.

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::TextViewFixture;
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Lines in the probe document.
pub const LINES: usize = 2_000;

/// The probe document.
#[must_use]
pub fn document() -> String {
    (0..LINES).map(|line| format!("line {line:05}\n")).collect()
}

/// A cursor-less view over [`document`] with the insert mark at the start.
#[must_use]
pub fn cursorless_view() -> TextViewFixture {
    let fixture = TextViewFixture::new(gtk4::WrapMode::None);
    fixture.view.set_cursor_visible(false);
    fixture.buffer.set_text(&document());
    fixture.buffer.place_cursor(&fixture.buffer.start_iter());
    fixture
}

/// The insert mark's offset and the vertical adjustment's value.
#[must_use]
pub fn caret_and_scroll(fixture: &TextViewFixture) -> (i32, f64) {
    let insert = fixture.buffer.iter_at_mark(&fixture.buffer.get_insert());
    (insert.offset(), fixture.scrolled.vadjustment().value())
}

/// Emit one Page Down `move-cursor` on `fixture`'s view.
pub fn page_down(fixture: &TextViewFixture) {
    fixture
        .view
        .emit_move_cursor(gtk4::MovementStep::Pages, 1, false);
}

/// Probe A34.
#[must_use]
pub fn probe_a34() -> Observation {
    Recorder::run(AxiomId::new(34), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let settings = recorder.require(gtk4::Settings::default(), "control: GTK has settings")?;
        let caret_browsing_before = settings.is_gtk_keynav_use_caret();
        recorder.measure("keynav_use_caret_before", caret_browsing_before);
        recorder.control(!caret_browsing_before, "control: caret browsing starts off")?;
        let fixture = cursorless_view();
        let _shown = Presented::checked(
            recorder,
            &fixture.scrolled,
            "control: the text view was presented",
        )?;
        fixture.view.grab_focus();
        settle(LAYOUT_SETTLE);
        let before = caret_and_scroll(&fixture);
        page_down(&fixture);
        settle(LAYOUT_SETTLE);
        let after = caret_and_scroll(&fixture);
        recorder.measure("cursorless_before", format!("{before:?}"));
        recorder.measure("cursorless_after_page_down", format!("{after:?}"));

        settings.set_gtk_keynav_use_caret(true);
        settle(LAYOUT_SETTLE);
        page_down(&fixture);
        settle(LAYOUT_SETTLE);
        let browsing = caret_and_scroll(&fixture);
        settings.set_gtk_keynav_use_caret(caret_browsing_before);
        settle(LAYOUT_SETTLE);
        recorder.measure("caret_browsing_after_page_down", format!("{browsing:?}"));
        recorder.measure(
            "keynav_use_caret_restored",
            settings.is_gtk_keynav_use_caret() == caret_browsing_before,
        );

        recorder.axiom(
            after.0 == before.0 && after.1 > before.1,
            "without a visible cursor, move-cursor scrolls the viewport and leaves the insert mark",
        )?;
        recorder.axiom(
            browsing.0 > after.0,
            "with gtk-keynav-use-caret set, move-cursor moves the insert mark",
        )
    })
}
