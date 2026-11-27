// SPDX-License-Identifier: GPL-3.0-or-later

//! Content search domain types — pure Rust, no GTK dependencies.
//!
//! Used by the search service, UI search panel, tests, and benchmarks.

use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::workspace_search::{WorkspaceSearchFallbackMetrics, WorkspaceSearchIncompleteReason};

/// Maximum UTF-8 bytes retained for one matching search line.
///
/// Workspace search retains matches against a byte budget
/// (`ui::search_panel::policy::SEARCH_RESULTS_RETAINED_BYTES`) that charges
/// each match's line. Four KiB per match keeps one minified or generated
/// line from consuming a large share of it, and is the per-match line of
/// `main`'s worst case that budget is sized against.
///
/// Kind: presentation cap; a power of two, never rounded down (`power-of-two-thresholds` D12).
pub const MAX_SEARCH_MATCH_LINE_BYTES: usize = 4 * 1024;
/// Maximum number of complete rows retained by Replace Preview.
///
/// Kind: presentation cap; 10,000 rounded up to a power of two
/// (`power-of-two-thresholds` D12; its bound measured in task 9.5 M3).
pub const MAX_REPLACE_PREVIEW_ROWS: usize = 16 * 1024;
/// Maximum conservatively charged UTF-8 bytes retained by Replace Preview.
///
/// Kind: presentation cap; a power of two, never rounded down (`power-of-two-thresholds` D12).
pub const MAX_REPLACE_PREVIEW_BYTES: usize = 64 * 1024 * 1024;
/// Maximum diagnostic records retained by Replace All or Undo.
///
/// Kind: presentation cap; a power of two, never rounded down (`power-of-two-thresholds` D12).
pub const MAX_REPLACE_DIAGNOSTIC_ENTRIES: usize = 32;
/// Maximum conservative heap bytes retained by Replace All or Undo diagnostics.
///
/// Kind: presentation cap; a power of two, never rounded down (`power-of-two-thresholds` D12).
pub const MAX_REPLACE_DIAGNOSTIC_BYTES: usize = 32 * 1024;
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
    /// Absolute path to the file containing the match, interned once per file
    /// by the worker and shared by every match of that file, its file row and
    /// the navigation index (`bound-lists-by-budget-not-count` task 3.3).
    pub path: Arc<Path>,
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
    /// Engine-root position used to recover configured display ownership.
    pub traversal_root_index: usize,
}

impl SearchMatch {
    /// Build one bounded search match from a full source line.
    #[must_use]
    pub fn new(
        path: impl Into<Arc<Path>>,
        line_number: u64,
        line_content: &str,
        match_range: Range<usize>,
    ) -> Self {
        let original_line_byte_len = line_content.len();
        let (line_content, match_range, line_truncated) =
            bounded_match_line(line_content, match_range);
        Self {
            id: SearchMatchId::from_index(0),
            path: path.into(),
            line_number,
            line_content: Arc::from(line_content),
            match_range,
            line_truncated,
            original_line_byte_len,
            traversal_root_index: 0,
        }
    }

    /// Assign the generation-scoped identity owned by streamed-result ingestion.
    #[must_use]
    pub fn with_id(mut self, id: SearchMatchId) -> Self {
        self.id = id;
        self
    }

    /// Attach the immutable traversal-root position owned by the search plan.
    #[must_use]
    pub fn with_traversal_root_index(mut self, traversal_root_index: usize) -> Self {
        self.traversal_root_index = traversal_root_index;
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
    /// When true, dotfiles are searched. Default: false.
    ///
    /// Serde-defaulted because these options are flattened into persisted
    /// search history and saved searches, which load through the recovery
    /// envelope; a missing field must not quarantine a user's saved searches.
    #[serde(default)]
    pub hidden: bool,
}

impl Default for ContentSearchOptions {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            regex: false,
            whole_word: false,
            gitignore: true,
            glob: None,
            hidden: false,
        }
    }
}

