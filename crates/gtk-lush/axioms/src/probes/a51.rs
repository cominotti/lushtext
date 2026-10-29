// SPDX-License-Identifier: MIT OR Apache-2.0

//! A51: realizing a toplevel **schedules a frame** on its frame clock. The
//! realize hands the window's pending resize to the clock, whose paint source
//! is a `GDK_PRIORITY_REDRAW` timeout (`gdkframeclockidle.c`,
//! `maybe_start_idle`), so it runs **ahead of `G_PRIORITY_DEFAULT_IDLE`**
//! sources. When the window is **not yet mapped**, that frame runs its layout,
//! emits `after-paint`, and is **never presented**; without presentation
//! timings its period falls back to `FRAME_INTERVAL` (16,667 µs), and
//! `min_next_frame_time` keeps the **next frame from starting earlier than one
//! `FRAME_INTERVAL` after it**. Realizing and presenting **in one dispatch**
//! runs no such frame: the first frame paints on the mapped window.
//!
//! LushText's window composition (`shorten-window-content-fill-in`, design D1)
//! realizes and presents in one dispatch for this reason: realizing in a
//! dispatch of its own ran an unseen frame that delayed the first visible one
//! by a full 60 Hz interval.
//!
//! The fixture is a plain window with a label: no tick callback, animation or
//! pending input, so the only frame request is the one the realize forwards.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

use crate::observation::{Recorder, Stop};
use crate::session::{REALIZE_BUDGET, flush_events, settle, wait_until};
use crate::{AxiomId, Observation};

/// GDK's fallback frame period without presentation timings
/// (`gdkframeclockidle.c`, `FRAME_INTERVAL`).
pub const FRAME_INTERVAL_US: i64 = 16_667;
/// The smallest gap the probe accepts as "one `FRAME_INTERVAL`": the clock
/// rounds its wait to whole milliseconds, which may cost up to 0.5 ms, and
/// the unseen frame's own work sits inside the interval.
pub const PACED_GAP_FLOOR_US: i64 = 16_000;
/// How long the probe keeps driving the window after its first mapped frame,
/// so any later unmapped frame or presentation feedback is recorded. A window
/// with nothing to animate may paint no further frame.
const AFTER_MAP_SETTLE: std::time::Duration = std::time::Duration::from_millis(150);

/// How a probe window is realized and presented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// Realize in one default-idle dispatch, present in a later one.
    Split,
    /// Realize and present in the same default-idle dispatch.
    Merged,
}

/// One `after-paint` of a probe window's frame clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AfterPaint {
    /// Whether the window was mapped when the frame painted.
    pub mapped: bool,
    /// The frame's counter.
    pub counter: i64,
    /// The frame's frame time, in microseconds.
    pub frame_time_us: i64,
}

/// A window composed in `order`, with every `after-paint` recorded.
pub struct Composed {
    window: libadwaita::Window,
    paints: Rc<RefCell<Vec<AfterPaint>>>,
}

impl Composed {
    /// Build a fixture window and queue its realize (and present) in
    /// `G_PRIORITY_DEFAULT_IDLE` sources, in `order`. The frames run as the
    /// caller drives the main context.
    #[must_use]
    pub fn queue(order: Order, title: &str) -> Self {
        // A46's fixture: a plain window holding one label.
        let window = super::a46::fixture_window(title);
        let paints = Rc::new(RefCell::new(Vec::new()));
        let target = window.clone();
        let record = Rc::clone(&paints);
        gtk4::glib::idle_add_local_full(gtk4::glib::Priority::DEFAULT_IDLE, move || {
            WidgetExt::realize(&target);
            if let Some(clock) = target.frame_clock() {
                let watched = target.downgrade();
                let record = Rc::clone(&record);
                clock.connect_after_paint(move |clock| {
                    let mapped = watched.upgrade().is_some_and(|window| window.is_mapped());
                    record.borrow_mut().push(AfterPaint {
                        mapped,
                        counter: clock.frame_counter(),
                        frame_time_us: clock.frame_time(),
                    });
                });
            }
            match order {
                Order::Merged => target.present(),
                Order::Split => {
                    let later = target.clone();
                    gtk4::glib::idle_add_local_full(
                        gtk4::glib::Priority::DEFAULT_IDLE,
                        move || {
                            later.present();
                            gtk4::glib::ControlFlow::Break
                        },
                    );
                }
            }
            gtk4::glib::ControlFlow::Break
        });
        Self { window, paints }
    }

