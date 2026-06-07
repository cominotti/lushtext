// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests for Markdown inline-footnote lowering.
//!
//! These exercise the GTK-free preprocessing hook used by the real preview
//! renderer, focusing on protected-region preservation and generated label
//! collision avoidance.

use lushtext_core::ui::markdown_preview::lower_inline_footnotes_for_property_test;
use proptest::prelude::*;

use crate::support;

/// Internal generated label prefix emitted by the preview lowering pass.
const INLINE_FOOTNOTE_LABEL_PREFIX: &str = "__lush_inline_footnote_";

proptest! {
    #![proptest_config(support::property_config())]

    #[test]
    fn lowering_preserves_protected_regions(
        link_text in support::text_fragment(),
        link_note in support::text_fragment(),
        code_note in support::text_fragment(),
        table_text in support::text_fragment(),
        table_note in support::text_fragment(),
        outside_note in support::text_fragment(),
    ) {
        let link = format!("[{link_text} ^[{link_note}]](https://example.com)");
        let code = format!("`code ^[{code_note}]`");
        let table_row = format!("| {table_text} ^[{table_note}] |");
        let markdown = format!(
            "{link}\n\n{code}\n\n| h |\n|---|\n{table_row}\n\nOutside^[{outside_note}]."
        );

        let lowered = lower_inline_footnotes_for_property_test(&markdown);

        prop_assert!(lowered.is_some(), "outside inline footnote should lower");
        let lowered = lowered.unwrap_or_default();
        prop_assert!(lowered.contains(&link));
        prop_assert!(lowered.contains(&code));
        prop_assert!(lowered.contains(&table_row));
        let original_outside_marker = format!("Outside^[{outside_note}]");
        let generated_definition_body = format!("]: {}", outside_note.trim());
        prop_assert!(!lowered.contains(&original_outside_marker));
        prop_assert!(lowered.contains(&generated_definition_body));
    }

    #[test]
    fn lowering_avoids_generated_label_collisions(
        collision_count in 0usize..=support::MAX_VECTOR_LEN,
        notes in prop::collection::vec(support::text_fragment(), 1..=support::MAX_VECTOR_LEN),
    ) {
        let mut markdown = String::new();
        for label_number in 1..=collision_count {
            let label = generated_label(label_number);
            markdown.push_str(&format!("Existing[^{label}]\n\n[^{label}]: Existing note\n\n"));
        }
        for (index, note) in notes.iter().enumerate() {
            markdown.push_str(&format!("Body{index}^[{note}]. "));
        }

        let lowered = lower_inline_footnotes_for_property_test(&markdown);

        prop_assert!(lowered.is_some(), "generated inline footnotes should lower");
        let lowered = lowered.unwrap_or_default();
        for (index, note) in notes.iter().enumerate() {
            let label = generated_label(collision_count + index + 1);
            let lowered_reference = format!("Body{index}[^{label}].");
            let lowered_definition = format!("[^{}]: {}", label, note.trim());
            prop_assert!(lowered.contains(&lowered_reference));
            prop_assert!(lowered.contains(&lowered_definition));
        }
        for label_number in 1..=collision_count {
            let label = generated_label(label_number);
            let existing_definition = format!("[^{label}]: Existing note");
            prop_assert!(lowered.contains(&existing_definition));
        }
    }
}

/// Build the internal label text expected after collision-aware lowering.
fn generated_label(label_number: usize) -> String {
    format!("{INLINE_FOOTNOTE_LABEL_PREFIX}{label_number}")
}