/// Boolean matching and traversal flags for [`ContentSearchOptions::new`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContentSearchFlags {
    /// Whether ASCII/Unicode matching is case-sensitive.
    pub case_sensitive: bool,
    /// Whether the query is parsed as a regular expression.
    pub regex: bool,
    /// Whether matches must align on whole-word boundaries.
    pub whole_word: bool,
    /// Whether traversal respects `.gitignore` rules.
    pub gitignore: bool,
}

impl Default for ContentSearchFlags {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            regex: false,
            whole_word: false,
            gitignore: true,
        }
    }
}

impl ContentSearchOptions {
    /// Build one search-options value object from the current toggle state.
    #[must_use]
    pub fn new(flags: ContentSearchFlags, glob: Option<String>) -> Self {
        Self {
            case_sensitive: flags.case_sensitive,
            regex: flags.regex,
            whole_word: flags.whole_word,
            gitignore: flags.gitignore,
            glob,
            hidden: false,
        }
    }

    /// Return the same options with the hidden-files axis set.
    #[must_use]
    pub fn with_hidden(mut self, hidden: bool) -> Self {
        self.hidden = hidden;
        self
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
        if self.hidden {
            parts.push("hidden".to_string());
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
    /// The retained-byte budget refused a match: every match sent before this
    /// event is retained, none after it. The search keeps walking and counting.
    BudgetReached {
        /// Matches retained (sent as `Match`) before the refusal.
        retained: usize,
    },
    /// Running count of matches found, including those past the budget.
    /// Coalesced to at most one every [`BEYOND_BUDGET_REPORT_INTERVAL_MS`],
    /// best-effort like `Progress`; `Done` carries the final count.
    BeyondBudget(usize),
    /// Progress update: number of files visited so far. Sent approximately
    /// every 100 files to avoid flooding the channel. Best-effort — may be
    /// skipped if the channel is full.
    Progress(usize),
    /// An error occurred (e.g., invalid regex).
    Error(String),
    /// Search stopped before ambiguous alias identity ownership exceeded policy.
    Incomplete(WorkspaceSearchIncompleteReason),
    /// Compact terminal evidence from the optional ambiguous-alias ledger.
    TraversalMetrics(WorkspaceSearchFallbackMetrics),
    /// Search completed (always the last event sent), with every match
    /// found, retained or not.
    Done {
        /// Matches found by the whole walk, including those past the budget.
        matches_found: usize,
    },
}

/// Smallest interval between two `BeyondBudget` reports, milliseconds.
///
/// Kind: out: time; the cadence of a running count label.
pub const BEYOND_BUDGET_REPORT_INTERVAL_MS: u64 = 250;

/// How the search worker charges retained matches against its budget
/// (`bound-lists-by-budget-not-count` D3). The budget and the fixed charge are
/// the search panel's policy (`ui::search_panel::policy`); the arithmetic is
/// here because the worker, a service, applies it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchRetention {
    /// The retained-byte budget.
    pub budget_bytes: u64,
    /// Bytes charged per match beyond its line text: the match's own struct,
    /// its list item, row and navigation entries.
    pub match_fixed_charge: u64,
}

impl SearchRetention {
    /// No budget: every match is retained (tests and benchmarks).
    pub const UNLIMITED: Self = Self {
        budget_bytes: u64::MAX,
        match_fixed_charge: 0,
    };

    /// The charge of one match: its line bytes and the fixed charge, plus,
    /// for the first match of a file, the file's path bytes twice (the
    /// canonical path and the display path, which is never longer).
    #[must_use]
    pub fn match_charge(self, line_bytes: usize, first_of_file_path_bytes: Option<usize>) -> u64 {
        let path = first_of_file_path_bytes.map_or(0u64, |bytes| (bytes as u64).saturating_mul(2));
        (line_bytes as u64)
            .saturating_add(self.match_fixed_charge)
            .saturating_add(path)
    }
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

