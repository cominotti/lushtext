// SPDX-License-Identifier: GPL-3.0-or-later

//! Focus Mode shell workflow.
//!
//! This module coordinates reversible window chrome suppression, fullscreen
//! ownership, preview compatibility, and tab-local Focus Mode presentation.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;

use crate::config::keys;

use super::LushtextWindow;

/// Pointer distance from the top edge that reveals the Focus Mode affordance.
///
/// Forty-eight pixels is large enough to hit deliberately while staying above
/// the centered prose column during ordinary typing.
const TOP_EDGE_REVEAL_HEIGHT: f64 = 48.0;

impl LushtextWindow {
    /// Register the Focus Mode action, reveal behavior, Escape handling, and settings hooks.
    pub(super) fn setup_focus_mode(&self) {
        let action =
            gio::SimpleAction::new_stateful("toggle-focus-mode", None, &false.to_variant());
        {
            action.connect_activate(move |action, _| {
                let current = action
                    .state()
                    .and_then(|state| state.get::<bool>())
                    .unwrap_or(false);
                action.change_state(&(!current).to_variant());
            });
        }
        {
            let window_weak = self.downgrade();
            action.connect_change_state(move |action, state| {
                let Some(state) = state else { return };
                let Some(active) = state.get::<bool>() else {
                    tracing::error!("toggle-focus-mode: expected bool state");
                    return;
                };
                action.set_state(&active.to_variant());
                if let Some(window) = window_weak.upgrade() {
                    window.set_focus_mode_active(active);
                }
            });
        }
        self.add_action(&action);

        self.setup_focus_mode_reveal();
        self.setup_focus_mode_escape();
        self.setup_focus_mode_settings();
    }

    /// Return whether this window is currently in Focus Mode.
    #[must_use]
    pub(crate) fn is_focus_mode_active(&self) -> bool {
        self.imp().focus_mode.active.get()
    }

    /// Record that the user changed preview state while Focus Mode was active.
    ///
    /// This prevents exit from restoring a previous side-by-side preview state
    /// over an explicit focused preview-only choice.
    pub(crate) fn mark_focus_mode_preview_changed(&self) {
        if self.is_focus_mode_active() {
            self.imp()
                .focus_mode
                .preview_changed_while_focused
                .set(true);
        }
    }

    /// Reapply the Focus Mode readable-column policy to rendered Markdown.
    ///
    /// The preview text view only receives Focus Mode margins while preview-only
    /// mode is visible, so normal side-by-side preview keeps its usual padding.
    pub(crate) fn refresh_focus_mode_preview_column(&self) {
        let active = self.is_focus_mode_active() && self.imp().preview_mode.get();
        let target = self.imp().settings.uint(keys::FOCUS_MODE_TARGET_COLUMNS);
        self.imp()
            .markdown_preview
            .set_focus_mode_readable_column(active, target);
    }

    /// Route the requested state change to the Focus Mode transition helpers.
    fn set_focus_mode_active(&self, active: bool) {
        if active {
            self.enter_focus_mode();
        } else {
            self.exit_focus_mode();
        }
    }

    /// Enter Focus Mode and capture the shell state that exit must restore.
    fn enter_focus_mode(&self) {
        let imp = self.imp();
        if imp.focus_mode.active.get() {
            return;
        }

        imp.focus_mode.active.set(true);
        imp.focus_mode
            .was_fullscreen_on_entry
            .set(self.is_fullscreen());
        imp.focus_mode
            .restore_side_by_side_preview
            .set(imp.preview_visible.get());
        imp.focus_mode.preview_changed_while_focused.set(false);

        if !self.is_fullscreen() {
            self.fullscreen();
        }
        if imp.preview_visible.get() {
            self.set_preview_pane_visible_for_focus_mode(false);
        }

        self.apply_focus_mode_chrome();
        self.apply_focus_mode_to_editors();
        self.refresh_focus_mode_preview_column();
        self.sync_secondary_surface_layout();
        self.reveal_focus_mode_affordance_temporarily();
        if let Some(editor) = self.active_editor() {
            editor.source_view().grab_focus();
        }
    }

    /// Exit Focus Mode and restore only the shell state owned by this mode.
    fn exit_focus_mode(&self) {
        let imp = self.imp();
        if !imp.focus_mode.active.get() {
            return;
        }

        let restore_preview = imp.focus_mode.restore_side_by_side_preview.get()
            && !imp.focus_mode.preview_changed_while_focused.get();
        let should_leave_fullscreen = !imp.focus_mode.was_fullscreen_on_entry.get();

        imp.focus_mode.active.set(false);
        imp.focus_mode_revealer.set_reveal_child(false);
        self.set_preview_mode_for_focus_mode(false);
        if restore_preview {
            self.set_preview_pane_visible_for_focus_mode(true);
        }
        if should_leave_fullscreen && self.is_fullscreen() {
            self.unfullscreen();
        }

        self.apply_focus_mode_chrome();
        self.apply_focus_mode_to_editors();
        self.refresh_focus_mode_preview_column();
        self.sync_secondary_surface_layout();
        self.sync_focus_mode_action_state();
    }

    /// Hide or restore persistent chrome according to current Focus Mode state.
    fn apply_focus_mode_chrome(&self) {
        let active = self.is_focus_mode_active();
        let imp = self.imp();
        imp.header_bar.set_visible(!active);
        imp.tab_bar.set_visible(!active);
        imp.status_bar.set_visible(!active);
        if !active {
            imp.focus_mode_revealer.set_reveal_child(false);
        }
        self.sync_focus_mode_action_state();
    }

