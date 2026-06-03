// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tempfile::TempDir;

use super::*;
use crate::model::palette::{
    CommandCategory, IndexedFile, PaletteFileEntry, SearchMode, SearchResultItem,
};
use crate::services::filesystem::fixture;

fn file_names(index: &FileIndex) -> Vec<&str> {
    index
        .files()
        .iter()
        .map(|file| file.name.as_str())
        .collect()
}

fn indexed_file(root: &Arc<PathBuf>, relative_path: &str) -> IndexedFile {
    IndexedFile::new(root.join(relative_path), Arc::clone(root))
}

fn file_paths(index: &FileIndex) -> Vec<PathBuf> {
    index.files().iter().map(|file| file.path.clone()).collect()
}

#[test]
fn fuzzy_score_empty_query_matches_everything() {
    assert_eq!(fuzzy_score("", "anything"), Some(0));
    assert_eq!(fuzzy_score("", ""), Some(0));
}

#[test]
fn fuzzy_score_exact_match_beats_partial_match() {
    let exact = fuzzy_score("main.rs", "main.rs").expect("expected operation to succeed");
    let partial = fuzzy_score("mn", "main.rs").expect("expected operation to succeed");
    assert!(exact > partial);
}

#[test]
fn fuzzy_score_is_case_insensitive() {
    assert!(fuzzy_score("main", "Main.rs").is_some());
    assert!(fuzzy_score("MAIN", "main.rs").is_some());
}

#[test]
fn all_commands_cover_expected_categories() {
    let commands = all_commands();
    assert!(
        commands
            .iter()
            .any(|command| command.category == CommandCategory::File)
    );
    assert!(
        commands
            .iter()
            .any(|command| command.category == CommandCategory::Edit)
    );
    assert!(
        commands
            .iter()
            .any(|command| command.category == CommandCategory::View)
    );
    assert!(
        commands
            .iter()
            .any(|command| command.category == CommandCategory::App)
    );
}

#[test]
fn search_commands_zoom_finds_all_zoom_entries() {
    let results = search_commands("zoom", 10);
    let labels: Vec<&str> = results
        .iter()
        .filter_map(|result| match &result.item {
            SearchResultItem::Command(command) => Some(command.label),
            SearchResultItem::OpenFile(_) | SearchResultItem::File(_) => None,
        })
        .collect();
    assert!(labels.contains(&"Zoom In"));
    assert!(labels.contains(&"Zoom Out"));
    assert!(labels.contains(&"Reset Zoom"));
}

#[test]
fn search_all_mixed_mode_includes_files_and_commands() {
    let dir = TempDir::new().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("save.rs"), "");

    let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
    let results = search_all(&index, "save", SearchMode::All, 50);
    assert!(
        results
            .iter()
            .any(|result| matches!(result.item, SearchResultItem::File(_)))
    );
    assert!(
        results
            .iter()
            .any(|result| matches!(result.item, SearchResultItem::Command(_)))
    );
}

#[test]
fn search_all_mixed_mode_preserves_score_order_and_max_zero() {
    let dir = TempDir::new().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("save.rs"), "");

    let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
    assert!(search_all(&index, "save", SearchMode::All, 0).is_empty());

    let results = search_all(&index, "save", SearchMode::All, 10);
    assert!(
        results
            .windows(2)
            .all(|pair| pair[0].score >= pair[1].score),
        "merged file and command results should stay sorted by descending score: {:?}",
        results
            .iter()
            .map(|result| result.score)
            .collect::<Vec<_>>()
    );
}

#[test]
fn search_open_files_finds_active_documents_and_respects_max() {
    let files = vec![
        PaletteFileEntry::new(
            "main.rs".to_string(),
            "/workspace/src/main.rs".to_string(),
            PathBuf::from("/workspace/src/main.rs"),
        ),
        PaletteFileEntry::new(
            "manifest.json".to_string(),
            "/workspace/manifest.json".to_string(),
            PathBuf::from("/workspace/manifest.json"),
        ),
    ];

    let results = search_open_files(&files, "main", 1);

    assert_eq!(results.len(), 1);
    match results[0].item {
        SearchResultItem::OpenFile(file) => {
            assert_eq!(file.path, PathBuf::from("/workspace/src/main.rs"));
        }
        SearchResultItem::File(_) | SearchResultItem::Command(_) => {
            panic!("expected open-file result");
        }
    }
}

