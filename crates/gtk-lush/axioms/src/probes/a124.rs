// SPDX-License-Identifier: MIT OR Apache-2.0

//! A124: on a **mapped**, presented `GtkListView` over a `GtkTreeListModel`,
//! a removal of bound rows run from a one-shot `G_PRIORITY_HIGH_IDLE` source,
//! **outside any frame clock dispatch**, leaves every row widget it created
//! for the rows entering the band **mapped** (so with accessible `hidden`
//! false: `gtk_widget_map` writes it, `gtkwidget.c`) when the splice returns,
//! and the next frame runs **no factory `setup` or `bind`** for them: it only
//! styles, measures, allocates and paints them.
//!
//! A118 already records that such a removal creates and binds the entering
//! rows inside its `items-changed` and never reuses a removed widget for
//! another item; this probe adds what running the removal between frames
//! relies on. The probe expands a directory row over 256 entries with 400
//! sibling rows after it, and removes the first 32 children from the idle.
//! It cannot read the `hidden` state's value (the crate forbids `unsafe`, and
//! `gtk_test_accessible_check_state` is variadic), so it records that the
//! state is present and that each entering widget and its row widget are
//! mapped, which is when GTK writes it false.
//!
//! LushText's sliced sidebar collapse (`slice-large-directory-collapse` D25)
//! runs each bound unit in a dispatch of its own between frames, so the frame
//! after it lays out that unit's rows and runs no step.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::glib;
use gtk4::prelude::*;

use super::a118::{AFTER_ROWS, SMALL_CHILDREN, TreeFixture, tree_fixture};
use crate::observation::Recorder;
use crate::session::{flush_events, wait_until};
use crate::{AxiomId, Observation};

/// The bound rows the idle removes: one LushText collapse unit
/// (`SIDEBAR_REALIZED_ROWS_PER_FRAME`).
pub const REMOVED_ROWS: u32 = 32;
/// How far the counts may stray from the rows removed (the band's edges vary
/// by a few rows between runs).
pub const COUNT_SLACK: u32 = 8;

/// What the idle removal did.
#[derive(Clone, Debug, Default)]
pub struct IdleRemoval {
    /// Whether the idle ran inside a frame clock dispatch (between its
    /// `flush-events` and `resume-events`).
    pub ran_inside_frame: bool,
    /// Bound children removed.
    pub bound_removed: u32,
    /// Factory `setup` and `bind` emissions inside the splice.
    pub in_call: (u32, u32),
    /// List items bound when the splice returned that were not bound before
    /// it: the rows that entered the band.
    pub entering: u32,
    /// Of those, the ones whose row widget (the list item child) and its
    /// parent, the list's row widget, were both mapped when it returned.
    pub entering_mapped: u32,
    /// Of those, the ones whose list row widget has the accessible `hidden`
    /// state present.
    pub entering_hidden_present: u32,
    /// The splice's time, microseconds.
    pub call_us: u64,
    /// Factory `setup` and `bind` emissions from the splice's return through
    /// the `after-paint` of the next frame.
    pub next_frame_counts: (u32, u32),
    /// The next frame's time from `before-paint` to `after-paint`, and to the
    /// end of its layout phase, microseconds.
    pub next_frame: Option<(u64, u64)>,
}

/// The next frame after the splice: the factory's `(setup, bind)` emissions
/// up to its `after-paint`, and its `(total, layout)` time in microseconds.
type NextFrame = ((u32, u32), (u64, u64));

fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

/// The list items bound now, as `(item, list item)`.
fn bound_items(fixture: &TreeFixture) -> Vec<(glib::Object, gtk4::ListItem)> {
    fixture
        .bound()
        .into_iter()
        .filter_map(|(_, item)| {
            item.item()
                .and_downcast::<gtk4::TreeListRow>()
                .and_then(|row| row.item())
                .map(|object| (object, item))
        })
        .collect()
}

