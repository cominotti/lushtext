// SPDX-License-Identifier: MIT OR Apache-2.0

//! A128: `GtkSourceSearchSettings::set_at_word_boundaries(true)` finds, on an
//! ASCII-identifier fixture, the occurrences that workspace search's
//! whole-word mode finds: those not preceded or followed by an ASCII word
//! character (`[A-Za-z0-9_]`, the `grep-regex` word rule on ASCII text): an
//! underscore and a digit join a word, a dot and brackets do not. The probe
//! records, as values only, what each finds on non-ASCII text (an accented
//! letter, a combining mark, `ñ`); on GtkSourceView 5.20, 5.22 and 5.23 they
//! agree there too.
//!
//! "Open and Replace in Editor" (`stream-replace-all` design D7) carries the
//! whole-word toggle of a workspace Replace All into the editor's search bar,
//! so the editor must find the same occurrences the workspace search did.

use sourceview5::prelude::*;

use crate::observation::Recorder;
use crate::{AxiomId, Observation};

/// The searched word.
pub const NEEDLE: &str = "foo";
/// Identifiers and separators around [`NEEDLE`]: alone, inside a longer
/// identifier, beside an underscore, a digit, a dot and brackets.
pub const ASCII_DOCUMENT: &str = "foo foobar barfoo foo_bar bar_foo foo.foo (foo) 2foo foo2 foo";
/// The searched word on non-ASCII text.
pub const UNICODE_NEEDLE: &str = "caf";
/// [`UNICODE_NEEDLE`] followed by an accented letter, a combining mark and
/// a space.
pub const UNICODE_DOCUMENT: &str = "caf café cafe\u{301} caf_ ñcaf caf";

/// Character offsets of `needle` in `text` that no ASCII word character
/// touches: workspace search's whole-word rule on ASCII text.
#[must_use]
pub fn ascii_whole_word_offsets(text: &str, needle: &str) -> Vec<usize> {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
    text.match_indices(needle)
        .filter(|(byte, _)| {
            !word(text[..*byte].chars().next_back())
                && !word(text[byte + needle.len()..].chars().next())
        })
        .map(|(byte, _)| text[..byte].chars().count())
        .collect()
}

/// Character offsets of `needle` in `text` bounded by Unicode word
/// characters (letters, marks, digits, connector punctuation): workspace
/// search's whole-word rule beyond ASCII, approximated for the record.
#[must_use]
pub fn unicode_whole_word_offsets(text: &str, needle: &str) -> Vec<usize> {
    let word = |c: Option<char>| {
        c.is_some_and(|c| c.is_alphanumeric() || c == '_' || ('\u{300}'..='\u{36f}').contains(&c))
    };
    text.match_indices(needle)
        .filter(|(byte, _)| {
            !word(text[..*byte].chars().next_back())
                && !word(text[byte + needle.len()..].chars().next())
        })
        .map(|(byte, _)| text[..byte].chars().count())
        .collect()
}

/// The occurrences GtkSourceView finds for `needle` in `text` with
/// `at-word-boundaries` on, regex on or off, as character offsets.
#[must_use]
pub fn word_boundary_offsets(text: &str, needle: &str, regex: bool) -> Vec<usize> {
    let buffer = sourceview5::Buffer::new(None);
    buffer.set_text(text);
    let settings = sourceview5::SearchSettings::new();
    settings.set_search_text(Some(needle));
    settings.set_at_word_boundaries(true);
    settings.set_regex_enabled(regex);
    settings.set_wrap_around(false);
    let context = sourceview5::SearchContext::new(&buffer, Some(&settings));
    let mut at = buffer.start_iter();
    let mut found = Vec::new();
    while let Some((start, end, wrapped)) = context.forward(&at) {
        if wrapped {
            break;
        }
        found.push(usize::try_from(start.offset()).unwrap_or(usize::MAX));
        at = end;
    }
    found
}

/// Probe A128.
#[must_use]
pub fn probe_a128() -> Observation {
    Recorder::run(AxiomId::new(128), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let expected = ascii_whole_word_offsets(ASCII_DOCUMENT, NEEDLE);
        let plain = word_boundary_offsets(ASCII_DOCUMENT, NEEDLE, false);
        let regex = word_boundary_offsets(ASCII_DOCUMENT, NEEDLE, true);
        recorder.measure("ascii_workspace_whole_word", format!("{expected:?}"));
        recorder.measure("ascii_plain_at_word_boundaries", format!("{plain:?}"));
        recorder.measure("ascii_regex_at_word_boundaries", format!("{regex:?}"));
        let unicode_expected = unicode_whole_word_offsets(UNICODE_DOCUMENT, UNICODE_NEEDLE);
        recorder.measure(
            "unicode_workspace_whole_word",
            format!("{unicode_expected:?}"),
        );
        recorder.measure(
            "unicode_plain_at_word_boundaries",
            format!(
                "{:?}",
                word_boundary_offsets(UNICODE_DOCUMENT, UNICODE_NEEDLE, false)
            ),
        );
        recorder.measure(
            "unicode_regex_at_word_boundaries",
            format!(
                "{:?}",
                word_boundary_offsets(UNICODE_DOCUMENT, UNICODE_NEEDLE, true)
            ),
        );
        recorder.control(
            !expected.is_empty() && expected.len() < ASCII_DOCUMENT.matches(NEEDLE).count(),
            "control: the fixture has whole-word and embedded occurrences",
        )?;
        recorder.axiom(
            plain == expected,
            "plain-text at-word-boundaries finds workspace whole-word's ASCII occurrences",
        )?;
        recorder.axiom(
            regex == expected,
            "regex at-word-boundaries finds workspace whole-word's ASCII occurrences",
        )
    })
}
