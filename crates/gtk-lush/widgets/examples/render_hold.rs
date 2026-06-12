// SPDX-License-Identifier: MIT OR Apache-2.0

use std::rc::Rc;
use std::time::Duration;

use gtk_lush_widgets::{RenderHoldCapture, RenderHoldOverlay};
use gtk4::glib;
use gtk4::prelude::*;

fn main() {
    let app = gtk4::Application::builder()
        .application_id("dev.gtk_lush.RenderHoldExample")
        .build();

    app.connect_activate(|app| {
        let overlay = gtk4::Overlay::new();
        let label = gtk4::Label::new(Some("live child"));
        label.set_hexpand(true);
        label.set_vexpand(true);
        overlay.set_child(Some(&label));

        let hold = Rc::new(RenderHoldOverlay::new(&overlay, &label));
        let hold_for_idle = Rc::clone(&hold);
        glib::idle_add_local_once(move || match hold_for_idle.capture() {
            RenderHoldCapture::Captured => {
                hold_for_idle.warm_live_child();
                let hold_for_reveal = Rc::clone(&hold_for_idle);
                glib::timeout_add_local_once(Duration::from_millis(250), move || {
                    hold_for_reveal.reveal();
                });
            }
            RenderHoldCapture::AlreadyHolding | RenderHoldCapture::NotReady(_) => {}
        });

        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .title("RenderHoldOverlay")
            .default_width(360)
            .default_height(180)
            .child(&overlay)
            .build();

        let keep_hold_alive = Rc::clone(&hold);
        window.connect_close_request(move |_| {
            let _ = keep_hold_alive.is_active();
            glib::Propagation::Proceed
        });
        window.present();
    });

    app.run();
}
