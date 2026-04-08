// SPDX-License-Identifier: GPL-3.0-or-later

//! Content search domain types — pure Rust, no GTK dependencies.
//!
//! Used by the search service, UI search panel, tests, and benchmarks.

use std::ops::Range;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A single match within a file.
#[derive(Debug, Clone)]
pub struct SearchMatch {
    /// Absolute path to the file containing the match.
    pub path: PathBuf,
    /// 1-based line number of the match.
    pub line_number: u64,
    /// Full text content of the matching line (no trailing newline).
    pub line_content: String,
    /// Byte range within `line_content` that matched the query.
    pub match_range: Range<usize>,
}

/// Options controlling search behavior.
#[derive(Debug, Clone)]
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
        .map(|m| {
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

            Replacement {
                path: m.path.clone(),
                line_number: m.line_number,
                original_line,
                replaced_line,
                replacement: replacement_text,
                match_range: m.match_range.clone(),
            }
        })
        .collect()
}

/// A single entry in the search history, capturing query text and all toggle
/// states at the time of search. Persisted to `search-history.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHistoryEntry {
    pub query: String,
    pub case_sensitive: bool,
    pub regex: bool,
    pub whole_word: bool,
    pub gitignore: bool,
    pub glob: Option<String>,
}

/// A named saved search, persisted permanently to `saved-searches.json`.
///
/// Unlike `SearchHistoryEntry` (capped at 20, auto-managed), saved searches
/// are user-created and persist until explicitly deleted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedSearch {
    pub name: String,
    pub query: String,
    pub case_sensitive: bool,
    pub regex: bool,
    pub whole_word: bool,
    pub gitignore: bool,
    pub glob: Option<String>,
}
