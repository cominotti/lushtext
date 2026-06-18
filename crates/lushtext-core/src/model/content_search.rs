// SPDX-License-Identifier: GPL-3.0-or-later

//! Content search domain types — pure Rust, no GTK dependencies.
//!
//! Used by the search service, UI search panel, tests, and benchmarks.

use std::ops::Range;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Maximum UTF-8 bytes retained for one matching search line.
///
/// Workspace search can keep up to 10,000 matches in memory. Four KiB per
/// match keeps worst-case retained line text in the tens of MiB rather than
/// allowing minified or generated files to retain multi-GiB result sets.
pub const MAX_SEARCH_MATCH_LINE_BYTES: usize = 4 * 1024;
/// ASCII marker added when a search-result line is shortened.
const SEARCH_MATCH_TRUNCATION_MARKER: &str = " [truncated]";

/// A single match within a file.
#[derive(Debug, Clone)]
pub struct SearchMatch {
    /// Absolute path to the file containing the match.
    pub path: PathBuf,
    /// 1-based line number of the match.
    pub line_number: u64,
    /// Bounded text of the matching line (no trailing newline).
    ///
    /// For long lines this is a match-containing excerpt, not the full source
    /// line. `line_truncated` tells Replace All preview to avoid using it as a
    /// correctness snapshot.
    pub line_content: String,
    /// Byte range within the stored `line_content` that matched the query.
    pub match_range: Range<usize>,
    /// Whether `line_content` was shortened from the source line.
    pub line_truncated: bool,
    /// Original source line byte length before bounding.
    pub original_line_byte_len: usize,
}

impl SearchMatch {
    /// Build one bounded search match from a full source line.
    #[must_use]
    pub fn new(
        path: PathBuf,
        line_number: u64,
        line_content: &str,
        match_range: Range<usize>,
    ) -> Self {
        let original_line_byte_len = line_content.len();
        let (line_content, match_range, line_truncated) =
            bounded_match_line(line_content, match_range);
        Self {
            path,
            line_number,
            line_content,
            match_range,
            line_truncated,
            original_line_byte_len,
        }
    }
}

fn bounded_match_line(
    line_content: &str,
    match_range: Range<usize>,
) -> (String, Range<usize>, bool) {
    let raw_match_start = match_range.start.min(line_content.len());
    let raw_match_end = match_range
        .end
        .max(match_range.start)
        .min(line_content.len());
    if line_content.len() <= MAX_SEARCH_MATCH_LINE_BYTES {
        return (
            line_content.to_string(),
            line_content.floor_char_boundary(raw_match_start)
                ..line_content.ceil_char_boundary(raw_match_end),
            false,
        );
    }

    let marker_budget = SEARCH_MATCH_TRUNCATION_MARKER.len() * 2;
    let excerpt_budget = MAX_SEARCH_MATCH_LINE_BYTES.saturating_sub(marker_budget);
    let match_start = line_content.floor_char_boundary(raw_match_start);
    let match_end = line_content.ceil_char_boundary(raw_match_end);
    let match_len = match_end.saturating_sub(match_start);
    let prefix_budget = excerpt_budget.saturating_sub(match_len) / 2;
    let raw_start = match_start.saturating_sub(prefix_budget);
    let excerpt_start = line_content.ceil_char_boundary(raw_start);
    let raw_end = excerpt_start
        .saturating_add(excerpt_budget)
        .min(line_content.len());
    let excerpt_end = line_content.floor_char_boundary(raw_end);
    let has_prefix = excerpt_start > 0;
    let has_suffix = excerpt_end < line_content.len();

    let mut bounded = String::with_capacity(MAX_SEARCH_MATCH_LINE_BYTES);
    if has_prefix {
        bounded.push_str(SEARCH_MATCH_TRUNCATION_MARKER);
    }
    bounded.push_str(&line_content[excerpt_start..excerpt_end]);
    if has_suffix {
        bounded.push_str(SEARCH_MATCH_TRUNCATION_MARKER);
    }

    let marker_offset = if has_prefix {
        SEARCH_MATCH_TRUNCATION_MARKER.len()
    } else {
        0
    };
    let adjusted_start = marker_offset + match_start.saturating_sub(excerpt_start);
    let adjusted_end = marker_offset + match_end.min(excerpt_end).saturating_sub(excerpt_start);
    (bounded, adjusted_start..adjusted_end, true)
}

