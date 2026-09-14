// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: evidence surface — the adaptive shell geometry workflow's single
//! observable state.
//!
//! One accessor replaces the row's single `*_for_test` inspection function
//! (`workspace_sidebar_transition_pending_for_test`) and, more importantly,
//! gives the workflow's **persistence discipline** an observation path it never
//! had. The rule that allocation-time paths clamp but never persist was
//! previously checkable only by reading the code or by watching an animation
//! stutter in the installed Flatpak; `persisted_workspace_fraction` and
//! `persisted_properties_fraction` make it assertable, because a test can drive
//! a resize and require the persisted value **not** to have moved.
//!
//! * **Reading must not mutate.** Every field is a `Cell`/`RefCell` read, a
//!   widget-property read, a GSettings read, or a pure derivation. Nothing here
//!   calls a `sync_*` function, sets a breakpoint condition, or arms a settle
//!   burst.
//! * **No field may be read from inside a mutable borrow.** The only `RefCell`
//!   this surface touches is `properties_breakpoint`, and the borrow is scoped
//!   and dropped before the returned struct literal is built.
//! * **A disposed widget is a stage.** Both split views and the layout view are
//!   template children, reached through `try_get()` so a teardown observation
//!   cannot become a crash. This is not a precaution: the first draft of this
//!   surface called the workflow's own `rendered_workspace_sidebar_visible()`,
//!   `rendered_document_properties_visible()`, and
//!   `document_properties_uses_bottom_sheet()`, each of which **derefs** a
//!   `TemplateChild` through the panicking accessor, and the disposal proof
//!   caught it as a real panic — *"Failed to retrieve template child"* — before
//!   it shipped. Those three accessors stay as they are for production, which
//!   only ever calls them on a live window; the surface derives the same facts
//!   defensively instead.
//!
//! **No materialization.** Nothing here walks a lazily created toolkit
//! collection; the fields are scalars and widget properties.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::config::keys;

use super::super::LushtextWindow;
use super::execution;

/// Everything a test or probe may observe about the adaptive shell geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct ShellGeometryEvidence {
    /// Whether the user last explicitly left the workspace sidebar open.
    pub workspace_requested_visible: bool,
    /// Whether the workspace pane is actually showing right now.
    pub workspace_rendered_visible: bool,
    /// Whether the user last explicitly left document properties open.
    pub properties_requested_visible: bool,
    /// Whether the properties surface is actually showing right now.
    pub properties_rendered_visible: bool,
    /// Whether properties are presented as a compact bottom sheet.
    pub properties_uses_bottom_sheet: bool,
    /// Live workspace sidebar width fraction on the split view.
    pub workspace_fraction: f64,
    /// Live properties sidebar width fraction on the split view.
    pub properties_fraction: f64,
    /// The **persisted** workspace fraction in GSettings.
    ///
    /// Separate from the live fraction on purpose: an allocation tick may move
    /// the live value and must never move this one.
    pub persisted_workspace_fraction: f64,
    /// The **persisted** properties fraction in GSettings, for the same reason.
    pub persisted_properties_fraction: f64,
    /// Cached properties-breakpoint threshold. `set_condition()` runs only when
    /// this integer actually changes, so a test can assert it stayed put across
    /// an animation.
    pub properties_breakpoint_max_width: i32,
    /// Whether a properties breakpoint is installed at all.
    pub properties_breakpoint_installed: bool,
    /// Allocation width the split-view sync last completed for.
    pub split_width_synced_for_width: i32,
    /// Whether a split-view width sync is re-entrant right now.
    pub split_width_syncing: bool,
    /// Whether the sidebar transition still blocks geometry readiness.
    ///
    /// This is the `workspace-sidebar-animation` readiness blocker's own state,
    /// which stays with the **animation** rather than with this row's name.
    pub workspace_sidebar_transition_pending: bool,
    /// Live window width the derivations are running against.
    pub window_width: i32,
}

/// Read the whole adaptive shell geometry surface.
#[must_use]
pub fn shell_geometry_evidence(window: &LushtextWindow) -> ShellGeometryEvidence {
    let imp = window.imp();
    let settings = &imp.settings;

    // Scoped so the borrow is released before the struct literal below.
    let properties_breakpoint_installed = imp.properties_breakpoint.borrow().is_some();

    let workspace_split = imp.workspace_split_view.try_get();
    let properties_split = imp.properties_split_view.try_get();
    let properties_sheet = imp.properties_bottom_sheet.try_get();

    let workspace_fraction = workspace_split
        .as_ref()
        .map_or(0.0, libadwaita::OverlaySplitView::sidebar_width_fraction);
    let properties_fraction = properties_split
        .as_ref()
        .map_or(0.0, libadwaita::OverlaySplitView::sidebar_width_fraction);

    // A disposed shell has no presentation, so every rendered fact is neutral.
    // The *requested* facts stay honest either way: they are `Cell` reads on the
    // workflow's own state, which outlives the widgets.
    let shell_alive = workspace_split.is_some() && imp.properties_layout_view.try_get().is_some();
    let properties_uses_bottom_sheet =
        shell_alive && execution::properties_surface_is_compact(window);
    let properties_rendered_visible = if !shell_alive {
        false
    } else if properties_uses_bottom_sheet {
        properties_sheet
            .as_ref()
            .is_some_and(libadwaita::BottomSheet::is_open)
    } else {
        properties_split
            .as_ref()
            .is_some_and(libadwaita::OverlaySplitView::shows_sidebar)
    };

    ShellGeometryEvidence {
        workspace_requested_visible: window.workspace_sidebar_requested_visible(),
        workspace_rendered_visible: workspace_split
            .as_ref()
            .is_some_and(libadwaita::OverlaySplitView::shows_sidebar),
        properties_requested_visible: window.document_properties_requested_visible(),
        properties_rendered_visible,
        properties_uses_bottom_sheet,
        workspace_fraction,
        properties_fraction,
        persisted_workspace_fraction: settings.double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION),
        persisted_properties_fraction: settings.double(keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION),
        properties_breakpoint_max_width: imp.properties_breakpoint_max_width.get(),
        properties_breakpoint_installed,
        split_width_synced_for_width: imp.split_width_synced_for_width.get(),
        split_width_syncing: imp.split_width_syncing.get(),
        workspace_sidebar_transition_pending: window.workspace_sidebar_transition_pending(),
        window_width: execution::current_window_width(window),
    }
}
