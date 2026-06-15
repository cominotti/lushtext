// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the GTK-free command palette service: fuzzy scoring, command
//! filtering, and file-index maintenance.

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

fn indexed_file(folder: &Arc<PathBuf>, relative_path: &str) -> IndexedFile {
    IndexedFile::new(folder.join(relative_path), Arc::clone(folder))
}

fn file_paths(index: &FileIndex) -> Vec<PathBuf> {
    index.files().iter().map(|file| file.path.clone()).collect()
}

fn indexed_file_by_path<'a>(index: &'a FileIndex, path: &Path) -> &'a IndexedFile {
    index
        .files()
        .iter()
        .find(|file| file.path == path)
        .unwrap_or_else(|| panic!("expected indexed file {}", path.display()))
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
            .any(|command| command.category == CommandCategory::Notes)
    );
    assert!(
        commands
            .iter()
            .any(|command| command.category == CommandCategory::App)
    );
}

fn command_by_id(id: &str) -> &'static crate::model::palette::CommandDef {
    all_commands()
        .iter()
        .find(|command| command.id == id)
        .unwrap_or_else(|| panic!("expected command '{id}' in registry"))
}

#[test]
fn note_commands_use_notes_category_and_expected_sections() {
    let expected = [
        (
            "win.show-notes",
            "Browse Notes",
            Some("Ctrl+Alt+A"),
            NoteCommandSection::Browse,
        ),
        (
            "win.show-bookmarks",
            "Browse Bookmarks",
            Some("Ctrl+Alt+B"),
            NoteCommandSection::Browse,
        ),
        (
            "win.toggle-bookmark",
            "Toggle Bookmark",
            Some("Ctrl+F2"),
            NoteCommandSection::CurrentDocument,
        ),
        (
            "win.edit-bookmark-label",
            "Edit Bookmark",
            Some("Ctrl+Shift+F2"),
            NoteCommandSection::CurrentDocument,
        ),
        (
            "win.open-document-note",
            "Open Document Note",
            None,
            NoteCommandSection::CurrentDocument,
        ),
        (
            "win.next-bookmark",
            "Next Bookmark",
            Some("F2"),
            NoteCommandSection::BookmarkNavigation,
        ),
        (
            "win.prev-bookmark",
            "Previous Bookmark",
            Some("Shift+F2"),
            NoteCommandSection::BookmarkNavigation,
        ),
        (
            "win.open-folder-note",
            "Open Folder Note",
            None,
            NoteCommandSection::Workspace,
        ),
    ];

    for (id, label, shortcut, section) in expected {
        let command = command_by_id(id);
        assert_eq!(command.label, label);
        assert_eq!(command.shortcut, shortcut);
        assert_eq!(command.category, CommandCategory::Notes);
        assert_eq!(note_command_section(command), Some(section));
        assert!(is_note_command(command));
    }
}

#[test]
fn notes_category_and_section_mapping_stay_in_sync() {
    for command in all_commands() {
        assert_eq!(
            command.category == CommandCategory::Notes,
            note_command_section(command).is_some(),
            "Notes category and Notes section mapping drifted for {}",
            command.id,
        );
    }
}

#[test]
fn note_command_sections_keep_intent_order_and_labels() {
    assert_eq!(
        NoteCommandSection::ALL.map(NoteCommandSection::label),
        [
            "Browse",
            "Current Document",
            "Bookmark Navigation",
            "Workspace",
        ]
    );
}

#[test]
fn search_note_commands_only_returns_note_workflows() {
    let results = search_note_commands("", 100);
    let ids: Vec<&str> = results
        .iter()
        .filter_map(|result| match result.item {
            SearchResultItem::Command(command) => Some(command.id),
            SearchResultItem::OpenFile(_) | SearchResultItem::File(_) => None,
        })
        .collect();

    assert_eq!(ids.len(), 8);
    assert!(ids.iter().all(|id| is_note_command(command_by_id(id))));
    assert!(ids.contains(&"win.show-notes"));
    assert!(ids.contains(&"win.open-folder-note"));
}

