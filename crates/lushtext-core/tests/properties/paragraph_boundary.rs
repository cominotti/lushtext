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

use crate::support;

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
    let mut accumulator = ParagraphProfileAccumulator::new(ProfileConfig::PRODUCTION);
    for piece in support::split_at_scalar_cuts(text, cuts) {
        accumulator.push_str(piece);
    }
    accumulator.finish()
}

/// The byte loop `next_paragraph_break` ran before its `memchr3` skip, kept
/// here as the oracle: `(content_end, next_start)` of the first break at or
/// after `from`.
fn byte_loop_break(text: &str, from: usize) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut index = from;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => return Some((index, index + 1)),
            b'\r' => {
                let crlf = bytes.get(index + 1) == Some(&b'\n');
                return Some((index, index + if crlf { 2 } else { 1 }));
            }
            0xE2 if bytes.get(index + 1) == Some(&0x80) && bytes.get(index + 2) == Some(&0xA9) => {
                return Some((index, index + 3));
            }
            _ => index += 1,
        }
    }
    None
}

/// Long delimiter-free runs (past every SIMD block width) between the
/// delimiters and the other `0xE2`-led scalars (U+2027, U+2028, `…`, `€`,
/// `’`, `—`).
fn simd_text() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        3 => paragraph_text(4),
        1 => Just("\u{2026}\u{20AC}\u{2028}".to_string()),
        2 => (1usize..200).prop_map(|n| "y".repeat(n)),
        1 => (1usize..40).prop_map(|n| "é".repeat(n)),
        // Typographic runs: enough `0xE2`-led scalars that are not U+2029 for
        // the search to switch to `memchr2` + `memmem`.
        1 => (1usize..60).prop_map(|n| "\u{2019}x\u{2014}".repeat(n)),
    ];
    prop::collection::vec(piece, 0..30).prop_map(|pieces| pieces.concat())
}

/// `slice_end_after_paragraph` as it was before its backward search: walk
/// every break forwards from `start`, kept here as the oracle.
fn forward_slice_end(text: &str, start: usize, budget: usize) -> usize {
    if start >= text.len() {
        return text.len();
    }
    let mut budget_end = start.saturating_add(budget).min(text.len());
    while budget_end > start && !text.is_char_boundary(budget_end) {
        budget_end -= 1;
    }
    if budget_end == text.len() {
        return text.len();
    }
    let mut last_within = None;
    let mut cursor = start;
    while let Some((_, next_start)) = byte_loop_break(text, cursor) {
        if next_start > budget_end {
            return last_within.unwrap_or(next_start);
        }
        last_within = Some(next_start);
        cursor = next_start;
    }
    last_within.unwrap_or(text.len())
}

/// The profile `push_char` builds, one character at a time (the oracle for
/// the paragraph-granular `push_str`).
fn per_character_profile(text: &str) -> DocumentProfile {
    let mut accumulator = ParagraphProfileAccumulator::new(ProfileConfig::PRODUCTION);
    for character in text.chars() {
        accumulator.push_char(character);
    }
    accumulator.finish()
}

/// ASCII, multibyte, and CRLF-mixed text with long paragraphs.
fn profile_text() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        4 => paragraph_text(5),
        1 => Just("\r\n".repeat(3)),
        1 => (1usize..800).prop_map(|n| "q".repeat(n)),
        1 => (1usize..300).prop_map(|n| "é𝄞\u{2028}".repeat(n)),
    ];
    prop::collection::vec(piece, 0..30).prop_map(|pieces| pieces.concat())
}

proptest! {
    #[test]
    fn push_str_equals_push_char_per_character(
        text in profile_text(),
        cuts in prop::collection::vec(any::<usize>(), 0..8),
    ) {
        // `profile_in_slices` feeds `push_str`; every cut lands on a scalar
        // boundary, including between the `\r` and `\n` of a CRLF.
        prop_assert_eq!(profile_in_slices(&text, &cuts), per_character_profile(&text));
    }

    #[test]
    fn backward_slice_end_equals_the_forward_walk(text in simd_text(), budgets in prop::collection::vec(0usize..600, 1..6)) {
        let bytes = text.as_bytes();
        let starts = (0..=text.len()).filter(|&start| {
            text.is_char_boundary(start)
                && !(start > 0 && start < bytes.len() && bytes[start - 1] == b'\r' && bytes[start] == b'\n')
        });
        for start in starts {
            for &budget in &budgets {
                prop_assert_eq!(
                    slice_end_after_paragraph(&text, start, budget),
                    forward_slice_end(&text, start, budget),
                    "start {} budget {}", start, budget
                );
            }
        }
    }

    #[test]
    fn fast_break_search_equals_the_byte_loop(text in simd_text()) {
        for from in (0..=text.len()).filter(|&from| text.is_char_boundary(from)) {
            let fast = next_paragraph_break(&text, from)
                .map(|found| (found.content_end, found.next_start));
            prop_assert_eq!(fast, byte_loop_break(&text, from), "from {}", from);
        }
    }

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