/// Options controlling search behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentSearchOptions {
    /// When true, matching is case-sensitive. Default: false.
    pub case_sensitive: bool,
    /// When true, the query is interpreted as a regex. Default: false.
    pub regex: bool,
    /// When true, only whole-word matches are returned. Default: false.
    pub whole_word: bool,
    /// When true, `.gitignore` rules are respected. Default: true.
    pub gitignore: bool,
    /// Optional glob filter — only files matching this pattern are searched.
    pub glob: Option<String>,
}

impl Default for ContentSearchOptions {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            regex: false,
            whole_word: false,
            gitignore: true,
            glob: None,
        }
    }
}

impl ContentSearchOptions {
    /// Build one search-options value object from the current toggle state.
    #[must_use]
    pub fn new(
        case_sensitive: bool,
        regex: bool,
        whole_word: bool,
        gitignore: bool,
        glob: Option<String>,
    ) -> Self {
        Self {
            case_sensitive,
            regex,
            whole_word,
            gitignore,
            glob,
        }
    }

    /// Build the compact toggle summary used by history and saved-search rows.
    #[must_use]
    pub fn toggle_summary(&self) -> String {
        let mut parts = Vec::new();
        if self.case_sensitive {
            parts.push("Aa".to_string());
        }
        if self.regex {
            parts.push(".*".to_string());
        }
        if self.whole_word {
            parts.push("W".to_string());
        }
        if !self.gitignore {
            parts.push("no .gitignore".to_string());
        }
        if let Some(glob) = self.glob.as_deref()
            && !glob.is_empty()
        {
            parts.push(glob.to_string());
        }
        parts.join("  ")
    }
}

/// Shared search query state used across runtime search, history, and saved searches.
///
/// This stays in the domain layer so GTK adapters can pass around one value
/// object instead of rebuilding the same query-plus-toggle shape in multiple
/// widget methods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchQuerySpec {
    /// Free-form search text entered by the user.
    pub query: String,
    /// Toggle and filter state that changes how the query is interpreted.
    #[serde(flatten)]
    pub options: ContentSearchOptions,
}

impl SearchQuerySpec {
    /// Build a query spec from the current query text and resolved options.
    #[must_use]
    pub fn new(query: impl Into<String>, options: ContentSearchOptions) -> Self {
        Self {
            query: query.into(),
            options,
        }
    }

    /// Truncate the query for compact list-row display without losing Unicode boundaries.
    #[must_use]
    pub fn display_query(&self, max_chars: usize) -> String {
        if self.query.len() > max_chars {
            format!(
                "{}…",
                &self.query[..self.query.floor_char_boundary(max_chars)]
            )
        } else {
            self.query.clone()
        }
    }
}

/// Events sent through the channel during search.
#[derive(Debug)]
pub enum SearchEvent {
    /// A matching line was found.
    Match(SearchMatch),
    /// The 10,000-match result cap was reached; search stopped early.
    ResultCap,
    /// Progress update: number of files visited so far. Sent approximately
    /// every 100 files to avoid flooding the channel. Best-effort — may be
    /// skipped if the channel is full.
    Progress(usize),
    /// An error occurred (e.g., invalid regex).
    Error(String),
    /// Search completed (always the last event sent).
    Done,
}

/// A replacement instruction for Replace All, including preview data.
#[derive(Debug, Clone)]
pub struct Replacement {
    /// Absolute path to the file containing the match.
    pub path: PathBuf,
    /// 1-based line number.
    pub line_number: u64,
    /// The original full line content (before replacement).
    pub original_line: String,
    /// The full line content after applying the replacement (for preview).
    pub replaced_line: String,
    /// The literal replacement text for the matched range.
    pub replacement: String,
    /// Byte range within `original_line` that matched the query.
    pub match_range: Range<usize>,
}

