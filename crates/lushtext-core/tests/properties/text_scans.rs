// SPDX-License-Identifier: GPL-3.0-or-later

//! Generated equivalence of the accelerated text scans with their scalar
//! references (`simd-line-scans`).
//!
//! The load's file-health count, the save rewrites, and the minimap
//! accumulator's slice entry point each skip between the few bytes that matter
//! with `memchr`; these properties pin them to the per-character forms they
//! replaced (`services::editor_io::text_scans::reference`, and
//! `MinimapAnalysisAccumulator::inspect_char`). The alphabet is biased toward
//! what the fast paths branch on: line breaks, blanks, every hit and look-alike
//! lead byte (`C2 A0`/`C2 A9`, `E2 80 8B`/`E2 80 99`/`E2 80 94`,
//! `EF BB BF`/`EF BB BE`), U+2028/U+2029, and four-byte scalars, and every
//! split is cut on scalar boundaries chosen by the generator.

use lushtext_core::services::editor_io::text_scans::reference::ENDINGS;
use lushtext_core::services::editor_io::text_scans::{self, reference};
use lushtext_core::ui::editor_page::{MinimapAnalysisAccumulator, MinimapAnalysisPolicy};
use proptest::prelude::*;

use crate::support;

/// One character from the biased alphabet, or occasionally any scalar.
fn scan_char() -> impl Strategy<Value = char> {
    prop_oneof![
        8 => prop::sample::select(vec![
            '\n', '\r', ' ', '\t', 'a', 'z', 'é', '日', '😀', '\u{00A0}', '\u{00A9}', '\u{200B}',
            '\u{200C}', '\u{200D}', '\u{200A}', '\u{2060}', '\u{2061}', '\u{FEFF}', '\u{FEFE}',
            '\u{2019}', '\u{2014}', '\u{2028}', '\u{2029}', '\u{0085}',
        ]),
        1 => any::<char>(),
    ]
}

/// Text from [`scan_char`], with occasional long runs so minimap thresholds
/// and memchr's vector blocks are crossed inside one line.
fn scan_text() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        6 => scan_char().prop_map(String::from),
        1 => (1usize..200, scan_char()).prop_map(|(count, character)| {
            std::iter::repeat_n(character, count).collect()
        }),
    ];
    prop::collection::vec(piece, 0..96).prop_map(|pieces| pieces.concat())
}

/// Byte offsets within a few bytes of a 32-byte half or 64-byte block seam of
/// the AVX2 kernel (and of the next block's), where a lead's continuation
/// bytes cross into the next half or block.
const SEAM_OFFSETS: [usize; 24] = [
    0, 1, 2, 29, 30, 31, 32, 33, 34, 61, 62, 63, 64, 65, 66, 93, 94, 95, 96, 97, 125, 126, 127, 128,
];

/// A hit or look-alike scalar for the seam placements: every lead byte the
/// kernel tests, with matching and non-matching continuations.
fn seam_char() -> impl Strategy<Value = char> {
    prop::sample::select(vec![
        '\u{00A0}', '\u{00A9}', '\u{00C2}', '\u{200B}', '\u{200C}', '\u{200D}', '\u{200A}',
        '\u{200E}', '\u{2060}', '\u{2061}', '\u{2020}', '\u{FEFF}', '\u{FEFE}', '\u{FFBF}',
        '\u{2019}', '\u{2014}', '\u{2026}', 'é', '日', '😀',
    ])
}

/// Text of at least 66 bytes (one whole kernel step) with hits and look-alikes
/// starting at or just after the seam offsets, on an ASCII ground.
fn seam_text() -> impl Strategy<Value = String> {
    (
        prop::collection::vec(
            (prop::sample::select(SEAM_OFFSETS.to_vec()), seam_char()),
            1..8,
        ),
        66usize..200,
    )
        .prop_map(|(mut placements, minimum_len)| {
            placements.sort_by_key(|&(offset, _)| offset);
            let mut text = String::new();
            for (offset, character) in placements {
                while text.len() < offset {
                    text.push('x');
                }
                text.push(character);
            }
            while text.len() < minimum_len {
                text.push('y');
            }
            text
        })
}

/// One health scan under comparison; `None` when it cannot run on this host.
type HealthScan = dyn Fn(&str) -> Option<(usize, usize)>;

fn cuts() -> impl Strategy<Value = Vec<usize>> {
    prop::collection::vec(any::<usize>(), 0..6)
}