    /// The `after-paint`s recorded so far.
    #[must_use]
    pub fn paints(&self) -> Vec<AfterPaint> {
        self.paints.borrow().clone()
    }

    /// Whether a frame has painted on the mapped window.
    #[must_use]
    pub fn painted_mapped(&self) -> bool {
        self.paints.borrow().iter().any(|paint| paint.mapped)
    }

    /// The presentation time GDK recorded for `counter`, `0` when it has
    /// none (or the clock no longer holds that frame's timings).
    #[must_use]
    pub fn presentation_us(&self, counter: i64) -> i64 {
        self.window
            .frame_clock()
            .and_then(|clock| clock.timings(counter))
            .map_or(0, |timings| timings.presentation_time())
    }

    /// The window.
    #[must_use]
    pub fn window(&self) -> &libadwaita::Window {
        &self.window
    }
}

impl Drop for Composed {
    fn drop(&mut self) {
        self.window.destroy();
        flush_events();
    }
}

/// Compose a window in `order` and drive it until its first mapped frame has
/// painted, then a little longer.
fn compose(recorder: &mut Recorder, order: Order, title: &str) -> Result<Composed, Stop> {
    let composed = Composed::queue(order, title);
    let painted = wait_until(REALIZE_BUDGET, || composed.painted_mapped());
    // Presentation feedback arrives after the frame.
    settle(AFTER_MAP_SETTLE);
    recorder.control(painted, "control: the window paints a mapped frame")?;
    Ok(composed)
}

/// A compact rendering of the recorded frames, for the observation.
fn describe(composed: &Composed) -> String {
    super::join_entries(composed.paints().iter().map(|paint| {
        format!(
            "#{}:{}@{}/p{}",
            paint.counter,
            if paint.mapped { "mapped" } else { "unmapped" },
            paint.frame_time_us,
            composed.presentation_us(paint.counter)
        )
    }))
}

/// Probe A51.
#[must_use]
pub fn probe_a51() -> Observation {
    Recorder::run(AxiomId::new(51), |recorder| {
        let split = compose(recorder, Order::Split, "A51 split")?;
        let paints = split.paints();
        recorder.measure("split_frames", describe(&split));
        let unmapped = paints.first().copied().filter(|paint| !paint.mapped);
        recorder.axiom(
            unmapped.is_some(),
            "A51: realize in its own dispatch runs a frame on the unmapped window before the present dispatch",
        )?;
        let unmapped = recorder.require(unmapped, "control: the unmapped frame was recorded")?;
        let first_mapped = recorder.require(
            paints.iter().copied().find(|paint| paint.mapped),
            "control: a mapped frame followed",
        )?;
        let gap = first_mapped.frame_time_us - unmapped.frame_time_us;
        recorder.measure("split_unmapped_to_first_mapped_us", gap);
        recorder.measure(
            "split_unmapped_presentation_us",
            split.presentation_us(unmapped.counter),
        );
        recorder.axiom(
            split.presentation_us(unmapped.counter) == 0,
            "A51: the frame on the unmapped window is never presented",
        )?;
        recorder.axiom(
            gap >= PACED_GAP_FLOOR_US,
            "A51: the first mapped frame starts no earlier than one FRAME_INTERVAL after the unmapped one",
        )?;
        drop(split);

        let merged = compose(recorder, Order::Merged, "A51 merged")?;
        let paints = merged.paints();
        recorder.measure("merged_frames", describe(&merged));
        recorder.axiom(
            paints.first().is_some_and(|paint| paint.mapped),
            "A51: realize and present in one dispatch: the first after-paint is on the mapped window",
        )?;
        recorder.axiom(
            paints.iter().all(|paint| paint.mapped),
            "A51: realize and present in one dispatch run no frame on the unmapped window",
        )
    })
}
