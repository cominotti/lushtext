// SPDX-License-Identifier: GPL-3.0-or-later

//! **Called presentation surface** — not a role.
//!
//! Focus Mode's widget projection: hiding and restoring persistent
//! chrome, revealing and hiding the overlaid affordance with its accessibility
//! state, and pushing the mode's presentation settings onto every open editor.
//! It owns no ordered stage, no timer, and no coordination job, so it takes none
//! of the bounded role names and holds no `policy.rs` or `evidence.rs`. The
//! facade narrates and decides; this module projects.
//!
//! Every decision here comes from `super::policy` rather than being re-derived
//! locally, so the chrome the user sees and the chrome the evidence surface
//! reports cannot disagree.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::config::keys;
use crate::ui::accessibility;
use crate::ui::editor_page::LushtextEditorPage;

use super::super::LushtextWindow;
use super::policy;

/// Hide or restore persistent chrome for the current Focus Mode state.
pub(super) fn apply_chrome(window: &LushtextWindow) {
    let active = window.is_focus_mode_active();
    let visible = policy::chrome_visible(active);
    let imp = window.imp();
    imp.header_bar.set_visible(visible);
    window.sync_tab_bar_visibility();
    imp.status_bar.set_visible(visible);
    if !active {
        set_affordance_revealed(window, false);
    }
}

/// Keep the overlay revealer and its accessibility hidden state synchronized.
///
/// The two must move together: a revealed affordance that stays
/// `accessible-hidden` is invisible to a screen reader, and a hidden one that is
/// not marked hidden leaves a phantom control in the accessible tree.
pub(super) fn set_affordance_revealed(window: &LushtextWindow, revealed: bool) {
    let imp = window.imp();
    imp.focus_mode_revealer.set_reveal_child(revealed);
    accessibility::set_hidden(&*imp.focus_mode_affordance, !revealed);
}

#[cfg(feature = "test-utils")]
/// Whether the affordance revealer currently reveals its child.
///
/// Reached through `try_get()`: this is read by the evidence surface, which must
/// answer honestly on a disposed window rather than panicking.
pub(super) fn affordance_revealed(window: &LushtextWindow) -> bool {
    window
        .imp()
        .focus_mode_revealer
        .try_get()
        .is_some_and(|revealer| revealer.reveals_child())
}

/// Whether keyboard focus is on the affordance or one of its controls.
pub(super) fn affordance_contains_focus(window: &LushtextWindow) -> bool {
    let Some(focus) = gtk4::prelude::GtkWindowExt::focus(window) else {
        return false;
    };
    let Some(affordance) = window.imp().focus_mode_affordance.try_get() else {
        return false;
    };
    let affordance = affordance.upcast_ref::<gtk4::Widget>();
    focus.as_ptr() == affordance.as_ptr() || focus.is_ancestor(affordance)
}

/// Apply the current Focus Mode settings to every open editor tab.
///
/// Called on mode toggles, preference changes, and tab selection, so a newly
/// created or restored page immediately matches the window shell instead of
/// rendering one frame of the wrong presentation.
pub(super) fn apply_to_editors(window: &LushtextWindow) {
    let active = window.is_focus_mode_active();
    let imp = window.imp();
    let target = imp.settings.uint(keys::FOCUS_MODE_TARGET_COLUMNS);
    let typewriter = imp.settings.boolean(keys::FOCUS_MODE_TYPEWRITER_SCROLLING);

    let Some(tab_view) = imp.tab_view.try_get() else {
        return;
    };
    for index in 0..tab_view.n_pages() {
        let Ok(editor) = tab_view
            .nth_page(index)
            .child()
            .downcast::<LushtextEditorPage>()
        else {
            continue;
        };
        editor.set_focus_mode_target_columns(target);
        editor.set_focus_mode_typewriter_scrolling(typewriter);
        editor.set_focus_mode_active(active);
    }
}

#[cfg(feature = "test-utils")]
/// Chrome visibility as the shell currently presents it.
///
/// Read by the evidence surface so a test can assert what the user can see
/// rather than what the workflow intended. Every child is reached through
/// `try_get()`, and a cleared child reads as not visible.
pub(super) fn chrome_visibility(window: &LushtextWindow) -> (bool, bool) {
    let imp = window.imp();
    (
        imp.header_bar
            .try_get()
            .is_some_and(|bar| bar.property::<bool>("visible")),
        imp.status_bar
            .try_get()
            .is_some_and(|bar| bar.property::<bool>("visible")),
    )
}

#[cfg(feature = "test-utils")]
/// The number of open editor pages currently carrying Focus Mode presentation.
///
/// Bounded by the tab count, answers `0` honestly when the tab view is gone, and
/// skips a disposed page rather than panicking on it — the disposed-widget rule
/// applied to a set rather than to one child.
pub(super) fn editors_with_focus_mode_applied(window: &LushtextWindow) -> usize {
    let Some(tab_view) = window.imp().tab_view.try_get() else {
        return 0;
    };
    (0..tab_view.n_pages())
        .filter_map(|index| {
            tab_view
                .nth_page(index)
                .child()
                .downcast::<LushtextEditorPage>()
                .ok()
        })
        // The editor page exposes no `is_focus_mode_active()`; its state is a
        // plain `Cell<bool>` on the imp struct, which is safe to read on a
        // disposed page because it derefs no template child.
        .filter(|editor| editor.imp().focus_mode.active.get())
        .count()
}

/// Install the pointer and keyboard reveal controllers for the affordance.
///
/// Widget wiring, not a stage: the controllers call back into the facade's
/// `reveal_focus_mode_affordance_temporarily`, which is where the workflow's one
/// inversion lives. The reveal *band* is a `policy` decision so the geometry is
/// testable without a compositor.
pub(super) fn install_reveal_controllers(window: &LushtextWindow) {
    let motion = gtk4::EventControllerMotion::new();
    {
        // Signal closures outlive a single stack frame; keep only a weak window
        // reference so a controller never extends window lifetime.
        let window_weak = window.downgrade();
        motion.connect_motion(move |_, _x, y| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            if policy::pointer_reveals_affordance(window.is_focus_mode_active(), y) {
                window.reveal_focus_mode_affordance_temporarily();
            }
        });
    }
    window.imp().window_overlay.add_controller(motion);

    window
        .imp()
        .focus_mode_affordance
        .connect_has_focus_notify({
            let window_weak = window.downgrade();
            move |affordance| {
                if affordance.has_focus()
                    && let Some(window) = window_weak.upgrade()
                    && window.is_focus_mode_active()
                {
                    set_affordance_revealed(&window, true);
                }
            }
        });
}

/// Keep active Focus Mode presentation synchronized with preference changes.
///
/// Both keys reach editors; only the column width also reaches the preview,
/// because typewriter scrolling is an editor-local behavior with no preview
/// counterpart.
pub(super) fn install_settings_hooks(window: &LushtextWindow) {
    let imp = window.imp();
    {
        let window_weak = window.downgrade();
        imp.settings
            .connect_changed(Some(keys::FOCUS_MODE_TARGET_COLUMNS), move |_, _| {
                if let Some(window) = window_weak.upgrade() {
                    apply_to_editors(&window);
                    window.refresh_focus_mode_preview_column();
                }
            });
    }
    {
        let window_weak = window.downgrade();
        imp.settings
            .connect_changed(Some(keys::FOCUS_MODE_TYPEWRITER_SCROLLING), move |_, _| {
                if let Some(window) = window_weak.upgrade() {
                    apply_to_editors(&window);
                }
            });
    }
}
