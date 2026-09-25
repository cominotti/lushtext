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
//! The role home is the per-workflow subdirectory `ui/window/geometry/`, which
//! this module moved into when the workflow migrated. **The move required a
//! gate re-key, and the re-key is done.** This workflow's GTK adapter halves
//! stay behind in `ui/window/imp.rs` (the `size_allocate` vfunc and the
//! `constructed()` wiring) and `ui/window/actions.rs` (the two toggle action
//! bodies), and both remain literal path keys in the native-minimap highlight,
//! native-minimap animation, and workspace-sidebar animation-matrix visual-proof
//! predicates in **both** `scripts/check-visual-proof-policy.py` and
//! `crates/cargo-gtk-proof/src/policy.rs`.
//!
//! Moving the geometry code here without adding a key **did** disarm two named
//! pixel invariants and the sidebar animation matrix while every gate exited 0 —
//! that disarm was observed deliberately before it was fixed, because a
//! path-keyed gate that matches nothing does not fail. The role home is now a
//! narrow prefix key in both implementations, each with a parity self-test
//! proved by a deliberate red. A `ui/window/` prefix was rejected: it would
//! sweep in seven subdirectories, four of them role homes no predicate has ever
//! protected.

#![deny(clippy::float_arithmetic)]

use crate::ui::sidebar::width_preset::WorkspaceSidebarWidthPreset;

#[cfg(kani)]
mod kani_proofs;

/// Tiny non-zero floor used before the first real workspace-width sync.
pub(in crate::ui::window) const WORKSPACE_SIDEBAR_MIN_WIDTH_SP: i32 = 1;
/// Properties sidebar minimum width in scale-independent pixels.
pub(in crate::ui::window) const PROPERTIES_SIDEBAR_MIN_WIDTH_SP: i32 = 280;
/// Minimum normal-mode height that preserves persistent chrome and an editor.
pub const NORMAL_MODE_MIN_HEIGHT_SP: i32 = 360;
/// Collapse the left workspace pane on narrower windows.
pub(in crate::ui::window) const WORKSPACE_BREAKPOINT_MAX_WIDTH_SP: i32 = 860;
/// GNOME Text Editor switches the header Open control to an icon at 400sp.
pub(in crate::ui::window) const OPEN_BUTTON_BREAKPOINT_MAX_WIDTH_SP: i32 = 400;

/// The visible right properties pane targets `1 / FIXED_PROPERTIES_SIDEBAR_DIVISOR`
/// of the total window width: a quarter.
const FIXED_PROPERTIES_SIDEBAR_DIVISOR: i32 = 4;
/// Minimum center width that keeps restored-document inline alerts stable.
const MIN_EDITOR_CONTENT_WIDTH_SP: i32 = 620;
/// Width budget for split separators, padding, and rounding noise.
const DUAL_PANE_LAYOUT_OVERHEAD_SP: i32 = 32;
/// Wide document-properties presentation in the multi-layout view.
const PROPERTIES_LAYOUT_PANE: &str = "pane";
/// Compact document-properties presentation in the multi-layout view.
const PROPERTIES_LAYOUT_SHEET: &str = "sheet";

/// Secondary surfaces that can compete for the compact-width slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum SecondarySurface {
    /// The left workspace sidebar.
    Workspace,
    /// The document-properties surface.
    DocumentProperties,
}

