// SPDX-License-Identifier: MIT OR Apache-2.0

//! A33: which text `GtkSourceSearchContext` sees depends on its settings'
//! `visible-only` property (GtkSourceView 5.12+), which **defaults to true**:
//! by default both the plain-text and the regex search **skip text tagged
//! `invisible`**. With `visible-only` false, the plain-text path **finds
//! matches inside hidden text**, while the regex path still sees **only the
//! visible** text (it always searches `gtk_text_iter_get_visible_text`). The
//! synchronous `forward` walk and the asynchronous occurrence count agree.
//!
//! Recorded beside it, as values only: how long a `forward` search for a
//! match at the end of a long line takes, at two line lengths.
//!
//! The protected view (C2) moves find off the stock search context; it must
//! not rely on hidden text being searched, nor on it being skipped, without
//! setting `visible-only` explicitly.

use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{ParagraphKind, invisible_tag, offset, paragraph, time_us};
use crate::session::{REALIZE_BUDGET, wait_until};
use crate::{AxiomId, Observation};

/// The searched word.
pub const NEEDLE: &str = "needle";
/// Visible occurrences of [`NEEDLE`] in the probe buffer.
pub const VISIBLE_MATCHES: usize = 2;
/// Hidden occurrences of [`NEEDLE`] in the probe buffer.
pub const HIDDEN_MATCHES: usize = 3;
/// The line lengths the per-match time is recorded at.
pub const TIMED_LENGTHS: [usize; 2] = [20_000, 200_000];

/// A buffer with [`VISIBLE_MATCHES`] visible and [`HIDDEN_MATCHES`] hidden
/// occurrences of [`NEEDLE`] on one line.
#[must_use]
pub fn mixed_buffer() -> sourceview5::Buffer {
    let buffer = sourceview5::Buffer::new(None);
    let tag = invisible_tag(buffer.upcast_ref());
    let visible = "visible needle and another needle then ";
    let hidden = "hidden needle, needle, needle end";
    buffer.set_text(&format!("{visible}{hidden}"));
    let start = buffer.iter_at_offset(offset(visible.chars().count()));
    buffer.apply_tag(&tag, &start, &buffer.end_iter());
    buffer
}

/// A search context over `buffer` for [`NEEDLE`], with regex on or off and
/// the settings' default `visible-only`.
#[must_use]
pub fn search(buffer: &sourceview5::Buffer, regex: bool) -> sourceview5::SearchContext {
    search_with(buffer, regex, None)
}

/// [`search`] with `visible-only` set explicitly when `visible_only` is
/// `Some`.
#[must_use]
pub fn search_with(
    buffer: &sourceview5::Buffer,
    regex: bool,
    visible_only: Option<bool>,
) -> sourceview5::SearchContext {
    let settings = sourceview5::SearchSettings::new();
    if let Some(visible_only) = visible_only {
        settings.set_visible_only(visible_only);
    }
    settings.set_search_text(Some(NEEDLE));
    settings.set_regex_enabled(regex);
    settings.set_wrap_around(false);
    sourceview5::SearchContext::new(buffer, Some(&settings))
}

/// Matches the synchronous `forward` walk finds.
#[must_use]
pub fn forward_matches(context: &sourceview5::SearchContext) -> usize {
    let buffer = context.buffer();
    let mut at = buffer.start_iter();
    let mut found = 0;
    while let Some((_, end, wrapped)) = context.forward(&at) {
        if wrapped {
            break;
        }
        found += 1;
        at = end;
    }
    found
}

/// The occurrence count once the context's asynchronous scan finishes, or
/// `None` if it does not finish within the budget.
#[must_use]
pub fn counted_matches(context: &sourceview5::SearchContext) -> Option<usize> {
    wait_until(REALIZE_BUDGET, || context.occurrences_count() >= 0)
        .then(|| usize::try_from(context.occurrences_count()).ok())
        .flatten()
}

/// Microseconds one `forward` takes to find a match at the end of a
/// `chars`-character line, with regex on or off.
#[must_use]
pub fn end_match_us(chars: usize, regex: bool) -> u128 {
    let buffer = sourceview5::Buffer::new(None);
    buffer.set_text(&format!(
        "{} {NEEDLE}",
        paragraph(ParagraphKind::Ascii, chars, 33)
    ));
    let context = search(&buffer, regex);
    let start = buffer.start_iter();
    time_us(|| context.forward(&start)).1
}

/// Probe A33.
#[must_use]
pub fn probe_a33() -> Observation {
    Recorder::run(AxiomId::new(33), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let buffer = mixed_buffer();
        let default_visible_only = sourceview5::SearchSettings::new().is_visible_only();
        recorder.measure("default_visible_only", default_visible_only);
        let mut found = Vec::new();
        for (label, regex, visible_only) in [
            ("plain_default", false, None),
            ("regex_default", true, None),
            ("plain_all_text", false, Some(false)),
            ("regex_all_text", true, Some(false)),
        ] {
            let context = search_with(&buffer, regex, visible_only);
            let forward = forward_matches(&context);
            let counted = counted_matches(&context);
            found.push((forward, counted));
            recorder.measure(label, format!("forward {forward}, counted {counted:?}"));
        }
        for chars in TIMED_LENGTHS {
            let small = chars == TIMED_LENGTHS[0];
            recorder.measure(
                if small {
                    "plain_end_match_20k_us"
                } else {
                    "plain_end_match_200k_us"
                },
                end_match_us(chars, false),
            );
            recorder.measure(
                if small {
                    "regex_end_match_20k_us"
                } else {
                    "regex_end_match_200k_us"
                },
                end_match_us(chars, true),
            );
        }
        recorder.control(
            found.iter().all(|(_, counted)| counted.is_some()),
            "control: every occurrence scan finished",
        )?;
        let all = VISIBLE_MATCHES + HIDDEN_MATCHES;
        let agrees = |(forward, counted): (usize, Option<usize>), expected: usize| {
            forward == expected && counted == Some(expected)
        };
        recorder.axiom(default_visible_only, "visible-only defaults to true")?;
        recorder.axiom(
            agrees(found[0], VISIBLE_MATCHES) && agrees(found[1], VISIBLE_MATCHES),
            "by default both plain-text and regex search skip hidden text",
        )?;
        recorder.axiom(
            agrees(found[2], all),
            "with visible-only false the plain-text search finds matches inside hidden text",
        )?;
        recorder.axiom(
            agrees(found[3], VISIBLE_MATCHES),
            "with visible-only false the regex search still finds only the visible matches",
        )
    })
}
