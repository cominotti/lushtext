// SPDX-License-Identifier: GPL-3.0-or-later

//! Generated invariants for the GTK paragraph-boundary function and the
//! paragraph profile, on strings far longer than the Kani harnesses' 8-byte
//! domain.
//!
//! The first property is the evidence that `model::paragraph_boundary` agrees
//! with Pango itself: `GtkTextBuffer` splits lines with
//! `pango_find_paragraph_boundary`, so every split here is compared with it.
//! It lives in this property target, not in `model/`, so `model/` keeps no
//! toolkit dependency even under `cfg(test)`.

use lushtext_core::model::paragraph_boundary::{
    DocumentProfile, ParagraphProfileAccumulator, ProfileConfig, next_paragraph_break,
    slice_end_after_paragraph,
};
use proptest::prelude::*;

/// A delimiter-heavy, multibyte alphabet without NUL (Pango stops at NUL).
fn paragraph_text(max_pieces: usize) -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        Just("\r".to_string()),
        Just("\n".to_string()),
        Just("\r\n".to_string()),
        Just("\u{2028}".to_string()),
        Just("\u{2029}".to_string()),
        Just("\u{85}".to_string()),
        Just("é".to_string()),
        Just("𝄞".to_string()),
        Just("\u{2027}".to_string()),
        "[a-z]{0,12}",
    ];
    prop::collection::vec(piece, 0..max_pieces).prop_map(|pieces| pieces.concat())
}

/// Long paragraphs too, so the production thresholds are crossed.
fn long_paragraph_text() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        3 => paragraph_text(6),
        1 => (1usize..3_000).prop_map(|n| "x".repeat(n)),
    ];
    prop::collection::vec(piece, 0..40).prop_map(|pieces| pieces.concat())
}

fn byte_offset(value: i32) -> usize {
    usize::try_from(value).expect("Pango returns non-negative byte offsets")
}

fn profile_in_slices(text: &str, cuts: &[usize]) -> DocumentProfile {
    let mut boundaries: Vec<usize> = cuts
        .iter()
        .map(|&cut| {
            let mut cut = cut % (text.len() + 1);
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            cut
        })
        .collect();
    boundaries.sort_unstable();
    let mut accumulator = ParagraphProfileAccumulator::new(ProfileConfig::PRODUCTION);
    let mut start = 0;
    for cut in boundaries {
        accumulator.push_str(&text[start..cut]);
        start = cut;
    }
    accumulator.push_str(&text[start..]);
    accumulator.finish()
}

proptest! {
    #[test]
    fn paragraph_breaks_agree_with_pango(text in paragraph_text(200)) {
        let mut start = 0usize;
        loop {
            let rest = &text[start..];
            let (delimiter, next) = pango::find_paragraph_boundary(rest);
            let (delimiter, next) = (byte_offset(delimiter), byte_offset(next));
            let Some(found) = next_paragraph_break(&text, start) else {
                // Pango reports no delimiter as one at the text end.
                prop_assert_eq!(delimiter, rest.len());
                prop_assert_eq!(next, rest.len());
                break;
            };
            prop_assert_eq!(found.content_end, start + delimiter);
            prop_assert_eq!(found.next_start, start + next);
            start = found.next_start;
        }
    }

    #[test]
    fn slices_end_at_pango_paragraph_ends(
        text in paragraph_text(400),
        budget in 1usize..64,
    ) {
        let mut start = 0usize;
        let mut rebuilt = String::new();
        while start < text.len() {
            let end = slice_end_after_paragraph(&text, start, budget);
            prop_assert!(end > start && text.is_char_boundary(end));
            if end < text.len() {
                // `end` is the start of a Pango paragraph that began at or
                // after `start`: walking Pango from `start` reaches it.
                let mut cursor = start;
                while cursor < end {
                    let (_, next) = pango::find_paragraph_boundary(&text[cursor..]);
                    cursor += byte_offset(next);
                }
                prop_assert_eq!(cursor, end);
            }
            rebuilt.push_str(&text[start..end]);
            start = end;
        }
        prop_assert_eq!(rebuilt, text);
    }

    #[test]
    fn profile_is_independent_of_slicing_in_production(
        text in long_paragraph_text(),
        cuts in prop::collection::vec(any::<usize>(), 0..12),
    ) {
        let whole = DocumentProfile::of_text(&text);
        prop_assert_eq!(profile_in_slices(&text, &cuts), whole);
    }

    #[test]
    fn profile_longest_paragraph_matches_pango(text in long_paragraph_text()) {
        let mut longest = 0u64;
        let mut paragraphs = 0u64;
        let mut start = 0usize;
        loop {
            let rest = &text[start..];
            let (delimiter, next) = pango::find_paragraph_boundary(rest);
            let (delimiter, next) = (byte_offset(delimiter), byte_offset(next));
            longest = longest.max(rest[..delimiter].chars().count() as u64);
            paragraphs += 1;
            if next == delimiter {
                break;
            }
            start += next;
        }
        let profile = DocumentProfile::of_text(&text);
        prop_assert_eq!(profile.longest_paragraph_chars, longest);
        prop_assert_eq!(profile.paragraph_count, paragraphs);
        prop_assert_eq!(profile.total_bytes, text.len() as u64);
    }
}
