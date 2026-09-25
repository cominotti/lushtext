// SPDX-License-Identifier: MIT OR Apache-2.0

//! The probes' own small main-context driving: present a fixture in a plain
//! window, and wait a bounded time for GTK to reach a state.
//!
//! Spinning the default main context here is test tooling, as it is in
//! `gtk-lush-proof-harness`, not ownership of an application's control flow:
//! a probe runs to a bounded end on a thread that already initialized GTK and
//! then hands control back. The crate cannot depend on the harness (family
//! crates are leaves), so the few lines it needs live here.

use std::time::{Duration, Instant};

use gtk4::prelude::*;

use crate::fixtures::{FIXTURE_WINDOW_SIZE, REALIZE_BUDGET};

const POLL_INTERVAL: Duration = Duration::from_millis(20);
/// Settle time after the window first has a size, matching the old in-app
/// probes so their measured values stay comparable.
const PRESENT_SETTLE: Duration = Duration::from_millis(200);

/// Dispatch every main-context source that is ready now.
pub(crate) fn flush_events() {
    while gtk4::glib::MainContext::default().iteration(false) {}
}

/// Sleep for `delay`, then dispatch every ready source.
pub(crate) fn settle(delay: Duration) {
    std::thread::sleep(delay);
    flush_events();
}

/// Poll `predicate` until it holds or `budget` runs out, draining the main
/// context between polls. Returns whether it held.
pub(crate) fn wait_until(budget: Duration, mut predicate: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + budget;
    loop {
        if predicate() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(POLL_INTERVAL);
        flush_events();
    }
}

/// A fixture presented in its own window, closed when dropped.
pub(crate) struct Presented {
    window: libadwaita::Window,
    realized: bool,
}

impl Presented {
    /// Present `content` in a plain `AdwWindow` with no application.
    pub(crate) fn new(content: &impl IsA<gtk4::Widget>) -> Self {
        let (width, height) = FIXTURE_WINDOW_SIZE;
        let window = libadwaita::Window::builder()
            .default_width(width)
            .default_height(height)
            .content(content)
            .build();
        window.present();
        let realized = wait_until(REALIZE_BUDGET, || window.width() > 0 && window.height() > 0);
        settle(POLL_INTERVAL);
        settle(PRESENT_SETTLE);
        Self { window, realized }
    }

    /// Whether the window received a size within the realization budget.
    pub(crate) const fn realized(&self) -> bool {
        self.realized
    }
}

impl Drop for Presented {
    fn drop(&mut self) {
        self.window.destroy();
        flush_events();
    }
}
