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
//! The policy is integer-only apart from those stored fractions, which it only
//! compares: the split-view fraction is formed in the GTK adapter.
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

/// A preset's position in the Small < Comfy < Large order.
fn rank(preset: WorkspaceSidebarWidthPreset) -> u32 {
    preset.index()
}

/// `clamped_width_sp` is the spec formula
/// `clamp(max(window_width, 1) * percent / 100, min_width_sp, max_width_sp)`
/// with the product floored, and it lies within the preset's bounds, for every
/// `i32` width.
#[kani::proof]
fn clamp_matches_the_spec_formula_and_bounds() {
    let preset = any_preset();
    let window_width: i32 = kani::any();
    let width = preset.clamped_width_sp(window_width);
    let hinted = i64::from(window_width.max(1)) * i64::from(preset.percent()) / 100;
    let expected = hinted.clamp(
        i64::from(preset.min_width_sp()),
        i64::from(preset.max_width_sp()),
    );
    assert!(i64::from(width) == expected);
    assert!(width >= preset.min_width_sp() && width <= preset.max_width_sp());
    assert!(width >= 1);
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

/// The integer percentage is the stored hint fraction, exactly.
#[kani::proof]
fn percent_is_the_hint_fraction() {
    let preset = any_preset();
    assert!(f64::from(preset.percent()) == preset.fraction() * 100.0);
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

/// `from_fraction` resolves a non-finite value to the default preset; for
/// finite values it never moves down as the stored value grows, it resolves
/// each midpoint (0.25, 0.35) to `Comfy`, and the preset it picks is at least
/// as near as any other (up to the rounding of the reference deltas, checked
/// for `|fraction| <= 2`, which contains every value the key has ever held).
#[kani::proof]
fn from_fraction_picks_the_nearest_preset() {
    let fraction: f64 = kani::any();
    let resolved = WorkspaceSidebarWidthPreset::from_fraction(fraction);
    if !fraction.is_finite() {
        assert!(resolved == WorkspaceSidebarWidthPreset::DEFAULT);
        return;
    }
    let larger: f64 = kani::any();
    kani::assume(larger.is_finite() && fraction <= larger);
    assert!(rank(resolved) <= rank(WorkspaceSidebarWidthPreset::from_fraction(larger)));
    if fraction == 0.25 || fraction == 0.35 {
        assert!(resolved == WorkspaceSidebarWidthPreset::Comfy);
    }
    if fraction.abs() <= 2.0 {
        let delta = |preset: WorkspaceSidebarWidthPreset| (fraction - preset.fraction()).abs();
        for other in WorkspaceSidebarWidthPreset::ALL {
            assert!(delta(resolved) <= delta(other) + 1e-15);
        }
    }
}
