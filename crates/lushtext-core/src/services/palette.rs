// SPDX-License-Identifier: GPL-3.0-or-later

//! Command palette service — fuzzy matching, file indexing, and command registry.
//!
//! Pure Rust with no GTK dependencies. All functions operate on domain types
//! from `model::palette` and are fully unit-testable without a display server.

use crate::model::palette::{
    CommandCategory, CommandDef, IndexedFile, ScoredResult, SearchMode, SearchResultItem,
};
use crate::services::file_tree;
use nucleo_matcher::pattern::{Atom, AtomKind, CaseMatching, Normalization};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
        let mut files = Vec::with_capacity(10_000);
        let mut visited = HashSet::new();
        for root in roots {
            let canonical_root = match root.canonicalize() {
                Ok(p) => p,
                Err(_) => continue,
            };
            let root_arc = Arc::new(root.clone());
            collect_files_recursive(
                root,
                &root_arc,
                &mut files,
                &mut visited,
                &canonical_root,
                0,
            );
        }
        if files.len() > MAX_INDEXED_FILES {
            tracing::warn!(
                "File index truncated: {} files exceeds {} limit",
                files.len(),
                MAX_INDEXED_FILES
            );
            files.truncate(MAX_INDEXED_FILES);
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

impl From<Vec<IndexedFile>> for FileIndex {
    fn from(files: Vec<IndexedFile>) -> Self {
        Self { files }
    }
}

/// Maximum recursion depth to prevent runaway scanning in deeply nested trees.
const MAX_SCAN_DEPTH: u32 = 64;

/// Maximum number of files to index. Beyond this, linear scan per query
/// takes >10ms on a single core.
const MAX_INDEXED_FILES: usize = 100_000;

/// Recursively scan a directory, collecting files (not directories) into `out`.
///
/// Three layers of protection against problematic filesystem structures:
/// 1. **Workspace containment**: symlinks whose canonical target is outside
///    `canonical_root` are skipped (prevents Wine `dosdevices/z:` → `/` from
///    scanning the entire filesystem).
/// 2. **Visited-path tracking**: canonical paths already seen are skipped
///    (breaks direct symlink cycles like `dosdevices/c:` → parent).
/// 3. **Depth limit**: recursion beyond `MAX_SCAN_DEPTH` is stopped
///    (catches pathological non-cyclic deep trees).
fn collect_files_recursive(
    dir: &Path,
    workspace_root: &Arc<PathBuf>,
    out: &mut Vec<IndexedFile>,
    visited: &mut HashSet<PathBuf>,
    canonical_root: &Path,
    depth: u32,
) {
    if depth > MAX_SCAN_DEPTH {
        tracing::warn!(
            "Skipping deeply nested directory (depth > {MAX_SCAN_DEPTH}): {}",
            dir.display()
        );
        return;
    }

    let canonical = match dir.canonicalize() {
        Ok(p) => p,
        Err(_) => return, // broken symlink or permission denied
    };

    if !canonical.starts_with(canonical_root) {
        return;
    }

    if !visited.insert(canonical) {
        return;
    }

    for (path, is_dir) in file_tree::scan_directory(dir) {
        if is_dir {
            collect_files_recursive(
                &path,
                workspace_root,
                out,
                visited,
                canonical_root,
                depth + 1,
            );
        } else {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            out.push(IndexedFile {
                path,
                name,
                workspace_root: Arc::clone(workspace_root),
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
// Fuzzy matching (nucleo SIMD-accelerated)
// ---------------------------------------------------------------------------

/// Score a fuzzy subsequence match of `query` against `candidate` using nucleo.
///
/// Returns `Some(score)` if `query` fuzzy-matches `candidate`, with higher
/// scores for better matches. Returns `None` if there is no match.
/// Empty query matches everything with score 0.
///
/// Uses SIMD-accelerated matching via nucleo-matcher (AVX2 on x86-64-v3,
/// NEON on aarch64).
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<u32> {
    if query.is_empty() {
        return Some(0);
    }
    let mut matcher = Matcher::new(Config::DEFAULT);
    let atom = Atom::new(
        query,
        CaseMatching::Ignore,
        Normalization::Smart,
        AtomKind::Fuzzy,
        false,
    );
    let mut buf = Vec::new();
    let haystack = Utf32Str::new(candidate, &mut buf);
    atom.score(haystack, &mut matcher).map(|s| s as u32)
}

/// Generic search helper: filter + score items using nucleo, sort by score
/// descending, cap at max. Reuses a single `Matcher` and char buffer across
/// all candidates for efficiency.
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
    if query.is_empty() {
        return items
            .map(|item| ScoredResult {
                item: wrap(item),
                score: 0,
            })
            .take(max)
            .collect();
    }

    let mut matcher = Matcher::new(Config::DEFAULT);
    let atom = Atom::new(
        query,
        CaseMatching::Ignore,
        Normalization::Smart,
        AtomKind::Fuzzy,
        false,
    );
    let mut buf = Vec::new();

    // Bounded min-heap: keeps only the top `max` results by score.
    // O(n log k) where k=max, vs O(n log n) for collect-sort-truncate.
    let mut heap: BinaryHeap<Reverse<ScoredResult<'a>>> = BinaryHeap::with_capacity(max + 1);

    for item in items {
        let text = get_text(item);
        buf.clear();
        let haystack = Utf32Str::new(text, &mut buf);
        if let Some(score) = atom.score(haystack, &mut matcher) {
            heap.push(Reverse(ScoredResult {
                item: wrap(item),
                score: score as u32,
            }));
            if heap.len() > max {
                heap.pop(); // remove lowest score
            }
        }
    }

    let mut results: Vec<ScoredResult<'a>> = heap.into_vec().into_iter().map(|r| r.0).collect();
    results.sort_by(|a, b| b.score.cmp(&a.score));
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

    /// Extract file names from an index for assertion.
    fn file_names(index: &FileIndex) -> Vec<&str> {
        index.files().iter().map(|f| f.name.as_str()).collect()
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
        let names = file_names(&index);
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
        assert_eq!(*index.files()[0].workspace_root, root);
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
        let names = file_names(&index);
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
        let names = file_names(&index);
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
        let names = file_names(&index);
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
        let names = file_names(&index);
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
        let names = file_names(&index);
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
        let names = file_names(&index);
        assert!(names.contains(&"game.exe"));
        // c: cycle detected, z: escape rejected — no duplicates or foreign files
        assert_eq!(names.iter().filter(|n| **n == "game.exe").count(), 1);
    }

    // --- Workspace containment (escape prevention) ---

    #[cfg(unix)]
    #[test]
    fn test_file_index_symlink_escape_to_root_rejected() {
        // A symlink to / should be completely skipped (the z: case)
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("local.rs"), "").unwrap();
        std::fs::create_dir(dir.path().join("escape")).unwrap();
        std::os::unix::fs::symlink("/", dir.path().join("escape/root")).unwrap();

        let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
        let names = file_names(&index);
        assert!(names.contains(&"local.rs"));
        // /etc/passwd, /usr/bin/* etc. must NOT appear
        assert!(
            !names.iter().any(|n| *n == "passwd" || *n == "hosts"),
            "system files must not leak into index: {:?}",
            names
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_file_index_symlink_escape_to_tmp_rejected() {
        // A symlink pointing to a sibling tempdir (outside workspace) is rejected
        let workspace = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        std::fs::write(workspace.path().join("inside.rs"), "").unwrap();
        std::fs::write(outside.path().join("outside.rs"), "").unwrap();

        std::os::unix::fs::symlink(outside.path(), workspace.path().join("escape")).unwrap();

        let index = FileIndex::rebuild(&[workspace.path().to_path_buf()]);
        let names = file_names(&index);
        assert!(names.contains(&"inside.rs"));
        assert!(
            !names.contains(&"outside.rs"),
            "files outside workspace root must not be indexed"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_file_index_symlink_within_workspace_allowed() {
        // A symlink that stays within the workspace root IS followed
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("real")).unwrap();
        std::fs::write(dir.path().join("real/allowed.rs"), "").unwrap();
        // link → real (both under workspace root)
        std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("shortcut")).unwrap();

        let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
        let names = file_names(&index);
        assert!(names.contains(&"allowed.rs"));
    }

    #[cfg(unix)]
    #[test]
    fn test_file_index_symlink_escape_no_child_scanning() {
        // When a symlink escapes, its children must not be scanned at all.
        // We verify this indirectly: no files from the escape target appear.
        let workspace = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        std::fs::create_dir(outside.path().join("deep")).unwrap();
        std::fs::write(outside.path().join("deep/secret.rs"), "").unwrap();

        std::os::unix::fs::symlink(outside.path(), workspace.path().join("escape")).unwrap();
        std::fs::write(workspace.path().join("safe.rs"), "").unwrap();

        let index = FileIndex::rebuild(&[workspace.path().to_path_buf()]);
        let names = file_names(&index);
        assert!(names.contains(&"safe.rs"));
        assert!(
            !names.contains(&"secret.rs"),
            "child of escaped dir must not be scanned"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_file_index_wine_full_dosdevices_simulation() {
        // Full Wine prefix simulation with multiple drive letters
        let dir = TempDir::new().unwrap();
        let prefix = dir.path().join("prefix");
        let drive_c = prefix.join("drive_c");
        std::fs::create_dir_all(&drive_c).unwrap();
        std::fs::write(drive_c.join("Program Files"), "").unwrap();

        let dosdevices = prefix.join("dosdevices");
        std::fs::create_dir(&dosdevices).unwrap();
        // c: → drive_c (within workspace)
        std::os::unix::fs::symlink(&drive_c, dosdevices.join("c:")).unwrap();
        // z: → / (escape to filesystem root)
        std::os::unix::fs::symlink("/", dosdevices.join("z:")).unwrap();
        // s: → parent (cycle)
        std::os::unix::fs::symlink(&prefix, dosdevices.join("s:")).unwrap();

        let index = FileIndex::rebuild(&[prefix.clone()]);
        let names = file_names(&index);
        // drive_c content should be found
        assert!(names.contains(&"Program Files"));
        // No duplicates from c: symlink (canonical dedup)
        assert_eq!(names.iter().filter(|n| **n == "Program Files").count(), 1);
        // No system files from z: escape
        assert!(!names.iter().any(|n| *n == "passwd" || *n == "hosts"));
    }

    #[cfg(unix)]
    #[test]
    fn test_file_index_permission_denied_dir_skipped() {
        // A directory with no read permission should be silently skipped
        // (not cause errors or stop scanning siblings)
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("readable.rs"), "").unwrap();
        let restricted = dir.path().join("restricted");
        std::fs::create_dir(&restricted).unwrap();
        std::fs::write(restricted.join("hidden.rs"), "").unwrap();

        // Remove read permission on the restricted directory
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&restricted, std::fs::Permissions::from_mode(0o000)).unwrap();

        let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
        let names = file_names(&index);
        assert!(
            names.contains(&"readable.rs"),
            "siblings of restricted dir should be indexed"
        );
        assert!(
            !names.contains(&"hidden.rs"),
            "files in restricted dir should not appear"
        );

        // Restore permissions for cleanup
        std::fs::set_permissions(&restricted, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // --- nucleo-based fuzzy scoring ---

    #[test]
    fn test_nucleo_exact_match_scores_high() {
        let score = fuzzy_score("main.rs", "main.rs").unwrap();
        let partial = fuzzy_score("mn", "main.rs").unwrap();
        assert!(
            score > partial,
            "exact match ({score}) should score higher than partial ({partial})"
        );
    }

    #[test]
    fn test_nucleo_scores_are_nonzero_for_matches() {
        assert!(fuzzy_score("m", "main.rs").unwrap() > 0);
        assert!(fuzzy_score("main", "main.rs").unwrap() > 0);
    }

    #[test]
    fn test_nucleo_no_match_returns_none() {
        assert!(fuzzy_score("xyz", "main.rs").is_none());
        assert!(fuzzy_score("zzz", "a.rs").is_none());
    }

    #[test]
    fn test_nucleo_case_insensitive() {
        let lower = fuzzy_score("main", "Main.rs");
        let upper = fuzzy_score("MAIN", "main.rs");
        assert!(lower.is_some(), "case-insensitive match should work");
        assert!(upper.is_some(), "case-insensitive match should work");
    }

    // --- Arc<PathBuf> workspace_root ---

    #[test]
    fn test_workspace_root_shared_across_files() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.rs"), "").unwrap();
        std::fs::write(dir.path().join("b.rs"), "").unwrap();

        let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
        assert_eq!(index.files().len(), 2);

        // Both files should share the same Arc (same pointer address)
        assert!(
            Arc::ptr_eq(
                &index.files()[0].workspace_root,
                &index.files()[1].workspace_root
            ),
            "files in the same workspace should share the Arc<PathBuf>"
        );
    }

    #[test]
    fn test_workspace_root_different_across_workspaces() {
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();
        std::fs::write(dir1.path().join("a.rs"), "").unwrap();
        std::fs::write(dir2.path().join("b.rs"), "").unwrap();

        let index = FileIndex::rebuild(&[dir1.path().to_path_buf(), dir2.path().to_path_buf()]);
        assert_eq!(index.files().len(), 2);

        // Files from different workspaces should have different Arcs
        assert!(
            !Arc::ptr_eq(
                &index.files()[0].workspace_root,
                &index.files()[1].workspace_root
            ),
            "files from different workspaces should not share Arc"
        );
    }

    // --- MAX_INDEXED_FILES ---

    #[test]
    fn test_max_indexed_files_constant_is_100k() {
        assert_eq!(MAX_INDEXED_FILES, 100_000);
    }

    // --- search_items with empty query returns all (capped) ---

    #[test]
    fn test_search_items_empty_query_returns_up_to_max() {
        let dir = TempDir::new().unwrap();
        for i in 0..100 {
            std::fs::write(dir.path().join(format!("file{i}.rs")), "").unwrap();
        }
        let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
        let results = index.search("", 10);
        assert_eq!(results.len(), 10, "empty query should cap at max");
    }

    #[test]
    fn test_search_items_empty_query_returns_all_when_under_max() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.rs"), "").unwrap();
        std::fs::write(dir.path().join("b.rs"), "").unwrap();

        let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
        let results = index.search("", 100);
        assert_eq!(
            results.len(),
            2,
            "empty query should return all when under max"
        );
    }
}
