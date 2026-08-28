// SPDX-License-Identifier: GPL-3.0-or-later

//! Inline-footnote lowering for the GTK-native Markdown preview.
//!
//! This module stays GTK-free so the preview-specific Markdown preprocessing can
//! be unit-tested without constructing widgets. It belongs beside the renderer
//! because the generated labels and parser input are purely presentation details.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::collections::HashSet;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::services::markdown_render::{
    MAX_MARKDOWN_EVENTS, MAX_MARKDOWN_RETAINED_BYTES, MAX_MARKDOWN_SOURCE_BYTES, MarkdownPlanLimit,
    MarkdownPlanMetrics, MarkdownRenderPlan,
};

/// Prefix for labels generated from markdown-it-style inline footnotes.
///
/// The long app-specific prefix keeps generated labels away from ordinary user
/// labels, while collision checks still handle documents that intentionally use
/// the same internal-looking name.
const INLINE_FOOTNOTE_LABEL_PREFIX: &str = "__lush_inline_footnote_";
const MAX_INLINE_FOOTNOTE_REPLACEMENTS: usize = MAX_MARKDOWN_EVENTS / 4;
const LOWERING_CANCEL_CHECK_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum InlineFootnoteLowering {
    Unchanged,
    Lowered(String),
    Limited,
    Cancelled,
}

#[derive(Debug, Clone, Copy)]
struct InlineFootnoteBudget {
    retained_bytes: usize,
    output_bytes: usize,
}

struct InlineFootnoteLimits {
    retention: InlineFootnoteBudget,
    close_scan_work_remaining: usize,
}

impl InlineFootnoteLimits {
    fn new(source_bytes: usize) -> Self {
        Self {
            retention: InlineFootnoteBudget::new(source_bytes),
            close_scan_work_remaining: source_bytes.saturating_mul(2),
        }
    }
}

impl InlineFootnoteBudget {
    fn new(source_bytes: usize) -> Self {
        Self {
            retained_bytes: 0,
            output_bytes: source_bytes.saturating_add(2),
        }
    }

    fn admit(&mut self, label_bytes: usize, body: &str) -> bool {
        let body_output_bytes = body
            .lines()
            .map(str::len)
            .sum::<usize>()
            .saturating_add(body.lines().count().saturating_sub(1).saturating_mul(5));
        let retained_charge = std::mem::size_of::<InlineFootnoteReplacement>()
            .saturating_add(label_bytes.saturating_mul(2))
            .saturating_add(body.len());
        let output_charge = label_bytes
            .saturating_mul(2)
            .saturating_add(body_output_bytes)
            .saturating_add(10);
        let next_retained = self.retained_bytes.saturating_add(retained_charge);
        let next_output = self.output_bytes.saturating_add(output_charge);
        if next_retained > MAX_MARKDOWN_RETAINED_BYTES || next_output > MAX_MARKDOWN_RETAINED_BYTES
        {
            return false;
        }
        self.retained_bytes = next_retained;
        self.output_bytes = next_output;
        true
    }
}

/// One source replacement generated from an inline footnote definition.
#[derive(Debug, Clone, PartialEq, Eq)]
struct InlineFootnoteReplacement {
    /// Source span covering the original `^[...]` marker and body.
    source: Range<usize>,
    /// Collision-free footnote label inserted into the temporary parser input.
    label: String,
    /// Raw Markdown body captured between the inline footnote delimiters.
    body: String,
}

/// Source ranges that can be scanned for inline footnote syntax.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct InlineFootnoteScanPlan {
    /// Paragraph and heading ranges where prose-level inline footnotes may occur.
    eligible_ranges: Vec<Range<usize>>,
    /// Parser-recognized regions that must remain byte-for-byte untouched.
    protected_ranges: Vec<Range<usize>>,
    /// Footnote labels already used by the source document.
    used_labels: HashSet<String>,
}

enum ScanPlanOutcome {
    Plan(InlineFootnoteScanPlan),
    Limited,
    Cancelled,
}

/// Lower markdown-it-style inline footnotes into parser-native footnote syntax.
///
/// The returned string is temporary preview input only; callers must continue to
/// save and edit the original Markdown source. `None` means no eligible inline
/// footnote was found, allowing the renderer to keep borrowing the original text.
pub(super) fn lower_inline_footnotes(markdown: &str, options: Options) -> InlineFootnoteLowering {
    lower_inline_footnotes_inner(markdown, options, None)
}

pub(super) fn lower_inline_footnotes_cancellable(
    markdown: &str,
    options: Options,
    cancel: &AtomicBool,
) -> InlineFootnoteLowering {
    lower_inline_footnotes_inner(markdown, options, Some(cancel))
}

