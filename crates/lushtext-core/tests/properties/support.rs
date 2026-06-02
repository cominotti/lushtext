// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared bounded generators and runner configuration for property tests.
//!
//! Keeping limits in one place makes the CI cost reviewable and gives deeper
//! manual runs a single opt-in knob instead of scattered per-test constants.

use std::env;
use std::path::PathBuf;

use proptest::prelude::*;
use proptest::test_runner::{Config as ProptestConfig, FileFailurePersistence};

/// Default case count for pull-request and local property runs.
///
/// Sixty-four cases keeps the first lane quick while still covering many more
/// combinations than the deterministic example tests.
pub const DEFAULT_PROPERTY_CASES: u32 = 64;
/// Upper bound for opt-in deep runs set through `LUSHTEXT_PROPTEST_CASES`.
///
/// The cap prevents an accidental environment value from making CI or local
/// verification look hung while still allowing a meaningful scheduled pass.
pub const MAX_PROPERTY_CASES: u32 = 4096;
/// Environment variable used to raise the case count for manual/deep runs.
pub const PROPERTY_CASES_ENV: &str = "LUSHTEXT_PROPTEST_CASES";
/// Maximum number of shrink iterations per failing generated case.
///
/// A thousand shrink attempts is enough to minimize these small strings and
/// path vectors without spending mutation-test-scale time on one failure.
pub const PROPERTY_MAX_SHRINK_ITERS: u32 = 1024;
/// Per-case timeout in milliseconds.
///
/// All current properties are pure CPU work over tiny inputs, so ten seconds is
/// deliberately generous and mainly guards against accidental unbounded loops.
pub const PROPERTY_TIMEOUT_MS: u32 = 10_000;
/// Reviewable file where minimized failing cases are persisted.
///
/// The directory is committed with a README so a failure-created `properties.txt`
/// file appears in a predictable, documented location.
pub const PROPERTY_REGRESSION_FILE: &str = "proptest-regressions/properties.txt";
/// Maximum characters in generated single-line text fragments.
pub const MAX_TEXT_FRAGMENT_CHARS: usize = 24;
/// Maximum path components in generated relative paths.
pub const MAX_PATH_SEGMENTS: usize = 4;
/// Maximum elements in generated vectors used by the initial properties.
pub const MAX_VECTOR_LEN: usize = 8;
/// Maximum bytes used for sidecar hash samples.
pub const MAX_BYTE_VECTOR_LEN: usize = 32;

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

/// Generate a bounded non-empty single-line fragment.
pub fn text_fragment() -> impl Strategy<Value = String> {
    (
        non_space_fragment_char(),
        prop::collection::vec(fragment_char(), 0..MAX_TEXT_FRAGMENT_CHARS),
    )
        .prop_map(|(first, rest)| {
            let mut text = String::new();
            text.push(first);
            text.extend(rest);
            text
        })
}

/// Generate a bounded single-line fragment that may be empty.
pub fn optional_text_fragment() -> impl Strategy<Value = String> {
    prop::collection::vec(fragment_char(), 0..=MAX_TEXT_FRAGMENT_CHARS)
        .prop_map(|chars| chars.into_iter().collect())
}

/// Generate a component-safe relative path suffix.
pub fn path_suffix() -> impl Strategy<Value = PathBuf> {
    prop::collection::vec(path_segment(), 0..=MAX_PATH_SEGMENTS).prop_map(|segments| {
        let mut path = PathBuf::new();
        for segment in segments {
            path.push(segment);
        }
        path
    })
}

/// Generate one path component without separators or platform-sensitive bytes.
fn path_segment() -> impl Strategy<Value = String> {
    prop::collection::vec(path_char(), 1..=12).prop_map(|chars| chars.into_iter().collect())
}

/// Generate a readable ASCII fragment character with no Markdown delimiters.
fn fragment_char() -> impl Strategy<Value = char> {
    (0u8..=39).prop_map(|code| match code {
        0..=25 => char::from(b'a' + code),
        26..=35 => char::from(b'0' + (code - 26)),
        36 => ' ',
        37 => '_',
        38 => '-',
        _ => '.',
    })
}

/// Generate the first character for fragments that must stay non-blank.
fn non_space_fragment_char() -> impl Strategy<Value = char> {
    (0u8..=38).prop_map(|code| match code {
        0..=25 => char::from(b'a' + code),
        26..=35 => char::from(b'0' + (code - 26)),
        36 => '_',
        37 => '-',
        _ => '.',
    })
}

/// Generate one path-safe ASCII character.
fn path_char() -> impl Strategy<Value = char> {
    (0u8..=37).prop_map(|code| match code {
        0..=25 => char::from(b'a' + code),
        26..=35 => char::from(b'0' + (code - 26)),
        36 => '_',
        _ => '-',
    })
}
