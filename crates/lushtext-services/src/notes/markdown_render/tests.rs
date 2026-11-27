// SPDX-License-Identifier: GPL-3.0-or-later

//! Unit tests for GTK-free Markdown preview planning.

use super::*;
use std::fmt::Write as _;

/// Concatenate every text-bearing event a plan would hand to the projector.
fn projected_text(plan: &MarkdownRenderPlan) -> String {
    let mut text = String::new();
    for batch in &plan.batches {
        for event in batch.events() {
            if let MarkdownEvent::Text(value) | MarkdownEvent::Code(value) = event {
                text.push_str(value);
                text.push('\n');
            }
        }
    }
    text
}

fn helm_readme_fixture() -> String {
    let mut markdown = String::from("# Example Chart\n\nIntro prose paragraph.\n\n## Values\n\n");
    markdown.push_str("| key | type | default | description | notes |\n");
    markdown.push_str("| --- | --- | --- | --- | --- |\n");
    for index in 0..40 {
        writeln!(
            markdown,
            "| key-{index} | string | `\"\"` | description {index} | note {index} |"
        )
        .expect("write values row");
    }
    markdown
        .push_str("\n## Upgrading\n\nTAIL-UPGRADING-PROSE\n\n## License\n\nTAIL-LICENSE-PROSE\n");
    markdown
}

/// A fenced block whose *bytes* cross the slice budget.
///
/// `pulldown_cmark` 0.13 coalesces a fenced block's lines into one `Text`
/// event, so a code block never crosses the per-slice *event* budget and
/// has no interior checkpoint. The byte budget is its only real trigger.
fn oversized_code_block_fixture() -> String {
    let mut markdown = String::from("# Script\n\n```sh\n");
    while markdown.len() <= MARKDOWN_BYTES_PER_PROJECTION_SLICE {
        markdown.push_str("echo an-ordinary-shell-line-of-moderate-length\n");
    }
    markdown.push_str("```\n\nTAIL-AFTER-CODE\n");
    markdown
}

fn oversized_list_fixture() -> String {
    let mut markdown = String::from("# List\n\n");
    for index in 0..120 {
        writeln!(markdown, "- item-{index}").expect("write list item");
    }
    markdown.push_str("\nTAIL-AFTER-LIST\n");
    markdown
}

fn oversized_blockquote_fixture() -> String {
    let mut markdown = String::from("# Quote\n\n");
    for index in 0..90 {
        writeln!(markdown, "> quoted-{index}\n>").expect("write quoted paragraph");
    }
    markdown.push_str("\nTAIL-AFTER-QUOTE\n");
    markdown
}

fn list_with_one_overflowing_item_fixture() -> String {
    let mut markdown = String::from("# Mixed list\n\n");
    for index in 0..60 {
        // A loose list puts each item's content inside a `Paragraph`, which
        // is cut-forbidding, so item 17 is one indivisible segment.
        if index == 17 {
            let dense = (0..200).map(|_| "**x** ").collect::<String>();
            writeln!(markdown, "- {dense}\n").expect("write dense item");
        } else {
            writeln!(markdown, "- item-{index}\n").expect("write list item");
        }
    }
    markdown.push_str("\nTAIL-AFTER-MIXED-LIST\n");
    markdown
}

/// Entries in [`oversized_definition_list_fixture`].
const DEFINITION_ENTRIES: usize = 60;

fn oversized_definition_list_fixture() -> String {
    let mut markdown = String::from("# Definitions\n\n");
    for index in 0..DEFINITION_ENTRIES {
        writeln!(markdown, "term-{index}\n: definition-{index}\n").expect("write definition");
    }
    markdown.push_str("\nTAIL-AFTER-DEFINITIONS\n");
    markdown
}

/// Lines in [`indented_code_block_fixture`].
const INDENTED_CODE_LINES: usize = 400;

/// An **indented** code block over the per-slice event budget.
///
/// Unlike a fenced block, an indented block emits one `Text` event per
/// line, so its retained-text checkpoints are reachable and the block is
/// sub-sliced between lines. Its total stays well under
/// [`MAX_MARKDOWN_CARRIED_EMBED_BYTES`], so nothing is dropped.
fn indented_code_block_fixture() -> String {
    let mut markdown = String::from("# Indented\n\n");
    for index in 0..INDENTED_CODE_LINES {
        writeln!(markdown, "    indented-line-{index}").expect("write indented line");
    }
    markdown.push_str("\nTAIL-AFTER-INDENTED-CODE\n");
    markdown
}

fn nested_table_in_list_fixture() -> String {
    let mut markdown = String::from("# Nested\n\n- outer item\n\n");
    markdown.push_str("  | a | b | c | d | e |\n  | --- | --- | --- | --- | --- |\n");
    for index in 0..40 {
        writeln!(
            markdown,
            "  | nested-{index} | b{index} | c{index} | d{index} | e{index} |"
        )
        .expect("write nested row");
    }
    markdown.push_str("\nTAIL-AFTER-NESTED\n");
    markdown
}

/// Header plus body cells produced by [`huge_byte_table_fixture`].
const HUGE_BYTE_TABLE_CELLS: usize = 3 + 3 * 200;

/// A table well inside [`MAX_MARKDOWN_CARRIED_TABLE_CELLS`] whose total
/// bytes are far above [`MAX_MARKDOWN_CARRIED_EMBED_BYTES`]. It must retain
/// every cell: the byte ceiling is scoped to code blocks precisely so a
/// table like this keeps rendering exactly as it does today.
fn huge_byte_table_fixture() -> String {
    let cell = "z".repeat(200);
    let mut markdown = String::from("# Wide bytes\n\n| a | b | c |\n| --- | --- | --- |\n");
    for _ in 0..200 {
        writeln!(markdown, "| {cell} | {cell} | {cell} |").expect("write wide row");
    }
    markdown.push_str("\nTAIL-AFTER-WIDE\n");
    markdown
}

/// Build a table with `columns` header cells and `rows` body rows.
fn cell_table(columns: usize, rows: usize) -> String {
    let mut markdown = String::new();
    for _ in 0..columns {
        markdown.push_str("| h ");
    }
    markdown.push_str("|\n");
    for _ in 0..columns {
        markdown.push_str("| --- ");
    }
    markdown.push_str("|\n");
    for _ in 0..rows {
        for _ in 0..columns {
            markdown.push_str("| c ");
        }
        markdown.push_str("|\n");
    }
    markdown
}

/// Cells produced by a `columns` x `rows` [`cell_table`], header included.
fn cell_table_cells(columns: usize, rows: usize) -> usize {
    columns * (rows + 1)
}

/// Columns and body rows of the table that crosses the cell ceiling.
const PAST_CEILING_COLUMNS: usize = 4;
const PAST_CEILING_ROWS: usize = 260;

/// A table past [`MAX_MARKDOWN_CARRIED_TABLE_CELLS`], where the projector
/// already replaces the whole table with one fallback widget, so the
/// crossing cannot change any rendered outcome.
fn table_past_the_cell_ceiling_fixture() -> String {
    let mut markdown = String::from("# Past ceiling\n\n");
    markdown.push_str(&cell_table(PAST_CEILING_COLUMNS, PAST_CEILING_ROWS));
    markdown.push_str("\nTAIL-AFTER-PAST-CEILING\n");
    markdown
}

/// Cells the past-ceiling fixture's table contains.
fn past_ceiling_cells() -> usize {
    cell_table_cells(PAST_CEILING_COLUMNS, PAST_CEILING_ROWS)
}

/// Count retained `Start(TableCell)` events across a whole plan.
fn retained_cells(plan: &MarkdownRenderPlan) -> usize {
    plan.batches
        .iter()
        .flat_map(MarkdownEventBatch::events)
        .filter(|event| matches!(event, MarkdownEvent::Start(MarkdownTag::TableCell)))
        .count()
}

/// Sum every batch's retained bytes.
fn retained_bytes(plan: &MarkdownRenderPlan) -> usize {
    plan.batches
        .iter()
        .map(MarkdownEventBatch::retained_bytes)
        .sum()
}

/// Restore the test-only cancellation seam even if an assertion panics.
struct CancelAfterEvents;

impl CancelAfterEvents {
    fn arm(events: usize) -> Self {
        CANCEL_AFTER_EVENTS.set(Some(events));
        Self
    }
}

impl Drop for CancelAfterEvents {
    fn drop(&mut self) {
        CANCEL_AFTER_EVENTS.set(None);
    }
}

/// The single omission a plan is expected to carry.
fn single_omission(plan: &MarkdownRenderPlan) -> MarkdownBlockOmission {
    let markers: Vec<MarkdownOmissionMarker> = plan
        .batches
        .iter()
        .flat_map(|batch| batch.omissions().iter().copied())
        .collect();
    assert_eq!(markers.len(), 1, "expected exactly one omission marker");
    markers[0].omission
}

fn byte_budget_fixture() -> String {
    let mut markdown = String::from("# Bytes\n\n");
    markdown.push_str(&"x".repeat(MARKDOWN_BYTES_PER_PROJECTION_SLICE + 1));
    markdown.push_str("\n\nTAIL-AFTER-BYTES\n");
    markdown
}

