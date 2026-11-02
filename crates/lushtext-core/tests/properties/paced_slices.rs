// SPDX-License-Identifier: GPL-3.0-or-later

//! Generated invariants for the two-budget install and replacement slicer
//! (`bound-post-open-settle`, design D6): coverage, progress, paragraph
//! alignment, and both budgets, on documents far longer than the Kani
//! harnesses' six abstract paragraphs.

use lushtext_core::model::buffer_replacement::{SliceBudget, next_slice_boundary};
use lushtext_core::model::paragraph_boundary::{next_paragraph_break, slice_end_after_paragraph};
use proptest::prelude::*;

/// Delimiter-heavy multibyte text with long lines too.
fn document() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        Just("\n".to_string()),
        Just("\r".to_string()),
        Just("\r\n".to_string()),
        Just("\u{2029}".to_string()),
        Just("é".to_string()),
        Just("𝄞".to_string()),
        "[a-z]{0,40}",
        (1usize..400).prop_map(|n| "x".repeat(n)),
    ];
    prop::collection::vec(piece, 0..120).prop_map(|pieces| pieces.concat())
}

/// The visible characters (delimiters excluded) of `text[start..end]`.
fn visible_chars(text: &str, start: usize, end: usize) -> u64 {
    let mut chars = 0u64;
    let mut at = start;
    while at < end {
        let (content_end, next) = next_paragraph_break(text, at)
            .map_or((text.len(), text.len()), |found| {
                (found.content_end, found.next_start)
            });
        chars += u64::try_from(text[at..content_end.min(end)].chars().count()).expect("fits");
        at = next;
    }
    chars
}

/// How many paragraphs `text[start..end]` holds.
fn paragraphs(text: &str, start: usize, end: usize) -> usize {
    let mut count = 0;
    let mut at = start;
    while at < end {
        at = next_paragraph_break(text, at).map_or(text.len(), |found| found.next_start);
        count += 1;
    }
    count
}

proptest! {
    #[test]
    fn paced_slices_cover_align_and_hold_both_budgets(
        text in document(),
        bytes in 1u64..600,
        chars in 1u64..300,
    ) {
        let budget = SliceBudget { bytes, chars };
        let mut start = 0;
        let mut rebuilt = String::new();
        while start < text.len() {
            let end = next_slice_boundary(&text, start, budget);
            prop_assert!(end > start, "progress at {}", start);
            prop_assert!(text.is_char_boundary(end));
            // Aligned: the slice ends where a paragraph does.
            let mut at = start;
            while at < end {
                at = next_paragraph_break(&text, at).map_or(text.len(), |found| found.next_start);
            }
            prop_assert_eq!(at, end, "slice ends inside a paragraph");
            let alone = paragraphs(&text, start, end) == 1;
            prop_assert!(
                alone
                    || (u64::try_from(end - start).expect("fits") <= bytes
                        && visible_chars(&text, start, end) <= chars),
                "slice {}..{} over budget", start, end
            );
            rebuilt.push_str(&text[start..end]);
            start = end;
        }
        prop_assert_eq!(rebuilt, text);
    }

    #[test]
    fn the_byte_budget_alone_cuts_like_the_existing_slicer(
        text in document(),
        bytes in 1usize..600,
    ) {
        let budget = SliceBudget {
            bytes: u64::try_from(bytes).expect("fits"),
            chars: u64::MAX - 1,
        };
        let mut start = 0;
        while start < text.len() {
            let fast = slice_end_after_paragraph(&text, start, bytes);
            prop_assert_eq!(next_slice_boundary(&text, start, budget), fast);
            start = fast;
        }
    }
}
