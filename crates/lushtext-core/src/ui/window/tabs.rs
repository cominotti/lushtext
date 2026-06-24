// SPDX-License-Identifier: GPL-3.0-or-later

//! Tab-strip context menu, pinning, bulk close, and reordering workflow.
//!
//! This module keeps the `AdwTabView` tab-management surface in one place so
//! the rest of the window shell can keep treating tab selection and close
//! cleanup as the single source of truth. The tricky parts here are:
//! remembering which tab invoked the context menu, preserving pin state in the
//! session snapshot, and allowing one combined save dialog to authorize a bulk
//! close without reopening one dialog per modified tab.

use std::cmp::Reverse;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;

use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::status_bar::MessageKind;

use super::LushtextWindow;

/// Symbolic icon shown on pinned tabs so the leading segment is explicit.
const PIN_INDICATOR_ICON_NAME: &str = "pin-symbolic";

/// Minimal tab-layout snapshot used by the pure target-selection helpers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TabLayoutEntry {
    /// Whether the page lives in the pinned leading segment.
    pinned: bool,
}

impl LushtextWindow {
    /// Handle AdwTabView's close request for one tab page.
    pub(super) fn handle_tab_close_request(
        window: Option<&Self>,
        tab_view: &libadwaita::TabView,
        page: &libadwaita::TabPage,
    ) -> glib::Propagation {
        if let Some(window) = window
            && window.consume_preconfirmed_tab_close(page)
        {
            tab_view.close_page_finish(page, true);
            return glib::Propagation::Stop;
        }

        let child = page.child();
        let Some(editor) = child.downcast_ref::<LushtextEditorPage>() else {
            tab_view.close_page_finish(page, true);
            return glib::Propagation::Stop;
        };
        if !editor.is_modified() {
            tab_view.close_page_finish(page, true);
            return glib::Propagation::Stop;
        }
        let Some(window) = window else {
            tab_view.close_page_finish(page, false);
            return glib::Propagation::Stop;
        };
        let tab_view = tab_view.clone();
        let page = page.clone();
        let page_for_finish = page.clone();
        window.confirm_close_tab(&page, editor, move |confirmed| {
            tab_view.close_page_finish(&page_for_finish, confirmed);
        });
        glib::Propagation::Stop
    }

