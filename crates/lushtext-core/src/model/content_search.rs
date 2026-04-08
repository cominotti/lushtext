// SPDX-License-Identifier: GPL-3.0-or-later

//! Content search domain types — pure Rust, no GTK dependencies.
//!
//! Used by the search service, UI search panel (later stories),
//! tests, and benchmarks.

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
    /// An error occurred (e.g., invalid regex).
    Error(String),
    /// Search completed (always the last event sent).
    Done,
}

/// A replacement instruction for Replace All (used in later stories).
#[derive(Debug, Clone)]
pub struct Replacement {
    pub path: PathBuf,
    pub line_number: u64,
    pub original: String,
    pub replacement: String,
    pub match_range: Range<usize>,
}

/// Result of a Replace All operation (used in later stories).
#[derive(Debug)]
pub struct ReplaceResult {
    pub replaced_count: usize,
    pub files_affected: usize,
    pub files_skipped: usize,
}

/// A search history entry (used in later stories).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHistoryEntry {
    pub query: String,
    pub case_sensitive: bool,
    pub regex: bool,
    pub whole_word: bool,
    pub glob: Option<String>,
}

/// A saved/pinned search (used in later stories).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSearch {
    pub name: String,
    pub query: String,
    pub case_sensitive: bool,
    pub regex: bool,
    pub whole_word: bool,
    pub glob: Option<String>,
}