/// Assert one batch's retained events are structurally balanced.
///
/// A batch may close containers it inherited and leave containers open, but
/// its net depth change must agree with its carry signatures and every
/// locally opened tag must be closed by the matching end. This is what
/// catches a crossing that retained `Start(TableRow)` without its end.
fn assert_events_balanced(batch: &MarkdownEventBatch) {
    let mut opened: Vec<MarkdownTagEnd> = Vec::new();
    let mut closed_inherited = 0usize;
    for event in batch.events() {
        match event {
            MarkdownEvent::Start(tag) => opened.push(tag.to_end()),
            MarkdownEvent::End(end) => match opened.pop() {
                Some(expected) => assert_eq!(
                    expected, *end,
                    "a batch closed a tag it did not open: {end:?}"
                ),
                None => closed_inherited += 1,
            },
            _ => {}
        }
    }
    assert!(
        closed_inherited <= batch.expected_carry().len(),
        "a batch closed more inherited containers than it inherited"
    );
    assert_eq!(
        batch.expected_carry().len() - closed_inherited + opened.len(),
        batch.open_carry().len(),
        "a batch's net structural depth must match its carry signatures"
    );
}

/// Assert the shared plan invariants every emitted plan must satisfy.
fn assert_plan_invariants(plan: &MarkdownRenderPlan) {
    let mut expected = MarkdownCarrySignature::default();
    for batch in &plan.batches {
        assert!(
            batch.len() <= MARKDOWN_EVENTS_PER_PROJECTION_SLICE,
            "batch of {} events exceeds the slice event budget",
            batch.len()
        );
        assert!(
            batch.retained_bytes() <= MARKDOWN_BYTES_PER_PROJECTION_SLICE,
            "batch of {} bytes exceeds the slice byte budget",
            batch.retained_bytes()
        );
        assert!(batch.expected_carry().len() <= MAX_MARKDOWN_STRUCTURE_DEPTH);
        assert!(batch.open_carry().len() <= MAX_MARKDOWN_STRUCTURE_DEPTH);
        assert_eq!(*batch.expected_carry(), expected, "batch carry must chain");
        assert!(
            batch
                .omissions()
                .iter()
                .all(|marker| marker.at_event <= batch.len())
        );
        assert_events_balanced(batch);
        expected = batch.open_carry().clone();
    }
    assert!(expected.is_empty(), "the last batch must close everything");
    let markers: usize = plan
        .batches
        .iter()
        .map(|batch| batch.omissions().len())
        .sum();
    assert_eq!(markers, plan.omissions());
}

#[test]
fn helm_style_values_table_is_projected_with_its_document_tail() {
    let plan = plan_markdown(&helm_readme_fixture());
    assert!(plan.is_complete());
    assert_eq!(plan.omissions(), 0);
    assert_plan_invariants(&plan);
    let text = projected_text(&plan);
    assert!(text.contains("Intro prose paragraph."));
    assert!(text.contains("key-39"));
    assert!(text.contains("TAIL-UPGRADING-PROSE"));
    assert!(text.contains("TAIL-LICENSE-PROSE"));
    assert!(plan.batches.len() > 1, "an oversized table is sub-sliced");
}

#[test]
fn oversized_code_block_keeps_the_document_tail() {
    let plan = plan_markdown(&oversized_code_block_fixture());
    assert!(plan.is_complete());
    assert_plan_invariants(&plan);
    assert!(projected_text(&plan).contains("TAIL-AFTER-CODE"));
}

#[test]
fn oversized_list_is_projected_completely_with_its_tail() {
    let plan = plan_markdown(&oversized_list_fixture());
    assert!(plan.is_complete());
    assert_eq!(plan.omissions(), 0);
    assert_plan_invariants(&plan);
    let text = projected_text(&plan);
    for index in 0..120 {
        assert!(
            text.contains(&format!("item-{index}\n")),
            "item-{index} missing"
        );
    }
    assert!(text.contains("TAIL-AFTER-LIST"));
    assert!(plan.batches.len() > 1, "an oversized list is sub-sliced");
}

#[test]
fn oversized_blockquote_is_projected_completely_with_its_tail() {
    let plan = plan_markdown(&oversized_blockquote_fixture());
    assert!(plan.is_complete());
    assert_eq!(plan.omissions(), 0);
    assert_plan_invariants(&plan);
    let text = projected_text(&plan);
    assert!(text.contains("quoted-0\n"));
    assert!(text.contains("quoted-89\n"));
    assert!(text.contains("TAIL-AFTER-QUOTE"));
}

#[test]
fn projection_byte_budget_omits_only_the_oversized_block() {
    let plan = plan_markdown(&byte_budget_fixture());
    assert!(plan.is_complete());
    assert_eq!(plan.omissions(), 1);
    assert_plan_invariants(&plan);
    let omission = single_omission(&plan);
    assert_eq!(omission.reason, MarkdownOmissionReason::SliceBytes);
    assert_eq!(omission.scope, MarkdownOmissionScope::TopLevelBlock);
    assert!(projected_text(&plan).contains("TAIL-AFTER-BYTES"));
}

#[test]
fn one_overflowing_list_item_keeps_its_siblings_and_the_tail() {
    let plan = plan_markdown(&list_with_one_overflowing_item_fixture());
    assert!(plan.is_complete());
    assert_eq!(plan.omissions(), 1);
    assert_plan_invariants(&plan);
    let omission = single_omission(&plan);
    assert_eq!(omission.reason, MarkdownOmissionReason::SliceEvents);
    assert_eq!(
        omission.scope,
        MarkdownOmissionScope::ContainerSegment,
        "one overflowing item must not be scoped to the whole list"
    );
    let text = projected_text(&plan);
    for index in 0..60 {
        if index == 17 {
            continue;
        }
        assert!(
            text.contains(&format!("item-{index}\n")),
            "item-{index} missing"
        );
    }
    assert!(text.contains("TAIL-AFTER-MIXED-LIST"));
}

#[test]
fn one_overflowing_definition_body_keeps_its_siblings_and_the_tail() {
    // The definition-list analogue of the overflowing list item: one body's
    // inline run is indivisible, so only that body is replaced while every
    // other title and definition still renders.
    // A tight definition body holds its inline events directly, so it is
    // cuttable between bare leaves. Wrapping the whole run in one `Strong`
    // span is what makes it genuinely indivisible.
    // The closing `**` must not be preceded by whitespace or CommonMark
    // refuses the delimiter run and the span never opens.
    let dense = format!("**{}z**", (0..140).map(|_| "a `c` ").collect::<String>());
    let mut markdown = String::from("# Definitions\n\n");
    for index in 0..DEFINITION_ENTRIES {
        if index == 17 {
            writeln!(markdown, "term-{index}\n: {dense}\n").expect("write dense definition");
        } else {
            writeln!(markdown, "term-{index}\n: definition-{index}\n").expect("write definition");
        }
    }
    markdown.push_str("\nTAIL-AFTER-DEFINITIONS\n");

    let plan = plan_markdown(&markdown);
    assert!(plan.is_complete());
    assert_eq!(plan.omissions(), 1);
    assert_plan_invariants(&plan);
    let omission = single_omission(&plan);
    assert_eq!(omission.reason, MarkdownOmissionReason::SliceEvents);
    assert_eq!(
        omission.scope,
        MarkdownOmissionScope::ContainerSegment,
        "one overflowing body must not be scoped to the whole list"
    );
    let text = projected_text(&plan);
    for index in 0..DEFINITION_ENTRIES {
        assert!(
            text.contains(&format!("term-{index}")),
            "term-{index} missing"
        );
        if index != 17 {
            assert!(
                text.contains(&format!("definition-{index}")),
                "definition-{index} missing"
            );
        }
    }
    assert!(text.contains("TAIL-AFTER-DEFINITIONS"));
}

#[test]
fn a_flat_oversized_list_carries_its_containers_across_every_turn() {
    // No nesting at all: the carry path must work for the plainest shape
    // there is, and every consecutive pair of batches must chain through a
    // signature that still holds the list open.
    let mut markdown = String::from("# Flat\n\n");
    for index in 0..200 {
        writeln!(markdown, "- flat-{index}").expect("write flat list item");
    }
    markdown.push_str("\nTAIL-AFTER-FLAT-LIST\n");

    let plan = plan_markdown(&markdown);
    assert!(plan.is_complete());
    assert_eq!(plan.omissions(), 0);
    assert_plan_invariants(&plan);
    assert!(plan.batches.len() > 1, "a 200-item list must sub-slice");
    let carried = plan
        .batches
        .iter()
        .filter(|batch| {
            batch
                .open_carry()
                .containers()
                .iter()
                .any(|container| matches!(container, MarkdownOpenContainer::List { .. }))
        })
        .count();
    assert!(
        carried >= plan.batches.len() - 1,
        "every batch but the last must leave the flat list open: {carried} of {}",
        plan.batches.len()
    );
    let text = projected_text(&plan);
    for index in 0..200 {
        assert!(text.contains(&format!("flat-{index}")), "flat-{index} lost");
    }
    assert!(text.contains("TAIL-AFTER-FLAT-LIST"));
}

#[test]
fn a_carried_blockquote_keeps_its_alert_kind_across_a_turn() {
    // A GitHub alert callout is a blockquote with a `kind`. If the carry
    // signature dropped that kind, the projector would silently resume the
    // callout as a plain quote, so the kind must survive the boundary.
    // Each quoted paragraph is separated by a bare `>` so the alert body is
    // many cuttable paragraphs rather than one indivisible lazy-continued
    // paragraph.
    let mut markdown = String::from("> [!WARNING]\n");
    for index in 0..200 {
        writeln!(markdown, "> alert-line-{index}\n>").expect("write alert paragraph");
    }
    markdown.push_str("\nTAIL-AFTER-ALERT\n");

    let plan = plan_markdown(&markdown);
    assert!(plan.is_complete());
    assert_eq!(plan.omissions(), 0);
    assert_plan_invariants(&plan);
    assert!(plan.batches.len() > 1, "the alert must span turns");
    let carried_kinds: Vec<Option<MarkdownBlockQuoteKind>> = plan
        .batches
        .iter()
        .flat_map(|batch| batch.open_carry().containers())
        .filter_map(|container| match container {
            MarkdownOpenContainer::BlockQuote { kind } => Some(*kind),
            _ => None,
        })
        .collect();
    assert!(
        !carried_kinds.is_empty(),
        "the cut must land inside the alert blockquote"
    );
    assert!(
        carried_kinds
            .iter()
            .all(|kind| *kind == Some(MarkdownBlockQuoteKind::Warning)),
        "every carried frame must keep the alert kind: {carried_kinds:?}"
    );
    assert!(projected_text(&plan).contains("TAIL-AFTER-ALERT"));
}

