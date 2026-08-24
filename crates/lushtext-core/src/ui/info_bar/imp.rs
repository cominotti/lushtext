// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the editor inline alert widget.
//!
//! Contains one GTK5-safe alert row for different notification scenarios:
//! - Access error ("Could Not Open File") with a Retry button
//! - Draft/external change ("Draft Changes Restored" / "File Has Changed on Disk")
//!   with Discard and Save buttons
//!
//! The row is wrapped in `GtkRevealer` for the same inline placement as
//! `GtkInfoBar`, while ordinary labels and buttons keep the widget compatible
//! with GTK5.

use crate::ui::accessibility;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, glib};
use std::cell::RefCell;

/// Stored callback for inline alert button actions.
/// Same pattern as `SearchBar::connect_close` and `Sidebar::connect_file_activated`.
type Callback = Box<dyn Fn()>;

/// Allow a button's internal label to wrap so inline alert actions stay
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

/// Private template implementation for the editor inline alert bar.
///
/// Binds the alert controls from `info-bar.ui` while the callbacks below stay
/// Rust-owned so editor pages can swap them per alert.
#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/info-bar.ui")]
pub struct LushtextInfoBar {
    /// Reveals and hides the alert row above the editor content.
    #[template_child]
    pub alert_revealer: TemplateChild<gtk4::Revealer>,
    /// Styled alert surface that groups the message and recovery actions.
    #[template_child]
    pub alert_box: TemplateChild<gtk4::Box>,
    /// Wrapping container that keeps the message and action group on one line
    /// when the editor column is wide and drops the action group onto its own
    /// row beneath the message when the column is too narrow.
    #[template_child]
    pub content_wrap: TemplateChild<libadwaita::WrapBox>,
    /// Title text for the current notification; it carries the leaf alert role
    /// so AT-SPI keeps the sibling action buttons reachable.
    #[template_child]
    pub alert_title: TemplateChild<gtk4::Label>,
    /// Body text for the currently rendered inline notification.
    #[template_child]
    pub alert_body: TemplateChild<gtk4::Label>,
    /// Holds workflow buttons plus the dismiss affordance in one grouped row.
    #[template_child]
    pub actions_box: TemplateChild<gtk4::Box>,
    /// Error-primary action used for retryable file access failures.
    #[template_child]
    pub retry_button: TemplateChild<gtk4::Button>,
    /// Warning-primary action used for discard, reload, normalize, or undo flows.
    #[template_child]
    pub discard_button: TemplateChild<gtk4::Button>,
    /// Warning-secondary action used for save and save-as flows.
    #[template_child]
    pub save_button: TemplateChild<gtk4::Button>,
    /// Explicit close affordance that clears the owning editor notification.
    #[template_child]
    pub dismiss_button: TemplateChild<gtk4::Button>,

    /// Callback invoked by the retry action.
    pub retry_callback: RefCell<Option<Callback>>,
    /// Callback invoked by the save action.
    pub save_callback: RefCell<Option<Callback>>,
    /// Callback invoked by the warning primary action.
    pub discard_callback: RefCell<Option<Callback>>,
    /// Callback invoked when the user explicitly dismisses the alert.
    pub dismissed_callback: RefCell<Option<Callback>>,
    /// Throttles repeated warning announcements when notification renders repeat.
    pub alert_announcement_throttler: accessibility::AnnouncementThrottler,
}

impl Default for LushtextInfoBar {
    fn default() -> Self {
        Self {
            alert_revealer: TemplateChild::default(),
            alert_box: TemplateChild::default(),
            content_wrap: TemplateChild::default(),
            alert_title: TemplateChild::default(),
            alert_body: TemplateChild::default(),
            actions_box: TemplateChild::default(),
            retry_button: TemplateChild::default(),
            discard_button: TemplateChild::default(),
            save_button: TemplateChild::default(),
            dismiss_button: TemplateChild::default(),
            retry_callback: RefCell::new(None),
            save_callback: RefCell::new(None),
            discard_callback: RefCell::new(None),
            dismissed_callback: RefCell::new(None),
            alert_announcement_throttler: accessibility::AnnouncementThrottler::default(),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextInfoBar {
    const NAME: &str = "LushtextInfoBar";
    type Type = super::LushtextInfoBar;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        // The template hosts the message and action group inside an AdwWrapBox so
        // the toolkit drops the trailing action cluster onto its own row beneath
        // the message when the editor column is too narrow. Register the
        // libadwaita type before binding the template so the builder can
        // instantiate `AdwWrapBox`.
        libadwaita::WrapBox::ensure_type();
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextInfoBar {
    fn constructed(&self) {
        self.parent_constructed();

        self.obj().set_visible(false);
        // A container with role Alert can become a leaf in AT-SPI, hiding the
        // retry/dismiss buttons. Keep the row grouped and put Alert on the
        // message label that actually needs high-priority announcement.
        accessibility::set_role(&*self.alert_box, gtk4::AccessibleRole::Group);
        accessibility::set_role(&*self.alert_title, gtk4::AccessibleRole::Alert);

        // GNOME Text Editor wraps its inline alert action labels so restored-file
        // banners stay readable on narrow windows. LushText follows the same
        // pattern instead of hiding actions behind a larger window minimum.
        wrap_button_label(&self.retry_button);
        wrap_button_label(&self.discard_button);
        wrap_button_label(&self.save_button);

        // GObject signals can outlive the Rust stack frame that installed them,
        // so closures keep only weak references back to the wrapper object.
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

        {
            let obj_weak = self.obj().downgrade();
            self.dismiss_button.connect_clicked(move |_| {
                if let Some(obj) = obj_weak.upgrade() {
                    obj.render_notification(None);
                    if let Some(ref cb) = *obj.imp().dismissed_callback.borrow() {
                        cb();
                    }
                }
            });
        }
    }
}

impl WidgetImpl for LushtextInfoBar {}
impl BoxImpl for LushtextInfoBar {}