/// Present the fixture, then remove the first [`REMOVED_ROWS`] children from
/// a one-shot `G_PRIORITY_HIGH_IDLE` source, and dispatch the next frame.
///
/// # Panics
///
/// Never in practice: the idle's result cell is filled before it is read.
#[must_use]
pub fn idle_removal(fixture: &TreeFixture) -> Option<IdleRemoval> {
    let clock = fixture.list.frame_clock()?;
    let in_frame = Rc::new(Cell::new(false));
    let handlers = vec![
        clock.connect_flush_events({
            let in_frame = Rc::clone(&in_frame);
            move |_| in_frame.set(true)
        }),
        clock.connect_resume_events({
            let in_frame = Rc::clone(&in_frame);
            move |_| in_frame.set(false)
        }),
    ];
    let result: Rc<RefCell<Option<IdleRemoval>>> = Rc::default();
    // The first frame after the idle's splice: its phases and the factory
    // emissions from the splice's return to its after-paint.
    let idle_done = Rc::new(Cell::new(false));
    let frame_start: Rc<Cell<Option<Instant>>> = Rc::default();
    let layout_end: Rc<Cell<Option<Instant>>> = Rc::default();
    let next_frame_seen: Rc<Cell<Option<NextFrame>>> = Rc::default();
    let frame_handlers = vec![
        clock.connect_before_paint({
            let idle_done = Rc::clone(&idle_done);
            let frame_start = Rc::clone(&frame_start);
            move |_| {
                if idle_done.get() && frame_start.get().is_none() {
                    frame_start.set(Some(Instant::now()));
                }
            }
        }),
        clock.connect_layout({
            let frame_start = Rc::clone(&frame_start);
            let layout_end = Rc::clone(&layout_end);
            move |_| {
                if frame_start.get().is_some() && layout_end.get().is_none() {
                    layout_end.set(Some(Instant::now()));
                }
            }
        }),
        clock.connect_after_paint({
            let frame_start = Rc::clone(&frame_start);
            let layout_end = Rc::clone(&layout_end);
            let next_frame_seen = Rc::clone(&next_frame_seen);
            let counters = Rc::clone(&fixture.counters);
            move |_| {
                if let Some(start) = frame_start.get()
                    && next_frame_seen.get().is_none()
                {
                    let layout = layout_end.get().unwrap_or(start);
                    next_frame_seen.set(Some((
                        (counters.setups.get(), counters.binds.get()),
                        (
                            micros(start.elapsed()),
                            micros(layout.saturating_duration_since(start)),
                        ),
                    )));
                }
            }
        }),
    ];
    let before: Vec<glib::Object> = bound_items(fixture)
        .into_iter()
        .map(|(object, _)| object)
        .collect();
    let removed = u32::try_from(
        fixture
            .bound()
            .iter()
            .filter(|(position, _)| (1..=REMOVED_ROWS).contains(position))
            .count(),
    )
    .unwrap_or(u32::MAX);
    {
        let fixture = fixture.clone();
        let in_frame = Rc::clone(&in_frame);
        let result = Rc::clone(&result);
        let idle_done = Rc::clone(&idle_done);
        glib::idle_add_local_full(glib::Priority::HIGH_IDLE, move || {
            let ran_inside_frame = in_frame.get();
            fixture.counters.reset();
            fixture.counters.binds.set(0);
            let started = Instant::now();
            fixture
                .children
                .splice(0, REMOVED_ROWS, &[] as &[glib::Object]);
            let call_us = micros(started.elapsed());
            let in_call = (fixture.counters.setups.get(), fixture.counters.binds.get());
            let mut entering = 0;
            let mut entering_mapped = 0;
            let mut entering_hidden_present = 0;
            for (object, item) in bound_items(&fixture) {
                if before.contains(&object) {
                    continue;
                }
                entering += 1;
                let child = item.child();
                let row_widget = child.as_ref().and_then(WidgetExt::parent);
                if child.as_ref().is_some_and(WidgetExt::is_mapped)
                    && row_widget.as_ref().is_some_and(WidgetExt::is_mapped)
                {
                    entering_mapped += 1;
                }
                if row_widget.as_ref().is_some_and(|widget| {
                    gtk4::test_accessible_has_state(widget, gtk4::AccessibleState::Hidden)
                }) {
                    entering_hidden_present += 1;
                }
            }
            fixture.counters.reset();
            fixture.counters.binds.set(0);
            result.replace(Some(IdleRemoval {
                ran_inside_frame,
                bound_removed: removed,
                in_call,
                entering,
                entering_mapped,
                entering_hidden_present,
                call_us,
                ..IdleRemoval::default()
            }));
            idle_done.set(true);
            glib::ControlFlow::Break
        });
    }
    let ran = wait_until(Duration::from_secs(2), || {
        result.borrow().is_some() && next_frame_seen.get().is_some()
    });
    for handler in frame_handlers {
        clock.disconnect(handler);
    }
    for handler in handlers {
        clock.disconnect(handler);
    }
    let (next_frame_counts, next_frame) = next_frame_seen.get().unzip();
    let next_frame_counts = next_frame_counts.unwrap_or_default();
    flush_events();
    if !ran {
        return None;
    }
    let mut removal = result.take()?;
    removal.next_frame_counts = next_frame_counts;
    removal.next_frame = next_frame;
    Some(removal)
}

