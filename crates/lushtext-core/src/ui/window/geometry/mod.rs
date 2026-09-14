// SPDX-License-Identifier: GPL-3.0-or-later

//! The adaptive shell geometry workflow (`WFR-SHELL-GEOMETRY`) — narrative facade.
//!
//! A family of operations sharing one ordered stage sequence: **read the
//! requested state and the live width -> derive the effective presentation ->
//! apply it to the two split views -> settle, and only then persist**. The
//! entry points are the workspace-sidebar toggle, the document-properties
//! toggle, a window resize, and a restore at construction; all four run that
//! same sequence, which is why they are one row.
//!
//! # Role home
//!
//! **Per-workflow subdirectory** `ui/window/geometry/`. `ui/window/` hosts many
//! workflows and the flat `policy.rs` / `evidence.rs` names are not available
//! there.
//!
//! | Module | Role |
//! | --- | --- |
//! | `mod.rs` (this file) | narrative facade |
//! | `policy.rs` | pure policy — the effective-fraction arithmetic, the preset clamp, the properties presentation choice, and the derived breakpoint threshold |
//! | `execution.rs` | coordination (`execution`) — restore, breakpoint installation, and allocation-time reconciliation |
//! | `evidence.rs` | evidence surface (`test-utils`-gated; production reads live state directly) |
//!
//! The GTK adapter halves that stay behind are **deliberate**, not residue:
//! `ui/window/imp.rs` keeps the `size_allocate` vfunc (a subclass override GTK
//! calls, which cannot move) and delegates into `execution.rs`; `ui/window/actions.rs`
//! keeps the two toggle action bodies, which persist user intent and announce
//! it before handing the presentation decision to this facade.
//!
//! # The persistence rule
//!
//! The single most important thing about this workflow is **what does not
//! happen on an allocation tick**. Allocation and programmatic-notify paths
//! clamp runtime geometry and cache the derived breakpoint threshold; they
//! never write a width fraction to GSettings and never reparse an
//! `AdwBreakpoint` condition. Persistence runs only from explicit user intent,
//! restore, or animation completion. Breaking that does not fail a widget test:
//! it shows up as sidebar and properties animations running below the monitor
//! refresh rate in the installed Flatpak, which is how it was found.
//!
//! # The inversion
//!
//! The sidebar show/hide transition is a toolkit-owned animation. The facade's
//! `start_workspace_sidebar_transition` **arms the settle burst before** setting
//! the animated property — Libadwaita can emit the notify synchronously from
//! the setter, so arming afterwards would let same-frame reconciliation run
//! before the guard exists — and then control leaves. It resumes in the settle
//! burst's completion, which is also what the `workspace-sidebar-animation`
//! readiness blocker reports on. That blocker stays with the **animation**, not
//! with this row's name.
//!
//! # Absences, recorded as conclusions
//!
//! * **No seam value object beyond `AdaptiveShellInputs`.** That bundle is the
//!   reified seam: it is constructed once per reconciliation and validated as a
//!   unit by the policy module. Nothing else here crosses two boundaries.
//! * **No `test_policy.rs`.** The workflow has no test-only timing or limit
//!   override; the animation duration is production policy.

pub mod policy;

pub(super) mod execution;

#[cfg(feature = "test-utils")]
pub mod evidence;

use glib::object::ObjectExt;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use super::LushtextWindow;
use super::editor_focus::{EDITOR_FOCUS_MAX_ATTEMPTS, EDITOR_FOCUS_RETRY_INTERVAL};
use super::imp::WORKSPACE_SIDEBAR_TRANSITION_SETTLE_DELAY_MS;
use execution::{
    adaptive_shell_inputs_for_width, current_window_width, properties_surface_is_compact,
    sync_properties_breakpoint, sync_properties_split_view, sync_secondary_surfaces,
    sync_workspace_sidebar_width_constraints,
};
use policy::derive_adaptive_shell_layout;

impl LushtextWindow {
    /// Return whether the workspace sidebar is requested in user state.
    pub(crate) fn workspace_sidebar_requested_visible(&self) -> bool {
        self.imp()
            .secondary_surfaces
            .workspace_requested_visible
            .get()
    }

    /// Return whether document properties are requested in user state.
    pub(crate) fn document_properties_requested_visible(&self) -> bool {
        self.imp()
            .secondary_surfaces
            .properties_requested_visible
            .get()
    }

    /// Return whether the workspace sidebar is currently rendered on screen.
    pub(crate) fn rendered_workspace_sidebar_visible(&self) -> bool {
        self.imp().workspace_split_view.shows_sidebar()
    }

    /// Return whether document properties are currently rendered on screen.
    pub(crate) fn rendered_document_properties_visible(&self) -> bool {
        if self.document_properties_uses_bottom_sheet() {
            self.imp().properties_bottom_sheet.is_open()
        } else {
            self.imp().properties_split_view.shows_sidebar()
        }
    }

