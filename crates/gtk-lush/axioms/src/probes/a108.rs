// SPDX-License-Identifier: MIT OR Apache-2.0

//! A108: the frame after the text of every realized `GtkListView` row
//! changes costs layout in proportion to the realized rows when the rows'
//! text is a `GtkLabel`: each label shapes its text to report its size, so
//! the frame re-measures every realized row (more than half the frame is
//! layout at about 205 realized rows, and it grows from 16 to 205). The same
//! frame with a text widget whose measure ignores its text has almost no
//! layout and does not grow: it redraws the rows on screen.
//!
//! LushText's `LushtextRowText` (`lighten-notes-browser-rows` D2) is that
//! widget for the notes browser and palette rows; on `main` the notes
//! browser's query frame spent 18–19 ms in this layout.

use std::time::Duration;

use super::row_lists::{
    BAND_MODEL_ROWS, FEW_ROWS, PhaseRecorder, RowText, WINDOW_SIZE, median, row_list,
};
use crate::observation::Recorder;
use crate::session::{Presented, flush_events, settle};
use crate::{AxiomId, Observation};

/// Retext rounds per case; the median is kept.
pub const ROUNDS: u32 = 5;
/// The share of the frame a label list's layout must take at the band.
pub const LAYOUT_SHARE_PERCENT: u64 = 50;
/// The growth from 16 to about 205 realized label rows that reads as
/// "grows with the rows".
pub const GROWTH_FACTOR: u64 = 2;

/// The median frame after retexting every row of a `rows`-row list of
/// `kind`, and the rows realized.
#[must_use]
pub fn retext_frame(kind: RowText, rows: u32) -> (super::row_lists::FramePhases, u32) {
    let fixture = row_list(rows, kind);
    let window = libadwaita::Window::builder()
        .default_width(WINDOW_SIZE.0)
        .default_height(WINDOW_SIZE.1)
        .content(&fixture.column)
        .build();
    let shown = Presented::shown(window);
    let realized = fixture.realized_rows();
    let Some(recorder) = PhaseRecorder::new(&fixture.list) else {
        drop(shown);
        return (super::row_lists::FramePhases::default(), realized);
    };
    let mut frames = Vec::new();
    for generation in 1..=ROUNDS {
        fixture.retext(generation);
        if let Some(frame) = recorder.next_frame(Duration::from_secs(2)) {
            frames.push(frame);
        }
        settle(Duration::from_millis(30));
    }
    drop(recorder);
    drop(shown);
    flush_events();
    let pick = |field: fn(&super::row_lists::FramePhases) -> u64| {
        median(frames.iter().map(field).collect())
    };
    (
        super::row_lists::FramePhases {
            total_us: pick(|frame| frame.total_us),
            layout_us: pick(|frame| frame.layout_us),
            paint_us: pick(|frame| frame.paint_us),
        },
        realized,
    )
}

/// Probe A108. See the module documentation.
#[must_use]
pub fn probe_a108() -> Observation {
    Recorder::run(AxiomId::new(108), |recorder| {
        let (label_few, few_rows) = retext_frame(RowText::Label, FEW_ROWS);
        let (label_band, band_rows) = retext_frame(RowText::Label, BAND_MODEL_ROWS);
        let (free_few, _) = retext_frame(RowText::MeasureFree, FEW_ROWS);
        let (free_band, free_band_rows) = retext_frame(RowText::MeasureFree, BAND_MODEL_ROWS);
        recorder.measure("few_realized_rows", few_rows);
        recorder.measure("band_realized_rows", band_rows);
        recorder.measure("label_few_layout_us", label_few.layout_us);
        recorder.measure("label_few_total_us", label_few.total_us);
        recorder.measure("label_band_layout_us", label_band.layout_us);
        recorder.measure("label_band_total_us", label_band.total_us);
        recorder.measure("measure_free_few_layout_us", free_few.layout_us);
        recorder.measure("measure_free_band_layout_us", free_band.layout_us);
        recorder.measure("measure_free_band_total_us", free_band.total_us);
        recorder.control(
            few_rows == FEW_ROWS && band_rows >= 150 && free_band_rows >= 150,
            "control: 16 rows realize in full, 16,384 realize the band (A1)",
        )?;
        recorder.control(
            label_band.total_us > 0 && free_band.total_us > 0,
            "control: a frame painted after each retext",
        )?;
        recorder.axiom(
            label_band.layout_us * 100 >= label_band.total_us * LAYOUT_SHARE_PERCENT
                && label_band.layout_us >= label_few.layout_us * GROWTH_FACTOR,
            "A108: after a retext, a label list's frame is mostly layout and grows with the \
             realized rows",
        )?;
        recorder.axiom(
            free_band.layout_us * 4 <= label_band.layout_us
                && free_band.layout_us <= free_few.layout_us * GROWTH_FACTOR + 1_000,
            "A108: with a text-independent measure the frame has little layout and does not grow",
        )
    })
}
