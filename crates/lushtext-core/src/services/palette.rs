// SPDX-License-Identifier: GPL-3.0-or-later

//! Command palette service — fuzzy matching, file indexing, and command registry.
//!
//! Pure Rust with no GTK dependencies. All functions operate on domain types
//! from `model::palette` and are fully unit-testable without a display server.

use crate::model::palette::{
    CommandCategory, CommandDef, IndexedFile, ScoredResult, SearchMode, SearchResultItem,
};
use crate::services::file_tree;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// File index
// ---------------------------------------------------------------------------

/// In-memory index of all files across workspace roots.
#[derive(Debug, Default, Clone)]
pub struct FileIndex {
    files: Vec<IndexedFile>,
}

impl FileIndex {
    /// Build a file index by recursively scanning all workspace root directories.
    /// Runs synchronous I/O — call from a background thread via `spawn_blocking_then`.
    ///
    /// Uses visited-path tracking and depth limiting to handle symlink cycles
    /// (e.g., Wine/Proton `dosdevices/` symlink loops).
    pub fn rebuild(roots: &[PathBuf]) -> Self {
        let mut files = Vec::new();
        let mut visited = HashSet::new();
        for root in roots {
            collect_files_recursive(root, root, &mut files, &mut visited, 0);
        }
        Self { files }
    }

    pub fn files(&self) -> &[IndexedFile] {
        &self.files
    }

    /// Search the file index with a fuzzy query, returning up to `max` scored results.
    pub fn search(&self, query: &str, max: usize) -> Vec<ScoredResult<'_>> {
        search_items(
            self.files.iter(),
            |f| &f.name,
            SearchResultItem::File,
            query,
            max,
        )
    }
}

/// Maximum recursion depth to prevent runaway scanning in deeply nested trees.
const MAX_SCAN_DEPTH: u32 = 64;

