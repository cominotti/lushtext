// SPDX-License-Identifier: MIT OR Apache-2.0

//! A46: the first `gtk_widget_realize` of a toplevel in a process costs a
//! one-time amount that later toplevels do not pay. Realizing creates the
//! `GdkSurface` and its renderer and connects the window to its frame clock;
//! the first time a process does it, GTK and GDK also initialise per-display
//! state, which is neither GL (`GDK_DISABLE=gl,vulkan` leaves it unchanged)
//! nor the window icon.
//!
//! The probe realizes two identical windows, one after the other, and
//! records both durations. Durations are recorded values, never the verdict
//! (the ledger's rule): the probe holds when both windows realize, and the
//! ledger records the measured magnitudes. LushText's window composition
//! (`first-frame-window-shell`) gives this cost a dispatch of its own
//! (stage R), so that nothing else shares it.

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::session::flush_events;
use crate::{AxiomId, Observation};

/// The size of both fixture windows.
const WINDOW_SIZE: (i32, i32) = (320, 200);

/// A plain window holding one label, unrealized.
#[must_use]
pub fn fixture_window(title: &str) -> libadwaita::Window {
    let (width, height) = WINDOW_SIZE;
    libadwaita::Window::builder()
        .default_width(width)
        .default_height(height)
        .title(title)
        .content(&gtk4::Label::new(Some(title)))
        .build()
}

/// Realize `window` and return how long `realize` took, in microseconds.
#[must_use]
pub fn timed_realize(window: &libadwaita::Window) -> i64 {
    let start = gtk4::glib::monotonic_time();
    WidgetExt::realize(window);
    gtk4::glib::monotonic_time() - start
}

/// Probe A46. Run it first in its process: the first realize is the
/// one-time one.
#[must_use]
pub fn probe_a46() -> Observation {
    Recorder::run(AxiomId::new(46), |recorder| {
        let first = fixture_window("A46 first toplevel");
        let first_us = timed_realize(&first);
        let second = fixture_window("A46 second toplevel");
        let second_us = timed_realize(&second);
        recorder.measure("first_realize_us", first_us);
        recorder.measure("second_realize_us", second_us);
        recorder.measure("one_time_us", first_us - second_us);
        recorder.control(
            first.is_realized() && second.is_realized(),
            "control: both toplevels realize",
        )?;
        let surfaces = first.surface().is_some() && second.surface().is_some();
        first.destroy();
        second.destroy();
        flush_events();
        recorder.axiom(
            surfaces,
            "A46: each realize creates the toplevel's surface (the cost is recorded, not judged)",
        )
    })
}