fn lower_inline_footnotes_inner(
    markdown: &str,
    options: Options,
    cancel: Option<&AtomicBool>,
) -> InlineFootnoteLowering {
    if markdown.len() > MAX_MARKDOWN_SOURCE_BYTES {
        return InlineFootnoteLowering::Limited;
    }
    if !markdown.contains("^[") {
        return InlineFootnoteLowering::Unchanged;
    }

    let scan_plan = match build_scan_plan_cancellable(markdown, options, cancel) {
        ScanPlanOutcome::Plan(plan) => plan,
        ScanPlanOutcome::Limited => return InlineFootnoteLowering::Limited,
        ScanPlanOutcome::Cancelled => return InlineFootnoteLowering::Cancelled,
    };
    if scan_plan.eligible_ranges.is_empty() {
        return InlineFootnoteLowering::Unchanged;
    }

    let mut replacements = Vec::new();
    let mut label_generator = InlineFootnoteLabelGenerator::new(scan_plan.used_labels);
    let protected_ranges = merge_ranges(scan_plan.protected_ranges);
    let mut limits = InlineFootnoteLimits::new(markdown.len());

    for range in scan_plan.eligible_ranges {
        match collect_inline_footnote_replacements_bounded(
            markdown,
            range,
            &protected_ranges,
            &mut label_generator,
            &mut replacements,
            &mut limits,
            cancel,
        ) {
            LoweringScanControl::Continue => {}
            LoweringScanControl::Limited => return InlineFootnoteLowering::Limited,
            LoweringScanControl::Cancelled => return InlineFootnoteLowering::Cancelled,
        }
    }

    if replacements.is_empty() {
        return InlineFootnoteLowering::Unchanged;
    }

    InlineFootnoteLowering::Lowered(build_lowered_markdown(
        markdown,
        &replacements,
        limits.retention.output_bytes,
    ))
}

/// Collision-aware label generator for preview-only inline footnotes.
#[derive(Debug, Clone)]
struct InlineFootnoteLabelGenerator {
    /// Labels already present in the source or generated earlier in this pass.
    used: HashSet<String>,
    /// Next numeric suffix to try for the internal label prefix.
    next: usize,
}

impl InlineFootnoteLabelGenerator {
    /// Create a generator seeded with labels from reference-style footnotes.
    fn new(used: HashSet<String>) -> Self {
        Self { used, next: 1 }
    }

    /// Return the next unused internal label and reserve it immediately.
    fn next_label(&mut self) -> String {
        loop {
            let label = format!("{INLINE_FOOTNOTE_LABEL_PREFIX}{}", self.next);
            self.next += 1;
            if self.used.insert(label.clone()) {
                return label;
            }
        }
    }
}

/// Build one parser-derived scan plan for a source document.
#[cfg(test)]
fn build_scan_plan(markdown: &str, options: Options) -> InlineFootnoteScanPlan {
    match build_scan_plan_cancellable(markdown, options, None) {
        ScanPlanOutcome::Plan(plan) => plan,
        ScanPlanOutcome::Limited => panic!("test scan unexpectedly exceeded its budget"),
        ScanPlanOutcome::Cancelled => panic!("uncancelled inline-footnote scan cannot cancel"),
    }
}

fn build_scan_plan_cancellable(
    markdown: &str,
    options: Options,
    cancel: Option<&AtomicBool>,
) -> ScanPlanOutcome {
    let mut plan = InlineFootnoteScanPlan::default();
    let mut protected_depth = 0usize;
    let mut current_prose_range: Option<Range<usize>> = None;
    let mut used_label_bytes = 0usize;

    for (event_index, (event, range)) in Parser::new_ext(markdown, options)
        .into_offset_iter()
        .enumerate()
    {
        if event_index % 64 == 0 && cancelled(cancel) {
            return ScanPlanOutcome::Cancelled;
        }
        if event_index >= MAX_MARKDOWN_EVENTS {
            return ScanPlanOutcome::Limited;
        }
        match &event {
            Event::Start(tag) => {
                if let Tag::FootnoteDefinition(label) = tag {
                    used_label_bytes = used_label_bytes.saturating_add(label.len());
                    if used_label_bytes > MAX_MARKDOWN_RETAINED_BYTES {
                        return ScanPlanOutcome::Limited;
                    }
                    plan.used_labels.insert(label.to_string());
                }

                if is_protected_start(tag) {
                    protected_depth += 1;
                    push_non_empty_range(&mut plan.protected_ranges, range);
                } else if protected_depth > 0 {
                    push_non_empty_range(&mut plan.protected_ranges, range);
                } else if is_eligible_start(tag) {
                    current_prose_range = Some(range.clone());
                    push_non_empty_range(&mut plan.eligible_ranges, range);
                }
            }
            Event::End(tag_end) => {
                if protected_depth > 0 {
                    push_non_empty_range(&mut plan.protected_ranges, range);
                }
                if is_protected_end(*tag_end) {
                    protected_depth = protected_depth.saturating_sub(1);
                }
                if is_eligible_end(*tag_end) {
                    current_prose_range = None;
                }
            }
            Event::Code(_) | Event::Html(_) => {
                push_non_empty_range(&mut plan.protected_ranges, range);
            }
            Event::InlineHtml(_) => {
                // Inline HTML tags can wrap normal text events. Protecting the
                // surrounding prose span keeps raw HTML content out of this
                // Markdown extension instead of trying to parse HTML nesting.
                push_non_empty_range(
                    &mut plan.protected_ranges,
                    current_prose_range.clone().unwrap_or(range),
                );
            }
            Event::FootnoteReference(label) => {
                used_label_bytes = used_label_bytes.saturating_add(label.len());
                if used_label_bytes > MAX_MARKDOWN_RETAINED_BYTES {
                    return ScanPlanOutcome::Limited;
                }
                plan.used_labels.insert(label.to_string());
                if protected_depth > 0 {
                    push_non_empty_range(&mut plan.protected_ranges, range);
                }
            }
            _ if protected_depth > 0 => {
                push_non_empty_range(&mut plan.protected_ranges, range);
            }
            _ => {}
        }
    }

    ScanPlanOutcome::Plan(plan)
}