/// Recursively scan a directory, collecting files (not directories) into `out`.
///
/// Tracks visited canonical paths to break symlink cycles, and enforces a depth
/// limit. Both defenses are needed: canonical path tracking handles direct cycles
/// (symlink → ancestor), while the depth limit catches indirect expansion from
/// non-cyclic but deeply nested symlink trees.
fn collect_files_recursive(
    dir: &Path,
    workspace_root: &Path,
    out: &mut Vec<IndexedFile>,
    visited: &mut HashSet<PathBuf>,
    depth: u32,
) {
    if depth > MAX_SCAN_DEPTH {
        tracing::warn!(
            "Skipping deeply nested directory (depth > {MAX_SCAN_DEPTH}): {}",
            dir.display()
        );
        return;
    }

    // Resolve to canonical path to detect symlink cycles
    let canonical = match dir.canonicalize() {
        Ok(p) => p,
        Err(_) => return, // unresolvable path (broken symlink, permission denied)
    };
    if !visited.insert(canonical) {
        return; // already visited — symlink cycle
    }

    for (path, is_dir) in file_tree::scan_directory(dir) {
        if is_dir {
            collect_files_recursive(&path, workspace_root, out, visited, depth + 1);
        } else {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            out.push(IndexedFile {
                path,
                name,
                workspace_root: workspace_root.to_path_buf(),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Command registry
// ---------------------------------------------------------------------------

/// All built-in commands available in the palette.
pub fn all_commands() -> &'static [CommandDef] {
    static COMMANDS: &[CommandDef] = &[
        // File
        CommandDef {
            id: "win.new-tab",
            label: "New File",
            category: CommandCategory::File,
            shortcut: Some("Ctrl+T"),
        },
        CommandDef {
            id: "win.open-file",
            label: "Open File",
            category: CommandCategory::File,
            shortcut: Some("Ctrl+O"),
        },
        CommandDef {
            id: "win.open-folder",
            label: "Open Folder",
            category: CommandCategory::File,
            shortcut: None,
        },
        CommandDef {
            id: "win.save",
            label: "Save",
            category: CommandCategory::File,
            shortcut: Some("Ctrl+S"),
        },
        CommandDef {
            id: "win.save-as",
            label: "Save As",
            category: CommandCategory::File,
            shortcut: Some("Ctrl+Shift+S"),
        },
        // Edit
        CommandDef {
            id: "win.toggle-search",
            label: "Find and Replace",
            category: CommandCategory::Edit,
            shortcut: Some("Ctrl+F"),
        },
        CommandDef {
            id: "win.close-tab",
            label: "Close Tab",
            category: CommandCategory::Edit,
            shortcut: Some("Ctrl+W"),
        },
        // View
        CommandDef {
            id: "win.show-help-overlay",
            label: "Keyboard Shortcuts",
            category: CommandCategory::View,
            shortcut: None,
        },
        // App
        CommandDef {
            id: "app.preferences",
            label: "Preferences",
            category: CommandCategory::App,
            shortcut: None,
        },
        CommandDef {
            id: "app.about",
            label: "About LushText",
            category: CommandCategory::App,
            shortcut: None,
        },
        CommandDef {
            id: "app.quit",
            label: "Quit",
            category: CommandCategory::App,
            shortcut: Some("Ctrl+Q"),
        },
    ];
    COMMANDS
}

/// Search the command registry with a fuzzy query.
pub fn search_commands(query: &str, max: usize) -> Vec<ScoredResult<'static>> {
    search_items(
        all_commands().iter(),
        |c| c.label,
        SearchResultItem::Command,
        query,
        max,
    )
}

// ---------------------------------------------------------------------------
// Unified search
// ---------------------------------------------------------------------------

/// Search both files and commands according to the given mode.
pub fn search_all<'a>(
    index: &'a FileIndex,
    query: &str,
    mode: SearchMode,
    max: usize,
) -> Vec<ScoredResult<'a>> {
    match mode {
        SearchMode::Files => index.search(query, max),
        SearchMode::Commands => search_commands(query, max),
        SearchMode::All => {
            let half = max / 2;
            let file_max = max - half;
            let cmd_max = half.max(1);

            let mut results: Vec<ScoredResult<'a>> = index.search(query, file_max);
            results.extend(search_commands(query, cmd_max));
            results.sort_by(|a, b| b.score.cmp(&a.score));
            results.truncate(max);
            results
        }
    }
}

// ---------------------------------------------------------------------------
// Fuzzy matching
// ---------------------------------------------------------------------------

/// Score a fuzzy subsequence match of `query` against `candidate`.
///
/// Returns `Some(score)` if all characters in `query` appear in order within
/// `candidate`, with higher scores for better matches. Returns `None` if there
/// is no match.
///
/// Empty query matches everything with score 0.
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<u32> {
    if query.is_empty() {
        return Some(0);
    }
    let query_chars: Vec<char> = query.to_lowercase().chars().collect();
    fuzzy_score_chars(&query_chars, candidate)
}

/// Inner scoring function that takes pre-lowercased query chars.
/// Avoids re-allocating the query on every call within `search_items`.
fn fuzzy_score_chars(query_chars: &[char], candidate: &str) -> Option<u32> {
    if query_chars.is_empty() {
        return Some(0);
    }

    let mut score: u32 = 0;
    let mut query_idx = 0;
    let mut prev_match_idx: Option<usize> = None;
    let mut prev_cand_char = '\0';

    for (cand_idx, cand_char) in candidate.chars().enumerate() {
        let cand_lower = cand_char.to_lowercase().next().unwrap_or(cand_char);
        if query_idx < query_chars.len() && cand_lower == query_chars[query_idx] {
            score += 1;

            if cand_idx == 0 {
                score += 3;
            }

            if cand_idx > 0 && matches!(prev_cand_char, '/' | '.' | '_' | '-' | ' ') {
                score += 2;
            }

            if let Some(prev_idx) = prev_match_idx {
                if cand_idx == prev_idx + 1 {
                    score += 2;
                }
            }

            prev_match_idx = Some(cand_idx);
            query_idx += 1;
        }
        prev_cand_char = cand_char;
    }

    if query_idx == query_chars.len() {
        Some(score)
    } else {
        None
    }
}

/// Generic search helper: filter + score items, sort by score descending, cap at max.
fn search_items<'a, I, T, F, G>(
    items: I,
    get_text: F,
    wrap: G,
    query: &str,
    max: usize,
) -> Vec<ScoredResult<'a>>
where
    I: Iterator<Item = &'a T>,
    T: 'a,
    F: Fn(&T) -> &str,
    G: Fn(&'a T) -> SearchResultItem<'a>,
{
    let query_chars: Vec<char> = query.to_lowercase().chars().collect();
    let mut results: Vec<ScoredResult<'a>> = items
        .filter_map(|item| {
            let text = get_text(item);
            fuzzy_score_chars(&query_chars, text).map(|score| ScoredResult {
                item: wrap(item),
                score,
            })
        })
        .collect();
    results.sort_by(|a, b| b.score.cmp(&a.score));
    results.truncate(max);
    results
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // --- fuzzy_score ---

    #[test]
    fn test_empty_query_matches_everything() {
        assert_eq!(fuzzy_score("", "anything"), Some(0));
        assert_eq!(fuzzy_score("", ""), Some(0));
    }

    #[test]
    fn test_exact_match() {
        let score = fuzzy_score("main", "main").unwrap();
        assert!(score > 0);
    }

    #[test]
    fn test_no_match_returns_none() {
        assert!(fuzzy_score("xyz", "main.rs").is_none());
    }

    #[test]
    fn test_case_insensitive() {
        let lower = fuzzy_score("main", "Main.rs").unwrap();
        let upper = fuzzy_score("MAIN", "main.rs").unwrap();
        assert!(lower > 0);
        assert!(upper > 0);
    }

    #[test]
    fn test_subsequence_match() {
        assert!(fuzzy_score("mr", "main.rs").is_some());
        assert!(fuzzy_score("mrs", "main.rs").is_some());
    }

    #[test]
    fn test_prefix_match_scores_higher() {
        let prefix = fuzzy_score("ma", "main.rs").unwrap();
        let non_prefix = fuzzy_score("ai", "main.rs").unwrap();
        assert!(prefix > non_prefix);
    }

    #[test]
    fn test_consecutive_match_scores_higher() {
        let consecutive = fuzzy_score("main", "main.rs").unwrap();
        let spread = fuzzy_score("mins", "main.rs").unwrap();
        assert!(consecutive > spread);
    }

    #[test]
    fn test_separator_bonus() {
        // 'r' after '.' separator should score higher
        let after_sep = fuzzy_score("r", "main.rs").unwrap();
        let mid_word = fuzzy_score("a", "main.rs").unwrap();
        assert!(after_sep >= mid_word);
    }

    #[test]
    fn test_query_longer_than_candidate_returns_none() {
        assert!(fuzzy_score("very_long_query", "ab").is_none());
    }

    #[test]
    fn test_single_char_query() {
        assert!(fuzzy_score("m", "main.rs").is_some());
        assert!(fuzzy_score("z", "main.rs").is_none());
    }

    // --- all_commands ---

    #[test]
    fn test_all_commands_non_empty() {
        assert!(!all_commands().is_empty());
    }

    #[test]
    fn test_all_commands_have_valid_fields() {
        for cmd in all_commands() {
            assert!(!cmd.id.is_empty(), "command id must not be empty");
            assert!(!cmd.label.is_empty(), "command label must not be empty");
            assert!(
                cmd.id.starts_with("win.") || cmd.id.starts_with("app."),
                "command id must have win. or app. prefix: {}",
                cmd.id
            );
        }
    }

    #[test]
    fn test_all_commands_covers_categories() {
        let cmds = all_commands();
        assert!(cmds.iter().any(|c| c.category == CommandCategory::File));
        assert!(cmds.iter().any(|c| c.category == CommandCategory::Edit));
        assert!(cmds.iter().any(|c| c.category == CommandCategory::View));
        assert!(cmds.iter().any(|c| c.category == CommandCategory::App));
    }

    // --- search_commands ---

    #[test]
    fn test_search_commands_empty_query_returns_all() {
        let results = search_commands("", 100);
        assert_eq!(results.len(), all_commands().len());
    }

    #[test]
    fn test_search_commands_filter() {
        let results = search_commands("save", 10);
        assert!(!results.is_empty());
        for r in &results {
            match &r.item {
                SearchResultItem::Command(c) => {
                    assert!(
                        fuzzy_score("save", c.label).is_some(),
                        "'{}' should match 'save'",
                        c.label
                    );
                }
                SearchResultItem::File(_) => panic!("expected command"),
            }
        }
    }

    #[test]
    fn test_search_commands_max_results() {
        let results = search_commands("", 3);
        assert!(results.len() <= 3);
    }

    // --- FileIndex ---

    #[test]
    fn test_file_index_empty_roots() {
        let index = FileIndex::rebuild(&[]);
        assert!(index.files().is_empty());
    }

    #[test]
    fn test_file_index_single_dir() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("hello.rs"), "").unwrap();
        std::fs::write(dir.path().join("world.txt"), "").unwrap();

        let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
        assert_eq!(index.files().len(), 2);
    }

    #[test]
    fn test_file_index_nested_dirs() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "").unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();

        let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
        let names: Vec<&str> = index.files().iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"main.rs"));
        assert!(names.contains(&"Cargo.toml"));
    }

    #[test]
    fn test_file_index_skips_hidden_files() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("visible.txt"), "").unwrap();
        std::fs::write(dir.path().join(".hidden"), "").unwrap();

        let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
        assert_eq!(index.files().len(), 1);
        assert_eq!(index.files()[0].name, "visible.txt");
    }

    #[test]
    fn test_file_index_skips_directories() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();

        let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
        assert!(
            index.files().is_empty(),
            "directories should not appear in file index"
        );
    }

    #[test]
    fn test_file_index_workspace_root_set_correctly() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();

        let root = dir.path().to_path_buf();
        let index = FileIndex::rebuild(&[root.clone()]);
        assert_eq!(index.files()[0].workspace_root, root);
    }

    #[test]
    fn test_file_index_multiple_roots() {
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();
        std::fs::write(dir1.path().join("a.rs"), "").unwrap();
        std::fs::write(dir2.path().join("b.rs"), "").unwrap();

        let index = FileIndex::rebuild(&[dir1.path().to_path_buf(), dir2.path().to_path_buf()]);
        assert_eq!(index.files().len(), 2);
    }

    #[test]
    fn test_file_index_nonexistent_root() {
        let index = FileIndex::rebuild(&[PathBuf::from("/nonexistent/path")]);
        assert!(index.files().is_empty());
    }

    // --- FileIndex::search ---

    #[test]
    fn test_file_index_search_empty_query() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.rs"), "").unwrap();
        std::fs::write(dir.path().join("b.rs"), "").unwrap();

        let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
        let results = index.search("", 10);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_file_index_search_filters() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("main.rs"), "").unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();

        let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
        let results = index.search("main", 10);
        assert_eq!(results.len(), 1);
        match &results[0].item {
            SearchResultItem::File(f) => assert_eq!(f.name, "main.rs"),
            SearchResultItem::Command(_) => panic!("expected file"),
        }
    }

    #[test]
    fn test_file_index_search_respects_max() {
        let dir = TempDir::new().unwrap();
        for i in 0..20 {
            std::fs::write(dir.path().join(format!("file{i}.rs")), "").unwrap();
        }

        let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
        let results = index.search("file", 5);
        assert_eq!(results.len(), 5);
    }

    // --- search_all ---

    #[test]
    fn test_search_all_files_mode() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("save.rs"), "").unwrap();

        let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
        let results = search_all(&index, "save", SearchMode::Files, 50);
        for r in &results {
            assert!(matches!(r.item, SearchResultItem::File(_)));
        }
    }

    #[test]
    fn test_search_all_commands_mode() {
        let index = FileIndex::default();
        let results = search_all(&index, "save", SearchMode::Commands, 50);
        for r in &results {
            assert!(matches!(r.item, SearchResultItem::Command(_)));
        }
    }

    #[test]
    fn test_search_all_mixed_mode() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("save.rs"), "").unwrap();

        let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
        let results = search_all(&index, "save", SearchMode::All, 50);
        let has_file = results
            .iter()
            .any(|r| matches!(r.item, SearchResultItem::File(_)));
        let has_cmd = results
            .iter()
            .any(|r| matches!(r.item, SearchResultItem::Command(_)));
        assert!(has_file, "All mode should include files");
        assert!(has_cmd, "All mode should include commands");
    }

    #[test]
    fn test_search_all_sorted_by_score() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.rs"), "").unwrap();

        let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
        let results = search_all(&index, "", SearchMode::All, 50);
        for pair in results.windows(2) {
            assert!(pair[0].score >= pair[1].score);
        }
    }

    // --- Symlink cycle protection ---

    #[cfg(unix)]
    #[test]
    fn test_file_index_symlink_cycle_to_self() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("file.rs"), "").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        // sub/loop → parent directory (creates a cycle)
        std::os::unix::fs::symlink(dir.path(), dir.path().join("sub/loop")).unwrap();

        let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
        // Should find file.rs without infinite recursion
        let names: Vec<&str> = index.files().iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"file.rs"));
        // Should NOT have duplicates from following the cycle
        assert_eq!(
            names.iter().filter(|n| **n == "file.rs").count(),
            1,
            "file.rs should appear exactly once despite symlink cycle"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_file_index_symlink_cycle_mutual() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("a")).unwrap();
        std::fs::create_dir(dir.path().join("b")).unwrap();
        std::fs::write(dir.path().join("a/file_a.rs"), "").unwrap();
        std::fs::write(dir.path().join("b/file_b.rs"), "").unwrap();
        // a/link_to_b → ../b and b/link_to_a → ../a (mutual cycle)
        std::os::unix::fs::symlink(dir.path().join("b"), dir.path().join("a/link_to_b")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("a"), dir.path().join("b/link_to_a")).unwrap();

        let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
        let names: Vec<&str> = index.files().iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"file_a.rs"));
        assert!(names.contains(&"file_b.rs"));
        // No duplicates
        assert_eq!(names.iter().filter(|n| **n == "file_a.rs").count(), 1);
        assert_eq!(names.iter().filter(|n| **n == "file_b.rs").count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn test_file_index_symlink_to_sibling_no_cycle() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("real")).unwrap();
        std::fs::write(dir.path().join("real/target.rs"), "").unwrap();
        // link → real (not a cycle, just a shortcut)
        std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("link")).unwrap();

        let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
        let names: Vec<&str> = index.files().iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"target.rs"));
        // Canonical path dedup means it appears once even though accessible via two paths
        assert_eq!(names.iter().filter(|n| **n == "target.rs").count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn test_file_index_broken_symlink_skipped() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("real.rs"), "").unwrap();
        // Broken symlink pointing to nonexistent target
        std::os::unix::fs::symlink("/nonexistent/target", dir.path().join("broken")).unwrap();

        let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
        let names: Vec<&str> = index.files().iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"real.rs"));
        assert!(!names.contains(&"broken"));
    }

    #[cfg(unix)]
    #[test]
    fn test_file_index_deeply_nested_terminates() {
        let dir = TempDir::new().unwrap();
        // Create a moderately deep tree (not exceeding MAX_SCAN_DEPTH)
        let mut current = dir.path().to_path_buf();
        for i in 0..10 {
            current = current.join(format!("level{i}"));
            std::fs::create_dir(&current).unwrap();
        }
        std::fs::write(current.join("deep_file.rs"), "").unwrap();

        let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
        let names: Vec<&str> = index.files().iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"deep_file.rs"));
    }

    #[cfg(unix)]
    #[test]
    fn test_file_index_wine_dosdevices_pattern() {
        // Simulates the Wine/Proton dosdevices pattern that causes symlink loops
        let dir = TempDir::new().unwrap();
        let game_dir = dir.path().join("game");
        std::fs::create_dir(&game_dir).unwrap();
        std::fs::write(game_dir.join("game.exe"), "").unwrap();

        let dosdevices = game_dir.join("dosdevices");
        std::fs::create_dir(&dosdevices).unwrap();
        // c: → game dir (cycle back to parent)
        std::os::unix::fs::symlink(&game_dir, dosdevices.join("c:")).unwrap();
        // z: → filesystem root (would scan entire filesystem without protection)
        std::os::unix::fs::symlink("/", dosdevices.join("z:")).unwrap();

        let index = FileIndex::rebuild(&[game_dir.clone()]);
        // Should complete without hanging
        // game.exe should be found
        let names: Vec<&str> = index.files().iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"game.exe"));
        // c: cycle should be detected and broken
        assert_eq!(names.iter().filter(|n| **n == "game.exe").count(), 1);
    }
}
