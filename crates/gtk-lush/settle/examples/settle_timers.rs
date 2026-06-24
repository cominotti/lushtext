// SPDX-License-Identifier: MIT OR Apache-2.0

//! Standalone GTK app demonstrating the settle timers used by LushText widgets.
//!
//! It shows debounce, burst settle, and superseding cleanup behavior in one
//! manually launchable surface plus an opt-in smoke mode for adoption checks.

use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Button, Entry, Label, Orientation, glib};
use gtk_lush_settle::{Debounce, SettleBurst, SupersedingTimer};
use gtk4 as gtk;
use std::rc::Rc;
use std::time::Duration;

/// Stable app id for manually running the standalone adoption example.
const APP_ID: &str = "dev.cominotti.gtk_lush_settle.standalone";

/// Opt-in smoke mode lets CI launch the app under headless Mutter and exit.
const HEADLESS_SMOKE_ENV: &str = "GTK_LUSH_STANDALONE_SMOKE";

fn main() -> glib::ExitCode {
    let app = Application::builder().application_id(APP_ID).build();

    app.connect_activate(|app| {
        let debounce = Rc::new(Debounce::new());
        let settle = Rc::new(SettleBurst::new());
        let pulse_timer = Rc::new(SupersedingTimer::new());

        let entry = Entry::builder()
            .placeholder_text("Type to debounce")
            .build();
        let preview = Label::new(Some("Waiting"));
        let state = Label::new(Some("Settled"));
        let pulse = Button::with_label("Pulse");

        entry.connect_changed({
            let debounce = Rc::clone(&debounce);
            let settle = Rc::clone(&settle);
            let preview = preview.clone();
            let state = state.clone();
            move |entry| {
                state.set_label("Pending");
                debounce.schedule(entry, Duration::from_millis(150), {
                    let preview = preview.clone();
                    let state = state.clone();
                    let settle = Rc::clone(&settle);
                    move |entry, _| {
                        preview.set_label(entry.text().as_str());
                        settle.schedule(&entry, Duration::from_millis(80), move |_, handle| {
                            state.set_label("Settled");
                            handle.finish_if_current();
                        });
                    }
                });
            }
        });

        pulse.connect_clicked({
            let pulse_timer = Rc::clone(&pulse_timer);
            move |button| {
                button.add_css_class("suggested-action");
                pulse_timer.arm(button, Duration::from_millis(350), |button, _| {
                    button.remove_css_class("suggested-action");
                });
            }
        });

        let content = gtk::Box::new(Orientation::Vertical, 12);
        content.set_margin_top(18);
        content.set_margin_bottom(18);
        content.set_margin_start(18);
        content.set_margin_end(18);
        content.append(&entry);
        content.append(&preview);
        content.append(&state);
        content.append(&pulse);

        let window = ApplicationWindow::builder()
            .application(app)
            .title("GTK Lush Settle")
            .default_width(320)
            .default_height(220)
            .child(&content)
            .build();

        window.present();

        if std::env::var_os(HEADLESS_SMOKE_ENV).is_some() {
            app.quit();
        }
    });

    app.run()
}