/// Result of a Replace All operation.
#[derive(Debug)]
pub struct ReplaceResult {
    pub replaced_count: usize,
    pub files_affected: usize,
    /// Paths of files that were skipped (e.g., open with unsaved modifications).
    pub skipped_paths: Vec<PathBuf>,
    /// Per-file errors encountered during replace (non-fatal — other files still processed).
    pub errors: Vec<String>,
}

/// Generate replacement previews from search matches.
///
/// Pure function — no I/O, no GTK. For each `SearchMatch`, produces a `Replacement`
/// with the original line, the replaced line (for preview display), and the literal
/// replacement text for the matched range.
///
/// - **Literal mode** (`options.regex == false`): direct string replacement at `match_range`.
/// - **Regex mode** (`options.regex == true`): re-compiles the query, expands backreferences
///   (`$1`, `$2`, etc.) via `regex::Regex::replace()`.
#[must_use]
pub fn generate_replacement_preview(
    matches: &[SearchMatch],
    query: &str,
    replacement_template: &str,
    options: &ContentSearchOptions,
) -> Vec<Replacement> {
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

    matches
        .iter()
        .filter_map(|m| {
            if m.line_truncated {
                return None;
            }
            let original_line = m.line_content.clone();
            let start =
                original_line.floor_char_boundary(m.match_range.start.min(original_line.len()));
            let end = original_line.ceil_char_boundary(m.match_range.end.min(original_line.len()));

            let (replaced_line, replacement_text) = if let Some(ref re) = compiled_regex {
                // Regex mode: find the match within the line at the known range
                // and expand backreferences.
                if let Some(cap) = re.captures(&original_line[start..end]) {
                    let mut expanded = String::new();
                    cap.expand(replacement_template, &mut expanded);
                    let mut line = original_line.clone();
                    line.replace_range(start..end, &expanded);
                    (line, expanded)
                } else {
                    // Regex didn't match the extracted range — skip this match rather
                    // than inserting unexpanded backreference syntax ($1/$2) literally.
                    tracing::warn!(
                        "Regex did not match extracted range for line {}: {:?}",
                        m.line_number,
                        &original_line[start..end],
                    );
                    (original_line.clone(), original_line[start..end].to_string())
                }
            } else {
                // Literal mode: direct string replacement.
                let mut line = original_line.clone();
                line.replace_range(start..end, replacement_template);
                (line, replacement_template.to_string())
            };

            Some(Replacement {
                path: m.path.clone(),
                line_number: m.line_number,
                original_line,
                replaced_line,
                replacement: replacement_text,
                match_range: m.match_range.clone(),
            })
        })
        .collect()
}

/// A single entry in the search history, capturing query text and all toggle
/// states at the time of search. Persisted to `search-history.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHistoryEntry {
    /// Flattened so persisted JSON keeps the existing `query` + toggle fields.
    #[serde(flatten)]
    pub spec: SearchQuerySpec,
}

impl SearchHistoryEntry {
    /// Convert a persisted history record into the shared query-spec shape.
    #[must_use]
    pub fn query_spec(&self) -> SearchQuerySpec {
        self.spec.clone()
    }

    /// Create a history record from the shared query-spec value object.
    #[must_use]
    pub fn from_spec(spec: SearchQuerySpec) -> Self {
        Self { spec }
    }

    /// Build the compact subtitle shown for a recent-search row.
    #[must_use]
    pub fn toggle_summary(&self) -> String {
        self.spec.options.toggle_summary()
    }

    /// Truncated query text suitable for recent-search rows.
    #[must_use]
    pub fn display_query(&self, max_chars: usize) -> String {
        self.spec.display_query(max_chars)
    }
}

/// A named saved search, persisted permanently to `saved-searches.json`.
///
/// Unlike `SearchHistoryEntry` (capped at 20, auto-managed), saved searches
/// are user-created and persist until explicitly deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedSearch {
    pub name: String,
    /// Flattened so saved-search JSON remains compatible with older builds.
    #[serde(flatten)]
    pub spec: SearchQuerySpec,
}

impl SavedSearch {
    /// Convert a saved search into the shared runtime query-spec shape.
    #[must_use]
    pub fn query_spec(&self) -> SearchQuerySpec {
        self.spec.clone()
    }

