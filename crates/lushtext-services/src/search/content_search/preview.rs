// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded replacement-preview generation over workspace search matches.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use lushtext_model::workspace::content_search::{
    ContentSearchOptions, ReplacePreviewBudget, ReplacePreviewIneligibleFile, ReplacePreviewLimit,
    ReplacePreviewOutcome, ReplacePreviewSkipCounts, ReplacePreviewSkipReason, ReplaceSkipReason,
    Replacement, SearchMatch,
};

/// Generate replacement previews from search matches.
///
/// Service function — no I/O, no GTK. For each `SearchMatch`, produces a
/// `Replacement` with the original line, the replaced line (for preview display),
/// and the replacement text for the matched range.
///
/// - **Literal mode** (`options.regex == false`): direct string replacement at `match_range`.
/// - **Regex mode** (`options.regex == true`): compiles the query, expands backreferences
///   (`$1`, `$2`, etc.) via `regex::Captures::expand()`.
#[must_use]
pub fn generate_replacement_preview(
    matches: &[SearchMatch],
    query: &str,
    replacement_template: &str,
    options: &ContentSearchOptions,
) -> ReplacePreviewOutcome {
    generate_replacement_preview_with_budget(
        matches,
        query,
        replacement_template,
        options,
        ReplacePreviewBudget::default(),
    )
}

/// Generate a replacement preview using an explicit resource budget.
#[must_use]
pub fn generate_replacement_preview_with_budget(
    matches: &[SearchMatch],
    query: &str,
    replacement_template: &str,
    options: &ContentSearchOptions,
    budget: ReplacePreviewBudget,
) -> ReplacePreviewOutcome {
    generate_replacement_preview_impl(
        matches,
        query,
        replacement_template,
        options,
        budget,
        || false,
    )
}

/// Generate a bounded preview that stops between rows when its owner is superseded.
///
/// A cancelled call returns the bounded partial work completed so far. The owning
/// generation guard must discard that partial outcome rather than presenting it.
#[must_use]
pub fn generate_replacement_preview_with_budget_and_cancel(
    matches: &[SearchMatch],
    query: &str,
    replacement_template: &str,
    options: &ContentSearchOptions,
    budget: ReplacePreviewBudget,
    is_cancelled: impl Fn() -> bool,
) -> ReplacePreviewOutcome {
    generate_replacement_preview_impl(
        matches,
        query,
        replacement_template,
        options,
        budget,
        is_cancelled,
    )
}

/// Count one unconfirmable match and fold it into its file's skip entry.
fn record_ineligible<'m>(
    outcome: &mut ReplacePreviewOutcome,
    index: &mut HashMap<&'m Path, usize>,
    search_match: &'m SearchMatch,
    reason: ReplacePreviewSkipReason,
) {
    outcome.skipped.increment(reason);
    let skip_reason = match reason {
        ReplacePreviewSkipReason::TruncatedSource => ReplaceSkipReason::LineTooLong,
        ReplacePreviewSkipReason::RegexRangeMismatch => ReplaceSkipReason::InvalidPreview,
    };
    let path: &Path = &search_match.path;
    if let Some(&slot) = index.get(path) {
        let file = &mut outcome.ineligible_files[slot];
        file.matches = file.matches.saturating_add(1);
        file.first_line = file.first_line.min(search_match.line_number);
        file.reason = file.reason.min(skip_reason);
    } else {
        index.insert(path, outcome.ineligible_files.len());
        outcome.ineligible_files.push(ReplacePreviewIneligibleFile {
            path: search_match.path.to_path_buf(),
            reason: skip_reason,
            matches: 1,
            first_line: search_match.line_number,
        });
    }
}

