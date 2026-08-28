// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure policy for the window's adaptive secondary surfaces.
//!
//! The GTK adapter supplies current settings and widget-derived intent, then
//! applies the returned decision. This module never reads GSettings or mutates
//! widgets, so allocation and breakpoint behavior can be verified without GTK.

use crate::ui::sidebar::width_preset::WorkspaceSidebarWidthPreset;

/// Tiny non-zero floor used before the first real workspace-width sync.
pub(super) const WORKSPACE_SIDEBAR_MIN_WIDTH_SP: f64 = 1.0;
/// Properties sidebar minimum width in scale-independent pixels.
pub(super) const PROPERTIES_SIDEBAR_MIN_WIDTH_SP: f64 = 280.0;
/// Minimum normal-mode height that preserves persistent chrome and an editor.
pub(super) const NORMAL_MODE_MIN_HEIGHT_SP: i32 = 360;
/// Collapse the left workspace pane on narrower windows.
pub(super) const WORKSPACE_BREAKPOINT_MAX_WIDTH_SP: i32 = 860;
/// GNOME Text Editor switches the header Open control to an icon at 400sp.
pub(super) const OPEN_BUTTON_BREAKPOINT_MAX_WIDTH_SP: i32 = 400;

/// Target total-window width for the visible right properties pane.
const FIXED_PROPERTIES_SIDEBAR_FRACTION: f64 = 0.25;
/// Minimum center width that keeps restored-document inline alerts stable.
const MIN_EDITOR_CONTENT_WIDTH_SP: f64 = 620.0;
/// Width budget for split separators, padding, and rounding noise.
const DUAL_PANE_LAYOUT_OVERHEAD_SP: f64 = 32.0;
/// Wide document-properties presentation in the multi-layout view.
const PROPERTIES_LAYOUT_PANE: &str = "pane";
/// Compact document-properties presentation in the multi-layout view.
const PROPERTIES_LAYOUT_SHEET: &str = "sheet";

/// Secondary surfaces that can compete for the compact-width slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecondarySurface {
    /// The left workspace sidebar.
    Workspace,
    /// The document-properties surface.
    DocumentProperties,
}

/// Adaptive presentation currently used for document properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PropertiesPresentation {
    /// Properties render as the right sidebar of the inner split view.
    Pane,
    /// Properties render as the sheet of the compact bottom sheet.
    Sheet,
}

impl PropertiesPresentation {
    pub(super) const fn layout_name(self) -> &'static str {
        match self {
            Self::Pane => PROPERTIES_LAYOUT_PANE,
            Self::Sheet => PROPERTIES_LAYOUT_SHEET,
        }
    }

    pub(super) fn from_layout_name(name: Option<&str>) -> Self {
        match name {
            Some(PROPERTIES_LAYOUT_SHEET) => Self::Sheet,
            _ => Self::Pane,
        }
    }
}

/// Stable inputs for one adaptive-shell decision.
#[derive(Clone, Copy, Debug)]
pub(super) struct AdaptiveShellInputs {
    /// Current allocated or restored window width in scale-independent pixels.
    pub(super) window_width: i32,
    /// Workspace width preset selected by the user.
    pub(super) workspace_preset: WorkspaceSidebarWidthPreset,
    /// Whether the user last requested the workspace sidebar open.
    pub(super) workspace_requested_visible: bool,
    /// Whether the user last requested document properties open.
    pub(super) properties_requested_visible: bool,
    /// Which surface was explicitly chosen for the compact slot, if any.
    pub(super) compact_surface: Option<SecondarySurface>,
    /// Focus Mode suppresses secondary surfaces while preserving requests.
    pub(super) focus_mode_active: bool,
}

