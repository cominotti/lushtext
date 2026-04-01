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

/// Register the compiled GResource bundle. Must be called before constructing
/// any widgets that use composite templates.
pub fn register_resources() {
    let resource_bytes = glib::Bytes::from_static(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/lushtext.gresource"
    )));
    let resource = gio::Resource::from_data(&resource_bytes).expect("failed to load GResource");
    gio::resources_register(&resource);
}

/// Entry point called from `main()`. Registers GResources, creates the application,
/// and runs the GTK main loop.
pub fn run() -> ExitCode {
    register_resources();
    init_schema_dir();

    let app = app::LushtextApplication::new();
    app.run()
}

/// For dev/uninstalled builds, point GLib to the compiled GSettings schemas
/// in the source tree. Installed builds use the system schema directory.
pub fn init_schema_dir() {
    if std::env::var_os("GSETTINGS_SCHEMA_DIR").is_none() {
        let dev_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        if dev_dir.join("gschemas.compiled").exists() {
            // SAFETY: called once at startup before any other threads.
            unsafe { std::env::set_var("GSETTINGS_SCHEMA_DIR", &dev_dir) };
        }
    }
}

/// Load the application CSS and set up the font customization provider.
/// Must be called after GTK is initialized (i.e. during startup).
pub(crate) fn load_css() {
    let display = gdk4::Display::default().expect("display");

    // App stylesheet
    let css_provider = gtk4::CssProvider::new();
    css_provider.load_from_resource("/dev/cominotti/lushtext/style/style.css");
    gtk4::style_context_add_provider_for_display(
        &display,
        &css_provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // Font customization provider — targets .monospace widgets (all GtkSourceViews).
    // Updated reactively via GSettings; overrides at USER priority.
    let font_provider = gtk4::CssProvider::new();
    gtk4::style_context_add_provider_for_display(
        &display,
        &font_provider,
        gtk4::STYLE_PROVIDER_PRIORITY_USER,
    );
    let settings = gio::Settings::new(config::APP_ID);
    apply_font_css(&font_provider, &settings);
    for key in [config::keys::USE_SYSTEM_FONT, config::keys::CUSTOM_FONT] {
        let p = font_provider.clone();
        let s = settings.clone();
        settings.connect_changed(Some(key), move |_, _| apply_font_css(&p, &s));
    }
}

fn apply_font_css(provider: &gtk4::CssProvider, settings: &gio::Settings) {
    if settings.boolean(config::keys::USE_SYSTEM_FONT) {
        provider.load_from_string("");
    } else {
        let font_str = settings.string(config::keys::CUSTOM_FONT);
        let desc = pango::FontDescription::from_string(&font_str);
        let family = desc.family().unwrap_or_else(|| "Monospace".into());
        let size_pt = desc.size() as f64 / pango::SCALE as f64;
        let css = format!(".monospace {{ font-family: \"{family}\"; font-size: {size_pt}pt; }}");
        provider.load_from_string(&css);
    }
}