/// Adaptive presentation currently used for document properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui::window) enum PropertiesPresentation {
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
pub(in crate::ui::window) struct AdaptiveShellInputs {
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
pub(in crate::ui::window) struct AdaptiveShellLayout {
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

pub(in crate::ui::window) fn properties_breakpoint_condition(max_width_sp: i32) -> String {
    format!("max-width: {max_width_sp}sp")
}

pub(in crate::ui::window) fn workspace_breakpoint_condition() -> String {
    properties_breakpoint_condition(WORKSPACE_BREAKPOINT_MAX_WIDTH_SP)
}

/// A secondary pane's width and the width it is a share of, both in whole sp.
///
/// The policy decides widths; the split views take a fraction, and the GTK
/// adapter forms it once (`execution::split_fraction`), so no fraction is
/// computed here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui::window) struct PaneShare {
    /// The pane's width.
    pub(in crate::ui::window) width_sp: i32,
    /// The width the pane is a share of: the window, or the inner split.
    pub(in crate::ui::window) of_sp: i32,
}

/// Return the preset-clamped workspace target width for this window.
pub(in crate::ui::window) fn effective_workspace_sidebar_width_sp(
    input: AdaptiveShellInputs,
) -> i32 {
    input.workspace_preset.clamped_width_sp(input.window_width)
}

/// The workspace pane's share of the window for one preset.
pub(in crate::ui::window) fn workspace_preset_share(
    preset: WorkspaceSidebarWidthPreset,
    window_width: i32,
) -> PaneShare {
    PaneShare {
        width_sp: preset.clamped_width_sp(window_width),
        of_sp: window_width.max(1),
    }
}

/// The workspace pane's share of the window for this intent.
pub(in crate::ui::window) fn workspace_sidebar_share(input: AdaptiveShellInputs) -> PaneShare {
    workspace_preset_share(input.workspace_preset, input.window_width)
}

/// The properties pane's share, measured against its current inner split: the
/// window, or what the workspace pane leaves of it.
///
/// The width is always the window-relative target (a quarter of the window, at
/// least the properties minimum); when the workspace consumes width only the
/// denominator changes, to the inner split. No `minimum / inner` floor is needed:
/// the target is at least `min(minimum, window)`, which is at least
/// `min(minimum, inner)`.
pub(in crate::ui::window) fn effective_properties_share(input: AdaptiveShellInputs) -> PaneShare {
    let desired = desired_properties_width_sp(input.window_width);
    if derive_adaptive_shell_layout(input).workspace_consumes_width {
        let inner_width = properties_inner_split_width(
            input.window_width.max(1),
            effective_workspace_sidebar_width_sp(input),
        );
        PaneShare {
            width_sp: desired,
            of_sp: inner_width,
        }
    } else {
        desired_properties_share(input.window_width)
    }
}

/// The inner split width the properties pane is measured against.
///
/// The `max(1)` floor is load-bearing rather than defensive: the pane's share is
/// this width's denominator, so a workspace pane that consumes the whole window
/// must not produce a zero or negative one.
///
/// This started as an extraction made only to narrow a mutation exclusion, and the
/// extraction is what made the exclusion unnecessary: a named pure function has a
/// contract of its own, and that contract is directly testable, which kills the
/// whole mutant family at once.
fn properties_inner_split_width(total_width: i32, workspace_width: i32) -> i32 {
    total_width.saturating_sub(workspace_width).max(1)
}

/// The properties pane's window-relative target width: a quarter of the window
/// (floored), at least the properties minimum, and never wider than the window.
pub(in crate::ui::window) fn desired_properties_width_sp(window_width: i32) -> i32 {
    let width = window_width.max(1);
    (width / FIXED_PROPERTIES_SIDEBAR_DIVISOR).max(PROPERTIES_SIDEBAR_MIN_WIDTH_SP.min(width))
}

/// The properties pane's window-relative share.
pub(in crate::ui::window) fn desired_properties_share(window_width: i32) -> PaneShare {
    PaneShare {
        width_sp: desired_properties_width_sp(window_width),
        of_sp: window_width.max(1),
    }
}

/// Derive which secondary surfaces render, and how, for one stable intent.
///
/// Kani proves (`policy/kani_proofs.rs`), for every `i32` width, preset,
/// requested visibility, Focus Mode, and compact-surface choice: Focus Mode
/// renders nothing; an unrequested surface never renders; the sheet
/// presentation renders at most one surface; the pane presentation above the
/// workspace breakpoint renders every requested surface; and the sheet is
/// chosen exactly at or below the derived breakpoint.
pub(in crate::ui::window) fn derive_adaptive_shell_layout(
    input: AdaptiveShellInputs,
) -> AdaptiveShellLayout {
    let workspace_consumes_width = workspace_consumes_width_for_intent(input);
    let workspace_width_sp = if workspace_consumes_width {
        effective_workspace_sidebar_width_sp(input)
    } else {
        0
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
///
/// Two guards, in whole sp: the window width at which the properties pane's
/// quarter leaves the editor-content minimum plus the layout overhead plus the
/// workspace (rounded up), and that same sum plus the properties minimum. Kani
/// proves it never decreases as the workspace width grows over the reachable
/// range [0, 440] sp, and that it is at least the second guard.
fn properties_breakpoint_max_width_sp(workspace_width_sp: i32) -> i32 {
    let center_target = MIN_EDITOR_CONTENT_WIDTH_SP + DUAL_PANE_LAYOUT_OVERHEAD_SP;
    let fraction_guard = dual_sidebar_window_width_for_center(center_target, workspace_width_sp);
    let min_width_guard = center_target
        .saturating_add(workspace_width_sp)
        .saturating_add(PROPERTIES_SIDEBAR_MIN_WIDTH_SP);
    fraction_guard.max(min_width_guard)
}

/// The smallest window width whose properties quarter leaves `center + workspace`
/// for the rest: `ceil((center + workspace) * 4 / 3)`.
fn dual_sidebar_window_width_for_center(center_width_sp: i32, workspace_width_sp: i32) -> i32 {
    let rest_parts = i64::from(FIXED_PROPERTIES_SIDEBAR_DIVISOR - 1);
    let needed = (i64::from(center_width_sp) + i64::from(workspace_width_sp))
        * i64::from(FIXED_PROPERTIES_SIDEBAR_DIVISOR);
    let width = (needed + rest_parts - 1).div_euclid(rest_parts);
    i32::try_from(width).unwrap_or(i32::MAX)
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

    // Tests below this point were added when `adaptive_shell.rs` became this
    // `policy.rs`. The rename brought 248 production lines of already-pure
    // geometry policy inside the `ui/**/policy.rs` mutation convention, and it
    // had generated **zero** mutants before, so it had never been mutation
    // tested at all. Fifteen survivors on the first run; each test here names
    // the decision it pins. This is the test module's own narrative rather than
    // documentation of the one test that happens to follow it, so it is a plain
    // comment: a `///` block here would attach to that single `#[test]`.

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
    fn the_workspace_share_follows_the_preset_and_stays_positive() {
        // Not zero and not negative: a zero share collapses the pane.
        for width in [400, 860, 1280, 1920, 3840] {
            let mut probe = input(width);
            for preset in WorkspaceSidebarWidthPreset::ALL {
                probe.workspace_preset = preset;
                let share = workspace_sidebar_share(probe);
                assert_eq!(share.width_sp, preset.clamped_width_sp(width));
                assert_eq!(share.of_sp, width);
                assert!(share.width_sp >= 1 && share.width_sp <= share.of_sp);
            }
        }
    }

    #[test]
    fn a_wider_preset_never_yields_a_narrower_workspace_share() {
        let mut small = input(1920);
        small.workspace_preset = WorkspaceSidebarWidthPreset::Small;
        let mut large = input(1920);
        large.workspace_preset = WorkspaceSidebarWidthPreset::Large;
        assert!(workspace_sidebar_share(large).width_sp >= workspace_sidebar_share(small).width_sp);
    }

    #[test]
    fn the_properties_share_is_taken_of_the_inner_split_not_the_window() {
        // When the workspace consumes width, the properties share is measured
        // against the remaining inner split, so its denominator is the window
        // minus the workspace. Subtracting the wrong way round would make the
        // properties pane too narrow to meet its own minimum.
        let wide = input(2560);
        let layout = derive_adaptive_shell_layout(wide);
        assert!(layout.workspace_consumes_width);

        let rebased = effective_properties_share(wide);
        assert_eq!(rebased.width_sp, desired_properties_width_sp(2560));
        assert_eq!(
            rebased.of_sp,
            2560 - effective_workspace_sidebar_width_sp(wide)
        );

        // With no workspace consuming width, the two agree exactly.
        let mut no_workspace = input(2560);
        no_workspace.workspace_requested_visible = false;
        assert!(!derive_adaptive_shell_layout(no_workspace).workspace_consumes_width);
        assert_eq!(
            effective_properties_share(no_workspace),
            desired_properties_share(2560)
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
        assert_eq!(properties_breakpoint_max_width_sp(100), 1032);
        assert_eq!(properties_breakpoint_max_width_sp(0), 932);
        // 932 == center_target + properties minimum, exactly.
        assert_eq!(
            properties_breakpoint_max_width_sp(0),
            MIN_EDITOR_CONTENT_WIDTH_SP
                + DUAL_PANE_LAYOUT_OVERHEAD_SP
                + PROPERTIES_SIDEBAR_MIN_WIDTH_SP
        );
        // And above the crossover the fraction guard takes over.
        assert_eq!(properties_breakpoint_max_width_sp(400), 1403);
    }

    #[test]
    fn the_properties_width_is_the_quarter_above_the_crossover_and_the_minimum_below() {
        // Below 1120sp the target is the properties minimum; at or above it the
        // floored quarter takes over. The crossover is `minimum * 4`, so
        // changing either constant moves it.
        assert_eq!(
            PROPERTIES_SIDEBAR_MIN_WIDTH_SP * FIXED_PROPERTIES_SIDEBAR_DIVISOR,
            1120
        );
        assert_eq!(desired_properties_width_sp(1119), 280);
        assert_eq!(desired_properties_width_sp(1120), 280);
        assert_eq!(desired_properties_width_sp(1124), 281);
        assert_eq!(desired_properties_width_sp(1800), 450);
        // Never wider than a narrow window.
        assert_eq!(desired_properties_width_sp(200), 200);
        assert_eq!(desired_properties_width_sp(0), 1);
        // The rebased share keeps the width and only changes its denominator.
        for width in (861..4000).step_by(37) {
            for preset in WorkspaceSidebarWidthPreset::ALL {
                let mut probe = input(width);
                probe.workspace_preset = preset;
                if !derive_adaptive_shell_layout(probe).workspace_consumes_width {
                    continue;
                }
                let share = effective_properties_share(probe);
                assert_eq!(share.width_sp, desired_properties_width_sp(width));
                assert!(
                    share.width_sp <= share.of_sp,
                    "width {width} preset {preset:?}"
                );
            }
        }
    }

    #[test]
    fn the_inner_split_width_contract_holds() {
        // A named pure function with a contract of its own: asserting it directly
        // kills its mutant family — subtraction swapped for addition or division,
        // and whole-body replacement alike.
        assert_eq!(properties_inner_split_width(1200, 360), 840);

        // The floor is load-bearing, not defensive: the width is the properties
        // share's denominator, so a workspace pane that consumes the entire
        // window must still yield a positive one.
        assert_eq!(properties_inner_split_width(360, 360), 1);
        assert_eq!(properties_inner_split_width(100, 500), 1);
        assert_eq!(properties_inner_split_width(i32::MIN, i32::MAX), 1);
    }

    #[test]
    fn the_properties_breakpoint_width_grows_with_the_workspace_it_must_clear() {
        // The guard is `center + workspace + properties-minimum`, so a wider
        // workspace pushes the breakpoint up. Subtracting instead of adding the
        // properties minimum would let the pane appear below its own floor.
        let narrow = properties_breakpoint_max_width_sp(0);
        let wide = properties_breakpoint_max_width_sp(400);
        assert!(wide > narrow, "{wide} must exceed {narrow}");
        assert!(
            narrow >= MIN_EDITOR_CONTENT_WIDTH_SP + PROPERTIES_SIDEBAR_MIN_WIDTH_SP,
            "the breakpoint must clear the editor floor plus the properties minimum"
        );
        // Monotonic across the whole preset range.
        let mut previous = 0;
        for workspace in [0, 100, 250, 400, 800] {
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
        assert_eq!(properties_breakpoint_max_width_sp(0), 932);
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
        // The smallest window whose remaining three quarters hold the center
        // target plus the widest workspace: one sp less would not.
        let center_target = MIN_EDITOR_CONTENT_WIDTH_SP + DUAL_PANE_LAYOUT_OVERHEAD_SP;
        let workspace = WorkspaceSidebarWidthPreset::Large.max_width_sp();
        let total_width = dual_sidebar_window_width_for_center(center_target, workspace);
        let rest = |width: i32| i64::from(width) * 75 / 100;
        assert!(rest(total_width) >= i64::from(center_target + workspace));
        assert!(i64::from(total_width - 1) * 75 < i64::from(center_target + workspace) * 100);
        assert_eq!(
            dual_sidebar_window_width_for_center(center_target, 100),
            1003
        );
    }

    #[test]
    fn workspace_sidebar_target_width_clamps_for_representative_window_sizes() {
        assert_eq!(
            WorkspaceSidebarWidthPreset::Small.clamped_width_sp(900),
            220
        );
        assert_eq!(
            WorkspaceSidebarWidthPreset::Comfy.clamped_width_sp(1200),
            360
        );
        assert_eq!(
            WorkspaceSidebarWidthPreset::Large.clamped_width_sp(1400),
            440
        );
        assert_eq!(
            WorkspaceSidebarWidthPreset::Comfy.clamped_width_sp(2000),
            360
        );
    }

    #[test]
    fn properties_share_preserves_total_window_quarter_with_workspace_width() {
        // The pane stays a quarter of the whole window even though its share is
        // taken of the inner split.
        let input = input(1800);
        let share = effective_properties_share(input);
        assert_eq!(share.width_sp, 1800 / 4);
        assert_eq!(
            share.of_sp,
            1800 - effective_workspace_sidebar_width_sp(input)
        );
    }
}