/// Derived shell geometry and presentation for one stable set of inputs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct AdaptiveShellLayout {
    /// Document-properties breakpoint threshold for the current intent.
    pub(super) properties_breakpoint_max_width: i32,
    /// Whether the workspace consumes side-by-side width in this layout.
    pub(super) workspace_consumes_width: bool,
    /// Resolved document-properties presentation.
    pub(super) properties_presentation: PropertiesPresentation,
    /// Compact surface that should render for this pass.
    pub(super) compact_surface: Option<SecondarySurface>,
    /// Whether the workspace sidebar should be rendered now.
    pub(super) render_workspace: bool,
    /// Whether document properties should be rendered now.
    pub(super) render_properties: bool,
}

pub(super) fn properties_breakpoint_condition(max_width_sp: i32) -> String {
    format!("max-width: {max_width_sp}sp")
}

pub(super) fn workspace_breakpoint_condition() -> String {
    properties_breakpoint_condition(WORKSPACE_BREAKPOINT_MAX_WIDTH_SP)
}

/// Return the preset-clamped workspace target width for this window.
pub(super) fn effective_workspace_sidebar_width_sp(input: AdaptiveShellInputs) -> f64 {
    input.workspace_preset.clamped_width_sp(input.window_width)
}

/// Return the preset-clamped workspace fraction for this window.
pub(super) fn effective_workspace_sidebar_fraction(input: AdaptiveShellInputs) -> f64 {
    input
        .workspace_preset
        .effective_fraction(input.window_width)
}

/// Return the right-properties fraction relative to its current inner split.
pub(super) fn effective_properties_fraction(input: AdaptiveShellInputs) -> f64 {
    let total_fraction = desired_properties_fraction(input.window_width);
    if derive_adaptive_shell_layout(input).workspace_consumes_width {
        let total_width = f64::from(input.window_width.max(1));
        let workspace_width = effective_workspace_sidebar_width_sp(input);
        let remaining_fraction = (1.0 - workspace_width / total_width).max(f64::EPSILON);
        let inner_width = (total_width - workspace_width).max(1.0);
        let lower = (PROPERTIES_SIDEBAR_MIN_WIDTH_SP / inner_width).min(1.0);
        (total_fraction / remaining_fraction).max(lower).min(1.0)
    } else {
        total_fraction
    }
}

pub(super) fn desired_properties_fraction(window_width: i32) -> f64 {
    fixed_fraction(
        window_width,
        PROPERTIES_SIDEBAR_MIN_WIDTH_SP,
        FIXED_PROPERTIES_SIDEBAR_FRACTION,
    )
}

pub(super) fn derive_adaptive_shell_layout(input: AdaptiveShellInputs) -> AdaptiveShellLayout {
    let workspace_consumes_width = workspace_consumes_width_for_intent(input);
    let workspace_width_sp = if workspace_consumes_width {
        effective_workspace_sidebar_width_sp(input)
    } else {
        0.0
    };
    let properties_breakpoint_max_width = properties_breakpoint_max_width_sp(workspace_width_sp);
    let properties_presentation = if input.window_width <= properties_breakpoint_max_width {
        PropertiesPresentation::Sheet
    } else {
        PropertiesPresentation::Pane
    };
    let compact = properties_presentation == PropertiesPresentation::Sheet;
    let workspace_collapsed = input.window_width <= WORKSPACE_BREAKPOINT_MAX_WIDTH_SP;
    let compact_surface = if compact {
        preferred_compact_surface_for_intent(input)
    } else {
        None
    };

    let render_workspace = if input.focus_mode_active {
        false
    } else if workspace_collapsed {
        compact_surface == Some(SecondarySurface::Workspace) && input.workspace_requested_visible
    } else if compact {
        !(compact_surface == Some(SecondarySurface::DocumentProperties)
            && input.properties_requested_visible)
            && input.workspace_requested_visible
    } else {
        input.workspace_requested_visible
    };
    let render_properties = if input.focus_mode_active {
        false
    } else if compact {
        compact_surface == Some(SecondarySurface::DocumentProperties)
            && input.properties_requested_visible
    } else {
        input.properties_requested_visible
    };

    AdaptiveShellLayout {
        properties_breakpoint_max_width,
        workspace_consumes_width,
        properties_presentation,
        compact_surface,
        render_workspace,
        render_properties,
    }
}

