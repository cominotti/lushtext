// SPDX-License-Identifier: MIT OR Apache-2.0

//! Standalone GTK app demonstrating scoped signal and property-binding cleanup.
//!
//! The example mirrors the lifecycle pattern used by LushText custom widgets:
//! track handlers while the widget is alive, then clear them on close/dispose.

use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Button, Label, Orientation, Switch, gio, glib};
use gtk_lush_signals::{BindingBag, SignalBag};
use gtk4 as gtk;
use std::cell::Cell;
use std::rc::Rc;

/// Stable app id for manually running the standalone adoption example.
const APP_ID: &str = "dev.cominotti.gtk_lush_signals.standalone";

/// Opt-in smoke mode lets CI launch the app under headless Mutter and exit.
const HEADLESS_SMOKE_ENV: &str = "GTK_LUSH_STANDALONE_SMOKE";

fn main() -> glib::ExitCode {
    let app = Application::builder().application_id(APP_ID).build();

    app.connect_activate(|app| {
        let signals = Rc::new(SignalBag::new());
        let bindings = Rc::new(BindingBag::new());
        let clicks = Rc::new(Cell::new(0));

        let label = Label::new(Some("0 clicks"));
        let button = Button::with_label("Count");
        let toggle = Switch::builder().active(true).build();

        signals.track(
            &button,
            button.connect_clicked({
                let label = label.clone();
                let clicks = Rc::clone(&clicks);
                move |_| {
                    let next = clicks.get() + 1;
                    clicks.set(next);
                    label.set_label(&format!("{next} clicks"));
                }
            }),
        );

        let reset = gio::SimpleAction::new("reset", None);
        signals.track(
            &reset,
            reset.connect_activate({
                let label = label.clone();
                let clicks = Rc::clone(&clicks);
                move |_, _| {
                    clicks.set(0);
                    label.set_label("0 clicks");
                }
            }),
        );
        app.add_action(&reset);

        bindings.track(
            toggle
                .bind_property("active", &button, "sensitive")
                .sync_create()
                .build(),
        );

        let content = gtk::Box::new(Orientation::Vertical, 12);
        content.set_margin_top(18);
        content.set_margin_bottom(18);
        content.set_margin_start(18);
        content.set_margin_end(18);
        content.append(&label);
        content.append(&button);
        content.append(&toggle);

        let window = ApplicationWindow::builder()
            .application(app)
            .title("GTK Lush Signals")
            .default_width(320)
            .default_height(180)
            .child(&content)
            .build();

        window.connect_close_request(move |_| {
            signals.clear();
            bindings.clear();
            glib::Propagation::Proceed
        });

        window.present();

        if std::env::var_os(HEADLESS_SMOKE_ENV).is_some() {
            app.quit();
        }
    });

    app.run()
}
