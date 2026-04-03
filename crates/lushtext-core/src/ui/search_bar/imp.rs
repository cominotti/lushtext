// SPDX-License-Identifier: GPL-3.0-or-later

use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, glib};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/search-bar.ui")]
pub struct LushtextSearchBar {
    #[template_child]
    pub search_entry: TemplateChild<gtk4::SearchEntry>,
    #[template_child]
    pub replace_entry: TemplateChild<gtk4::Entry>,
    #[template_child]
    pub match_label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub prev_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub next_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub close_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub replace_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub replace_all_button: TemplateChild<gtk4::Button>,
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextSearchBar {
    const NAME: &'static str = "LushtextSearchBar";
    type Type = super::LushtextSearchBar;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextSearchBar {}
impl WidgetImpl for LushtextSearchBar {}
impl BoxImpl for LushtextSearchBar {}