    /// Increment the counter for one non-content skip reason.
    pub fn increment(&mut self, reason: ReplacePreviewSkipReason) {
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
    /// Every file with a match that cannot be confirmed, in first-seen order,
    /// with its reason class, count and first line, for the Replace All skip
    /// report (design D7). One per file; paths only, never text.
    pub ineligible_files: Vec<ReplacePreviewIneligibleFile>,
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
        self.into_checked_run(checked_match_ids).replacements
    }

    /// The checked rows, with every file the preview could not confirm, as one
    /// Replace All request (design D7).
    #[must_use]
    pub fn into_checked_run(self, checked_match_ids: &HashSet<SearchMatchId>) -> CheckedReplaceRun {
        CheckedReplaceRun {
            replacements: self
                .replacements
                .into_iter()
                .filter(|replacement| checked_match_ids.contains(&replacement.match_id))
                .collect(),
            ineligible: self.ineligible_files,
        }
    }
}

/// What one confirmed Replace All applies: the checked preview rows, and the
/// files whose matches the preview could not confirm, which the run reports.
#[derive(Debug, Clone, Default)]
pub struct CheckedReplaceRun {
    /// Checked rows of the current preview generation.
    pub replacements: Vec<Replacement>,
    /// Files with unconfirmable matches, folded into the skip report.
    pub ineligible: Vec<ReplacePreviewIneligibleFile>,
}

impl CheckedReplaceRun {
    /// Whether nothing would be replaced.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.replacements.is_empty()
    }

    /// Number of checked rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.replacements.len()
    }
}

impl std::ops::Deref for ReplacePreviewOutcome {
    type Target = [Replacement];

    fn deref(&self) -> &Self::Target {
        &self.replacements
    }
}

/// Reversible byte accounting for the currently retryable Replace All payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UndoPayloadLedger {
    limit: u64,
    live_bytes: u64,
    high_water_bytes: u64,
}

impl UndoPayloadLedger {
    #[must_use]
    pub const fn new(limit: u64) -> Self {
        Self {
            limit,
            live_bytes: 0,
            high_water_bytes: 0,
        }
    }

    /// Charge before retaining one reversible entry. Exact-limit ownership is valid.
    pub fn try_charge(&mut self, bytes: u64) -> bool {
        let Some(next) = self.live_bytes.checked_add(bytes) else {
            return false;
        };
        if next > self.limit {
            return false;
        }
        self.live_bytes = next;
        self.high_water_bytes = self.high_water_bytes.max(next);
        true
    }

    /// Reclaim bytes for an entry removed before its target rename.
    pub fn reclaim(&mut self, bytes: u64) -> u64 {
        let reclaimed = self.live_bytes.min(bytes);
        self.live_bytes -= reclaimed;
        reclaimed
    }

    #[must_use]
    pub const fn remaining_bytes(self) -> u64 {
        self.limit.saturating_sub(self.live_bytes)
    }

    #[must_use]
    pub const fn live_bytes(self) -> u64 {
        self.live_bytes
    }

    #[must_use]
    pub const fn high_water_bytes(self) -> u64 {
        self.high_water_bytes
    }
}

/// Exact diagnostic totals plus a deterministic bounded sample.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct BoundedDiagnosticSample {
    total_count: usize,
    retained_bytes: usize,
    entries: Vec<String>,
}

impl BoundedDiagnosticSample {
    pub fn record(&mut self, message: String) {
        self.total_count = self.total_count.saturating_add(1);
        if self.entries.len() == MAX_REPLACE_DIAGNOSTIC_ENTRIES {
            return;
        }
        let vector_shell_charge = if self.entries.capacity() == 0 {
            MAX_REPLACE_DIAGNOSTIC_ENTRIES.saturating_mul(std::mem::size_of::<String>())
        } else {
            0
        };
        let weight = vector_shell_charge.saturating_add(message.capacity());
        if self.retained_bytes.saturating_add(weight) > MAX_REPLACE_DIAGNOSTIC_BYTES {
            return;
        }
        if self.entries.capacity() == 0 {
            self.entries = Vec::with_capacity(MAX_REPLACE_DIAGNOSTIC_ENTRIES);
            debug_assert_eq!(self.entries.capacity(), MAX_REPLACE_DIAGNOSTIC_ENTRIES);
        }
        self.retained_bytes = self.retained_bytes.saturating_add(weight);
        self.entries.push(message);
    }