/// Probe A124. See the module documentation.
#[must_use]
pub fn probe_a124() -> Observation {
    Recorder::run(AxiomId::new(124), |recorder| {
        let fixture = tree_fixture(SMALL_CHILDREN, AFTER_ROWS);
        let shown = fixture.present();
        recorder.control(shown.realized(), "control: the window received a size")?;
        recorder.control(fixture.list.is_mapped(), "control: the list is mapped")?;
        let removal = recorder.require(
            idle_removal(&fixture),
            "control: the high-priority idle ran and the list has a frame clock",
        )?;
        recorder.measure("ran_inside_frame", removal.ran_inside_frame);
        recorder.measure("bound_removed", removal.bound_removed);
        recorder.measure("in_call(setups,binds)", format!("{:?}", removal.in_call));
        recorder.measure("entering", removal.entering);
        recorder.measure("entering_mapped", removal.entering_mapped);
        recorder.measure("entering_hidden_present", removal.entering_hidden_present);
        recorder.measure("call_us", removal.call_us);
        recorder.measure(
            "next_frame_counts(setups,binds)",
            format!("{:?}", removal.next_frame_counts),
        );
        recorder.measure(
            "next_frame(total_us,layout_us)",
            format!("{:?}", removal.next_frame),
        );
        recorder.control(
            !removal.ran_inside_frame,
            "control: the removal ran outside any frame clock dispatch",
        )?;
        recorder.control(
            removal.bound_removed + COUNT_SLACK >= REMOVED_ROWS,
            "control: the removed head rows were bound",
        )?;
        recorder.control(
            removal.next_frame.is_some(),
            "control: a frame painted after the removal",
        )?;
        recorder.axiom(
            removal.entering + COUNT_SLACK >= removal.bound_removed
                && removal.in_call.0 + COUNT_SLACK >= removal.entering
                && removal.in_call.1 + COUNT_SLACK >= removal.entering,
            "A124: the removal from the idle creates and binds the entering rows' widgets inside \
             the splice (A118's creation, outside a frame)",
        )?;
        recorder.axiom(
            removal.entering > 0 && removal.entering_mapped == removal.entering,
            "A124: every entering row widget and its list row widget are mapped when the splice \
             returns (so accessible hidden is false, written by gtk_widget_map)",
        )?;
        recorder.axiom(
            removal.next_frame_counts == (0, 0),
            "A124: the next frame runs no factory setup or bind for the entering rows",
        )?;
        drop(shown);
        drop(fixture);
        flush_events();
        Ok(())
    })
}
