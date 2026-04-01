// SPDX-License-Identifier: GPL-3.0-or-later

//! LushText — a minimalist text editor for GNOME.
//!
//! This crate contains all application logic: data models, services,
//! and GTK4/Libadwaita UI widgets.

pub mod app;
pub mod config;
pub mod model;
pub mod services;
pub mod ui;

use gio::prelude::*;
use glib::ExitCode;
use gtk4::gio;

/// Entry point called from `main()`. Sets up GResources, creates the application,
/// and runs the GTK main loop.
pub fn run() -> ExitCode {
    // Register GResources compiled by build.rs
    let resource_bytes = glib::Bytes::from_static(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/lushtext.gresource"
    )));
    let resource = gio::Resource::from_data(&resource_bytes).expect("failed to load GResource");
    gio::resources_register(&resource);

    // Load CSS
    let css_provider = gtk4::CssProvider::new();
    css_provider.load_from_resource("/dev/cominotti/lushtext/style/style.css");
    gtk4::style_context_add_provider_for_display(
        &gdk4::Display::default().expect("display"),
        &css_provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let app = app::LushtextApplication::new();
    app.run()
}