#[test]
fn file_index_skips_hidden_files_and_directories() {
    let dir = TempDir::new().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("visible.txt"), "");
    fixture::write_text(&dir.path().join(".hidden"), "");
    fixture::create_dir(&dir.path().join("subdir"));

    let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
    assert_eq!(index.files().len(), 1);
    assert_eq!(index.files()[0].name, "visible.txt");
}

#[test]
fn file_index_multiple_roots_collects_files_from_each_root() {
    let dir1 = TempDir::new().expect("expected operation to succeed");
    let dir2 = TempDir::new().expect("expected operation to succeed");
    fixture::write_text(&dir1.path().join("a.rs"), "");
    fixture::write_text(&dir2.path().join("b.rs"), "");

    let index = FileIndex::rebuild(&[dir1.path().to_path_buf(), dir2.path().to_path_buf()]);
    assert_eq!(index.files().len(), 2);
}

#[test]
fn file_index_search_respects_max() {
    let dir = TempDir::new().expect("expected operation to succeed");
    for i in 0..20 {
        fixture::write_text(&dir.path().join(format!("file{i}.rs")), "");
    }

    let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
    let results = index.search("file", 5);
    assert_eq!(results.len(), 5);
}

#[test]
fn file_index_workspace_root_is_shared_with_arc() {
    let dir = TempDir::new().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("a.rs"), "");
    fixture::write_text(&dir.path().join("b.rs"), "");

    let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
    assert!(Arc::ptr_eq(
        &index.files()[0].workspace_root,
        &index.files()[1].workspace_root
    ));
}

#[test]
fn file_index_root_named_as_ignored_dir_is_still_scanned() {
    let dir = TempDir::new().expect("expected operation to succeed");
    let root = dir.path().join("node_modules");
    fixture::create_dir(&root);
    fixture::write_text(&root.join("index.js"), "");

    let index = FileIndex::rebuild(&[root]);
    assert!(file_names(&index).contains(&"index.js"));
}

#[test]
fn file_index_skips_ignored_directories() {
    let dir = TempDir::new().expect("expected operation to succeed");
    fixture::create_dir(&dir.path().join("src"));
    fixture::write_text(&dir.path().join("src/main.rs"), "");
    for ignored in super::index::IGNORED_INDEX_DIRS {
        let ignored_dir = dir.path().join(ignored);
        fixture::create_dir(&ignored_dir);
        fixture::write_text(&ignored_dir.join("ignored.txt"), "");
    }

    let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
    assert_eq!(index.len(), 1);
    assert!(file_names(&index).contains(&"main.rs"));
}

#[cfg(unix)]
#[test]
fn file_index_symlink_escape_to_root_is_rejected() {
    let dir = TempDir::new().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("local.rs"), "");
    fixture::create_dir(&dir.path().join("escape"));
    fixture::symlink(Path::new("/"), &dir.path().join("escape/root"));

    let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
    let names = file_names(&index);
    assert!(names.contains(&"local.rs"));
    assert!(
        !names
            .iter()
            .any(|name| *name == "passwd" || *name == "hosts")
    );
}

#[cfg(unix)]
#[test]
fn file_index_symlink_cycle_does_not_duplicate_results() {
    let dir = TempDir::new().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("file.rs"), "");
    fixture::create_dir(&dir.path().join("sub"));
    fixture::symlink(dir.path(), &dir.path().join("sub/loop"));

    let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
    let names = file_names(&index);
    assert!(names.contains(&"file.rs"));
    assert_eq!(names.iter().filter(|name| **name == "file.rs").count(), 1);
}

