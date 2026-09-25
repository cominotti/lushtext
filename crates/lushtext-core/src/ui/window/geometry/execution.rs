// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination (`execution`) for `WFR-SHELL-GEOMETRY`.
//!
//! This module owns the workflow's synchronization machinery: restoring the two
//! split views from GSettings under a pre-clamp, installing and re-tuning the
//! adaptive breakpoints, and reconciling the rendered surfaces with the
//! requested state on every allocation.
//!
//! # The contracts this module must not break
//!
//! These are quoted from `.agents/rules/ui.md` and
//! `.agents/rules/widget-wiring.md` because a role move is exactly the kind of
//! change that preserves the code and loses the rule:
//!
//! * **Allocation-time sync is for live geometry only.** `size_allocate()` may
//!   clamp the current fractions and update a cached properties-breakpoint
//!   threshold, but it must **not** write `workspace-sidebar-width-fraction` or
//!   `properties-sidebar-width-fraction` to GSettings, and must not call
//!   `AdwBreakpoint::set_condition()` with a newly parsed condition on every
//!   animation frame. Persistence stays tied to explicit user intent, restore,
//!   or animation completion. This is the rule the Flatpak animation regression
//!   was traced to, and it is the one a move is most likely to break silently.
//! * **The derived breakpoint threshold is cached**, and `set_condition()` runs
//!   only when that integer threshold actually changes.
//! * **A `SettleBurst` guarding a toolkit animation is armed *before* the
//!   animated property is set**, because GTK/Libadwaita notify signals can run
//!   synchronously from the setter.
//! * **Width presets clamp to the live allocation** before the effective split
//!   fraction is derived.
//!
//! The pure arithmetic behind all of it is in this workflow's `policy.rs`, which
//! imports no toolkit crate; this module supplies live inputs and applies the
//! returned decision.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{gio, glib};
use libadwaita::prelude::AdwApplicationWindowExt;

use crate::config::keys;
use crate::ui::sidebar::width_preset::WorkspaceSidebarWidthPreset;

use super::super::imp::PREVIEW_LAYOUT_EDITOR;
use super::policy::{
    self, AdaptiveShellInputs, AdaptiveShellLayout, OPEN_BUTTON_BREAKPOINT_MAX_WIDTH_SP,
    PROPERTIES_SIDEBAR_MIN_WIDTH_SP, PaneShare, PropertiesPresentation, RenderedShellState,
    ShellReconciliation, ShellWrite, WORKSPACE_SIDEBAR_MIN_WIDTH_SP, derive_adaptive_shell_layout,
    plan_shell_reconciliation, properties_breakpoint_condition, workspace_breakpoint_condition,
};
use crate::ui::markdown_preview::PREVIEW_MIN_WIDTH_SP;

/// Turn a pane's whole-sp share into the fraction an `AdwOverlaySplitView`
/// takes. The one place the shell geometry becomes `f64`: the policy decides
/// widths in whole sp, and this adapter divides once, capped at the whole split.
/// The result is finite and in (0, 1] for every share whose width is positive,
/// which the policy guarantees (`split_fraction_is_finite_and_in_unit_interval`).
pub(in crate::ui::window) fn split_fraction(share: PaneShare) -> f64 {
    (f64::from(share.width_sp) / f64::from(share.of_sp.max(1))).min(1.0)
}

/// The properties pane's window-relative fraction, the value stored in
/// `properties-sidebar-width-fraction`.
pub(in crate::ui::window) fn desired_properties_fraction(window_width: i32) -> f64 {
    split_fraction(policy::desired_properties_share(window_width))
}