    pub fn push(&mut self, message: String) {
        self.record(message);
    }

    pub fn record_path(&mut self, path: &std::path::Path) {
        self.record(path.display().to_string());
    }

    #[must_use]
    pub const fn total_count(&self) -> usize {
        self.total_count
    }

    #[must_use]
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    #[must_use]
    pub const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    #[must_use]
    pub const fn omitted_count(&self) -> usize {
        self.total_count.saturating_sub(self.entries.len())
    }

    #[must_use]
    pub fn summary(&self, prefix: &str) -> String {
        let mut summary = format!("{prefix}: {} issue(s)", self.total_count);
        let mut rendered_entries = 0usize;
        if let Some(first) = self.entries.first() {
            summary.push_str("; first: ");
            summary.push_str(first);
            rendered_entries = 1;
        }
        let unrendered_count = self.total_count.saturating_sub(rendered_entries);
        if unrendered_count > 0 {
            summary.push_str(&format!("; {unrendered_count} more omitted"));
        }
        summary
    }
}

impl Clone for BoundedDiagnosticSample {
    fn clone(&self) -> Self {
        let mut entries = if self.entries.is_empty() {
            Vec::new()
        } else {
            Vec::with_capacity(MAX_REPLACE_DIAGNOSTIC_ENTRIES)
        };
        entries.extend(self.entries.iter().cloned());
        let retained_bytes = entries
            .capacity()
            .saturating_mul(std::mem::size_of::<String>())
            .saturating_add(
                entries
                    .iter()
                    .map(String::capacity)
                    .fold(0usize, usize::saturating_add),
            );
        debug_assert!(retained_bytes <= MAX_REPLACE_DIAGNOSTIC_BYTES);
        Self {
            total_count: self.total_count,
            retained_bytes,
            entries,
        }
    }
}

impl std::ops::Deref for BoundedDiagnosticSample {
    type Target = [String];

    fn deref(&self) -> &Self::Target {
        self.entries()
    }
}

/// Result of a Replace All operation.
#[derive(Debug)]
pub struct ReplaceResult {
    pub replaced_count: usize,
    pub files_affected: usize,
    /// Exact number of files skipped without mutation.
    pub skipped_count: usize,
    /// Exact number of non-fatal failures encountered.
    pub error_count: usize,
    /// Bounded deterministic paths for skipped targets.
    pub skipped_sample: BoundedDiagnosticSample,
    /// Bounded deterministic failure messages; never includes document text.
    pub error_sample: BoundedDiagnosticSample,
    /// Successfully affected paths intersected with the caller's open identities.
    pub affected_open_paths: Vec<PathBuf>,
}

/// Charged path bytes the Replace All skip list keeps (design D7).
///
/// Kind: presentation cap; a power of two. One run touches at most
/// `MAX_REPLACE_PREVIEW_ROWS` files, and at a typical 100-byte path every one
/// of them is listed; only a path-heavy worst case is cut, and the report then
/// says how many it did not list.
pub const MAX_REPLACE_SKIP_LIST_BYTES: usize = 4 * 1024 * 1024;

