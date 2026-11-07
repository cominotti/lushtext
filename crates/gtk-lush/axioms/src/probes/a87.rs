// SPDX-License-Identifier: MIT OR Apache-2.0

//! A87: on a **presented, visible** `GtkListView`, every `items-changed` of
//! its model costs a fixed amount that does not grow with the item count
//! (about 30 µs on GTK 4.22.5), so 256 single appends cost several times one
//! `splice` of the same 256 items; while the list is **not realized**
//! (unparented, or under an invisible `GtkScrolledWindow` in a presented
//! window) an append costs less, and that cost does not grow with the item
//! count either.
//!
//! This refutes the hypothesis `bound-lists-by-budget-not-count` started from
//! (design D4: "an unrealized list walks its whole item list on every
//! `items-changed`"): the cost of a turn of per-match appends is the
//! per-emission cost of the presented list, paid once per match.
//!
//! The probe times 256 single appends and one 256-item splice at two store
//! sizes, in the three states. Each figure is the fastest of three
//! repetitions, so a slow scheduler only ever inflates a ratio.
//!
//! LushText's workspace search panel (`ui/search_panel/execution.rs`,
//! `bound-lists-by-budget-not-count` D4) streams matches into a results list
//! that is invisible until the first match arrives; it applies each turn's
//! matches as one splice per touched store, so the per-emission cost an
//! unrealized list pays is paid once per turn, not once per match.

use std::time::{Duration, Instant};

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::session::{Presented, flush_events};
use crate::{AxiomId, Observation};

/// The two store sizes compared.
pub const SIZES: [u32; 2] = [1_024, 16 * 1024];
/// Single appends timed at each size.
pub const APPENDS: u32 = 256;
/// Repetitions; the fastest is kept.
const REPETITIONS: u32 = 3;
/// The largest growth, large size over small, that still reads as a cost not
/// linear in the item count: half the sixteen-fold size ratio the probes use,
/// so a linear cost lands far above it and a constant or logarithmic one far
/// below.
pub const BOUNDED_GROWTH: f64 = 8.0;
/// How many times one splice must undercut the same items appended singly on
/// a presented list for the per-emission cost to dominate.
pub const SPLICE_ADVANTAGE: f64 = 4.0;

/// Where the list sits while it takes the appends.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListState {
    /// In no window at all.
    Unparented,
    /// In a presented window, under an invisible scrolled window.
    HiddenInPresentedWindow,
    /// Presented and visible.
    Presented,
}

/// A `GtkListView` of labels over a `GListStore` of `StringObject`s.
#[must_use]
pub fn label_list(model: &impl IsA<gtk4::gio::ListModel>) -> gtk4::ListView {
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
            .and_then(|object| {
                object
                    .downcast_ref::<gtk4::TreeListRow>()
                    .and_then(gtk4::TreeListRow::item)
                    .or(Some(object))
            })
            .and_downcast::<gtk4::StringObject>()
            .map(|object| object.string().to_string())
            .unwrap_or_default();
        if let Some(label) = item.child().and_downcast::<gtk4::Label>() {
            label.set_text(&text);
        }
    });
    gtk4::ListView::new(
        Some(gtk4::NoSelection::new(Some(model.clone().upcast()))),
        Some(factory),
    )
}

/// A store of `rows` numbered strings.
#[must_use]
pub fn numbered_store(rows: u32) -> gtk4::gio::ListStore {
    let store = gtk4::gio::ListStore::new::<gtk4::StringObject>();
    let items: Vec<gtk4::StringObject> = (0..rows)
        .map(|index| gtk4::StringObject::new(&format!("row {index}")))
        .collect();
    store.splice(0, 0, &items);
    store
}

/// The fastest of three timings of [`APPENDS`] single appends to a store of
/// `rows`, with its list in `state`.
#[must_use]
pub fn time_single_appends(rows: u32, state: ListState) -> Duration {
    time_appends(rows, state, false)
}

/// The fastest of three timings of one splice of [`APPENDS`] items into a
/// store of `rows`, with its list in `state`.
#[must_use]
pub fn time_one_splice(rows: u32, state: ListState) -> Duration {
    time_appends(rows, state, true)
}

fn time_appends(rows: u32, state: ListState, as_one_splice: bool) -> Duration {
    let mut fastest = Duration::MAX;
    for _ in 0..REPETITIONS {
        let store = numbered_store(rows);
        let list = label_list(&store);
        let scroller = gtk4::ScrolledWindow::builder()
            .child(&list)
            .vexpand(true)
            .build();
        let _shown = match state {
            ListState::Unparented => None,
            ListState::HiddenInPresentedWindow => {
                scroller.set_visible(false);
                Some(Presented::detached(&scroller))
            }
            ListState::Presented => Some(Presented::detached(&scroller)),
        };
        flush_events();
        let items: Vec<gtk4::StringObject> = (0..APPENDS)
            .map(|index| gtk4::StringObject::new(&format!("new {index}")))
            .collect();
        let started = Instant::now();
        if as_one_splice {
            store.splice(rows, 0, &items);
        } else {
            for item in &items {
                store.append(item);
            }
        }
        fastest = fastest.min(started.elapsed());
        flush_events();
    }
    fastest
}

/// Large-size over small-size cost, as a float ratio.
fn growth(small: Duration, large: Duration) -> f64 {
    large.as_secs_f64() / small.as_secs_f64().max(1e-9)
}

/// Probe A87. See the module documentation.
#[must_use]
pub fn probe_a87() -> Observation {
    Recorder::run(AxiomId::new(87), |recorder| {
        let [small, large] = SIZES;
        let mut ratios = Vec::new();
        for (state, key_small, key_large) in [
            (
                ListState::Unparented,
                "unparented_small_us",
                "unparented_large_us",
            ),
            (
                ListState::HiddenInPresentedWindow,
                "hidden_small_us",
                "hidden_large_us",
            ),
            (
                ListState::Presented,
                "presented_small_us",
                "presented_large_us",
            ),
        ] {
            let small_cost = time_single_appends(small, state);
            let large_cost = time_single_appends(large, state);
            recorder.measure(key_small, small_cost.as_micros());
            recorder.measure(key_large, large_cost.as_micros());
            ratios.push(growth(small_cost, large_cost));
        }
        let presented_splice = time_one_splice(large, ListState::Presented);
        recorder.measure("presented_large_splice_us", presented_splice.as_micros());
        let advantage = time_single_appends(large, ListState::Presented).as_secs_f64()
            / presented_splice.as_secs_f64().max(1e-9);
        recorder.measure("unparented_growth", format!("{:.2}", ratios[0]));
        recorder.measure("hidden_growth", format!("{:.2}", ratios[1]));
        recorder.measure("presented_growth", format!("{:.2}", ratios[2]));
        recorder.measure("presented_splice_advantage", format!("{advantage:.2}"));
        recorder.axiom(
            ratios[0] < BOUNDED_GROWTH && ratios[1] < BOUNDED_GROWTH,
            "A87: an unrealized list view does not pay per item on an append",
        )?;
        recorder.axiom(
            ratios[2] < BOUNDED_GROWTH,
            "A87: a presented list view's per-append cost does not grow with its items",
        )?;
        recorder.axiom(
            advantage > SPLICE_ADVANTAGE,
            "A87: on a presented list, one splice costs a fraction of the same appends singly",
        )
    })
}
