// SPDX-License-Identifier: MIT OR Apache-2.0

//! A31: after the on-screen part, `GtkTextView` validates (lays out) the rest
//! of a buffer in idle batches of **2000 px**: each batch grows the vertical
//! adjustment's `upper` by about 2000 px (2000 plus at most one line), so a
//! document of height `H` settles after about `H / 2000` batches.
//!
//! The observation is indirect: the batch size is read from how `upper`
//! grows between main-loop dispatches, not from the private validation call.
//! Recorded beside it: the rows per batch with wrapping off, which the
//! document scale policy (C1) uses as `R_batch`.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::TextViewFixture;
use crate::session::{Presented, flush_events};
use crate::{AxiomId, Observation};

/// Lines in the probe document.
pub const LINES: usize = 6_000;
/// The validation batch height GTK uses, in pixels.
pub const BATCH_PX: f64 = 2_000.0;
/// Most main-loop dispatches the probe drives before giving up.
pub const MAX_DISPATCHES: usize = 20_000;

/// The probe document: [`LINES`] short lines.
#[must_use]
pub fn short_lines() -> String {
    (0..LINES).map(|line| format!("line {line:05}\n")).collect()
}

/// Load the document into `fixture` and record `upper` each time it changes
/// while the main loop runs to idle. Returns the successive `upper` values.
#[must_use]
pub fn upper_growth(fixture: &TextViewFixture) -> Vec<f64> {
    let adjustment = fixture.scrolled.vadjustment();
    let uppers = Rc::new(RefCell::new(vec![adjustment.upper()]));
    let recorder = uppers.clone();
    let handler = adjustment.connect_changed(move |adjustment| {
        let upper = adjustment.upper();
        let mut values = recorder.borrow_mut();
        if values
            .last()
            .is_none_or(|last| (last - upper).abs() > f64::EPSILON)
        {
            values.push(upper);
        }
    });
    fixture.buffer.set_text(&short_lines());
    let context = gtk4::glib::MainContext::default();
    let mut idle_dispatches = 0usize;
    while idle_dispatches < MAX_DISPATCHES && context.iteration(false) {
        idle_dispatches += 1;
    }
    flush_events();
    adjustment.disconnect(handler);
    uppers.take()
}

/// Probe A31.
#[must_use]
pub fn probe_a31() -> Observation {
    Recorder::run(AxiomId::new(31), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let fixture = TextViewFixture::new(gtk4::WrapMode::None);
        let _shown = Presented::checked(
            recorder,
            &fixture.scrolled,
            "control: the text view was presented",
        )?;
        let uppers = upper_growth(&fixture);
        let line_height = f64::from(
            fixture
                .view
                .iter_location(&fixture.buffer.start_iter())
                .height(),
        );
        let final_upper = uppers.last().copied().unwrap_or_default();
        let deltas: Vec<f64> = uppers
            .array_windows::<2>()
            .map(|[before, after]| after - before)
            .filter(|delta| *delta > 0.0)
            .collect();
        let mut sorted = deltas.clone();
        sorted.sort_by(f64::total_cmp);
        let median = sorted.get(sorted.len() / 2).copied().unwrap_or_default();
        recorder.measure("line_height", line_height);
        recorder.measure("final_upper", final_upper);
        recorder.measure("upper_steps", deltas.len());
        recorder.measure("expected_steps_about", (final_upper / BATCH_PX).round());
        recorder.measure("median_step_px", median);
        recorder.measure("first_steps_px", super::join_entries(deltas.iter().take(6)));
        recorder.measure(
            "rows_per_batch_nowrap",
            (median / line_height.max(1.0)).round(),
        );
        recorder.control(
            line_height > 0.0 && final_upper > 10.0 * BATCH_PX,
            "control: the document is many batches tall",
        )?;
        recorder.axiom(
            median >= BATCH_PX && median <= BATCH_PX + line_height,
            "a validation batch grows upper by 2000 px plus at most one line",
        )?;
        recorder.axiom(
            deltas.len() as f64 >= (final_upper / BATCH_PX) * 0.5,
            "the document settles over about upper / 2000 batches",
        )
    })
}
