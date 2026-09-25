// SPDX-License-Identifier: MIT OR Apache-2.0

//! A18: how breakpoints change a window's minimum width, and what happens
//! below the smallest breakpoint's condition.
//!
//! Three `AdwWindow`s carry the same content, whose minimum width is
//! [`CONTENT_MINIMUM`]: one without breakpoints or a size request, one with a
//! size request only, and one with a size request and a breakpoint whose
//! setter lowers the content's minimum. The probe measures each window's
//! minimum width, and presents the last one asking for less than its size
//! request. Then two `max-width` breakpoints are installed on one bin, in
//! both orders, and the bin is allocated below both conditions, where both
//! match.

use gtk4::prelude::*;
use libadwaita::prelude::*;

use super::adaptive::{BreakpointFixture, max_width_condition};
use crate::observation::{Recorder, Stop};
use crate::session::Presented;
use crate::{AxiomId, Observation};

/// The content's minimum width while no breakpoint applies.
pub const CONTENT_MINIMUM: i32 = 700;
/// The content's minimum width while the breakpoint applies.
pub const CONTENT_MINIMUM_NARROW: i32 = 100;
/// The window's size request, below the content's minimum.
pub const WINDOW_REQUEST: i32 = 300;
/// The window breakpoint's condition, in sp.
pub const WINDOW_CONDITION_SP: i32 = 500;
/// The window width asked for, wide enough for the content.
pub(crate) const WINDOW_WIDTH: i32 = 1_000;
/// A window width asked for below the size request.
pub const BELOW_REQUEST: i32 = 250;
/// The height of every window.
const WINDOW_HEIGHT: i32 = 300;
/// The wider bin breakpoint, in sp.
pub const WIDE_SP: i32 = 900;
/// The narrower bin breakpoint, in sp.
pub const NARROW_SP: i32 = 500;
/// A bin width inside only the wider condition.
pub const BETWEEN_WIDTH: i32 = 700;
/// A bin width inside both conditions.
pub const BELOW_BOTH_WIDTH: i32 = 450;

/// Build a window around content whose minimum width is
/// [`CONTENT_MINIMUM`], optionally with a size request and a breakpoint that
/// lowers that minimum to [`CONTENT_MINIMUM_NARROW`].
fn content_window(
    recorder: &mut Recorder,
    request: bool,
    breakpoint: bool,
    width: i32,
) -> Result<(libadwaita::Window, gtk4::Label), Stop> {
    let content = gtk4::Label::new(Some("content with a large minimum"));
    content.set_width_request(CONTENT_MINIMUM);
    let window = libadwaita::Window::builder()
        .default_width(width)
        .default_height(WINDOW_HEIGHT)
        .content(&content)
        .build();
    if request {
        window.set_width_request(WINDOW_REQUEST);
        window.set_height_request(WINDOW_HEIGHT / 2);
    }
    if breakpoint {
        let Some(condition) = max_width_condition(WINDOW_CONDITION_SP) else {
            recorder.control(false, "control: the condition parses")?;
            return Ok((window, content));
        };
        let lowered = libadwaita::Breakpoint::new(condition);
        lowered.add_setter(
            &content,
            "width-request",
            Some(&CONTENT_MINIMUM_NARROW.to_value()),
        );
        window.add_breakpoint(lowered);
    }
    Ok((window, content))
}

/// The minimum width `window` reports.
fn minimum_width(window: &libadwaita::Window) -> i32 {
    window.measure(gtk4::Orientation::Horizontal, -1).0
}