fn workspace_consumes_width_for_intent(input: AdaptiveShellInputs) -> bool {
    !input.focus_mode_active
        && input.workspace_requested_visible
        && input.window_width > WORKSPACE_BREAKPOINT_MAX_WIDTH_SP
}

fn preferred_compact_surface_for_intent(input: AdaptiveShellInputs) -> Option<SecondarySurface> {
    if let Some(surface) = input.compact_surface
        && secondary_surface_requested_for_intent(input, surface)
    {
        return Some(surface);
    }

    if input.properties_requested_visible {
        Some(SecondarySurface::DocumentProperties)
    } else {
        None
    }
}

fn secondary_surface_requested_for_intent(
    input: AdaptiveShellInputs,
    surface: SecondarySurface,
) -> bool {
    match surface {
        SecondarySurface::Workspace => input.workspace_requested_visible,
        SecondarySurface::DocumentProperties => input.properties_requested_visible,
    }
}

/// Compute the total width below which properties must stop consuming width.
#[expect(
    clippy::cast_possible_truncation,
    reason = "Stored window geometry is clamped to GTK window dimensions before converting to i32"
)]
fn properties_breakpoint_max_width_sp(workspace_width_sp: f64) -> i32 {
    let center_target = MIN_EDITOR_CONTENT_WIDTH_SP + DUAL_PANE_LAYOUT_OVERHEAD_SP;
    let fraction_guard = dual_sidebar_window_width_for_center(center_target, workspace_width_sp);
    let min_width_guard = center_target + workspace_width_sp + PROPERTIES_SIDEBAR_MIN_WIDTH_SP;
    fraction_guard.max(min_width_guard).ceil() as i32
}

/// Convert a center-width target and workspace width into total window width.
fn dual_sidebar_window_width_for_center(center_width_sp: f64, workspace_width_sp: f64) -> f64 {
    (center_width_sp + workspace_width_sp)
        / (1.0 - FIXED_PROPERTIES_SIDEBAR_FRACTION).max(f64::EPSILON)
}

