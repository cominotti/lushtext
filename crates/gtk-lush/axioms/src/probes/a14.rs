// SPDX-License-Identifier: MIT OR Apache-2.0

//! A14: how an `AdwBreakpoint` condition `max-width: N sp` compares with the
//! width it is evaluated against, and how that depends on the text scale.
//!
//! The probe allocates an `AdwBreakpointBin` at exact widths and bisects the
//! widest width at which the breakpoint is current, under three text scales
//! set through `GtkSettings:gtk-xft-dpi`. It then checks the same boundary on
//! real `AdwWindow`s, whose breakpoints go through the same bin, and whether a
//! text-scale change alone re-evaluates a breakpoint.

use gtk4::prelude::*;
use libadwaita::prelude::*;

use super::LAYOUT_SETTLE;
use super::adaptive::{BreakpointFixture, SettingsOverride, max_width_sp, xft_dpi_for_scale};
use crate::observation::{Recorder, Stop};
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

/// The condition the probe bisects, in sp. Odd on purpose: at 1.25 and 1.5
/// it is a fractional pixel width, which shows how the boundary rounds.
pub const CONDITION_SP: i32 = 601;
/// The text scales the probe bisects under, in permille.
pub const TEXT_SCALES: [i32; 3] = [1000, 1250, 1500];
/// The narrowest width the bisection starts from (the breakpoint applies).
pub(crate) const SEARCH_LOW: i32 = 200;
/// The widest width the bisection starts from (the breakpoint does not).
pub(crate) const SEARCH_HIGH: i32 = 2_000;
/// The setter value the breakpoint writes.
pub const APPLIED_LABEL: &str = "applied";
/// The width the text-scale re-evaluation step rests at: unapplied at 1.0
/// (601 px), applied at 1.25 (751.25 px).
pub const RESCALE_WIDTH: i32 = 700;
/// Size requests of the windows the boundary cross-check presents.
const WINDOW_MIN: i32 = 200;
/// Height of those windows.
const WINDOW_HEIGHT: i32 = 300;

/// `CONDITION_SP` × `scale_permille` / 1000, rounded down: the widest whole
/// pixel width not above the scaled condition.
const fn floor_scaled(scale_permille: i32) -> i32 {
    CONDITION_SP * scale_permille / 1000
}

/// Whether the breakpoint is current after allocating the bin at `width`.
fn applies_at(
    recorder: &mut Recorder,
    fixture: &BreakpointFixture,
    breakpoint: &libadwaita::Breakpoint,
    width: i32,
) -> Result<bool, Stop> {
    let allocated = fixture.allocate_at(width);
    recorder.control(allocated, "control: the bin is re-allocated at every width")?;
    Ok(fixture.is_current(breakpoint))
}

/// Whether an `AdwWindow` presented `width` wide applies the breakpoint, and
/// its measured width.
fn window_applies_at(recorder: &mut Recorder, width: i32) -> Result<(bool, i32), Stop> {
    let window = libadwaita::Window::builder()
        .default_width(width)
        .default_height(WINDOW_HEIGHT)
        .width_request(WINDOW_MIN)
        .height_request(WINDOW_MIN)
        .content(&gtk4::Label::new(Some("window breakpoint")))
        .build();
    let Ok(condition) = libadwaita::BreakpointCondition::parse(&max_width_sp(CONDITION_SP)) else {
        recorder.control(false, "control: the condition parses")?;
        return Ok((false, 0));
    };
    window.add_breakpoint(libadwaita::Breakpoint::new(condition));
    let shown = Presented::checked_window(recorder, window, "control: the window realizes")?;
    let window = shown.window();
    Ok((window.current_breakpoint().is_some(), window.width()))
}

