// SPDX-License-Identifier: GPL-3.0-or-later

//! Enter and leave Focus Mode.
//!
//! One user operation with two directions, and they are one workflow rather than
//! two: entry *captures* the shell state that exit *hands back*, so neither half
//! is meaningful without the other. Everything Focus Mode does is reversible
//! chrome suppression — it borrows fullscreen, the header bar, the tab bar, the
//! status bar, and the side-by-side preview pane, and the whole contract is
//! giving back exactly what it took and nothing more.
//!
//! ## Stages
//!
//! Entry points, all converging on the stateful `win.toggle-focus-mode` action so
//! that every route runs the same fullscreen, chrome, and editor side effects:
//! the action itself (menu and shortcut), the parameterized `win.set-focus-mode`
//! automation target-state action, and the shell's Escape ladder, which calls
//! `set_focus_mode_action_state(false)` after transient dismissal declines.
//!
//! **Entering:**
//!
//! 1. **Capture.** Record the shell state Focus Mode is about to borrow as one
//!    `policy::FocusModeEntry`, and clear the explicit-preview-choice flag that
//!    the session is about to start accumulating.
//! 2. **Take.** Take fullscreen if the window does not already have it, and hide
//!    the side-by-side preview pane if one is showing.
//! 3. **Present.** Suppress chrome, push the mode onto every open editor, apply
//!    the readable column, settle the secondary-surface layout, reveal the
//!    affordance briefly, announce, and focus the editor.
//!
//! **Leaving:**
//!
//! 1. **Decide.** Ask `policy::exit_restoration` what this session must hand
//!    back, from the captured entry state and whether the user made an explicit
//!    preview choice while focused.
//! 2. **Give back.** Cancel the affordance timer, drop preview-only mode, restore
//!    the preview pane if the decision says so, and leave fullscreen **only** if
//!    Focus Mode was the one that took it.
//! 3. **Present.** The same presentation stage as entry, plus mirroring the state
//!    back to the action so every entry route agrees.
//!
//! ## The one inversion
//!
//! The affordance hide is deferred: revealing it arms
//! `FocusModeState::affordance_timer`, a `SupersedingTimer`, for
//! `FOCUS_MODE_AFFORDANCE_REVEAL_BUDGET`. **Control resumes in that timer's
//! callback**, which re-reads live focus and hides the affordance only if focus
//! is not inside it. Pointer motion near the top edge re-arms rather than
//! stacking, which is the whole reason it is a superseding timer: a user sweeping
//! the pointer along the top edge would otherwise queue one hide per motion event.
//! Exit invalidates the timer, so a pending hide cannot fire against a window
//! that has already left the mode.
//!
//! ## Module roles
//!
//! | Module | Role |
//! | --- | --- |
//! | `mod.rs` (this file) | narrative facade |
//! | `policy` | pure policy — the capture/restore rule, the reveal band, the readable-column and chrome predicates |
//! | `evidence` | evidence surface — the row's single observable state; `test-utils`-gated |
//! | `chrome` | **called presentation surface, not a role** — the workflow's only widget contact, including controller and settings wiring |
//!
//! Two absences are recorded conclusions rather than unmet obligations:
//!
//! * **No coordination module.** Both directions run to completion inside the
//!   action's `change_state` handler; the one inversion is a superseding timer
//!   owned by the `imp` state group, not a coordination job. None of the five
//!   bounded role names describes it.
//! * **No `test_policy.rs`.** The row has no test-only override.
//!   `FOCUS_MODE_AFFORDANCE_REVEAL_BUDGET` is a **production** constant that
//!   tests read so they derive their waits from the real deadline instead of
//!   re-declaring it; exporting a production value is not a test seam.

pub mod policy;

mod chrome;
#[cfg(feature = "test-utils")]
pub mod evidence;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;

use crate::config::keys;
use crate::ui::accessibility::AnnouncementLane;

use super::LushtextWindow;
use policy::FocusModeEntry;

