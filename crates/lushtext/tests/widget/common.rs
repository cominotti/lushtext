// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared test infrastructure for widget tests.
//!
//! GTK4 must be initialized before constructing any widget, and GResources
//! must be registered before constructing widgets that use composite templates.
//! Both operations are one-time setup via `std::sync::Once`.

use std::sync::Once;

static GTK_INIT: Once = Once::new();

/// Initialize GTK4 and register GResources. Safe to call multiple times;
/// the actual work only runs once.
///
/// Requires a display server — run under `xvfb-run` for headless environments.
pub fn ensure_gtk_init() {
    GTK_INIT.call_once(|| {
        gtk4::init().expect("GTK4 init failed — is a display server available? Try xvfb-run.");
        lushtext_core::register_resources();
    });
}
