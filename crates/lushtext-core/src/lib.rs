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
///
/// Installed/Flatpak builds: loads from the Meson-installed `.gresource` file.
/// Dev builds: falls back to the `build.rs`-compiled bundle via `include_bytes!`.
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
        if dev_dir.join("gschemas.compiled").exists() {
            // SAFETY: set_var is unsafe because concurrent env access is UB.
            // This runs during run(), before app.run() starts the GTK main
            // loop and before any background threads are spawned.
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
    // USER priority (higher than APPLICATION) so custom font overrides the base stylesheet.
    let font_provider = gtk4::CssProvider::new();
    gtk4::style_context_add_provider_for_display(
        &display,
        &font_provider,
        gtk4::STYLE_PROVIDER_PRIORITY_USER,
    );
    let settings = gio::Settings::new(config::APP_ID);
    apply_font_css(&font_provider, &settings);
    for key in [
        config::keys::USE_SYSTEM_FONT,
        config::keys::CUSTOM_FONT,
        config::keys::ZOOM_LEVEL,
    ] {
        let p = font_provider.clone();
        let s = settings.clone();
        settings.connect_changed(Some(key), move |_, _| apply_font_css(&p, &s));
    }
}

fn apply_font_css(provider: &gtk4::CssProvider, settings: &gio::Settings) {
    let zoom = settings.uint(config::keys::ZOOM_LEVEL).clamp(50, 400);
    let use_system = settings.boolean(config::keys::USE_SYSTEM_FONT);

    // System font at 100% — no CSS override needed, let GTK defaults apply.
    if use_system && zoom == 100 {
        provider.load_from_string("");
        return;
    }

    // Resolve the base font: system monospace from GNOME desktop settings,
    // or the user's custom font from our own GSettings.
    // Guard against non-GNOME desktops where the schema may not exist
    // (gio::Settings::new aborts if the schema is missing).
    let desc = if use_system {
        let source = gio::SettingsSchemaSource::default().expect("schema source");
        if source.lookup("org.gnome.desktop.interface", true).is_some() {
            let iface = gio::Settings::new("org.gnome.desktop.interface");
            pango::FontDescription::from_string(&iface.string("monospace-font-name"))
        } else {
            pango::FontDescription::from_string("Monospace 11")
        }
    } else {
        pango::FontDescription::from_string(&settings.string(config::keys::CUSTOM_FONT))
    };

    let family = desc.family().unwrap_or_else(|| "Monospace".into());
    // Pango stores font sizes in 1/1024 pt (PANGO_SCALE); divide to get CSS-compatible points.
    let base_pt = {
        let raw = f64::from(desc.size()) / f64::from(pango::SCALE);
        if raw > 0.0 { raw } else { 11.0 }
    };
    let zoomed_pt = base_pt * f64::from(zoom) / 100.0;

    let css = format!(".monospace {{ font-family: \"{family}\"; font-size: {zoomed_pt:.1}pt; }}");
    provider.load_from_string(&css);
}
