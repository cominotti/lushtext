// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the bottom status bar widget.
//!
//! The widget keeps the left workspace toggle fixed while the metadata cluster
//! stays limited to glanceable, document-local state.

use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, glib};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/status-bar.ui")]
pub struct LushtextStatusBar {
    /// Fixed left toggle for the workspace sidebar.
    #[template_child]
    pub sidebar_toggle_button: TemplateChild<gtk4::ToggleButton>,
    /// Status-message area that stretches across the middle of the bar.
    #[template_child]
    pub message_label: TemplateChild<gtk4::Label>,
    /// Container for the document metadata cluster. Hidden when no tab is active.
    #[template_child]
    pub metadata_box: TemplateChild<gtk4::Box>,
    /// Badge indicating when EditorConfig overrides are active.
    #[template_child]
    pub editorconfig_label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub editorconfig_separator: TemplateChild<gtk4::Separator>,
    /// Line-ending entry point for the active document.
    #[template_child]
    pub line_ending_button: TemplateChild<gtk4::Button>,
    /// Encoding entry point for the active document.
    #[template_child]
    pub encoding_button: TemplateChild<gtk4::Button>,
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

impl ObjectImpl for LushtextStatusBar {}
impl WidgetImpl for LushtextStatusBar {}
impl BoxImpl for LushtextStatusBar {}
