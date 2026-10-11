// SPDX-License-Identifier: MIT OR Apache-2.0

//! A30: an input method that asks for surrounding text gets the **cursor's
//! whole paragraph** from `GtkTextView` (its `retrieve-surrounding` handler
//! slices from the line's start to its end, widened by three words each
//! side), and the view arms that request on every caret move (through the
//! cursor-location update and the input-method reset), on cursor movement,
//! on typing, and on scrolling (the cursor location moves on screen). So input-method work per caret move grows with the cursor
//! paragraph's length, even with the window unfocused.
//!
//! The probe registers its own `GtkIMContext` on the `gtk-im-module`
//! extension point under a probe-local id and selects it for its view only,
//! through the view's `im-module` property; the runner's `GTK_IM_MODULE`
//! and the display's `gtk-im-module` setting are read before and after and
//! must be unchanged. Clicks are not driven (GTK 4 has no public way to
//! synthesize pointer events), and focus-in is recorded but not required
//! (the headless compositor does not activate the probe window); the
//! responsiveness lane drives both through the compositor. Recorded as
//! values: the requests per action (focus, caret move, `move-cursor`,
//! typing, scrolling) with each answer's length and duration.
//!
//! The protected view (C2) keeps the caret out of giant paragraphs, since
//! every caret move there would hand the whole paragraph to the input method.

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{
    ImRequest, PROBE_IM_ID, ParagraphKind, TextViewFixture, paragraph, register_probe_im,
    take_im_requests,
};
use crate::session::{Presented, flush_events, settle};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Characters in the probe's long paragraph (the buffer's second line).
pub const PARAGRAPH_CHARS: usize = 50_000;
/// Where the probe first places the caret: inside [`FIRST_LINE`].
pub const SHORT_LINE_OFFSET: i32 = 3;
/// The buffer's first line.
pub const FIRST_LINE: &str = "a short first line";

/// The probe text: [`FIRST_LINE`], the long paragraph, and a short last line.
#[must_use]
pub fn probe_text() -> String {
    format!(
        "{FIRST_LINE}\n{}\nshort last line",
        paragraph(ParagraphKind::Ascii, PARAGRAPH_CHARS, 30)
    )
}

/// A view over [`probe_text`] that uses the probe input method, or `None`
/// when the extension point does not list it.
#[must_use]
pub fn probe_im_view() -> Option<TextViewFixture> {
    if !register_probe_im() {
        return None;
    }
    let fixture = TextViewFixture::new(gtk4::WrapMode::WordChar);
    fixture.view.set_im_module(Some(PROBE_IM_ID));
    fixture.buffer.set_text(&probe_text());
    fixture.buffer.place_cursor(&fixture.buffer.start_iter());
    Some(fixture)
}

/// The requests one action armed, summarised as
/// `trigger:chars:micros` entries.
#[must_use]
pub fn describe(requests: &[ImRequest]) -> String {
    super::join_entries(requests.iter().map(|request| {
        format!(
            "{}:{}:{}us",
            request.trigger,
            request
                .surrounding_chars
                .map_or_else(|| "none".to_owned(), |chars| chars.to_string()),
            request.micros
        )
    }))
}

fn largest_surrounding(requests: &[ImRequest]) -> usize {
    requests
        .iter()
        .filter_map(|request| request.surrounding_chars)
        .max()
        .unwrap_or(0)
}

/// Probe A30.
#[must_use]
pub fn probe_a30() -> Observation {
    Recorder::run(AxiomId::new(30), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let env_before = std::env::var("GTK_IM_MODULE").ok();
        let settings = gtk4::Settings::default();
        let setting_before = settings.as_ref().and_then(gtk4::Settings::gtk_im_module);
        recorder.measure("gtk_im_module_env", format!("{env_before:?}"));
        recorder.measure("gtk_im_module_setting", format!("{setting_before:?}"));
        let fixture = recorder.require(
            probe_im_view(),
            "control: the probe input method is registered on gtk-im-module",
        )?;
        let _shown = Presented::checked(
            recorder,
            &fixture.scrolled,
            "control: the text view was presented",
        )?;
        let _ = take_im_requests();

        fixture.view.grab_focus();
        settle(LAYOUT_SETTLE);
        let focus = take_im_requests();
        recorder.measure("focus_requests", describe(&focus));
        recorder.measure(
            "window_is_active",
            fixture
                .view
                .root()
                .and_downcast::<gtk4::Window>()
                .is_some_and(|window| window.is_active()),
        );

        fixture
            .buffer
            .place_cursor(&fixture.buffer.iter_at_offset(SHORT_LINE_OFFSET));
        settle(LAYOUT_SETTLE);
        let short_line = take_im_requests();
        recorder.measure("short_line_caret_requests", describe(&short_line));
        recorder.control(
            !short_line.is_empty(),
            "control: GTK selected the probe input method through the view's im-module",
        )?;

        let middle = i32::try_from(FIRST_LINE.len() + 1 + PARAGRAPH_CHARS / 2).unwrap_or(0);
        fixture
            .buffer
            .place_cursor(&fixture.buffer.iter_at_offset(middle));
        settle(LAYOUT_SETTLE);
        let caret = take_im_requests();
        recorder.measure("caret_move_requests", describe(&caret));

        fixture
            .view
            .emit_move_cursor(gtk4::MovementStep::VisualPositions, 1, false);
        settle(LAYOUT_SETTLE);
        let arrow = take_im_requests();
        recorder.measure("move_cursor_requests", describe(&arrow));

        fixture.view.emit_insert_at_cursor("x");
        settle(LAYOUT_SETTLE);
        let typing = take_im_requests();
        recorder.measure("insert_at_cursor_requests", describe(&typing));

        let adjustment = fixture.scrolled.vadjustment();
        adjustment.set_value(adjustment.value() + 200.0);
        settle(LAYOUT_SETTLE);
        let scroll = take_im_requests();
        recorder.measure("scroll_requests", describe(&scroll));
        recorder.measure("clicks", "not driven: no synthetic pointer events in GTK 4");
        flush_events();

        let env_after = std::env::var("GTK_IM_MODULE").ok();
        let setting_after = settings.as_ref().and_then(gtk4::Settings::gtk_im_module);
        recorder.control(
            env_after == env_before && setting_after == setting_before,
            "control: GTK_IM_MODULE and the display's gtk-im-module are unchanged",
        )?;
        let in_paragraph = largest_surrounding(&caret).max(largest_surrounding(&arrow));
        recorder.measure("largest_surrounding_in_paragraph", in_paragraph);
        recorder.axiom(
            !caret.is_empty() && !arrow.is_empty() && !typing.is_empty() && !scroll.is_empty(),
            "caret moves, cursor movement, typing and scrolling each arm a surrounding-text request",
        )?;
        recorder.axiom(
            in_paragraph >= PARAGRAPH_CHARS,
            "the surrounding text spans the cursor's whole paragraph",
        )?;
        recorder.axiom(
            largest_surrounding(&short_line) < PARAGRAPH_CHARS,
            "with the cursor on a short line, the surrounding text is that line, not the buffer",
        )
    })
}