#[test]
fn search_non_note_commands_excludes_note_workflows() {
    let results = search_non_note_commands("", 100);

    assert!(results.iter().all(|result| match result.item {
        SearchResultItem::Command(command) => !is_note_command(command),
        SearchResultItem::OpenFile(_) | SearchResultItem::File(_) => false,
    }));
    assert!(results.iter().any(|result| match result.item {
        SearchResultItem::Command(command) => command.id == "win.open-file",
        SearchResultItem::OpenFile(_) | SearchResultItem::File(_) => false,
    }));
}

#[test]
fn search_note_commands_for_section_filters_by_intent() {
    let results = search_note_commands_for_section(NoteCommandSection::Workspace, "", 10);
    let ids: Vec<&str> = results
        .iter()
        .filter_map(|result| match result.item {
            SearchResultItem::Command(command) => Some(command.id),
            SearchResultItem::OpenFile(_) | SearchResultItem::File(_) => None,
        })
        .collect();

    assert_eq!(ids, vec!["win.open-folder-note"]);
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
fn search_all_notes_mode_uses_cached_note_source_elsewhere() {
    let dir = TempDir::new().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("notes.rs"), "");

    let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
    let results = search_all(&index, "notes", SearchMode::Notes, 50);

    assert!(
        results.is_empty(),
        "note records require the cached note source, not file/command search"
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
fn file_index_multiple_folders_collects_files_from_each_folder() {
    let dir1 = TempDir::new().expect("expected operation to succeed");
    let dir2 = TempDir::new().expect("expected operation to succeed");
    fixture::write_text(&dir1.path().join("a.rs"), "");
    fixture::write_text(&dir2.path().join("b.rs"), "");

    let index = FileIndex::rebuild(&[dir1.path().to_path_buf(), dir2.path().to_path_buf()]);
    assert_eq!(index.files().len(), 2);
}

#[test]
fn file_index_overlapping_workspace_folders_deduplicate_files() {
    let dir = TempDir::new().expect("expected operation to succeed");
    let workspace_folder = dir.path().to_path_buf();
    let nested_folder = workspace_folder.join("src");
    let nested_file = nested_folder.join("main.rs");
    fixture::create_dir(&nested_folder);
    fixture::write_text(&workspace_folder.join("README.md"), "");
    fixture::write_text(&nested_file, "");

    let parent_first = FileIndex::rebuild(&[workspace_folder.clone(), nested_folder.clone()]);
    let nested_first = FileIndex::rebuild(&[nested_folder, workspace_folder]);

    for index in [parent_first, nested_first] {
        assert_eq!(index.len(), 2);
        assert_eq!(
            file_paths(&index)
                .iter()
                .filter(|path| *path == &nested_file)
                .count(),
            1
        );
    }
}

#[test]
fn file_index_uses_folder_order_for_primary_context_when_folders_overlap() {
    let dir = TempDir::new().expect("expected operation to succeed");
    let workspace_folder = dir.path().to_path_buf();
    let nested_folder = workspace_folder.join("src");
    let nested_file = nested_folder.join("main.rs");
    fixture::create_dir(&nested_folder);
    fixture::write_text(&workspace_folder.join("README.md"), "");
    fixture::write_text(&nested_file, "");

    let parent_first = FileIndex::rebuild(&[workspace_folder.clone(), nested_folder.clone()]);
    assert_eq!(
        indexed_file_by_path(&parent_first, &nested_file)
            .workspace_folder
            .as_ref(),
        &workspace_folder,
        "the parent folder should be the primary context when it appears first",
    );

    let nested_first = FileIndex::rebuild(&[nested_folder.clone(), workspace_folder]);
    assert_eq!(
        indexed_file_by_path(&nested_first, &nested_file)
            .workspace_folder
            .as_ref(),
        &nested_folder,
        "the nested folder should become primary context after reordering first",
    );
}

#[test]
fn file_index_duplicate_workspace_folders_deduplicate_files_for_aggregate_scope() {
    let dir = TempDir::new().expect("expected operation to succeed");
    let file = dir.path().join("main.rs");
    fixture::write_text(&file, "");

    let index = FileIndex::rebuild(&[dir.path().to_path_buf(), dir.path().to_path_buf()]);

    assert_eq!(index.len(), 1);
    assert_eq!(indexed_file_by_path(&index, &file).path, file);
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
fn file_index_workspace_folder_is_shared_with_arc() {
    let dir = TempDir::new().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("a.rs"), "");
    fixture::write_text(&dir.path().join("b.rs"), "");

    let index = FileIndex::rebuild(&[dir.path().to_path_buf()]);
    assert!(Arc::ptr_eq(
        &index.files()[0].workspace_folder,
        &index.files()[1].workspace_folder
    ));
}

#[test]
fn file_index_top_level_folder_named_as_ignored_dir_is_still_scanned() {
    let dir = TempDir::new().expect("expected operation to succeed");
    let folder = dir.path().join("node_modules");
    fixture::create_dir(&folder);
    fixture::write_text(&folder.join("index.js"), "");

    let index = FileIndex::rebuild(&[folder]);
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
fn file_index_symlink_escape_outside_folder_is_rejected() {
    let dir = TempDir::new().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("local.rs"), "");
    fixture::create_dir(&dir.path().join("escape"));
    fixture::symlink(Path::new("/"), &dir.path().join("escape/outside"));

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
    let folder = Arc::new(PathBuf::from("/workspace"));
    let mut below_limit = vec![
        indexed_file(&folder, "one.rs"),
        indexed_file(&folder, "two.rs"),
    ];
    let mut at_limit = vec![
        indexed_file(&folder, "one.rs"),
        indexed_file(&folder, "two.rs"),
        indexed_file(&folder, "three.rs"),
    ];
    let mut above_limit = vec![
        indexed_file(&folder, "one.rs"),
        indexed_file(&folder, "two.rs"),
        indexed_file(&folder, "three.rs"),
        indexed_file(&folder, "four.rs"),
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
fn workspace_folder_for_returns_matching_folder() {
    let dir = TempDir::new().expect("expected operation to succeed");
    fixture::create_dir(&dir.path().join("src"));
    let path = dir.path().join("src/lib.rs");
    fixture::write_text(&path, "");

    let folder = dir.path().to_path_buf();
    let index = FileIndex::rebuild(std::slice::from_ref(&folder));
    let matched_folder = index
        .workspace_folder_for(&path)
        .expect("expected operation to succeed");
    assert_eq!(*matched_folder, folder);
}

#[test]
fn search_commands_empty_query_returns_registry() {
    let results = search_commands("", 100);
    assert_eq!(results.len(), all_commands().len());
}

#[test]
fn file_index_nonexistent_folder_returns_empty_index() {
    let index = FileIndex::rebuild(&[PathBuf::from("/nonexistent/path")]);
    assert!(index.files().is_empty());
}

#[test]
fn file_index_len_and_empty_reflect_actual_file_count() {
    let empty = FileIndex::default();
    assert_eq!(empty.len(), 0);
    assert!(empty.is_empty());

    let folder = Arc::new(PathBuf::from("/workspace"));
    let index = FileIndex::from(vec![
        indexed_file(&folder, "src/main.rs"),
        indexed_file(&folder, "src/lib.rs"),
    ]);
    assert_eq!(index.len(), 2);
    assert!(!index.is_empty());
}

#[test]
fn add_file_registers_the_file_and_workspace_folder() {
    let folder = Arc::new(PathBuf::from("/workspace"));
    let path = folder.join("src/main.rs");
    let mut index = FileIndex::default();

    index.add_file(IndexedFile::new(path.clone(), Arc::clone(&folder)));

    assert_eq!(index.len(), 1);
    assert_eq!(index.files()[0].path, path);
    assert_eq!(
        index
            .workspace_folder_for(&folder.join("src/other.rs"))
            .expect("workspace folder should be registered")
            .as_ref(),
        folder.as_ref()
    );
}

#[test]
fn add_file_does_not_exceed_index_cap() {
    let folder = Arc::new(PathBuf::from("/workspace"));
    let files: Vec<IndexedFile> = (0..super::index::MAX_INDEXED_FILES)
        .map(|i| indexed_file(&folder, &format!("src/file-{i}.rs")))
        .collect();
    let mut index = FileIndex::from(files);

    index.add_file(indexed_file(&folder, "src/over-cap.rs"));

    assert_eq!(index.len(), super::index::MAX_INDEXED_FILES);
    assert!(
        !index.files().iter().any(|file| file.name == "over-cap.rs"),
        "incremental updates should respect the same cap as full rebuilds"
    );
}

#[test]
fn file_index_from_vec_preserves_files_and_workspace_folders() {
    let folder_a = Arc::new(PathBuf::from("/workspace-a"));
    let folder_b = Arc::new(PathBuf::from("/workspace-b"));
    let index = FileIndex::from(vec![
        indexed_file(&folder_a, "a.rs"),
        indexed_file(&folder_a, "nested/b.rs"),
        indexed_file(&folder_b, "c.rs"),
    ]);

    assert_eq!(index.len(), 3);
    assert_eq!(
        index
            .workspace_folder_for(&folder_a.join("nested/other.rs"))
            .expect("workspace A folder should be indexed")
            .as_ref(),
        folder_a.as_ref()
    );
    assert_eq!(
        index
            .workspace_folder_for(&folder_b.join("other.rs"))
            .expect("workspace B folder should be indexed")
            .as_ref(),
        folder_b.as_ref()
    );
}

#[test]
fn remove_path_removes_exact_and_descendant_paths_only() {
    let folder = Arc::new(PathBuf::from("/workspace"));
    let mut index = FileIndex::from(vec![
        indexed_file(&folder, "README.md"),
        indexed_file(&folder, "src/lib.rs"),
        indexed_file(&folder, "src/nested/mod.rs"),
        indexed_file(&folder, "tests/main.rs"),
    ]);

    index.remove_path(&folder.join("src"));
    assert_eq!(
        file_paths(&index),
        vec![folder.join("README.md"), folder.join("tests/main.rs")]
    );

    index.remove_path(&folder.join("README.md"));
    assert_eq!(file_paths(&index), vec![folder.join("tests/main.rs")]);
}

#[test]
fn remove_path_prunes_workspace_folders_after_large_removals() {
    let folder_a = Arc::new(PathBuf::from("/workspace-a"));
    let folder_b = Arc::new(PathBuf::from("/workspace-b"));
    let mut index = FileIndex::from(vec![
        indexed_file(&folder_a, "one.rs"),
        indexed_file(&folder_a, "two.rs"),
        indexed_file(&folder_a, "three.rs"),
        indexed_file(&folder_a, "four.rs"),
        indexed_file(&folder_b, "survivor.rs"),
    ]);

    index.remove_path(folder_a.as_path());

    assert_eq!(file_paths(&index), vec![folder_b.join("survivor.rs")]);
    assert!(
        index
            .workspace_folder_for(&folder_a.join("ghost.rs"))
            .is_none(),
        "removed folders should not stay addressable after pruning"
    );
    assert_eq!(
        index
            .workspace_folder_for(&folder_b.join("other.rs"))
            .expect("surviving folder should remain registered")
            .as_ref(),
        folder_b.as_ref()
    );
}

#[test]
fn rename_path_updates_exact_and_descendant_paths_only() {
    let folder = Arc::new(PathBuf::from("/workspace"));
    let mut index = FileIndex::from(vec![
        indexed_file(&folder, "src/main.rs"),
        indexed_file(&folder, "src/nested/lib.rs"),
        indexed_file(&folder, "src-sibling/file.rs"),
    ]);

    index.rename_path(&folder.join("src"), &folder.join("crate"));

    assert_eq!(
        file_paths(&index),
        vec![
            folder.join("crate/main.rs"),
            folder.join("crate/nested/lib.rs"),
            folder.join("src-sibling/file.rs"),
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
