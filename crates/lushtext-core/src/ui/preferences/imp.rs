// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the preferences dialog.
//!
//! Binds GSettings keys to Adwaita preference rows (switches, combos, spin)
//! using two-way `Settings::bind()`. The color scheme row and font button
//! require manual wiring because their value types don't map directly to
//! GSettings string/bool keys, and the transparency control formats a double
//! setting into the percentage label shown in its row suffix.

use crate::config::keys;
use crate::ui::sidebar::WorkspaceSidebarWidthPreset;
use glib::value::ToValue;
use gtk4::{self, CompositeTemplate, gio, glib};
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;

#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/preferences.ui")]
pub struct LushtextPreferences {
    #[template_child]
    pub style_scheme_row: TemplateChild<libadwaita::ComboRow>,
    #[template_child]
    pub workspace_sidebar_width_row: TemplateChild<libadwaita::ComboRow>,
    #[template_child]
    pub use_system_font_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub custom_font_row: TemplateChild<libadwaita::ActionRow>,
    #[template_child]
    pub font_button: TemplateChild<gtk4::FontDialogButton>,
    #[template_child]
    pub transparency_row: TemplateChild<libadwaita::ActionRow>,
    #[template_child]
    pub transparency_button: TemplateChild<gtk4::MenuButton>,
    /// Percentage suffix for the tab-content opacity slider.
    #[template_child]
    pub transparency_label: TemplateChild<gtk4::Label>,
    /// Slider adjustment for the persisted opacity value and visible percentage projection.
    #[template_child]
    pub transparency_adjustment: TemplateChild<gtk4::Adjustment>,
    #[template_child]
    pub focus_mode_target_columns_row: TemplateChild<libadwaita::SpinRow>,
    #[template_child]
    pub focus_mode_typewriter_scrolling_row: TemplateChild<libadwaita::SwitchRow>,
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
    pub minimap_group: TemplateChild<libadwaita::PreferencesGroup>,
    #[template_child]
    pub show_minimap_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub minimap_long_line_markers_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub bookmark_gutter_row: TemplateChild<libadwaita::SwitchRow>,
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
            workspace_sidebar_width_row: TemplateChild::default(),
            editorconfig_row: TemplateChild::default(),
            use_system_font_row: TemplateChild::default(),
            custom_font_row: TemplateChild::default(),
            font_button: TemplateChild::default(),
            transparency_row: TemplateChild::default(),
            transparency_button: TemplateChild::default(),
            transparency_label: TemplateChild::default(),
            transparency_adjustment: TemplateChild::default(),
            focus_mode_target_columns_row: TemplateChild::default(),
            focus_mode_typewriter_scrolling_row: TemplateChild::default(),
            word_wrap_row: TemplateChild::default(),
            tab_width_row: TemplateChild::default(),
            insert_spaces_row: TemplateChild::default(),
            show_line_numbers_row: TemplateChild::default(),
            highlight_line_row: TemplateChild::default(),
            minimap_group: TemplateChild::default(),
            show_minimap_row: TemplateChild::default(),
            minimap_long_line_markers_row: TemplateChild::default(),
            bookmark_gutter_row: TemplateChild::default(),
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
        s.bind(keys::SHOW_MINIMAP, &*self.show_minimap_row, "active")
            .build();
        s.bind(
            keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE,
            &*self.minimap_long_line_markers_row,
            "active",
        )
        .build();
        s.bind(
            keys::BOOKMARK_GUTTER_VISIBLE,
            &*self.bookmark_gutter_row,
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
        s.bind(
            keys::TAB_CONTENT_OPACITY,
            &*self.transparency_adjustment,
            "value",
        )
        .build();
        s.bind(
            keys::FOCUS_MODE_TARGET_COLUMNS,
            &self.focus_mode_target_columns_row.adjustment(),
            "value",
        )
        .build();
        s.bind(
            keys::FOCUS_MODE_TYPEWRITER_SCROLLING,
            &*self.focus_mode_typewriter_scrolling_row,
            "active",
        )
        .build();

        self.setup_color_scheme_row();
        self.setup_workspace_sidebar_width_row();
        self.setup_font_button();
        self.setup_transparency_row();
        self.apply_accessibility_metadata();
    }
}