fn cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel.is_some_and(|cancel| cancel.load(Ordering::Acquire))
}

/// Return whether a tag opens a prose span where inline footnotes may appear.
fn is_eligible_start(tag: &Tag<'_>) -> bool {
    matches!(tag, Tag::Paragraph | Tag::Heading { .. })
}

/// Return whether an end tag closes an eligible prose span.
fn is_eligible_end(tag_end: TagEnd) -> bool {
    matches!(tag_end, TagEnd::Paragraph | TagEnd::Heading(_))
}

/// Return whether a tag opens a source range that must not be rewritten.
fn is_protected_start(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::Link { .. }
            | Tag::Image { .. }
            | Tag::Table(_)
            | Tag::MetadataBlock(_)
    )
}

/// Return whether an end tag closes a protected source range.
fn is_protected_end(tag_end: TagEnd) -> bool {
    matches!(
        tag_end,
        TagEnd::CodeBlock
            | TagEnd::HtmlBlock
            | TagEnd::Link
            | TagEnd::Image
            | TagEnd::Table
            | TagEnd::MetadataBlock(_)
    )
}

/// Store a range only when it can protect or scan at least one byte.
fn push_non_empty_range(ranges: &mut Vec<Range<usize>>, range: Range<usize>) {
    if range.start < range.end {
        ranges.push(range);
    }
}

/// Merge overlapping parser ranges so scans can skip protected regions cheaply.
fn merge_ranges(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::new();

    for range in ranges {
        if let Some(last) = merged.last_mut()
            && range.start <= last.end
        {
            last.end = last.end.max(range.end);
            continue;
        }
        merged.push(range);
    }

    merged
}

