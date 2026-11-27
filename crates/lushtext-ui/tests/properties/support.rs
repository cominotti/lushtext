// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared bounded generators and runner configuration for UI property tests.

use std::env;

use proptest::test_runner::{Config as ProptestConfig, FileFailurePersistence};

/// Default case count for pull-request and local property runs.
pub const DEFAULT_PROPERTY_CASES: u32 = 64;
/// Upper bound for opt-in deep runs set through `LUSHTEXT_PROPTEST_CASES`.
pub const MAX_PROPERTY_CASES: u32 = 4096;
/// Environment variable used to raise the case count for manual/deep runs.
pub const PROPERTY_CASES_ENV: &str = "LUSHTEXT_PROPTEST_CASES";
/// Maximum number of shrink iterations per failing generated case.
pub const PROPERTY_MAX_SHRINK_ITERS: u32 = 1024;
/// Per-case timeout in milliseconds.
pub const PROPERTY_TIMEOUT_MS: u32 = 10_000;
/// Reviewable file where minimized failing cases are persisted.
pub const PROPERTY_REGRESSION_FILE: &str = "proptest-regressions/properties.txt";

/// Build the common `proptest` runner configuration for every property block.
#[must_use]
pub fn property_config() -> ProptestConfig {
    ProptestConfig {
        cases: configured_case_count(),
        max_shrink_iters: PROPERTY_MAX_SHRINK_ITERS,
        timeout: PROPERTY_TIMEOUT_MS,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            PROPERTY_REGRESSION_FILE,
        ))),
        ..ProptestConfig::default()
    }
}

/// Resolve the requested case count, falling back to the CI-safe default.
fn configured_case_count() -> u32 {
    env::var(PROPERTY_CASES_ENV)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|cases| *cases > 0)
        .map_or(DEFAULT_PROPERTY_CASES, |cases| {
            cases.min(MAX_PROPERTY_CASES)
        })
}

/// `text` split at the scalar boundaries at or below `cuts` (each taken modulo
/// `text.len() + 1`), in order; the pieces concatenate back to `text`.
#[must_use]
pub fn split_at_scalar_cuts<'a>(text: &'a str, cuts: &[usize]) -> Vec<&'a str> {
    let mut boundaries: Vec<usize> = cuts
        .iter()
        .map(|&cut| text.floor_char_boundary(cut % (text.len() + 1)))
        .collect();
    boundaries.sort_unstable();
    let mut pieces = Vec::with_capacity(boundaries.len() + 1);
    let mut start = 0;
    for boundary in boundaries {
        pieces.push(&text[start..boundary]);
        start = boundary;
    }
    pieces.push(&text[start..]);
    pieces
}