impl LushtextPreferences {
    /// Keep numeric Adwaita preference rows discoverable as composite groups.
    /// Their internal child owns the `SpinButton` role, so the row itself must
    /// avoid the weaker presentation role that hides the control grouping.
    fn apply_accessibility_metadata(&self) {
        self.tab_width_row
            .set_accessible_role(gtk4::AccessibleRole::Group);
        self.focus_mode_target_columns_row
            .set_accessible_role(gtk4::AccessibleRole::Group);
        self.workspace_empty_folder_lookahead_cap_row
            .set_accessible_role(gtk4::AccessibleRole::Group);
    }

    /// Keep the workspace width preference aligned with the three named shell presets
    /// instead of exposing the raw GSettings backing value to users.
    fn setup_workspace_sidebar_width_row(&self) {
        let model = gtk4::StringList::new(&[]);
        for preset in WorkspaceSidebarWidthPreset::ALL {
            model.append(preset.label());
        }

        self.workspace_sidebar_width_row.set_model(Some(&model));

        let current = WorkspaceSidebarWidthPreset::from_fraction(
            self.settings.double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION),
        );
        self.workspace_sidebar_width_row
            .set_selected(current.index());

        let settings = self.settings.clone();
        self.workspace_sidebar_width_row
            .connect_selected_notify(move |row| {
                let Some(preset) = WorkspaceSidebarWidthPreset::from_index(row.selected()) else {
                    return;
                };
                if (settings.double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION) - preset.fraction())
                    .abs()
                    > f64::EPSILON
                {
                    let _ = settings
                        .set_double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION, preset.fraction());
                }
            });
    }

    fn setup_color_scheme_row(&self) {
        let scheme_manager = sourceview5::StyleSchemeManager::default();
        let model = gtk4::StringList::new(&[]);

        // Collect base scheme IDs only; dark variants (e.g., "Adwaita-dark")
        // are selected automatically based on StyleManager::is_dark().
        let scheme_ids: Vec<String> = scheme_manager
            .scheme_ids()
            .iter()
            .filter(|id| !id.ends_with("-dark"))
            .map(std::string::ToString::to_string)
            .collect();

        for id in &scheme_ids {
            if let Some(scheme) = scheme_manager.scheme(id) {
                model.append(&scheme.name());
            }
        }

        self.style_scheme_row.set_model(Some(&model));

        let current = self.settings.string(keys::STYLE_SCHEME);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "The style-scheme dropdown cannot approach u32::MAX entries in practice"
        )]
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

    /// Mirror the Fedora-style transparency control with a percentage label
    /// while keeping the slider value persisted through GSettings.
    fn setup_transparency_row(&self) {
        // This is one-way UI projection: the adjustment value formats into the
        // label text, and `sync_create()` replaces the old explicit initial
        // label update.
        self.transparency_adjustment
            .bind_property("value", &*self.transparency_label, "label")
            .transform_to(|_: &glib::Binding, value: &glib::Value| {
                let opacity = value.get::<f64>().ok()?;
                Some(transparency_label_text(opacity).to_value())
            })
            .sync_create()
            .build();
    }
}

impl WidgetImpl for LushtextPreferences {}
impl AdwDialogImpl for LushtextPreferences {}
impl PreferencesDialogImpl for LushtextPreferences {}

/// Format one stored opacity value as a whole-percent label for the row suffix.
fn transparency_label_text(opacity: f64) -> String {
    format!("{:>3.0}%", (opacity.clamp(0.0, 1.0) * 100.0).floor())
}
