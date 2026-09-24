// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over the adaptive-shell policy: the secondary-surface
//! layout, the document-properties breakpoint, and the split-view fractions.
//!
//! Domain: every `i32` window width, every workspace preset, every combination
//! of requested visibility and Focus Mode, and every compact-surface choice.
//! The breakpoint harness takes every finite workspace width in [0, 440] sp,
//! the reachable range: 0 when the workspace does not consume width, and up to
//! the `Large` preset's maximum otherwise.
//!
//! **Independence from rendered state holds by construction** and is recorded
//! here rather than proved: [`AdaptiveShellInputs`] has no rendered-visibility
//! field, so no decision can read what is currently on screen. The feedback
//! loop from an allocated width back into a breakpoint is not modelled here.
//!
//! Compiled only under `cfg(kani)` (`make kani`); ordinary builds and tests
//! never see this module. The harnesses check the functions that ship.

use super::{
    AdaptiveShellInputs, DUAL_PANE_LAYOUT_OVERHEAD_SP, FIXED_PROPERTIES_SIDEBAR_FRACTION,
    MIN_EDITOR_CONTENT_WIDTH_SP, PROPERTIES_SIDEBAR_MIN_WIDTH_SP, PropertiesPresentation,
    SecondarySurface, WORKSPACE_BREAKPOINT_MAX_WIDTH_SP, derive_adaptive_shell_layout,
    desired_properties_fraction, effective_properties_fraction,
    effective_workspace_sidebar_fraction, effective_workspace_sidebar_width_sp, fixed_fraction,
    properties_breakpoint_max_width_sp,
};
use crate::ui::sidebar::width_preset::WorkspaceSidebarWidthPreset;

/// The largest workspace width any preset can produce.
const MAX_WORKSPACE_WIDTH_SP: f64 = 440.0;

/// Any preset.
fn any_preset() -> WorkspaceSidebarWidthPreset {
    let index: u32 = kani::any();
    kani::assume(index < 3);
    WorkspaceSidebarWidthPreset::from_index(index).expect("indices 0..3 name presets")
}

/// Any compact-surface choice, including none.
fn any_compact_surface() -> Option<SecondarySurface> {
    match kani::any::<u8>() % 3 {
        0 => None,
        1 => Some(SecondarySurface::Workspace),
        _ => Some(SecondarySurface::DocumentProperties),
    }
}

/// Any input the shell can hand the policy.
fn any_inputs() -> AdaptiveShellInputs {
    AdaptiveShellInputs {
        window_width: kani::any(),
        workspace_preset: any_preset(),
        workspace_requested_visible: kani::any(),
        properties_requested_visible: kani::any(),
        compact_surface: any_compact_surface(),
        focus_mode_active: kani::any(),
    }
}

/// Focus Mode renders neither secondary surface, whatever was requested.
#[kani::proof]
fn focus_mode_renders_no_secondary_surface() {
    let mut input = any_inputs();
    input.focus_mode_active = true;
    let layout = derive_adaptive_shell_layout(input);
    assert!(!layout.render_workspace && !layout.render_properties);
    assert!(!layout.workspace_consumes_width);
}

/// A surface that is not requested is never rendered.
#[kani::proof]
fn layout_never_renders_an_unrequested_surface() {
    let input = any_inputs();
    let layout = derive_adaptive_shell_layout(input);
    assert!(!layout.render_workspace || input.workspace_requested_visible);
    assert!(!layout.render_properties || input.properties_requested_visible);
}

/// The compact (sheet) presentation renders at most one secondary surface.
#[kani::proof]
fn compact_layout_renders_at_most_one_surface() {
    let input = any_inputs();
    let layout = derive_adaptive_shell_layout(input);
    if layout.properties_presentation == PropertiesPresentation::Sheet {
        assert!(!(layout.render_workspace && layout.render_properties));
    }
}

/// The wide (pane) presentation, above the workspace breakpoint and outside
/// Focus Mode, renders every requested surface.
#[kani::proof]
fn wide_layout_renders_every_requested_surface() {
    let input = any_inputs();
    kani::assume(!input.focus_mode_active);
    kani::assume(input.window_width > WORKSPACE_BREAKPOINT_MAX_WIDTH_SP);
    let layout = derive_adaptive_shell_layout(input);
    if layout.properties_presentation == PropertiesPresentation::Pane {
        assert!(layout.render_workspace == input.workspace_requested_visible);
        assert!(layout.render_properties == input.properties_requested_visible);
    }
}

/// The sheet presentation is chosen exactly when the window width is at or
/// below the derived breakpoint threshold.
#[kani::proof]
fn sheet_presentation_matches_the_breakpoint() {
    let input = any_inputs();
    let layout = derive_adaptive_shell_layout(input);
    let sheet = layout.properties_presentation == PropertiesPresentation::Sheet;
    kani::cover!(sheet, "sheet presentation");
    kani::cover!(!sheet, "pane presentation");
    assert!(sheet == (input.window_width <= layout.properties_breakpoint_max_width));
}

/// The breakpoint never decreases as the workspace width grows, and it is at
/// least the editor-content minimum plus the layout overhead, the workspace
/// width, and the properties minimum.
#[kani::proof]
fn breakpoint_is_monotone_and_bounded() {
    let narrow: f64 = kani::any();
    let wide: f64 = kani::any();
    kani::assume((0.0..=MAX_WORKSPACE_WIDTH_SP).contains(&narrow));
    kani::assume((0.0..=MAX_WORKSPACE_WIDTH_SP).contains(&wide));
    kani::assume(narrow <= wide);
    let narrow_threshold = properties_breakpoint_max_width_sp(narrow);
    assert!(narrow_threshold <= properties_breakpoint_max_width_sp(wide));
    let floor = MIN_EDITOR_CONTENT_WIDTH_SP
        + DUAL_PANE_LAYOUT_OVERHEAD_SP
        + narrow
        + PROPERTIES_SIDEBAR_MIN_WIDTH_SP;
    assert!(f64::from(narrow_threshold) >= floor);
}

/// Every split-view fraction is finite and in (0, 1] for every `i32` width.
#[kani::proof]
fn fractions_are_finite_and_in_unit_interval() {
    let input = any_inputs();
    let in_unit_interval =
        |fraction: f64| fraction.is_finite() && fraction > 0.0 && fraction <= 1.0;
    assert!(in_unit_interval(fixed_fraction(
        input.window_width,
        PROPERTIES_SIDEBAR_MIN_WIDTH_SP,
        FIXED_PROPERTIES_SIDEBAR_FRACTION
    )));
    assert!(in_unit_interval(desired_properties_fraction(
        input.window_width
    )));
    assert!(in_unit_interval(effective_properties_fraction(input)));
    assert!(in_unit_interval(effective_workspace_sidebar_fraction(
        input
    )));
}

/// No function in the module panics on any input in the domain.
#[kani::proof]
fn shell_policy_never_panics() {
    let input = any_inputs();
    let _ = derive_adaptive_shell_layout(input);
    let _ = effective_workspace_sidebar_width_sp(input);
    let _ = effective_workspace_sidebar_fraction(input);
    let _ = effective_properties_fraction(input);
    let _ = desired_properties_fraction(input.window_width);
}
