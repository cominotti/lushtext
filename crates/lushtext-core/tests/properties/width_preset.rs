// SPDX-License-Identifier: GPL-3.0-or-later

//! `WorkspaceSidebarWidthPreset::from_fraction` became comparisons only (a
//! whole-pixel policy module denies float arithmetic). These properties pin it
//! to the nearest-delta form it replaced, which lives here as the reference
//! because it needs the arithmetic the policy module no longer may do.

use lushtext_core::ui::sidebar::width_preset::WorkspaceSidebarWidthPreset::{
    self, Comfy, Large, Small,
};
use proptest::prelude::*;

/// The nearest-delta form, with the non-finite guard it gained first.
fn legacy_from_fraction(fraction: f64) -> WorkspaceSidebarWidthPreset {
    if !fraction.is_finite() {
        return Comfy;
    }
    let small_delta = (fraction - Small.fraction()).abs();
    let comfy_delta = (fraction - Comfy.fraction()).abs();
    let large_delta = (fraction - Large.fraction()).abs();
    let min_delta = small_delta.min(comfy_delta.min(large_delta));
    if (comfy_delta - min_delta).abs() < f64::EPSILON {
        Comfy
    } else if (small_delta - min_delta).abs() < f64::EPSILON {
        Small
    } else {
        Large
    }
}

/// Where the nearest-delta form saw a tie within `f64::EPSILON` it said Comfy,
/// and the comparisons name the strictly nearer preset instead. That happens
/// in two places only: a few ulps around each midpoint, and at magnitudes so
/// large that the three deltas round to the same value. The deltas differ by at
/// least 0.1, so they stay distinct while the spacing of doubles near the value
/// is below that, which holds up to about 2^49 (5.6 * 10^14); the band starts at
/// 10^14 to leave margin. Those are the only permitted disagreements, and the
/// old answer there was always Comfy.
fn in_tie_band(fraction: f64) -> bool {
    (fraction - 0.25).abs() < 1e-12 || (fraction - 0.35).abs() < 1e-12 || fraction.abs() > 1e14
}

#[test]
fn comparisons_agree_with_the_nearest_delta_form_on_a_dense_sweep() {
    for step in -2_000_000i32..=4_000_000 {
        let fraction = f64::from(step) / 2_000_000.0;
        assert_eq!(
            WorkspaceSidebarWidthPreset::from_fraction(fraction),
            legacy_from_fraction(fraction),
            "stored fraction {fraction}"
        );
    }
    for preset in WorkspaceSidebarWidthPreset::ALL {
        assert_eq!(
            WorkspaceSidebarWidthPreset::from_fraction(preset.fraction()),
            preset
        );
        assert_eq!(legacy_from_fraction(preset.fraction()), preset);
    }
}

#[test]
fn the_tie_band_is_a_few_ulps_and_only_ever_said_comfy() {
    let mut band = 0u32;
    for midpoint in [0.25f64, 0.35] {
        let (mut below, mut above) = (midpoint, midpoint);
        for _ in 0..64 {
            below = below.next_down();
            above = above.next_up();
            for fraction in [below, above] {
                let new = WorkspaceSidebarWidthPreset::from_fraction(fraction);
                let old = legacy_from_fraction(fraction);
                if new != old {
                    assert_eq!(old, Comfy, "{fraction}: {new:?} against {old:?}");
                    band += 1;
                }
            }
        }
    }
    assert!(band <= 32, "the tie band grew to {band} ulps");
}

#[test]
fn huge_magnitudes_now_resolve_to_the_nearer_end() {
    assert_eq!(legacy_from_fraction(1e300), Comfy);
    assert_eq!(WorkspaceSidebarWidthPreset::from_fraction(1e300), Large);
    assert_eq!(legacy_from_fraction(-1e300), Comfy);
    assert_eq!(WorkspaceSidebarWidthPreset::from_fraction(-1e300), Small);
    // The shrunk counterexample CI's proptest seed found below 10^15.
    assert_eq!(legacy_from_fraction(-669_941_564_817_968.5), Comfy);
    assert_eq!(
        WorkspaceSidebarWidthPreset::from_fraction(-669_941_564_817_968.5),
        Small
    );
    // Below 10^14 the two forms agree at every sampled power of two.
    for exponent in 0..=46 {
        let magnitude = 2f64.powi(exponent);
        for fraction in [magnitude, -magnitude] {
            assert_eq!(
                WorkspaceSidebarWidthPreset::from_fraction(fraction),
                legacy_from_fraction(fraction),
                "{fraction}"
            );
        }
    }
}

proptest! {
    #[test]
    fn comparisons_agree_with_the_nearest_delta_form(fraction in any::<f64>()) {
        let new = WorkspaceSidebarWidthPreset::from_fraction(fraction);
        let old = legacy_from_fraction(fraction);
        if in_tie_band(fraction) {
            prop_assert!(new == old || old == Comfy);
        } else {
            prop_assert_eq!(new, old);
        }
    }
}
