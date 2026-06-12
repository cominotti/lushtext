// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Duration;

use gtk4::prelude::*;
use gtk_lush_settle::{Debounce, SettleBurst, SupersedingTimer};

fn main() {
    let app = gtk4::Application::builder()
        .application_id("dev.gtk_lush.StockSettleFixture")
        .build();

    app.connect_activate(|app| {
        let debounce = Debounce::new();
        let burst = SettleBurst::new();
        let timer = SupersedingTimer::new();

        let label = gtk4::Label::new(Some("Type to schedule the latest generation."));
        label.set_hexpand(true);
        label.set_wrap(true);
        label.set_xalign(0.0);

        let entry = gtk4::Entry::new();
        entry.set_placeholder_text(Some("stock gtk-rs entry"));
        entry.connect_changed({
            let debounce = debounce.clone();
            let label = label.clone();
            move |entry| {
                let text = entry.text().to_string();
                let token = debounce.schedule(&label, Duration::from_millis(150), move |label, token| {
                    label.set_text(&format!("generation {} accepted: {text}", token.value()));
                });
                label.set_text(&format!("scheduled generation {}", token.value()));
            }
        });

        let settle_button = gtk4::Button::with_label("Settle");
        settle_button.connect_clicked({
            let burst = burst.clone();
            let label = label.clone();
            move |_| {
                let handle = burst.schedule(&label, Duration::from_millis(100), move |label, handle| {
                    label.set_text(&format!("settled generation {}", handle.token().value()));
                    handle.finish_if_current();
                });
                label.set_text(&format!("pending generation {}", handle.token().value()));
            }
        });

        let cleanup_button = gtk4::Button::with_label("Re-arm cleanup");
        cleanup_button.connect_clicked({
            let timer = timer.clone();
            let label = label.clone();
            move |_| {
                let token = timer.arm(&label, Duration::from_millis(100), move |label, token| {
                    label.set_text(&format!("cleanup generation {} ran", token.value()));
                });
                label.set_text(&format!("cleanup generation {} armed", token.value()));
            }
        });

        let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        controls.append(&settle_button);
        controls.append(&cleanup_button);

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        root.set_margin_top(18);
        root.set_margin_bottom(18);
        root.set_margin_start(18);
        root.set_margin_end(18);
        root.append(&entry);
        root.append(&controls);
        root.append(&label);

        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .title("Stock gtk-rs + gtk-lush-settle")
            .default_width(480)
            .default_height(220)
            .child(&root)
            .build();
        window.present();
    });

    app.run();
}