    /// Clean up all window bookkeeping after AdwTabView detaches a page.
    pub(super) fn handle_tab_detached(&self, page: &libadwaita::TabPage) {
        self.forget_tab_page(page);
        if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
            if let Some(ref path) = editor.file_path() {
                let mut paths = self.imp().open_paths.borrow_mut();
                paths.remove(path.as_path());
                paths.remove(&super::documents::open_path_key(path));
                if let Some(canonical_path) = editor.canonical_file_path() {
                    paths.remove(&canonical_path);
                }
            }
            self.dismiss_editor_notifications(editor);
            self.untrack_editor_memory(editor);
            editor.cancel_load();
            editor.stop_file_monitor();
        }
        if !self.tab_projection_refresh_deferred() {
            self.refresh_tab_model_projections();
        }
        self.save_session_debounced();
    }

    /// Install the native Adwaita tab context menu and its actions.
    ///
    /// The menu model lives on `AdwTabView` itself so right-click handling stays
    /// toolkit-owned instead of relying on custom tab-hit testing.
    pub(super) fn setup_tab_management(&self) {
        self.register_tab_context_actions();
        self.imp()
            .tab_view
            .set_menu_model(Some(&self.imp().tab_management.context_menu));

        let window_weak = self.downgrade();
        self.imp().tab_view.connect_setup_menu(move |_view, page| {
            if let Some(window) = window_weak.upgrade() {
                window.refresh_tab_context_menu(page);
            }
        });

        let window_weak = self.downgrade();
        self.imp().tab_view.connect_page_reordered(move |_, _, _| {
            if let Some(window) = window_weak.upgrade() {
                window.save_session_debounced();
                window.refresh_tab_context_menu(None);
            }
        });

        for i in 0..self.imp().tab_view.n_pages() {
            let page = self.imp().tab_view.nth_page(i);
            self.configure_tab_page(&page);
        }
        self.refresh_tab_context_menu(None);
    }

    /// Attach pin-state bookkeeping to a newly created page exactly once.
    pub(crate) fn configure_tab_page(&self, page: &libadwaita::TabPage) {
        let page_key = tab_page_key(page);
        if !self
            .imp()
            .tab_management
            .configured_pages
            .borrow_mut()
            .insert(page_key)
        {
            return;
        }

        Self::refresh_tab_page_indicator(page);

        let window_weak = self.downgrade();
        let page_weak = page.downgrade();
        page.connect_pinned_notify(move |_| {
            if let Some(window) = window_weak.upgrade()
                && let Some(page) = page_weak.upgrade()
            {
                Self::refresh_tab_page_indicator(&page);
                window.save_session_debounced();
            }
        });
    }

    /// Apply a restored pinned state without surfacing user-facing feedback.
    pub(crate) fn restore_tab_pinned_state(&self, page: &libadwaita::TabPage, pinned: bool) {
        if page.is_pinned() != pinned {
            self.imp().tab_view.set_page_pinned(page, pinned);
        }
        Self::refresh_tab_page_indicator(page);
    }

    /// Consume one bulk-close authorization token for `page`, if present.
    ///
    /// `connect_close_page` calls this before showing the normal confirmation
    /// dialog so a previously confirmed bulk close can continue without asking
    /// again for every modified tab in the batch.
    pub(crate) fn consume_preconfirmed_tab_close(&self, page: &libadwaita::TabPage) -> bool {
        self.imp()
            .tab_management
            .preconfirmed_close_pages
            .borrow_mut()
            .remove(&tab_page_key(page))
    }

    /// Drop any stored menu or close state that still points at `page`.
    pub(crate) fn forget_tab_page(&self, page: &libadwaita::TabPage) {
        let page_key = tab_page_key(page);
        self.imp()
            .tab_management
            .configured_pages
            .borrow_mut()
            .remove(&page_key);
        self.imp()
            .tab_management
            .preconfirmed_close_pages
            .borrow_mut()
            .remove(&page_key);

        let target_matches = self
            .imp()
            .tab_management
            .target_page
            .borrow()
            .as_ref()
            .and_then(glib::WeakRef::upgrade)
            .is_some_and(|target| tab_page_key(&target) == page_key);
        if target_matches {
            self.refresh_tab_context_menu(None);
        }
    }

    /// Register the window actions that back the tab context menu.
    fn register_tab_context_actions(&self) {
        self.add_tab_context_action("toggle-tab-pinned", |window| {
            window.toggle_target_tab_pinned();
        });
        self.add_tab_context_action("close-tabs-right", |window| {
            window.close_tabs_to_the_right_from_target();
        });
        self.add_tab_context_action("close-other-tabs", |window| {
            window.close_other_tabs_from_target();
        });
        self.add_tab_context_action("move-tab-left", |window| {
            window.move_target_tab_left();
        });
        self.add_tab_context_action("move-tab-right", |window| {
            window.move_target_tab_right();
        });
    }

    /// Add one disabled-by-default action that only becomes active for a menu target.
    fn add_tab_context_action(&self, name: &'static str, on_activate: fn(&LushtextWindow)) {
        let action = gio::SimpleAction::new(name, None);
        action.set_enabled(false);
        let window_weak = self.downgrade();
        action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                on_activate(&window);
            }
        });
        self.add_action(&action);
    }

    /// Rebuild the menu label and enabled state for the current setup target.
    fn refresh_tab_context_menu(&self, target: Option<&libadwaita::TabPage>) {
        {
            let mut target_slot = self.imp().tab_management.target_page.borrow_mut();
            *target_slot = target.map(gtk4::prelude::ObjectExt::downgrade);
        }

        let pin_label = if target.is_some_and(libadwaita::TabPage::is_pinned) {
            "Unpin"
        } else {
            "Pin"
        };
        rebuild_tab_context_menu(&self.imp().tab_management.context_menu, pin_label);

        if let Some((_, _pages, layout, target_index)) = self.current_target_context() {
            self.set_tab_action_enabled("toggle-tab-pinned", true);
            self.set_tab_action_enabled(
                "close-tabs-right",
                !eligible_close_right_positions(&layout, target_index).is_empty(),
            );
            self.set_tab_action_enabled(
                "close-other-tabs",
                !eligible_close_other_positions(&layout, target_index).is_empty(),
            );
            self.set_tab_action_enabled("move-tab-left", can_move_left(&layout, target_index));
            self.set_tab_action_enabled("move-tab-right", can_move_right(&layout, target_index));
            return;
        }

        for name in [
            "toggle-tab-pinned",
            "close-tabs-right",
            "close-other-tabs",
            "move-tab-left",
            "move-tab-right",
        ] {
            self.set_tab_action_enabled(name, false);
        }
    }

    /// Resolve the current menu target into a stable page plus layout snapshot.
    fn current_target_context(
        &self,
    ) -> Option<(
        libadwaita::TabPage,
        Vec<libadwaita::TabPage>,
        Vec<TabLayoutEntry>,
        usize,
    )> {
        let target = self
            .imp()
            .tab_management
            .target_page
            .borrow()
            .as_ref()
            .and_then(glib::WeakRef::upgrade)?;
        let pages = collect_tab_pages(self);
        let target_index = pages
            .iter()
            .position(|page| tab_page_key(page) == tab_page_key(&target))?;
        let layout = pages
            .iter()
            .map(|page| TabLayoutEntry {
                pinned: page.is_pinned(),
            })
            .collect();
        Some((target, pages, layout, target_index))
    }

    /// Toggle the current menu target between the pinned and unpinned segments.
    fn toggle_target_tab_pinned(&self) {
        let Some((target, _, _, _)) = self.current_target_context() else {
            self.refresh_tab_context_menu(None);
            return;
        };

        let pinned = !target.is_pinned();
        self.imp().tab_view.set_page_pinned(&target, pinned);
        let title = tab_display_title(&target);
        let message = if pinned {
            format!("Pinned {title}")
        } else {
            format!("Unpinned {title}")
        };
        self.publish_status_message(&message, MessageKind::Info);
        self.refresh_tab_context_menu(None);
    }

    /// Move the current menu target one slot toward the pinned edge.
    fn move_target_tab_left(&self) {
        let Some((target, _pages, layout, target_index)) = self.current_target_context() else {
            self.refresh_tab_context_menu(None);
            return;
        };
        if !can_move_left(&layout, target_index) {
            self.refresh_tab_context_menu(None);
            return;
        }

        if self.imp().tab_view.reorder_backward(&target) {
            self.publish_status_message(
                &format!("Moved {} left", tab_display_title(&target)),
                MessageKind::Info,
            );
        }
        self.refresh_tab_context_menu(None);
    }

    /// Move the current menu target one slot away from the pinned edge.
    fn move_target_tab_right(&self) {
        let Some((target, _pages, layout, target_index)) = self.current_target_context() else {
            self.refresh_tab_context_menu(None);
            return;
        };
        if !can_move_right(&layout, target_index) {
            self.refresh_tab_context_menu(None);
            return;
        }

        if self.imp().tab_view.reorder_forward(&target) {
            self.publish_status_message(
                &format!("Moved {} right", tab_display_title(&target)),
                MessageKind::Info,
            );
        }
        self.refresh_tab_context_menu(None);
    }

    /// Close all unpinned tabs except the current menu target.
    fn close_other_tabs_from_target(&self) {
        let Some((_target, pages, layout, target_index)) = self.current_target_context() else {
            self.refresh_tab_context_menu(None);
            return;
        };
        let targets = eligible_close_other_positions(&layout, target_index)
            .into_iter()
            .map(|index| pages[index].clone())
            .collect::<Vec<_>>();
        self.confirm_and_close_tab_pages(targets);
    }

    /// Close all unpinned tabs strictly after the current menu target.
    fn close_tabs_to_the_right_from_target(&self) {
        let Some((_target, pages, layout, target_index)) = self.current_target_context() else {
            self.refresh_tab_context_menu(None);
            return;
        };
        let targets = eligible_close_right_positions(&layout, target_index)
            .into_iter()
            .map(|index| pages[index].clone())
            .collect::<Vec<_>>();
        self.confirm_and_close_tab_pages(targets);
    }

    /// Ask once for any modified targets, then close the authorized batch.
    fn confirm_and_close_tab_pages(&self, mut targets: Vec<libadwaita::TabPage>) {
        if targets.is_empty() {
            self.refresh_tab_context_menu(None);
            return;
        }

        // Close from right to left so page indices and selection adjustments
        // stay stable while `page_detached` updates shell bookkeeping.
        targets.sort_by_key(|page| Reverse(self.imp().tab_view.page_position(page)));
        if targets.iter().any(page_has_saving_editor) {
            self.publish_save_in_progress_warning();
            self.refresh_tab_context_menu(None);
            return;
        }
        let modified_targets = collect_modified_close_targets(&targets);
        let close_count = targets.len();
        let window = self.clone();
        if modified_targets.is_empty() {
            self.authorize_and_close_tab_pages(&targets, close_count);
            return;
        }

        self.show_save_changes_dialog(&modified_targets, move |confirmed| {
            if confirmed {
                window.authorize_and_close_tab_pages(&targets, close_count);
            } else {
                window.refresh_tab_context_menu(None);
            }
        });
    }

    /// Mark the batch as already confirmed, then drive the normal close path.
    fn authorize_and_close_tab_pages(&self, targets: &[libadwaita::TabPage], close_count: usize) {
        {
            let mut preconfirmed = self
                .imp()
                .tab_management
                .preconfirmed_close_pages
                .borrow_mut();
            preconfirmed.extend(targets.iter().map(tab_page_key));
        }

        self.begin_tab_projection_refresh_batch();
        for page in targets {
            self.imp().tab_view.close_page(page);
        }
        self.end_tab_projection_refresh_batch();

        self.publish_status_message(
            &format!(
                "Closed {close_count} tab{}",
                if close_count == 1 { "" } else { "s" }
            ),
            MessageKind::Info,
        );
        self.refresh_tab_context_menu(None);
    }

    /// Refresh the pinned indicator shown on an individual page.
    fn refresh_tab_page_indicator(page: &libadwaita::TabPage) {
        let indicator = page
            .is_pinned()
            .then(|| gio::ThemedIcon::new(PIN_INDICATOR_ICON_NAME));
        page.set_indicator_icon(indicator.as_ref());
        page.set_indicator_activatable(false);
    }

    /// Enable or disable one tab-menu-backed action if it exists.
    fn set_tab_action_enabled(&self, action_name: &str, enabled: bool) {
        if let Some(action) = self.lookup_action(action_name)
            && let Some(simple) = action.downcast_ref::<gio::SimpleAction>()
        {
            simple.set_enabled(enabled);
        }
    }
}