fn generate_replacement_preview_impl(
    matches: &[SearchMatch],
    query: &str,
    replacement_template: &str,
    options: &ContentSearchOptions,
    budget: ReplacePreviewBudget,
    is_cancelled: impl Fn() -> bool,
) -> ReplacePreviewOutcome {
    // Pre-compile regex once if in regex mode. If compilation fails,
    // fall back to literal replacement (the search already validated the pattern,
    // but belt-and-suspenders).
    let compiled_regex = if options.regex {
        let mut builder = regex::RegexBuilder::new(query);
        builder.case_insensitive(!options.case_sensitive);
        builder.build().ok()
    } else {
        None
    };

    let mut literal_replacement: Option<Arc<str>> = None;
    let mut outcome = ReplacePreviewOutcome {
        replacements: Vec::with_capacity(matches.len().min(budget.max_rows)),
        match_to_preview: vec![None; matches.len()],
        omitted_eligible: 0,
        skipped: ReplacePreviewSkipCounts::default(),
        charged_bytes: 0,
        limiting_reason: None,
        ineligible_files: Vec::new(),
    };
    let mut ineligible_index: HashMap<&Path, usize> = HashMap::new();
    let mut budget_exhausted = false;
    let mut literal_charged = false;

    for m in matches {
        if is_cancelled() {
            break;
        }
        if m.line_truncated {
            record_ineligible(
                &mut outcome,
                &mut ineligible_index,
                m,
                ReplacePreviewSkipReason::TruncatedSource,
            );
            continue;
        }
        let original_line = m.line_content.clone();
        let start = original_line.floor_char_boundary(m.match_range.start.min(original_line.len()));
        let end = original_line.ceil_char_boundary(m.match_range.end.min(original_line.len()));

        if budget_exhausted {
            if compiled_regex
                .as_ref()
                .is_some_and(|regex| regex.captures(&original_line[start..end]).is_none())
            {
                record_ineligible(
                    &mut outcome,
                    &mut ineligible_index,
                    m,
                    ReplacePreviewSkipReason::RegexRangeMismatch,
                );
            } else {
                outcome.omitted_eligible = outcome.omitted_eligible.saturating_add(1);
            }
            continue;
        }

        if outcome.replacements.len() >= budget.max_rows {
            outcome.limiting_reason = Some(ReplacePreviewLimit::Rows);
            outcome.omitted_eligible = outcome.omitted_eligible.saturating_add(1);
            budget_exhausted = true;
            continue;
        }

        let replacement_text = if let Some(ref re) = compiled_regex {
            // Regex mode: find the match within the line at the known range
            // and expand backreferences.
            if let Some(cap) = re.captures(&original_line[start..end]) {
                if is_cancelled() {
                    break;
                }
                let expansion_upper_bound =
                    regex_expansion_upper_bound(replacement_template, end.saturating_sub(start));
                let conservative_replaced_len = original_line
                    .len()
                    .saturating_sub(end.saturating_sub(start))
                    .saturating_add(expansion_upper_bound);
                let conservative_next_bytes = saturating_preview_bytes(
                    outcome.charged_bytes,
                    [
                        original_line.len(),
                        conservative_replaced_len,
                        m.path.as_os_str().as_encoded_bytes().len(),
                        expansion_upper_bound,
                    ],
                );
                if conservative_next_bytes > budget.max_bytes {
                    outcome.limiting_reason = Some(ReplacePreviewLimit::Bytes);
                    outcome.omitted_eligible = outcome.omitted_eligible.saturating_add(1);
                    budget_exhausted = true;
                    continue;
                }
                let mut expanded = String::new();
                cap.expand(replacement_template, &mut expanded);
                Arc::<str>::from(expanded)
            } else {
                // Regex didn't match the extracted range — skip this match rather
                // than inserting unexpanded backreference syntax ($1/$2) literally.
                record_ineligible(
                    &mut outcome,
                    &mut ineligible_index,
                    m,
                    ReplacePreviewSkipReason::RegexRangeMismatch,
                );
                continue;
            }
        } else {
            literal_replacement
                .get_or_insert_with(|| Arc::from(replacement_template))
                .clone()
        };

        let replacement_charge = if options.regex || !literal_charged {
            replacement_text.len()
        } else {
            0
        };
        let replaced_line_len = original_line
            .len()
            .saturating_sub(end.saturating_sub(start))
            .saturating_add(replacement_text.len());
        let next_bytes = saturating_preview_bytes(
            outcome.charged_bytes,
            [
                original_line.len(),
                replaced_line_len,
                m.path.as_os_str().as_encoded_bytes().len(),
                replacement_charge,
            ],
        );
        let limiting_reason = if next_bytes > budget.max_bytes {
            Some(ReplacePreviewLimit::Bytes)
        } else {
            None
        };
        if let Some(reason) = limiting_reason {
            outcome.limiting_reason = Some(reason);
            outcome.omitted_eligible = outcome.omitted_eligible.saturating_add(1);
            budget_exhausted = true;
            continue;
        }

        outcome.charged_bytes = next_bytes;
        literal_charged |= !options.regex;
        let mut replaced_line = original_line.to_string();
        replaced_line.replace_range(start..end, &replacement_text);
        let preview_index = outcome.replacements.len();
        if let Some(slot) = outcome.match_to_preview.get_mut(m.id.index()) {
            *slot = Some(preview_index);
        }
        outcome.replacements.push(Replacement {
            match_id: m.id,
            path: m.path.to_path_buf(),
            line_number: m.line_number,
            original_line,
            replaced_line,
            replacement: replacement_text,
            match_range: m.match_range.clone(),
        });
    }

    outcome
}

