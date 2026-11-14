// SPDX-License-Identifier: MIT OR Apache-2.0

//! A110: on a `GtkListView` that is not mapped, removing the rows GTK never
//! realized, after and before the realized band (one `items-changed` each),
//! destroys and creates no row widget; removing realized rows from the end of
//! what is left then destroys only those rows, at a cost in proportion to
//! them, and creates none.
//!
//! The probe scrolls a 16,384-row list to its middle (about 205 rows
//! realized, A1, plus the focus row GTK keeps at the top), hides it, removes
//! every row no list item is bound to (after the band, between the focus row
//! and the band), then 64 rows from the end, and compares that cost with a
//! full clear of an identical list. It also records, without asserting, how many
//! rows are created when the band is removed from the end while the
//! unrealized rows before it are still listed: the order LushText's sliced
//! retirement (`ui/list_retirement.rs`, `lighten-notes-browser-rows` D4)
//! avoids.

use std::time::{Duration, Instant};

use gtk4::prelude::*;

use super::row_lists::{BAND_MODEL_ROWS, RowList, RowText, WINDOW_SIZE, row_list};
use crate::observation::Recorder;
use crate::session::{Presented, flush_events, settle};
use crate::{AxiomId, Observation};

/// Where the list is scrolled before it is hidden.
pub const MIDDLE: u32 = BAND_MODEL_ROWS / 2;
/// Rows removed from the end of the band in one step.
pub const UNIT: u32 = 64;
/// How far a unit's cost may stray above its share of a full clear.
pub const COST_FACTOR: u64 = 2;
/// Slack for timer and scheduling noise on small costs, microseconds.
pub const COST_SLACK_US: u64 = 1_000;

/// A presented list scrolled to [`MIDDLE`], then hidden (unmapped).
#[must_use]
pub(crate) fn hidden_scrolled_list() -> (RowList, Presented) {
    let fixture = row_list(BAND_MODEL_ROWS, RowText::Label);
    let window = libadwaita::Window::builder()
        .default_width(WINDOW_SIZE.0)
        .default_height(WINDOW_SIZE.1)
        .content(&fixture.column)
        .build();
    let shown = Presented::shown(window);
    fixture
        .list
        .scroll_to(MIDDLE, gtk4::ListScrollFlags::NONE, None);
    settle(Duration::from_millis(300));
    fixture.scroller.set_visible(false);
    flush_events();
    (fixture, shown)
}

fn micros(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// Probe A110. See the module documentation.
#[must_use]
pub fn probe_a110() -> Observation {
    Recorder::run(AxiomId::new(110), |recorder| {
        let (fixture, shown) = hidden_scrolled_list();
        let realized = fixture.realized_rows();
        let span = fixture.bound_span();
        recorder.measure("realized_rows", realized);
        recorder.measure("bound_span", format!("{span:?}"));
        recorder.control(shown.realized(), "control: the window received a size")?;
        recorder.control(
            !fixture.list.is_mapped(),
            "control: the hidden list is unmapped",
        )?;
        let (_, high) = recorder.require(span, "control: the list holds a realized band")?;
        recorder.control(
            realized >= 150 && high + 1 < BAND_MODEL_ROWS && high > MIDDLE,
            "control: the band is realized in the middle of the list",
        )?;

        fixture.counters.reset();
        let ranges = fixture.remove_unbound();
        flush_events();
        let outside = (
            fixture.counters.setups.get(),
            fixture.counters.teardowns.get(),
        );
        recorder.measure("unbound_ranges", format!("{ranges:?}"));
        recorder.measure("outside_band_setups", outside.0);
        recorder.measure("outside_band_teardowns", outside.1);
        recorder.measure("rows_left", fixture.model.n_items());
        recorder.axiom(
            outside == (0, 0),
            "A110: removing the unrealized rows on both sides of the band destroys and creates \
             no row widget",
        )?;

        fixture.counters.reset();
        let len = fixture.model.n_items();
        let started = Instant::now();
        fixture.remove_tail(len.saturating_sub(UNIT));
        flush_events();
        let unit_us = micros(started);
        let unit = (
            fixture.counters.setups.get(),
            fixture.counters.teardowns.get(),
        );
        recorder.measure("unit_setups", unit.0);
        recorder.measure("unit_teardowns", unit.1);
        recorder.measure("unit_us", unit_us);
        drop(shown);
        drop(fixture);
        flush_events();

        let (full, full_shown) = hidden_scrolled_list();
        let full_realized = full.realized_rows();
        full.counters.reset();
        let started = Instant::now();
        full.remove_head(BAND_MODEL_ROWS);
        flush_events();
        let full_us = micros(started);
        let full_teardowns = full.counters.teardowns.get();
        recorder.measure("full_clear_us", full_us);
        recorder.measure("full_clear_teardowns", full_teardowns);
        drop(full_shown);
        drop(full);
        flush_events();

        let (wrong_order, wrong_shown) = hidden_scrolled_list();
        if let Some((_, high)) = wrong_order.bound_span() {
            wrong_order.remove_tail(high + 1);
            flush_events();
            wrong_order.counters.reset();
            for _ in 0..8 {
                let len = wrong_order.model.n_items();
                wrong_order.remove_tail(len.saturating_sub(UNIT));
                flush_events();
            }
            recorder.measure(
                "band_from_end_with_rows_before_setups",
                wrong_order.counters.setups.get(),
            );
        }
        drop(wrong_shown);
        drop(wrong_order);
        flush_events();

        recorder.control(
            full_teardowns > 0 && full_realized > 0,
            "control: a full clear destroys the realized rows",
        )?;
        let share = full_us.saturating_mul(u64::from(UNIT)) / u64::from(full_realized.max(1));
        recorder.measure("unit_share_of_full_us", share);
        recorder.axiom(
            unit.0 == 0 && unit.1 <= UNIT,
            "A110: removing realized rows from the end destroys at most those rows and creates \
             none",
        )?;
        recorder.axiom(
            unit_us <= share.saturating_mul(COST_FACTOR) + COST_SLACK_US,
            "A110: removing realized rows costs in proportion to them",
        )
    })
}
