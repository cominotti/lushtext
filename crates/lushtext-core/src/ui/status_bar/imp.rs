// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the bottom status bar widget.
//!
//! The widget keeps the left workspace toggle fixed while the metadata cluster
//! stays limited to glanceable, document-local state.

use std::cell::Cell;

use crate::ui::accessibility::{self, AnnouncementThrottler};
use gtk_lush_settle::SupersedingTimer;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, glib};

/// Template-backed private state for the status bar GObject subclass.
///
/// `CompositeTemplate` loads the XML from the compiled GResource, and each
/// `#[template_child]` field binds to the matching widget id in that template.
#[derive(Default, CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/status-bar.ui")]
pub struct LushtextStatusBar {
    /// Fixed left toggle for the workspace sidebar.
    #[template_child]
    pub sidebar_toggle_button: TemplateChild<gtk4::ToggleButton>,
    /// Full-width lane reserved for transient and progress status messages.
    ///
    /// The pulse background lives here instead of on the label so repeated
    /// notifications acknowledge the whole available message area, including
    /// empty space to the right of short messages.
    #[template_child]
    pub message_area_box: TemplateChild<gtk4::Box>,
    /// Text label inside the full-width status-message lane.
    ///
    /// Steady severity text classes live here, while flash/pulse classes live
    /// on `message_area_box` so the empty part of the lane is highlighted too.
    #[template_child]
    pub message_label: TemplateChild<gtk4::Label>,
    /// Container for the document metadata cluster. Hidden when no tab is active.
    #[template_child]
    pub metadata_box: TemplateChild<gtk4::Box>,
    /// Badge indicating when EditorConfig overrides are active.
    #[template_child]
    pub editorconfig_label: TemplateChild<gtk4::Label>,
    /// Visual divider shown with the EditorConfig badge so document metadata
    /// controls remain grouped when formatting overrides are active.
    #[template_child]
    pub editorconfig_separator: TemplateChild<gtk4::Separator>,
    /// Line-ending entry point for the active document.
    #[template_child]
    pub line_ending_button: TemplateChild<gtk4::Button>,
    /// Encoding entry point for the active document.
    #[template_child]
    pub encoding_button: TemplateChild<gtk4::Button>,
    /// Superseding cleanup timer for delayed pulse-class removal.
    pub pulse_cleanup_timer: SupersedingTimer,
    /// Throttles repeated screen-reader status announcements from this bar.
    pub status_announcement_throttler: AnnouncementThrottler,
    /// Alternates otherwise equivalent pulse classes so rapid repeated messages
    /// restart GTK's CSS animation even when severity and text are unchanged.
    pub pulse_alt: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextStatusBar {
    const NAME: &'static str = "LushtextStatusBar";
    type Type = super::LushtextStatusBar;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextStatusBar {
    fn constructed(&self) {
        self.parent_constructed();

        accessibility::set_label(&*self.sidebar_toggle_button, "Toggle workspace sidebar");
        accessibility::set_pressed(&*self.sidebar_toggle_button, false);
        accessibility::set_labelled_description(
            &*self.message_label,
            "Status message",
            "Current editor status and feedback",
        );
        accessibility::set_role(&*self.metadata_box, gtk4::AccessibleRole::Group);
        accessibility::set_labelled_description(
            &*self.metadata_box,
            "Document metadata",
            "Line ending and text encoding controls for the active document",
        );
        accessibility::set_labelled_description(
            &*self.line_ending_button,
            "Choose line endings",
            "Current line endings for the active document",
        );
        accessibility::set_value_text(&*self.line_ending_button, "LF");
        accessibility::set_labelled_description(
            &*self.encoding_button,
            "Choose text encoding",
            "Current text encoding for the active document",
        );
        accessibility::set_value_text(&*self.encoding_button, "UTF-8");
    }
}
impl WidgetImpl for LushtextStatusBar {}
impl BoxImpl for LushtextStatusBar {}