/// Probe A14. See the module documentation.
#[must_use]
pub fn probe_a14() -> Observation {
    Recorder::run(AxiomId::new(14), |recorder| {
        let settings = SettingsOverride::new();
        recorder.measure("session_xft_dpi", settings.xft_dpi().unwrap_or(-1));
        recorder.control(
            settings.available(),
            "control: a default GtkSettings exists",
        )?;
        let fixture = BreakpointFixture::new(SEARCH_LOW);
        let Some(breakpoint) = fixture.add_max_width_breakpoint(CONDITION_SP, APPLIED_LABEL) else {
            return recorder.control(false, "control: the condition parses");
        };
        let _shown = Presented::checked(
            recorder,
            &fixture.host,
            "control: the fixture window realizes",
        )?;

        let mut boundaries = Vec::new();
        for (scale, keys) in TEXT_SCALES.into_iter().zip([
            (
                "scale_1000",
                "sp_to_px_1000",
                "last_applied_1000",
                "first_unapplied_1000",
            ),
            (
                "scale_1250",
                "sp_to_px_1250",
                "last_applied_1250",
                "first_unapplied_1250",
            ),
            (
                "scale_1500",
                "sp_to_px_1500",
                "last_applied_1500",
                "first_unapplied_1500",
            ),
        ]) {
            settings.set_text_scale(scale);
            settle(LAYOUT_SETTLE);
            recorder.measure(keys.0, settings.xft_dpi().unwrap_or(-1));
            recorder.measure(keys.1, settings.sp_to_px(f64::from(CONDITION_SP)));
            recorder.control(
                settings.xft_dpi() == Some(xft_dpi_for_scale(scale)),
                "control: the text scale takes effect",
            )?;
            let low = applies_at(recorder, &fixture, &breakpoint, SEARCH_LOW)?;
            let high = applies_at(recorder, &fixture, &breakpoint, SEARCH_HIGH)?;
            recorder.control(
                low && !high,
                "control: the breakpoint applies at the narrow end and not at the wide end",
            )?;
            let (mut applied, mut unapplied) = (SEARCH_LOW, SEARCH_HIGH);
            while unapplied - applied > 1 {
                let middle = applied + (unapplied - applied) / 2;
                if applies_at(recorder, &fixture, &breakpoint, middle)? {
                    applied = middle;
                } else {
                    unapplied = middle;
                }
            }
            recorder.measure(keys.2, applied);
            recorder.measure(keys.3, unapplied);
            boundaries.push((scale, applied));
        }

        // A text-scale change alone, with the bin resting at one width.
        settings.set_text_scale(1000);
        let rested = applies_at(recorder, &fixture, &breakpoint, RESCALE_WIDTH)?;
        let allocations_before_rescale = fixture.host.allocations();
        settings.set_text_scale(1250);
        settle(LAYOUT_SETTLE);
        let applied_after_rescale = fixture.is_current(&breakpoint);
        recorder.measure("rescale_applied_before", rested);
        recorder.measure("rescale_applied_after", applied_after_rescale);
        recorder.measure(
            "rescale_host_allocations",
            fixture.host.allocations() - allocations_before_rescale,
        );
        recorder.measure("rescale_bin_width", fixture.bin.width());
        recorder.control(
            !rested,
            "control: at 1.0 the rest width is outside the condition",
        )?;

        // The window-level cross-check, at the boundary the bin showed.
        let mut window_results = Vec::new();
        for (scale, keys) in [
            (1000, ("window_1000", "window_1000_width")),
            (1500, ("window_1500", "window_1500_width")),
        ] {
            settings.set_text_scale(scale);
            let edge = floor_scaled(scale);
            let (inside, inside_width) = window_applies_at(recorder, edge)?;
            let (outside, outside_width) = window_applies_at(recorder, edge + 1)?;
            recorder.measure(keys.0, format!("{edge}:{inside} {}:{outside}", edge + 1));
            recorder.measure(keys.1, format!("{inside_width} {outside_width}"));
            recorder.control(
                inside_width == edge && outside_width == edge + 1,
                "control: each window is allocated the width it asked for",
            )?;
            window_results.push((inside, outside));
        }

        for (scale, applied) in boundaries {
            recorder.axiom(
                applied == floor_scaled(scale),
                "A14: max-width: N sp applies up to and including N × text scale px, rounded down",
            )?;
        }
        recorder.axiom(
            window_results
                .iter()
                .all(|&(inside, outside)| inside && !outside),
            "A14: an AdwWindow applies its breakpoint at the same boundary as the bin",
        )?;
        recorder.axiom(
            applied_after_rescale,
            "A14: a text-scale change alone re-evaluates the breakpoint",
        )
    })
}
