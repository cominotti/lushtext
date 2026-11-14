// SPDX-License-Identifier: MIT OR Apache-2.0

//! A111: under the cairo renderer, drawing an `AdwDialog`'s open or close
//! animation frame costs several milliseconds whatever the dialog holds:
//! the animation draws the dialog as one scaled, faded node, which cairo
//! composites in software (pixman's bilinear path), about as much at 8 list
//! rows as at 205. The GL renderer draws the same node in a fraction of that.
//!
//! The probe presents an `AdwDialog` holding a 16-row and a 16,384-row list
//! (all 16 and about 205 rows realized, A1) over a 1000×760 window and reads
//! the paint phase of the frames while it opens, under the active renderer.
//! The GTK axiom lane runs cairo, so the axiom is asserted there and the GL
//! half is skipped, saying why; run with `GSK_RENDERER=gl`, the probe records
//! the GL figures and skips the cairo assertion, saying why.
//!
//! LushText's widget harness runs `GSK_RENDERER=cairo`, so its dialogs'
//! animation frames cost 16–30 ms there: a property of the test renderer,
//! not of the product (`lighten-notes-browser-rows` D1, D6).

use std::time::{Duration, Instant};

use gtk4::prelude::*;
use libadwaita::prelude::AdwDialogExt;

use super::row_lists::{BAND_MODEL_ROWS, FEW_ROWS, RowText, WINDOW_SIZE, median, row_list};
use crate::observation::Recorder;
use crate::session::{Presented, flush_events, settle};
use crate::{AxiomId, Observation};

/// The least a cairo animation frame of the fixture costs, microseconds.
pub const SEVERAL_MS_US: u64 = 2_000;

/// The paint phase of the frames of an `AdwDialog` holding a list of `rows`
/// rows while it opens (its animation), median microseconds, and the active
/// renderer's type name.
#[must_use]
pub fn dialog_animation_paint_us(rows: u32) -> (u64, usize, String) {
    let host = gtk4::Label::new(Some("host"));
    let window = libadwaita::Window::builder()
        .default_width(WINDOW_SIZE.0)
        .default_height(WINDOW_SIZE.1)
        .content(&host)
        .build();
    let shown = Presented::shown(window);
    let renderer = shown.window().renderer().map_or_else(
        || "none".to_owned(),
        |renderer| renderer.type_().name().to_owned(),
    );
    let fixture = row_list(rows, RowText::Label);
    let dialog = libadwaita::Dialog::builder()
        .content_width(640)
        .content_height(560)
        .child(&fixture.column)
        .build();
    let mut paints = Vec::new();
    if let Some(recorder) = super::row_lists::PhaseRecorder::new(shown.window()) {
        AdwDialogExt::present(&dialog, Some(shown.window()));
        let deadline = Instant::now() + ANIMATION_WINDOW;
        while Instant::now() < deadline {
            if let Some(frame) = recorder.next_frame(Duration::from_millis(100)) {
                paints.push(frame.paint_us);
            }
        }
    }
    let frames = paints.len();
    dialog.force_close();
    settle(Duration::from_millis(100));
    drop(shown);
    flush_events();
    (median(paints), frames, renderer)
}

/// How long the probe records frames after presenting the dialog: the
/// open animation and its settle.
pub const ANIMATION_WINDOW: Duration = Duration::from_millis(400);

/// Probe A111. See the module documentation.
#[must_use]
pub fn probe_a111() -> Observation {
    Recorder::run(AxiomId::new(111), |recorder| {
        let (few, few_frames, renderer) = dialog_animation_paint_us(FEW_ROWS);
        let (band, band_frames, _) = dialog_animation_paint_us(BAND_MODEL_ROWS);
        recorder.measure("renderer", &renderer);
        recorder.measure("few_rows_animation_paint_us", few);
        recorder.measure("band_rows_animation_paint_us", band);
        recorder.measure("few_rows_frames", few_frames);
        recorder.measure("band_rows_frames", band_frames);
        recorder.control(
            few_frames >= 3 && band_frames >= 3,
            "control: the dialog's opening painted frames",
        )?;
        if renderer != "GskCairoRenderer" {
            // The figures above are this renderer's (GL where it is the
            // active one): recorded, the cairo half not asserted.
            recorder.measure(
                "cairo_half_skipped",
                "the active renderer is not cairo (the lane runs GSK_RENDERER=cairo)",
            );
            return Ok(());
        }
        recorder.measure(
            "gl_half_skipped",
            "the lane renders with cairo; run the sample with GSK_RENDERER=gl for the GL figure \
             (realizing a second, GL renderer in-process prints driver warnings the lane fails on)",
        );
        recorder.axiom(
            few >= SEVERAL_MS_US && band >= SEVERAL_MS_US && band < few.saturating_mul(3),
            "A111: under cairo a dialog animation frame costs several ms of paint, about as \
             much with 8 rows as with 205",
        )
    })
}