/// Why Replace All did not replace one file (design D7).
///
/// The variants are in the precedence order a file with two reasons is
/// reported by: [`Self::UnsavedChanges`] first (decided before any worker
/// runs), then the preview's reasons for a file none of whose matches was
/// eligible, then the worker's in the order it checks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ReplaceSkipReason {
    /// The file is open with unsaved changes or an active save.
    UnsavedChanges,
    /// Its matches are on lines longer than the search excerpt.
    LineTooLong,
    /// Its preview rows violated an invariant of preview generation.
    InvalidPreview,
    /// It could not be read, locked or examined.
    Unreadable,
    /// It, or its replacement output, is larger than the largest file LushText opens.
    LargerThanOpenTier,
    /// It is not valid UTF-8.
    NotUtf8,
    /// A matched line changed after the search.
    ChangedSinceSearch,
    /// Its undo body or output would not fit the free disk space.
    NotEnoughDiskSpace,
    /// Its undo journal entry could not be written.
    UndoJournalFailed,
    /// Another program changed it while Replace All was rewriting it.
    ChangedDuringReplace,
    /// Writing its new bytes failed; the file keeps its previous bytes.
    WriteFailed,
}

impl ReplaceSkipReason {
    /// Every reason, in precedence order.
    pub const ALL: [Self; 11] = [
        Self::UnsavedChanges,
        Self::LineTooLong,
        Self::InvalidPreview,
        Self::Unreadable,
        Self::LargerThanOpenTier,
        Self::NotUtf8,
        Self::ChangedSinceSearch,
        Self::NotEnoughDiskSpace,
        Self::UndoJournalFailed,
        Self::ChangedDuringReplace,
        Self::WriteFailed,
    ];

    /// Dense index into [`Self::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Stable machine name used by automation snapshots.
    #[must_use]
    pub const fn automation_name(self) -> &'static str {
        match self {
            Self::UnsavedChanges => "unsaved-changes",
            Self::LineTooLong => "line-too-long",
            Self::InvalidPreview => "invalid-preview",
            Self::Unreadable => "unreadable",
            Self::LargerThanOpenTier => "larger-than-open-tier",
            Self::NotUtf8 => "not-utf8",
            Self::ChangedSinceSearch => "changed-since-search",
            Self::NotEnoughDiskSpace => "not-enough-disk-space",
            Self::UndoJournalFailed => "undo-journal-failed",
            Self::ChangedDuringReplace => "changed-during-replace",
            Self::WriteFailed => "write-failed",
        }
    }
}

/// One file the skip list names. Paths and reason classes only, never text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceSkippedFile {
    /// The file.
    pub path: PathBuf,
    /// Why it was not (fully) replaced.
    pub reason: ReplaceSkipReason,
    /// First skipped line, one-based, when one is known.
    pub first_line: Option<u64>,
    /// For a partly replaced file, the matches on long lines left unreplaced;
    /// `None` for a file that was not replaced at all.
    pub matches_left: Option<usize>,
}

/// Per-file skip facts that preview generation decided, folded into the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplacePreviewIneligibleFile {
    /// The file.
    pub path: PathBuf,
    /// `LineTooLong` or `InvalidPreview`.
    pub reason: ReplaceSkipReason,
    /// Ineligible matches in the file.
    pub matches: usize,
    /// First ineligible line, one-based.
    pub first_line: u64,
}

/// Every skipped and partly replaced file of one Replace All run (design D7).
///
/// Counts are exact; the list is bounded by [`MAX_REPLACE_SKIP_LIST_BYTES`] of
/// charged path bytes, and [`Self::not_listed`] counts what it cut.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplaceSkipReport {
    counts: [usize; 11],
    partly_replaced: usize,
    files: Vec<ReplaceSkippedFile>,
    charged_bytes: usize,
    not_listed: usize,
}

impl ReplaceSkipReport {
    /// Conservative charge of one listed file: its row plus its path bytes.
    #[must_use]
    pub fn charge_of(path: &Path) -> usize {
        std::mem::size_of::<ReplaceSkippedFile>().saturating_add(path.as_os_str().len())
    }

    /// Record one file that was not replaced.
    pub fn record_skipped(
        &mut self,
        path: &Path,
        reason: ReplaceSkipReason,
        first_line: Option<u64>,
    ) {
        self.counts[reason.index()] = self.counts[reason.index()].saturating_add(1);
        self.list(ReplaceSkippedFile {
            path: path.to_path_buf(),
            reason,
            first_line,
            matches_left: None,
        });
    }