/// Rebuild the current tab context menu with the right pin label.
fn rebuild_tab_context_menu(menu: &gio::Menu, pin_label: &str) {
    menu.remove_all();

    let pin_section = gio::Menu::new();
    pin_section.append(Some(pin_label), Some("win.toggle-tab-pinned"));
    menu.append_section(None, &pin_section);

    let close_section = gio::Menu::new();
    close_section.append(
        Some("Close All Tabs to the Right"),
        Some("win.close-tabs-right"),
    );
    close_section.append(Some("Close Other Tabs"), Some("win.close-other-tabs"));
    menu.append_section(None, &close_section);

    let move_section = gio::Menu::new();
    move_section.append(Some("Move Left"), Some("win.move-tab-left"));
    move_section.append(Some("Move Right"), Some("win.move-tab-right"));
    menu.append_section(None, &move_section);
}

/// Snapshot the current visual page order from `AdwTabView`.
fn collect_tab_pages(window: &LushtextWindow) -> Vec<libadwaita::TabPage> {
    let tab_view = &window.imp().tab_view;
    (0..tab_view.n_pages())
        .map(|index| tab_view.nth_page(index))
        .collect()
}

/// Keep close-target collection in the widget layer, but only return modified editors.
fn collect_modified_close_targets(
    targets: &[libadwaita::TabPage],
) -> Vec<(libadwaita::TabPage, LushtextEditorPage)> {
    targets
        .iter()
        .filter_map(|page| {
            let editor = page.child().downcast::<LushtextEditorPage>().ok()?;
            editor.is_modified().then(|| (page.clone(), editor))
        })
        .collect()
}

