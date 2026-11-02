// SPDX-License-Identifier: GPL-3.0-or-later

//! The Protected View menu (`protected-view-inspector-and-surfaces` task 2.1,
//! design D3 step 3): a **called presentation surface** of the Protected View
//! workflow, with no role. A secondary click, Shift+F10 or the Menu key opens
//! it instead of GtkTextView's default context menu (which offers Select All).
//! It offers Inspect Line, Copy Whole Line, Find and, while available, Undo
//! Edit (the entry undo); Cut, Paste and Select All are shown insensitive so
//! the user sees what the view-only document does not allow.

use gtk4::prelude::*;

use crate::ui::editor_page::LushtextEditorPage;

/// The menu's private action group on the source view.
const MENU_GROUP: &str = "protected";

/// The menu's accessible name.
const PROTECTED_MENU_LABEL: &str = "Protected View menu";

impl LushtextEditorPage {
    /// Open the menu at a secondary click at `(x, y)` in the view's
    /// coordinates, for the paragraph there.
    pub(in crate::ui::editor_page) fn open_protected_menu_at(&self, x: f64, y: f64) {
        let view = self.source_view();
        let paragraph = self
            .paragraph_at_widget_y(view.upcast_ref(), y)
            .or_else(|| self.first_folded_paragraph_in_viewport());
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a pointer position rounded to the popover's integer pointing rectangle"
        )]
        let point = gtk4::gdk::Rectangle::new(x.round() as i32, y.round() as i32, 1, 1);
        self.present_protected_menu(paragraph, point);
    }

    /// Open the menu from Shift+F10 or the Menu key, for the paragraph the
    /// keyboard Inspect Line would choose, near the top of the viewport.
    pub(in crate::ui::editor_page) fn open_protected_menu_from_keyboard(&self) {
        let view = self.source_view();
        let paragraph = self.first_folded_paragraph_in_viewport();
        let point = gtk4::gdk::Rectangle::new(view.width() / 4, view.height() / 4, 1, 1);
        self.present_protected_menu(paragraph, point);
    }

    fn present_protected_menu(&self, paragraph: Option<u64>, point: gtk4::gdk::Rectangle) {
        let view = self.source_view();
        self.install_protected_menu_actions(paragraph);
        let menu = gio::Menu::new();
        let reading = gio::Menu::new();
        reading.append(Some("_Inspect Line"), Some("protected.inspect-line"));
        reading.append(Some("_Copy Whole Line"), Some("protected.copy-whole-line"));
        reading.append(Some("_Find…"), Some("win.begin-search"));
        menu.append_section(None, &reading);
        let editing = gio::Menu::new();
        editing.append(Some("_Undo Edit"), Some("protected.undo-entry"));
        editing.append(Some("Cu_t"), Some("protected.unavailable"));
        editing.append(Some("_Paste"), Some("protected.unavailable"));
        editing.append(Some("Select _All"), Some("protected.unavailable"));
        menu.append_section(None, &editing);
        let popover = gtk4::PopoverMenu::from_model(Some(&menu));
        popover.set_has_arrow(false);
        crate::ui::accessibility::set_label(&popover, PROTECTED_MENU_LABEL);
        popover.set_parent(view);
        popover.set_pointing_to(Some(&point));
        popover.connect_closed(|popover| {
            // Unparent after the popdown finishes, not from inside it.
            let popover = popover.clone();
            glib::idle_add_local_once(move || popover.unparent());
        });
        popover.popup();
    }

    /// The menu's actions, bound to `paragraph`: replaced on every popup so
    /// they always act on the paragraph the menu was opened for.
    fn install_protected_menu_actions(&self, paragraph: Option<u64>) {
        let view = self.source_view();
        let group = gio::SimpleActionGroup::new();
        let inspect = gio::SimpleAction::new("inspect-line", None);
        let copy = gio::SimpleAction::new("copy-whole-line", None);
        inspect.set_enabled(paragraph.is_some());
        copy.set_enabled(paragraph.is_some() && self.protected_source().is_some());
        if let Some(paragraph) = paragraph {
            let page = self.downgrade();
            inspect.connect_activate(move |_, _| {
                if let Some(page) = page.upgrade() {
                    page.open_line_inspector(paragraph, 0);
                }
            });
            let page = self.downgrade();
            copy.connect_activate(move |_, _| {
                if let Some(page) = page.upgrade() {
                    page.copy_protected_whole_line(paragraph);
                }
            });
        }
        let undo = gio::SimpleAction::new("undo-entry", None);
        undo.set_enabled(self.protected_entry_undo_available());
        let page = self.downgrade();
        undo.connect_activate(move |_, _| {
            if let Some(page) = page.upgrade() {
                let _ = page.undo_protected_entry();
            }
        });
        let unavailable = gio::SimpleAction::new("unavailable", None);
        unavailable.set_enabled(false);
        for action in [&inspect, &copy, &undo, &unavailable] {
            group.add_action(action);
        }
        view.insert_action_group(MENU_GROUP, Some(&group));
    }
}