    /// The report of a run that never started because every file it targets
    /// is open with unsaved changes or an active save: one `UnsavedChanges`
    /// row per distinct file, at the first line its replacements target.
    #[must_use]
    pub fn unsaved_only(replacements: &[Replacement]) -> Self {
        let mut first_lines: std::collections::BTreeMap<&Path, u64> =
            std::collections::BTreeMap::new();
        for replacement in replacements {
            first_lines
                .entry(replacement.path.as_path())
                .and_modify(|line| *line = (*line).min(replacement.line_number))
                .or_insert(replacement.line_number);
        }
        let mut report = Self::default();
        for (path, line) in first_lines {
            report.record_skipped(path, ReplaceSkipReason::UnsavedChanges, Some(line));
        }
        report.finish();
        report
    }

    /// Record one replaced file whose matches on long lines were not replaced.
    pub fn record_partly_replaced(
        &mut self,
        path: &Path,
        reason: ReplaceSkipReason,
        first_line: u64,
        matches_left: usize,
    ) {
        self.partly_replaced = self.partly_replaced.saturating_add(1);
        self.list(ReplaceSkippedFile {
            path: path.to_path_buf(),
            reason,
            first_line: Some(first_line),
            matches_left: Some(matches_left),
        });
    }

    fn list(&mut self, file: ReplaceSkippedFile) {
        let charge = Self::charge_of(&file.path);
        if self.charged_bytes.saturating_add(charge) > MAX_REPLACE_SKIP_LIST_BYTES {
            self.not_listed = self.not_listed.saturating_add(1);
            return;
        }
        self.charged_bytes += charge;
        self.files.push(file);
    }

    /// Sort the list for presentation: by reason, then by path.
    pub fn finish(&mut self) {
        self.files
            .sort_by(|a, b| a.reason.cmp(&b.reason).then_with(|| a.path.cmp(&b.path)));
    }

    /// Exact number of files skipped for `reason` (partly replaced files excluded).
    #[must_use]
    pub fn count(&self, reason: ReplaceSkipReason) -> usize {
        self.counts[reason.index()]
    }

    /// Exact number of files not replaced, over every reason.
    #[must_use]
    pub fn skipped_total(&self) -> usize {
        self.counts.iter().copied().fold(0, usize::saturating_add)
    }

    /// Exact number of replaced files with matches left on long lines.
    #[must_use]
    pub const fn partly_replaced(&self) -> usize {
        self.partly_replaced
    }

    /// The listed files, in [`Self::finish`] order once finished.
    #[must_use]
    pub fn files(&self) -> &[ReplaceSkippedFile] {
        &self.files
    }

    /// Listed and unlisted files together.
    #[must_use]
    pub fn total(&self) -> usize {
        self.skipped_total().saturating_add(self.partly_replaced)
    }

    /// Files the list does not name because of its byte bound.
    #[must_use]
    pub const fn not_listed(&self) -> usize {
        self.not_listed
    }

    /// Charged bytes the list retains, at most [`MAX_REPLACE_SKIP_LIST_BYTES`].
    #[must_use]
    pub const fn charged_bytes(&self) -> usize {
        self.charged_bytes
    }

    /// Whether the run skipped or partly replaced anything.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
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
        let options = ContentSearchOptions::new(
            ContentSearchFlags {
                case_sensitive: true,
                regex: true,
                whole_word: true,
                gitignore: false,
            },
            Some("src/**/*.rs".to_string()),
        );

