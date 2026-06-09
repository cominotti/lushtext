// SPDX-License-Identifier: GPL-3.0-or-later

//! Window-shell dismissal for transient surfaces.
//!
//! Command palette, search UI, shell menus, and Focus Mode all live at the
//! window adapter layer, so their shared dismissal rules belong here instead
//! of inside any one surface.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use super::LushtextWindow;

impl LushtextWindow {
    /// Install global shell handlers for Escape and command-palette click-away.
    pub(super) fn setup_transient_surface_dismissal(&self) {
        self.setup_transient_escape_dismissal();
        self.setup_command_palette_click_away();
    }

    /// Close the topmost dismissible shell surface, returning whether one was closed.
    ///
    /// The order is intentional: palette first, then in-editor search, workspace
    /// search, and shell menus. Focus Mode exit runs only after this ladder says
    /// no transient surface handled the request.
    pub(super) fn close_topmost_transient_surface(&self) -> bool {
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

    /// Record that a child widget already handled the current Escape request.
    ///
    /// `GtkSearchEntry::stop-search` is emitted by a child widget before the
    /// window Bubble controller may see the same key event. This latch lets the
    /// shell stop that event without closing the next surface underneath.
    pub(super) fn mark_child_transient_escape_handled(&self) {
        self.imp().transient_child_escape_handled.set(true);
        let window_weak = self.downgrade();
        glib::idle_add_local_once(move || {
            if let Some(window) = window_weak.upgrade() {
                window.imp().transient_child_escape_handled.set(false);
            }
        });
    }

    /// Install the window-level Escape controller after child widgets get first chance.
    ///
    /// The handler claims Escape only when it closes a transient surface or
    /// exits Focus Mode.
    fn setup_transient_escape_dismissal(&self) {
        let controller = gtk4::EventControllerKey::new();
        // Bubble phase lets focused children, popovers, dropdowns, and dialogs
        // handle Escape before the shell considers closing its own surfaces.
        controller.set_propagation_phase(gtk4::PropagationPhase::Bubble);
        {
            // Signal closures outlive a single stack frame; keep only a weak
            // window reference so this controller never extends window lifetime.
            let window_weak = self.downgrade();
            controller.connect_key_pressed(move |_, key, _, _| {
                let Some(window) = window_weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                if key != gtk4::gdk::Key::Escape {
                    return glib::Propagation::Proceed;
                }
                if window.handle_transient_escape() {
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            });
        }
        self.add_controller(controller);
    }

    /// Install command-palette click-away handling on primary pointer presses.
    ///
    /// The capture-phase gesture observes presses anywhere in the window, but
    /// claims only outside presses that actually close the palette.
    fn setup_command_palette_click_away(&self) {
        let click = gtk4::GestureClick::new();
        click.set_button(1);
        // Capture phase sees click-away presses before an underlying shell
        // control can consume them. Inside-palette presses still proceed.
        click.set_propagation_phase(gtk4::PropagationPhase::Capture);
        {
            let window_weak = self.downgrade();
            click.connect_pressed(move |gesture, _, x, y| {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                if window.handle_command_palette_pointer_press(x, y) {
                    // Claim only dismissed click-away presses so the same click
                    // cannot also activate the widget underneath the palette.
                    gesture.set_state(gtk4::EventSequenceState::Claimed);
                }
            });
        }
        self.add_controller(click);
    }

    /// Apply the shell Escape contract for the current window state.
    ///
    /// First closes one topmost transient surface; if none is visible, exits
    /// Focus Mode through its action-backed path. Returns whether the key event
    /// should stop.
    fn handle_transient_escape(&self) -> bool {
        if self.imp().transient_child_escape_handled.replace(false) {
            return true;
        }
        if self.close_topmost_transient_surface() {
            return true;
        }
        if self.is_focus_mode_active() {
            self.set_focus_mode_action_state(false);
            return true;
        }
        false
    }

    /// Decide whether a primary pointer press should dismiss the command palette.
    ///
    /// Coordinates are window-relative. Hidden palettes and inside-palette
    /// presses are ignored; outside presses call `close_command_palette()`, which
    /// clears palette state and restores focus.
    fn handle_command_palette_pointer_press(&self, x: f64, y: f64) -> bool {
        if !self.imp().palette_revealer.reveals_child() {
            return false;
        }
        if self.command_palette_contains_window_point(x, y) {
            return false;
        }
        self.close_command_palette();
        true
    }

    /// Return whether a window-relative point belongs to the palette surface.
    ///
    /// `compute_bounds()` covers the palette allocation in window coordinates,
    /// while `pick()` catches descendant widgets that GTK may target directly,
    /// such as result rows, controls, and scrollbars.
    fn command_palette_contains_window_point(&self, x: f64, y: f64) -> bool {
        let window_widget = self.upcast_ref::<gtk4::Widget>();
        let palette_widget = self.imp().command_palette.upcast_ref::<gtk4::Widget>();
        let Some(bounds) = palette_widget.compute_bounds(window_widget) else {
            return false;
        };
        let point = graphene_point_from_window_coordinates(x, y);
        if bounds.contains_point(&point) {
            return true;
        }

        let Some(picked) = window_widget.pick(x, y, gtk4::PickFlags::DEFAULT) else {
            return false;
        };
        picked.is_ancestor(palette_widget)
    }

    /// Exercise the real Escape dismissal decision without synthesizing a GTK key event.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn handle_transient_escape_for_test(&self) -> bool {
        self.handle_transient_escape()
    }

    /// Exercise the real pointer dismissal decision without brittle controller dispatch.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn handle_command_palette_pointer_press_for_test(&self, x: f64, y: f64) -> bool {
        self.handle_command_palette_pointer_press(x, y)
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "Graphene points use f32 coordinates while GTK pointer events report f64 coordinates."
)]
fn graphene_point_from_window_coordinates(x: f64, y: f64) -> gtk4::graphene::Point {
    gtk4::graphene::Point::new(x as f32, y as f32)
}