/// Which of two breakpoints the bin applies below both conditions, with the
/// wider one installed first when `wide_first`.
fn below_both(recorder: &mut Recorder, wide_first: bool) -> Result<(String, String, String), Stop> {
    let fixture = BreakpointFixture::new(BETWEEN_WIDTH);
    let order = if wide_first {
        [(WIDE_SP, "wide"), (NARROW_SP, "narrow")]
    } else {
        [(NARROW_SP, "narrow"), (WIDE_SP, "wide")]
    };
    let mut installed = Vec::new();
    for (sp, value) in order {
        let Some(breakpoint) = fixture.add_max_width_breakpoint(sp, value) else {
            recorder.control(false, "control: the condition parses")?;
            return Ok((String::new(), String::new(), String::new()));
        };
        installed.push((breakpoint, value));
    }
    let _shown = Presented::checked(
        recorder,
        &fixture.host,
        "control: the fixture window realizes",
    )?;
    let current = |fixture: &BreakpointFixture| {
        installed
            .iter()
            .find(|(breakpoint, _)| fixture.is_current(breakpoint))
            .map_or("none", |(_, value)| value)
            .to_owned()
    };
    let between = current(&fixture);
    let allocated = fixture.allocate_at(BELOW_BOTH_WIDTH);
    recorder.control(
        allocated,
        "control: the bin is re-allocated below both conditions",
    )?;
    Ok((between, current(&fixture), fixture.label.text().to_string()))
}

/// Probe A18. See the module documentation.
#[must_use]
pub fn probe_a18() -> Observation {
    Recorder::run(AxiomId::new(18), |recorder| {
        let (plain, _) = content_window(recorder, false, false, WINDOW_WIDTH)?;
        let plain_minimum = minimum_width(&plain);
        let (requested, _) = content_window(recorder, true, false, WINDOW_WIDTH)?;
        let requested_minimum = minimum_width(&requested);
        let (adaptive, adaptive_content) = content_window(recorder, true, true, WINDOW_WIDTH)?;
        let adaptive_minimum = minimum_width(&adaptive);
        recorder.measure("minimum_without_breakpoints", plain_minimum);
        recorder.measure("minimum_with_request_only", requested_minimum);
        recorder.measure("minimum_with_request_and_breakpoint", adaptive_minimum);
        plain.destroy();
        requested.destroy();
        {
            let shown = Presented::checked_window(
                recorder,
                adaptive,
                "control: the adaptive window realizes",
            )?;
            recorder.measure("adaptive_width", shown.window().width());
            recorder.measure(
                "adaptive_breakpoint_applied",
                shown.window().current_breakpoint().is_some(),
            );
            recorder.measure("adaptive_content_width", adaptive_content.width());
        }
        let (narrow, narrow_content) = content_window(recorder, true, true, BELOW_REQUEST)?;
        let shown =
            Presented::checked_window(recorder, narrow, "control: the narrow window realizes")?;
        let narrow_width = shown.window().width();
        let narrow_applied = shown.window().current_breakpoint().is_some();
        let narrow_content_width = narrow_content.width();
        recorder.measure("below_request_width", narrow_width);
        recorder.measure("below_request_breakpoint_applied", narrow_applied);
        recorder.measure("below_request_content_width", narrow_content_width);
        drop(shown);

        let (between_wide_first, below_wide_first, label_wide_first) = below_both(recorder, true)?;
        let (between_narrow_first, below_narrow_first, label_narrow_first) =
            below_both(recorder, false)?;
        recorder.measure("wide_first_between", &between_wide_first);
        recorder.measure("wide_first_below_both", &below_wide_first);
        recorder.measure("wide_first_label", &label_wide_first);
        recorder.measure("narrow_first_between", &between_narrow_first);
        recorder.measure("narrow_first_below_both", &below_narrow_first);
        recorder.measure("narrow_first_label", &label_narrow_first);
        recorder.control(
            between_wide_first == "wide" && between_narrow_first == "wide",
            "control: between the conditions only the wider breakpoint matches",
        )?;

        recorder.axiom(
            plain_minimum == CONTENT_MINIMUM && requested_minimum == CONTENT_MINIMUM,
            "A18: without breakpoints the window's minimum is its content's, size request or not",
        )?;
        recorder.axiom(
            adaptive_minimum == WINDOW_REQUEST,
            "A18: with a breakpoint the window's minimum is its size request, not its content's",
        )?;
        recorder.axiom(
            narrow_width == WINDOW_REQUEST && narrow_applied,
            "A18: a window asked narrower than its size request is allocated the request",
        )?;
        recorder.axiom(
            below_wide_first == "narrow" && below_narrow_first == "wide",
            "A18: where several breakpoints match, only the last one added applies",
        )?;
        recorder.axiom(
            label_wide_first == "narrow" && label_narrow_first == "wide",
            "A18: the other matching breakpoints' setters are not applied",
        )
    })
}
