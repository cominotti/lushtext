// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared test infrastructure for widget tests.
//!
//! GTK4 must be initialized before constructing any widget, and GResources
//! must be registered before constructing widgets that use composite templates.
//! Both operations are one-time setup via `std::sync::Once`.

use std::sync::Once;

static GTK_INIT: Once = Once::new();

/// Initialize GTK4, register GResources, and set up GSettings for testing.
/// Safe to call multiple times; the actual work only runs once.
///
/// Uses the in-memory GSettings backend so tests don't pollute user's dconf.
/// Sets `LUSHTEXT_DATA_DIR` to a temp directory so session/draft I/O doesn't
/// touch the user's real data.
/// Requires a display server — use `mutter --headless` for headless environments.
pub fn ensure_gtk_init() {
    GTK_INIT.call_once(|| {
        // Use in-memory GSettings backend: starts with schema defaults, no persistence
        unsafe { std::env::set_var("GSETTINGS_BACKEND", "memory") };
        // Isolate session/draft I/O from the user's real data directory.
        // PID-based naming prevents nextest's parallel test processes from
        // interfering with each other via shared session files.
        let test_data_dir =
            std::env::temp_dir().join(format!("lushtext-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&test_data_dir);
        let _ = std::fs::create_dir_all(&test_data_dir);
        unsafe { std::env::set_var("LUSHTEXT_DATA_DIR", &test_data_dir) };
        lushtext_core::init_schema_dir();
        gtk4::init()
            .expect("GTK4 init failed — is a display server available? Try mutter --headless.");
        sourceview5::init();
        lushtext_core::register_resources();
    });
}