/// Scan one eligible source range and append every valid inline-footnote rewrite.
#[cfg(test)]
fn collect_inline_footnote_replacements(
    markdown: &str,
    range: Range<usize>,
    protected_ranges: &[Range<usize>],
    label_generator: &mut InlineFootnoteLabelGenerator,
    replacements: &mut Vec<InlineFootnoteReplacement>,
) {
    let mut limits = InlineFootnoteLimits::new(markdown.len());
    let result = collect_inline_footnote_replacements_bounded(
        markdown,
        range,
        protected_ranges,
        label_generator,
        replacements,
        &mut limits,
        None,
    );
    assert_eq!(result, LoweringScanControl::Continue);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LoweringScanControl {
    Continue,
    Limited,
    Cancelled,
}

fn collect_inline_footnote_replacements_bounded(
    markdown: &str,
    range: Range<usize>,
    protected_ranges: &[Range<usize>],
    label_generator: &mut InlineFootnoteLabelGenerator,
    replacements: &mut Vec<InlineFootnoteReplacement>,
    limits: &mut InlineFootnoteLimits,
    cancel: Option<&AtomicBool>,
) -> LoweringScanControl {
    let bytes = markdown.as_bytes();
    let mut index = range.start;
    let mut protected_index = first_relevant_protected_range(protected_ranges, index);
    let mut next_cancel_check = index.saturating_add(LOWERING_CANCEL_CHECK_BYTES);

    while index + 1 < range.end {
        if index >= next_cancel_check {
            if cancelled(cancel) {
                return LoweringScanControl::Cancelled;
            }
            next_cancel_check = index.saturating_add(LOWERING_CANCEL_CHECK_BYTES);
        }
        while protected_index < protected_ranges.len()
            && protected_ranges[protected_index].end <= index
        {
            protected_index += 1;
        }

        if protected_index < protected_ranges.len() {
            let protected = &protected_ranges[protected_index];
            if protected.start <= index && index < protected.end {
                index = protected.end.min(range.end);
                continue;
            }
        }

        if bytes[index] == b'^' && bytes.get(index + 1) == Some(&b'[') && !is_escaped(bytes, index)
        {
            let close = match find_inline_footnote_close_cancellable(
                markdown,
                index + 2,
                range.end,
                protected_ranges,
                &mut limits.close_scan_work_remaining,
                cancel,
            ) {
                Ok(Some(close)) => close,
                Ok(None) => {
                    index += next_char_len(markdown, index);
                    continue;
                }
                Err(control) => return control,
            };
            let body = markdown[index + 2..close].trim();
            if !body.is_empty() {
                if replacements.len() >= MAX_INLINE_FOOTNOTE_REPLACEMENTS {
                    return LoweringScanControl::Limited;
                }
                let label = label_generator.next_label();
                if !limits.retention.admit(label.len(), body) {
                    return LoweringScanControl::Limited;
                }
                replacements.push(InlineFootnoteReplacement {
                    source: index..close + 1,
                    label,
                    body: body.to_string(),
                });
            }
            index = close + 1;
            continue;
        }

        index += next_char_len(markdown, index);
    }
    if cancelled(cancel) {
        LoweringScanControl::Cancelled
    } else {
        LoweringScanControl::Continue
    }
}

/// Find the first protected range that could affect scanning at `index`.
fn first_relevant_protected_range(protected_ranges: &[Range<usize>], index: usize) -> usize {
    protected_ranges.partition_point(|range| range.end <= index)
}

/// Find the closing bracket for one inline footnote body.
#[cfg(test)]
fn find_inline_footnote_close(
    markdown: &str,
    start: usize,
    end: usize,
    protected_ranges: &[Range<usize>],
) -> Option<usize> {
    let mut close_scan_work_remaining = markdown.len().saturating_mul(2);
    find_inline_footnote_close_cancellable(
        markdown,
        start,
        end,
        protected_ranges,
        &mut close_scan_work_remaining,
        None,
    )
    .expect("uncancelled inline-footnote close scan cannot cancel")
}

fn find_inline_footnote_close_cancellable(
    markdown: &str,
    start: usize,
    end: usize,
    protected_ranges: &[Range<usize>],
    close_scan_work_remaining: &mut usize,
    cancel: Option<&AtomicBool>,
) -> Result<Option<usize>, LoweringScanControl> {
    let bytes = markdown.as_bytes();
    let mut index = start;
    let mut bracket_depth = 0usize;
    let mut protected_index = first_relevant_protected_range(protected_ranges, index);
    let mut next_cancel_check = index.saturating_add(LOWERING_CANCEL_CHECK_BYTES);

    while index < end {
        let Some(remaining) = close_scan_work_remaining.checked_sub(1) else {
            return Err(LoweringScanControl::Limited);
        };
        *close_scan_work_remaining = remaining;
        if index >= next_cancel_check {
            if cancelled(cancel) {
                return Err(LoweringScanControl::Cancelled);
            }
            next_cancel_check = index.saturating_add(LOWERING_CANCEL_CHECK_BYTES);
        }
        while protected_index < protected_ranges.len()
            && protected_ranges[protected_index].end <= index
        {
            protected_index += 1;
        }

        if protected_index < protected_ranges.len() {
            let protected = &protected_ranges[protected_index];
            if protected.start <= index && index < protected.end {
                index = protected.end.min(end);
                continue;
            }
        }

        match bytes[index] {
            b'\\' => {
                index += next_char_len(markdown, index);
                if index < end {
                    index += next_char_len(markdown, index);
                }
            }
            b'[' => {
                bracket_depth += 1;
                index += 1;
            }
            b']' if bracket_depth == 0 => return Ok(Some(index)),
            b']' => {
                bracket_depth -= 1;
                index += 1;
            }
            _ => index += next_char_len(markdown, index),
        }
    }

    if cancelled(cancel) {
        Err(LoweringScanControl::Cancelled)
    } else {
        Ok(None)
    }
}

/// Return true when an ASCII marker is escaped by an odd number of backslashes.
fn is_escaped(bytes: &[u8], index: usize) -> bool {
    let mut slash_count = 0usize;
    let mut cursor = index;

    while cursor > 0 && bytes[cursor - 1] == b'\\' {
        slash_count += 1;
        cursor -= 1;
    }

    slash_count % 2 == 1
}

/// Return the byte length of the UTF-8 character starting at `index`.
fn next_char_len(markdown: &str, index: usize) -> usize {
    markdown[index..].chars().next().map_or(1, char::len_utf8)
}

/// Build the lowered Markdown string consumed by the real preview parser.
fn build_lowered_markdown(
    markdown: &str,
    replacements: &[InlineFootnoteReplacement],
    output_capacity: usize,
) -> String {
    let mut lowered = String::with_capacity(output_capacity.min(MAX_MARKDOWN_RETAINED_BYTES));
    let mut previous = 0usize;

    for replacement in replacements {
        lowered.push_str(&markdown[previous..replacement.source.start]);
        lowered.push_str("[^");
        lowered.push_str(&replacement.label);
        lowered.push(']');
        previous = replacement.source.end;
    }
    lowered.push_str(&markdown[previous..]);

    if !lowered.ends_with('\n') {
        lowered.push('\n');
    }
    lowered.push('\n');

    for replacement in replacements {
        lowered.push_str("[^");
        lowered.push_str(&replacement.label);
        lowered.push_str("]: ");
        push_definition_body(&mut lowered, &replacement.body);
        lowered.push('\n');
    }

    lowered
}

/// Append a generated definition body with continuation lines indented.
fn push_definition_body(output: &mut String, body: &str) {
    for (index, line) in body.lines().enumerate() {
        if index > 0 {
            output.push('\n');
            output.push_str("    ");
        }
        output.push_str(line);
    }
}

/// The plan published when inline-footnote lowering hits its own limit.
///
/// Production policy with three production callers (`admission.rs` twice and
/// `planning_execution.rs` once). It reports the *source* byte count alongside
/// the `InlineFootnotes` limit so a reader of the published plan can tell a
/// limited render from an empty one.
pub(super) fn inline_footnote_limited_plan(source_bytes: usize) -> MarkdownRenderPlan {
    MarkdownRenderPlan {
        batches: Vec::new(),
        metrics: MarkdownPlanMetrics {
            source_bytes,
            ..MarkdownPlanMetrics::default()
        },
        limit: Some(MarkdownPlanLimit::InlineFootnotes),
    }
}

#[cfg(any(feature = "property-tests", feature = "fuzzing"))]
use crate::services::markdown_render::markdown_render_options;

// ─── Fuzzing and property-test entry points ───────────────────────────
//
// Pure wrappers over this module's lowering. They live here rather than in
// the facade because they are policy inputs with no widget, no stage, and no
// GTK dependency; the fuzz targets and the property suite are their only
// callers.
//
// Every item below is feature-gated, so the default mutation lane — which runs
// without `fuzzing` and without `property-tests`, deliberately — does not compile
// them and therefore cannot kill their mutants. See `docs/mutation-testing.md`.

/// Result of fuzzing Markdown preprocessing without constructing GTK widgets.
///
/// The fuzz target only needs to know that the preprocessing and parser setup
/// completed. Counts keep the helper useful for sanity checks without exposing
/// renderer internals as a stable public API.
#[cfg(feature = "fuzzing")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FuzzedMarkdownPreprocess {
    /// Number of pulldown-cmark events produced after preprocessing.
    pub parser_event_count: usize,
    /// Byte length of the Markdown text passed to the parser.
    pub parser_input_len: usize,
    /// Whether markdown-it-style inline footnotes were lowered first.
    pub lowered_inline_footnotes: bool,
}

