// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over [`WorkspaceSidebarWidthPreset`], the
//! `workspace-sidebar-width-policy` capability's value.
//!
//! Domain: every preset, every `i32` window width (a width of zero or less
//! counts as 1 sp, as the functions themselves say), every `u32` combo-row
//! index, and every `f64` stored fraction — NaN and both infinities included,
//! because the `workspace-sidebar-width-fraction` key has no schema range and
//! GVariant text syntax parses `nan` and `inf`.
//!
//! Compiled only under `cfg(kani)` (`make kani`); ordinary builds and tests
//! never see this module. The harnesses check the functions that ship.

use super::WorkspaceSidebarWidthPreset;

/// Any of the three presets.
fn any_preset() -> WorkspaceSidebarWidthPreset {
    let index: u32 = kani::any();
    kani::assume(index < 3);
    WorkspaceSidebarWidthPreset::from_index(index).expect("indices 0..3 name presets")
}

/// `clamped_width_sp` is the spec formula
/// `clamp(max(window_width, 1) * hint_fraction, min_width_sp, max_width_sp)`,
/// and it lies within the preset's bounds, for every `i32` width.
#[kani::proof]
fn clamp_matches_the_spec_formula_and_bounds() {
    let preset = any_preset();
    let window_width: i32 = kani::any();
    let width = preset.clamped_width_sp(window_width);
    let raw = f64::from(window_width.max(1)) * preset.fraction();
    let expected = if raw < preset.min_width_sp() {
        preset.min_width_sp()
    } else if raw > preset.max_width_sp() {
        preset.max_width_sp()
    } else {
        raw
    };
    assert!(width == expected);
    assert!(width >= preset.min_width_sp() && width <= preset.max_width_sp());
}

/// `clamped_width_sp` never decreases as the window width grows.
#[kani::proof]
fn clamp_is_monotone_in_window_width() {
    let preset = any_preset();
    let narrow: i32 = kani::any();
    let wide: i32 = kani::any();
    kani::assume(narrow <= wide);
    assert!(preset.clamped_width_sp(narrow) <= preset.clamped_width_sp(wide));
}

/// `effective_fraction` is finite and in (0, 1] for every `i32` width.
#[kani::proof]
fn effective_fraction_is_in_unit_interval() {
    let preset = any_preset();
    let fraction = preset.effective_fraction(kani::any());
    assert!(fraction.is_finite());
    assert!(fraction > 0.0 && fraction <= 1.0);
}

/// `from_index(index())` is the identity, and every index from 3 up names no
/// preset.
#[kani::proof]
fn index_round_trips() {
    let preset = any_preset();
    assert!(WorkspaceSidebarWidthPreset::from_index(preset.index()) == Some(preset));
    let index: u32 = kani::any();
    kani::assume(index >= 3);
    assert!(WorkspaceSidebarWidthPreset::from_index(index).is_none());
}

/// Storing a preset as its hint fraction and reading it back restores it.
#[kani::proof]
fn fraction_round_trips() {
    let preset = any_preset();
    assert!(WorkspaceSidebarWidthPreset::from_fraction(preset.fraction()) == preset);
}

/// `from_fraction` resolves a non-finite value to the default preset, and a
/// finite value to a preset whose hint fraction is within `f64::EPSILON` of the
/// nearest, following the tie order Comfy, Small, Large.
#[kani::proof]
fn from_fraction_picks_the_nearest_preset() {
    let fraction: f64 = kani::any();
    let resolved = WorkspaceSidebarWidthPreset::from_fraction(fraction);
    if !fraction.is_finite() {
        assert!(resolved == WorkspaceSidebarWidthPreset::DEFAULT);
        return;
    }
    let delta = |preset: WorkspaceSidebarWidthPreset| (fraction - preset.fraction()).abs();
    let nearest = delta(WorkspaceSidebarWidthPreset::Small)
        .min(delta(WorkspaceSidebarWidthPreset::Comfy))
        .min(delta(WorkspaceSidebarWidthPreset::Large));
    let ties = |preset| delta(preset) - nearest < f64::EPSILON;
    assert!(ties(resolved));
    let expected = if ties(WorkspaceSidebarWidthPreset::Comfy) {
        WorkspaceSidebarWidthPreset::Comfy
    } else if ties(WorkspaceSidebarWidthPreset::Small) {
        WorkspaceSidebarWidthPreset::Small
    } else {
        WorkspaceSidebarWidthPreset::Large
    };
    assert!(resolved == expected);
}
