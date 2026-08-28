// SPDX-License-Identifier: GPL-3.0-or-later

//! Named workspace sidebar width presets.
//!
//! # Ownership
//!
//! This module is **cross-cutting, and it is not the workspace tree workflow's**.
//! It is the `workspace-sidebar-width-policy` capability's value, owned by
//! `WFR-SHELL-LAYOUT` (slot 7): Preferences renders it as a picker, and the window
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
//! Its three consumers are `ui/preferences/imp.rs`, `ui/window/policy.rs`,
//! and `ui/window/imp.rs`.

/// Supported named workspace sidebar presets used by Preferences and shell math.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

    /// Map an arbitrary stored fraction back onto the nearest supported preset.
    #[must_use]
    pub fn from_fraction(fraction: f64) -> Self {
        let small_delta = (fraction - Self::Small.fraction()).abs();
        let comfy_delta = (fraction - Self::Comfy.fraction()).abs();
        let large_delta = (fraction - Self::Large.fraction()).abs();
        let min_delta = small_delta.min(comfy_delta.min(large_delta));

        if (comfy_delta - min_delta).abs() < f64::EPSILON {
            Self::Comfy
        } else if (small_delta - min_delta).abs() < f64::EPSILON {
            Self::Small
        } else {
            Self::Large
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

    /// Lower bound for this preset once the sidebar is side-by-side on desktop widths.
    #[must_use]
    pub const fn min_width_sp(self) -> f64 {
        match self {
            Self::Small => 220.0,
            Self::Comfy => 280.0,
            Self::Large => 340.0,
        }
    }

    /// Upper bound that keeps the sidebar comfortable on wide and ultrawide windows.
    #[must_use]
    pub const fn max_width_sp(self) -> f64 {
        match self {
            Self::Small => 280.0,
            Self::Comfy => 360.0,
            Self::Large => 440.0,
        }
    }

    /// Convert the preset's hint fraction into a bounded visible width for the current window.
    #[must_use]
    pub fn clamped_width_sp(self, window_width: i32) -> f64 {
        (f64::from(window_width.max(1)) * self.fraction())
            .clamp(self.min_width_sp(), self.max_width_sp())
    }

    /// Return the effective split-view fraction after clamping this preset for the window width.
    #[must_use]
    pub fn effective_fraction(self, window_width: i32) -> f64 {
        (self.clamped_width_sp(window_width) / f64::from(window_width.max(1))).min(1.0)
    }
}