/// Run the preview's real inline-footnote lowering for feature-gated generated tests.
///
/// Keeping this as a narrow feature-only hook lets generated tests and fuzzing
/// exercise the production lowering path without making the private scanner
/// part of the normal application API.
#[cfg(any(feature = "property-tests", feature = "fuzzing"))]
#[must_use]
fn lower_inline_footnotes_for_generated_test(markdown: &str) -> Option<String> {
    match lower_inline_footnotes(markdown, markdown_render_options()) {
        InlineFootnoteLowering::Lowered(lowered) => Some(lowered),
        InlineFootnoteLowering::Unchanged
        | InlineFootnoteLowering::Limited
        | InlineFootnoteLowering::Cancelled => None,
    }
}

/// Run the preview's real inline-footnote lowering for feature-gated property tests.
///
/// This preserves the original property-test API while sharing the same
/// generated-input hook used by fuzzing.
#[cfg(feature = "property-tests")]
#[must_use]
pub fn lower_inline_footnotes_for_property_test(markdown: &str) -> Option<String> {
    lower_inline_footnotes_for_generated_test(markdown)
}

/// Expose the preview's inline-footnote lowering result to fuzz harnesses.
///
/// The real preview plans the *lowered* text, not the raw source, so a fuzz
/// harness that plans raw input would cover shapes the app never plans.
#[cfg(feature = "fuzzing")]
#[must_use]
pub fn lowered_markdown_for_fuzzing(markdown: &str) -> Option<String> {
    lower_inline_footnotes_for_generated_test(markdown)
}

