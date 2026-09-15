// SPDX-License-Identifier: GPL-3.0-or-later

//! Preferences editor for the always-excluded workspace entry names.
//!
//! The preferences implementation owns the template children; this module owns
//! the row-per-name projection of `workspace-excluded-names`, the add/remove/
//! reset commands, and the accessible error state. The normalization decisions
//! themselves are GTK-free in `model::workspace_visibility`.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use crate::config::keys;
use crate::model::workspace_visibility::{
    ExcludedNameRejection, add_excluded_name, remove_excluded_name,
};
use crate::ui::accessibility;
use crate::ui::workspace_visibility::{excluded_names, set_excluded_names};

use super::LushtextPreferences;

/// Title of the row shown when the excluded list is empty.
pub(super) const EMPTY_EXCLUDED_NAMES_TITLE: &str = "No excluded names";

impl LushtextPreferences {
    /// Wire the excluded-names expander: projection, add, remove, and reset.
    pub(super) fn setup_excluded_names(&self) {
        let imp = self.imp();

        accessibility::set_labelled_description(
            &*imp.workspace_excluded_reset_button,
            "Reset excluded names to defaults",
            "Restore the default always-excluded names",
        );
        accessibility::set_labelled_description(
            &*imp.workspace_excluded_add_row,
            "Add an excluded name",
            "Type an exact file or folder name and press Enter",
        );

        let prefs_weak = self.downgrade();
        imp.workspace_excluded_add_row.connect_apply(move |_| {
            if let Some(prefs) = prefs_weak.upgrade() {
                prefs.submit_excluded_name();
            }
        });
        let prefs_weak = self.downgrade();
        imp.workspace_excluded_add_row.connect_changed(move |_| {
            if let Some(prefs) = prefs_weak.upgrade() {
                prefs.clear_excluded_name_error();
            }
        });
        let prefs_weak = self.downgrade();
        imp.workspace_excluded_reset_button
            .connect_clicked(move |_| {
                // The schema default is the single source of truth; the key's
                // `changed` subscription below re-projects the rows.
                if let Some(prefs) = prefs_weak.upgrade() {
                    prefs.imp().settings.reset(keys::WORKSPACE_EXCLUDED_NAMES);
                }
            });

        // Re-project from the key so dconf edits, automation, and Reset all
        // render through the same path without duplicating rows.
        let prefs_weak = self.downgrade();
        imp.settings
            .connect_changed(Some(keys::WORKSPACE_EXCLUDED_NAMES), move |_, _| {
                if let Some(prefs) = prefs_weak.upgrade() {
                    prefs.project_excluded_names();
                }
            });
        self.project_excluded_names();
    }

    /// Rebuild the name rows from the persisted array.
    ///
    /// The template's add row stays first and is never removed:
    /// `AdwExpanderRow::remove` only accepts rows appended through `add_row`,
    /// so the projection owns and recycles only the rows it appends itself.
    fn project_excluded_names(&self) {
        let imp = self.imp();
        let expander = &*imp.workspace_excluded_names_row;
        for row in imp.workspace_excluded_rows.take() {
            expander.remove(&row);
        }
        if let Some(empty_row) = imp.workspace_excluded_empty_row.take() {
            expander.remove(&empty_row);
        }

        let names = excluded_names(&imp.settings);
        let total = names.len();
        let mut rows = Vec::with_capacity(total);
        for (index, name) in names.iter().enumerate() {
            let row = libadwaita::ActionRow::builder().title(name).build();
            let position = i32::try_from(index + 1).unwrap_or(i32::MAX);
            let total_rows = i32::try_from(total).unwrap_or(i32::MAX);
            accessibility::apply_row_accessibility(
                &row,
                accessibility::RowAccessibility::new(&format!("Excluded name {name}"))
                    .description("Never shown in workspace surfaces")
                    .position(position, total_rows),
            );
            let remove = gtk4::Button::builder()
                .icon_name("list-remove-symbolic")
                .valign(gtk4::Align::Center)
                .css_classes(["flat"])
                .tooltip_text("Remove")
                .build();
            accessibility::set_labelled_description(
                &remove,
                &format!("Remove {name} from excluded names"),
                "Stop hiding entries with this exact name",
            );
            let prefs_weak = self.downgrade();
            let name_for_remove = name.clone();
            remove.connect_clicked(move |_| {
                if let Some(prefs) = prefs_weak.upgrade() {
                    let settings = &prefs.imp().settings;
                    let next = remove_excluded_name(&excluded_names(settings), &name_for_remove);
                    set_excluded_names(settings, &next);
                }
            });
            row.add_suffix(&remove);
            expander.add_row(&row);
            rows.push(row);
        }
        if rows.is_empty() {
            let empty_row = libadwaita::ActionRow::builder()
                .title(EMPTY_EXCLUDED_NAMES_TITLE)
                .subtitle("Every entry follows the hidden-files switch")
                .build();
            accessibility::apply_row_accessibility(
                &empty_row,
                accessibility::RowAccessibility::new(EMPTY_EXCLUDED_NAMES_TITLE)
                    .description("Every entry follows the hidden-files switch"),
            );
            expander.add_row(&empty_row);
            imp.workspace_excluded_empty_row.replace(Some(empty_row));
        }
        imp.workspace_excluded_rows.replace(rows);
        expander.set_subtitle(&excluded_names_subtitle(total));
    }

