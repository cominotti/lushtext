// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the editor info bar widget.
//!
//! Contains two `GtkInfoBar` sub-bars for different notification scenarios:
//! - Access error ("Could Not Open File") with a Retry button
//! - Draft/external change ("Draft Changes Restored" / "File Has Changed on Disk")
//!   with Discard and Save buttons
//!
//! Each sub-bar starts with `revealed=false` and is shown via the public API
//! methods in `mod.rs`. The yellow/amber and red colors come from the Adwaita
//! theme's built-in `GtkInfoBar` styling — no custom CSS needed.
//!
//! GtkInfoBar was deprecated in GTK 4.10, but its replacement (AdwBanner)
//! doesn't support multi-button action areas. GNOME Text Editor still uses
//! GtkInfoBar in its latest code for the same reason.

use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, glib};
use std::cell::RefCell;

/// Stored callback for info bar button actions.
/// Same pattern as `SearchBar::connect_close` and `Sidebar::connect_file_activated`.
type Callback = Box<dyn Fn()>;

// GtkInfoBar is deprecated since GTK 4.10 but has no multi-button replacement.
// GNOME Text Editor still uses GtkInfoBar in its latest code for the same reason.
#[expect(
    deprecated,
    reason = "GtkInfoBar still provides the only multi-action infobar pattern that matches GNOME Text Editor for this UI"
)]
type GtkInfoBar = gtk4::InfoBar;

/// Allow a button's internal label to wrap so `GtkInfoBar` actions stay
/// visible when the editor column gets narrow instead of collapsing away.
fn wrap_button_label(button: &gtk4::Button) {
    let Some(child) = button.child() else {
        return;
    };
    let Ok(label) = child.downcast::<gtk4::Label>() else {
        return;
    };

    label.set_wrap(true);
    label.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
    label.set_justify(gtk4::Justification::Center);
}

#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/info-bar.ui")]
pub struct LushtextInfoBar {
    // --- Access error bar (red, message-type=error) ---
    #[template_child]
    pub access_infobar: TemplateChild<GtkInfoBar>,
    #[template_child]
    pub access_title: TemplateChild<gtk4::Label>,
    #[template_child]
    pub access_subtitle: TemplateChild<gtk4::Label>,
    #[template_child]
    pub retry_button: TemplateChild<gtk4::Button>,

    // --- Discard/draft bar (yellow, message-type=warning) ---
    #[template_child]
    pub discard_infobar: TemplateChild<GtkInfoBar>,
    #[template_child]
    pub discard_title: TemplateChild<gtk4::Label>,
    #[template_child]
    pub discard_subtitle: TemplateChild<gtk4::Label>,
    #[template_child]
    pub discard_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub save_button: TemplateChild<gtk4::Button>,

    // --- Button callbacks ---
    pub retry_callback: RefCell<Option<Callback>>,
    pub save_callback: RefCell<Option<Callback>>,
    pub discard_callback: RefCell<Option<Callback>>,
    pub dismissed_callback: RefCell<Option<Callback>>,
}

impl Default for LushtextInfoBar {
    fn default() -> Self {
        Self {
            access_infobar: TemplateChild::default(),
            access_title: TemplateChild::default(),
            access_subtitle: TemplateChild::default(),
            retry_button: TemplateChild::default(),
            discard_infobar: TemplateChild::default(),
            discard_title: TemplateChild::default(),
            discard_subtitle: TemplateChild::default(),
            discard_button: TemplateChild::default(),
            save_button: TemplateChild::default(),
            retry_callback: RefCell::new(None),
            save_callback: RefCell::new(None),
            discard_callback: RefCell::new(None),
            dismissed_callback: RefCell::new(None),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextInfoBar {
    const NAME: &'static str = "LushtextInfoBar";
    type Type = super::LushtextInfoBar;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextInfoBar {
    #[expect(
        deprecated,
        reason = "GtkInfoBar still provides the only multi-action infobar pattern that matches GNOME Text Editor for this UI"
    )]
    fn constructed(&self) {
        self.parent_constructed();

        // GNOME Text Editor wraps its infobar action labels so restored-file
        // banners stay readable on narrow windows. LushText follows the same
        // pattern instead of hiding actions behind a larger window minimum.
        wrap_button_label(&self.retry_button);
        wrap_button_label(&self.discard_button);
        wrap_button_label(&self.save_button);

        // Wire button clicks to invoke stored callbacks.
        // Each button fires its callback and then hides the parent info bar.
        {
            let obj_weak = self.obj().downgrade();
            self.retry_button.connect_clicked(move |_| {
                if let Some(obj) = obj_weak.upgrade()
                    && let Some(ref cb) = *obj.imp().retry_callback.borrow()
                {
                    cb();
                }
            });
        }
        {
            let obj_weak = self.obj().downgrade();
            self.save_button.connect_clicked(move |_| {
                if let Some(obj) = obj_weak.upgrade()
                    && let Some(ref cb) = *obj.imp().save_callback.borrow()
                {
                    cb();
                }
            });
        }
        {
            let obj_weak = self.obj().downgrade();
            self.discard_button.connect_clicked(move |_| {
                if let Some(obj) = obj_weak.upgrade()
                    && let Some(ref cb) = *obj.imp().discard_callback.borrow()
                {
                    cb();
                }
            });
        }

        // Wire the close button on each GtkInfoBar to dismiss it.
        // GtkInfoBar emits `response` with GTK_RESPONSE_CLOSE when the
        // built-in close button is clicked.
        {
            let obj_weak = self.obj().downgrade();
            self.access_infobar.connect_response(move |_, _| {
                if let Some(obj) = obj_weak.upgrade()
                    && let Some(ref cb) = *obj.imp().dismissed_callback.borrow()
                {
                    cb();
                }
            });
        }
        {
            let obj_weak = self.obj().downgrade();
            self.discard_infobar.connect_response(move |_, _| {
                if let Some(obj) = obj_weak.upgrade()
                    && let Some(ref cb) = *obj.imp().dismissed_callback.borrow()
                {
                    cb();
                }
            });
        }
    }
}

impl WidgetImpl for LushtextInfoBar {}
impl BoxImpl for LushtextInfoBar {}
