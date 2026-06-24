// SPDX-License-Identifier: GPL-3.0-or-later

//! LushText — a minimalist text editor for GNOME.
//!
//! This crate contains all application logic: data models, services,
//! and GTK4/Libadwaita UI widgets.

pub mod app;
pub mod config;
#[cfg(feature = "fuzzing")]
pub mod fuzzing;
pub mod model;
pub mod services;
pub mod ui;

use gio::prelude::*;
use glib::ExitCode;
use gtk4::gio;
use services::filesystem::metadata as fs_metadata;

/// Register the compiled GResource bundle. Must be called before constructing
/// any widgets that use composite templates.
///
/// Installed/Flatpak builds: loads from the Meson-installed `.gresource` file.
/// Dev builds: falls back to the `build.rs`-compiled bundle via `include_bytes!`.
///
/// # Panics
///
/// Panics if the installed `.gresource` file or embedded development resource
/// bundle cannot be loaded, because that indicates a broken build or install.
pub fn register_resources() {
    // Installed build: load from Meson-installed path (panic on failure — a
    // missing .gresource means a broken installation, not a reason to fall back)
    if let Some(pkgdatadir) = config::PKGDATADIR {
        let path = std::path::Path::new(pkgdatadir).join("lushtext.gresource");
        let resource = gio::Resource::load(&path).unwrap_or_else(|e| {
            panic!(
                "failed to load installed GResource at {}: {e}",
                path.display()
            )
        });
        gio::resources_register(&resource);
        return;
    }

    // Dev build: embedded resources from build.rs
    let resource_bytes = glib::Bytes::from_static(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/lushtext.gresource"
    )));
    let resource = gio::Resource::from_data(&resource_bytes).expect("failed to load GResource");
    gio::resources_register(&resource);
}

/// Entry point called from `main()`. Registers GResources, creates the application,
/// and runs the GTK main loop.
#[must_use]
pub fn run() -> ExitCode {
    register_resources();
    init_schema_dir();

    let app = app::LushtextApplication::new();
    app.run()
}

/// For dev/uninstalled builds, point GLib to the compiled GSettings schemas
/// in the source tree. Installed builds use the system schema directory.
pub fn init_schema_dir() {
    // Installed builds: schema is in the system directory via Meson install
    if config::PKGDATADIR.is_some() {
        return;
    }

    // Dev builds: point to source tree's compiled schemas
    if std::env::var_os("GSETTINGS_SCHEMA_DIR").is_none() {
        let dev_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        if fs_metadata::exists(&dev_dir.join("gschemas.compiled")) {
            // SAFETY: set_var is unsafe because concurrent env access is UB.
            // This runs during run(), before app.run() starts the GTK main
            // loop and before any background threads are spawned.
            unsafe { std::env::set_var("GSETTINGS_SCHEMA_DIR", &dev_dir) };
        }
    }
}
