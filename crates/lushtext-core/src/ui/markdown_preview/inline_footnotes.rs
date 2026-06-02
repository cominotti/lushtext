// SPDX-License-Identifier: GPL-3.0-or-later

//! Inline-footnote lowering for the GTK-native Markdown preview.
//!
//! This module stays GTK-free so the preview-specific Markdown preprocessing can
//! be unit-tested without constructing widgets. It belongs beside the renderer
//! because the generated labels and parser input are purely presentation details.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::collections::HashSet;
use std::ops::Range;

/// Prefix for labels generated from markdown-it-style inline footnotes.
///
/// The long app-specific prefix keeps generated labels away from ordinary user
/// labels, while collision checks still handle documents that intentionally use
/// the same internal-looking name.
const INLINE_FOOTNOTE_LABEL_PREFIX: &str = "__lush_inline_footnote_";

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

/// Lower markdown-it-style inline footnotes into parser-native footnote syntax.
///
/// The returned string is temporary preview input only; callers must continue to
/// save and edit the original Markdown source. `None` means no eligible inline
/// footnote was found, allowing the renderer to keep borrowing the original text.
pub(super) fn lower_inline_footnotes(markdown: &str, options: Options) -> Option<String> {
    if !markdown.contains("^[") {
        return None;
    }

    let scan_plan = build_scan_plan(markdown, options);
    if scan_plan.eligible_ranges.is_empty() {
        return None;
    }

    let mut replacements = Vec::new();
    let mut label_generator = InlineFootnoteLabelGenerator::new(scan_plan.used_labels);
    let protected_ranges = merge_ranges(scan_plan.protected_ranges);

    for range in scan_plan.eligible_ranges {
        collect_inline_footnote_replacements(
            markdown,
            range,
            &protected_ranges,
            &mut label_generator,
            &mut replacements,
        );
    }

    if replacements.is_empty() {
        return None;
    }

    Some(build_lowered_markdown(markdown, &replacements))
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
fn build_scan_plan(markdown: &str, options: Options) -> InlineFootnoteScanPlan {
    let mut plan = InlineFootnoteScanPlan::default();
    let mut protected_depth = 0usize;
    let mut current_prose_range: Option<Range<usize>> = None;

    for (event, range) in Parser::new_ext(markdown, options).into_offset_iter() {
        match &event {
            Event::Start(tag) => {
                if let Tag::FootnoteDefinition(label) = tag {
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

    plan
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
fn collect_inline_footnote_replacements(
    markdown: &str,
    range: Range<usize>,
    protected_ranges: &[Range<usize>],
    label_generator: &mut InlineFootnoteLabelGenerator,
    replacements: &mut Vec<InlineFootnoteReplacement>,
) {
    let bytes = markdown.as_bytes();
    let mut index = range.start;
    let mut protected_index = first_relevant_protected_range(protected_ranges, index);

    while index + 1 < range.end {
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

        if bytes[index] == b'^'
            && bytes.get(index + 1) == Some(&b'[')
            && !is_escaped(bytes, index)
            && let Some(close) =
                find_inline_footnote_close(markdown, index + 2, range.end, protected_ranges)
        {
            let body = markdown[index + 2..close].trim();
            if !body.is_empty() {
                replacements.push(InlineFootnoteReplacement {
                    source: index..close + 1,
                    label: label_generator.next_label(),
                    body: body.to_string(),
                });
            }
            index = close + 1;
            continue;
        }

        index += next_char_len(markdown, index);
    }
}

/// Find the first protected range that could affect scanning at `index`.
fn first_relevant_protected_range(protected_ranges: &[Range<usize>], index: usize) -> usize {
    protected_ranges
        .iter()
        .position(|range| range.end > index)
        .unwrap_or(protected_ranges.len())
}

/// Find the closing bracket for one inline footnote body.
fn find_inline_footnote_close(
    markdown: &str,
    start: usize,
    end: usize,
    protected_ranges: &[Range<usize>],
) -> Option<usize> {
    let bytes = markdown.as_bytes();
    let mut index = start;
    let mut bracket_depth = 0usize;
    let mut protected_index = first_relevant_protected_range(protected_ranges, index);

    while index < end {
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
            b']' if bracket_depth == 0 => return Some(index),
            b']' => {
                bracket_depth -= 1;
                index += 1;
            }
            _ => index += next_char_len(markdown, index),
        }
    }

    None
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
fn build_lowered_markdown(markdown: &str, replacements: &[InlineFootnoteReplacement]) -> String {
    let mut lowered = String::with_capacity(markdown.len() + replacements.len() * 48);
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

#[cfg(test)]
mod tests {
    use super::*;

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
        lower_inline_footnotes(markdown, test_options())
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
