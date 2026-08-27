// SPDX-License-Identifier: GPL-3.0-or-later

//! Called presentation surface: the header-bar Notes menu.
//!
//! This module carries **no role**. It only projects the notes workflow onto the
//! `GtkMenuButton`'s menu model and the four `notes-*` menu-only actions, so
//! under `gtk-adapter-module-boundaries` it is a called presentation surface
//! rather than one of the five role modules: it owns no pure policy and no
//! evidence surface, and the workflow's `policy.rs` decides the bookmark row's
//! label. It is named in the `WFR-NOTES-BOOKMARKS` matrix row.
//!
//! Behavior obligation preserved from `menu-workflow-coverage`: the menu model is
//! replaced only when the bookmark label actually changes, because replacing a
//! `GtkMenuButton`'s model from a popup path cancels the visible popover.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;

use super::LushtextWindow;
use super::policy;

impl LushtextWindow {
    /// Refresh the window-scoped Notes menu label and menu-only action state.
    ///
    /// The header button and `Browse Notes…` stay window-scoped so the browser
    /// can show workspace rows, open-tab rows, or its empty state even when no
    /// editor tab is active. Target-specific rows still use sensitivity below.
    ///
    /// The dedicated menu uses its own `notes-*` actions so it can become
    /// insensitive without disabling the existing shortcuts or command-palette
    /// commands that still rely on the workflow guards below.
    pub(in crate::ui::window) fn refresh_notes_menu_state(&self) {
        let active_editor = self.active_editor();
        let saved_editor = active_editor
            .as_ref()
            .filter(|editor| editor.file_path().is_some());
        let bookmark_label = policy::bookmark_menu_label(
            saved_editor
                .as_ref()
                .is_some_and(|editor| editor.current_bookmark().is_some()),
        );

        if !self.notes_menu_uses_bookmark_label(bookmark_label) {
            self.rebuild_notes_menu(bookmark_label);
        }

        self.imp().notes_menu_button.set_visible(true);

        self.set_notes_menu_action_enabled("notes-toggle-bookmark", saved_editor.is_some());
        self.set_notes_menu_action_enabled("notes-open-document-note", saved_editor.is_some());
        self.set_notes_menu_action_enabled(
            "notes-open-folder-note",
            self.current_folder_note_action_available(),
        );
        self.set_notes_menu_action_enabled("notes-show-notes", true);
    }

    /// Check the existing menu model before replacing it during ordinary state refreshes.
    ///
    /// The menu is small, and avoiding no-op replacements keeps GTK's popup
    /// lifecycle stable if a refresh races with user activation.
    fn notes_menu_uses_bookmark_label(&self, bookmark_label: &'static str) -> bool {
        let Some(menu) = self.imp().notes_menu_button.menu_model() else {
            return false;
        };

        Self::menu_label_for_action(&menu, "win.notes-toggle-bookmark")
            .is_some_and(|label| label == bookmark_label)
    }

    /// Find the label for one action in a possibly sectioned menu model.
    ///
    /// Searching by action keeps the bookmark-label guard independent from the
    /// visual section order, which is allowed to change as the menu evolves.
    fn menu_label_for_action(model: &gio::MenuModel, action_name: &str) -> Option<String> {
        for index in 0..model.n_items() {
            let action = model
                .item_attribute_value(index, "action", Some(glib::VariantTy::STRING))
                .and_then(|variant| variant.get::<String>());
            if action.as_deref() == Some(action_name) {
                return model
                    .item_attribute_value(index, "label", Some(glib::VariantTy::STRING))
                    .and_then(|variant| variant.get::<String>());
            }

            for link_name in ["section", "submenu"] {
                if let Some(link) = model.item_link(index, link_name)
                    && let Some(label) = Self::menu_label_for_action(&link, action_name)
                {
                    return Some(label);
                }
            }
        }
        None
    }

    /// Rebuild the small header-bar Notes menu so its bookmark row can use
    /// the active cursor context without disabling the expert command actions.
    fn rebuild_notes_menu(&self, bookmark_label: &'static str) {
        let menu = gio::Menu::new();

        let browse_section = gio::Menu::new();
        browse_section.append(Some("Browse Notes…"), Some("win.notes-show-notes"));
        menu.append_section(None, &browse_section);

        let document_section = gio::Menu::new();
        document_section.append(Some(bookmark_label), Some("win.notes-toggle-bookmark"));
        document_section.append(
            Some("Open Document Note…"),
            Some("win.notes-open-document-note"),
        );
        menu.append_section(None, &document_section);

        let workspace_section = gio::Menu::new();
        workspace_section.append(
            Some("Open Folder Note…"),
            Some("win.notes-open-folder-note"),
        );
        menu.append_section(None, &workspace_section);

        self.imp().notes_menu_button.set_menu_model(Some(&menu));
    }

    /// Update one Notes-menu-only action without affecting shortcut actions.
    fn set_notes_menu_action_enabled(&self, action_name: &str, enabled: bool) {
        if let Some(action) = self.lookup_action(action_name)
            && let Some(simple) = action.downcast_ref::<gio::SimpleAction>()
        {
            simple.set_enabled(enabled);
        }
    }
}