    /// Build a named saved search from the shared query-spec value object.
    #[must_use]
    pub fn from_spec(name: impl Into<String>, spec: SearchQuerySpec) -> Self {
        Self {
            name: name.into(),
            spec,
        }
    }

    /// Build the saved-search subtitle: compact query plus toggle state when needed.
    #[must_use]
    pub fn row_subtitle(&self) -> String {
        let toggles = self.spec.options.toggle_summary();
        let query = self.spec.display_query(40);
        if toggles.is_empty() {
            query
        } else {
            format!("{query}  {toggles}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn match_in_line(line: &str, range: Range<usize>) -> SearchMatch {
        SearchMatch::new(PathBuf::from("/tmp/file.rs"), 7, line, range)
    }

    #[test]
    fn toggle_summary_lists_enabled_options_in_display_order() {
        let options =
            ContentSearchOptions::new(true, true, true, false, Some("src/**/*.rs".to_string()));

        assert_eq!(
            options.toggle_summary(),
            "Aa  .*  W  no .gitignore  src/**/*.rs"
        );
    }

    #[test]
    fn toggle_summary_omits_disabled_and_empty_values() {
        assert_eq!(ContentSearchOptions::default().toggle_summary(), "");

        let options = ContentSearchOptions::new(false, false, false, true, Some(String::new()));
        assert_eq!(options.toggle_summary(), "");
    }

    #[test]
    fn display_query_truncates_at_unicode_boundary_only_when_needed() {
        let spec = SearchQuerySpec::new("abcdéfg", ContentSearchOptions::default());

        assert_eq!(spec.display_query(4), "abcd…");
        assert_eq!(spec.display_query(5), "abcd…");
        assert_eq!(spec.display_query(6), "abcdé…");
        assert_eq!(spec.display_query(8), "abcdéfg");
    }

    #[test]
    fn search_match_line_budget_stays_at_four_kib() {
        assert_eq!(MAX_SEARCH_MATCH_LINE_BYTES, 4096);
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
    fn search_match_bounds_keep_boundary_markers_only_when_content_was_omitted() {
        let long_suffix = format!("needle{}", "b".repeat(MAX_SEARCH_MATCH_LINE_BYTES * 2));
        let match_at_start = match_in_line(&long_suffix, 0.."needle".len());
        assert!(match_at_start.line_truncated);
        assert!(
            !match_at_start
                .line_content
                .starts_with(SEARCH_MATCH_TRUNCATION_MARKER)
        );
        assert!(
            match_at_start
                .line_content
                .ends_with(SEARCH_MATCH_TRUNCATION_MARKER)
        );
        assert_eq!(
            &match_at_start.line_content[match_at_start.match_range],
            "needle"
        );

        let long_prefix = format!("{}needle", "a".repeat(MAX_SEARCH_MATCH_LINE_BYTES * 2));
        let start = long_prefix.len() - "needle".len();
        let match_at_end = match_in_line(&long_prefix, start..long_prefix.len());
        assert!(match_at_end.line_truncated);
        assert!(
            match_at_end
                .line_content
                .starts_with(SEARCH_MATCH_TRUNCATION_MARKER)
        );
        assert!(
            !match_at_end
                .line_content
                .ends_with(SEARCH_MATCH_TRUNCATION_MARKER)
        );
        assert_eq!(
            &match_at_end.line_content[match_at_end.match_range],
            "needle"
        );
    }

    #[test]
    fn centered_search_match_uses_half_remaining_budget_on_each_side() {
        let needle = "needle";
        let long_line = format!(
            "{}{needle}{}",
            "a".repeat(MAX_SEARCH_MATCH_LINE_BYTES * 2),
            "b".repeat(MAX_SEARCH_MATCH_LINE_BYTES * 2)
        );
        let match_start = MAX_SEARCH_MATCH_LINE_BYTES * 2;
        let search_match = match_in_line(&long_line, match_start..match_start + needle.len());
        let marker_len = SEARCH_MATCH_TRUNCATION_MARKER.len();
        let excerpt_budget = MAX_SEARCH_MATCH_LINE_BYTES - marker_len * 2;
        let expected_left_context = (excerpt_budget - needle.len()) / 2;

        assert!(
            search_match
                .line_content
                .starts_with(SEARCH_MATCH_TRUNCATION_MARKER)
        );
        assert!(
            search_match
                .line_content
                .ends_with(SEARCH_MATCH_TRUNCATION_MARKER)
        );
        assert_eq!(
            search_match.match_range.start,
            marker_len + expected_left_context
        );
        assert_eq!(&search_match.line_content[search_match.match_range], needle);
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
        assert_eq!(previews[0].original_line, "hello world");
        assert_eq!(previews[0].replaced_line, "hello Rust");
        assert_eq!(previews[0].replacement, "Rust");
        assert_eq!(previews[0].match_range, 6..11);
    }

    #[test]
    fn generate_replacement_preview_regex_expands_backreferences() {
        let options = ContentSearchOptions::new(false, true, false, true, None);
        let matches = vec![match_in_line("name: Ada", 6..9)];
        let previews = generate_replacement_preview(&matches, "([a-z]+)", "<$1>", &options);

        assert_eq!(previews.len(), 1);
        assert_eq!(previews[0].replaced_line, "name: <Ada>");
        assert_eq!(previews[0].replacement, "<Ada>");
    }

    #[test]
    fn history_and_saved_search_rows_delegate_to_query_spec() {
        let spec = SearchQuerySpec::new(
            "abcdefghijklmnopqrstuvwxyz",
            ContentSearchOptions::new(true, false, false, true, None),
        );

        let history = SearchHistoryEntry::from_spec(spec.clone());
        assert_eq!(history.toggle_summary(), "Aa");
        assert_eq!(history.display_query(8), "abcdefgh…");
        assert_eq!(history.query_spec(), spec);

        let saved = SavedSearch::from_spec("Letters", history.query_spec());
        assert_eq!(saved.name, "Letters");
        assert_eq!(saved.row_subtitle(), "abcdefghijklmnopqrstuvwxyz  Aa");
    }

    #[test]
    fn search_history_json_remains_flat() {
        let entry = SearchHistoryEntry::from_spec(SearchQuerySpec::new(
            "needle",
            ContentSearchOptions::new(true, false, true, false, Some("*.rs".to_string())),
        ));

        let json = serde_json::to_value(&entry).expect("expected operation to succeed");
        assert_eq!(json["query"], "needle");
        assert_eq!(json["case_sensitive"], true);
        assert_eq!(json["whole_word"], true);
        assert_eq!(json["gitignore"], false);
        assert_eq!(json["glob"], "*.rs");
        assert!(json.get("spec").is_none());
        assert!(json.get("options").is_none());
    }

    #[test]
    fn search_history_json_backwards_compatibility() {
        let json = serde_json::json!({
            "query": "needle",
            "case_sensitive": true,
            "regex": false,
            "whole_word": true,
            "gitignore": false,
            "glob": "*.rs"
        });

        let entry: SearchHistoryEntry =
            serde_json::from_value(json).expect("expected operation to succeed");
        assert_eq!(entry.spec.query, "needle");
        assert!(entry.spec.options.case_sensitive);
        assert!(entry.spec.options.whole_word);
        assert!(!entry.spec.options.gitignore);
        assert_eq!(entry.spec.options.glob.as_deref(), Some("*.rs"));
    }

    #[test]
    fn saved_search_json_remains_flat() {
        let entry = SavedSearch::from_spec(
            "Rust files",
            SearchQuerySpec::new(
                "needle",
                ContentSearchOptions::new(true, true, false, true, Some("*.rs".to_string())),
            ),
        );

        let json = serde_json::to_value(&entry).expect("expected operation to succeed");
        assert_eq!(json["name"], "Rust files");
        assert_eq!(json["query"], "needle");
        assert_eq!(json["case_sensitive"], true);
        assert_eq!(json["regex"], true);
        assert_eq!(json["glob"], "*.rs");
        assert!(json.get("spec").is_none());
        assert!(json.get("options").is_none());
    }
}
