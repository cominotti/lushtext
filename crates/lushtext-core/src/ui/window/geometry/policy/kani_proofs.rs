// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over the adaptive-shell policy: the secondary-surface
//! layout, the document-properties breakpoint, and the pane shares the
//! split-view fractions are formed from.
//!
//! Domain: every `i32` window width, every workspace preset, every combination
//! of requested visibility and Focus Mode, and every compact-surface choice.
//! The policy is whole-pixel (widths in, widths and shares out; the fraction is
//! formed once in the GTK adapter). The breakpoint harness takes every
//! workspace width in [0, 440] sp,
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
    AdaptiveShellInputs, DUAL_PANE_LAYOUT_OVERHEAD_SP, MIN_EDITOR_CONTENT_WIDTH_SP,
    PROPERTIES_SIDEBAR_MIN_WIDTH_SP, PaneShare, PropertiesPresentation,
    WORKSPACE_BREAKPOINT_MAX_WIDTH_SP, derive_adaptive_shell_layout, desired_properties_share,
    effective_properties_share, effective_workspace_sidebar_width_sp,
    properties_breakpoint_max_width_sp, workspace_sidebar_share,
};
use crate::ui::sidebar::width_preset::WorkspaceSidebarWidthPreset;

/// The largest workspace width any preset can produce: the `Large` maximum.
const MAX_WORKSPACE_WIDTH_SP: i32 = WorkspaceSidebarWidthPreset::Large.max_width_sp();

/// Any input the shell can hand the policy.
fn any_inputs() -> AdaptiveShellInputs {
    AdaptiveShellInputs {
        window_width: kani::any(),
        workspace_preset: kani::any(),
        workspace_requested_visible: kani::any(),
        properties_requested_visible: kani::any(),
        compact_surface: kani::any(),
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
    let narrow: i32 = kani::any();
    let wide: i32 = kani::any();
    kani::assume((0..=MAX_WORKSPACE_WIDTH_SP).contains(&narrow));
    kani::assume((0..=MAX_WORKSPACE_WIDTH_SP).contains(&wide));
    kani::assume(narrow <= wide);
    let narrow_threshold = properties_breakpoint_max_width_sp(narrow);
    assert!(narrow_threshold <= properties_breakpoint_max_width_sp(wide));
    let floor = MIN_EDITOR_CONTENT_WIDTH_SP
        + DUAL_PANE_LAYOUT_OVERHEAD_SP
        + narrow
        + PROPERTIES_SIDEBAR_MIN_WIDTH_SP;
    assert!(narrow_threshold >= floor);
}

/// Every pane share has a positive width and a positive denominator, for every
/// `i32` width, preset, and intent, so the adapter's one division yields a
/// finite split fraction in (0, 1]; a share taken of the inner split has a
/// denominator narrower than the window.
#[kani::proof]
fn pane_shares_are_positive() {
    let input = any_inputs();
    let positive = |share: PaneShare| share.width_sp >= 1 && share.of_sp >= 1;
    assert!(positive(desired_properties_share(input.window_width)));
    assert!(positive(workspace_sidebar_share(input)));
    let properties = effective_properties_share(input);
    assert!(positive(properties));
    if derive_adaptive_shell_layout(input).workspace_consumes_width {
        assert!(properties.of_sp < input.window_width);
    }
}

/// No function in the module panics on any input in the domain.
#[kani::proof]
fn shell_policy_never_panics() {
    let input = any_inputs();
    let _ = derive_adaptive_shell_layout(input);
    let _ = effective_workspace_sidebar_width_sp(input);
    let _ = workspace_sidebar_share(input);
    let _ = effective_properties_share(input);
    let _ = desired_properties_share(input.window_width);
}