/// Return whether a tab page owns an editor with an in-flight background save.
fn page_has_saving_editor(page: &libadwaita::TabPage) -> bool {
    page.child()
        .downcast::<LushtextEditorPage>()
        .is_ok_and(|editor| editor.is_saving())
}

/// Use the editor title when available so status messages do not include the modified dot.
fn tab_display_title(page: &libadwaita::TabPage) -> String {
    page.child()
        .downcast::<LushtextEditorPage>()
        .map_or_else(|_| page.title().to_string(), |editor| editor.title())
}

/// Convert a tab page into a stable hash-set key for this process.
fn tab_page_key(page: &libadwaita::TabPage) -> usize {
    page.as_ptr() as usize
}

/// Return every unpinned page except the target itself.
fn eligible_close_other_positions(layout: &[TabLayoutEntry], target_index: usize) -> Vec<usize> {
    if target_index >= layout.len() {
        return Vec::new();
    }

    layout
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| (index != target_index && !entry.pinned).then_some(index))
        .collect()
}

/// Return every unpinned page strictly after the target.
fn eligible_close_right_positions(layout: &[TabLayoutEntry], target_index: usize) -> Vec<usize> {
    if target_index >= layout.len() {
        return Vec::new();
    }

    ((target_index + 1)..layout.len())
        .filter(|&index| !layout[index].pinned)
        .collect()
}