#[test]
fn an_indented_code_block_past_the_code_ceiling_crosses_on_the_code_track() {
    // The indented shape reaches the code byte ceiling through per-line
    // events, which is the path a fenced block cannot take: the pinned
    // parser coalesces a fence into one `Text` event.
    let line = "    ".to_string() + &"z".repeat(508);
    let lines = MAX_MARKDOWN_CARRIED_EMBED_BYTES / 512 + 8;
    let mut markdown = String::from("# Indented\n\n");
    for _ in 0..lines {
        markdown.push_str(&line);
        markdown.push('\n');
    }
    markdown.push_str("\nTAIL-AFTER-INDENTED-CROSSING\n");

    let plan = plan_markdown(&markdown);
    assert!(plan.is_complete());
    assert_eq!(
        plan.user_visible_omissions(),
        0,
        "a carried-embed crossing is a charge carrier, not a user-visible omission"
    );
    assert_plan_invariants(&plan);
    let omission = single_omission(&plan);
    assert_eq!(omission.reason, MarkdownOmissionReason::CarriedEmbedBytes);
    assert_eq!(
        omission.unretained.cells, 0,
        "a code block has no cells to charge"
    );
    assert!(
        omission.unretained.source_bytes > 0,
        "the unretained remainder must be carried forward for the projector"
    );
    assert!(
        retained_bytes(&plan) <= MAX_MARKDOWN_CARRIED_EMBED_BYTES,
        "retention must stop at the code ceiling"
    );
    assert!(
        projected_text(&plan).contains("TAIL-AFTER-INDENTED-CROSSING"),
        "the document after the crossing must still render"
    );
}

#[test]
fn every_global_budget_is_still_terminal() {
    let events = "x\n\n".repeat(MAX_MARKDOWN_EVENTS);
    assert_eq!(
        plan_markdown(&events).limit,
        Some(MarkdownPlanLimit::Events)
    );
    let depth = format!("{}deep{}", "> ".repeat(200), "\n");
    assert_eq!(
        plan_markdown(&depth).limit,
        Some(MarkdownPlanLimit::StructuralDepth)
    );
}

#[test]
fn table_head_forbids_a_cut_between_header_cells() {
    assert!(matches!(
        classify_frame(&Tag::TableHead),
        MarkdownFrame::InlineFlow
    ));
    assert!(matches!(
        classify_frame(&Tag::Table(vec![pulldown_cmark::Alignment::None])),
        MarkdownFrame::Block(MarkdownOpenContainer::Table { .. })
    ));
    // A cut after `End(TableHead)` leaves only `Table` open, which is
    // admissible; a cut between header cells would leave `TableHead` open.
    let head_open = [
        MarkdownFrame::Block(MarkdownOpenContainer::Table {
            alignments: Vec::new(),
        }),
        classify_frame(&Tag::TableHead),
    ];
    assert!(!MarkdownFrameStack::frames_are_admissible(&head_open));
    assert!(MarkdownFrameStack::frames_are_admissible(&head_open[..1]));
}

#[test]
fn footnote_definitions_are_cut_forbidding_by_decision() {
    assert!(matches!(
        classify_frame(&Tag::FootnoteDefinition("label".into())),
        MarkdownFrame::InlineFlow
    ));
}

#[test]
fn depth_zero_is_the_trivial_admissible_boundary() {
    assert!(MarkdownFrameStack::frames_are_admissible(&[]));
    assert!(MarkdownFrameStack::frames_are_admissible(&[
        MarkdownFrame::Block(MarkdownOpenContainer::Item)
    ]));
    assert!(!MarkdownFrameStack::frames_are_admissible(&[
        MarkdownFrame::Block(MarkdownOpenContainer::Item),
        MarkdownFrame::InlineFlow,
    ]));
}

#[test]
fn ordinary_documents_keep_todays_batch_packing() {
    // Blocks that fit a slice are never sub-sliced, so a document of
    // ordinary blocks packs into exactly the batches it packed into before
    // checkpoint sub-slicing existed: every batch is filled to just under
    // the slice budget by whole blocks.
    let mut markdown = String::new();
    for index in 0..400 {
        writeln!(markdown, "paragraph {index}\n").expect("write paragraph fixture");
    }
    let plan = plan_markdown(&markdown);
    assert!(plan.is_complete());
    assert_eq!(plan.omissions(), 0);
    assert_plan_invariants(&plan);
    assert_eq!(plan.batches.len(), 5);
    assert_eq!(
        plan.batches
            .iter()
            .map(MarkdownEventBatch::len)
            .collect::<Vec<_>>(),
        vec![255, 255, 255, 255, 180]
    );
    assert!(
        plan.batches
            .iter()
            .all(|batch| batch.expected_carry().is_empty() && batch.open_carry().is_empty()),
        "whole-block packing never leaves a container open"
    );
}

#[test]
fn sub_sliced_batches_stay_within_both_slice_budgets() {
    for markdown in [
        helm_readme_fixture(),
        oversized_list_fixture(),
        oversized_blockquote_fixture(),
        list_with_one_overflowing_item_fixture(),
        nested_table_in_list_fixture(),
        oversized_definition_list_fixture(),
        indented_code_block_fixture(),
    ] {
        let plan = plan_markdown(&markdown);
        assert_plan_invariants(&plan);
    }
}

#[test]
fn an_oversized_definition_list_is_sub_sliced_at_definition_boundaries() {
    let plan = plan_markdown(&oversized_definition_list_fixture());
    assert!(plan.is_complete());
    assert_eq!(plan.omissions(), 0);
    assert_plan_invariants(&plan);
    assert!(
        plan.batches.len() > 1,
        "an oversized definition list is sub-sliced"
    );
    let text = projected_text(&plan);
    for index in 0..DEFINITION_ENTRIES {
        assert!(
            text.contains(&format!("definition-{index}")),
            "definition-{index} missing"
        );
    }
    assert!(text.contains("TAIL-AFTER-DEFINITIONS"));
    let carried = plan.batches.iter().any(|batch| {
        batch.open_carry().containers().iter().any(|container| {
            matches!(
                container,
                MarkdownOpenContainer::DefinitionList
                    | MarkdownOpenContainer::DefinitionListDefinition
            )
        })
    });
    assert!(carried, "the cut must land inside the definition list");
}

#[test]
fn an_indented_code_block_is_sub_sliced_at_text_run_boundaries() {
    // The fenced-body coalescing that makes code-block checkpoints
    // unreachable is fenced-only: an indented block emits one `Text` per
    // line, so this is the reachable retained-code-text checkpoint path.
    let plan = plan_markdown(&indented_code_block_fixture());
    assert!(plan.is_complete());
    assert_eq!(
        plan.omissions(),
        0,
        "the block stays inside the carried byte ceiling"
    );
    assert_plan_invariants(&plan);
    assert!(
        plan.batches.len() > 1,
        "an indented code block over one slice is sub-sliced"
    );
    let carried = plan.batches.iter().any(|batch| {
        matches!(
            batch.open_carry().containers(),
            [MarkdownOpenContainer::CodeBlock { .. }]
        )
    });
    assert!(carried, "a cut must leave the code block itself open");
    let text = projected_text(&plan);
    assert!(text.contains("indented-line-0\n"));
    assert!(text.contains(&format!("indented-line-{}\n", INDENTED_CODE_LINES - 1)));
    assert!(text.contains("TAIL-AFTER-INDENTED-CODE"));
}

#[test]
fn an_indivisible_segment_subsumes_a_crossing_marker_inside_it() {
    // A footnote definition is cut-forbidding, so a footnote holding both
    // dense inline content and an over-ceiling table is one indivisible
    // segment. Dropping it must report exactly one omission: the crossing
    // marker recorded inside the dropped unit is consumed, not re-reported,
    // because the segment's own omission already replaces that content.
    let dense = (0..200).map(|_| "**x** ").collect::<String>();
    let mut markdown = format!("See[^{FOOTNOTE_LABEL}].\n\n[^{FOOTNOTE_LABEL}]: {dense}\n\n");
    for line in cell_table(PAST_CEILING_COLUMNS, PAST_CEILING_ROWS).lines() {
        markdown.push_str("    ");
        markdown.push_str(line);
        markdown.push('\n');
    }
    markdown.push_str("\nTAIL-AFTER-SUBSUMED\n");

    let plan = plan_markdown(&markdown);
    assert!(plan.is_complete());
    assert_plan_invariants(&plan);
    assert_eq!(
        plan.omissions(),
        1,
        "a dropped segment must not also report the crossing inside it"
    );
    let omission = single_omission(&plan);
    assert_eq!(omission.reason, MarkdownOmissionReason::SliceEvents);
    assert_eq!(retained_cells(&plan), 0);
    assert!(projected_text(&plan).contains("TAIL-AFTER-SUBSUMED"));
}

