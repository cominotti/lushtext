// SPDX-License-Identifier: GPL-3.0-or-later

//! LushtextApplication — AdwApplication subclass.

use crate::config;
use crate::ui::preferences::LushtextPreferences;
use gtk4::gio;
use gtk4::prelude::*;
use libadwaita::prelude::*;

mod imp {
    use crate::ui::window::LushtextWindow;
    use glib::prelude::*;
    use gtk4::prelude::*;
    use libadwaita::subclass::prelude::*;

    #[derive(Default)]
    pub struct LushtextApplication;

    #[glib::object_subclass]
    impl ObjectSubclass for LushtextApplication {
        const NAME: &'static str = "LushtextApplication";
        type Type = super::LushtextApplication;
        type ParentType = libadwaita::Application;
    }

    impl ObjectImpl for LushtextApplication {}

    impl ApplicationImpl for LushtextApplication {
        fn startup(&self) {
            self.parent_startup();
            crate::load_css();

            // Apply persisted color scheme before any window is created so
            // the first paint uses the correct theme.
            let settings = gtk4::gio::Settings::new(crate::config::APP_ID);
            let scheme = settings.string(crate::config::keys::COLOR_SCHEME);
            let color_scheme = crate::ui::window::parse_color_scheme(scheme.as_str());
            libadwaita::StyleManager::default().set_color_scheme(color_scheme);
        }

        fn activate(&self) {
            let binding = self.obj();
            let app: &super::LushtextApplication = binding.as_ref();

            // If a window already exists, present it
            if let Some(window) = app.active_window() {
                window.present();
                return;
            }

            let window = LushtextWindow::new(app.upcast_ref());
            window.present();
        }

        fn open(&self, files: &[gio::File], _hint: &str) {
            let app = self.obj();

            let window = if let Some(w) = app.active_window() {
                w.downcast::<LushtextWindow>()
                    .expect("active window is LushtextWindow")
            } else {
                LushtextWindow::new(app.upcast_ref())
            };

            for file in files {
                if let Some(path) = file.path() {
                    window.open_document(&path);
                }
            }

            window.present();
        }
    }

    impl GtkApplicationImpl for LushtextApplication {}
    impl AdwApplicationImpl for LushtextApplication {}
}

glib::wrapper! {
    pub struct LushtextApplication(ObjectSubclass<imp::LushtextApplication>)
        @extends libadwaita::Application, gtk4::Application, gio::Application,
        @implements gio::ActionMap, gio::ActionGroup;
}

impl LushtextApplication {
    pub fn new() -> Self {
        Self::new_with_application_id(config::APP_ID)
    }

    pub fn new_with_application_id(application_id: &str) -> Self {
        let app: Self = glib::Object::builder()
            .property("application-id", application_id)
            .property("flags", gio::ApplicationFlags::HANDLES_OPEN)
            .build();

        app.setup_actions();
        app
    }

    fn setup_actions(&self) {
        let action_quit = gio::ActionEntry::builder("quit")
            .activate(|app: &Self, _, _| app.quit())
            .build();

        let action_prefs = gio::ActionEntry::builder("preferences")
            .activate(|app: &Self, _, _| {
                if let Some(window) = app.active_window() {
                    let prefs = LushtextPreferences::new();
                    prefs.present(Some(&window));
                }
            })
            .build();

        let action_about = gio::ActionEntry::builder("about")
            .activate(|app: &Self, _, _| {
                if let Some(window) = app.active_window() {
                    let about = libadwaita::AboutDialog::builder()
                        .application_name("LushText")
                        .application_icon("dev.cominotti.lushtext")
                        .developer_name("Danilo Cominotti")
                        .version(config::VERSION)
                        .license_type(gtk4::License::Gpl30)
                        .website("https://github.com/cominotti/lushtext")
                        .build();
                    about.present(Some(&window));
                }
            })
            .build();

        self.add_action_entries([action_quit, action_prefs, action_about]);
        self.set_accels_for_action("app.quit", &["<Control>q"]);
    }
}

impl Default for LushtextApplication {
    fn default() -> Self {
        Self::new()
    }
}
