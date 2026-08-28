// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: pure policy — the adaptive shell geometry workflow's `policy.rs`.
//!
//! Pure policy for the window's adaptive secondary surfaces.
//!
//! The GTK adapter supplies current settings and widget-derived intent, then
//! applies the returned decision. This module never reads GSettings or mutates
//! widgets, so allocation and breakpoint behavior can be verified without GTK.
//!
//! It was named `adaptive_shell.rs` until it took this role. The rename is a
//! **role assignment, not a re-decomposition**: no responsibility moved between
//! modules, no file was split, and the contents are unchanged. What changed is
//! that the module is now inside the `ui/**/policy.rs` mutation convention; it
//! was previously pure, correct, and reachable by no scope entry at all, so it
//! carried zero mutation coverage while every command exited 0.
//!
//! The role home is **flat** `ui/window/` rather than a per-workflow
//! subdirectory. That is a constraint rather than a preference: this workflow's
//! GTK adapter halves live in `ui/window/imp.rs` and `ui/window/actions.rs`,
//! which are literal path keys in the native-minimap highlight, native-minimap
//! animation, and workspace-sidebar animation-matrix visual-proof predicates in
//! both `scripts/check-visual-proof-policy.py` and
//! `crates/cargo-gtk-proof/src/policy.rs`. Moving that geometry code into a new
//! directory no predicate names would disarm two named pixel invariants and the
//! sidebar animation matrix while every gate still exited 0.

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
        let inner_width = properties_inner_split_width(total_width, workspace_width);
        let lower = (PROPERTIES_SIDEBAR_MIN_WIDTH_SP / inner_width).min(1.0);
        (total_fraction / remaining_fraction).max(lower).min(1.0)
    } else {
        total_fraction
    }
}

