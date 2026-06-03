// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared test infrastructure for widget tests.
//!
//! GTK4 must be initialized before constructing any widget, and GResources
//! must be registered before constructing widgets that use composite templates.
//! Both operations are one-time setup via `std::sync::Once`.

use gio::prelude::{ApplicationExt, Cast, ListModelExt, ObjectExt};
use glib::prelude::IsA;
use glib::prelude::ToValue;
use gtk4::prelude::{GtkWindowExt, WidgetExt};
use lushtext_core::config::APP_ID;
pub use lushtext_core::services::filesystem::{
    fixture, metadata as fs_metadata, mutate as fs_mutate, read as fs_read,
};
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Once;

static GTK_INIT: Once = Once::new();

/// Initialize GTK4, register GResources, and set up GSettings for testing.
/// Safe to call multiple times; the actual work only runs once.
///
/// Uses the in-memory GSettings backend so tests don't pollute user's dconf.
/// Sets `LUSHTEXT_DATA_DIR` to a temp directory so session/draft I/O doesn't
/// touch the user's real data.
/// Requires the private headless compositor owned by the widget harness.
pub fn ensure_gtk_init() {
    GTK_INIT.call_once(|| {
        // Widget tests run in one isolated process before GTK startup, so they
        // can safely pin a memory-only GSettings backend for deterministic runs.
        // SAFETY: widget tests set these process environment variables before
        // GTK startup and before any background worker threads are spawned.
        unsafe { std::env::set_var("GSETTINGS_BACKEND", "memory") };
        // Isolate session/draft I/O from the user's real data directory.
        // PID-based naming prevents nextest's parallel test processes from
        // interfering with each other via shared session files.
        let test_data_dir =
            std::env::temp_dir().join(format!("lushtext-test-{}", std::process::id()));
        let _ = fs_mutate::remove_dir_all_if_exists(&test_data_dir);
        let _ = fs_mutate::create_dir_all(&test_data_dir);
        // SAFETY: widget tests set these process environment variables before
        // GTK startup and before any background worker threads are spawned.
        unsafe { std::env::set_var("LUSHTEXT_DATA_DIR", &test_data_dir) };
        lushtext_core::init_schema_dir();
        gtk4::init()
            .expect("GTK4 init failed — is a display server available? Try mutter --headless.");
        // Initialize libadwaita so widget templates can instantiate Adw widgets
        // (e.g. AdwWrapBox in the inline alert) in tests that construct widgets
        // directly without going through AdwApplication startup. adw_init is
        // idempotent, so later AdwApplication startups remain safe.
        libadwaita::init().expect("libadwaita init failed");
        sourceview5::init();
        lushtext_core::register_resources();
    });
}

pub fn test_application() -> libadwaita::Application {
    ensure_gtk_init();
    static APP_COUNTER: AtomicUsize = AtomicUsize::new(0);
    let app_id = format!(
        "{APP_ID}.widget-test-{}-{}",
        std::process::id(),
        APP_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let app: libadwaita::Application = lushtext_core::app::LushtextApplication::new_with_application_id(&app_id).upcast();
    app.register(gio::Cancellable::NONE)
        .expect("test application registration");
    app.emit_by_name::<()>("startup", &[]);
    while glib::MainContext::default().iteration(false) {}
    app
}

pub fn test_window() -> lushtext_core::ui::window::LushtextWindow {
    let app = test_application();
    lushtext_core::ui::window::LushtextWindow::new(&app)
}

/// Run the GTK main loop until no immediate events remain.
pub fn flush_events() {
    while glib::MainContext::default().iteration(false) {}
}

/// Sleep briefly, then flush pending GTK work.
pub fn flush_after_delay(delay: Duration) {
    std::thread::sleep(delay);
    flush_events();
}

/// Interval between `wait_until` predicate checks.
const WAIT_UNTIL_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Poll until the predicate becomes true or the timeout expires.
///
/// Each poll sleeps briefly and then **drains every ready main-loop source** via
/// `flush_events()` (`while iteration(false) {}`). Draining to exhaustion is the
/// important part: `spawn_blocking_then` delivers its completion through
/// `glib::idle_add_once`, a *low-priority idle source*. A loop that only blocks
/// on `MainContext::iteration(true)` with a higher-priority timeout source can
/// starve that idle indefinitely, so the async result never lands and the wait
/// times out even though the work finished. Drain-all dispatches the idle as
/// soon as nothing higher-priority is pending, which is exactly how these tests
/// observe background completion. Do not "optimize" this into a single blocking
/// iteration — that regresses every `spawn_blocking_then`-backed wait.
///
/// The flake this guards against is a *budget* problem, not a polling-gap one:
/// give async/realization waits a generous timeout (see callers) rather than
/// changing the poll mechanism.
pub fn wait_until(timeout: Duration, mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if predicate() {
            return;
        }
        std::thread::sleep(WAIT_UNTIL_POLL_INTERVAL);
        flush_events();
    }
    panic!("condition was not met within {timeout:?}");
}

/// Present a window and flush the initial realization work.
pub fn present_window(window: &impl IsA<gtk4::Window>) {
    window.present();
    flush_events();
}

fn try_emit_key_pressed(widget: &gtk4::Widget, key: gtk4::gdk::Key) -> Option<glib::Propagation> {
    let controllers = widget.observe_controllers();
    for index in 0..controllers.n_items() {
        if let Some(controller) = controllers
            .item(index)
            .and_then(|object| object.downcast::<gtk4::EventControllerKey>().ok())
        {
            let args: [&dyn ToValue; 3] = [
                &key,
                &0u32,
                &gtk4::gdk::ModifierType::empty(),
            ];
            let stopped: bool =
                glib::object::ObjectExt::emit_by_name(&controller, "key-pressed", &args);
            return Some(if stopped {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            });
        }
    }
    None
}

/// Emit a synthetic key press on the window's currently focused widget.
pub fn emit_key_pressed_on_focus(
    window: &impl IsA<gtk4::Window>,
    key: gtk4::gdk::Key,
) -> glib::Propagation {
    let focus = window
        .as_ref()
        .focus()
        .expect("window should have a focused widget");
    let mut current = Some(focus);
    while let Some(widget) = current {
        if let Some(result) = try_emit_key_pressed(&widget, key) {
            return result;
        }
        current = widget.parent();
    }
    panic!("focused widget ancestry had no EventControllerKey");
}
