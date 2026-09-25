// SPDX-License-Identifier: GPL-3.0-or-later

//! Named workspace sidebar width presets.
//!
//! # Ownership
//!
//! This module is **cross-cutting, and it is not the workspace tree workflow's**.
//! It is the `workspace-sidebar-width-policy` capability's value, owned by
//! `WFR-SHELL-GEOMETRY` (slot 7b): Preferences renders it as a picker, and the window
//! shell does the split-view math with it. The workspace tree workflow neither
//! reads nor writes it — `.agents/rules/ui.md` states plainly that "the window
//! layer owns the split-view math; the sidebar does not expose a duplicate width
//! control".
//!
//! It lives under `ui/sidebar/` only because the preset names a *sidebar*
//! dimension. Do not read that path as workflow ownership; the file exists
//! separately from `ui/sidebar/mod.rs` precisely so the tree workflow's narrative
//! facade is not 103 lines of a neighbouring row's value type.
//!
//! Its three consumers are `ui/preferences/imp.rs`,
//! `ui/window/geometry/policy.rs`, and `ui/window/geometry/execution.rs`.

#![deny(clippy::float_arithmetic)]

#[cfg(kani)]
mod kani_proofs;

/// Supported named workspace sidebar presets used by Preferences and shell math.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum WorkspaceSidebarWidthPreset {
    Small,
    Comfy,
    Large,
}

impl WorkspaceSidebarWidthPreset {
    pub const DEFAULT: Self = Self::Comfy;
    pub const ALL: [Self; 3] = [Self::Small, Self::Comfy, Self::Large];

    /// Return the user-visible label for the preset picker.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Small => "Small",
            Self::Comfy => "Comfy",
            Self::Large => "Large",
        }
    }

    /// Return the stored preset hint fraction used to identify the selected preset.
    #[must_use]
    pub const fn fraction(self) -> f64 {
        match self {
            Self::Small => 0.2,
            Self::Comfy => 0.3,
            Self::Large => 0.4,
        }
    }

    /// The hint as a whole percentage of the window width (20, 30, 40), the
    /// integer form [`Self::clamped_width_sp`] computes with.
    #[must_use]
    pub const fn percent(self) -> i32 {
        match self {
            Self::Small => 20,
            Self::Comfy => 30,
            Self::Large => 40,
        }
    }

    /// Map an arbitrary stored fraction back onto the nearest supported preset.
    ///
    /// Comparisons only: below the Small/Comfy midpoint (0.25) is `Small`,
    /// above the Comfy/Large midpoint (0.35) is `Large`, and everything from
    /// 0.25 to 0.35 inclusive is `Comfy`, so a value exactly at a midpoint
    /// resolves to `Comfy`. A non-finite value resolves to [`Self::DEFAULT`]:
    /// the settings key has no schema range and GVariant text parses `nan` and
    /// `inf`, and the original nearest-delta form silently turned NaN and
    /// `-inf` into `Large`. Kani proves this over every `f64`
    /// (`width_preset/kani_proofs.rs`).
    #[must_use]
    pub fn from_fraction(fraction: f64) -> Self {
        if !fraction.is_finite() {
            Self::DEFAULT
        } else if fraction < SMALL_COMFY_MIDPOINT {
            Self::Small
        } else if fraction > COMFY_LARGE_MIDPOINT {
            Self::Large
        } else {
            Self::Comfy
        }
    }

    /// Convert the preset into a stable position for Adwaita combo rows.
    #[must_use]
    pub const fn index(self) -> u32 {
        match self {
            Self::Small => 0,
            Self::Comfy => 1,
            Self::Large => 2,
        }
    }

    /// Convert a combo-row selection back into a workspace width preset.
    #[must_use]
    pub const fn from_index(index: u32) -> Option<Self> {
        match index {
            0 => Some(Self::Small),
            1 => Some(Self::Comfy),
            2 => Some(Self::Large),
            _ => None,
        }
    }

    /// Lower bound, in whole sp, for this preset once the sidebar is
    /// side-by-side on desktop widths.
    #[must_use]
    pub const fn min_width_sp(self) -> i32 {
        match self {
            Self::Small => 220,
            Self::Comfy => 280,
            Self::Large => 340,
        }
    }

    /// Upper bound, in whole sp, that keeps the sidebar comfortable on wide and
    /// ultrawide windows.
    #[must_use]
    pub const fn max_width_sp(self) -> i32 {
        match self {
            Self::Small => 280,
            Self::Comfy => 360,
            Self::Large => 440,
        }
    }

    /// Convert the preset's hint into a bounded visible width, in whole sp,
    /// for the current window: `clamp(max(window_width, 1) * percent / 100,
    /// min_width_sp, max_width_sp)`, with the product floored.
    ///
    /// Integer-only. The split-view fraction the window needs is this width
    /// divided by the window width, and that conversion happens once, in the
    /// GTK adapter (`ui/window/geometry/execution.rs`). Kani proves, for every
    /// `i32` width (zero or less counts as 1 sp), that the result is the
    /// formula, lies within the preset's bounds, and never decreases as the
    /// width grows.
    #[must_use]
    pub fn clamped_width_sp(self, window_width: i32) -> i32 {
        let hinted = i64::from(window_width.max(1)) * i64::from(self.percent()) / 100;
        let clamped = hinted.clamp(
            i64::from(self.min_width_sp()),
            i64::from(self.max_width_sp()),
        );
        i32::try_from(clamped).unwrap_or_else(|_| self.max_width_sp())
    }
}

