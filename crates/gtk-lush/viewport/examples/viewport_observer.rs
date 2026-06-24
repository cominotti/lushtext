// SPDX-License-Identifier: MIT OR Apache-2.0

//! Standalone viewport observer example for scrollable GTK widgets.
//!
//! It demonstrates adjustment-change reporting and rest-state recording without
//! bringing in LushText's editor, making the adoption contract easy to inspect.

use std::rc::Rc;

use gtk_lush_viewport::{RestState, ViewportAxis, ViewportObserver};
use gtk4::glib;
use gtk4::prelude::*;

fn main() {
    let app = gtk4::Application::builder()
        .application_id("dev.gtk_lush.ViewportObserverExample")
        .build();

    app.connect_activate(|app| {
        let view = gtk4::TextView::new();
        view.set_monospace(true);
        view.buffer()
            .set_text("Resize the window to emit viewport adjustment changes.");

        let rest_state = RestState::new();
        let observer = ViewportObserver::for_scrollable(
            &view,
            move |change| match change.axis() {
                ViewportAxis::Horizontal => println!("viewport width: {}", change.page_size()),
                ViewportAxis::Vertical => println!("viewport height: {}", change.page_size()),
            },
            move |change| {
                let _ = rest_state.record_adjustment(change.axis(), change.adjustment());
            },
        )
        .expect("GtkTextView exposes scroll adjustments after construction");
        let observer = Rc::new(observer);

        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .title("ViewportObserver")
            .default_width(480)
            .default_height(320)
            .child(&view)
            .build();

        let keep_observer_alive = Rc::clone(&observer);
        window.connect_close_request(move |_| {
            let _ = keep_observer_alive.len();
            glib::Propagation::Proceed
        });
        window.present();
    });

    app.run();
}