pub(in crate::ui::window) fn configure_split_views(
    workspace_split_view: &libadwaita::OverlaySplitView,
    properties_layout_view: &libadwaita::MultiLayoutView,
    properties_split_view: &libadwaita::OverlaySplitView,
    properties_bottom_sheet: &libadwaita::BottomSheet,
    preview_layout_view: &libadwaita::MultiLayoutView,
    preview_split_view: &libadwaita::OverlaySplitView,
) {
    workspace_split_view.set_sidebar_position(gtk4::PackType::Start);
    workspace_split_view.set_sidebar_width_unit(libadwaita::LengthUnit::Sp);
    workspace_split_view.set_min_sidebar_width(f64::from(WORKSPACE_SIDEBAR_MIN_WIDTH_SP));
    workspace_split_view.set_max_sidebar_width(f64::from(WORKSPACE_SIDEBAR_MIN_WIDTH_SP));
    workspace_split_view.set_pin_sidebar(true);
    workspace_split_view.set_enable_show_gesture(false);
    workspace_split_view.set_enable_hide_gesture(false);

    properties_layout_view.set_layout_name(PropertiesPresentation::Pane.layout_name());

    properties_split_view.set_sidebar_position(gtk4::PackType::End);
    properties_split_view.set_sidebar_width_unit(libadwaita::LengthUnit::Sp);
    properties_split_view.set_min_sidebar_width(f64::from(PROPERTIES_SIDEBAR_MIN_WIDTH_SP));
    properties_split_view.set_pin_sidebar(true);
    properties_split_view.set_enable_show_gesture(false);
    properties_split_view.set_enable_hide_gesture(false);

    preview_layout_view.set_layout_name(PREVIEW_LAYOUT_EDITOR);
    preview_split_view.set_sidebar_position(gtk4::PackType::End);
    preview_split_view.set_sidebar_width_unit(libadwaita::LengthUnit::Sp);
    preview_split_view.set_min_sidebar_width(f64::from(PREVIEW_MIN_WIDTH_SP));
    preview_split_view.set_max_sidebar_width(f64::from(PREVIEW_MIN_WIDTH_SP));
    preview_split_view.set_pin_sidebar(true);
    preview_split_view.set_enable_show_gesture(false);
    preview_split_view.set_enable_hide_gesture(false);
    preview_split_view.set_show_sidebar(false);

    // The compact presentation is driven only by the same window action that
    // owns the wide pane. Disabling swipe open/close keeps that requested
    // visibility state deterministic.
    properties_bottom_sheet.set_can_open(false);
    properties_bottom_sheet.set_can_close(false);
    properties_bottom_sheet.set_full_width(true);
    properties_bottom_sheet.set_modal(false);
}

pub(in crate::ui::window) fn migrate_split_view_settings(
    settings: &gio::Settings,
    restored_width: i32,
) {
    if settings.boolean(keys::SPLIT_VIEW_LAYOUT_MIGRATED) {
        return;
    }

    let width = restored_width.max(1);
    let legacy_visible = if settings.user_value(keys::SIDEBAR_VISIBLE).is_some() {
        settings.boolean(keys::SIDEBAR_VISIBLE)
    } else {
        true
    };
    let workspace_fraction = WorkspaceSidebarWidthPreset::DEFAULT.fraction();
    let properties_fraction = desired_properties_fraction(width);

    let _ = settings.set_boolean(keys::WORKSPACE_SIDEBAR_VISIBLE, legacy_visible);
    let _ = settings.set_double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION, workspace_fraction);
    let _ = settings.set_boolean(keys::PROPERTIES_SIDEBAR_VISIBLE, false);
    let _ = settings.set_double(keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION, properties_fraction);
    let _ = settings.set_boolean(keys::SPLIT_VIEW_LAYOUT_MIGRATED, true);
}

pub(in crate::ui::window) fn restore_workspace_split_view(window: &super::LushtextWindow) {
    let width = current_window_width(window);
    let visible = window
        .imp()
        .settings
        .boolean(keys::WORKSPACE_SIDEBAR_VISIBLE);
    let preset = workspace_sidebar_preset(window);
    let fraction = split_fraction(policy::workspace_preset_share(preset, width));
    sync_workspace_sidebar_width_constraints(window, width);
    window.imp().split_width_synced_for_width.set(width);
    window
        .imp()
        .workspace_split_view
        .set_sidebar_width_fraction(fraction);
    let _ = window
        .imp()
        .settings
        .set_double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION, preset.fraction());
    window
        .imp()
        .secondary_surfaces
        .workspace_requested_visible
        .set(visible);
}

