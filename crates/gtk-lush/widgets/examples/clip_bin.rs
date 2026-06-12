// SPDX-License-Identifier: MIT OR Apache-2.0

use gtk_lush_widgets::ClipBin;
use gtk4::prelude::*;

fn main() {
    let app = gtk4::Application::builder()
        .application_id("dev.gtk_lush.ClipBinExample")
        .build();

    app.connect_activate(|app| {
        let child = gtk4::Label::new(Some("flexible content"));
        child.set_hexpand(true);
        child.set_vexpand(true);

        let bin = ClipBin::with_child(&child);
        bin.set_hexpand(true);
        bin.set_vexpand(true);

        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .title("ClipBin")
            .default_width(360)
            .default_height(180)
            .child(&bin)
            .build();
        window.present();
    });

    app.run();
}
