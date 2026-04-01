// SPDX-License-Identifier: GPL-3.0-or-later

use libadwaita::subclass::prelude::*;
use gtk4::{self, glib, CompositeTemplate};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/preferences.ui")]
pub struct LushtextPreferences {
    #[template_child]
    pub style_scheme_row: TemplateChild<libadwaita::ComboRow>,
    #[template_child]
    pub use_system_font_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub tab_width_row: TemplateChild<libadwaita::SpinRow>,
    #[template_child]
    pub insert_spaces_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub show_line_numbers_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub highlight_line_row: TemplateChild<libadwaita::SwitchRow>,
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextPreferences {
    const NAME: &'static str = "LushtextPreferences";
    type Type = super::LushtextPreferences;
    type ParentType = libadwaita::PreferencesDialog;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextPreferences {}
impl WidgetImpl for LushtextPreferences {}
impl AdwDialogImpl for LushtextPreferences {}
impl PreferencesDialogImpl for LushtextPreferences {}
