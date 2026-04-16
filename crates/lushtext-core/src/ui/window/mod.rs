// SPDX-License-Identifier: GPL-3.0-or-later

//! Main application window.
//!
//! The window is the top-level driving adapter for the application shell. Its
//! public API stays here, while document lifecycle, action wiring, notifications,
//! and focus/indexing helpers live in dedicated modules to keep the adapter readable.

mod actions;
mod dialogs;
mod documents;
mod drafts;
mod encoding;
mod focus_indexing;
mod imp;
mod notes;
mod notifications;
mod preview;
mod print;
mod search;
mod session_persistence;
mod tabs;
mod zoom;

use crate::config::keys;
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;

/// Maximum total estimated buffer memory across all tabs before evicting
/// unmodified background tabs. ~256MB is comfortable on 8GB machines.
const BUFFER_MEMORY_BUDGET: u64 = 256_000_000;

/// Map a GSettings `color-scheme` string to its `libadwaita::ColorScheme` variant.
/// Unknown values fall back to `Default` (follow system).
#[must_use]
pub fn parse_color_scheme(value: &str) -> libadwaita::ColorScheme {
    match value {
        "force-light" => libadwaita::ColorScheme::ForceLight,
        "force-dark" => libadwaita::ColorScheme::ForceDark,
        _ => libadwaita::ColorScheme::Default,
    }
}

// glib::wrapper! generates the public wrapper type for this widget.
// @extends declares the GTK class hierarchy; @implements lists interfaces.
glib::wrapper! {
    pub struct LushtextWindow(ObjectSubclass<imp::LushtextWindow>)
        @extends libadwaita::ApplicationWindow, gtk4::ApplicationWindow, gtk4::Window, gtk4::Widget,
        @implements gio::ActionMap, gio::ActionGroup, gtk4::Accessible, gtk4::Buildable,
                    gtk4::ConstraintTarget, gtk4::Native, gtk4::Root, gtk4::ShortcutManager;
}

impl LushtextWindow {
    #[must_use]
    pub fn new(app: &libadwaita::Application) -> Self {
        let window: Self = Object::builder().property("application", app).build();
        window.setup_actions();
        window.setup_tab_management();
        window.setup_fullscreen();
        window.setup_theme_selector();
        preview::setup_preview_actions(&window);
        print::setup_print_action(&window);
        zoom::setup_zoom_actions(&window);
        zoom::setup_zoom_controls(&window);
        search::setup_search_panel(&window);
        window.start_notification_sweep_timer();
        window.setup_shortcuts();
        window.update_content_stack();
        window.refresh_status_bar();
        window.render_notifications();
        window
    }

    /// Create the theme selector widget (follow-system/light/dark circles)
    /// matching GNOME Text Editor's visual pattern, and insert it into
    /// the hamburger menu's popover as a custom child.
    fn setup_theme_selector(&self) {
        let settings = &self.imp().settings;
        let style_manager = libadwaita::StyleManager::default();

        // Container with the CSS class that targets the custom styling.
        let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        container.add_css_class("theme-selector");
        container.set_hexpand(true);
        container.set_halign(gtk4::Align::Center);

        // GtkCheckButton radio group — CSS hides the radio indicator
        // and styles each button as a 44px colored circle.
        let follow_btn = gtk4::CheckButton::builder()
            .tooltip_text("Follow System Style")
            .halign(gtk4::Align::Center)
            .hexpand(true)
            .focus_on_click(false)
            .build();
        follow_btn.add_css_class("follow");

        let light_btn = gtk4::CheckButton::builder()
            .tooltip_text("Light Style")
            .halign(gtk4::Align::Center)
            .hexpand(true)
            .focus_on_click(false)
            .group(&follow_btn)
            .build();
        light_btn.add_css_class("light");

        let dark_btn = gtk4::CheckButton::builder()
            .tooltip_text("Dark Style")
            .halign(gtk4::Align::Center)
            .hexpand(true)
            .focus_on_click(false)
            .group(&follow_btn)
            .build();
        dark_btn.add_css_class("dark");

        container.append(&follow_btn);
        container.append(&light_btn);
        container.append(&dark_btn);

        let scheme = settings.string(keys::COLOR_SCHEME);
        match parse_color_scheme(scheme.as_str()) {
            libadwaita::ColorScheme::ForceLight => light_btn.set_active(true),
            libadwaita::ColorScheme::ForceDark => dark_btn.set_active(true),
            _ => follow_btn.set_active(true),
        }

        {
            let sm = style_manager.clone();
            let s = settings.clone();
            light_btn.connect_toggled(move |btn| {
                if btn.is_active() {
                    sm.set_color_scheme(libadwaita::ColorScheme::ForceLight);
                    let _ = s.set_string(keys::COLOR_SCHEME, "force-light");
                }
            });
        }
        {
            let sm = style_manager.clone();
            let s = settings.clone();
            follow_btn.connect_toggled(move |btn| {
                if btn.is_active() {
                    sm.set_color_scheme(libadwaita::ColorScheme::Default);
                    let _ = s.set_string(keys::COLOR_SCHEME, "default");
                }
            });
        }
        {
            let sm = style_manager;
            let s = settings.clone();
            dark_btn.connect_toggled(move |btn| {
                if btn.is_active() {
                    sm.set_color_scheme(libadwaita::ColorScheme::ForceDark);
                    let _ = s.set_string(keys::COLOR_SCHEME, "force-dark");
                }
            });
        }

        let menu_button = &self.imp().primary_menu_button;
        let Some(popover) = menu_button.popover() else {
            tracing::error!("setup_theme_selector: primary_menu_button has no popover");
            return;
        };
        let Ok(popover_menu) = popover.downcast::<gtk4::PopoverMenu>() else {
            tracing::error!("setup_theme_selector: popover is not a PopoverMenu");
            return;
        };
        if !popover_menu.add_child(&container, "theme") {
            tracing::error!(
                "setup_theme_selector: failed to add theme widget \
                 (missing 'theme' custom slot in menu XML?)"
            );
        }
    }
}
