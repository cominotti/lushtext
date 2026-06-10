// SPDX-License-Identifier: MIT OR Apache-2.0

use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Label, glib};
use gtk_lush_settle as _;
use gtk4 as gtk;

/// Stable app id for manually running the standalone adoption example.
const APP_ID: &str = "dev.cominotti.gtk_lush_settle.standalone";

/// Opt-in smoke mode lets CI launch the app under headless Mutter and exit.
const HEADLESS_SMOKE_ENV: &str = "GTK_LUSH_STANDALONE_SMOKE";

fn main() -> glib::ExitCode {
    let app = Application::builder().application_id(APP_ID).build();

    app.connect_activate(|app| {
        let label = Label::new(Some("gtk-lush-settle placeholder"));
        let window = ApplicationWindow::builder()
            .application(app)
            .title("GTK Lush Settle")
            .default_width(320)
            .default_height(120)
            .child(&label)
            .build();

        window.present();

        if std::env::var_os(HEADLESS_SMOKE_ENV).is_some() {
            app.quit();
        }
    });

    app.run()
}