#[test]
fn state_extremes_keep_planning_to_the_end_of_the_document() {
    let dense = || (0..300).map(|_| "**x** ").collect::<String>();

    // Empty document.
    let plan = plan_markdown("");
    assert!(plan.is_complete());
    assert!(plan.batches.is_empty());
    assert_eq!(plan.omissions(), 0);
    assert_plan_invariants(&plan);

    // One tiny block.
    let plan = plan_markdown("tiny\n");
    assert_eq!(plan.batches.len(), 1);
    assert_eq!(plan.omissions(), 0);
    assert_plan_invariants(&plan);

    // One oversized block and nothing else.
    let plan = plan_markdown(&dense());
    assert!(plan.is_complete());
    assert_eq!(plan.omissions(), 1);
    assert_eq!(plan.projected_events(), 0);
    assert_plan_invariants(&plan);

    // Oversized first, middle, and last.
    for markdown in [
        format!("{}\n\nHEAD-A\n\nHEAD-B\n", dense()),
        format!("HEAD-A\n\n{}\n\nHEAD-B\n", dense()),
        format!("HEAD-A\n\nHEAD-B\n\n{}\n", dense()),
    ] {
        let plan = plan_markdown(&markdown);
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), 1);
        assert_plan_invariants(&plan);
        let text = projected_text(&plan);
        assert!(text.contains("HEAD-A"), "sibling lost: {text}");
        assert!(text.contains("HEAD-B"), "sibling lost: {text}");
    }

    // Several oversized blocks separated by ordinary prose.
    let markdown = format!(
        "{a}\n\nBETWEEN\n\n{b}\n\nTAIL-SEVERAL\n",
        a = dense(),
        b = dense()
    );
    let plan = plan_markdown(&markdown);
    assert!(plan.is_complete());
    assert_eq!(plan.omissions(), 2);
    assert_plan_invariants(&plan);
    let text = projected_text(&plan);
    assert!(text.contains("BETWEEN"));
    assert!(text.contains("TAIL-SEVERAL"));
}

#[test]
fn a_container_whose_every_unit_overflows_keeps_its_shell_and_the_tail() {
    let dense = (0..200).map(|_| "**x** ").collect::<String>();
    let mut markdown = String::from("# Every item overflows\n\n");
    for _ in 0..6 {
        writeln!(markdown, "- {dense}\n").expect("write dense item");
    }
    markdown.push_str("\nTAIL-AFTER-ALL-OVERFLOW\n");

    let plan = plan_markdown(&markdown);
    assert!(plan.is_complete());
    assert_plan_invariants(&plan);
    assert_eq!(plan.omissions(), 6, "each item overflows on its own");
    assert!(
        plan.batches
            .iter()
            .flat_map(MarkdownEventBatch::omissions)
            .all(|marker| marker.omission.scope == MarkdownOmissionScope::ContainerSegment),
        "an overflowing item is a container segment, not the whole list"
    );
    // The list shell and its item structure survive: every `Item` start is
    // its own tiny segment, so the empty items still render.
    let items = plan
        .batches
        .iter()
        .flat_map(MarkdownEventBatch::events)
        .filter(|event| matches!(event, MarkdownEvent::Start(MarkdownTag::Item)))
        .count();
    assert_eq!(items, 6);
    assert!(projected_text(&plan).contains("TAIL-AFTER-ALL-OVERFLOW"));
}

#[test]
fn an_oversized_block_followed_by_a_global_stop_reports_the_global_limit() {
    let dense = (0..300).map(|_| "**x** ").collect::<String>();
    let mut markdown = format!("{dense}\n\n");
    while markdown.len() < 512 * 1024 {
        markdown.push_str("filler paragraph\n\n");
    }
    let plan = plan_markdown(&markdown);
    assert_eq!(
        plan.limit,
        Some(MarkdownPlanLimit::Events),
        "the global budget still terminates planning"
    );
    assert!(plan.omissions() >= 1, "the oversized block still omitted");
    assert_plan_invariants(&plan);
}

#[test]
fn an_oversized_block_at_the_depth_ceiling_is_still_omitted_and_continued() {
    let depth = MAX_MARKDOWN_STRUCTURE_DEPTH / 2;
    let dense = (0..300).map(|_| "**x** ").collect::<String>();
    let quote = "> ".repeat(depth);
    let markdown = format!("{quote}{dense}\n\nTAIL-AT-DEPTH\n");
    let plan = plan_markdown(&markdown);
    assert_eq!(
        plan.limit, None,
        "the fixture must stay inside the depth ceiling"
    );
    assert!(plan.metrics.max_depth >= depth);
    assert_plan_invariants(&plan);
    assert_eq!(plan.omissions(), 1);
    assert_eq!(
        single_omission(&plan).scope,
        MarkdownOmissionScope::ContainerSegment,
        "a nested oversized paragraph is a container segment"
    );
    assert!(projected_text(&plan).contains("TAIL-AT-DEPTH"));
}

#[test]
fn a_table_nested_in_a_list_item_is_still_sub_sliced() {
    let plan = plan_markdown(&nested_table_in_list_fixture());
    assert!(plan.is_complete());
    assert_eq!(plan.omissions(), 0);
    assert_plan_invariants(&plan);
    let text = projected_text(&plan);
    assert!(text.contains("nested-39"));
    assert!(text.contains("TAIL-AFTER-NESTED"));
    let carried = plan.batches.iter().any(|batch| {
        batch
            .open_carry()
            .containers()
            .iter()
            .any(|container| matches!(container, MarkdownOpenContainer::Table { .. }))
    });
    assert!(carried, "a nested table must be cut at a row boundary");
}

#[test]
fn a_large_byte_table_within_the_cell_ceiling_retains_every_cell() {
    let plan = plan_markdown(&huge_byte_table_fixture());
    assert!(plan.is_complete());
    assert_plan_invariants(&plan);
    assert_eq!(
        plan.omissions(),
        0,
        "the code-block byte ceiling must not reach tables"
    );
    assert_eq!(retained_cells(&plan), HUGE_BYTE_TABLE_CELLS);
    assert_eq!(retained_bytes(&plan), plan.metrics.retained_bytes);
    assert!(
        retained_bytes(&plan) > MAX_MARKDOWN_CARRIED_EMBED_BYTES,
        "the fixture must actually exceed the code-block byte ceiling"
    );
    assert!(projected_text(&plan).contains("TAIL-AFTER-WIDE"));
}

#[test]
fn a_small_table_of_huge_cells_still_renders_completely() {
    // Few events but ~192 KiB of cells: this renders completely today, so
    // no ceiling may drop a cell from it.
    let cell = "q".repeat(32 * 1024);
    let mut markdown = String::from("| a | b |\n| --- | --- |\n");
    for _ in 0..3 {
        writeln!(markdown, "| {cell} | {cell} |").expect("write huge row");
    }
    markdown.push_str("\nTAIL-AFTER-HUGE-CELLS\n");
    let plan = plan_markdown(&markdown);
    assert!(plan.is_complete());
    assert_plan_invariants(&plan);
    assert_eq!(plan.omissions(), 0);
    assert_eq!(retained_cells(&plan), 2 + 3 * 2);
    assert_eq!(retained_bytes(&plan), plan.metrics.retained_bytes);
    assert!(projected_text(&plan).contains("TAIL-AFTER-HUGE-CELLS"));
}

#[test]
fn a_table_past_the_cell_ceiling_reports_its_unretained_counts() {
    let plan = plan_markdown(&table_past_the_cell_ceiling_fixture());
    assert!(plan.is_complete());
    assert_plan_invariants(&plan);
    let omission = single_omission(&plan);
    assert_eq!(omission.reason, MarkdownOmissionReason::CarriedEmbedCells);
    assert_eq!(omission.scope, MarkdownOmissionScope::ContainerSegment);

    // The projector charges these counts onto the in-flight table, so its
    // cell check sees the block's true total and fires exactly as today.
    let total_cells = past_ceiling_cells();
    assert!(total_cells > MAX_MARKDOWN_CARRIED_TABLE_CELLS);
    assert_eq!(
        retained_cells(&plan) + omission.unretained.cells,
        total_cells
    );
    assert!(retained_cells(&plan) <= MAX_MARKDOWN_CARRIED_TABLE_CELLS);
    assert!(omission.unretained.cells > 0);
    assert_eq!(
        retained_bytes(&plan) + omission.unretained.source_bytes,
        plan.metrics.retained_bytes,
        "every charged byte is retained or reported as unretained"
    );
    assert!(projected_text(&plan).contains("TAIL-AFTER-PAST-CEILING"));
}

#[test]
fn a_crossing_marker_sits_inside_its_still_open_container() {
    let plan = plan_markdown(&table_past_the_cell_ceiling_fixture());
    assert_plan_invariants(&plan);
    let batch = plan
        .batches
        .iter()
        .find(|batch| !batch.omissions().is_empty())
        .expect("the crossing emits a marker");
    let marker = batch.omissions()[0];
    assert!(
        matches!(
            batch.events().get(marker.at_event),
            Some(MarkdownEvent::End(_))
        ),
        "the marker must precede the container's own end event"
    );
    // The crossing withdrew its partial row, so the last retained event
    // before the marker closes a row rather than opening one.
    assert!(matches!(
        batch.events().get(marker.at_event.saturating_sub(1)),
        Some(MarkdownEvent::End(MarkdownTagEnd::TableRow))
    ));
}

#[test]
fn carried_embed_crossing_reports_code_block_bytes() {
    let mut markdown = String::from("```text\n");
    markdown.push_str(&"c".repeat(MAX_MARKDOWN_CARRIED_EMBED_BYTES + 4096));
    markdown.push_str("\n```\n\nTAIL-AFTER-BIG-CODE\n");
    let plan = plan_markdown(&markdown);
    assert!(plan.is_complete());
    assert_plan_invariants(&plan);
    let omission = single_omission(&plan);
    assert_eq!(omission.reason, MarkdownOmissionReason::CarriedEmbedBytes);
    assert_eq!(omission.unretained.cells, 0);
    assert!(omission.unretained.source_bytes > MAX_MARKDOWN_CARRIED_EMBED_BYTES);
    assert_eq!(
        retained_bytes(&plan) + omission.unretained.source_bytes,
        plan.metrics.retained_bytes
    );
    assert!(projected_text(&plan).contains("TAIL-AFTER-BIG-CODE"));
}

