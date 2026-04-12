// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the preferences dialog.
//!
//! Binds GSettings keys to Adwaita preference rows (switches, combos, spin)
//! using two-way `Settings::bind()`. The color scheme row and font button
//! require manual wiring because their value types don't map directly to
//! GSettings string/bool keys.

use crate::config::keys;
use gtk4::{self, CompositeTemplate, gio, glib};
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;

#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/preferences.ui")]
pub struct LushtextPreferences {
    #[template_child]
    pub style_scheme_row: TemplateChild<libadwaita::ComboRow>,
    #[template_child]
    pub use_system_font_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub custom_font_row: TemplateChild<libadwaita::ActionRow>,
    #[template_child]
    pub font_button: TemplateChild<gtk4::FontDialogButton>,
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
    #[template_child]
    pub workspace_auto_collapse_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub workspace_empty_folder_lookahead_cap_row: TemplateChild<libadwaita::SpinRow>,

    pub settings: gio::Settings,
}

impl Default for LushtextPreferences {
    fn default() -> Self {
        Self {
            style_scheme_row: TemplateChild::default(),
            editorconfig_row: TemplateChild::default(),
            use_system_font_row: TemplateChild::default(),
            custom_font_row: TemplateChild::default(),
            font_button: TemplateChild::default(),
            word_wrap_row: TemplateChild::default(),
            tab_width_row: TemplateChild::default(),
            insert_spaces_row: TemplateChild::default(),
            show_line_numbers_row: TemplateChild::default(),
            highlight_line_row: TemplateChild::default(),
            workspace_auto_collapse_row: TemplateChild::default(),
            workspace_empty_folder_lookahead_cap_row: TemplateChild::default(),
            settings: gio::Settings::new(crate::config::APP_ID),
        }
    }
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

impl ObjectImpl for LushtextPreferences {
    fn constructed(&self) {
        self.parent_constructed();

        let s = &self.settings;

        // GSettings bind() creates a live two-way sync between settings keys
        // and widget properties. DEFAULT flags (the default) means changes to
        // either side automatically propagate to the other.
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
        s.bind(keys::USE_SYSTEM_FONT, &*self.use_system_font_row, "active")
            .build();
        s.bind(keys::TAB_WIDTH, &self.tab_width_row.adjustment(), "value")
            .build();
        s.bind(
            keys::WORKSPACE_AUTO_COLLAPSE,
            &*self.workspace_auto_collapse_row,
            "active",
        )
        .build();
        s.bind(
            keys::WORKSPACE_EMPTY_FOLDER_LOOKAHEAD_CAP,
            &self.workspace_empty_folder_lookahead_cap_row.adjustment(),
            "value",
        )
        .build();

        s.bind(keys::USE_SYSTEM_FONT, &*self.custom_font_row, "sensitive")
            .flags(gio::SettingsBindFlags::GET | gio::SettingsBindFlags::INVERT_BOOLEAN)
            .build();

        self.setup_color_scheme_row();
        self.setup_font_button();
    }
}

impl LushtextPreferences {
    fn setup_color_scheme_row(&self) {
        let scheme_manager = sourceview5::StyleSchemeManager::default();
        let model = gtk4::StringList::new(&[]);

        // Collect base scheme IDs only; dark variants (e.g., "Adwaita-dark")
        // are selected automatically based on StyleManager::is_dark().
        let scheme_ids: Vec<String> = scheme_manager
            .scheme_ids()
            .iter()
            .filter(|id| !id.ends_with("-dark"))
            .map(|id| id.to_string())
            .collect();

        for id in &scheme_ids {
            if let Some(scheme) = scheme_manager.scheme(id) {
                model.append(&scheme.name());
            }
        }

        self.style_scheme_row.set_model(Some(&model));

        let current = self.settings.string(keys::STYLE_SCHEME);
        let selected_pos = scheme_ids
            .iter()
            .position(|id| id == current.as_str())
            .unwrap_or(0) as u32;
        self.style_scheme_row.set_selected(selected_pos);

        let settings = self.settings.clone();
        self.style_scheme_row.connect_selected_notify(move |row| {
            let pos = row.selected() as usize;
            if pos < scheme_ids.len() {
                let _ = settings.set_string(keys::STYLE_SCHEME, &scheme_ids[pos]);
            }
        });
    }

    fn setup_font_button(&self) {
        self.font_button
            .set_dialog(&gtk4::FontDialog::builder().build());

        let current = self.settings.string(keys::CUSTOM_FONT);
        let desc = pango::FontDescription::from_string(&current);
        self.font_button.set_font_desc(&desc);

        let settings = self.settings.clone();
        self.font_button.connect_font_desc_notify(move |btn| {
            if let Some(desc) = btn.font_desc() {
                let _ = settings.set_string(keys::CUSTOM_FONT, &desc.to_string());
            }
        });
    }
}

impl WidgetImpl for LushtextPreferences {}
impl AdwDialogImpl for LushtextPreferences {}
impl PreferencesDialogImpl for LushtextPreferences {}
