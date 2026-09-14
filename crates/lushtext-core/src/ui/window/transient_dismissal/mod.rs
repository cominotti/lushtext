// SPDX-License-Identifier: GPL-3.0-or-later

//! Dismiss one transient shell surface.
//!
//! Two user gestures reach the same question — *which surface, if any, does the
//! shell give up right now?* — so they are one workflow with one ordered ladder
//! rather than two independent handlers. Escape asks it for the keyboard; a
//! primary pointer press outside the command palette asks it for the pointer.
//!
//! The workflow owns no widget of its own. Every surface it dismisses belongs to
//! another row, which is why the ladder lives at the window shell: no single
//! surface can know whether something sits above it.
//!
//! ## Stages
//!
//! Entry points: the window's Bubble-phase `EventControllerKey` and its
//! Capture-phase `GestureClick`, both installed by
//! `setup_transient_surface_dismissal`. Bubble is deliberate — focused children,
//! dialogs, popovers, dropdowns, and text entries get first chance at Escape, and
//! the shell acts only if the key reaches it. Capture is equally deliberate for
//! the pointer: a click-away press must be seen before an underlying shell
//! control consumes it.
//!
//! 1. **Observe.** Read the live visibility of every dismissible surface into one
//!    `policy::TransientSurfaceState`. The evidence surface calls the same
//!    observer, so a probe sees exactly what the ladder will act on.
//! 2. **Decide.** `policy` resolves the observation, the child-handled latch, and
//!    Focus Mode into one `EscapeOutcome`; or resolves a pointer press into a
//!    dismiss/ignore verdict. No widget is touched in this stage.
//! 3. **Apply.** Close exactly the one surface the decision named, and claim the
//!    event only when the shell actually did something. Focus restoration belongs
//!    to the palette's own close path — a dismissal that hid the revealer
//!    directly would strand focus on a sidebar button.
//!
//! ## The one inversion
//!
//! Stage 3 has a deferred continuation with no return call:
//! `mark_child_transient_escape_handled` sets a latch and schedules
//! `glib::idle_add_local_once` to clear it. **Control resumes in that idle
//! callback**, one main-loop turn later, and nowhere else — no later stage of
//! this workflow clears the latch. The latch exists because
//! `GtkSearchEntry::stop-search` fires from a child before the window's Bubble
//! controller observes the same key event; without it the shell would close the
//! next surface underneath the one the child just closed, which is the cascade
//! the dismissal contract forbids. One turn is the shortest span that outlives
//! the event's own propagation and nothing more.
//!
//! ## Module roles
//!
//! | Module | Role |
//! | --- | --- |
//! | `mod.rs` (this file) | narrative facade |
//! | `policy` | pure policy — the ladder's order and the two decisions |
//! | `evidence` | evidence surface — the row's single observable state; `test-utils`-gated |
//! | `surfaces` | **called presentation surface, not a role** — the workflow's only widget contact: observation, palette containment, and applying one verdict |
//!
//! Three absences are recorded conclusions rather than unmet obligations:
//!
//! * **No coordination module.** Every stage completes inside the event handler,
//!   and the one inversion is a two-line latch, not a coordination job. None of
//!   the five bounded role names describes it.
//! * **No `test_policy.rs`.** The row has no timing or limit override: its single
//!   timer is a bare one-turn idle with nothing to tune.
//! * **No seam value object beyond `TransientSurfaceState`.** That bundle is the
//!   seam — it crosses the observer → `escape_outcome` → `topmost_dismissible`
//!   boundary and is reconstructed by the evidence surface, which is why it is
//!   reified instead of passed as five same-typed booleans.

pub mod policy;
mod surfaces;

#[cfg(feature = "test-utils")]
pub mod evidence;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use super::LushtextWindow;
use policy::EscapeOutcome;

impl LushtextWindow {
    /// Install the shell's Escape and command-palette click-away handlers.
    pub(super) fn setup_transient_surface_dismissal(&self) {
        self.setup_transient_escape_dismissal();
        self.setup_command_palette_click_away();
    }

    /// Record that a child widget already handled the current Escape request.
    ///
    /// The inversion named in the module doc: control resumes in the idle
    /// callback that clears the latch, one main-loop turn later.
    pub(super) fn mark_child_transient_escape_handled(&self) {
        self.imp().transient_child_escape_handled.set(true);
        let window_weak = self.downgrade();
        glib::idle_add_local_once(move || {
            if let Some(window) = window_weak.upgrade() {
                window.imp().transient_child_escape_handled.set(false);
            }
        });
    }

    /// Install the window-level Escape controller, after children get first chance.
    fn setup_transient_escape_dismissal(&self) {
        let controller = gtk4::EventControllerKey::new();
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
                if window.apply_transient_escape() {
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            });
        }
        self.add_controller(controller);
    }

    /// Install command-palette click-away handling on primary pointer presses.
    fn setup_command_palette_click_away(&self) {
        let click = gtk4::GestureClick::new();
        click.set_button(1);
        click.set_propagation_phase(gtk4::PropagationPhase::Capture);
        {
            let window_weak = self.downgrade();
            click.connect_pressed(move |gesture, _, x, y| {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                if window.apply_command_palette_pointer_press(x, y) {
                    // Claim only dismissed click-away presses so the same click
                    // cannot also activate the widget underneath the palette.
                    gesture.set_state(gtk4::EventSequenceState::Claimed);
                }
            });
        }
        self.add_controller(click);
    }

    /// Run one whole Escape press through the three stages.
    ///
    /// Returns whether the key event should stop at the shell.
    fn apply_transient_escape(&self) -> bool {
        // Stage 1. The latch is *consumed* here, unlike the evidence surface's
        // read: one Escape may spend it, and only one.
        let child_already_handled = self.imp().transient_child_escape_handled.replace(false);
        let surfaces = surfaces::observed_surface_state(self);
        // Stage 2.
        let outcome =
            policy::escape_outcome(child_already_handled, surfaces, self.is_focus_mode_active());
        // Stage 3.
        match outcome {
            EscapeOutcome::ChildAlreadyHandled | EscapeOutcome::Unhandled => {}
            EscapeOutcome::Dismiss(surface) => surfaces::dismiss(self, surface),
            EscapeOutcome::ExitFocusMode => self.set_focus_mode_action_state(false),
        }
        outcome.stops_event()
    }

    /// Run one primary pointer press through the three stages.
    ///
    /// Coordinates are window-relative.
    fn apply_command_palette_pointer_press(&self, x: f64, y: f64) -> bool {
        // Stage 1. The containment probe is asked only when the palette is up, so
        // an ordinary click never pays for a widget-tree walk.
        let revealed = surfaces::command_palette_revealed(self);
        let inside = revealed && surfaces::command_palette_contains_window_point(self, x, y);
        // Stage 2.
        let dismisses = policy::palette_click_away_dismisses(revealed, inside);
        // Stage 3. Through the palette's own close path, never the revealer, so
        // saved-focus restoration and cleanup run.
        if dismisses {
            self.close_command_palette();
        }
        dismisses
    }

    /// Exercise the real Escape decision without synthesizing a GTK key event.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn handle_transient_escape_for_test(&self) -> bool {
        self.apply_transient_escape()
    }

    /// Exercise the real pointer decision without brittle controller dispatch.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn handle_command_palette_pointer_press_for_test(&self, x: f64, y: f64) -> bool {
        self.apply_command_palette_pointer_press(x, y)
    }
}
