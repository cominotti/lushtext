// SPDX-License-Identifier: MIT OR Apache-2.0

//! A93: a presented `GtkListView` with a `header-factory`, over a
//! `GtkSingleSelection` over a custom lazy `GListModel` + `GtkSectionModel`,
//! answers **one** `items-changed(0, old, new)` that replaces realized rows by
//! calling `get_item` for **every added item**: its item manager looks each
//! added item up among the items it removed, to keep their widgets, and a
//! `GtkSingleSelection` holding a selected item searches the added items for
//! it. With fresh objects per call nothing matches, so the walk is complete
//! (131,073 calls for 131,072 rows, twice with a selection). Removing first,
//! `items-changed(0, old, 0)`, and then adding, `items-changed(0, 0, new)`,
//! leaves nothing to look up: the add asks only around the rows the list
//! realizes (A1), and the publish costs the same at 512 and at 131,072 rows.
//! A model may return a **fresh object** from every `get_item` call:
//! selection, `scroll_to` and rebinding keep working without stable identity.
//!
//! LushText's notes browser (`ui/window/notes/row_model.rs`,
//! `bound-notes-browser-by-budget` D2) publishes every match in those two
//! emissions, on this axiom, and keeps no row-object cache.

use std::time::{Duration, Instant};

use gtk4::prelude::*;

use super::lazy_sections::{CountingSectionModel, children_named, sectioned_list};
use crate::observation::Recorder;
use crate::session::{Presented, flush_events};
use crate::{AxiomId, Observation};

/// The published sizes compared (256 times the rows).
pub const SIZES: [u32; 2] = [512, 128 * 1024];
/// Rows per section.
pub const SECTION_LEN: u32 = 4_096;
/// The most `get_item` calls a two-step publish may make at either size:
/// about four times A1's realized-row cap.
pub const ITEM_CALL_CEILING: u64 = 1_024;
/// The largest growth of the two-step publish time, large over small, that
/// still reads as flat: a thirty-second of the size ratio.
pub const BOUNDED_GROWTH: f64 = 8.0;

/// How the rows are replaced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Publish {
    /// One `items-changed(0, old, new)`.
    OneStep,
    /// `items-changed(0, old, 0)`, then `items-changed(0, 0, new)`.
    RemoveThenAdd,
}

/// One publish of `rows` into a presented list whose first row is selected:
/// its time (the emissions plus the flush that lays it out) and the
/// `get_item`/`get_section` calls.
#[must_use]
pub fn time_publish(rows: u32, publish: Publish) -> (Duration, u64, u64) {
    let model = CountingSectionModel::new(64, SECTION_LEN);
    let (scroller, _list, selection) = sectioned_list(&model);
    let _shown = Presented::detached(&scroller);
    selection.set_selected(0);
    flush_events();
    model.reset_calls();
    let started = Instant::now();
    match publish {
        Publish::OneStep => model.publish(rows),
        Publish::RemoveThenAdd => model.publish_two_step(rows),
    }
    flush_events();
    let elapsed = started.elapsed();
    let (items, sections) = model.calls();
    (elapsed, items, sections)
}

/// Select a far row, scroll to it and back, with fresh item objects: whether
/// the selection still names the row and the realized rows still bind.
#[must_use]
pub fn fresh_items_keep_working(rows: u32) -> bool {
    let model = CountingSectionModel::new(rows, SECTION_LEN);
    let (scroller, list, selection) = sectioned_list(&model);
    let _shown = Presented::detached(&scroller);
    flush_events();
    let far = rows - 1;
    selection.set_selected(far);
    list.scroll_to(far, gtk4::ListScrollFlags::FOCUS, None);
    flush_events();
    list.scroll_to(0, gtk4::ListScrollFlags::NONE, None);
    flush_events();
    let selected = selection
        .selected_item()
        .and_downcast::<gtk4::StringObject>()
        .is_some_and(|item| item.string() == format!("row {far}"));
    let rows_bound = !children_named(&list, "row").is_empty();
    selection.selected() == far && selected && rows_bound
}

/// Probe A93. See the module documentation.
#[must_use]
pub fn probe_a93() -> Observation {
    Recorder::run(AxiomId::new(93), |recorder| {
        let [small, large] = SIZES;
        let (one_cost, one_items, _) = time_publish(large, Publish::OneStep);
        let (small_cost, small_items, small_sections) = time_publish(small, Publish::RemoveThenAdd);
        let (large_cost, large_items, large_sections) = time_publish(large, Publish::RemoveThenAdd);
        recorder.measure("one_step_large_us", one_cost.as_micros());
        recorder.measure("one_step_large_item_calls", one_items);
        recorder.measure("two_step_small_us", small_cost.as_micros());
        recorder.measure("two_step_large_us", large_cost.as_micros());
        recorder.measure("two_step_small_item_calls", small_items);
        recorder.measure("two_step_large_item_calls", large_items);
        recorder.measure("two_step_small_section_calls", small_sections);
        recorder.measure("two_step_large_section_calls", large_sections);
        let growth = large_cost.as_secs_f64() / small_cost.as_secs_f64().max(1e-9);
        recorder.measure("two_step_growth", format!("{growth:.2}"));
        recorder.control(small_items > 0, "control: the list asked for its rows")?;
        recorder.axiom(
            one_items >= u64::from(large),
            "A93: one items-changed replacing realized rows asks get_item for every added item",
        )?;
        recorder.axiom(
            small_items <= ITEM_CALL_CEILING && large_items <= ITEM_CALL_CEILING,
            "A93: remove-then-add asks get_item only around the realized rows",
        )?;
        recorder.axiom(
            growth < BOUNDED_GROWTH,
            "A93: a remove-then-add publish costs the same at 512 and 131,072 items",
        )?;
        let fresh = fresh_items_keep_working(large);
        recorder.measure("fresh_items_accepted", fresh);
        recorder.axiom(
            fresh,
            "A93: a fresh object per get_item keeps selection and scrolling working",
        )
    })
}