proptest! {
    #![proptest_config(support::property_config())]

    #[test]
    fn health_counts_equal_the_reference_over_every_slicing(
        text in scan_text(),
        cuts in cuts(),
    ) {
        let (mut nbsp, mut zero_width) = (0, 0);
        for piece in support::split_at_scalar_cuts(&text, &cuts) {
            let (piece_nbsp, piece_zero_width) = text_scans::count_health_characters(piece);
            nbsp += piece_nbsp;
            zero_width += piece_zero_width;
        }
        prop_assert_eq!((nbsp, zero_width), reference::count_health_characters(&text));
    }

    /// The dispatching entry, the `memchr3` fallback and (where it can run)
    /// the AVX2 kernel each equal the reference on seam-placed hits, also when
    /// the text is cut in two at every scalar boundary of its first 66 bytes:
    /// the head is then shorter than one kernel step (every tail length 0..=65,
    /// counted by the tail scan alone), and the rest's tail past its last
    /// whole 64-byte step runs through every residue while its block seams
    /// shift across the placed hits.
    #[test]
    fn health_kernel_and_fallback_equal_the_reference_at_block_seams(text in seam_text()) {
        let expected = reference::count_health_characters(&text);
        prop_assert_eq!(text_scans::count_health_characters(&text), expected);
        prop_assert_eq!(
            text_scans::count_health_characters_memchr(text.as_bytes(), 0),
            expected
        );
        if let Some(kernel) = text_scans::count_health_characters_kernel(&text) {
            prop_assert_eq!(kernel, expected);
        }
        for tail in 0..=65usize.min(text.len()) {
            let cut = text.floor_char_boundary(tail);
            let (head, rest) = text.split_at(cut);
            let scans: [&HealthScan; 3] = [
                &|piece| Some(text_scans::count_health_characters(piece)),
                &|piece| Some(text_scans::count_health_characters_memchr(piece.as_bytes(), 0)),
                &text_scans::count_health_characters_kernel,
            ];
            for scan in scans {
                if let (Some(head_counts), Some(rest_counts)) = (scan(head), scan(rest)) {
                    prop_assert_eq!(
                        (head_counts.0 + rest_counts.0, head_counts.1 + rest_counts.1),
                        expected,
                        "cut at {}", cut
                    );
                }
            }
        }
    }

    #[test]
    fn trim_equals_the_reference_and_borrows_when_unchanged(text in scan_text()) {
        let trimmed = text_scans::trim_trailing_space_and_tabs(&text);
        let expected = reference::trim_trailing_space_and_tabs(&text);
        prop_assert_eq!(trimmed.as_ref(), expected.as_str());
        prop_assert_eq!(
            matches!(trimmed, std::borrow::Cow::Borrowed(_)),
            expected == text
        );
    }

    #[test]
    fn normalization_equals_the_reference_in_every_mode(text in scan_text()) {
        for ending in ENDINGS {
            prop_assert_eq!(
                text_scans::line_endings_already_normalized(&text, ending),
                reference::line_endings_already_normalized(&text, ending)
            );
            let normalized = text_scans::normalize_line_endings(&text, ending);
            let expected = reference::normalize_line_endings(&text, ending);
            prop_assert_eq!(normalized.as_deref(), expected.as_deref());
        }
    }

    #[test]
    fn inspect_str_equals_inspect_char(
        text in scan_text(),
        cuts in cuts(),
        warning_line_chars in 0usize..=8,
        wrapped_line_chars in 0usize..=8,
        marker_limit in 0usize..=3,
        collect_markers in any::<bool>(),
    ) {
        let policy = MinimapAnalysisPolicy {
            warning_line_chars,
            wrapped_line_chars,
            marker_limit,
        };
        let mut per_character = MinimapAnalysisAccumulator::new(policy, collect_markers);
        for character in text.chars() {
            per_character.inspect_char(character);
        }
        let mut by_slices = MinimapAnalysisAccumulator::new(policy, collect_markers);
        for piece in support::split_at_scalar_cuts(&text, &cuts) {
            by_slices.inspect_str(piece);
        }
        prop_assert_eq!(&by_slices, &per_character);

        // Random cuts rarely land between a `\r` and its `\n`, so also cut
        // after every `\r`: each CRLF is then split across two calls.
        let mut after_every_cr = MinimapAnalysisAccumulator::new(policy, collect_markers);
        for piece in text.split_inclusive('\r') {
            after_every_cr.inspect_str(piece);
        }
        prop_assert_eq!(after_every_cr, per_character);
    }
}