/// Lines and per-line length of the footnote-nested code block.
const FOOTNOTE_CODE_LINES: usize = 80;
const FOOTNOTE_CODE_LINE_LEN: usize = 900;
/// Intro paragraph inside the footnote definition, before the embed.
const FOOTNOTE_INTRO: &str = "an intro paragraph that belongs to the footnote, not the embed";
/// A deliberately long label, so a withdrawal that reached below the
/// embed's own start would visibly leak the footnote's own bytes.
const FOOTNOTE_LABEL: &str = "an-unusually-long-footnote-label-for-leak-detection";

/// Code text bytes the footnote-nested code block contributes.
fn footnote_code_bytes() -> usize {
    FOOTNOTE_CODE_LINES * (FOOTNOTE_CODE_LINE_LEN + 1)
}

/// A code block past [`MAX_MARKDOWN_CARRIED_EMBED_BYTES`] nested inside a
/// `FootnoteDefinition`, after an intro paragraph in the same footnote.
///
/// `FootnoteDefinition` is cut-forbidding, so it suppresses every
/// checkpoint inside itself and the embedded block has no checkpoint of its
/// own. The withdrawal must still floor at the embed's own start, or the
/// intro paragraph and the container starts go with it.
fn footnote_nested_code_fixture() -> String {
    let mut markdown =
        format!("See[^{FOOTNOTE_LABEL}].\n\n[^{FOOTNOTE_LABEL}]: {FOOTNOTE_INTRO}\n\n");
    let line = "c".repeat(FOOTNOTE_CODE_LINE_LEN);
    for _ in 0..FOOTNOTE_CODE_LINES {
        markdown.push_str("        ");
        markdown.push_str(&line);
        markdown.push('\n');
    }
    markdown.push_str("\nTAIL-AFTER-FOOTNOTE-CODE\n");
    markdown
}

/// A table past [`MAX_MARKDOWN_CARRIED_TABLE_CELLS`] nested inside a
/// `FootnoteDefinition`, after an intro paragraph in the same footnote.
fn footnote_nested_table_fixture() -> String {
    let mut markdown =
        format!("See[^{FOOTNOTE_LABEL}].\n\n[^{FOOTNOTE_LABEL}]: {FOOTNOTE_INTRO}\n\n");
    for line in cell_table(PAST_CEILING_COLUMNS, PAST_CEILING_ROWS).lines() {
        markdown.push_str("    ");
        markdown.push_str(line);
        markdown.push('\n');
    }
    markdown.push_str("\nTAIL-AFTER-FOOTNOTE-TABLE\n");
    markdown
}

#[test]
fn a_code_block_crossing_inside_a_footnote_definition_stays_balanced() {
    assert!(
        footnote_code_bytes() > MAX_MARKDOWN_CARRIED_EMBED_BYTES,
        "the fixture must cross the carried byte ceiling"
    );
    let plan = plan_markdown(&footnote_nested_code_fixture());
    assert!(plan.is_complete());
    // The balance assertion is the point: a withdrawal that reached below
    // the code block's own start would drop `Start(FootnoteDefinition)` and
    // `Start(CodeBlock)` while their ends stay retained.
    assert_plan_invariants(&plan);

    let omission = single_omission(&plan);
    assert_eq!(omission.reason, MarkdownOmissionReason::CarriedEmbedBytes);
    assert_eq!(omission.scope, MarkdownOmissionScope::ContainerSegment);
    assert_eq!(omission.unretained.cells, 0);
    assert_eq!(
        omission.unretained.source_bytes,
        footnote_code_bytes(),
        "only the code block's own text may be charged as unretained"
    );
    assert_eq!(
        retained_bytes(&plan) + omission.unretained.source_bytes,
        plan.metrics.retained_bytes
    );
    let text = projected_text(&plan);
    assert!(
        text.contains(FOOTNOTE_INTRO),
        "the footnote's own intro paragraph must survive"
    );
    assert!(text.contains("TAIL-AFTER-FOOTNOTE-CODE"));
}

#[test]
fn a_table_crossing_inside_a_footnote_definition_stays_balanced() {
    let plan = plan_markdown(&footnote_nested_table_fixture());
    assert!(plan.is_complete());
    assert_plan_invariants(&plan);

    let omission = single_omission(&plan);
    assert_eq!(omission.reason, MarkdownOmissionReason::CarriedEmbedCells);
    assert_eq!(omission.scope, MarkdownOmissionScope::ContainerSegment);
    // With no checkpoint inside the footnote the withdrawal floors at the
    // table's own start, so the whole table body is reported unretained and
    // the projector's cell check still sees the real total.
    assert_eq!(retained_cells(&plan), 0);
    assert_eq!(omission.unretained.cells, past_ceiling_cells());
    // Every cell of `cell_table` holds exactly one text byte, so the
    // unretained byte count is the cell count. The footnote's label and
    // intro paragraph are far larger than one byte, so either of them
    // leaking into this charge would break the equality.
    assert_eq!(
        omission.unretained.source_bytes,
        past_ceiling_cells(),
        "no footnote-owned byte may be folded into the table's charge"
    );
    assert_eq!(
        retained_bytes(&plan) + omission.unretained.source_bytes,
        plan.metrics.retained_bytes
    );
    let text = projected_text(&plan);
    assert!(
        text.contains(FOOTNOTE_INTRO),
        "the footnote's own intro paragraph must survive"
    );
    assert!(text.contains("TAIL-AFTER-FOOTNOTE-TABLE"));
}

#[test]
fn a_code_block_exactly_at_the_carried_byte_ceiling_is_retained_whole() {
    // The fenced text event carries its trailing newline, so the body is
    // one byte short of the ceiling to land exactly on it.
    let markdown = format!(
        "```text\n{}\n```\n\nTAIL-AT-CEILING\n",
        "c".repeat(MAX_MARKDOWN_CARRIED_EMBED_BYTES - 1)
    );
    let plan = plan_markdown(&markdown);
    assert!(plan.is_complete());
    assert_plan_invariants(&plan);
    assert_eq!(plan.omissions(), 0, "the ceiling itself is admissible");
    assert_eq!(retained_bytes(&plan), plan.metrics.retained_bytes);
    assert!(projected_text(&plan).contains("TAIL-AT-CEILING"));

    let over = format!(
        "```text\n{}\n```\n\nTAIL-OVER-CEILING\n",
        "c".repeat(MAX_MARKDOWN_CARRIED_EMBED_BYTES)
    );
    let plan = plan_markdown(&over);
    assert_eq!(plan.omissions(), 1, "one byte over must cross");
    assert_eq!(
        single_omission(&plan).reason,
        MarkdownOmissionReason::CarriedEmbedBytes
    );
    assert!(projected_text(&plan).contains("TAIL-OVER-CEILING"));
}

#[test]
fn a_table_exactly_at_the_carried_cell_ceiling_is_retained_whole() {
    let columns = 4;
    let rows = MAX_MARKDOWN_CARRIED_TABLE_CELLS / columns - 1;
    assert_eq!(
        cell_table_cells(columns, rows),
        MAX_MARKDOWN_CARRIED_TABLE_CELLS
    );
    let plan = plan_markdown(&cell_table(columns, rows));
    assert!(plan.is_complete());
    assert_plan_invariants(&plan);
    assert_eq!(plan.omissions(), 0, "the ceiling itself is admissible");
    assert_eq!(retained_cells(&plan), MAX_MARKDOWN_CARRIED_TABLE_CELLS);

    let plan = plan_markdown(&cell_table(columns, rows + 1));
    assert_eq!(plan.omissions(), 1, "one row over must cross");
    assert_eq!(
        single_omission(&plan).reason,
        MarkdownOmissionReason::CarriedEmbedCells
    );
}

/// One paragraph whose inline run makes the paragraph exactly `events` long.
///
/// `Start(Paragraph)` and `End(Paragraph)` are two of the events, and the
/// remainder is filled with `Text`/`Code` alternations, which the pinned
/// parser never coalesces across an inline code span.
fn paragraph_of_exactly(events: usize) -> String {
    // "**s**" is Start(Strong), Text, End(Strong); each " `a`" adds a Text
    // and a Code; the optional trailing " end" adds one more Text. Each
    // pair contributes two events, so the trailing text is what makes an
    // odd total reachable.
    let with_trailing_text = events.is_multiple_of(2);
    let fixed = if with_trailing_text { 6 } else { 5 };
    assert!(events >= fixed, "paragraph is too short to build");
    let pairs = (events - fixed) / 2;
    let mut paragraph = String::from("**s**");
    for _ in 0..pairs {
        paragraph.push_str(" `a`");
    }
    if with_trailing_text {
        paragraph.push_str(" end");
    }
    paragraph.push('\n');
    paragraph
}