#[test]
fn max_indexed_files_constant_remains_100k() {
    assert_eq!(super::index::MAX_INDEXED_FILES, 100_000);
}

#[test]
fn truncate_to_index_limit_only_truncates_when_count_exceeds_limit() {
    let root = Arc::new(PathBuf::from("/workspace"));
    let mut below_limit = vec![indexed_file(&root, "one.rs"), indexed_file(&root, "two.rs")];
    let mut at_limit = vec![
        indexed_file(&root, "one.rs"),
        indexed_file(&root, "two.rs"),
        indexed_file(&root, "three.rs"),
    ];
    let mut above_limit = vec![
        indexed_file(&root, "one.rs"),
        indexed_file(&root, "two.rs"),
        indexed_file(&root, "three.rs"),
        indexed_file(&root, "four.rs"),
    ];

    super::index::truncate_to_index_limit(&mut below_limit, 3);
    super::index::truncate_to_index_limit(&mut at_limit, 3);
    super::index::truncate_to_index_limit(&mut above_limit, 3);

    assert_eq!(below_limit.len(), 2);
    assert_eq!(at_limit.len(), 3);
    assert_eq!(above_limit.len(), 3);
    assert_eq!(above_limit[2].name, "three.rs");
}

#[test]
fn compaction_threshold_starts_below_three_quarters_remaining() {
    assert!(!super::index::should_compact_after_removal(4, 3));
    assert!(super::index::should_compact_after_removal(4, 2));
    assert!(!super::index::should_compact_after_removal(8, 6));
    assert!(super::index::should_compact_after_removal(8, 5));
}

#[test]
fn empty_query_returns_all_results_up_to_cap() {
    let dir = TempDir::new().expect("expected operation to succeed");
    for i in 0..100 {
        fixture::write_text(&dir.path().join(format!("file{i}.rs")), "");
    }

    let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
    let results = index.search("", 10);
    assert_eq!(results.len(), 10);
}

#[test]
fn workspace_root_for_returns_matching_root() {
    let dir = TempDir::new().expect("expected operation to succeed");
    fixture::create_dir(&dir.path().join("src"));
    let path = dir.path().join("src/lib.rs");
    fixture::write_text(&path, "");

    let root = dir.path().to_path_buf();
    let index = FileIndex::rebuild(std::slice::from_ref(&root));
    let matched_root = index
        .workspace_root_for(&path)
        .expect("expected operation to succeed");
    assert_eq!(*matched_root, root);
}

#[test]
fn search_commands_empty_query_returns_registry() {
    let results = search_commands("", 100);
    assert_eq!(results.len(), all_commands().len());
}

#[test]
fn file_index_nonexistent_root_returns_empty_index() {
    let index = FileIndex::rebuild(&[PathBuf::from("/nonexistent/path")]);
    assert!(index.files().is_empty());
}

#[test]
fn file_index_len_and_empty_reflect_actual_file_count() {
    let empty = FileIndex::default();
    assert_eq!(empty.len(), 0);
    assert!(empty.is_empty());

    let root = Arc::new(PathBuf::from("/workspace"));
    let index = FileIndex::from(vec![
        indexed_file(&root, "src/main.rs"),
        indexed_file(&root, "src/lib.rs"),
    ]);
    assert_eq!(index.len(), 2);
    assert!(!index.is_empty());
}

#[test]
fn add_file_registers_the_file_and_workspace_root() {
    let root = Arc::new(PathBuf::from("/workspace"));
    let path = root.join("src/main.rs");
    let mut index = FileIndex::default();

    index.add_file(IndexedFile::new(path.clone(), Arc::clone(&root)));

    assert_eq!(index.len(), 1);
    assert_eq!(index.files()[0].path, path);
    assert_eq!(
        index
            .workspace_root_for(&root.join("src/other.rs"))
            .expect("workspace root should be registered")
            .as_ref(),
        root.as_ref()
    );
}