fn fixed_fraction(window_width: i32, min_width_sp: f64, target_fraction: f64) -> f64 {
    let width = f64::from(window_width.max(1));
    let lower = (min_width_sp / width).min(1.0);
    target_fraction.max(lower).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(window_width: i32) -> AdaptiveShellInputs {
        AdaptiveShellInputs {
            window_width,
            workspace_preset: WorkspaceSidebarWidthPreset::Comfy,
            workspace_requested_visible: true,
            properties_requested_visible: true,
            compact_surface: None,
            focus_mode_active: false,
        }
    }

    #[test]
    fn properties_breakpoint_width_accounts_for_workspace_preset() {
        assert_eq!(
            properties_breakpoint_max_width_sp(WorkspaceSidebarWidthPreset::Comfy.max_width_sp()),
            1350
        );
        assert_eq!(properties_breakpoint_max_width_sp(0.0), 932);
        assert_eq!(
            properties_breakpoint_max_width_sp(WorkspaceSidebarWidthPreset::Small.max_width_sp()),
            1243
        );
        assert_eq!(
            properties_breakpoint_max_width_sp(WorkspaceSidebarWidthPreset::Large.max_width_sp()),
            1456
        );
    }

    #[test]
    fn adaptive_layout_budgets_requested_workspace_even_when_compact_suppresses_it() {
        let layout = derive_adaptive_shell_layout(input(1200));

        assert_eq!(layout.properties_breakpoint_max_width, 1350);
        assert_eq!(
            layout.properties_presentation,
            PropertiesPresentation::Sheet
        );
        assert_eq!(
            layout.compact_surface,
            Some(SecondarySurface::DocumentProperties)
        );
        assert!(!layout.render_workspace);
        assert!(layout.render_properties);
    }

    #[test]
    fn adaptive_layout_does_not_open_workspace_overlay_for_passive_compact_shrink() {
        let mut input = input(837);
        input.properties_requested_visible = false;
        let layout = derive_adaptive_shell_layout(input);

        assert_eq!(
            layout.properties_presentation,
            PropertiesPresentation::Sheet
        );
        assert_eq!(layout.compact_surface, None);
        assert!(!layout.render_workspace);
        assert!(!layout.render_properties);
    }

    #[test]
    fn adaptive_layout_keeps_explicit_compact_workspace_overlay() {
        let mut input = input(837);
        input.properties_requested_visible = false;
        input.compact_surface = Some(SecondarySurface::Workspace);
        let layout = derive_adaptive_shell_layout(input);

        assert_eq!(layout.compact_surface, Some(SecondarySurface::Workspace));
        assert!(layout.render_workspace);
        assert!(!layout.render_properties);
    }

    #[test]
    fn wide_layout_renders_both_requested_surfaces() {
        let layout = derive_adaptive_shell_layout(input(1800));

        assert_eq!(layout.properties_presentation, PropertiesPresentation::Pane);
        assert_eq!(layout.compact_surface, None);
        assert!(layout.workspace_consumes_width);
        assert!(layout.render_workspace);
        assert!(layout.render_properties);
    }

    #[test]
    fn focus_mode_suppresses_rendering_without_changing_requested_intent() {
        let mut input = input(1800);
        input.focus_mode_active = true;
        let layout = derive_adaptive_shell_layout(input);

        assert!(!layout.workspace_consumes_width);
        assert!(!layout.render_workspace);
        assert!(!layout.render_properties);
        assert_eq!(input.compact_surface, None);
        assert!(input.workspace_requested_visible);
        assert!(input.properties_requested_visible);
    }

    #[test]
    fn workspace_breakpoint_boundary_changes_width_consumption_only_above_limit() {
        assert!(!derive_adaptive_shell_layout(input(860)).workspace_consumes_width);
        assert!(derive_adaptive_shell_layout(input(861)).workspace_consumes_width);
    }

    #[test]
    fn properties_breakpoint_boundary_switches_from_sheet_to_pane() {
        let at_boundary = input(1350);
        let above_boundary = input(1351);

        assert_eq!(
            derive_adaptive_shell_layout(at_boundary).properties_presentation,
            PropertiesPresentation::Sheet
        );
        assert_eq!(
            derive_adaptive_shell_layout(above_boundary).properties_presentation,
            PropertiesPresentation::Pane
        );
    }

    #[test]
    fn dual_sidebar_width_helper_preserves_requested_center_space() {
        let center_target = MIN_EDITOR_CONTENT_WIDTH_SP + DUAL_PANE_LAYOUT_OVERHEAD_SP;
        let total_width = dual_sidebar_window_width_for_center(
            center_target,
            WorkspaceSidebarWidthPreset::Large.max_width_sp(),
        );
        assert!(
            (total_width * 0.75
                - WorkspaceSidebarWidthPreset::Large.max_width_sp()
                - center_target)
                .abs()
                < 0.001
        );
    }

    #[test]
    fn workspace_sidebar_target_width_clamps_for_representative_window_sizes() {
        assert_eq!(
            WorkspaceSidebarWidthPreset::Small.clamped_width_sp(900),
            220.0
        );
        assert_eq!(
            WorkspaceSidebarWidthPreset::Comfy.clamped_width_sp(1200),
            360.0
        );
        assert_eq!(
            WorkspaceSidebarWidthPreset::Large.clamped_width_sp(1400),
            440.0
        );
        assert_eq!(
            WorkspaceSidebarWidthPreset::Comfy.clamped_width_sp(2000),
            360.0
        );
    }

    #[test]
    fn properties_fraction_preserves_total_window_quarter_with_workspace_width() {
        let input = input(1800);
        let workspace_width = effective_workspace_sidebar_width_sp(input);
        let inner_fraction = effective_properties_fraction(input);
        let total_fraction = inner_fraction * (1.0 - workspace_width / 1800.0);

        assert!((total_fraction - FIXED_PROPERTIES_SIDEBAR_FRACTION).abs() < 0.001);
    }
}
