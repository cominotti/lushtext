// SPDX-License-Identifier: GPL-3.0-or-later

use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, glib};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/status-bar.ui")]
pub struct LushtextStatusBar {
    #[template_child]
    pub sidebar_toggle_button: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub properties_toggle_button: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub message_label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub metadata_box: TemplateChild<gtk4::Box>,
    #[template_child]
    pub editorconfig_label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub encoding_label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub file_size_label: TemplateChild<gtk4::Label>,
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