#[test]
fn two_blocks_exactly_filling_one_slice_share_one_batch() {
    // The single-block fixture below cannot pin the packing comparison,
    // because a batch is never cut while it is still empty. Two blocks that
    // together land exactly on the budget can: the boundary must admit the
    // second block rather than open a batch for it.
    let mut markdown = String::from("a\n\n");
    markdown.push_str(&paragraph_of_exactly(
        MARKDOWN_EVENTS_PER_PROJECTION_SLICE - 3,
    ));
    let plan = plan_markdown(&markdown);
    assert_eq!(
        plan.metrics.events, MARKDOWN_EVENTS_PER_PROJECTION_SLICE,
        "the fixture must sit exactly on the event budget"
    );
    assert!(plan.is_complete());
    assert_eq!(plan.omissions(), 0);
    assert_eq!(
        plan.batches.len(),
        1,
        "the budget itself must not open a second batch"
    );
    assert_plan_invariants(&plan);

    // One event over must split, so the assertion above is a boundary and
    // not an accident of the fixture size.
    let mut markdown = String::from("a\n\n");
    markdown.push_str(&paragraph_of_exactly(
        MARKDOWN_EVENTS_PER_PROJECTION_SLICE - 2,
    ));
    let plan = plan_markdown(&markdown);
    assert_eq!(
        plan.metrics.events,
        MARKDOWN_EVENTS_PER_PROJECTION_SLICE + 1
    );
    assert_eq!(plan.batches.len(), 2);
    assert_eq!(plan.omissions(), 0);
    assert_plan_invariants(&plan);
}

#[test]
fn a_sub_sliced_block_retains_a_segment_exactly_at_the_event_budget() {
    // A loose list whose first item's paragraph is exactly one slice long,
    // followed by enough siblings that the list itself must sub-slice. The
    // segment on the budget is admissible, so nothing is omitted.
    let mut markdown = String::new();
    writeln!(
        markdown,
        "- {}",
        paragraph_of_exactly(MARKDOWN_EVENTS_PER_PROJECTION_SLICE).trim_end()
    )
    .expect("write loose list fixture");
    for index in 0..3 {
        writeln!(markdown, "\n- item-{index}").expect("write loose list sibling");
    }
    let plan = plan_markdown(&markdown);
    assert!(
        plan.metrics.events > MARKDOWN_EVENTS_PER_PROJECTION_SLICE,
        "the list must be large enough to sub-slice"
    );
    assert!(plan.batches.len() >= 2, "the list must actually sub-slice");
    assert_eq!(
        plan.omissions(),
        0,
        "a segment exactly on the budget must be retained, not omitted"
    );
    assert!(plan.is_complete());
    let text = projected_text(&plan);
    assert!(text.contains("end"), "the on-budget segment must render");
    for index in 0..3 {
        assert!(text.contains(&format!("item-{index}")));
    }
    assert_plan_invariants(&plan);
}

#[test]
fn two_blocks_exactly_filling_the_slice_byte_budget_share_one_batch() {
    // The byte half of the packing comparison, pinned the same way as the
    // event half: only a second block can exercise it, because a batch is
    // never cut while empty.
    let markdown = format!(
        "a\n\n{}\n",
        "x".repeat(MARKDOWN_BYTES_PER_PROJECTION_SLICE - 1)
    );
    let plan = plan_markdown(&markdown);
    assert_eq!(
        plan.metrics.retained_bytes, MARKDOWN_BYTES_PER_PROJECTION_SLICE,
        "the fixture must sit exactly on the byte budget"
    );
    assert!(plan.is_complete());
    assert_eq!(plan.omissions(), 0);
    assert_eq!(
        plan.batches.len(),
        1,
        "the byte budget itself must not open a second batch"
    );
    assert_plan_invariants(&plan);

    let markdown = format!("a\n\n{}\n", "x".repeat(MARKDOWN_BYTES_PER_PROJECTION_SLICE));
    let plan = plan_markdown(&markdown);
    assert_eq!(plan.batches.len(), 2, "one byte over must split");
    assert_eq!(plan.omissions(), 0);
    assert_plan_invariants(&plan);
}

#[test]
fn a_structure_exactly_at_the_depth_ceiling_still_completes() {
    // The ceiling is a maximum, not the first rejected depth, and the
    // `every_global_budget_is_still_terminal` case above only proves the
    // rejecting side.
    // The innermost paragraph is itself a frame, so N blockquotes reach
    // depth N + 1.
    let at_ceiling = format!("{}deep\n", "> ".repeat(MAX_MARKDOWN_STRUCTURE_DEPTH - 1));
    let plan = plan_markdown(&at_ceiling);
    assert_eq!(plan.limit, None, "the ceiling itself must still complete");
    assert_eq!(plan.metrics.max_depth, MAX_MARKDOWN_STRUCTURE_DEPTH);
    assert!(projected_text(&plan).contains("deep"));

    let over = format!("{}deep\n", "> ".repeat(MAX_MARKDOWN_STRUCTURE_DEPTH));
    assert_eq!(
        plan_markdown(&over).limit,
        Some(MarkdownPlanLimit::StructuralDepth)
    );
}

#[test]
fn a_segment_on_the_event_budget_but_over_the_byte_budget_reports_bytes() {
    // Both slice budgets can be crossed by one segment. The reported reason
    // must come from the budget that was actually exceeded: a segment
    // sitting exactly *on* the event budget has not exceeded it, so an
    // over-byte segment there is a byte omission, not an event omission.
    let span_bytes = MARKDOWN_BYTES_PER_PROJECTION_SLICE / 100;
    let mut dense = String::from("**s**");
    for _ in 0..125 {
        write!(dense, " `{}`", "y".repeat(span_bytes)).expect("write dense inline fixture");
    }
    dense.push_str(" end");
    let mut markdown = format!("- {dense}\n");
    for index in 0..3 {
        writeln!(markdown, "\n- item-{index}").expect("write loose list sibling");
    }

    let plan = plan_markdown(&markdown);
    let omission = single_omission(&plan);
    assert_eq!(
        omission.reason,
        MarkdownOmissionReason::SliceBytes,
        "the byte budget is the one that was exceeded"
    );
    assert_eq!(omission.scope, MarkdownOmissionScope::ContainerSegment);
    let text = projected_text(&plan);
    for index in 0..3 {
        assert!(
            text.contains(&format!("item-{index}")),
            "sibling {index} lost"
        );
    }
    assert_plan_invariants(&plan);
}

#[test]
fn a_sub_sliced_block_retains_a_segment_exactly_at_the_byte_budget() {
    // The byte twin of the segment-budget boundary above. The first loose
    // item's paragraph retains exactly one slice of bytes, and the siblings
    // push the list past a slice so the block really is sub-sliced.
    let mut markdown = format!("- {}\n", "x".repeat(MARKDOWN_BYTES_PER_PROJECTION_SLICE));
    for index in 0..3 {
        writeln!(markdown, "\n- item-{index}").expect("write loose list sibling");
    }
    let plan = plan_markdown(&markdown);
    assert!(
        plan.metrics.retained_bytes > MARKDOWN_BYTES_PER_PROJECTION_SLICE,
        "the list must be large enough to sub-slice"
    );
    assert!(plan.batches.len() >= 2, "the list must actually sub-slice");
    assert_eq!(
        plan.omissions(),
        0,
        "a segment exactly on the byte budget must be retained, not omitted"
    );
    assert!(plan.is_complete());
    let text = projected_text(&plan);
    for index in 0..3 {
        assert!(
            text.contains(&format!("item-{index}")),
            "sibling {index} lost"
        );
    }
    assert_plan_invariants(&plan);

    // One byte over the same segment must omit, so the assertion above is a
    // boundary rather than an artifact of the fixture size.
    let mut markdown = format!(
        "- {}\n",
        "x".repeat(MARKDOWN_BYTES_PER_PROJECTION_SLICE + 1)
    );
    for index in 0..3 {
        writeln!(markdown, "\n- item-{index}").expect("write loose list sibling");
    }
    let plan = plan_markdown(&markdown);
    let omission = single_omission(&plan);
    assert_eq!(omission.reason, MarkdownOmissionReason::SliceBytes);
    assert_eq!(omission.scope, MarkdownOmissionScope::ContainerSegment);
    assert_plan_invariants(&plan);
}

#[test]
fn documented_planning_ceilings_keep_their_published_values() {
    // README.md and AGENTS.md quote these numbers to users, and
    // `MAX_MARKDOWN_CARRIED_EMBED_BYTES` / `MAX_MARKDOWN_CARRIED_TABLE_CELLS`
    // must equal the projection-side widget budgets they mirror, which
    // `services` cannot import. Pin them here so a silent edit on either
    // side fails.
    assert_eq!(MAX_MARKDOWN_SOURCE_BYTES, 4 * 1024 * 1024);
    assert_eq!(MAX_MARKDOWN_EVENTS, 64 * 1024);
    assert_eq!(MAX_MARKDOWN_STRUCTURE_DEPTH, 128);
    assert_eq!(MAX_MARKDOWN_EMBED_DESCRIPTORS, 256);
    assert_eq!(MAX_MARKDOWN_RETAINED_BYTES, 8 * 1024 * 1024);
    assert_eq!(MARKDOWN_EVENTS_PER_PROJECTION_SLICE, 256);
    assert_eq!(MARKDOWN_BYTES_PER_PROJECTION_SLICE, 256 * 1024);
    assert_eq!(MAX_MARKDOWN_CARRIED_EMBED_BYTES, 64 * 1024);
    assert_eq!(MAX_MARKDOWN_CARRIED_TABLE_CELLS, 1_024);
    assert_eq!(MAX_MARKDOWN_PLACEHOLDER_WIDGETS, 64);
    // The pinned pair above already implies the ordering the code ceiling
    // depends on: a code block crosses 64 KiB before the 256 KiB slice
    // budget, so the crossing reason is always `CarriedEmbedBytes`.
}

#[test]
fn plan_metrics_report_the_source_byte_count() {
    let markdown = "one paragraph\n";
    let plan = plan_markdown(markdown);
    assert_eq!(plan.metrics.source_bytes, markdown.len());

    let oversized = MAX_MARKDOWN_SOURCE_BYTES + 1;
    let plan = source_limited_markdown_plan(oversized);
    assert_eq!(plan.metrics.source_bytes, oversized);
    assert_eq!(plan.limit, Some(MarkdownPlanLimit::SourceBytes));
}