pub(in crate::ui::window) fn restore_properties_split_view(window: &super::LushtextWindow) {
    let width = current_window_width(window);
    let visible = window
        .imp()
        .settings
        .boolean(keys::PROPERTIES_SIDEBAR_VISIBLE);
    let fraction = effective_properties_fraction(window, width);
    window
        .imp()
        .properties_split_view
        .set_sidebar_width_fraction(fraction);
    let _ = window.imp().settings.set_double(
        keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION,
        desired_properties_fraction(width),
    );
    window
        .imp()
        .secondary_surfaces
        .properties_requested_visible
        .set(visible);
    sync_properties_breakpoint(window);
    sync_secondary_surfaces(window);
}

pub(in crate::ui::window) fn install_split_view_breakpoints(window: &super::LushtextWindow) {
    let properties_max_width = properties_breakpoint_max_width_for_window(window);
    window
        .imp()
        .properties_breakpoint_max_width
        .set(properties_max_width);
    let properties_bp = libadwaita::Breakpoint::new(
        libadwaita::BreakpointCondition::parse(&properties_breakpoint_condition(
            properties_max_width,
        ))
        .expect("valid properties breakpoint condition"),
    );
    // The breakpoint triggers the reconciliation instead of carrying a
    // `layout-name` setter, so the plan is the property's only writer. A setter
    // was a second writer with a different predicate: under text scaling it
    // applied where the policy keeps the pane (A14), and on unapply it restored
    // the install-time value over the reconciliation's write (A16), so one
    // allocation flipped the layout and back (`shell_loop_layout_setter_flaps`).
    // The signals run inside the bin's allocation, where the setter ran.
    let reconcile = |window_weak: glib::WeakRef<super::LushtextWindow>| {
        move |_: &libadwaita::Breakpoint| {
            if let Some(window) = window_weak.upgrade() {
                sync_secondary_surfaces(&window);
            }
        }
    };
    properties_bp.connect_apply(reconcile(window.downgrade()));
    properties_bp.connect_unapply(reconcile(window.downgrade()));
    window
        .imp()
        .properties_breakpoint
        .replace(Some(properties_bp.clone()));
    window.add_breakpoint(properties_bp);

    let workspace_bp = libadwaita::Breakpoint::new(
        libadwaita::BreakpointCondition::parse(&workspace_breakpoint_condition())
            .expect("valid workspace breakpoint condition"),
    );
    workspace_bp.add_setter(
        window
            .imp()
            .workspace_split_view
            .upcast_ref::<glib::Object>(),
        "collapsed",
        Some(&true.to_value()),
    );
    window.add_breakpoint(workspace_bp);

    let open_button_bp = libadwaita::Breakpoint::new(
        libadwaita::BreakpointCondition::parse(&properties_breakpoint_condition(
            OPEN_BUTTON_BREAKPOINT_MAX_WIDTH_SP,
        ))
        .expect("valid Open button breakpoint condition"),
    );
    open_button_bp.add_setter(
        window.imp().open_button_stack.upcast_ref::<glib::Object>(),
        "visible-child-name",
        Some(&"narrow".to_value()),
    );
    // Only the last added matching breakpoint applies (ledger A18), so where
    // this one matches it displaces the workspace breakpoint. Every width it
    // matches (400sp) also matches the workspace's 860sp, so it collapses the
    // workspace too; without this, a text scale of 1.6 or more uncollapsed the
    // workspace at the narrowest windows.
    open_button_bp.add_setter(
        window
            .imp()
            .workspace_split_view
            .upcast_ref::<glib::Object>(),
        "collapsed",
        Some(&true.to_value()),
    );
    window.add_breakpoint(open_button_bp);
}

