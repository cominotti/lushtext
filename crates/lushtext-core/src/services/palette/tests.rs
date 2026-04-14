// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
use std::sync::Arc;

use tempfile::TempDir;

use super::*;
use crate::model::palette::{CommandCategory, SearchMode, SearchResultItem};

fn file_names(index: &FileIndex) -> Vec<&str> {
    index
        .files()
        .iter()
        .map(|file| file.name.as_str())
        .collect()
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
            SearchResultItem::File(_) => None,
        })
        .collect();
    assert!(labels.contains(&"Zoom In"));
    assert!(labels.contains(&"Zoom Out"));
    assert!(labels.contains(&"Reset Zoom"));
}

#[test]
fn search_all_mixed_mode_includes_files_and_commands() {
    let dir = TempDir::new().expect("expected operation to succeed");
    std::fs::write(dir.path().join("save.rs"), "").expect("expected operation to succeed");

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
fn file_index_skips_hidden_files_and_directories() {
    let dir = TempDir::new().expect("expected operation to succeed");
    std::fs::write(dir.path().join("visible.txt"), "").expect("expected operation to succeed");
    std::fs::write(dir.path().join(".hidden"), "").expect("expected operation to succeed");
    std::fs::create_dir(dir.path().join("subdir")).expect("expected operation to succeed");

    let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
    assert_eq!(index.files().len(), 1);
    assert_eq!(index.files()[0].name, "visible.txt");
}

#[test]
fn file_index_multiple_roots_collects_files_from_each_root() {
    let dir1 = TempDir::new().expect("expected operation to succeed");
    let dir2 = TempDir::new().expect("expected operation to succeed");
    std::fs::write(dir1.path().join("a.rs"), "").expect("expected operation to succeed");
    std::fs::write(dir2.path().join("b.rs"), "").expect("expected operation to succeed");

    let index = FileIndex::rebuild(&[dir1.path().to_path_buf(), dir2.path().to_path_buf()]);
    assert_eq!(index.files().len(), 2);
}

#[test]
fn file_index_search_respects_max() {
    let dir = TempDir::new().expect("expected operation to succeed");
    for i in 0..20 {
        std::fs::write(dir.path().join(format!("file{i}.rs")), "")
            .expect("expected operation to succeed");
    }

    let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
    let results = index.search("file", 5);
    assert_eq!(results.len(), 5);
}

#[test]
fn file_index_workspace_root_is_shared_with_arc() {
    let dir = TempDir::new().expect("expected operation to succeed");
    std::fs::write(dir.path().join("a.rs"), "").expect("expected operation to succeed");
    std::fs::write(dir.path().join("b.rs"), "").expect("expected operation to succeed");

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
    std::fs::create_dir(&root).expect("expected operation to succeed");
    std::fs::write(root.join("index.js"), "").expect("expected operation to succeed");

    let index = FileIndex::rebuild(&[root]);
    assert!(file_names(&index).contains(&"index.js"));
}

#[test]
fn file_index_skips_ignored_directories() {
    let dir = TempDir::new().expect("expected operation to succeed");
    std::fs::create_dir(dir.path().join("src")).expect("expected operation to succeed");
    std::fs::write(dir.path().join("src/main.rs"), "").expect("expected operation to succeed");
    for ignored in super::index::IGNORED_INDEX_DIRS {
        let ignored_dir = dir.path().join(ignored);
        std::fs::create_dir(&ignored_dir).expect("expected operation to succeed");
        std::fs::write(ignored_dir.join("ignored.txt"), "").expect("expected operation to succeed");
    }

    let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
    assert_eq!(index.len(), 1);
    assert!(file_names(&index).contains(&"main.rs"));
}

#[cfg(unix)]
#[test]
fn file_index_symlink_escape_to_root_is_rejected() {
    let dir = TempDir::new().expect("expected operation to succeed");
    std::fs::write(dir.path().join("local.rs"), "").expect("expected operation to succeed");
    std::fs::create_dir(dir.path().join("escape")).expect("expected operation to succeed");
    std::os::unix::fs::symlink("/", dir.path().join("escape/root"))
        .expect("expected operation to succeed");

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
    std::fs::write(dir.path().join("file.rs"), "").expect("expected operation to succeed");
    std::fs::create_dir(dir.path().join("sub")).expect("expected operation to succeed");
    std::os::unix::fs::symlink(dir.path(), dir.path().join("sub/loop"))
        .expect("expected operation to succeed");

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
fn empty_query_returns_all_results_up_to_cap() {
    let dir = TempDir::new().expect("expected operation to succeed");
    for i in 0..100 {
        std::fs::write(dir.path().join(format!("file{i}.rs")), "")
            .expect("expected operation to succeed");
    }

    let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
    let results = index.search("", 10);
    assert_eq!(results.len(), 10);
}

#[test]
fn workspace_root_for_returns_matching_root() {
    let dir = TempDir::new().expect("expected operation to succeed");
    std::fs::create_dir(dir.path().join("src")).expect("expected operation to succeed");
    let path = dir.path().join("src/lib.rs");
    std::fs::write(&path, "").expect("expected operation to succeed");

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