        assert_eq!(
            options.toggle_summary(),
            "Aa  .*  W  no .gitignore  src/**/*.rs"
        );
    }

    #[test]
    fn toggle_summary_omits_disabled_and_empty_values() {
        assert_eq!(ContentSearchOptions::default().toggle_summary(), "");

        let options = ContentSearchOptions::new(ContentSearchFlags::default(), Some(String::new()));
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
    fn history_and_saved_search_rows_delegate_to_query_spec() {
        let spec = SearchQuerySpec::new(
            "abcdefghijklmnopqrstuvwxyz",
            ContentSearchOptions::new(
                ContentSearchFlags {
                    case_sensitive: true,
                    ..ContentSearchFlags::default()
                },
                None,
            ),
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
            ContentSearchOptions::new(
                ContentSearchFlags {
                    case_sensitive: true,
                    regex: false,
                    whole_word: true,
                    gitignore: false,
                },
                Some("*.rs".to_string()),
            ),
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
                ContentSearchOptions::new(
                    ContentSearchFlags {
                        case_sensitive: true,
                        regex: true,
                        ..ContentSearchFlags::default()
                    },
                    Some("*.rs".to_string()),
                ),
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

    #[test]
    fn undo_payload_ledger_reclaims_live_bytes_without_lowering_high_water() {
        let mut ledger = UndoPayloadLedger::new(10);
        assert!(ledger.try_charge(4));
        assert!(ledger.try_charge(6));
        assert_eq!(ledger.live_bytes(), 10);
        assert_eq!(ledger.high_water_bytes(), 10);
        assert!(!ledger.try_charge(1));
        assert!(!ledger.try_charge(u64::MAX));
        assert_eq!(ledger.reclaim(4), 4);
        assert_eq!(ledger.live_bytes(), 6);
        assert_eq!(ledger.high_water_bytes(), 10);
        assert!(ledger.try_charge(4));
        assert_eq!(ledger.reclaim(99), 10);
        assert_eq!(ledger.live_bytes(), 0);
        assert_eq!(ledger.high_water_bytes(), 10);
    }

    #[test]
    fn diagnostic_sample_keeps_exact_totals_under_count_and_byte_caps() {
        let mut sample = BoundedDiagnosticSample::default();
        for index in 0..10_000 {
            sample.record(format!("/workspace/path-{index:05}: failed"));
        }

        assert_eq!(sample.total_count(), 10_000);
        assert_eq!(sample.entries().len(), MAX_REPLACE_DIAGNOSTIC_ENTRIES);
        assert!(sample.retained_bytes() <= MAX_REPLACE_DIAGNOSTIC_BYTES);
        assert_eq!(sample.omitted_count(), 10_000 - sample.entries().len());
        assert_eq!(
            sample.entries().first().map(String::as_str),
            Some("/workspace/path-00000: failed")
        );

        let vector_shell = MAX_REPLACE_DIAGNOSTIC_ENTRIES * std::mem::size_of::<String>();
        let exact_message_capacity = MAX_REPLACE_DIAGNOSTIC_BYTES - vector_shell;
        let exact_message = String::with_capacity(exact_message_capacity);
        assert_eq!(exact_message.capacity(), exact_message_capacity);
        let mut exact = BoundedDiagnosticSample::default();
        exact.record(exact_message);
        assert_eq!(exact.entries().len(), 1);
        assert_eq!(exact.retained_bytes(), MAX_REPLACE_DIAGNOSTIC_BYTES);
        let cloned = exact.clone();
        assert_eq!(cloned.entries(), exact.entries());
        assert!(cloned.retained_bytes() <= MAX_REPLACE_DIAGNOSTIC_BYTES);

        let mut one_over = BoundedDiagnosticSample::default();
        one_over.record(String::with_capacity(
            exact_message_capacity.saturating_add(1),
        ));
        assert_eq!(one_over.total_count(), 1);
        assert!(one_over.entries().is_empty());
        assert_eq!(one_over.retained_bytes(), 0);

        let mut hostile_capacity = String::with_capacity(MAX_REPLACE_DIAGNOSTIC_BYTES * 4);
        hostile_capacity.push('x');
        let mut hostile = BoundedDiagnosticSample::default();
        hostile.record(hostile_capacity);
        assert_eq!(hostile.total_count(), 1);
        assert!(hostile.entries().is_empty());
        assert_eq!(hostile.retained_bytes(), 0);
    }
}
