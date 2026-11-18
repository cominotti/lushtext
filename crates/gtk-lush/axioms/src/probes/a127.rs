// SPDX-License-Identifier: MIT OR Apache-2.0

//! A127: with regex enabled, `GtkSourceSearchContext::replace_all` expands
//! the replacement through GRegex's reference syntax: `\N` and `\g<N>` are
//! group `N`, `\g<name>` is a named group, `\g<1>0` is group 1 followed by a
//! literal `0` (the bracketed form ends the number), and `$1` is literal
//! text, not a reference.
//!
//! LushText's "Open and Replace in Editor" (`stream-replace-all` design D7)
//! hands a workspace Replace All's request to the editor's search bar. The
//! workspace engine is Rust `regex`, whose references are `$N`, `${N}` and
//! `$name`; the translator `rust_replacement_to_gtksourceview`
//! (`ui/search_panel/policy.rs`) turns them into this syntax, so each row of
//! its table rests on this axiom.

use sourceview5::prelude::*;

use crate::observation::Recorder;
use crate::{AxiomId, Observation};

/// The text each replacement runs over: two matches of [`PATTERN`].
pub const DOCUMENT: &str = "foo-bar baz-qux";
/// The search pattern: a named first group and a numbered second one.
pub const PATTERN: &str = r"(?P<first>\w+)-(\w+)";
/// Each replacement and what the buffer holds after `replace_all` with it.
pub const EXPANSIONS: [(&str, &str); 6] = [
    (r"\2-\1", "bar-foo qux-baz"),
    (r"\g<2>-\g<1>", "bar-foo qux-baz"),
    (r"\g<first>!", "foo! baz!"),
    (r"\g<1>0", "foo0 baz0"),
    (r"\0.", "foo-bar. baz-qux."),
    ("$1", "$1 $1"),
];

/// A buffer holding [`DOCUMENT`] and a regex search context for [`PATTERN`].
#[must_use]
pub fn regex_search() -> (sourceview5::Buffer, sourceview5::SearchContext) {
    let buffer = sourceview5::Buffer::new(None);
    buffer.set_text(DOCUMENT);
    let settings = sourceview5::SearchSettings::new();
    settings.set_regex_enabled(true);
    settings.set_search_text(Some(PATTERN));
    settings.set_wrap_around(false);
    let context = sourceview5::SearchContext::new(&buffer, Some(&settings));
    (buffer, context)
}

/// Run `replace_all` with `replacement` on a fresh buffer, and return the
/// replacement count and the text left.
///
/// # Errors
///
/// Returns GtkSourceView's error text when the replacement is refused.
pub fn replace_all_with(replacement: &str) -> Result<(u32, String), String> {
    let (buffer, context) = regex_search();
    let count = context
        .replace_all(replacement)
        .map_err(|error| error.to_string())?;
    let (start, end) = buffer.bounds();
    Ok((count, buffer.text(&start, &end, false).to_string()))
}

/// Probe A127.
#[must_use]
pub fn probe_a127() -> Observation {
    Recorder::run(AxiomId::new(127), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let (_, context) = regex_search();
        recorder.control(
            context.regex_error().is_none(),
            "control: the search pattern compiles",
        )?;
        let results: Vec<_> = EXPANSIONS
            .iter()
            .map(|(replacement, _)| replace_all_with(replacement))
            .collect();
        let keys = [
            "backslash_number",
            "bracketed_number",
            "named_group",
            "bracketed_number_then_digit",
            "whole_match",
            "dollar_number",
        ];
        for (key, result) in keys.into_iter().zip(&results) {
            recorder.measure(key, format!("{result:?}"));
        }
        recorder.control(
            results.iter().all(|result| matches!(result, Ok((2, _)))),
            "control: every replacement replaced both matches",
        )?;
        let expanded =
            |index: usize| matches!(&results[index], Ok((_, text)) if text == EXPANSIONS[index].1);
        recorder.axiom(
            expanded(0) && expanded(1) && expanded(4),
            r"\N and \g<N> expand to group N, and \0 to the whole match",
        )?;
        recorder.axiom(expanded(2), r"\g<name> expands to the named group")?;
        recorder.axiom(expanded(3), r"\g<1>0 is group 1 followed by a literal 0")?;
        recorder.axiom(expanded(5), "$1 is literal text, not a reference")
    })
}
