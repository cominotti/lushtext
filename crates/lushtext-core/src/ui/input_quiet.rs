// SPDX-License-Identifier: GPL-3.0-or-later

//! When the user last pressed a key or button, scrolled, or touched a window
//! (`bound-post-open-settle`, design D2, review R6).
//!
//! **Cross-cutting, no role.** Its one consumer is the deferred language
//! attach of `WFR-DOCUMENT-LOAD`, which waits for an input-quiet interval
//! before running the language-definition parse that cannot leave the GTK
//! thread (ledger A52). Only discrete user input counts: key presses, button
//! presses, scroll events and touch begins. Pointer motion, crossing events,
//! releases and touch updates do not, because they can arrive every frame for
//! as long as the pointer moves.
//!
//! Each window installs one capture-phase legacy controller ([`install`]);
//! the time is process-wide, because a parse blocks every window.

use std::cell::Cell;

use gtk4::prelude::*;
use gtk4::{gdk, glib};

thread_local! {
    static LAST_INPUT_US: Cell<Option<i64>> = const { Cell::new(None) };
}

/// Whether an event of `event_type` counts as user input for input-quiet.
#[must_use]
pub fn counts_as_input(event_type: gdk::EventType) -> bool {
    matches!(
        event_type,
        gdk::EventType::KeyPress
            | gdk::EventType::ButtonPress
            | gdk::EventType::Scroll
            | gdk::EventType::TouchBegin
    )
}

/// Record user input on `window` from its capture phase, before any child
/// handles it.
pub fn install(window: &(impl IsA<gtk4::Window> + IsA<gtk4::Widget>)) {
    let controller = gtk4::EventControllerLegacy::new();
    controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    controller.connect_event(|_, event| {
        if counts_as_input(event.event_type()) {
            LAST_INPUT_US.with(|last| last.set(Some(glib::monotonic_time())));
        }
        glib::Propagation::Proceed
    });
    window.add_controller(controller);
}

/// The monotonic time of the last counted input on any window, or `None`.
#[must_use]
pub fn last_input_us() -> Option<i64> {
    LAST_INPUT_US.with(Cell::get)
}

/// Record input now, as a counted event would (widget tests drive the attach
/// deadline with it, since synthetic key events bypass the capture phase).
#[cfg(feature = "test-utils")]
pub fn simulate_user_input() {
    LAST_INPUT_US.with(|last| last.set(Some(glib::monotonic_time())));
}