fn regex_expansion_upper_bound(template: &str, matched_bytes: usize) -> usize {
    let capture_markers = template.bytes().filter(|byte| *byte == b'$').count();
    template
        .len()
        .saturating_add(capture_markers.saturating_mul(matched_bytes))
}

fn saturating_preview_bytes(current: usize, components: impl IntoIterator<Item = usize>) -> usize {
    components.into_iter().fold(current, usize::saturating_add)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fmt;
    use std::ops::Range;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use lushtext_model::workspace::content_search::{
        MAX_REPLACE_PREVIEW_BYTES, MAX_REPLACE_PREVIEW_ROWS, MAX_SEARCH_MATCH_LINE_BYTES,
        SearchMatchId,
    };
    use tracing::field::{Field, Visit};
    use tracing::span::{Attributes, Id, Record};
    use tracing::{Event, Metadata, Subscriber};

    use super::*;

    struct CapturingSubscriber {
        output: Arc<Mutex<String>>,
    }

    impl Subscriber for CapturingSubscriber {
        fn enabled(&self, metadata: &Metadata<'_>) -> bool {
            matches!(
                *metadata.level(),
                tracing::Level::ERROR | tracing::Level::WARN
            )
        }

        fn new_span(&self, _span: &Attributes<'_>) -> Id {
            Id::from_u64(1)
        }

        fn record(&self, _span: &Id, _values: &Record<'_>) {}

        fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

        fn event(&self, event: &Event<'_>) {
            let mut visitor = CapturingVisitor::default();
            event.record(&mut visitor);
            let mut output = self.output.lock().expect("capture lock");
            output.push_str(&visitor.output);
            output.push('\n');
        }

        fn enter(&self, _span: &Id) {}

        fn exit(&self, _span: &Id) {}
    }

    #[derive(Default)]
    struct CapturingVisitor {
        output: String,
    }

    impl Visit for CapturingVisitor {
        fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
            use fmt::Write as _;
            let _ = write!(self.output, "{}={value:?};", field.name());
        }
    }

    fn match_in_line(line: &str, range: Range<usize>) -> SearchMatch {
        SearchMatch::new(PathBuf::from("/tmp/file.rs"), 7, line, range)
    }

    #[test]
    fn search_match_bounds_long_lines_around_match_and_skips_replace_preview() {
        let long_line = format!(
            "{}needle{}",
            "a".repeat(MAX_SEARCH_MATCH_LINE_BYTES * 2),
            "b".repeat(MAX_SEARCH_MATCH_LINE_BYTES * 2)
        );
        let match_start = MAX_SEARCH_MATCH_LINE_BYTES * 2;
        let search_match = match_in_line(&long_line, match_start..match_start + "needle".len());

        assert!(search_match.line_truncated);
        assert_eq!(search_match.original_line_byte_len, long_line.len());
        assert!(search_match.line_content.len() <= MAX_SEARCH_MATCH_LINE_BYTES);
        assert!(search_match.line_content.contains("needle"));
        assert_eq!(
            &search_match.line_content[search_match.match_range.clone()],
            "needle"
        );

        let previews = generate_replacement_preview(
            &[search_match],
            "needle",
            "thread",
            &ContentSearchOptions::default(),
        );
        assert!(previews.is_empty());
    }

    #[test]
    fn preview_partition_consumes_only_checked_stable_identities() {
        let matches = vec![
            match_in_line("hello one", 0..5).with_id(SearchMatchId::from_index(0)),
            match_in_line("hello two", 0..5).with_id(SearchMatchId::from_index(1)),
            match_in_line("hello three", 0..5).with_id(SearchMatchId::from_index(2)),
        ];
        let outcome = generate_replacement_preview(
            &matches,
            "hello",
            "goodbye",
            &ContentSearchOptions::default(),
        );
        let checked = HashSet::from([SearchMatchId::from_index(0), SearchMatchId::from_index(2)]);

        let selected = outcome.into_checked_replacements(&checked);

        assert_eq!(
            selected
                .iter()
                .map(|replacement| replacement.match_id)
                .collect::<Vec<_>>(),
            vec![SearchMatchId::from_index(0), SearchMatchId::from_index(2)]
        );
    }

    #[test]
    fn generate_replacement_preview_literal_preserves_match_metadata() {
        let matches = vec![match_in_line("hello world", 6..11)];
        let previews = generate_replacement_preview(
            &matches,
            "world",
            "Rust",
            &ContentSearchOptions::default(),
        );

        assert_eq!(previews.len(), 1);
        assert_eq!(previews[0].path, PathBuf::from("/tmp/file.rs"));
        assert_eq!(previews[0].line_number, 7);
        assert_eq!(previews[0].original_line.as_ref(), "hello world");
        assert_eq!(previews[0].replaced_line, "hello Rust");
        assert_eq!(previews[0].replacement.as_ref(), "Rust");
        assert_eq!(previews[0].match_range, 6..11);
    }

    #[test]
    fn generate_replacement_preview_regex_expands_backreferences() {
        let options = ContentSearchOptions::new(false, true, false, true, None);
        let matches = vec![match_in_line("name: Ada", 6..9)];
        let previews = generate_replacement_preview(&matches, "([a-z]+)", "<$1>", &options);

        assert_eq!(previews.len(), 1);
        assert_eq!(previews[0].replaced_line, "name: <Ada>");
        assert_eq!(previews[0].replacement.as_ref(), "<Ada>");
    }

    #[test]
    fn replacement_preview_zero_budget_omits_complete_eligible_rows() {
        let matches = vec![match_in_line("hello world", 6..11)];
        let outcome = generate_replacement_preview_with_budget(
            &matches,
            "world",
            "Rust",
            &ContentSearchOptions::default(),
            ReplacePreviewBudget {
                max_rows: 0,
                max_bytes: 0,
            },
        );

        assert!(outcome.is_empty());
        assert_eq!(outcome.omitted_eligible, 1);
        assert_eq!(outcome.limiting_reason, Some(ReplacePreviewLimit::Rows));
    }

    #[test]
    fn replacement_preview_row_budget_admits_exact_limit_and_omits_one_over() {
        let matches: Vec<_> = (0..3)
            .map(|index| {
                match_in_line("hello world", 6..11).with_id(SearchMatchId::from_index(index))
            })
            .collect();
        let outcome = generate_replacement_preview_with_budget(
            &matches,
            "world",
            "Rust",
            &ContentSearchOptions::default(),
            ReplacePreviewBudget {
                max_rows: 2,
                max_bytes: usize::MAX,
            },
        );

        assert_eq!(outcome.len(), 2);
        assert_eq!(outcome.omitted_eligible, 1);
        assert_eq!(outcome.limiting_reason, Some(ReplacePreviewLimit::Rows));
        assert_eq!(outcome.preview_index(SearchMatchId::from_index(0)), Some(0));
        assert_eq!(outcome.preview_index(SearchMatchId::from_index(1)), Some(1));
        assert_eq!(outcome.preview_index(SearchMatchId::from_index(2)), None);
    }

    #[test]
    fn replacement_preview_byte_budget_admits_exact_limit_and_rejects_one_less() {
        let matches = vec![match_in_line("hello world", 6..11)];
        let unrestricted = generate_replacement_preview_with_budget(
            &matches,
            "world",
            "Rust",
            &ContentSearchOptions::default(),
            ReplacePreviewBudget {
                max_rows: 1,
                max_bytes: usize::MAX,
            },
        );
        let exact = generate_replacement_preview_with_budget(
            &matches,
            "world",
            "Rust",
            &ContentSearchOptions::default(),
            ReplacePreviewBudget {
                max_rows: 1,
                max_bytes: unrestricted.charged_bytes,
            },
        );
        let one_less = generate_replacement_preview_with_budget(
            &matches,
            "world",
            "Rust",
            &ContentSearchOptions::default(),
            ReplacePreviewBudget {
                max_rows: 1,
                max_bytes: unrestricted.charged_bytes - 1,
            },
        );

        assert_eq!(exact.len(), 1);
        assert_eq!(exact.charged_bytes, unrestricted.charged_bytes);
        assert!(one_less.is_empty());
        assert_eq!(one_less.limiting_reason, Some(ReplacePreviewLimit::Bytes));
    }

    #[test]
    fn replacement_preview_shares_original_and_literal_but_not_regex_expansions() {
        let literal_matches: Vec<_> = (0..2)
            .map(|index| {
                match_in_line("hello world", 6..11).with_id(SearchMatchId::from_index(index))
            })
            .collect();
        let literal = generate_replacement_preview(
            &literal_matches,
            "world",
            "Rust",
            &ContentSearchOptions::default(),
        );
        assert!(Arc::ptr_eq(
            &literal_matches[0].line_content,
            &literal[0].original_line
        ));
        assert!(Arc::ptr_eq(
            &literal[0].replacement,
            &literal[1].replacement
        ));

        let regex_matches = vec![
            match_in_line("name: Ada", 6..9).with_id(SearchMatchId::from_index(0)),
            match_in_line("name: Bob", 6..9).with_id(SearchMatchId::from_index(1)),
        ];
        let regex = generate_replacement_preview(
            &regex_matches,
            "([a-z]+)",
            "<$1>",
            &ContentSearchOptions::new(false, true, false, true, None),
        );
        assert_eq!(regex[0].replacement.as_ref(), "<Ada>");
        assert_eq!(regex[1].replacement.as_ref(), "<Bob>");
        assert!(!Arc::ptr_eq(&regex[0].replacement, &regex[1].replacement));
    }

    #[test]
    fn replacement_preview_counts_truncated_invalid_large_and_unicode_inputs() {
        let long_line = format!("{}needle{}", "a".repeat(5000), "b".repeat(5000));
        let mut truncated = match_in_line(&long_line, 5000..5006);
        truncated.id = SearchMatchId::from_index(0);
        let invalid = match_in_line("name: 123", 6..9).with_id(SearchMatchId::from_index(1));
        let unicode = match_in_line("olá 🌍", 4..8).with_id(SearchMatchId::from_index(2));
        let outcome = generate_replacement_preview_with_budget(
            &[truncated, invalid, unicode],
            "([a-z]+)",
            &"界".repeat(1024),
            &ContentSearchOptions::new(false, true, false, true, None),
            ReplacePreviewBudget {
                max_rows: 10,
                max_bytes: 128,
            },
        );

        assert_eq!(
            outcome
                .skipped
                .count(ReplacePreviewSkipReason::TruncatedSource),
            1
        );
        assert_eq!(
            outcome
                .skipped
                .count(ReplacePreviewSkipReason::RegexRangeMismatch),
            2
        );
        assert_eq!(outcome.skipped_source_count(), 3);
        assert!(outcome.is_empty());
    }

    #[test]
    fn invalid_preview_diagnostics_and_typed_outcome_exclude_private_sentinels() {
        const SOURCE_SENTINEL: &str = "PRIVATE-SOURCE-7d13f09c";
        const REPLACEMENT_SENTINEL: &str = "PRIVATE-REPLACEMENT-2b64a811";
        let output = Arc::new(Mutex::new(String::new()));
        let dispatch = tracing::Dispatch::new(CapturingSubscriber {
            output: Arc::clone(&output),
        });
        let invalid = match_in_line(SOURCE_SENTINEL, 0..SOURCE_SENTINEL.len());

        let outcome = tracing::dispatcher::with_default(&dispatch, || {
            generate_replacement_preview(
                &[invalid],
                "^(definitely-no-match)$",
                REPLACEMENT_SENTINEL,
                &ContentSearchOptions::new(true, true, false, true, None),
            )
        });

        assert!(outcome.replacements.is_empty());
        assert_eq!(
            outcome
                .skipped
                .count(ReplacePreviewSkipReason::RegexRangeMismatch),
            1
        );
        let captured = output.lock().expect("capture lock").clone();
        let typed = format!("{outcome:?}");
        for private in [SOURCE_SENTINEL, REPLACEMENT_SENTINEL] {
            assert!(
                !captured.contains(private),
                "captured diagnostics leaked {private}"
            );
            assert!(
                !typed.contains(private),
                "typed invalid outcome leaked {private}"
            );
        }
    }

    #[test]
    fn replacement_preview_accounting_saturates() {
        assert_eq!(
            saturating_preview_bytes(usize::MAX - 1, [1, 1, usize::MAX]),
            usize::MAX
        );
    }

    #[test]
    fn replacement_preview_rejects_amplifying_regex_before_expansion() {
        let matched = "a".repeat(MAX_SEARCH_MATCH_LINE_BYTES - 1);
        let search_match = match_in_line(&matched, 0..matched.len());
        let template = "$1".repeat(20_000);
        let outcome = generate_replacement_preview_with_budget(
            &[search_match],
            "(a+)",
            &template,
            &ContentSearchOptions::new(true, true, false, true, None),
            ReplacePreviewBudget::default(),
        );

        assert!(outcome.is_empty());
        assert_eq!(outcome.omitted_eligible, 1);
        assert_eq!(outcome.charged_bytes, 0);
        assert_eq!(outcome.limiting_reason, Some(ReplacePreviewLimit::Bytes));
    }

    #[test]
    fn replacement_preview_large_literal_is_byte_limited_without_partial_row() {
        let matches = vec![match_in_line("needle", 0..6)];
        let replacement = "界".repeat(1024);
        let outcome = generate_replacement_preview_with_budget(
            &matches,
            "needle",
            &replacement,
            &ContentSearchOptions::default(),
            ReplacePreviewBudget {
                max_rows: 1,
                max_bytes: replacement.len() - 1,
            },
        );

        assert!(outcome.is_empty());
        assert_eq!(outcome.omitted_eligible, 1);
        assert_eq!(outcome.charged_bytes, 0);
        assert_eq!(outcome.limiting_reason, Some(ReplacePreviewLimit::Bytes));
    }

    /// No loss against `main` (`power-of-two-thresholds` D12): `main`'s Replace
    /// Preview kept 10,000 rows, so 10,000 eligible matches give 10,000 rows;
    /// the presentation cap rounds up to 16,384.
    #[test]
    fn no_loss_replace_preview_keeps_every_row_main_kept() {
        let rows = |count: usize| {
            let matches: Vec<_> = (0..count)
                .map(|index| {
                    match_in_line("prefix needle suffix", 7..13)
                        .with_id(SearchMatchId::from_index(index))
                })
                .collect();
            let outcome = generate_replacement_preview(
                &matches,
                "needle",
                "thread",
                &ContentSearchOptions::default(),
            );
            (outcome.len(), outcome.omitted_eligible)
        };
        assert_eq!(rows(10_000), (10_000, 0));
        assert_eq!(rows(16 * 1024), (16 * 1024, 0));
        assert_eq!(rows(16 * 1024 + 1), (16 * 1024, 1));
    }

    #[test]
    fn replacement_preview_ten_thousand_dense_ids_have_constant_shape_lookup() {
        let matches: Vec<_> = (0..MAX_REPLACE_PREVIEW_ROWS)
            .map(|index| {
                match_in_line("prefix needle suffix", 7..13)
                    .with_id(SearchMatchId::from_index(index))
            })
            .collect();
        let outcome = generate_replacement_preview(
            &matches,
            "needle",
            "thread",
            &ContentSearchOptions::default(),
        );

        assert_eq!(outcome.len(), MAX_REPLACE_PREVIEW_ROWS);
        assert!(outcome.charged_bytes <= MAX_REPLACE_PREVIEW_BYTES);
        assert_eq!(outcome.match_to_preview.len(), MAX_REPLACE_PREVIEW_ROWS);
        for index in 0..MAX_REPLACE_PREVIEW_ROWS {
            assert_eq!(
                outcome.preview_index(SearchMatchId::from_index(index)),
                Some(index)
            );
        }
    }
}