#[test]
fn file_index_from_vec_preserves_files_and_workspace_roots() {
    let root_a = Arc::new(PathBuf::from("/workspace-a"));
    let root_b = Arc::new(PathBuf::from("/workspace-b"));
    let index = FileIndex::from(vec![
        indexed_file(&root_a, "a.rs"),
        indexed_file(&root_a, "nested/b.rs"),
        indexed_file(&root_b, "c.rs"),
    ]);

    assert_eq!(index.len(), 3);
    assert_eq!(
        index
            .workspace_root_for(&root_a.join("nested/other.rs"))
            .expect("workspace A root should be indexed")
            .as_ref(),
        root_a.as_ref()
    );
    assert_eq!(
        index
            .workspace_root_for(&root_b.join("other.rs"))
            .expect("workspace B root should be indexed")
            .as_ref(),
        root_b.as_ref()
    );
}

#[test]
fn remove_path_removes_exact_and_descendant_paths_only() {
    let root = Arc::new(PathBuf::from("/workspace"));
    let mut index = FileIndex::from(vec![
        indexed_file(&root, "README.md"),
        indexed_file(&root, "src/lib.rs"),
        indexed_file(&root, "src/nested/mod.rs"),
        indexed_file(&root, "tests/main.rs"),
    ]);

    index.remove_path(&root.join("src"));
    assert_eq!(
        file_paths(&index),
        vec![root.join("README.md"), root.join("tests/main.rs")]
    );

    index.remove_path(&root.join("README.md"));
    assert_eq!(file_paths(&index), vec![root.join("tests/main.rs")]);
}

#[test]
fn remove_path_prunes_workspace_roots_after_large_removals() {
    let root_a = Arc::new(PathBuf::from("/workspace-a"));
    let root_b = Arc::new(PathBuf::from("/workspace-b"));
    let mut index = FileIndex::from(vec![
        indexed_file(&root_a, "one.rs"),
        indexed_file(&root_a, "two.rs"),
        indexed_file(&root_a, "three.rs"),
        indexed_file(&root_a, "four.rs"),
        indexed_file(&root_b, "survivor.rs"),
    ]);

    index.remove_path(root_a.as_path());

    assert_eq!(file_paths(&index), vec![root_b.join("survivor.rs")]);
    assert!(
        index.workspace_root_for(&root_a.join("ghost.rs")).is_none(),
        "removed roots should not stay addressable after pruning"
    );
    assert_eq!(
        index
            .workspace_root_for(&root_b.join("other.rs"))
            .expect("surviving root should remain registered")
            .as_ref(),
        root_b.as_ref()
    );
}

#[test]
fn rename_path_updates_exact_and_descendant_paths_only() {
    let root = Arc::new(PathBuf::from("/workspace"));
    let mut index = FileIndex::from(vec![
        indexed_file(&root, "src/main.rs"),
        indexed_file(&root, "src/nested/lib.rs"),
        indexed_file(&root, "src-sibling/file.rs"),
    ]);

    index.rename_path(&root.join("src"), &root.join("crate"));

    assert_eq!(
        file_paths(&index),
        vec![
            root.join("crate/main.rs"),
            root.join("crate/nested/lib.rs"),
            root.join("src-sibling/file.rs"),
        ]
    );
}

#[test]
fn file_index_recursion_depth_includes_boundary_and_skips_beyond_it() {
    let dir = TempDir::new().expect("expected operation to succeed");
    let mut boundary_dir = dir.path().to_path_buf();
    for depth in 0..64 {
        boundary_dir.push(format!("level-{depth}"));
    }
    fixture::create_dir_all(&boundary_dir);
    fixture::write_text(&boundary_dir.join("boundary.txt"), "");

    let too_deep_dir = boundary_dir.join("level-64");
    fixture::create_dir(&too_deep_dir);
    fixture::write_text(&too_deep_dir.join("too-deep.txt"), "");

    let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
    let names = file_names(&index);
    assert!(names.contains(&"boundary.txt"));
    assert!(!names.contains(&"too-deep.txt"));
}
