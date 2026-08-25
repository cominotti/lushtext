// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests for GTK-free Markdown projection planning.
//!
//! These cover the three plan-wide invariants the GTK projector depends on:
//! every emitted batch fits one projection slice, consecutive batches' carry
//! signatures chain into each other, and a plan that no global budget stopped
//! accounts for every parsed event as either projected or omitted.

use lushtext_core::services::markdown_render::{
    MARKDOWN_BYTES_PER_PROJECTION_SLICE, MARKDOWN_EVENTS_PER_PROJECTION_SLICE,
    MAX_MARKDOWN_STRUCTURE_DEPTH, MarkdownCarrySignature, MarkdownEventBatch, MarkdownRenderPlan,
    markdown_render_options, plan_markdown,
};
use proptest::prelude::*;
use pulldown_cmark::Parser;

use crate::support;

/// Maximum blocks in one generated document.
///
/// Six blocks is enough to place an oversized block first, last, and between
/// ordinary siblings while keeping each generated case small.
const MAX_GENERATED_BLOCKS: usize = 6;
/// Maximum rows, items, or entries in one generated container.
///
/// Sixty units is comfortably past the 256-event projection slice for tables and
/// lists, so sub-slicing is reachable without generating large documents.
const MAX_GENERATED_UNITS: usize = 60;
/// Maximum inline spans in one generated dense paragraph.
///
/// Each span is three events, so 120 spans reliably crosses the slice event
/// budget inside a single indivisible paragraph.
const MAX_GENERATED_INLINE_SPANS: usize = 120;
/// Maximum columns in one generated table.
const MAX_GENERATED_COLUMNS: usize = 6;

proptest! {
    #![proptest_config(support::property_config())]

    #[test]
    fn every_emitted_batch_fits_one_projection_slice(markdown in generated_document()) {
        let plan = plan_markdown(&markdown);
        for batch in &plan.batches {
            prop_assert!(
                batch.len() <= MARKDOWN_EVENTS_PER_PROJECTION_SLICE,
                "batch of {} events exceeds the slice event budget",
                batch.len()
            );
            prop_assert!(
                batch.retained_bytes() <= MARKDOWN_BYTES_PER_PROJECTION_SLICE,
                "batch of {} bytes exceeds the slice byte budget",
                batch.retained_bytes()
            );
        }
    }

    #[test]
    fn consecutive_batch_carry_signatures_chain(markdown in generated_document()) {
        let plan = plan_markdown(&markdown);
        let mut expected = MarkdownCarrySignature::default();
        for batch in &plan.batches {
            prop_assert_eq!(batch.expected_carry(), &expected);
            prop_assert!(batch.expected_carry().len() <= MAX_MARKDOWN_STRUCTURE_DEPTH);
            prop_assert!(batch.open_carry().len() <= MAX_MARKDOWN_STRUCTURE_DEPTH);
            expected = batch.open_carry().clone();
        }
        prop_assert!(
            expected.is_empty(),
            "the last batch must leave nothing open"
        );
    }

    #[test]
    fn a_complete_plan_projects_or_omits_every_parsed_event(markdown in generated_document()) {
        let plan = plan_markdown(&markdown);
        // Scoped to plans no global budget stopped. A global stop is allowed to
        // leave the document unfinished, and can legally emit no batch at all
        // when it fires with a top-level block still open.
        prop_assume!(plan.limit.is_none());

        let parsed = Parser::new_ext(&markdown, markdown_render_options()).count();
        prop_assert_eq!(
            plan.metrics.events,
            parsed,
            "a complete plan must consume the event stream to EOF"
        );
        let projected: usize = plan.batches.iter().map(MarkdownEventBatch::len).sum();
        prop_assert!(projected <= plan.metrics.events);
        if projected < plan.metrics.events {
            prop_assert!(
                plan.omissions() > 0,
                "unprojected events must be accounted for by an omission"
            );
        }
        prop_assert_eq!(marker_count(&plan), plan.omissions());
    }
}

/// Count the omission markers the plan's batches actually carry.
fn marker_count(plan: &MarkdownRenderPlan) -> usize {
    plan.batches
        .iter()
        .map(|batch| batch.omissions().len())
        .sum()
}

/// Generate a small document out of ordinary and oversized block shapes.
fn generated_document() -> impl Strategy<Value = String> {
    prop::collection::vec(generated_block(), 1..=MAX_GENERATED_BLOCKS)
        .prop_map(|blocks| blocks.join("\n"))
}

/// Generate one Markdown block, sometimes larger than a projection slice.
fn generated_block() -> impl Strategy<Value = String> {
    prop_oneof![
        support::text_fragment().prop_map(|text| format!("{text}\n\n")),
        support::text_fragment().prop_map(|text| format!("## {text}\n\n")),
        (1usize..=MAX_GENERATED_INLINE_SPANS)
            .prop_map(|spans| format!("{}\n\n", "**x** ".repeat(spans))),
        (2usize..=MAX_GENERATED_COLUMNS, 1usize..=MAX_GENERATED_UNITS)
            .prop_map(|(columns, rows)| generated_table(columns, rows)),
        (1usize..=MAX_GENERATED_UNITS, any::<bool>())
            .prop_map(|(items, ordered)| generated_list(items, ordered)),
        (1usize..=MAX_GENERATED_UNITS).prop_map(generated_blockquote),
        (1usize..=MAX_GENERATED_UNITS).prop_map(generated_definition_list),
        (1usize..=MAX_GENERATED_UNITS).prop_map(generated_indented_code),
        (1usize..=MAX_GENERATED_UNITS).prop_map(generated_fenced_code),
    ]
}

fn generated_table(columns: usize, rows: usize) -> String {
    let mut markdown = String::new();
    for index in 0..columns {
        markdown.push_str(&format!("| h{index} "));
    }
    markdown.push_str("|\n");
    for _ in 0..columns {
        markdown.push_str("| --- ");
    }
    markdown.push_str("|\n");
    for row in 0..rows {
        for column in 0..columns {
            markdown.push_str(&format!("| r{row}c{column} "));
        }
        markdown.push_str("|\n");
    }
    markdown.push('\n');
    markdown
}

fn generated_list(items: usize, ordered: bool) -> String {
    let mut markdown = String::new();
    for index in 0..items {
        if ordered {
            markdown.push_str(&format!("{}. item-{index}\n", index + 1));
        } else {
            markdown.push_str(&format!("- item-{index}\n"));
        }
    }
    markdown.push('\n');
    markdown
}

fn generated_blockquote(paragraphs: usize) -> String {
    let mut markdown = String::new();
    for index in 0..paragraphs {
        markdown.push_str(&format!("> quoted-{index}\n>\n"));
    }
    markdown.push('\n');
    markdown
}

fn generated_definition_list(entries: usize) -> String {
    let mut markdown = String::new();
    for index in 0..entries {
        markdown.push_str(&format!("term-{index}\n: definition-{index}\n\n"));
    }
    markdown
}

fn generated_indented_code(lines: usize) -> String {
    let mut markdown = String::new();
    for index in 0..lines {
        markdown.push_str(&format!("    indented-{index}\n"));
    }
    markdown.push('\n');
    markdown
}

fn generated_fenced_code(lines: usize) -> String {
    let mut markdown = String::from("```sh\n");
    for index in 0..lines {
        markdown.push_str(&format!("echo fenced-{index}\n"));
    }
    markdown.push_str("```\n\n");
    markdown
}