    /// Return whether document properties currently use the compact sheet presentation.
    pub(crate) fn document_properties_uses_bottom_sheet(&self) -> bool {
        properties_surface_is_compact(self)
    }

    // --- The sequence's final stage: give focus back ------------------------
    //
    // `.agents/rules/ui.md` makes focus return part of the split-view contract:
    // *"When a utility pane closes, return focus to the active editor rather
    // than leaving focus stranded on a toggle button."* The geometry sequence is
    // incomplete without it, which is why these two live here rather than with
    // the file they were extracted from. Each has exactly one caller and exists
    // only for a geometry transition; the argument is that behaviour, not the
    // caller relationship.
    //
    // Both defer through `idle_add_local_once`: the pane has to finish leaving
    // layout before there is a focusable editor to hand focus to. That is the
    // row's second inversion, and control resumes inside the idle callback.

    /// Return focus to the active editor after a split-view pane closes.
    pub(super) fn restore_focus_after_secondary_pane_close(&self) {
        let window_weak = self.downgrade();
        glib::idle_add_local_once(move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            if let Some(editor) = window.active_editor() {
                gtk4::prelude::GtkWindowExt::set_focus(
                    &window,
                    Some(editor.source_view().upcast_ref::<gtk4::Widget>()),
                );
                editor.source_view().grab_focus();
            } else {
                gtk4::prelude::GtkWindowExt::set_focus(&window, gtk4::Widget::NONE);
            }
        });
    }

    /// Return focus to the active editor after an adaptive breakpoint collapse.
    ///
    /// Breakpoint-driven split-view collapse can clear focus more than once as
    /// GTK settles the new adaptive layout, so retry a few short ticks until
    /// the active editor successfully owns focus again.
    pub(super) fn restore_focus_after_breakpoint_collapse(&self) {
        let window_weak = self.downgrade();
        let attempts = std::rc::Rc::new(std::cell::Cell::new(0u8));
        let attempts_clone = attempts;

        glib::timeout_add_local(EDITOR_FOCUS_RETRY_INTERVAL, move || {
            let Some(window) = window_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };

            let Some(editor) = window.active_editor() else {
                gtk4::prelude::GtkWindowExt::set_focus(&window, gtk4::Widget::NONE);
                return glib::ControlFlow::Break;
            };

            let source_view = editor.source_view();
            let source_ptr = source_view.upcast_ref::<gtk4::Widget>().as_ptr();
            gtk4::prelude::GtkWindowExt::set_focus(
                &window,
                Some(source_view.upcast_ref::<gtk4::Widget>()),
            );
            source_view.grab_focus();

            let focused = gtk4::prelude::GtkWindowExt::focus(&window).map(|widget| widget.as_ptr())
                == Some(source_ptr);
            let next_attempt = attempts_clone.get().saturating_add(1);
            attempts_clone.set(next_attempt);

            if focused || next_attempt >= EDITOR_FOCUS_MAX_ATTEMPTS {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    /// Recompute the adaptive properties host after any explicit visibility change.
    pub(super) fn sync_secondary_surface_layout(&self) {
        if self.imp().workspace_sidebar_transition_settle.pending() {
            return;
        }
        self.sync_secondary_surface_layout_now();
    }

    /// Start the Adwaita workspace-sidebar transition before reconciling other surfaces.
    pub(super) fn start_workspace_sidebar_transition(&self) {
        let width = current_window_width(self);
        let layout = derive_adaptive_shell_layout(adaptive_shell_inputs_for_width(self, width));
        sync_workspace_sidebar_width_constraints(self, width);
        let sidebar_will_change =
            self.imp().workspace_split_view.shows_sidebar() != layout.render_workspace;
        if sidebar_will_change {
            self.imp().workspace_sidebar_transition_settle.schedule(
                self,
                std::time::Duration::from_millis(WORKSPACE_SIDEBAR_TRANSITION_SETTLE_DELAY_MS),
                |window, handle| {
                    window.sync_secondary_surface_layout_now();
                    handle.finish_if_current();
                },
            );
            self.imp()
                .workspace_split_view
                .set_show_sidebar(layout.render_workspace);
        }
        self.sync_secondary_surface_action_states();

        if !sidebar_will_change {
            self.sync_secondary_surface_layout_now();
            let _ = self.imp().workspace_sidebar_transition_settle.clear();
        }
    }

    /// Return whether the sidebar transition is still blocking geometry readiness.
    pub(crate) fn workspace_sidebar_transition_pending(&self) -> bool {
        self.imp().workspace_sidebar_transition_settle.pending()
    }

    /// Recompute the adaptive properties host after any explicit visibility change.
    fn sync_secondary_surface_layout_now(&self) {
        let width = current_window_width(self);
        sync_properties_breakpoint(self);
        sync_properties_split_view(self, width);
        sync_secondary_surfaces(self);
    }
}
