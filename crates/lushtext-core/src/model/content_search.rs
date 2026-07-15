// SPDX-License-Identifier: GPL-3.0-or-later

//! Content search domain types — pure Rust, no GTK dependencies.
//!
//! Used by the search service, UI search panel, tests, and benchmarks.

use std::collections::HashSet;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Maximum UTF-8 bytes retained for one matching search line.
///
/// Workspace search can keep up to 10,000 matches in memory. Four KiB per
/// match keeps worst-case retained line text in the tens of MiB rather than
/// allowing minified or generated files to retain multi-GiB result sets.
pub const MAX_SEARCH_MATCH_LINE_BYTES: usize = 4 * 1024;
/// Maximum number of complete rows retained by Replace Preview.
pub const MAX_REPLACE_PREVIEW_ROWS: usize = 10_000;
/// Maximum conservatively charged UTF-8 bytes retained by Replace Preview.
pub const MAX_REPLACE_PREVIEW_BYTES: usize = 64 * 1024 * 1024;
/// ASCII marker added when a search-result line is shortened.
const SEARCH_MATCH_TRUNCATION_MARKER: &str = " [truncated]";

/// Dense identity of one search match within a single search generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SearchMatchId(usize);

impl SearchMatchId {
    /// Create an identity from the match's zero-based ingestion position.
    #[must_use]
    pub fn from_index(index: usize) -> Self {
        Self(index)
    }

    /// Return the dense zero-based index used by generation-scoped lookup tables.
    #[must_use]
    pub fn index(self) -> usize {
        self.0
    }
}

/// A single match within a file.
#[derive(Debug, Clone)]
pub struct SearchMatch {
    /// Dense identity assigned when the match enters the active search generation.
    pub id: SearchMatchId,
    /// Absolute path to the file containing the match.
    pub path: PathBuf,
    /// 1-based line number of the match.
    pub line_number: u64,
    /// Bounded text of the matching line (no trailing newline).
    ///
    /// For long lines this is a match-containing excerpt, not the full source
    /// line. `line_truncated` tells Replace All preview to avoid using it as a
    /// correctness snapshot.
    pub line_content: Arc<str>,
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
            id: SearchMatchId::from_index(0),
            path,
            line_number,
            line_content: Arc::from(line_content),
            match_range,
            line_truncated,
            original_line_byte_len,
        }
    }

    /// Assign the generation-scoped identity owned by streamed-result ingestion.
    #[must_use]
    pub fn with_id(mut self, id: SearchMatchId) -> Self {
        self.id = id;
        self
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
    /// Identity of the originating search match in the active generation.
    pub match_id: SearchMatchId,
    /// Absolute path to the file containing the match.
    pub path: PathBuf,
    /// 1-based line number.
    pub line_number: u64,
    /// The original full line content (before replacement).
    pub original_line: Arc<str>,
    /// The full line content after applying the replacement (for preview).
    pub replaced_line: String,
    /// The literal replacement text for the matched range.
    pub replacement: Arc<str>,
    /// Byte range within `original_line` that matched the query.
    pub match_range: Range<usize>,
}

/// Resource policy applied while constructing one Replace Preview outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplacePreviewBudget {
    /// Maximum number of complete replacement rows retained.
    pub max_rows: usize,
    /// Maximum conservatively charged UTF-8 payload bytes retained.
    pub max_bytes: usize,
}

impl Default for ReplacePreviewBudget {
    fn default() -> Self {
        Self {
            max_rows: MAX_REPLACE_PREVIEW_ROWS,
            max_bytes: MAX_REPLACE_PREVIEW_BYTES,
        }
    }
}

/// The resource limit that stopped Replace Preview from admitting complete rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacePreviewLimit {
    Rows,
    Bytes,
}

/// Non-content reason class for a Replace Preview row that cannot be confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacePreviewSkipReason {
    TruncatedSource,
    RegexRangeMismatch,
}

/// Constant-shape counts for every non-content Replace Preview skip reason.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReplacePreviewSkipCounts {
    truncated_source: usize,
    regex_range_mismatch: usize,
}

impl ReplacePreviewSkipCounts {
    /// Return the saturating count for one typed reason class.
    #[must_use]
    pub fn count(self, reason: ReplacePreviewSkipReason) -> usize {
        match reason {
            ReplacePreviewSkipReason::TruncatedSource => self.truncated_source,
            ReplacePreviewSkipReason::RegexRangeMismatch => self.regex_range_mismatch,
        }
    }

    /// Return the saturating total across the fixed reason universe.
    #[must_use]
    pub fn total(self) -> usize {
        self.truncated_source
            .saturating_add(self.regex_range_mismatch)
    }

    fn increment(&mut self, reason: ReplacePreviewSkipReason) {
        let count = match reason {
            ReplacePreviewSkipReason::TruncatedSource => &mut self.truncated_source,
            ReplacePreviewSkipReason::RegexRangeMismatch => &mut self.regex_range_mismatch,
        };
        *count = count.saturating_add(1);
    }
}

/// Bounded preview rows plus explicit accounting for every input match.
#[derive(Debug, Clone)]
pub struct ReplacePreviewOutcome {
    /// Complete, apply-capable rows admitted in deterministic search order.
    pub replacements: Vec<Replacement>,
    /// Dense match-ID-to-preview-index table. Omitted and skipped matches map to `None`.
    pub match_to_preview: Vec<Option<usize>>,
    /// Eligible matches excluded after the first row or byte limit was reached.
    pub omitted_eligible: usize,
    /// Constant-shape, non-content reason counts for rows that cannot be confirmed.
    pub skipped: ReplacePreviewSkipCounts,
    /// Conservative retained-payload charge for all admitted rows.
    pub charged_bytes: usize,
    /// First resource limit that prevented admission, if any.
    pub limiting_reason: Option<ReplacePreviewLimit>,
}

impl ReplacePreviewOutcome {
    #[must_use]
    pub fn len(&self) -> usize {
        self.replacements.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.replacements.is_empty()
    }

    #[must_use]
    pub fn skipped_source_count(&self) -> usize {
        self.skipped.total()
    }

    #[must_use]
    pub fn preview_index(&self, id: SearchMatchId) -> Option<usize> {
        self.match_to_preview.get(id.index()).copied().flatten()
    }

    /// Consume this preview and retain only replacements whose stable identities
    /// remain checked. Callers use this on a worker because rejected rows can own
    /// the full preview byte budget.
    #[must_use]
    pub fn into_checked_replacements(
        self,
        checked_match_ids: &HashSet<SearchMatchId>,
    ) -> Vec<Replacement> {
        self.replacements
            .into_iter()
            .filter(|replacement| checked_match_ids.contains(&replacement.match_id))
            .collect()
    }
}

impl std::ops::Deref for ReplacePreviewOutcome {
    type Target = [Replacement];

    fn deref(&self) -> &Self::Target {
        &self.replacements
    }
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
    };
    let mut budget_exhausted = false;
    let mut literal_charged = false;

    for m in matches {
        if is_cancelled() {
            break;
        }
        if m.line_truncated {
            outcome
                .skipped
                .increment(ReplacePreviewSkipReason::TruncatedSource);
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
                outcome
                    .skipped
                    .increment(ReplacePreviewSkipReason::RegexRangeMismatch);
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
                outcome
                    .skipped
                    .increment(ReplacePreviewSkipReason::RegexRangeMismatch);
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
            path: m.path.clone(),
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
    use std::fmt;
    use std::sync::{Arc, Mutex};

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