/// Build the properties-pane breakpoint from the minimum center width instead
/// of a magic number so the shell explains *why* it collapses earlier.
pub(in crate::ui::window) fn properties_breakpoint_max_width_for_window(
    window: &super::LushtextWindow,
) -> i32 {
    derive_adaptive_shell_layout(adaptive_shell_inputs(window)).properties_breakpoint_max_width
}

pub(in crate::ui::window) fn current_window_width(window: &super::LushtextWindow) -> i32 {
    if window.width() > 0 {
        window.width()
    } else {
        let (w, _) = window.default_size();
        w.max(1)
    }
}

pub(in crate::ui::window) fn workspace_sidebar_preset(
    window: &super::LushtextWindow,
) -> WorkspaceSidebarWidthPreset {
    WorkspaceSidebarWidthPreset::from_fraction(
        window
            .imp()
            .settings
            .double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION),
    )
}

pub(in crate::ui::window) fn adaptive_shell_inputs(
    window: &super::LushtextWindow,
) -> AdaptiveShellInputs {
    adaptive_shell_inputs_for_width(window, current_window_width(window))
}

pub(in crate::ui::window) fn adaptive_shell_inputs_for_width(
    window: &super::LushtextWindow,
    window_width: i32,
) -> AdaptiveShellInputs {
    let imp = window.imp();
    AdaptiveShellInputs {
        window_width,
        workspace_preset: workspace_sidebar_preset(window),
        workspace_requested_visible: imp.secondary_surfaces.workspace_requested_visible.get(),
        properties_requested_visible: imp.secondary_surfaces.properties_requested_visible.get(),
        compact_surface: imp.secondary_surfaces.compact_surface.get(),
        focus_mode_active: imp.focus_mode.active.get(),
    }
}

pub(in crate::ui::window) fn effective_workspace_sidebar_width_sp(
    window: &super::LushtextWindow,
    window_width: i32,
) -> i32 {
    policy::effective_workspace_sidebar_width_sp(adaptive_shell_inputs_for_width(
        window,
        window_width,
    ))
}

pub(in crate::ui::window) fn effective_workspace_sidebar_fraction(
    window: &super::LushtextWindow,
    window_width: i32,
) -> f64 {
    split_fraction(policy::workspace_sidebar_share(
        adaptive_shell_inputs_for_width(window, window_width),
    ))
}

pub(in crate::ui::window) fn sync_workspace_sidebar_width_constraints(
    window: &super::LushtextWindow,
    window_width: i32,
) {
    let target_width_request = effective_workspace_sidebar_width_sp(window, window_width);
    let target_width = f64::from(target_width_request);
    let split = &window.imp().workspace_split_view;
    if (split.min_sidebar_width() - target_width).abs() > f64::EPSILON {
        split.set_min_sidebar_width(target_width);
    }
    if (split.max_sidebar_width() - target_width).abs() > f64::EPSILON {
        split.set_max_sidebar_width(target_width);
    }
    if window.imp().sidebar.width_request() != target_width_request {
        window.imp().sidebar.set_width_request(target_width_request);
    }
}

pub(in crate::ui::window) fn effective_properties_fraction(
    window: &super::LushtextWindow,
    window_width: i32,
) -> f64 {
    split_fraction(policy::effective_properties_share(
        adaptive_shell_inputs_for_width(window, window_width),
    ))
}

pub(in crate::ui::window) fn properties_presentation(
    window: &super::LushtextWindow,
) -> PropertiesPresentation {
    let layout_name = window.imp().properties_layout_view.layout_name();
    PropertiesPresentation::from_layout_name(layout_name.as_deref())
}

pub(in crate::ui::window) fn properties_surface_is_compact(window: &super::LushtextWindow) -> bool {
    properties_presentation(window) == PropertiesPresentation::Sheet
}

pub(in crate::ui::window) fn focus_is_within(
    window: &super::LushtextWindow,
    folder: &gtk4::Widget,
) -> bool {
    let mut focus = gtk4::prelude::GtkWindowExt::focus(window);
    while let Some(widget) = focus {
        if widget.as_ptr() == folder.as_ptr() {
            return true;
        }
        focus = widget.parent();
    }
    false
}

