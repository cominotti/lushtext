// SPDX-License-Identifier: GPL-3.0-or-later

//! Property equivalence for exact save-encoding representability analysis.

use encoding_rs::EncoderResult;
use lushtext_core::model::encoding::DocumentEncoding;
use lushtext_core::services::editor_io::{LossyEncodingIssue, analyze_lossy_encoding};
use proptest::prelude::*;

use crate::support;

proptest! {
    #![proptest_config(support::property_config())]

    #[test]
    fn optimized_analysis_matches_no_replacement_encoding_and_reference_positions(
        text in prop::collection::vec(any::<char>(), 0..256)
            .prop_map(|characters| characters.into_iter().collect::<String>()),
        encoding in prop::sample::select(&DocumentEncoding::COMMON),
    ) {
        let analyzed = analyze_lossy_encoding(&text, encoding);
        let encodes_exactly = encodes_without_replacement(&text, encoding);
        prop_assert_eq!(analyzed.is_none(), encodes_exactly);

        let expected = reference_issues(&text, encoding);
        match analyzed {
            None => prop_assert!(expected.is_empty()),
            Some(preview) => {
                prop_assert_eq!(preview.total_issue_count, expected.len());
                prop_assert_eq!(preview.issues, expected.into_iter().take(8).collect::<Vec<_>>());
            }
        }
    }
}

fn encodes_without_replacement(text: &str, encoding: DocumentEncoding) -> bool {
    let mut encoder = encoding.codec().new_encoder();
    let mut scratch = [0u8; 128];
    let mut consumed = 0usize;
    loop {
        let (result, read, _) =
            encoder.encode_from_utf8_without_replacement(&text[consumed..], &mut scratch, true);
        consumed = consumed.saturating_add(read);
        match result {
            EncoderResult::InputEmpty => return consumed == text.len(),
            EncoderResult::OutputFull => {
                assert!(read > 0, "reference scratch must fit one scalar");
            }
            EncoderResult::Unmappable(_) => return false,
        }
    }
}

fn reference_issues(text: &str, encoding: DocumentEncoding) -> Vec<LossyEncodingIssue> {
    let mut line = 1usize;
    let mut column = 1usize;
    let mut issues = Vec::new();
    for character in text.chars() {
        let (_, _, had_errors) = encoding.codec().encode(&character.to_string());
        if had_errors {
            issues.push(LossyEncodingIssue {
                line,
                column,
                character,
            });
        }
        if character == '\n' {
            line = line.saturating_add(1);
            column = 1;
        } else {
            column = column.saturating_add(1);
        }
    }
    issues
}