/// The midpoint between the `Small` and `Comfy` hint fractions.
const SMALL_COMFY_MIDPOINT: f64 = 0.25;

/// The midpoint between the `Comfy` and `Large` hint fractions.
const COMFY_LARGE_MIDPOINT: f64 = 0.35;

#[cfg(test)]
mod tests {
    use super::WorkspaceSidebarWidthPreset;

    #[test]
    fn presets_and_midpoints_resolve_by_comparison() {
        use WorkspaceSidebarWidthPreset::{Comfy, Large, Small};
        for preset in WorkspaceSidebarWidthPreset::ALL {
            assert_eq!(
                WorkspaceSidebarWidthPreset::from_fraction(preset.fraction()),
                preset
            );
        }
        // Midpoints resolve to Comfy, matching the old tie order; the strict
        // neighbours on either side do not. The equivalence with the old
        // nearest-delta form is a property test
        // (`tests/properties/width_preset.rs`), because the reference needs the
        // float arithmetic this module denies.
        assert_eq!(WorkspaceSidebarWidthPreset::from_fraction(0.25), Comfy);
        assert_eq!(WorkspaceSidebarWidthPreset::from_fraction(0.35), Comfy);
        assert_eq!(
            WorkspaceSidebarWidthPreset::from_fraction(0.25f64.next_down()),
            Small
        );
        assert_eq!(
            WorkspaceSidebarWidthPreset::from_fraction(0.35f64.next_up()),
            Large
        );
        assert_eq!(WorkspaceSidebarWidthPreset::from_fraction(-1.0), Small);
        assert_eq!(WorkspaceSidebarWidthPreset::from_fraction(1.0), Large);
    }

    #[test]
    fn clamped_width_is_the_floored_percentage_within_the_bounds() {
        use WorkspaceSidebarWidthPreset::{Comfy, Large, Small};
        assert_eq!(Small.clamped_width_sp(900), 220);
        assert_eq!(Comfy.clamped_width_sp(1001), 300);
        assert_eq!(Comfy.clamped_width_sp(1200), 360);
        assert_eq!(Large.clamped_width_sp(1400), 440);
        assert_eq!(Comfy.clamped_width_sp(i32::MIN), 280);
        assert_eq!(Large.clamped_width_sp(i32::MAX), 440);
    }

    #[test]
    fn non_finite_stored_fraction_resolves_to_default() {
        for fraction in [f64::NAN, f64::NEG_INFINITY, f64::INFINITY] {
            assert_eq!(
                WorkspaceSidebarWidthPreset::from_fraction(fraction),
                WorkspaceSidebarWidthPreset::Comfy,
                "stored fraction {fraction}"
            );
        }
    }
}