pub(in crate::ui::window) fn set_workspace_sidebar_preset(
    window: &super::LushtextWindow,
    preset: WorkspaceSidebarWidthPreset,
) {
    let fraction = preset.fraction();
    if (window
        .imp()
        .settings
        .double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION)
        - fraction)
        .abs()
        > f64::EPSILON
    {
        let _ = window
            .imp()
            .settings
            .set_double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION, fraction);
    }
    sync_split_view_widths(window, current_window_width(window));
}

/// Read what the shell renders now, for the reconciliation plan.
fn rendered_shell_state(window: &super::LushtextWindow) -> RenderedShellState {
    let imp = window.imp();
    RenderedShellState {
        presentation: properties_presentation(window),
        compact_surface: imp.secondary_surfaces.compact_surface.get(),
        workspace_shows_sidebar: imp.workspace_split_view.shows_sidebar(),
        properties_shows_sidebar: imp.properties_split_view.shows_sidebar(),
        sheet_open: imp.properties_bottom_sheet.is_open(),
    }
}

/// Plan the reconciliation of the rendered shell with the current intent,
/// alongside the rendered state and layout the plan was derived from.
fn current_reconciliation(
    window: &super::LushtextWindow,
) -> (RenderedShellState, AdaptiveShellLayout, ShellReconciliation) {
    let rendered = rendered_shell_state(window);
    let layout = derive_adaptive_shell_layout(adaptive_shell_inputs(window));
    let plan = plan_shell_reconciliation(
        rendered,
        layout,
        window.imp().properties_breakpoint_max_width.get(),
    );
    (rendered, layout, plan)
}

/// Apply one planned write, unless the value already holds.
///
/// The recheck is not a second decision: a `layout-name` write notifies
/// synchronously, and its handler reconciles the whole shell before this plan's
/// remaining writes run, so by then they may already hold.
fn apply_shell_write(window: &super::LushtextWindow, write: ShellWrite) {
    let imp = window.imp();
    match write {
        ShellWrite::LayoutName(presentation) => {
            if properties_presentation(window) != presentation {
                imp.properties_layout_view
                    .set_layout_name(presentation.layout_name());
            }
        }
        ShellWrite::CompactSurface(surface) => {
            if imp.secondary_surfaces.compact_surface.get() != surface {
                imp.secondary_surfaces.compact_surface.set(surface);
            }
        }
        ShellWrite::WorkspaceShowSidebar(show) => {
            if imp.workspace_split_view.shows_sidebar() != show {
                imp.workspace_split_view.set_show_sidebar(show);
            }
        }
        ShellWrite::PropertiesShowSidebar(show) => {
            if imp.properties_split_view.shows_sidebar() != show {
                imp.properties_split_view.set_show_sidebar(show);
            }
        }
        ShellWrite::SheetOpen(open) => {
            if imp.properties_bottom_sheet.is_open() != open {
                imp.properties_bottom_sheet.set_open(open);
            }
        }
    }
}

pub(in crate::ui::window) fn sync_properties_breakpoint(window: &super::LushtextWindow) {
    let Some(max_width) = current_reconciliation(window).2.reinstall_threshold else {
        return;
    };
    let condition =
        libadwaita::BreakpointCondition::parse(&properties_breakpoint_condition(max_width))
            .expect("valid properties breakpoint condition");
    if let Some(breakpoint) = window.imp().properties_breakpoint.borrow().as_ref() {
        breakpoint.set_condition(Some(&condition));
    }
    window.imp().properties_breakpoint_max_width.set(max_width);
}

