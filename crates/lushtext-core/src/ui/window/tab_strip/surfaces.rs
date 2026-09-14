// SPDX-License-Identifier: GPL-3.0-or-later

//! **Called presentation surface** for `WFR-TAB-STRIP` — not a role.
//!
//! Everything here projects the workflow onto widgets: it reads the live
//! `AdwTabView` page order, builds the context menu model, keeps the five
//! menu-backed actions' enabled state in step with the policy verdicts, and
//! draws the pin indicator. None of it owns an ordered stage, an admission
//! budget, a generation counter, or a durable record, so it carries no bounded
//! role name deliberately.
//!
//! It is a separate module from the facade because these are the calls that
//! *touch GTK*, and keeping them here is what lets the facade narrate the
//! stages without interleaving widget mutation with the story.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;

use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::window::LushtextWindow;

use super::policy::{self, TabLayoutEntry};

/// Symbolic icon shown on pinned tabs so the leading segment is explicit.
const PIN_INDICATOR_ICON_NAME: &str = "pin-symbolic";

/// Snapshot the current visual page order from `AdwTabView`.
pub(super) fn collect_tab_pages(window: &LushtextWindow) -> Vec<libadwaita::TabPage> {
    let tab_view = &window.imp().tab_view;
    (0..tab_view.n_pages())
        .map(|index| tab_view.nth_page(index))
        .collect()
}

/// Convert a tab page into a stable hash-set key for this process.
pub(super) fn tab_page_key(page: &libadwaita::TabPage) -> usize {
    page.as_ptr() as usize
}

/// Use the editor title when available so status messages do not include the
/// modified dot.
pub(super) fn tab_display_title(page: &libadwaita::TabPage) -> String {
    page.child()
        .downcast::<LushtextEditorPage>()
        .map_or_else(|_| page.title().to_string(), |editor| editor.title())
}

/// Return whether a tab page owns an editor with an in-flight background save.
pub(super) fn page_has_saving_editor(page: &libadwaita::TabPage) -> bool {
    page.child()
        .downcast::<LushtextEditorPage>()
        .is_ok_and(|editor| editor.is_saving())
}

/// Collect close targets, but only return the modified editors among them.
pub(super) fn collect_modified_close_targets(
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

/// Resolve the current menu target into a stable page plus layout snapshot.
///
/// The layout is the policy module's view of the strip; everything downstream
/// of this call reasons over `[TabLayoutEntry]` rather than over live pages.
pub(super) fn current_target_context(
    window: &LushtextWindow,
) -> Option<(
    libadwaita::TabPage,
    Vec<libadwaita::TabPage>,
    Vec<TabLayoutEntry>,
    usize,
)> {
    let target = window
        .imp()
        .tab_management
        .target_page
        .borrow()
        .as_ref()
        .and_then(glib::WeakRef::upgrade)?;
    let pages = collect_tab_pages(window);
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

/// Refresh the pinned indicator shown on an individual page.
pub(super) fn refresh_tab_page_indicator(page: &libadwaita::TabPage) {
    let indicator = page
        .is_pinned()
        .then(|| gio::ThemedIcon::new(PIN_INDICATOR_ICON_NAME));
    page.set_indicator_icon(indicator.as_ref());
    page.set_indicator_activatable(false);
}

/// The five window actions the tab context menu drives.
pub(super) const TAB_CONTEXT_ACTIONS: [&str; 5] = [
    "toggle-tab-pinned",
    "close-tabs-right",
    "close-other-tabs",
    "move-tab-left",
    "move-tab-right",
];

/// Register the window actions that back the tab context menu.
pub(super) fn register_tab_context_actions(window: &LushtextWindow) {
    add_tab_context_action(
        window,
        "toggle-tab-pinned",
        LushtextWindow::toggle_pinned_tab,
    );
    add_tab_context_action(
        window,
        "close-tabs-right",
        LushtextWindow::close_tabs_to_the_right,
    );
    add_tab_context_action(window, "close-other-tabs", LushtextWindow::close_other_tabs);
    add_tab_context_action(window, "move-tab-left", LushtextWindow::move_tab_left);
    add_tab_context_action(window, "move-tab-right", LushtextWindow::move_tab_right);
}

/// Add one disabled-by-default action that only becomes active for a menu target.
fn add_tab_context_action(
    window: &LushtextWindow,
    name: &'static str,
    on_activate: fn(&LushtextWindow),
) {
    let action = gio::SimpleAction::new(name, None);
    action.set_enabled(false);
    let window_weak = window.downgrade();
    action.connect_activate(move |_, _| {
        if let Some(window) = window_weak.upgrade() {
            on_activate(&window);
        }
    });
    window.add_action(&action);
}

/// Enable or disable one tab-menu-backed action if it exists.
pub(super) fn set_tab_action_enabled(window: &LushtextWindow, action_name: &str, enabled: bool) {
    if let Some(action) = window.lookup_action(action_name)
        && let Some(simple) = action.downcast_ref::<gio::SimpleAction>()
    {
        simple.set_enabled(enabled);
    }
}

/// Rebuild the menu label and enabled state for the current setup target.
pub(super) fn refresh_tab_context_menu(
    window: &LushtextWindow,
    target: Option<&libadwaita::TabPage>,
) {
    {
        let mut target_slot = window.imp().tab_management.target_page.borrow_mut();
        *target_slot = target.map(gtk4::prelude::ObjectExt::downgrade);
    }

    let pin_label = policy::pin_menu_label(target.is_some_and(libadwaita::TabPage::is_pinned));
    rebuild_tab_context_menu(&window.imp().tab_management.context_menu, pin_label);

    if let Some((_, _pages, layout, target_index)) = current_target_context(window) {
        set_tab_action_enabled(window, "toggle-tab-pinned", true);
        set_tab_action_enabled(
            window,
            "close-tabs-right",
            !policy::eligible_close_right_positions(&layout, target_index).is_empty(),
        );
        set_tab_action_enabled(
            window,
            "close-other-tabs",
            !policy::eligible_close_other_positions(&layout, target_index).is_empty(),
        );
        set_tab_action_enabled(
            window,
            "move-tab-left",
            policy::can_move_left(&layout, target_index),
        );
        set_tab_action_enabled(
            window,
            "move-tab-right",
            policy::can_move_right(&layout, target_index),
        );
        return;
    }

    for name in TAB_CONTEXT_ACTIONS {
        set_tab_action_enabled(window, name, false);
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