/// How long the Focus Mode affordance stays revealed before it hides itself.
///
/// Named and exported because a widget test has to reason about it: entering
/// Focus Mode arms this budget, and when it expires the affordance hides *unless*
/// focus is inside it. A test that races this deadline must derive its own waits
/// from this value rather than re-declaring the number, or the two drift apart
/// and the test silently starts measuring the race instead of the behavior.
pub const FOCUS_MODE_AFFORDANCE_REVEAL_BUDGET: std::time::Duration =
    std::time::Duration::from_millis(1800);

impl LushtextWindow {
    /// Register the Focus Mode action, reveal behavior, and settings hooks.
    pub(super) fn setup_focus_mode(&self) {
        let action =
            gio::SimpleAction::new_stateful("toggle-focus-mode", None, &false.to_variant());
        action.connect_activate(move |action, _| {
            let current = action
                .state()
                .and_then(|state| state.get::<bool>())
                .unwrap_or(false);
            action.change_state(&(!current).to_variant());
        });
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
                    if active {
                        window.enter_focus_mode();
                    } else {
                        window.exit_focus_mode();
                    }
                }
            });
        }
        self.add_action(&action);
        self.add_action_entries([gio::ActionEntry::builder("set-focus-mode")
            .parameter_type(Some(glib::VariantTy::BOOLEAN))
            .activate(|window: &Self, _, parameter| {
                let Some(active) = parameter.and_then(glib::Variant::get::<bool>) else {
                    tracing::error!("set-focus-mode: expected bool parameter");
                    return;
                };
                window.set_focus_mode_action_state(active);
            })
            .build()]);

        chrome::install_reveal_controllers(self);
        chrome::install_settings_hooks(self);
    }

    /// Return whether this window is currently in Focus Mode.
    #[must_use]
    pub(crate) fn is_focus_mode_active(&self) -> bool {
        self.imp().focus_mode.active.get()
    }

    /// Record that the user changed preview state while Focus Mode was active.
    ///
    /// This is what makes an explicit choice beat restoration: exit must not
    /// restore a previous side-by-side preview over a preview state the user
    /// asked for during the focused session.
    pub(crate) fn mark_focus_mode_preview_changed(&self) {
        if self.is_focus_mode_active() {
            self.imp()
                .focus_mode
                .preview_changed_while_focused
                .set(true);
        }
    }

    /// Reapply the Focus Mode readable-column policy to rendered Markdown.
    pub(crate) fn refresh_focus_mode_preview_column(&self) {
        let target = self.imp().settings.uint(keys::FOCUS_MODE_TARGET_COLUMNS);
        self.imp()
            .markdown_preview
            .set_focus_mode_readable_column(preview_readable_column_applies(self), target);
        if self.imp().preview_mode.get() {
            self.queue_preview_layout_settle();
        }
    }

    /// Apply current Focus Mode settings to every open editor tab.
    ///
    /// Called from tab selection and document load as well as from this
    /// workflow, so a newly created or restored page matches the shell at once.
    pub(super) fn apply_focus_mode_to_editors(&self) {
        chrome::apply_to_editors(self);
    }

    /// Enter Focus Mode, capturing the shell state that exit must restore.
    fn enter_focus_mode(&self) {
        let imp = self.imp();
        if imp.focus_mode.active.get() {
            return;
        }

        // Stage 1 — capture.
        let entry = FocusModeEntry {
            was_fullscreen: self.is_fullscreen(),
            had_side_by_side_preview: imp.preview_visible.get(),
        };
        imp.focus_mode.active.set(true);
        imp.focus_mode
            .was_fullscreen_on_entry
            .set(entry.was_fullscreen);
        imp.focus_mode
            .restore_side_by_side_preview
            .set(entry.had_side_by_side_preview);
        imp.focus_mode.preview_changed_while_focused.set(false);

        // Stage 2 — take.
        if !entry.was_fullscreen {
            self.fullscreen();
        }
        if entry.had_side_by_side_preview {
            self.set_preview_pane_visible_for_focus_mode(false);
        }

        // Stage 3 — present.
        self.present_focus_mode_state();
        self.reveal_focus_mode_affordance_temporarily();
        self.announce_workflow_update(
            AnnouncementLane::StatusUpdate,
            "focus-mode-on",
            "Focus mode on",
        );
        if let Some(editor) = self.active_editor() {
            editor.source_view().grab_focus();
        }
    }

    /// Leave Focus Mode, restoring only the shell state this mode borrowed.
    fn exit_focus_mode(&self) {
        let imp = self.imp();
        if !imp.focus_mode.active.get() {
            return;
        }

        // Stage 1 — decide, before any state is cleared.
        let entry = FocusModeEntry {
            was_fullscreen: imp.focus_mode.was_fullscreen_on_entry.get(),
            had_side_by_side_preview: imp.focus_mode.restore_side_by_side_preview.get(),
        };
        let restore =
            policy::exit_restoration(entry, imp.focus_mode.preview_changed_while_focused.get());

        // Stage 2 — give back.
        imp.focus_mode.active.set(false);
        let _ = imp.focus_mode.affordance_timer.invalidate();
        chrome::set_affordance_revealed(self, false);
        self.set_preview_mode_for_focus_mode(false);
        if restore.side_by_side_preview {
            self.set_preview_pane_visible_for_focus_mode(true);
        }
        if restore.leave_fullscreen && self.is_fullscreen() {
            self.unfullscreen();
        }

        // Stage 3 — present.
        self.present_focus_mode_state();
        self.sync_focus_mode_action_state();
        self.announce_workflow_update(
            AnnouncementLane::StatusUpdate,
            "focus-mode-off",
            "Focus mode off",
        );
    }

    /// The presentation stage both directions share.
    ///
    /// Written once because entry and exit must leave the shell in states that
    /// are exact mirrors; two copies would let one direction gain a step the
    /// other never reverses, which is the class of bug this workflow is entirely
    /// about.
    fn present_focus_mode_state(&self) {
        chrome::apply_chrome(self);
        self.sync_focus_mode_action_state();
        chrome::apply_to_editors(self);
        self.refresh_focus_mode_preview_column();
        self.sync_secondary_surface_layout();
    }

    /// Reveal the affordance, then hide it unless focus stays inside it.
    ///
    /// The inversion named in the module doc: control resumes in the superseding
    /// timer's callback one budget later.
    pub(super) fn reveal_focus_mode_affordance_temporarily(&self) {
        let imp = self.imp();
        if !imp.focus_mode.active.get() {
            return;
        }
        chrome::set_affordance_revealed(self, true);

        imp.focus_mode.affordance_timer.arm(
            self,
            FOCUS_MODE_AFFORDANCE_REVEAL_BUDGET,
            move |window, _| {
                if policy::timed_hide_hides_affordance(chrome::affordance_contains_focus(&window)) {
                    chrome::set_affordance_revealed(&window, false);
                }
            },
        );
    }

    /// Mirror the internal Focus Mode state back to the stateful window action.
    fn sync_focus_mode_action_state(&self) {
        self.set_focus_mode_action_state(self.is_focus_mode_active());
    }

    /// Request or mirror Focus Mode through the stateful window action.
    ///
    /// Shared transient Escape handling uses this so mode exit follows the same
    /// action path as menus and shortcuts, including fullscreen, chrome, and
    /// editor-presentation side effects.
    pub(super) fn set_focus_mode_action_state(&self, active: bool) {
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

/// Whether the Markdown preview currently takes Focus Mode's readable column.
///
/// Shared by the refresh path and the evidence surface so a probe reports what
/// the preview was actually told rather than a parallel derivation.
fn preview_readable_column_applies(window: &LushtextWindow) -> bool {
    policy::preview_takes_readable_column(
        window.is_focus_mode_active(),
        window.imp().preview_mode.get(),
    )
}
