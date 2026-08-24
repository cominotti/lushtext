// SPDX-License-Identifier: GPL-3.0-or-later

//! Application root for startup, activation, app actions, and D-Bus ownership.
//!
//! The public wrapper is a Libadwaita application subclass. Its private
//! implementation owns process-wide registrations that must follow the
//! `GApplication` lifecycle, including the read-only automation object.

use crate::config;
use crate::model::automation::{AutomationWorkflowEventsSnapshot, AutomationWorkflowObservation};
use crate::ui::preferences::LushtextPreferences;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;
use libadwaita::prelude::*;

mod imp {
    use crate::model::automation::AutomationWorkflowEventLog;
    use crate::ui::automation::AutomationRegistration;
    use crate::ui::window::LushtextWindow;
    use glib::prelude::*;
    use gtk4::gio;
    use gtk4::prelude::*;
    use libadwaita::subclass::prelude::*;
    use std::cell::RefCell;

    /// Private GObject implementation for the application singleton.
    ///
    /// GLib stores instance state here while the public wrapper below exposes
    /// the reference-counted application API used by the rest of the app.
    #[derive(Default)]
    pub struct LushtextApplication {
        /// Active automation D-Bus object registration.
        ///
        /// The handle is cleared before the application bus name is released so
        /// tools never see a stale child object under the app's object path.
        /// `RefCell` is used because GApplication lifecycle hooks receive shared
        /// `&self` while registration state changes over time.
        pub automation_registration: RefCell<Option<AutomationRegistration>>,
        /// Bounded workflow events observed by the read-only automation adapter.
        ///
        /// The app owns this instead of individual windows so a D-Bus client can
        /// poll one process-level event stream across window recreation.
        /// `RefCell` keeps mutation local to the GTK main context despite
        /// GObject callbacks exposing only shared `&self`.
        pub automation_workflow_events: RefCell<AutomationWorkflowEventLog>,
    }

    // ObjectSubclass registers this Rust struct with GLib's runtime type
    // system. NAME becomes the GType identifier and ParentType declares that
    // this application extends Libadwaita's Application class.
    #[glib::object_subclass]
    impl ObjectSubclass for LushtextApplication {
        const NAME: &str = "LushtextApplication";
        type Type = super::LushtextApplication;
        type ParentType = libadwaita::Application;
    }

    impl ObjectImpl for LushtextApplication {}

    impl ApplicationImpl for LushtextApplication {
        fn startup(&self) {
            self.parent_startup();
            crate::ui::theme::load_css();
            crate::ui::theme::register_app_icons();
            crate::ui::theme::register_sourceview_style_schemes();

            // Apply persisted color scheme before any window is created so
            // the first paint uses the correct theme.
            let settings = gtk4::gio::Settings::new(crate::config::APP_ID);
            let scheme = settings.string(crate::config::keys::COLOR_SCHEME);
            let color_scheme = crate::ui::window::parse_color_scheme(scheme.as_str());
            libadwaita::StyleManager::default().set_color_scheme(color_scheme);
        }

        fn dbus_register(
            &self,
            connection: &gio::DBusConnection,
            object_path: &str,
        ) -> Result<(), glib::Error> {
            // GApplication calls this after owning its session-bus name. Export
            // the automation child object on the same connection so tools can
            // discover it under the normal application object path.
            self.parent_dbus_register(connection, object_path)?;
            let registration =
                crate::ui::automation::register(&self.obj(), connection, object_path)?;
            self.automation_registration.replace(Some(registration));
            Ok(())
        }

        fn dbus_unregister(&self, connection: &gio::DBusConnection, object_path: &str) {
            // Unregister the child object before the parent releases its D-Bus
            // object, keeping the exported object tree internally consistent.
            if let Some(registration) = self.automation_registration.borrow_mut().take() {
                registration.unregister();
            }
            self.parent_dbus_unregister(connection, object_path);
        }

        fn activate(&self) {
            let binding = self.obj();
            let app: &super::LushtextApplication = binding.as_ref();

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
                // GTK returns the active window as a generic ApplicationWindow;
                // downcast asks GLib's runtime type system for LushText's
                // concrete window type before forwarding opened files.
                w.downcast::<LushtextWindow>()
                    .expect("active window is LushtextWindow")
            } else {
                LushtextWindow::new(app.upcast_ref())
            };

            // Root the window before dispatching asynchronous file loads so
            // quick failures have a visible status and inline-error surface.
            window.present();
            for file in files {
                if let Some(path) = file.path() {
                    window.open_document_from_activation(&path);
                } else {
                    window.report_unsupported_open_file(file);
                }
            }

            window.present();
        }
    }

    impl GtkApplicationImpl for LushtextApplication {}
    impl AdwApplicationImpl for LushtextApplication {}
}

// glib::wrapper! generates the public reference-counted GObject wrapper for
// the private imp::LushtextApplication. The @extends chain records the GTK
// class hierarchy, and @implements lists supported GIO interfaces.
glib::wrapper! {
    /// Libadwaita application wrapper that owns startup, activation, CLI open, and app actions.
    pub struct LushtextApplication(ObjectSubclass<imp::LushtextApplication>)
        @extends libadwaita::Application, gtk4::Application, gio::Application,
        @implements gio::ActionMap, gio::ActionGroup;
}

impl LushtextApplication {
    /// Create the production application with the configured app id.
    #[must_use]
    pub fn new() -> Self {
        Self::new_with_application_id(config::APP_ID)
    }

    /// Create an application with a caller-provided id for isolated widget tests.
    #[must_use]
    pub fn new_with_application_id(application_id: &str) -> Self {
        let app: Self = glib::Object::builder()
            .property("application-id", application_id)
            .property("resource-base-path", config::RESOURCE_BASE_PATH)
            .property("flags", gio::ApplicationFlags::HANDLES_OPEN)
            .build();

        app.setup_actions();
        app
    }

    /// Install app-level actions and accelerators available before a window owns focus.
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

    /// Record current workflow observations and return the bounded event stream.
    ///
    /// This mutates only the process-level diagnostic log behind a `RefCell`;
    /// it does not change user-visible GTK state or persisted data.
    pub(crate) fn observe_automation_workflows(
        &self,
        observations: impl IntoIterator<Item = AutomationWorkflowObservation>,
    ) -> AutomationWorkflowEventsSnapshot {
        let mut events = self.imp().automation_workflow_events.borrow_mut();
        events.observe(observations);
        events.snapshot()
    }
}

impl Default for LushtextApplication {
    fn default() -> Self {
        Self::new()
    }
}
