// SPDX-License-Identifier: MIT OR Apache-2.0

//! A129: GRegex, through `GtkSourceSearchContext`, compiles the Rust-`regex`
//! constructs that "Open and Replace in Editor" passes through unchanged:
//! character classes, `\b`, `\d`, `\w`, the inline `(?i)` flag and named
//! groups in both the `(?P<n>…)` and `(?<n>…)` spellings, and each finds the
//! match the Rust engine finds on the probe text. A construct that fails is
//! recorded by name.
//!
//! The translator `rust_replacement_to_gtksourceview`
//! (`ui/search_panel/policy.rs`, `stream-replace-all` design D7) passes the
//! search pattern through, so a pattern whose construct GRegex rejects or
//! reads differently would make the editor search for something else.

use sourceview5::prelude::*;

use crate::observation::Recorder;
use crate::{AxiomId, Observation};

/// The text every pattern runs over.
pub const DOCUMENT: &str = "Alpha beta_2 42 GAMMA delta";
/// Each passed-through construct, its pattern, and the first match the Rust
/// `regex` engine finds in [`DOCUMENT`].
pub const CONSTRUCTS: [(&str, &str, &str); 7] = [
    ("class", "[a-z]+_[0-9]", "beta_2"),
    ("word_boundary", r"\bdelta\b", "delta"),
    ("digit", r"\d+", "2"),
    ("word", r"\w+", "Alpha"),
    ("inline_case_insensitive", "(?i)gamma", "GAMMA"),
    ("named_group_p", r"(?P<n>\d\d)", "42"),
    ("named_group_angle", r"(?<n>\d\d)", "42"),
];

/// The first match GtkSourceView's regex search finds for `pattern` in
/// [`DOCUMENT`].
///
/// # Errors
///
/// Returns the compile error's text when GRegex rejects `pattern`.
pub fn first_match(pattern: &str) -> Result<Option<String>, String> {
    let buffer = sourceview5::Buffer::new(None);
    buffer.set_text(DOCUMENT);
    let settings = sourceview5::SearchSettings::new();
    settings.set_regex_enabled(true);
    settings.set_case_sensitive(true);
    settings.set_search_text(Some(pattern));
    settings.set_wrap_around(false);
    let context = sourceview5::SearchContext::new(&buffer, Some(&settings));
    if let Some(error) = context.regex_error() {
        return Err(error.to_string());
    }
    Ok(context
        .forward(&buffer.start_iter())
        .map(|(start, end, _)| buffer.text(&start, &end, false).to_string()))
}

/// Probe A129.
#[must_use]
pub fn probe_a129() -> Observation {
    Recorder::run(AxiomId::new(129), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let results: Vec<_> = CONSTRUCTS
            .iter()
            .map(|(_, pattern, _)| first_match(pattern))
            .collect();
        for ((name, _, _), result) in CONSTRUCTS.iter().zip(&results) {
            recorder.measure(name, format!("{result:?}"));
        }
        let failing: Vec<&str> = CONSTRUCTS
            .iter()
            .zip(&results)
            .filter(|((_, _, expected), result)| {
                !matches!(result, Ok(Some(found)) if found == expected)
            })
            .map(|((name, _, _), _)| *name)
            .collect();
        recorder.measure("failing", format!("{failing:?}"));
        recorder.control(
            first_match("[").is_err(),
            "control: an invalid pattern reports a compile error",
        )?;
        recorder.axiom(
            failing.is_empty(),
            "GRegex compiles every passed-through construct and finds the Rust engine's match",
        )
    })
}