pub(in crate::ui::window) fn sync_secondary_surfaces(window: &super::LushtextWindow) {
    let imp = window.imp();
    let (rendered, layout, plan) = current_reconciliation(window);
    let was_workspace_visible = rendered.workspace_shows_sidebar;
    let was_properties_visible = window.rendered_document_properties_visible();
    let focus_in_workspace = focus_is_within(window, imp.sidebar.upcast_ref::<gtk4::Widget>());
    let focus_in_properties =
        focus_is_within(window, imp.properties_panel.upcast_ref::<gtk4::Widget>());

    for write in plan.surface_writes().into_iter().flatten() {
        apply_shell_write(window, write);
    }

    window.sync_secondary_surface_action_states();

    if (was_workspace_visible && !layout.render_workspace && focus_in_workspace)
        || (was_properties_visible
            && !layout.render_properties
            && (focus_in_properties || window.active_editor().is_none()))
    {
        window.restore_focus_after_secondary_pane_close();
    }
}

pub(in crate::ui::window) fn sync_properties_split_view(
    window: &super::LushtextWindow,
    window_width: i32,
) {
    let expected = effective_properties_fraction(window, window_width);
    if (window.imp().properties_split_view.sidebar_width_fraction() - expected).abs() > f64::EPSILON
    {
        window
            .imp()
            .properties_split_view
            .set_sidebar_width_fraction(expected);
    }
}

pub(in crate::ui::window) fn sync_split_view_widths_for_allocation(
    window: &super::LushtextWindow,
    window_width: i32,
) {
    if window.imp().split_width_synced_for_width.get() == window_width {
        return;
    }
    sync_split_view_widths(window, window_width);
}

pub(in crate::ui::window) fn sync_split_view_widths(
    window: &super::LushtextWindow,
    window_width: i32,
) {
    if window.imp().split_width_syncing.replace(true) {
        return;
    }

    let workspace_fraction = effective_workspace_sidebar_fraction(window, window_width);
    sync_workspace_sidebar_width_constraints(window, window_width);
    if (window.imp().workspace_split_view.sidebar_width_fraction() - workspace_fraction).abs()
        > f64::EPSILON
    {
        window
            .imp()
            .workspace_split_view
            .set_sidebar_width_fraction(workspace_fraction);
    }
    window.sync_preview_width_constraints(window_width);
    if !window.imp().workspace_sidebar_transition_settle.pending() {
        sync_properties_breakpoint(window);
        sync_properties_split_view(window, window_width);
        sync_secondary_surfaces(window);
    }

    window.imp().split_width_synced_for_width.set(window_width);
    window.imp().split_width_syncing.set(false);
}

#[cfg(test)]
mod tests {
    use super::split_fraction;
    use crate::ui::sidebar::width_preset::WorkspaceSidebarWidthPreset;
    use crate::ui::window::geometry::policy::{
        PaneShare, desired_properties_share, workspace_preset_share,
    };

    fn in_unit_interval(fraction: f64) -> bool {
        fraction.is_finite() && fraction > 0.0 && fraction <= 1.0
    }

    /// Every share the policy produces has a positive width, so the one
    /// division in the shell geometry yields a split-view fraction that is
    /// finite and in (0, 1], across the whole `i32` width range.
    #[test]
    fn split_fraction_is_finite_and_in_unit_interval() {
        let widths = [
            i32::MIN,
            -1,
            0,
            1,
            2,
            279,
            280,
            860,
            861,
            1119,
            1120,
            1350,
            1920,
            3840,
            i32::MAX - 1,
            i32::MAX,
        ];
        for width in widths.into_iter().chain((1..5_000).step_by(7)) {
            assert!(
                in_unit_interval(split_fraction(desired_properties_share(width))),
                "{width}"
            );
            for preset in WorkspaceSidebarWidthPreset::ALL {
                let share = workspace_preset_share(preset, width);
                assert!(share.width_sp >= 1, "{width} {preset:?}");
                assert!(
                    in_unit_interval(split_fraction(share)),
                    "{width} {preset:?}"
                );
            }
        }
        // A pane wider than its split is capped at the whole split.
        assert!(
            (split_fraction(PaneShare {
                width_sp: 500,
                of_sp: 100
            }) - 1.0)
                .abs()
                < f64::EPSILON
        );
    }
}
