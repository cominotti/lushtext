// SPDX-License-Identifier: GPL-3.0-or-later

//! Appearance preference rows: style scheme, sidebar width, custom font, and opacity.

use glib::subclass::prelude::ObjectSubclassIsExt;
use glib::value::ToValue;
use gtk4::{self, glib};
use libadwaita::prelude::*;

use crate::config::keys;
use crate::ui::sidebar::width_preset::WorkspaceSidebarWidthPreset;

use super::LushtextPreferences;

impl LushtextPreferences {
    /// Wire the appearance rows that translate between GSettings and widget models.
    pub(super) fn setup_appearance(&self) {
        self.setup_color_scheme_row();
        self.setup_workspace_sidebar_width_row();
        self.setup_font_button();
        self.setup_transparency_row();
    }

    /// Keep the workspace width preference aligned with the three named shell presets
    /// instead of exposing the raw GSettings backing value to users.
    fn setup_workspace_sidebar_width_row(&self) {
        let imp = self.imp();
        let model = gtk4::StringList::new(&[]);
        for preset in WorkspaceSidebarWidthPreset::ALL {
            model.append(preset.label());
        }

        imp.workspace_sidebar_width_row.set_model(Some(&model));

        let current = WorkspaceSidebarWidthPreset::from_fraction(
            imp.settings.double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION),
        );
        imp.workspace_sidebar_width_row
            .set_selected(current.index());

        let settings = imp.settings.clone();
        imp.workspace_sidebar_width_row
            .connect_selected_notify(move |row| {
                let Some(preset) = WorkspaceSidebarWidthPreset::from_index(row.selected()) else {
                    return;
                };
                if (settings.double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION) - preset.fraction())
                    .abs()
                    > f64::EPSILON
                {
                    crate::ui::settings_writes::set_double(
                        keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION,
                        preset.fraction(),
                    );
                }
            });
    }

    fn setup_color_scheme_row(&self) {
        let imp = self.imp();
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

        imp.style_scheme_row.set_model(Some(&model));

        let current = imp.settings.string(keys::STYLE_SCHEME);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "The style-scheme dropdown cannot approach u32::MAX entries in practice"
        )]
        let selected_pos = scheme_ids
            .iter()
            .position(|id| id == current.as_str())
            .unwrap_or(0) as u32;
        imp.style_scheme_row.set_selected(selected_pos);

        imp.style_scheme_row.connect_selected_notify(move |row| {
            let pos = row.selected() as usize;
            if pos < scheme_ids.len() {
                crate::ui::settings_writes::set_string(keys::STYLE_SCHEME, &scheme_ids[pos]);
            }
        });
    }

    fn setup_font_button(&self) {
        let imp = self.imp();
        imp.font_button
            .set_dialog(&gtk4::FontDialog::builder().build());

        let current = imp.settings.string(keys::CUSTOM_FONT);
        let desc = gtk4::pango::FontDescription::from_string(&current);
        imp.font_button.set_font_desc(&desc);

        imp.font_button.connect_font_desc_notify(move |btn| {
            if let Some(desc) = btn.font_desc() {
                crate::ui::settings_writes::set_string(keys::CUSTOM_FONT, &desc.to_string());
            }
        });
    }

    /// Mirror the transparency control with a percentage label
    /// while keeping the slider value persisted through GSettings.
    fn setup_transparency_row(&self) {
        let imp = self.imp();
        // This is one-way UI projection: the adjustment value formats into the
        // label text, and `sync_create()` replaces the old explicit initial
        // label update.
        imp.transparency_adjustment
            .bind_property("value", &*imp.transparency_label, "label")
            .transform_to(|_: &glib::Binding, value: &glib::Value| {
                let opacity = value.get::<f64>().ok()?;
                Some(transparency_label_text(opacity).to_value())
            })
            .sync_create()
            .build();
        let prefs_weak = self.downgrade();
        imp.transparency_adjustment.connect_value_changed(move |_| {
            if let Some(prefs) = prefs_weak.upgrade() {
                prefs.refresh_data_accessibility_state();
            }
        });
    }
}

/// Format one stored opacity value as a whole-percent label for the row suffix.
fn transparency_label_text(opacity: f64) -> String {
    format!("{:>3.0}%", (opacity.clamp(0.0, 1.0) * 100.0).floor())
}