#[test]
fn a_block_exactly_at_the_slice_budgets_packs_as_one_batch() {
    let mut paragraph = String::from("**s**");
    for _ in 0..125 {
        paragraph.push_str(" `a`");
    }
    paragraph.push_str(" end\n");
    let plan = plan_markdown(&paragraph);
    assert_eq!(
        plan.metrics.events, MARKDOWN_EVENTS_PER_PROJECTION_SLICE,
        "the fixture must sit exactly on the event budget"
    );
    assert!(plan.is_complete());
    assert_eq!(plan.omissions(), 0);
    assert_eq!(plan.batches.len(), 1, "the budget itself must not split");
    assert_plan_invariants(&plan);

    let exact_bytes = "x".repeat(MARKDOWN_BYTES_PER_PROJECTION_SLICE);
    let plan = plan_markdown(&exact_bytes);
    assert_eq!(
        plan.metrics.retained_bytes, MARKDOWN_BYTES_PER_PROJECTION_SLICE,
        "the fixture must sit exactly on the byte budget"
    );
    assert!(plan.is_complete());
    assert_eq!(plan.omissions(), 0);
    assert_eq!(plan.batches.len(), 1);
    assert_plan_invariants(&plan);
}

#[test]
fn a_global_stop_after_a_crossed_block_leaves_no_uncounted_omission() {
    // The crossed table sits inside a list item, and the global event
    // budget fires before that list closes. The crossing's marker therefore
    // never reaches a batch, so it must not be counted either.
    let mut markdown = String::from("- outer item\n\n");
    for line in cell_table(PAST_CEILING_COLUMNS, PAST_CEILING_ROWS).lines() {
        markdown.push_str("  ");
        markdown.push_str(line);
        markdown.push('\n');
    }
    markdown.push('\n');
    while markdown.len() < 256 * 1024 {
        markdown.push_str("- filler item\n\n");
    }
    let plan = plan_markdown(&markdown);
    assert_eq!(plan.limit, Some(MarkdownPlanLimit::Events));
    assert_eq!(
        plan.batches.len(),
        0,
        "the enclosing list never closed, so no batch was emitted"
    );
    assert_eq!(
        plan.omissions(),
        0,
        "an omission no batch carries must not be counted"
    );
    assert_plan_invariants(&plan);
}

#[test]
fn omitted_units_still_charge_the_global_budgets() {
    // An omitted unit's events and retained bytes are charged exactly as
    // today, so a hostile document cannot bypass a global ceiling by making
    // every block indivisible.
    let dense = "z".repeat(MARKDOWN_BYTES_PER_PROJECTION_SLICE + 1);
    let plan = plan_markdown(&dense);
    assert_eq!(plan.omissions(), 1);
    assert_eq!(
        plan.projected_events(),
        0,
        "the omitted block retains nothing"
    );
    assert_eq!(plan.metrics.events, 3);
    assert!(
        plan.metrics.retained_bytes > MARKDOWN_BYTES_PER_PROJECTION_SLICE,
        "the omitted block's bytes are still charged"
    );

    // A document of indivisible blocks still reaches a global terminal
    // instead of planning forever.
    let block = (0..200).map(|_| "**x** ").collect::<String>();
    let mut markdown = String::new();
    while markdown.len() < 512 * 1024 {
        markdown.push_str(&block);
        markdown.push_str("\n\n");
    }
    let plan = plan_markdown(&markdown);
    assert_eq!(plan.limit, Some(MarkdownPlanLimit::Events));
    assert_eq!(plan.metrics.events, MAX_MARKDOWN_EVENTS);
    assert!(plan.omissions() > 0);
}

#[test]
fn retention_charge_arithmetic_is_pinned_for_a_retention_heavy_document() {
    // The retained-byte *terminal* is unreachable under the 4 MiB source
    // cap, so this pins the charge arithmetic instead of the terminal: even
    // the largest admissible source stays under the ceiling (hence
    // `limit == None`), and bytes are charged before any per-slice decision
    // so an omitted block cannot duck the ceiling on the way past.
    let plan = plan_markdown(&"w".repeat(MAX_MARKDOWN_SOURCE_BYTES));
    assert!(plan.metrics.retained_bytes >= MAX_MARKDOWN_SOURCE_BYTES);
    assert!(plan.metrics.retained_bytes <= MAX_MARKDOWN_RETAINED_BYTES);
    assert_eq!(plan.limit, None);
    assert_eq!(plan.omissions(), 1);
}

#[test]
fn omission_markers_do_not_consume_embed_descriptors() {
    let plan = plan_markdown(&list_with_one_overflowing_item_fixture());
    assert_eq!(plan.omissions(), 1);
    assert_eq!(plan.metrics.embed_descriptors, 0);

    let plan = plan_markdown(&table_past_the_cell_ceiling_fixture());
    assert_eq!(plan.omissions(), 1);
    assert_eq!(
        plan.metrics.embed_descriptors, 1,
        "only the table itself is a descriptor"
    );
}

#[test]
fn omission_marker_copy_names_the_crossed_budget() {
    fn copy(reason: MarkdownOmissionReason, scope: MarkdownOmissionScope) -> String {
        MarkdownBlockOmission {
            reason,
            scope,
            unretained: UnretainedEmbedCounts::default(),
        }
        .marker_text()
    }

    assert_eq!(
        copy(
            MarkdownOmissionReason::SliceEvents,
            MarkdownOmissionScope::TopLevelBlock
        ),
        "Markdown preview omitted one block that exceeds 256 render events"
    );
    assert_eq!(
        copy(
            MarkdownOmissionReason::SliceBytes,
            MarkdownOmissionScope::ContainerSegment
        ),
        "Markdown preview omitted part of one block that exceeds 256 KiB"
    );
    assert_eq!(
        copy(
            MarkdownOmissionReason::CarriedEmbedBytes,
            MarkdownOmissionScope::ContainerSegment
        ),
        "Markdown preview omitted part of one block after 64 KiB of carried content"
    );
    assert_eq!(
        copy(
            MarkdownOmissionReason::CarriedEmbedCells,
            MarkdownOmissionScope::ContainerSegment
        ),
        "Markdown preview omitted part of one block after 1024 carried table cells"
    );
}

/// Padding that lands a batch boundary exactly on `Start(CodeBlock)`.
///
/// The list is loose (the code-block item's blank line makes it so), so each
/// padding item is five events and the leading inline span shifts the whole
/// stream by one. Together they align the second boundary onto the code
/// block's own start event.
const CODE_BLOCK_BOUNDARY_PAD_ITEMS: usize = 100;
const CODE_BLOCK_BOUNDARY_LEAD_SPANS: usize = 1;

/// A list whose projection boundary falls between a *tiny* code block's
/// start and its text, because the enclosing list overflows the slice.
fn code_block_start_boundary_fixture() -> String {
    let mut markdown = String::from("# Boundary\n\n");
    markdown.push_str(&format!(
        "{}\n\n",
        "`a` ".repeat(CODE_BLOCK_BOUNDARY_LEAD_SPANS)
    ));
    for index in 0..CODE_BLOCK_BOUNDARY_PAD_ITEMS {
        writeln!(markdown, "- pad-{index}").expect("write padding item");
    }
    markdown.push_str("- item with code\n\n  ```sh\n  echo tiny\n  ```\n\n");
    markdown.push_str("- after-code\n\nTAIL-AFTER-BOUNDARY\n");
    markdown
}

#[test]
fn a_turn_boundary_can_fall_between_a_code_block_start_and_its_text() {
    // A carried code block is not only the large-embed case: a checkpoint is
    // admissible immediately after `Start(CodeBlock)`, so a projector that
    // rebuilt its in-flight block per turn would lose a *tiny* block whose
    // enclosing list overflowed.
    let plan = plan_markdown(&code_block_start_boundary_fixture());
    assert!(plan.is_complete());
    assert_eq!(plan.omissions(), 0);
    assert_plan_invariants(&plan);

    let boundary = plan
        .batches
        .iter()
        .position(|batch| {
            matches!(
                batch.events().last(),
                Some(MarkdownEvent::Start(MarkdownTag::CodeBlock(_)))
            )
        })
        .expect("a batch must end exactly at the code block's start");
    assert!(
        matches!(
            plan.batches[boundary].open_carry().containers(),
            [
                MarkdownOpenContainer::List { .. },
                MarkdownOpenContainer::Item,
                MarkdownOpenContainer::CodeBlock {
                    kind: MarkdownCodeBlockKind::Fenced { .. }
                },
            ]
        ),
        "the boundary must carry the open code block itself: {:?}",
        plan.batches[boundary].open_carry().containers()
    );

    // The turn that opened the block carried no code text at all, and the
    // next turn resumes into the same block.
    let next = plan
        .batches
        .get(boundary + 1)
        .expect("the code block continues in a later batch");
    assert_eq!(next.expected_carry(), plan.batches[boundary].open_carry());
    assert!(
        matches!(next.events().first(), Some(MarkdownEvent::Text(text)) if text.contains("echo tiny")),
        "the next turn must resume with the code text: {:?}",
        next.events().first()
    );
    let text = projected_text(&plan);
    assert!(text.contains("echo tiny"));
    assert!(text.contains("TAIL-AFTER-BOUNDARY"));
}