    /// Validate and persist the typed name, or surface the rejection.
    fn submit_excluded_name(&self) {
        let imp = self.imp();
        let raw = imp.workspace_excluded_add_row.text().to_string();
        let current = excluded_names(&imp.settings);
        match add_excluded_name(&current, &raw) {
            Ok(next) => {
                set_excluded_names(&imp.settings, &next);
                imp.workspace_excluded_add_row.set_text("");
                self.clear_excluded_name_error();
            }
            Err(ExcludedNameRejection::Duplicate) => {
                self.clear_excluded_name_error();
                let trimmed = raw.trim();
                if let Some(row) = imp
                    .workspace_excluded_rows
                    .borrow()
                    .iter()
                    .find(|row| row.title().as_str() == trimmed)
                {
                    row.grab_focus();
                }
            }
            Err(rejection) => self.show_excluded_name_error(rejection),
        }
    }

    fn show_excluded_name_error(&self, rejection: ExcludedNameRejection) {
        let imp = self.imp();
        let message = excluded_name_rejection_message(rejection);
        imp.workspace_excluded_add_row.add_css_class("error");
        accessibility::set_invalid(&*imp.workspace_excluded_add_row, true);
        accessibility::set_description(&*imp.workspace_excluded_add_row, message);
        imp.workspace_excluded_add_row
            .set_tooltip_text(Some(message));
    }

    fn clear_excluded_name_error(&self) {
        let imp = self.imp();
        if !imp.workspace_excluded_add_row.has_css_class("error") {
            return;
        }
        imp.workspace_excluded_add_row.remove_css_class("error");
        accessibility::set_invalid(&*imp.workspace_excluded_add_row, false);
        accessibility::set_description(
            &*imp.workspace_excluded_add_row,
            "Type an exact file or folder name and press Enter",
        );
        imp.workspace_excluded_add_row.set_tooltip_text(None);
    }
}

/// Subtitle for the expander, naming how many names are excluded.
fn excluded_names_subtitle(count: usize) -> String {
    match count {
        0 => "Exact names never shown, even when hidden files are visible".to_owned(),
        1 => "1 name never shown, even when hidden files are visible".to_owned(),
        count => format!("{count} names never shown, even when hidden files are visible"),
    }
}

/// User-facing explanation for a rejected excluded name.
pub(super) const fn excluded_name_rejection_message(
    rejection: ExcludedNameRejection,
) -> &'static str {
    match rejection {
        ExcludedNameRejection::Empty => "Enter a file or folder name",
        ExcludedNameRejection::ContainsSeparator => {
            "Use a single file or folder name without slashes"
        }
        ExcludedNameRejection::Duplicate => "That name is already excluded",
    }
}
