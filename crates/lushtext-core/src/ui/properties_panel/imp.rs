// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the properties panel widget.
//!
//! Mirrors the existing preferences dialog bindings so the sidebar can expose
//! the same editor controls without duplicating settings state elsewhere.

use crate::config::keys;
use gtk4::{self, CompositeTemplate, gio, glib};
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;

#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/properties-panel.ui")]
pub struct LushtextPropertiesPanel {
    /// Full path for the active document, or an empty-state message when no
    /// file-backed editor is selected.
    #[template_child]
    pub path_row: TemplateChild<libadwaita::ActionRow>,
    /// Current encoding. Today LushText only opens UTF-8 text files, so this
    /// row is mostly an explicit confirmation for the user.
    #[template_child]
    pub encoding_row: TemplateChild<libadwaita::ActionRow>,
    /// On-disk size populated after async file load finishes.
    #[template_child]
    pub file_size_row: TemplateChild<libadwaita::ActionRow>,
    /// Whether formatting is coming from raw preferences or an EditorConfig
    /// override for the active file.
    #[template_child]
    pub formatting_source_row: TemplateChild<libadwaita::ActionRow>,
    /// Global formatting controls reused from the preferences dialog.
    #[template_child]
    pub editorconfig_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub word_wrap_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub tab_width_row: TemplateChild<libadwaita::SpinRow>,
    #[template_child]
    pub insert_spaces_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub show_line_numbers_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub highlight_line_row: TemplateChild<libadwaita::SwitchRow>,
    /// Shared application settings backing both the preferences dialog and
    /// this lightweight sidebar surface.
    pub settings: gio::Settings,
}

impl Default for LushtextPropertiesPanel {
    fn default() -> Self {
        Self {
            path_row: TemplateChild::default(),
            encoding_row: TemplateChild::default(),
            file_size_row: TemplateChild::default(),
            formatting_source_row: TemplateChild::default(),
            editorconfig_row: TemplateChild::default(),
            word_wrap_row: TemplateChild::default(),
            tab_width_row: TemplateChild::default(),
            insert_spaces_row: TemplateChild::default(),
            show_line_numbers_row: TemplateChild::default(),
            highlight_line_row: TemplateChild::default(),
            settings: gio::Settings::new(crate::config::APP_ID),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextPropertiesPanel {
    const NAME: &'static str = "LushtextPropertiesPanel";
    type Type = super::LushtextPropertiesPanel;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextPropertiesPanel {
    fn constructed(&self) {
        self.parent_constructed();

        let s = &self.settings;
        s.bind(keys::USE_EDITORCONFIG, &*self.editorconfig_row, "active")
            .build();
        s.bind(keys::WORD_WRAP, &*self.word_wrap_row, "active")
            .build();
        s.bind(
            keys::SHOW_LINE_NUMBERS,
            &*self.show_line_numbers_row,
            "active",
        )
        .build();
        s.bind(
            keys::HIGHLIGHT_CURRENT_LINE,
            &*self.highlight_line_row,
            "active",
        )
        .build();
        s.bind(keys::INSERT_SPACES, &*self.insert_spaces_row, "active")
            .build();
        s.bind(keys::TAB_WIDTH, &self.tab_width_row.adjustment(), "value")
            .build();
    }
}

impl WidgetImpl for LushtextPropertiesPanel {}
impl BoxImpl for LushtextPropertiesPanel {}