    /// Apply current Focus Mode settings to every open editor tab.
    ///
    /// This is called on mode toggles, preference changes, and tab selection so
    /// newly created or restored pages immediately match the window shell.
    pub(super) fn apply_focus_mode_to_editors(&self) {
        let active = self.is_focus_mode_active();
        let target = self.imp().settings.uint(keys::FOCUS_MODE_TARGET_COLUMNS);
        let typewriter = self
            .imp()
            .settings
            .boolean(keys::FOCUS_MODE_TYPEWRITER_SCROLLING);

        let tab_view = &self.imp().tab_view;
        for index in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(index);
            let Ok(editor) = page
                .child()
                .downcast::<crate::ui::editor_page::LushtextEditorPage>()
            else {
                continue;
            };
            editor.set_focus_mode_target_columns(target);
            editor.set_focus_mode_typewriter_scrolling(typewriter);
            editor.set_focus_mode_active(active);
        }
    }

    /// Wire pointer and keyboard focus reveal behavior for the overlaid affordance.
    fn setup_focus_mode_reveal(&self) {
        let motion = gtk4::EventControllerMotion::new();
        {
            let window_weak = self.downgrade();
            motion.connect_motion(move |_, _x, y| {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                if window.is_focus_mode_active() && y <= TOP_EDGE_REVEAL_HEIGHT {
                    window.reveal_focus_mode_affordance_temporarily();
                }
            });
        }
        self.imp().window_overlay.add_controller(motion);

        self.imp().focus_mode_affordance.connect_has_focus_notify({
            let window_weak = self.downgrade();
            move |affordance| {
                if affordance.has_focus()
                    && let Some(window) = window_weak.upgrade()
                    && window.is_focus_mode_active()
                {
                    window.imp().focus_mode_revealer.set_reveal_child(true);
                }
            }
        });
    }

    /// Reveal the overlaid affordance briefly, then hide it unless focus stays inside it.
    fn reveal_focus_mode_affordance_temporarily(&self) {
        let imp = self.imp();
        if !imp.focus_mode.active.get() {
            return;
        }
        imp.focus_mode_revealer.set_reveal_child(true);
        let generation = imp.focus_mode.affordance_generation.get().wrapping_add(1);
        imp.focus_mode.affordance_generation.set(generation);

        let window_weak = self.downgrade();
        glib::timeout_add_local_once(std::time::Duration::from_millis(1800), move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let imp = window.imp();
            if imp.focus_mode.affordance_generation.get() == generation
                && !imp.focus_mode_affordance.has_focus()
            {
                imp.focus_mode_revealer.set_reveal_child(false);
            }
        });
    }

    /// Install the Escape handler that gives transient surfaces priority over mode exit.
    fn setup_focus_mode_escape(&self) {
        let controller = gtk4::EventControllerKey::new();
        controller.set_propagation_phase(gtk4::PropagationPhase::Bubble);
        {
            let window_weak = self.downgrade();
            controller.connect_key_pressed(move |_, key, _, _| {
                let Some(window) = window_weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                if key != gtk4::gdk::Key::Escape || !window.is_focus_mode_active() {
                    return glib::Propagation::Proceed;
                }
                if window.close_focus_mode_transient_surface() {
                    glib::Propagation::Stop
                } else {
                    window.set_focus_mode_action_state(false);
                    glib::Propagation::Stop
                }
            });
        }
        self.add_controller(controller);
    }

    /// Close the topmost transient Focus Mode surface, returning whether one was handled.
    fn close_focus_mode_transient_surface(&self) -> bool {
        let imp = self.imp();
        if imp.palette_revealer.reveals_child() {
            self.close_command_palette();
            return true;
        }
        if let Some(editor) = self.active_editor()
            && editor.is_search_visible()
        {
            editor.hide_search();
            return true;
        }
        if imp.search_panel_revealer.reveals_child() {
            self.close_search_panel();
            return true;
        }
        if imp.primary_menu_button.is_active() {
            imp.primary_menu_button.set_active(false);
            return true;
        }
        if imp.notes_menu_button.is_active() {
            imp.notes_menu_button.set_active(false);
            return true;
        }
        false
    }

    /// Keep active Focus Mode presentation synchronized with preference changes.
    fn setup_focus_mode_settings(&self) {
        {
            let window_weak = self.downgrade();
            self.imp().settings.connect_changed(
                Some(keys::FOCUS_MODE_TARGET_COLUMNS),
                move |_, _| {
                    if let Some(window) = window_weak.upgrade() {
                        window.apply_focus_mode_to_editors();
                        window.refresh_focus_mode_preview_column();
                    }
                },
            );
        }
        {
            let window_weak = self.downgrade();
            self.imp().settings.connect_changed(
                Some(keys::FOCUS_MODE_TYPEWRITER_SCROLLING),
                move |_, _| {
                    if let Some(window) = window_weak.upgrade() {
                        window.apply_focus_mode_to_editors();
                    }
                },
            );
        }
    }

    /// Mirror the internal Focus Mode state back to the stateful window action.
    fn sync_focus_mode_action_state(&self) {
        self.set_focus_mode_action_state(self.is_focus_mode_active());
    }

    /// Set or request the stateful Focus Mode action without duplicating transitions.
    fn set_focus_mode_action_state(&self, active: bool) {
        let Some(action) = self.lookup_action("toggle-focus-mode") else {
            return;
        };
        let Some(action) = action.downcast_ref::<gio::SimpleAction>() else {
            return;
        };
        let current = action
            .state()
            .and_then(|state| state.get::<bool>())
            .unwrap_or(!active);
        if active != self.imp().focus_mode.active.get() {
            action.change_state(&active.to_variant());
        } else if current != active {
            action.set_state(&active.to_variant());
        }
    }
}
