// SPDX-License-Identifier: GPL-3.0-or-later

//! Smoke-only overrides of the workspace list budgets
//! (`bound-lists-by-budget-not-count` task 6.2).
//!
//! The accessibility and visual smokes must show a budget-limited workspace
//! search and a budget-limited directory. The real budgets are hundreds of MiB,
//! which no smoke fixture should reach, so with the `smoke-list-budgets` Cargo
//! feature [`search_retained_bytes`] reads `LUSHTEXT_SEARCH_RETAINED_BYTES` and
//! [`sidebar_directory_retained_bytes`] reads
//! `LUSHTEXT_SIDEBAR_DIRECTORY_RETAINED_BYTES`, once each. Without the feature
//! (every release, Meson, Flatpak and Snap build, and the ordinary debug
//! binary) both are empty inline functions and no environment read is compiled,
//! the `smoke-read-faults` pattern.

/// The search budget a smoke named in `LUSHTEXT_SEARCH_RETAINED_BYTES`.
#[cfg(feature = "smoke-list-budgets")]
#[must_use]
pub fn search_retained_bytes() -> Option<u64> {
    use std::sync::OnceLock;
    static VALUE: OnceLock<Option<u64>> = OnceLock::new();
    *VALUE.get_or_init(|| read("LUSHTEXT_SEARCH_RETAINED_BYTES"))
}

/// The search budget a smoke named; never one in a shipped build.
#[cfg(not(feature = "smoke-list-budgets"))]
#[inline]
#[must_use]
pub const fn search_retained_bytes() -> Option<u64> {
    None
}

/// The directory budget a smoke named in
/// `LUSHTEXT_SIDEBAR_DIRECTORY_RETAINED_BYTES`.
#[cfg(feature = "smoke-list-budgets")]
#[must_use]
pub fn sidebar_directory_retained_bytes() -> Option<u64> {
    use std::sync::OnceLock;
    static VALUE: OnceLock<Option<u64>> = OnceLock::new();
    *VALUE.get_or_init(|| read("LUSHTEXT_SIDEBAR_DIRECTORY_RETAINED_BYTES"))
}

/// The directory budget a smoke named; never one in a shipped build.
#[cfg(not(feature = "smoke-list-budgets"))]
#[inline]
#[must_use]
pub const fn sidebar_directory_retained_bytes() -> Option<u64> {
    None
}

#[cfg(feature = "smoke-list-budgets")]
fn read(name: &str) -> Option<u64> {
    let value = std::env::var(name)
        .ok()?
        .parse::<u64>()
        .ok()
        .filter(|bytes| *bytes > 0)?;
    eprintln!("{name}: list budget overridden to {value} bytes");
    Some(value)
}

#[cfg(test)]
mod tests {
    /// The audit of task 6.2: a build without the feature compiles no
    /// environment read, and both overrides are absent.
    #[cfg(not(feature = "smoke-list-budgets"))]
    #[test]
    fn a_build_without_the_feature_has_no_override() {
        assert_eq!(super::search_retained_bytes(), None);
        assert_eq!(super::sidebar_directory_retained_bytes(), None);
        let source = include_str!("smoke_list_budgets.rs");
        let gated = source
            .matches(concat!("#[cfg(feature = ", "\"smoke-list-budgets\")]"))
            .count();
        let reads = source.matches(concat!("std::env::", "var(")).count();
        assert_eq!(reads, 1, "one environment read, behind the feature");
        assert!(gated >= 3);
    }
}