#[test]
fn only_slice_reasons_count_as_user_visible_omissions() {
    // A carried-embed crossing is a charge carrier: the projector's own
    // in-place fallback already replaces that block and names its true
    // size, so the plan must not report a user-visible omission for it.
    for markdown in [
        table_past_the_cell_ceiling_fixture(),
        format!(
            "```text\n{}\n```\n\ntail\n",
            "c".repeat(MAX_MARKDOWN_CARRIED_EMBED_BYTES + 4096)
        ),
    ] {
        let plan = plan_markdown(&markdown);
        assert_eq!(plan.omissions(), 1, "the crossing is still recorded");
        assert_eq!(
            plan.user_visible_omissions(),
            0,
            "a crossing must not be reported to the reader"
        );
    }

    // Both slice reasons are user visible.
    let plan = plan_markdown(&byte_budget_fixture());
    assert_eq!(plan.user_visible_omissions(), 1);
    let dense = (0..300).map(|_| "**x** ").collect::<String>();
    let plan = plan_markdown(&dense);
    assert_eq!(plan.user_visible_omissions(), 1);

    // A plan with no omissions reports none.
    let plan = plan_markdown(&oversized_list_fixture());
    assert_eq!(plan.user_visible_omissions(), 0);
}

#[test]
fn placeholder_widget_cap_bounds_top_level_omissions() {
    let dense = (0..100).map(|_| "**x** ").collect::<String>();
    let mut markdown = String::new();
    for _ in 0..(MAX_MARKDOWN_PLACEHOLDER_WIDGETS + 4) {
        markdown.push_str(&dense);
        markdown.push_str("\n\n");
    }
    let plan = plan_markdown(&markdown);
    assert!(plan.is_complete());
    assert_eq!(plan.omissions(), MAX_MARKDOWN_PLACEHOLDER_WIDGETS + 4);
    assert_eq!(plan.top_level_omissions(), plan.omissions());
    assert!(
        plan.top_level_omissions() > MAX_MARKDOWN_PLACEHOLDER_WIDGETS,
        "projection must be able to see that the widget cap is exceeded"
    );
}

#[test]
fn batch_carry_signatures_chain_across_a_sub_sliced_block() {
    let plan = plan_markdown(&helm_readme_fixture());
    assert_plan_invariants(&plan);
    let mid = plan
        .batches
        .iter()
        .find(|batch| !batch.open_carry().is_empty())
        .expect("a sub-sliced table leaves its table open");
    assert!(matches!(
        mid.open_carry().containers(),
        [MarkdownOpenContainer::Table { .. }]
    ));
}

#[test]
fn carry_signature_tracks_ordered_list_numbering() {
    let mut markdown = String::from("# Ordered\n\n");
    for index in 1..=120 {
        writeln!(markdown, "{index}. ordered-item-{index}").expect("write ordered item");
    }
    let plan = plan_markdown(&markdown);
    assert_plan_invariants(&plan);
    let open = plan
        .batches
        .iter()
        .find_map(|batch| match batch.open_carry().containers() {
            [
                MarkdownOpenContainer::List {
                    ordered,
                    next_number,
                },
            ] => Some((*ordered, *next_number)),
            _ => None,
        })
        .expect("an oversized ordered list is cut at an item boundary");
    assert!(open.0);
    assert!(open.1 > 1, "numbering continues across the cut");
}

#[test]
fn earlier_batches_survive_a_carried_embed_crossing() {
    let plan = plan_markdown(&table_past_the_cell_ceiling_fixture());
    let unchanged = plan_markdown(&table_past_the_cell_ceiling_fixture());
    assert_eq!(plan, unchanged, "planning stays deterministic");
    assert_plan_invariants(&plan);
    let crossing_batch = plan
        .batches
        .iter()
        .position(|batch| !batch.omissions().is_empty())
        .expect("the crossing emits a marker");
    assert!(
        plan.batches[..crossing_batch]
            .iter()
            .all(|batch| batch.omissions().is_empty()),
        "batches emitted before the crossing are never rewritten"
    );
}

#[test]
fn cancellation_before_planning_retains_no_partial_plan() {
    let cancel = AtomicBool::new(true);
    let markdown = helm_readme_fixture();
    assert!(plan_markdown_cancellable(&markdown, &cancel).is_none());
}

#[test]
fn cancellation_at_a_checkpoint_inside_a_sub_sliced_block_retains_no_plan() {
    let markdown = helm_readme_fixture();
    let uncancelled = plan_markdown(&markdown);
    assert!(uncancelled.batches.len() > 1, "the fixture is sub-sliced");
    // Land well inside the values table, past its first emitted batch.
    let inside_block = 320;
    assert!(inside_block < uncancelled.metrics.events);

    let cancel = AtomicBool::new(false);
    let _armed = CancelAfterEvents::arm(inside_block);
    let plan = plan_markdown_cancellable(&markdown, &cancel);
    assert!(
        plan.is_none(),
        "cancellation mid-block must retain no partial plan"
    );
    assert!(
        cancel.load(Ordering::Acquire),
        "the seam must have flipped the caller's token mid-parse"
    );
}

#[test]
fn ordinary_blocks_are_packed_without_splitting() {
    let mut markdown = String::new();
    for index in 0..400 {
        writeln!(markdown, "paragraph {index}\n").expect("write paragraph fixture");
    }
    let plan = plan_markdown(&markdown);
    assert!(plan.is_complete());
    assert!(plan.batches.len() > 1);
    assert_plan_invariants(&plan);
    assert_eq!(plan.projected_events(), plan.metrics.events);
}

#[test]
fn one_dense_block_is_omitted_and_planning_continues() {
    let dense = (0..300).map(|_| "**x** ").collect::<String>();
    let markdown = format!("{dense}\n\nTAIL-AFTER-DENSE\n");
    let plan = plan_markdown(&markdown);
    assert!(plan.is_complete());
    assert_eq!(plan.omissions(), 1);
    assert_plan_invariants(&plan);
    let omission = single_omission(&plan);
    assert_eq!(omission.reason, MarkdownOmissionReason::SliceEvents);
    assert_eq!(omission.scope, MarkdownOmissionScope::TopLevelBlock);
    assert!(projected_text(&plan).contains("TAIL-AFTER-DENSE"));
}

#[test]
fn image_flood_stops_at_descriptor_budget() {
    // Exactly the budget is admissible: the ceiling is a maximum, not the
    // first rejected value.
    let mut markdown = String::new();
    for index in 0..MAX_MARKDOWN_EMBED_DESCRIPTORS {
        writeln!(markdown, "![image](image-{index}.png)\n").expect("write image-flood fixture");
    }
    let plan = plan_markdown(&markdown);
    assert_eq!(plan.limit, None, "the budget itself must still complete");
    assert_eq!(
        plan.metrics.embed_descriptors,
        MAX_MARKDOWN_EMBED_DESCRIPTORS
    );

    let mut markdown = String::new();
    for index in 0..=MAX_MARKDOWN_EMBED_DESCRIPTORS {
        writeln!(markdown, "![image](image-{index}.png)\n").expect("write image-flood fixture");
    }
    let plan = plan_markdown(&markdown);
    assert_eq!(plan.limit, Some(MarkdownPlanLimit::EmbedDescriptors));
    assert_eq!(
        plan.metrics.embed_descriptors,
        MAX_MARKDOWN_EMBED_DESCRIPTORS
    );
}

#[test]
fn oversized_source_retains_no_events() {
    let markdown = "x".repeat(MAX_MARKDOWN_SOURCE_BYTES + 1);
    let plan = plan_markdown(&markdown);
    assert_eq!(plan.limit, Some(MarkdownPlanLimit::SourceBytes));
    assert_eq!(plan.metrics.events, 0);
    assert!(plan.batches.is_empty());
}

#[test]
fn one_large_text_block_is_omitted_and_planning_continues() {
    let markdown = format!(
        "{}\n\nTAIL-AFTER-LARGE\n",
        "x".repeat(MARKDOWN_BYTES_PER_PROJECTION_SLICE + 1)
    );
    let plan = plan_markdown(&markdown);
    assert!(plan.is_complete());
    assert_eq!(plan.omissions(), 1);
    assert_plan_invariants(&plan);
    assert_eq!(
        single_omission(&plan).reason,
        MarkdownOmissionReason::SliceBytes
    );
    assert!(projected_text(&plan).contains("TAIL-AFTER-LARGE"));
}

#[test]
fn cancelled_planner_retains_no_partial_plan() {
    let cancel = AtomicBool::new(true);
    assert!(plan_markdown_cancellable("paragraph", &cancel).is_none());
}

#[test]
fn stale_generation_cannot_replace_a_new_terminal() {
    let mut session = MarkdownRenderSession::default();
    let stale = session.begin();
    assert!(session.transition(stale, MarkdownRenderState::Projecting));
    let current = session.begin();
    assert!(!session.transition(stale, MarkdownRenderState::Complete));
    assert!(session.transition(current, MarkdownRenderState::Limited));
    assert_eq!(session.state(), MarkdownRenderState::Limited);
    assert!(!session.pending());
}

#[test]
fn cancellation_invalidates_pending_generation() {
    let mut session = MarkdownRenderSession::default();
    let stale = session.begin();
    let cancelled_generation = session.cancel();
    assert_ne!(stale, cancelled_generation);
    assert_eq!(session.state(), MarkdownRenderState::Cancelled);
    assert!(!session.pending());
    assert!(!session.transition(stale, MarkdownRenderState::Complete));
}

#[test]
fn image_admission_enforces_count_and_bytes_then_reuses_released_capacity() {
    let mut admission = MarkdownImageAdmission::default();
    for _ in 0..4 {
        assert!(admission.try_admit(100, 4, 400));
    }
    assert!(!admission.try_admit(1, 4, 400));
    assert_eq!(
        admission.snapshot(),
        MarkdownImageAdmissionSnapshot {
            owned_count: 4,
            owned_bytes: 400,
            high_water_count: 4,
            high_water_bytes: 400,
        }
    );
    admission.release(100);
    assert!(admission.try_admit(100, 4, 400));
    admission.release(u64::MAX);
    assert_eq!(admission.snapshot().owned_bytes, 0);
}