/// Exercise Markdown preprocessing and parser setup for fuzz targets.
///
/// The helper stops before renderer code that touches `GtkTextBuffer`,
/// `LushtextMarkdownPreview`, links, images, GSettings, or other GTK state.
#[cfg(feature = "fuzzing")]
#[must_use]
pub fn preprocess_markdown_for_fuzzing(markdown: &str) -> FuzzedMarkdownPreprocess {
    let lowered = lower_inline_footnotes_for_generated_test(markdown);
    let lowered_inline_footnotes = lowered.is_some();
    let parser_input = lowered.as_deref().unwrap_or(markdown);
    let options = markdown_render_options();
    let parser_event_count = Parser::new_ext(parser_input, options).count();

    FuzzedMarkdownPreprocess {
        parser_event_count,
        parser_input_len: parser_input.len(),
        lowered_inline_footnotes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_inline_footnote_limited_plan_reports_the_source_size_it_refused() {
        // This function exists to publish the *source* byte count next to the
        // limit, so a reader of the plan can distinguish "we refused 1.2 MB of
        // footnotes" from "there was nothing to render". Nothing asserted that
        // until mutation testing deleted the field and no test noticed.
        let plan = inline_footnote_limited_plan(1_234_567);
        assert_eq!(plan.metrics.source_bytes, 1_234_567);
        assert_eq!(plan.limit, Some(MarkdownPlanLimit::InlineFootnotes));
        assert!(plan.batches.is_empty(), "a limited plan renders no batches");

        // Zero is a real input (an empty source can still trip the limit path),
        // and it must stay distinguishable from the limit being absent.
        let empty = inline_footnote_limited_plan(0);
        assert_eq!(empty.metrics.source_bytes, 0);
        assert_eq!(empty.limit, Some(MarkdownPlanLimit::InlineFootnotes));
    }

    fn test_options() -> Options {
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_TASKLISTS);
        options.insert(Options::ENABLE_FOOTNOTES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_GFM);
        options
    }

    fn lower(markdown: &str) -> Option<String> {
        match lower_inline_footnotes(markdown, test_options()) {
            InlineFootnoteLowering::Lowered(lowered) => Some(lowered),
            InlineFootnoteLowering::Unchanged => None,
            unexpected => panic!("unexpected inline-footnote terminal: {unexpected:?}"),
        }
    }

    #[test]
    fn dense_inline_footnotes_stop_at_the_replacement_budget() {
        let markdown = "x^[a] ".repeat(MAX_INLINE_FOOTNOTE_REPLACEMENTS + 1);

        assert_eq!(
            lower_inline_footnotes(&markdown, test_options()),
            InlineFootnoteLowering::Limited
        );
    }

    #[test]
    fn cancellation_stops_dense_inline_footnote_scanning() {
        let markdown = "x^[a] ".repeat(10_000);
        let cancel = AtomicBool::new(true);

        assert_eq!(
            lower_inline_footnotes_cancellable(&markdown, test_options(), &cancel),
            InlineFootnoteLowering::Cancelled
        );
    }

    #[test]
    fn dense_unclosed_inline_footnotes_stop_at_the_scan_work_budget() {
        let markdown = "^[".repeat(10_000);

        assert_eq!(
            lower_inline_footnotes(&markdown, test_options()),
            InlineFootnoteLowering::Limited
        );
    }

    #[test]
    fn lower_simple_inline_footnote() {
        let lowered = lower("Body text^[Inline note].").expect("inline footnote should lower");

        assert!(lowered.contains("Body text[^__lush_inline_footnote_1]."));
        assert!(lowered.contains("[^__lush_inline_footnote_1]: Inline note"));
    }

    #[test]
    fn lower_uses_exact_blank_line_between_source_and_generated_definitions() {
        let lowered = lower("Body^[Inline note].\n").expect("inline footnote should lower exactly");

        assert_eq!(
            lowered,
            "Body[^__lush_inline_footnote_1].\n\n[^__lush_inline_footnote_1]: Inline note\n"
        );
    }

    #[test]
    fn lower_preserves_supported_inline_formatting_in_definition_body() {
        let lowered = lower("Body^[Inline **bold** and `code`].")
            .expect("formatted inline footnote should lower");

        assert!(lowered.contains("[^__lush_inline_footnote_1]: Inline **bold** and `code`"));
    }

    #[test]
    fn lower_captures_link_markup_inside_inline_footnote_body() {
        let lowered = lower("Body^[See [docs](https://example.com)].")
            .expect("inline footnote with link should lower");

        assert!(lowered.contains("[^__lush_inline_footnote_1]: See [docs](https://example.com)"));
    }

    #[test]
    fn lower_keeps_reference_labels_and_avoids_generated_label_collision() {
        let lowered = lower(
            "Body^[Inline note]. Existing[^__lush_inline_footnote_1]\n\n[^__lush_inline_footnote_1]: Existing note",
        )
        .expect("inline footnote should lower with collision-free label");

        assert!(lowered.contains("Body[^__lush_inline_footnote_2]."));
        assert!(lowered.contains("[^__lush_inline_footnote_2]: Inline note"));
    }

    #[test]
    fn lower_does_not_expand_recursive_inline_footnotes_inside_body() {
        let lowered =
            lower("Body^[Outer ^[inner] note].").expect("outer inline footnote should lower");

        assert!(lowered.contains("[^__lush_inline_footnote_1]: Outer ^[inner] note"));
        assert!(!lowered.contains("[^__lush_inline_footnote_2]:"));
    }

    #[test]
    fn lower_rejects_escaped_inline_footnote_syntax() {
        assert_eq!(lower(r"\^[Not a footnote]"), None);
    }

    #[test]
    fn lower_accepts_even_backslashes_before_inline_footnote_marker() {
        let lowered =
            lower(r"Body \\^[A real note].").expect("even backslashes should not escape marker");

        assert!(lowered.contains(r"Body \\[^__lush_inline_footnote_1]."));
        assert!(lowered.contains("[^__lush_inline_footnote_1]: A real note"));
    }

    #[test]
    fn lower_ignores_inline_footnote_syntax_inside_inline_code() {
        assert_eq!(lower("Use `^[Not a footnote]` here."), None);
    }

    #[test]
    fn lower_ignores_inline_footnote_syntax_inside_fenced_code() {
        assert_eq!(lower("```\n^[Not a footnote]\n```"), None);
    }

    #[test]
    fn lower_ignores_inline_footnote_syntax_inside_indented_code() {
        assert_eq!(lower("    ^[Not a footnote]\n"), None);
    }

    #[test]
    fn lower_ignores_inline_footnote_syntax_inside_raw_html() {
        assert_eq!(lower("<span>^[Not a footnote]</span>"), None);
    }

    #[test]
    fn lower_ignores_inline_footnote_syntax_inside_links_images_and_tables() {
        let markdown = "[Link ^[No]](https://example.com) ![Image ^[No]](image.png)\n\n| h |\n|---|\n| ^[No] |\n\nOutside^[Yes].";
        let lowered = lower(markdown).expect("outside inline footnote should lower");

        assert!(lowered.contains("[Link ^[No]](https://example.com)"));
        assert!(lowered.contains("![Image ^[No]](image.png)"));
        assert!(lowered.contains("| ^[No] |"));
        assert!(lowered.contains("Outside[^__lush_inline_footnote_1]."));
        assert!(lowered.contains("[^__lush_inline_footnote_1]: Yes"));
    }

    #[test]
    fn lower_ignores_malformed_inline_footnote_delimiter() {
        assert_eq!(lower("Body^[No close."), None);
    }

    #[test]
    fn lower_ignores_empty_inline_footnote_body() {
        assert_eq!(lower("Body^[] text"), None);
        assert_eq!(lower("Body^[   ] text"), None);
    }

    #[test]
    fn lower_handles_nested_brackets_inside_body() {
        let lowered = lower("Body^[Inline [bracketed] note].")
            .expect("inline footnote with nested brackets should lower");

        assert!(lowered.contains("[^__lush_inline_footnote_1]: Inline [bracketed] note"));
    }

    #[test]
    fn lower_handles_escaped_brackets_and_code_brackets_inside_body() {
        let lowered = lower(r"Body^[Escaped \] bracket and `]` code].")
            .expect("inline footnote with escaped bracket should lower");

        assert!(lowered.contains(r"[^__lush_inline_footnote_1]: Escaped \] bracket and `]` code"));
    }

    #[test]
    fn lower_handles_adjacent_multibyte_inline_footnotes() {
        let lowered = lower("Café^[é note]中^[漢字 note].")
            .expect("adjacent multibyte inline footnotes should lower");

        assert!(lowered.contains("Café[^__lush_inline_footnote_1]中[^__lush_inline_footnote_2]."));
        assert!(lowered.contains("[^__lush_inline_footnote_1]: é note"));
        assert!(lowered.contains("[^__lush_inline_footnote_2]: 漢字 note"));
    }

    #[test]
    fn lower_indents_multiline_definition_bodies() {
        let lowered = lower("Body^[first line\nsecond line].")
            .expect("multiline inline footnote should lower");

        assert!(lowered.contains("[^__lush_inline_footnote_1]: first line\n    second line"));
    }

    #[test]
    fn range_helpers_ignore_empty_ranges_and_merge_touching_ranges() {
        let mut ranges = Vec::new();
        push_non_empty_range(&mut ranges, 2..2);
        push_non_empty_range(&mut ranges, 2..4);
        assert_eq!(ranges, vec![2..4]);

        let merged = merge_ranges(vec![8..10, 1..3, 3..6, 5..8, 12..13]);
        assert_eq!(merged, vec![1..10, 12..13]);
    }

    #[test]
    fn protected_range_lookup_skips_ranges_that_end_at_index() {
        let ranges = vec![0..2, 3..5, 8..10];

        assert_eq!(first_relevant_protected_range(&ranges, 0), 0);
        assert_eq!(first_relevant_protected_range(&ranges, 2), 1);
        assert_eq!(first_relevant_protected_range(&ranges, 5), 2);
        assert_eq!(first_relevant_protected_range(&ranges, 10), ranges.len());
    }

    #[test]
    fn tag_classifiers_accept_only_prose_and_protected_containers() {
        assert!(is_eligible_start(&Tag::Paragraph));
        assert!(!is_eligible_start(&Tag::HtmlBlock));
        assert!(is_eligible_end(TagEnd::Paragraph));
        assert!(!is_eligible_end(TagEnd::Link));

        assert!(is_protected_start(&Tag::HtmlBlock));
        assert!(!is_protected_start(&Tag::Paragraph));
        assert!(is_protected_end(TagEnd::Link));
        assert!(!is_protected_end(TagEnd::Paragraph));
    }

    fn protected_snippets<'a>(markdown: &'a str, plan: &InlineFootnoteScanPlan) -> Vec<&'a str> {
        plan.protected_ranges
            .iter()
            .map(|range| &markdown[range.clone()])
            .collect()
    }

    #[test]
    fn parser_scan_plan_tracks_reference_labels_and_protected_ranges() {
        let markdown = "Text[^alpha] and <span>protected</span>.\n\n```rust\ncode\n```\n\nBody^[note].\n\n[^alpha]: Existing definition";
        let plan = build_scan_plan(markdown, test_options());

        assert!(plan.used_labels.contains("alpha"));
        assert!(
            plan.eligible_ranges
                .iter()
                .any(|range| markdown[range.clone()].contains("Body^[note]"))
        );
        assert!(
            plan.protected_ranges
                .iter()
                .any(|range| markdown[range.clone()].contains("<span>protected</span>"))
        );
        assert!(
            plan.protected_ranges
                .iter()
                .any(|range| markdown[range.clone()].contains("code"))
        );
    }

    #[test]
    fn parser_scan_plan_records_nested_ranges_inside_protected_spans() {
        let markdown = "[Link *em ^[No]*](https://example.com) outside^[Yes].";
        let plan = build_scan_plan(markdown, test_options());
        let snippets = protected_snippets(markdown, &plan);

        assert!(snippets.contains(&"[Link *em ^[No]*](https://example.com)"));
        assert!(snippets.contains(&"em ^"));
        assert!(
            snippets
                .iter()
                .filter(|snippet| **snippet == "*em ^[No]*")
                .count()
                >= 2
        );
    }

    #[test]
    fn parser_scan_plan_keeps_unprotected_footnote_references_scannable() {
        let markdown = "Text[^alpha] outside^[Yes].\n\n[^alpha]: Existing definition";
        let plan = build_scan_plan(markdown, test_options());
        let snippets = protected_snippets(markdown, &plan);

        assert!(plan.used_labels.contains("alpha"));
        assert!(!snippets.contains(&"[^alpha]"));
    }

    #[test]
    fn parser_scan_plan_protects_footnote_references_inside_tables() {
        let markdown = "| h |\n|---|\n| [^alpha] |\n\n[^alpha]: Existing definition";
        let plan = build_scan_plan(markdown, test_options());
        let snippets = protected_snippets(markdown, &plan);

        assert!(plan.used_labels.contains("alpha"));
        assert!(snippets.contains(&"[^alpha]"));
    }

    #[test]
    fn replacement_scan_skips_multiple_protected_ranges() {
        let markdown = "ok ^[one] {first ^[skip]} mid {second ^[skip]}^[two] tail ^[out]";
        let first_start = markdown.find("{first").expect("first protected range");
        let first_end = first_start + "{first ^[skip]}".len();
        let second_start = markdown.find("{second").expect("second protected range");
        let second_end = second_start + "{second ^[skip]}".len();
        let scan_start = markdown.find("^[one]").expect("first inline footnote");
        let scan_end = markdown
            .find(" tail")
            .expect("scan end before final marker");
        let protected_ranges = vec![first_start..first_end, second_start..second_end];
        let mut label_generator =
            InlineFootnoteLabelGenerator::new(std::collections::HashSet::new());
        let mut replacements = Vec::new();

        collect_inline_footnote_replacements(
            markdown,
            scan_start..scan_end,
            &protected_ranges,
            &mut label_generator,
            &mut replacements,
        );

        assert_eq!(
            replacements,
            vec![
                InlineFootnoteReplacement {
                    source: scan_start..scan_start + "^[one]".len(),
                    label: "__lush_inline_footnote_1".to_string(),
                    body: "one".to_string(),
                },
                InlineFootnoteReplacement {
                    source: second_end..second_end + "^[two]".len(),
                    label: "__lush_inline_footnote_2".to_string(),
                    body: "two".to_string(),
                },
            ]
        );
    }

    #[test]
    fn replacement_scan_rejects_protected_marker_with_external_close() {
        let markdown = "ok {protected ^[skip}].";
        let protected_start = markdown.find("{protected").expect("protected start");
        let protected_end = protected_start + "{protected ^[skip}".len();
        let protected_range = protected_start..protected_end;
        let mut label_generator =
            InlineFootnoteLabelGenerator::new(std::collections::HashSet::new());
        let mut replacements = Vec::new();

        collect_inline_footnote_replacements(
            markdown,
            0..markdown.len(),
            std::slice::from_ref(&protected_range),
            &mut label_generator,
            &mut replacements,
        );

        assert!(replacements.is_empty());
    }

    #[test]
    fn replacement_scan_rechecks_later_protected_ranges_after_an_expired_one() {
        let markdown = "ok {first} mid {second ^[skip}].";
        let first_start = markdown.find("{first}").expect("first protected start");
        let first_end = first_start + "{first}".len();
        let second_start = markdown.find("{second").expect("second protected start");
        let second_end = second_start + "{second ^[skip}".len();
        let protected_ranges = vec![first_start..first_end, second_start..second_end];
        let mut label_generator =
            InlineFootnoteLabelGenerator::new(std::collections::HashSet::new());
        let mut replacements = Vec::new();

        collect_inline_footnote_replacements(
            markdown,
            0..markdown.len(),
            &protected_ranges,
            &mut label_generator,
            &mut replacements,
        );

        assert!(replacements.is_empty());
    }

    #[test]
    fn close_scanner_ignores_protected_brackets_inside_body() {
        let markdown = "^[body [protected ]] after]";
        let protected_start = markdown.find("[protected").expect("protected start");
        let protected_end = protected_start + "[protected ]]".len();
        let protected_range = protected_start..protected_end;
        let close = find_inline_footnote_close(
            markdown,
            2,
            markdown.len(),
            std::slice::from_ref(&protected_range),
        );

        assert_eq!(close, markdown.rfind(']'));
    }

    #[test]
    fn close_scanner_advances_between_multiple_protected_ranges() {
        let markdown = "^[body [first ] gap [second ]] after]";
        let first_start = markdown.find("[first").expect("first protected start");
        let first_end = first_start + "[first ]".len();
        let second_start = markdown.find("[second").expect("second protected start");
        let second_end = second_start + "[second ]]".len();
        let close = find_inline_footnote_close(
            markdown,
            2,
            markdown.len(),
            &[first_start..first_end, second_start..second_end],
        );

        assert_eq!(close, markdown.rfind(']'));
    }

    #[test]
    fn close_scanner_does_not_skip_early_close_before_later_protected_range() {
        let markdown = "^[early] [protected]";
        let protected_start = markdown.find("[protected]").expect("protected start");
        let protected_end = protected_start + "[protected]".len();
        let protected_range = protected_start..protected_end;
        let close = find_inline_footnote_close(
            markdown,
            2,
            markdown.len(),
            std::slice::from_ref(&protected_range),
        );

        assert_eq!(close, markdown.find(']'));
    }
}
