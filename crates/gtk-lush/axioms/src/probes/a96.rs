// SPDX-License-Identifier: MIT OR Apache-2.0

//! A96: a `GtkListView` over a lazy `GListModel`, inside a `GtkScrolledWindow`
//! with `propagate-natural-height` and `max-content-height` (the shape of
//! LushText's command palette lists), measures and allocates in time flat in
//! the item count: presenting it, and publishing a new length by
//! remove-then-add (A93), ask `get_item` only around the realized rows (A1),
//! at 512 items as at 131,072. The natural-height measure therefore does not
//! walk the items: the list's height estimate stands in for the rows it has
//! not realized.
//!
//! LushText's command palette category view (`drill-down-palette-categories`
//! D2) keeps both scrollers' `max-content-height: 400` and
//! `propagate-natural-height` over its lazy `PaletteCategoryModel` on this
//! axiom.

use std::time::{Duration, Instant};

use gtk4::prelude::*;

use super::lazy_sections::CountingSectionModel;
use crate::observation::Recorder;
use crate::session::{Presented, flush_events};
use crate::{AxiomId, Observation};

/// The item counts compared (256 times the items).
pub const SIZES: [u32; 2] = [512, 128 * 1024];
/// The scroller's `max-content-height`, as the palette's.
pub const MAX_CONTENT_HEIGHT: i32 = 400;
/// The most `get_item` calls presenting or publishing may make at either
/// size: about four times A1's realized-row cap.
pub const ITEM_CALL_CEILING: u64 = 1_024;
/// The largest growth of the publish time, large over small, that still
/// reads as flat.
pub const BOUNDED_GROWTH: f64 = 8.0;

/// A flat list (no header factory) over `model` in a scroller with
/// `propagate-natural-height` and `max-content-height`, in a vertical box.
#[must_use]
pub fn natural_height_list(model: &CountingSectionModel) -> (gtk4::Box, gtk4::ListView) {
    let selection = gtk4::SingleSelection::new(Some(model.clone()));
    selection.set_autoselect(false);
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        if let Some(item) = item.downcast_ref::<gtk4::ListItem>() {
            item.set_child(Some(&gtk4::Label::new(None)));
        }
    });
    factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let text = item
            .item()
            .and_downcast::<gtk4::StringObject>()
            .map(|object| object.string().to_string())
            .unwrap_or_default();
        if let Some(label) = item.child().and_downcast::<gtk4::Label>() {
            label.set_text(&text);
        }
    });
    let list = gtk4::ListView::new(Some(selection), Some(factory));
    let scroller = gtk4::ScrolledWindow::builder()
        .child(&list)
        .propagate_natural_height(true)
        .max_content_height(MAX_CONTENT_HEIGHT)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    column.append(&gtk4::Label::new(Some("query row")));
    column.append(&scroller);
    (column, list)
}

/// One size's figures.
#[derive(Clone, Copy, Debug)]
pub struct NaturalHeightRun {
    /// `get_item` calls presenting the list made.
    pub present_items: u64,
    /// Time and `get_item` calls of one remove-then-add publish of the same
    /// length, measured through the following layout.
    pub publish: Duration,
    /// `get_item` calls the publish made.
    pub publish_items: u64,
    /// The scroller's allocated height, bounded by `max-content-height`.
    pub scroller_height: i32,
}

/// Present the list over `items`, then publish `items` again.
#[must_use]
pub fn run(items: u32) -> NaturalHeightRun {
    let model = CountingSectionModel::new(items, items.max(1));
    let (column, list) = natural_height_list(&model);
    let _shown = Presented::detached(&column);
    flush_events();
    let present_items = model.calls().0;
    model.reset_calls();
    let started = Instant::now();
    model.publish_two_step(items);
    flush_events();
    let publish = started.elapsed();
    let scroller_height = list.parent().map_or(0, |scroller| scroller.height());
    NaturalHeightRun {
        present_items,
        publish,
        publish_items: model.calls().0,
        scroller_height,
    }
}

/// Probe A96. See the module documentation.
#[must_use]
pub fn probe_a96() -> Observation {
    Recorder::run(AxiomId::new(96), |recorder| {
        let [small, large] = SIZES;
        let near = run(small);
        let far = run(large);
        recorder.measure("small_present_item_calls", near.present_items);
        recorder.measure("large_present_item_calls", far.present_items);
        recorder.measure("small_publish_us", near.publish.as_micros());
        recorder.measure("large_publish_us", far.publish.as_micros());
        recorder.measure("small_publish_item_calls", near.publish_items);
        recorder.measure("large_publish_item_calls", far.publish_items);
        recorder.measure("large_scroller_height", far.scroller_height);
        let growth = far.publish.as_secs_f64() / near.publish.as_secs_f64().max(1e-9);
        recorder.measure("publish_growth", format!("{growth:.2}"));
        recorder.control(
            near.present_items > 0 && far.scroller_height > 0,
            "control: the list realized rows and the scroller has a height",
        )?;
        recorder.control(
            far.scroller_height <= MAX_CONTENT_HEIGHT,
            "control: max-content-height bounds the natural height",
        )?;
        recorder.axiom(
            [
                near.present_items,
                far.present_items,
                near.publish_items,
                far.publish_items,
            ]
            .iter()
            .all(|calls| *calls <= ITEM_CALL_CEILING)
                && growth < BOUNDED_GROWTH,
            "A96: a natural-height lazy list measures, allocates and publishes around its \
             realized rows, flat in the item count",
        )
    })
}
