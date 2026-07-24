// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared test infrastructure for widget tests.
//!
//! GTK4 must be initialized before constructing any widget, and GResources
//! must be registered before constructing widgets that use composite templates.
//! Both operations are one-time setup via `std::sync::Once`.

use gio::prelude::{ApplicationExt, Cast, ListModelExt, ObjectExt};
use glib::prelude::IsA;
use glib::prelude::ToValue;
pub use gtk_lush_proof_harness::{flush_after_delay, flush_events, wait_until};
use gtk4::prelude::{GtkWindowExt, WidgetExt};
use lushtext_core::config::APP_ID;
pub use lushtext_core::services::filesystem::{
    fixture, metadata as fs_metadata, mutate as fs_mutate, read as fs_read,
};
use std::ffi::OsString;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Once;
use std::time::Duration;

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

/// Present a test window and wait for the headless compositor to realize it.
///
/// This is the single shared presentation helper for the widget-test tree.
/// Window realization is a precondition, not the behavior under test, so it
/// gives the compositor a generous async-scale budget for the surface
/// `configure` that yields a non-zero allocation; `wait_until` returns the
/// instant the size is real, so the larger ceiling only costs time on a slow,
/// loaded compositor. A short post-realization settle drains main-loop work
/// scheduled during allocation before the caller interacts with the window.
pub fn present_window(window: &(impl IsA<gtk4::Window> + IsA<gtk4::Widget>)) {
    window.present();
    wait_until(Duration::from_secs(5), || {
        window.width() > 0 && window.height() > 0
    });
    flush_after_delay(Duration::from_millis(20));
}

/// Temporarily point app-data I/O at a fresh directory for one widget test.
///
/// `json_store::data_dir()` reads `LUSHTEXT_DATA_DIR` dynamically, so tests
/// that need deterministic startup or Preferences data scans can isolate only
/// their own metadata without depending on earlier widget-test residue.
pub struct IsolatedDataDir {
    tempdir: tempfile::TempDir,
    previous: Option<OsString>,
}

impl IsolatedDataDir {
    #[must_use]
    pub fn path(&self) -> &Path {
        self.tempdir.path()
    }
}

impl Drop for IsolatedDataDir {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            // SAFETY: widget tests are driven on the single GTK harness thread;
            // this guard restores the process env after windows from the test
            // have been dropped.
            unsafe { std::env::set_var("LUSHTEXT_DATA_DIR", previous) };
        } else {
            // SAFETY: see the restoration case above.
            unsafe { std::env::remove_var("LUSHTEXT_DATA_DIR") };
        }
    }
}

pub fn isolated_data_dir() -> IsolatedDataDir {
    ensure_gtk_init();
    let tempdir = tempfile::tempdir().expect("isolated app data tempdir");
    let previous = std::env::var_os("LUSHTEXT_DATA_DIR");
    // SAFETY: widget tests are serialized by the GTK harness before the test
    // constructs windows or starts background app-data tasks.
    unsafe { std::env::set_var("LUSHTEXT_DATA_DIR", tempdir.path()) };
    IsolatedDataDir { tempdir, previous }
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
