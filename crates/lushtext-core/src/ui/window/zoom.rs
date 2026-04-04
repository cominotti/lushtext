// SPDX-License-Identifier: GPL-3.0-or-later

//! Zoom controls: hamburger menu widget ([−] [100%] [+]) and window actions.
//!
//! The zoom level is stored as a percentage in GSettings (`zoom-level`, 50–400,
//! default 100). Changing the setting triggers a CSS re-apply in `lib.rs` that
//! scales the `.monospace` font-size across all editors and the sidebar file tree.

use crate::config::keys;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{gio, glib};

const ZOOM_MIN: u32 = 50;
const ZOOM_MAX: u32 = 400;
const ZOOM_STEP: u32 = 10;
const ZOOM_DEFAULT: u32 = 100;

/// Build the [−] [100%] [+] widget and insert it into the hamburger menu
/// popover at the `<attribute name="custom">zoom</attribute>` slot.
pub(super) fn setup_zoom_controls(window: &super::LushtextWindow) {
    let settings = &window.imp().settings;

    // Container matching GNOME Text Editor's zoom row layout.
    let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    container.add_css_class("zoom-controls");
    container.set_hexpand(true);
    container.set_halign(gtk4::Align::Center);

    let zoom_out_btn = gtk4::Button::builder()
        .icon_name("zoom-out-symbolic")
        .tooltip_text("Zoom Out")
        .build();
    zoom_out_btn.add_css_class("circular");
    zoom_out_btn.add_css_class("flat");

    // The percentage label doubles as the reset button (click to reset to 100%).
    let zoom_label_btn = gtk4::Button::builder()
        .tooltip_text("Reset Zoom")
        .hexpand(true)
        .build();
    zoom_label_btn.add_css_class("flat");
    zoom_label_btn.add_css_class("pill");

    let zoom_in_btn = gtk4::Button::builder()
        .icon_name("zoom-in-symbolic")
        .tooltip_text("Zoom In")
        .build();
    zoom_in_btn.add_css_class("circular");
    zoom_in_btn.add_css_class("flat");

    container.append(&zoom_out_btn);
    container.append(&zoom_label_btn);
    container.append(&zoom_in_btn);

    // Shared closure to sync the label text and button sensitivity with the
    // current zoom value. Called on init and on every zoom-level change.
    let sync_ui = {
        let label = zoom_label_btn.clone();
        let btn_in = zoom_in_btn.clone();
        let btn_out = zoom_out_btn.clone();
        move |zoom: u32| {
            label.set_label(&format!("{zoom}%"));
            btn_in.set_sensitive(zoom < ZOOM_MAX);
            btn_out.set_sensitive(zoom > ZOOM_MIN);
        }
    };
    sync_ui(settings.uint(keys::ZOOM_LEVEL).clamp(ZOOM_MIN, ZOOM_MAX));

    // Wire button clicks to GSettings changes.
    {
        let s = settings.clone();
        zoom_in_btn.connect_clicked(move |_| {
            let current = s.uint(keys::ZOOM_LEVEL).clamp(ZOOM_MIN, ZOOM_MAX);
            if current < ZOOM_MAX {
                let _ = s.set_uint(keys::ZOOM_LEVEL, (current + ZOOM_STEP).min(ZOOM_MAX));
            }
        });
    }
    {
        let s = settings.clone();
        zoom_out_btn.connect_clicked(move |_| {
            let current = s.uint(keys::ZOOM_LEVEL).clamp(ZOOM_MIN, ZOOM_MAX);
            if current > ZOOM_MIN {
                let _ = s.set_uint(keys::ZOOM_LEVEL, (current - ZOOM_STEP).max(ZOOM_MIN));
            }
        });
    }
    {
        let s = settings.clone();
        zoom_label_btn.connect_clicked(move |_| {
            let _ = s.set_uint(keys::ZOOM_LEVEL, ZOOM_DEFAULT);
        });
    }

    // React to external zoom changes (keyboard shortcuts, command palette).
    {
        settings.connect_changed(Some(keys::ZOOM_LEVEL), move |s, _| {
            sync_ui(s.uint(keys::ZOOM_LEVEL).clamp(ZOOM_MIN, ZOOM_MAX));
        });
    }

    // Insert into the hamburger menu popover.
    let menu_button = &window.imp().primary_menu_button;
    let Some(popover) = menu_button.popover() else {
        tracing::error!("setup_zoom_controls: primary_menu_button has no popover");
        return;
    };
    let Ok(popover_menu) = popover.downcast::<gtk4::PopoverMenu>() else {
        tracing::error!("setup_zoom_controls: popover is not a PopoverMenu");
        return;
    };
    if !popover_menu.add_child(&container, "zoom") {
        tracing::error!(
            "setup_zoom_controls: failed to add zoom widget \
             (missing 'zoom' custom slot in menu XML?)"
        );
    }
}

/// Register `zoom-in`, `zoom-out`, and `zoom-reset` window actions.
/// These are activated by keyboard shortcuts and command palette entries.
pub(super) fn setup_zoom_actions(window: &super::LushtextWindow) {
    window.add_action_entries([
        gio::ActionEntry::builder("zoom-in")
            .activate(|window: &super::LushtextWindow, _, _| {
                let s = &window.imp().settings;
                let current = s.uint(keys::ZOOM_LEVEL).clamp(ZOOM_MIN, ZOOM_MAX);
                if current < ZOOM_MAX {
                    let _ = s.set_uint(keys::ZOOM_LEVEL, (current + ZOOM_STEP).min(ZOOM_MAX));
                }
            })
            .build(),
        gio::ActionEntry::builder("zoom-out")
            .activate(|window: &super::LushtextWindow, _, _| {
                let s = &window.imp().settings;
                let current = s.uint(keys::ZOOM_LEVEL).clamp(ZOOM_MIN, ZOOM_MAX);
                if current > ZOOM_MIN {
                    let _ = s.set_uint(keys::ZOOM_LEVEL, (current - ZOOM_STEP).max(ZOOM_MIN));
                }
            })
            .build(),
        gio::ActionEntry::builder("zoom-reset")
            .activate(|window: &super::LushtextWindow, _, _| {
                let _ = window
                    .imp()
                    .settings
                    .set_uint(keys::ZOOM_LEVEL, ZOOM_DEFAULT);
            })
            .build(),
    ]);
}