/// Whether the target can move one slot toward the pinned edge.
fn can_move_left(layout: &[TabLayoutEntry], target_index: usize) -> bool {
    if target_index >= layout.len() {
        return false;
    }

    let first_unpinned = first_unpinned_index(layout);
    if layout[target_index].pinned {
        target_index > 0
    } else {
        target_index > first_unpinned
    }
}

/// Whether the target can move one slot away from the pinned edge.
fn can_move_right(layout: &[TabLayoutEntry], target_index: usize) -> bool {
    if target_index >= layout.len() {
        return false;
    }

    let first_unpinned = first_unpinned_index(layout);
    if layout[target_index].pinned {
        target_index + 1 < first_unpinned
    } else {
        target_index + 1 < layout.len()
    }
}

/// Count the contiguous leading pinned segment.
fn first_unpinned_index(layout: &[TabLayoutEntry]) -> usize {
    layout.iter().take_while(|entry| entry.pinned).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pinned_layout(flags: &[bool]) -> Vec<TabLayoutEntry> {
        flags
            .iter()
            .copied()
            .map(|pinned| TabLayoutEntry { pinned })
            .collect()
    }

    #[test]
    fn test_close_other_targets_only_unpinned_tabs() {
        let layout = pinned_layout(&[true, true, false, false, false]);
        assert_eq!(eligible_close_other_positions(&layout, 2), vec![3, 4]);
        assert_eq!(eligible_close_other_positions(&layout, 0), vec![2, 3, 4]);
    }

    #[test]
    fn test_close_right_targets_skip_pinned_segment() {
        let layout = pinned_layout(&[true, true, false, false, false]);
        assert_eq!(eligible_close_right_positions(&layout, 0), vec![2, 3, 4]);
        assert_eq!(eligible_close_right_positions(&layout, 2), vec![3, 4]);
        assert!(eligible_close_right_positions(&layout, 4).is_empty());
    }

    #[test]
    fn test_move_boundaries_respect_pinned_segments() {
        let layout = pinned_layout(&[true, true, false, false]);
        assert!(!can_move_left(&layout, 0));
        assert!(can_move_right(&layout, 0));
        assert!(!can_move_right(&layout, 1));
        assert!(!can_move_left(&layout, 2));
        assert!(can_move_right(&layout, 2));
        assert!(!can_move_right(&layout, 3));
    }

    #[test]
    fn test_out_of_range_targets_are_safe_noops() {
        let layout = pinned_layout(&[true, false]);
        assert!(eligible_close_other_positions(&layout, 9).is_empty());
        assert!(eligible_close_right_positions(&layout, 9).is_empty());
        assert!(!can_move_left(&layout, 9));
        assert!(!can_move_right(&layout, 9));
    }
}