/// The inner split width the properties pane is measured against.
///
/// The `max(1.0)` floor is load-bearing rather than defensive: the caller divides
/// [`PROPERTIES_SIDEBAR_MIN_WIDTH_SP`] by this width, so a workspace pane that
/// consumes the whole window must not produce a zero or negative divisor.
///
/// This started as an extraction made only to narrow a mutation exclusion, and the
/// extraction is what made the exclusion unnecessary. Every mutation of this
/// arithmetic is invisible *through the caller* — the floor it feeds is provably
/// non-binding for the current constants, and a mutated width only makes that floor
/// less binding — so at the caller's boundary these really are equivalences. But a
/// named pure function has a contract of its own, and that contract is directly
/// testable, which kills the whole family at once. Retiring the exclusion beats
/// documenting it: a justified exclusion has to be re-justified every time either
/// constant moves.
fn properties_inner_split_width(total_width: f64, workspace_width: f64) -> f64 {
    (total_width - workspace_width).max(1.0)
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

    /// Tests below this point were added when `adaptive_shell.rs` became this
    /// `policy.rs`. The rename brought 248 production lines of already-pure
    /// geometry policy inside the `ui/**/policy.rs` mutation convention, and it
    /// had generated **zero** mutants before, so it had never been mutation
    /// tested at all. Fifteen survivors on the first run; each test here names
    /// the decision it pins.

    #[test]
    fn layout_names_are_the_exact_adwaita_layout_ids() {
        // These strings are `AdwMultiLayoutView::layout-name` values. A wrong or
        // empty one silently selects no layout, so they are pinned literally.
        assert_eq!(PropertiesPresentation::Pane.layout_name(), "pane");
        assert_eq!(PropertiesPresentation::Sheet.layout_name(), "sheet");
        assert_ne!(
            PropertiesPresentation::Pane.layout_name(),
            PropertiesPresentation::Sheet.layout_name()
        );
    }

    #[test]
    fn only_the_sheet_layout_name_round_trips_to_sheet() {
        assert_eq!(
            PropertiesPresentation::from_layout_name(Some("sheet")),
            PropertiesPresentation::Sheet
        );
        // Everything else is the pane, including an absent name: the pane is the
        // safe default because it is the non-compact presentation.
        assert_eq!(
            PropertiesPresentation::from_layout_name(Some("pane")),
            PropertiesPresentation::Pane
        );
        assert_eq!(
            PropertiesPresentation::from_layout_name(None),
            PropertiesPresentation::Pane
        );
        assert_eq!(
            PropertiesPresentation::from_layout_name(Some("unknown")),
            PropertiesPresentation::Pane
        );
        // Round trip through the same vocabulary both ways.
        for presentation in [PropertiesPresentation::Pane, PropertiesPresentation::Sheet] {
            assert_eq!(
                PropertiesPresentation::from_layout_name(Some(presentation.layout_name())),
                presentation
            );
        }
    }

    #[test]
    fn breakpoint_conditions_render_the_adwaita_condition_syntax() {
        // `AdwBreakpoint::set_condition` parses this string; an empty or
        // malformed one installs no breakpoint and the adaptive layout silently
        // stops switching.
        assert_eq!(properties_breakpoint_condition(1350), "max-width: 1350sp");
        assert_eq!(properties_breakpoint_condition(0), "max-width: 0sp");
        assert_eq!(
            workspace_breakpoint_condition(),
            format!("max-width: {WORKSPACE_BREAKPOINT_MAX_WIDTH_SP}sp")
        );
        assert_eq!(workspace_breakpoint_condition(), "max-width: 860sp");
    }

    #[test]
    fn the_workspace_fraction_follows_the_preset_and_stays_in_range() {
        // Not 0.0, not 1.0, and not negative: a zero fraction collapses the pane
        // and a unit fraction hides the editor.
        for width in [400, 860, 1280, 1920, 3840] {
            let mut probe = input(width);
            for preset in [
                WorkspaceSidebarWidthPreset::Small,
                WorkspaceSidebarWidthPreset::Comfy,
                WorkspaceSidebarWidthPreset::Large,
            ] {
                probe.workspace_preset = preset;
                let fraction = effective_workspace_sidebar_fraction(probe);
                assert!(
                    fraction > 0.0 && fraction <= 1.0,
                    "width {width} preset {preset:?} produced {fraction}"
                );
                assert_eq!(fraction, preset.effective_fraction(width));
            }
        }
    }

    #[test]
    fn a_wider_preset_never_yields_a_narrower_workspace_fraction() {
        let mut small = input(1920);
        small.workspace_preset = WorkspaceSidebarWidthPreset::Small;
        let mut large = input(1920);
        large.workspace_preset = WorkspaceSidebarWidthPreset::Large;
        assert!(
            effective_workspace_sidebar_fraction(large)
                >= effective_workspace_sidebar_fraction(small)
        );
    }

    #[test]
    fn the_properties_fraction_is_taken_of_the_inner_split_not_the_window() {
        // When the workspace consumes width, the properties fraction must be
        // re-based onto the remaining inner split, so it is *larger* than the
        // window-relative target. Subtracting the wrong way round would make the
        // properties pane too narrow to meet its own minimum.
        let wide = input(2560);
        let layout = derive_adaptive_shell_layout(wide);
        assert!(layout.workspace_consumes_width);

        let rebased = effective_properties_fraction(wide);
        let window_relative = desired_properties_fraction(wide.window_width);
        assert!(
            rebased > window_relative,
            "rebased {rebased} must exceed window-relative {window_relative}"
        );
        assert!(rebased <= 1.0);

        // With no workspace consuming width, the two agree exactly.
        let mut no_workspace = input(2560);
        no_workspace.workspace_requested_visible = false;
        assert!(!derive_adaptive_shell_layout(no_workspace).workspace_consumes_width);
        assert_eq!(
            effective_properties_fraction(no_workspace),
            desired_properties_fraction(no_workspace.window_width)
        );
    }

    #[test]
    fn focus_mode_suppresses_both_surfaces_while_preserving_the_requests() {
        let mut focus = input(2560);
        focus.focus_mode_active = true;
        let layout = derive_adaptive_shell_layout(focus);
        assert!(!layout.render_workspace);
        assert!(!layout.render_properties);
        assert!(!layout.workspace_consumes_width);
        // The requests themselves are untouched, which is what lets exiting
        // Focus Mode restore both panes.
        assert!(focus.workspace_requested_visible);
        assert!(focus.properties_requested_visible);
    }

    #[test]
    fn a_collapsed_window_renders_the_workspace_only_when_it_wins_the_compact_slot() {
        // At or below the workspace breakpoint both surfaces cannot fit, so the
        // workspace renders only if it is the chosen compact surface. Replacing
        // the `&&` here with `||` would show both.
        let mut collapsed = input(WORKSPACE_BREAKPOINT_MAX_WIDTH_SP);
        collapsed.compact_surface = Some(SecondarySurface::Workspace);
        let layout = derive_adaptive_shell_layout(collapsed);
        assert!(layout.render_workspace);
        assert!(!layout.render_properties);

        collapsed.compact_surface = Some(SecondarySurface::DocumentProperties);
        let layout = derive_adaptive_shell_layout(collapsed);
        assert!(!layout.render_workspace);
        assert!(layout.render_properties);

        // Not requested means not rendered even when it holds the slot.
        collapsed.compact_surface = Some(SecondarySurface::Workspace);
        collapsed.workspace_requested_visible = false;
        assert!(!derive_adaptive_shell_layout(collapsed).render_workspace);
    }

    #[test]
    fn a_compact_window_gives_properties_the_slot_and_hides_the_workspace() {
        // Between the two breakpoints the properties sheet wins the compact slot
        // when requested, and the workspace must yield. `||` in place of `&&`
        // would leave the workspace visible behind the sheet.
        let compact_width = WORKSPACE_BREAKPOINT_MAX_WIDTH_SP + 1;
        let mut compact = input(compact_width);
        compact.compact_surface = Some(SecondarySurface::DocumentProperties);
        let layout = derive_adaptive_shell_layout(compact);
        assert_eq!(
            layout.properties_presentation,
            PropertiesPresentation::Sheet
        );
        assert!(layout.render_properties);
        assert!(!layout.render_workspace);

        // Properties not requested: the workspace gets the width back.
        compact.properties_requested_visible = false;
        let layout = derive_adaptive_shell_layout(compact);
        assert!(!layout.render_properties);
        assert!(layout.render_workspace);
    }

    #[test]
    fn a_secondary_surface_is_requested_only_when_its_own_flag_is_set() {
        // Replacing this with `true` would let an unrequested surface claim the
        // compact slot.
        let mut probe = input(1280);
        probe.properties_requested_visible = false;
        assert!(secondary_surface_requested_for_intent(
            probe,
            SecondarySurface::Workspace
        ));
        assert!(!secondary_surface_requested_for_intent(
            probe,
            SecondarySurface::DocumentProperties
        ));

        probe.workspace_requested_visible = false;
        probe.properties_requested_visible = true;
        assert!(!secondary_surface_requested_for_intent(
            probe,
            SecondarySurface::Workspace
        ));
        assert!(secondary_surface_requested_for_intent(
            probe,
            SecondarySurface::DocumentProperties
        ));
    }

    #[test]
    fn a_compact_window_keeps_the_workspace_when_the_workspace_holds_the_slot() {
        // The compact branch reads `!(slot_is_properties && properties_requested)`.
        // Replacing that `&&` with `||` is only observable when the computed slot
        // is NOT properties while properties are still requested — which happens
        // exactly when the user has explicitly chosen the workspace for the
        // compact slot. Then the workspace must stay visible; `||` would hide it.
        let mut compact = input(1200);
        compact.compact_surface = Some(SecondarySurface::Workspace);
        compact.workspace_requested_visible = true;
        compact.properties_requested_visible = true;

        let layout = derive_adaptive_shell_layout(compact);
        assert_eq!(
            layout.properties_presentation,
            PropertiesPresentation::Sheet,
            "1200sp must be compact for this preset"
        );
        assert_eq!(layout.compact_surface, Some(SecondarySurface::Workspace));
        assert!(
            layout.render_workspace,
            "the workspace holds the compact slot, so it must render"
        );
        assert!(
            !layout.render_properties,
            "properties lost the slot, so they must not render"
        );
    }

    #[test]
    fn the_min_width_guard_dominates_the_breakpoint_for_a_narrow_workspace() {
        // Two guards compete: a fraction guard, and a min-width guard of
        // `center + workspace + properties-minimum`. The fraction guard wins once
        // the workspace exceeds roughly 188sp, so the *sum* in the min-width guard
        // is only observable below that — which is why the monotonicity check
        // above cannot see an arithmetic error there.
        assert_eq!(properties_breakpoint_max_width_sp(100.0), 1032);
        assert_eq!(properties_breakpoint_max_width_sp(0.0), 932);
        // 932 == center_target + properties minimum, exactly.
        assert_eq!(
            f64::from(properties_breakpoint_max_width_sp(0.0)),
            MIN_EDITOR_CONTENT_WIDTH_SP
                + DUAL_PANE_LAYOUT_OVERHEAD_SP
                + PROPERTIES_SIDEBAR_MIN_WIDTH_SP
        );
        // And above the crossover the fraction guard takes over.
        assert_eq!(properties_breakpoint_max_width_sp(400.0), 1403);
    }

    #[test]
    fn the_properties_fraction_floor_is_non_binding_by_construction() {
        // `effective_properties_fraction` clamps its rebased ratio up to a floor
        // of `properties-minimum / inner-width`. That floor is **provably
        // non-binding** for the current constants, and this test pins the reason.
        //
        // Below 1120sp the target fraction *is* `minimum / total`, so the rebased
        // ratio equals the floor exactly; at or above 1120sp the target is the
        // fixed 0.25 and the ratio exceeds the floor because
        // `0.25 * total >= minimum`. The crossover is `minimum / 0.25 == 1120`,
        // so changing either constant can make the floor bind again.
        //
        // This proof no longer carries a mutation exclusion — see
        // `the_inner_split_width_contract_holds_independently_of_the_floor`, which
        // pins the width function directly and made the exclusion unnecessary. The
        // proof is kept because it is what makes the *caller's* clamp readable.
        assert_eq!(
            PROPERTIES_SIDEBAR_MIN_WIDTH_SP / FIXED_PROPERTIES_SIDEBAR_FRACTION,
            1120.0,
            "the floor's non-binding proof depends on this crossover"
        );

        // A **sampled** sweep, not an exhaustive one: 85 widths x 3 presets = 255
        // reachable pairs. The exhaustive claim would be false, and the crossover
        // assertion above is what actually generalises the result — the sampling
        // only demonstrates it.
        for width in (861..4000).step_by(37) {
            for preset in [
                WorkspaceSidebarWidthPreset::Small,
                WorkspaceSidebarWidthPreset::Comfy,
                WorkspaceSidebarWidthPreset::Large,
            ] {
                let mut probe = input(width);
                probe.workspace_preset = preset;
                if !derive_adaptive_shell_layout(probe).workspace_consumes_width {
                    continue;
                }
                let total = f64::from(width);
                let workspace = effective_workspace_sidebar_width_sp(probe);
                let remaining = (1.0 - workspace / total).max(f64::EPSILON);
                let ratio = desired_properties_fraction(width) / remaining;
                let floor =
                    (PROPERTIES_SIDEBAR_MIN_WIDTH_SP / (total - workspace).max(1.0)).min(1.0);
                assert!(
                    ratio + 1e-9 >= floor,
                    "floor bound at width {width} preset {preset:?}: ratio {ratio} < floor {floor}"
                );
                // And the published fraction is the ratio, clamped only at 1.0.
                assert!(
                    (effective_properties_fraction(probe) - ratio.min(1.0)).abs() < 1e-9,
                    "width {width} preset {preset:?} did not publish the rebased ratio"
                );
            }
        }
    }

    #[test]
    fn the_inner_split_width_contract_holds_independently_of_the_floor() {
        // Every mutation of `properties_inner_split_width` is invisible through
        // `effective_properties_fraction`, because the floor it feeds is
        // non-binding (see above) and a mutated width only makes that floor *less*
        // binding. That made these mutants look like a family of equivalences
        // needing a documented exclusion. They are not: the function is a named
        // pure function with a contract of its own, and asserting that contract
        // directly kills the whole family — subtraction swapped for addition or
        // division, and whole-body replacement alike.
        assert_eq!(properties_inner_split_width(1200.0, 360.0), 840.0);

        // The floor is load-bearing, not defensive: the caller divides
        // `PROPERTIES_SIDEBAR_MIN_WIDTH_SP` by this value, so a workspace pane
        // that consumes the entire window must still yield a positive divisor.
        assert_eq!(properties_inner_split_width(360.0, 360.0), 1.0);
        assert_eq!(properties_inner_split_width(100.0, 500.0), 1.0);
        assert!(
            PROPERTIES_SIDEBAR_MIN_WIDTH_SP / properties_inner_split_width(360.0, 360.0) > 0.0,
            "the floor exists so this division stays finite and positive"
        );
    }

    #[test]
    fn the_properties_breakpoint_width_grows_with_the_workspace_it_must_clear() {
        // The guard is `center + workspace + properties-minimum`, so a wider
        // workspace pushes the breakpoint up. Subtracting instead of adding the
        // properties minimum would let the pane appear below its own floor.
        let narrow = properties_breakpoint_max_width_sp(0.0);
        let wide = properties_breakpoint_max_width_sp(400.0);
        assert!(wide > narrow, "{wide} must exceed {narrow}");
        assert!(
            f64::from(narrow) >= MIN_EDITOR_CONTENT_WIDTH_SP + PROPERTIES_SIDEBAR_MIN_WIDTH_SP,
            "the breakpoint must clear the editor floor plus the properties minimum"
        );
        // Monotonic across the whole preset range.
        let mut previous = 0;
        for workspace in [0.0, 100.0, 250.0, 400.0, 800.0] {
            let value = properties_breakpoint_max_width_sp(workspace);
            assert!(value >= previous, "not monotonic at {workspace}");
            previous = value;
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
