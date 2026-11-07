// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the GTK-free command palette service: fuzzy scoring, command
//! filtering, and file-index maintenance.

use crate::model::workspace_visibility::WorkspaceEntryVisibility;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tempfile::TempDir;

use super::*;
use crate::model::palette::{
    CommandCategory, IndexedFile, PaletteCategory, PaletteFileEntry, PaletteFileIdentity,
    PaletteFileIdentityFailure, PaletteNoteCategory, PaletteNoteEntry, PaletteSearchRow,
    SearchMode, SearchResultItem,
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
    let path = folder.join(relative_path);
    IndexedFile::new(
        path.clone(),
        PaletteFileIdentity::canonical(path),
        Arc::clone(folder),
    )
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
            SearchResultItem::OpenFile(_)
            | SearchResultItem::File(_)
            | SearchResultItem::Note(_) => None,
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
        SearchResultItem::OpenFile(_) | SearchResultItem::File(_) | SearchResultItem::Note(_) =>
            false,
    }));
    assert!(results.iter().any(|result| match result.item {
        SearchResultItem::Command(command) => command.id == "win.open-file",
        SearchResultItem::OpenFile(_) | SearchResultItem::File(_) | SearchResultItem::Note(_) =>
            false,
    }));
}

#[test]
fn search_note_commands_for_section_filters_by_intent() {
    let results = search_note_commands_for_section(NoteCommandSection::Workspace, "", 10);
    let ids: Vec<&str> = results
        .iter()
        .filter_map(|result| match result.item {
            SearchResultItem::Command(command) => Some(command.id),
            SearchResultItem::OpenFile(_)
            | SearchResultItem::File(_)
            | SearchResultItem::Note(_) => None,
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
            SearchResultItem::OpenFile(_)
            | SearchResultItem::File(_)
            | SearchResultItem::Note(_) => None,
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
            PaletteFileIdentity::canonical(PathBuf::from("/workspace/src/main.rs")),
        ),
        PaletteFileEntry::new(
            "manifest.json".to_string(),
            "/workspace/manifest.json".to_string(),
            PathBuf::from("/workspace/manifest.json"),
            PaletteFileIdentity::canonical(PathBuf::from("/workspace/manifest.json")),
        ),
    ];

    let results = search_open_files(&files, "main", 1);

    assert_eq!(results.len(), 1);
    match results[0].item {
        SearchResultItem::OpenFile(file) => {
            assert_eq!(file.path, PathBuf::from("/workspace/src/main.rs"));
        }
        SearchResultItem::File(_) | SearchResultItem::Command(_) | SearchResultItem::Note(_) => {
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
fn file_index_visibility_reveals_dotfiles_but_keeps_excluded_and_internal_skips() {
    let dir = TempDir::new().expect("expected operation to succeed");
    fixture::write_text(&dir.path().join("visible.txt"), "");
    fixture::write_text(&dir.path().join(".env"), "");
    fixture::create_dir_all(&dir.path().join(".github").join("workflows"));
    fixture::write_text(&dir.path().join(".github/workflows/ci.yml"), "");
    fixture::create_dir_all(&dir.path().join(".git"));
    fixture::write_text(&dir.path().join(".git/HEAD"), "");
    fixture::create_dir_all(&dir.path().join("node_modules"));
    fixture::write_text(&dir.path().join("node_modules/pkg.js"), "");
    fixture::create_dir_all(&dir.path().join("vendor"));
    fixture::write_text(&dir.path().join("vendor/lib.rs"), "");

    let names = |index: &FileIndex| {
        let mut names = index
            .files()
            .iter()
            .map(|file| file.name.clone())
            .collect::<Vec<_>>();
        names.sort();
        names
    };

    let hidden_off = FileIndex::rebuild(&[dir.path().to_path_buf()]);
    assert_eq!(names(&hidden_off), vec!["visible.txt"]);

    let hidden_on = FileIndex::rebuild_with_visibility(
        &[dir.path().to_path_buf()],
        &WorkspaceEntryVisibility::new(true, [".git"]),
    );
    assert_eq!(
        names(&hidden_on),
        vec![".env", "ci.yml", "visible.txt"],
        "dotfiles appear, .git stays excluded, node_modules/vendor stay internally skipped"
    );

    let excluded_env = FileIndex::rebuild_with_visibility(
        &[dir.path().to_path_buf()],
        &WorkspaceEntryVisibility::new(true, [".git", ".env"]),
    );
    assert_eq!(names(&excluded_env), vec!["ci.yml", "visible.txt"]);
}

#[test]
fn file_index_always_indexes_dot_named_and_excluded_named_workspace_folders() {
    let dir = TempDir::new().expect("expected operation to succeed");
    let dot_folder = dir.path().join(".config");
    fixture::create_dir_all(&dot_folder);
    fixture::write_text(&dot_folder.join("settings.toml"), "");
    fixture::write_text(&dot_folder.join(".secret"), "");
    let excluded_folder = dir.path().join("vendor");
    fixture::create_dir_all(&excluded_folder.join("vendor"));
    fixture::write_text(&excluded_folder.join("lib.rs"), "");
    fixture::write_text(&excluded_folder.join("vendor/nested.rs"), "");

    let index = FileIndex::rebuild_with_visibility(
        &[dot_folder, excluded_folder],
        &WorkspaceEntryVisibility::new(false, [".git", "vendor"]),
    );
    let mut names = index
        .files()
        .iter()
        .map(|file| file.name.clone())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        vec!["lib.rs", "settings.toml"],
        "folders are indexed as roots while nested excluded/dot entries follow the rule"
    );
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

#[cfg(unix)]
#[test]
fn file_index_canonical_alias_roots_keep_first_folder_precedence() {
    let dir = TempDir::new().expect("temp directory");
    let real_root = dir.path().join("real");
    let alias_root = dir.path().join("alias");
    fixture::create_dir(&real_root);
    fixture::write_text(&real_root.join("main.rs"), "");
    fixture::symlink(&real_root, &alias_root);

    let index = FileIndex::rebuild(&[alias_root.clone(), real_root]);

    assert_eq!(index.len(), 1);
    assert_eq!(index.files()[0].path, alias_root.join("main.rs"));
    assert_eq!(index.files()[0].workspace_folder.as_ref(), &alias_root);
    assert_eq!(
        index.files()[0].identity.canonical_path(),
        Some(
            crate::services::filesystem::metadata::canonical_path(&alias_root.join("main.rs"))
                .expect("canonical alias")
                .as_path()
        )
    );
}

#[test]
fn file_index_retains_typed_identity_failure_without_cross_path_deduplication() {
    let folder = Arc::new(PathBuf::from("/workspace"));
    let missing_a = folder.join("missing-a.rs");
    let missing_b = folder.join("missing-b.rs");
    let mut index = FileIndex::default();

    index.add_path(missing_a.clone(), Arc::clone(&folder));
    index.add_path(missing_b.clone(), Arc::clone(&folder));

    assert_eq!(index.len(), 2);
    assert_eq!(file_paths(&index), vec![missing_a, missing_b]);
    assert!(index.files().iter().all(|file| matches!(
        file.identity,
        PaletteFileIdentity::Unavailable(PaletteFileIdentityFailure::NotFound)
    )));
}

#[test]
fn grouped_search_excludes_open_identity_before_top_one_workspace_retention() {
    let root = Arc::new(PathBuf::from("/workspace"));
    let canonical_best = PathBuf::from("/canonical/target.rs");
    let workspace_alias = root.join("target.rs");
    let fallback = root.join("target_fallback.rs");
    let index = FileIndex::from(vec![
        IndexedFile::new(
            workspace_alias.clone(),
            PaletteFileIdentity::canonical(canonical_best.clone()),
            Arc::clone(&root),
        ),
        IndexedFile::new(
            fallback.clone(),
            PaletteFileIdentity::canonical(fallback.clone()),
            Arc::clone(&root),
        ),
    ]);
    let open_tabs = vec![PaletteFileEntry::new(
        "target.rs".to_string(),
        "/open/alias/target.rs".to_string(),
        PathBuf::from("/open/alias/target.rs"),
        PaletteFileIdentity::canonical(canonical_best),
    )];
    let cancellation = PaletteSearchCancellation::default();

    let PaletteSearchOutcome::Complete { value: rows, .. } = grouped_search(
        GroupedSearchInput {
            index: &index,
            open_tabs: &open_tabs,
            note_entries: &[],
            workspace_group_label: "Selected Workspace",
            query: "target",
            mode: SearchMode::Files,
            max_per_source: 1,
        },
        &cancellation,
    ) else {
        panic!("fresh grouped search must complete");
    };

    let file_paths: Vec<PathBuf> = rows
        .into_iter()
        .filter_map(|row| match row {
            PaletteSearchRow::File { file_path, .. } => Some(file_path),
            PaletteSearchRow::Header { .. }
            | PaletteSearchRow::Command { .. }
            | PaletteSearchRow::Note { .. }
            | PaletteSearchRow::More { .. } => None,
        })
        .collect();
    assert_eq!(
        file_paths,
        vec![PathBuf::from("/open/alias/target.rs"), fallback]
    );
    assert!(!file_paths.contains(&workspace_alias));
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
fn max_indexed_files_constant_is_the_power_of_two_above_100k() {
    assert_eq!(super::index::MAX_INDEXED_FILES, 128 * 1024);
    assert_eq!(super::index::MAX_INDEXED_DIRECTORIES, 128 * 1024);
}

#[test]
fn empty_file_index_releases_a_large_reused_capacity_hint() {
    let index = FileIndex::rebuild_with_hint(&[], MAX_INDEXED_FILES);

    assert!(index.is_empty());
    assert_eq!(index.retained_byte_weight(), 0);
}

#[test]
fn file_index_retained_weight_charges_path_capacities_and_canonical_identity() {
    let mut raw_path = PathBuf::from("/workspace/src/main.rs");
    raw_path.reserve(8 * 1024);
    let mut canonical_path = PathBuf::from("/canonical/workspace/src/main.rs");
    canonical_path.reserve(16 * 1024);
    let mut folder_path = PathBuf::from("/workspace");
    folder_path.reserve(4 * 1024);
    let raw_capacity = raw_path.capacity();
    let canonical_capacity = canonical_path.capacity();
    let folder_capacity = folder_path.capacity();
    let folder = Arc::new(folder_path);
    let index = FileIndex::from(vec![IndexedFile::new(
        raw_path,
        PaletteFileIdentity::canonical(canonical_path),
        folder,
    )]);

    let retained = index.retained_byte_weight();
    let required_path_bytes = raw_capacity
        .saturating_add(canonical_capacity)
        .saturating_add(folder_capacity);
    assert!(retained >= u64::try_from(required_path_bytes).unwrap_or(u64::MAX));
    assert!(retained <= MAX_FILE_INDEX_RETAINED_BYTES);
}

#[test]
fn file_index_build_metrics_bound_unicode_paths_and_complete_installed_graph() {
    let dir = TempDir::new().expect("file-index byte-metric tempdir");
    let mut parent = dir.path().to_path_buf();
    for depth in 0..24 {
        parent = parent.join(format!("unicode-界🙂-{depth:02}"));
        fixture::create_dir(&parent);
        fixture::write_text(&parent.join(format!("source-界-{depth:02}.rs")), "");
    }
    let cancellation = PaletteSearchCancellation::default();

    let FileIndexBuildOutcome::Complete { index, metrics } =
        FileIndex::rebuild_cancellable_with_hint(
            &[dir.path().to_path_buf()],
            1,
            &cancellation,
            &WorkspaceEntryVisibility::default(),
        )
    else {
        panic!("fresh byte-bounded index should complete");
    };

    assert_eq!(index.len(), 24);
    assert_eq!(metrics.retained_index_bytes, index.retained_byte_weight());
    assert_eq!(metrics.current_build_bytes, metrics.retained_index_bytes);
    assert!(metrics.retained_index_bytes <= super::MAX_FILE_INDEX_RETAINED_BYTES);
    assert!(metrics.peak_build_bytes <= super::MAX_FILE_INDEX_BUILD_RETAINED_BYTES);
    assert!(metrics.peak_build_bytes >= metrics.retained_index_bytes);
}

#[test]
fn file_index_build_limit_accepts_exact_peak_and_truncates_deterministically_one_under() {
    let dir = TempDir::new().expect("file-index build-limit tempdir");
    let mut parent = dir.path().to_path_buf();
    for depth in 0..16 {
        parent = parent.join(format!("long-unicode-界🙂-{depth:02}"));
        fixture::create_dir(&parent);
        fixture::write_text(&parent.join(format!("source-界🙂-{depth:02}.rs")), "");
    }
    let roots = [dir.path().to_path_buf()];
    let cancellation = PaletteSearchCancellation::default();
    let FileIndexBuildOutcome::Complete {
        metrics: baseline, ..
    } = FileIndex::rebuild_cancellable_with_build_limit_for_test(
        &roots,
        64,
        64,
        super::MAX_FILE_INDEX_BUILD_RETAINED_BYTES,
        &cancellation,
    )
    else {
        panic!("fresh build-limit baseline should complete");
    };
    let exact_limit = baseline.peak_build_bytes;

    let FileIndexBuildOutcome::Complete {
        index: exact,
        metrics: exact_metrics,
    } = FileIndex::rebuild_cancellable_with_build_limit_for_test(
        &roots,
        64,
        64,
        exact_limit,
        &PaletteSearchCancellation::default(),
    )
    else {
        panic!("exact build-byte limit should complete");
    };
    assert_eq!(exact.len(), 16);
    assert_eq!(exact_metrics.truncation, None);
    assert!(exact_metrics.peak_build_bytes <= exact_limit);

    let run_one_under = || {
        FileIndex::rebuild_cancellable_with_build_limit_for_test(
            &roots,
            64,
            64,
            exact_limit.saturating_sub(1),
            &PaletteSearchCancellation::default(),
        )
    };
    let FileIndexBuildOutcome::Complete {
        index: first,
        metrics: first_metrics,
    } = run_one_under()
    else {
        panic!("one-under build-byte run should return its usable prefix");
    };
    let FileIndexBuildOutcome::Complete {
        index: second,
        metrics: second_metrics,
    } = run_one_under()
    else {
        panic!("repeated one-under run should return its usable prefix");
    };

    assert_eq!(
        first_metrics.truncation,
        Some(FileIndexTruncationReason::BuildByteLimit)
    );
    assert_eq!(second_metrics.truncation, first_metrics.truncation);
    assert!(first_metrics.peak_build_bytes < exact_limit);
    assert!(second_metrics.peak_build_bytes < exact_limit);
    assert_eq!(
        first
            .files()
            .iter()
            .map(|file| file.path.clone())
            .collect::<Vec<_>>(),
        second
            .files()
            .iter()
            .map(|file| file.path.clone())
            .collect::<Vec<_>>()
    );
    assert!(first.len() < exact.len());
}

#[test]
fn file_index_directory_limit_is_independent_from_file_limit() {
    let dir = TempDir::new().expect("temp directory");
    for index in 0..8 {
        fixture::create_dir(&dir.path().join(format!("empty-{index:02}")));
    }
    fixture::write_text(&dir.path().join("root.rs"), "");
    let cancellation = PaletteSearchCancellation::default();

    let FileIndexBuildOutcome::Complete { index, metrics } =
        FileIndex::rebuild_cancellable_with_limits_for_test(
            &[dir.path().to_path_buf()],
            64,
            4,
            &cancellation,
        )
    else {
        panic!("fresh index build must complete");
    };

    assert!(index.len() < 64);
    assert_eq!(metrics.retained_files, index.len());
    assert_eq!(metrics.scanned_directories, 4);
    assert_eq!(metrics.peak_retained_directories, 4);
    assert_eq!(
        metrics.truncation,
        Some(FileIndexTruncationReason::DirectoryRetentionLimit)
    );
}

#[test]
fn file_index_directory_limit_stops_before_scanning_another_batch() {
    let dir = TempDir::new().expect("temp directory");
    for index in 0..32 {
        let child = dir.path().join(format!("branch-{index:02}"));
        fixture::create_dir(&child);
        fixture::create_dir(&child.join("nested"));
    }
    let cancellation = PaletteSearchCancellation::default();

    let FileIndexBuildOutcome::Complete { metrics, .. } =
        FileIndex::rebuild_cancellable_with_limits_for_test(
            &[dir.path().to_path_buf()],
            128,
            5,
            &cancellation,
        )
    else {
        panic!("fresh index build must complete");
    };

    assert_eq!(metrics.scanned_directories, 5);
    assert_eq!(metrics.peak_retained_directories, 5);
    assert_eq!(
        metrics.truncation,
        Some(FileIndexTruncationReason::DirectoryRetentionLimit)
    );
}

#[cfg(unix)]
#[test]
fn file_index_directory_limit_counts_canonical_alias_once() {
    let dir = TempDir::new().expect("temp directory");
    let workspace = dir.path().join("workspace");
    fixture::create_dir(&workspace);
    fixture::write_text(&workspace.join("file.rs"), "");
    let alias = dir.path().join("alias");
    fixture::symlink(&workspace, &alias);
    let cancellation = PaletteSearchCancellation::default();

    let FileIndexBuildOutcome::Complete { index, metrics } =
        FileIndex::rebuild_cancellable_with_limits_for_test(
            &[workspace, alias],
            8,
            1,
            &cancellation,
        )
    else {
        panic!("fresh index build must complete");
    };

    assert_eq!(index.len(), 1);
    assert_eq!(metrics.scanned_directories, 1);
    assert_eq!(metrics.peak_retained_directories, 1);
    assert_eq!(metrics.truncation, None);
}

#[test]
fn file_index_flat_directory_retains_only_the_remaining_capacity() {
    let dir = TempDir::new().expect("temp directory");
    for index in 0..64 {
        fixture::write_text(&dir.path().join(format!("file-{index:03}.rs")), "");
    }
    let cancellation = PaletteSearchCancellation::default();

    let FileIndexBuildOutcome::Complete { index, metrics } =
        FileIndex::rebuild_cancellable_for_test(&[dir.path().to_path_buf()], 8, &cancellation)
    else {
        panic!("fresh index build must complete");
    };

    assert_eq!(index.len(), 8);
    assert_eq!(metrics.retained_files, 8);
    assert_eq!(metrics.peak_retained_directories, 1);
    assert_eq!(metrics.examined_directory_entries, 64);
    assert!(metrics.peak_retained_directory_entries <= 8);
    assert_eq!(
        metrics.truncation,
        Some(FileIndexTruncationReason::FileLimit)
    );
}

#[test]
fn file_index_deep_wide_traversal_accounts_for_the_global_pending_worklist() {
    let dir = TempDir::new().expect("temp directory");
    let mut level = dir.path().to_path_buf();
    for depth in 0..4 {
        for sibling in 0..7 {
            fixture::create_dir(&level.join(format!("branch-{depth}-{sibling}")));
        }
        level = level.join(format!("zz-chain-{depth}"));
        fixture::create_dir(&level);
    }
    let cancellation = PaletteSearchCancellation::default();
    let file_limit = 8;
    let directory_limit = 32;

    let FileIndexBuildOutcome::Complete { metrics, .. } =
        FileIndex::rebuild_cancellable_with_limits_for_test(
            &[dir.path().to_path_buf()],
            file_limit,
            directory_limit,
            &cancellation,
        )
    else {
        panic!("fresh index build must complete");
    };

    assert!(metrics.peak_retained_directory_entries > file_limit);
    assert!(
        metrics.peak_retained_directory_entries <= file_limit + directory_limit,
        "one scan batch plus the global pending worklist must bound live traversal entries"
    );
    assert!(metrics.peak_retained_directories <= directory_limit);
}

#[test]
fn file_index_cancelled_outcome_releases_partial_inventory() {
    let dir = TempDir::new().expect("temp directory");
    for index in 0..64 {
        fixture::write_text(&dir.path().join(format!("file-{index:02}.rs")), "");
    }
    let cancellation = PaletteSearchCancellation::default();
    cancellation.cancel_after_checks_for_test(140);

    let outcome =
        FileIndex::rebuild_cancellable_for_test(&[dir.path().to_path_buf()], 64, &cancellation);

    let FileIndexBuildOutcome::Cancelled { metrics } = outcome else {
        panic!("mid-traversal cancellation must not publish a partial index");
    };
    assert!(metrics.retained_files > 0);
    assert!(metrics.retained_files < 64);
    assert_eq!(metrics.current_build_bytes, 0);
    assert_eq!(metrics.retained_index_bytes, 0);
    assert!(metrics.peak_build_bytes > 0);
    assert!(metrics.peak_build_bytes <= super::MAX_FILE_INDEX_BUILD_RETAINED_BYTES);
}

#[test]
fn file_index_coordinator_retains_only_active_and_latest_compact_request() {
    let mut coordinator = FileIndexBuildCoordinator::default();
    let request = |name: &str| FileIndexBuildRequest {
        visibility: WorkspaceEntryVisibility::default(),
        workspace_folders: Arc::from([PathBuf::from(name)]),
        capacity_hint: 64,
    };
    let first = coordinator
        .submit(request("first"))
        .expect("first request starts");

    assert!(coordinator.submit(request("middle")).is_none());
    assert!(coordinator.submit(request("latest")).is_none());
    assert!(first.cancellation.is_cancelled());
    assert_eq!(coordinator.snapshot().active, 1);
    assert_eq!(coordinator.snapshot().pending, 1);

    let latest = coordinator
        .finish(first.generation)
        .expect("latest request starts next");
    assert_eq!(
        latest.request.workspace_folders.as_ref(),
        &[PathBuf::from("latest")]
    );
    assert_eq!(coordinator.snapshot().started, 2);
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

    index.add_file(IndexedFile::new(
        path.clone(),
        PaletteFileIdentity::canonical(path.clone()),
        Arc::clone(&folder),
    ));

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
fn add_file_replay_does_not_duplicate_an_existing_path() {
    let folder = Arc::new(PathBuf::from("/workspace"));
    let file = indexed_file(&folder, "src/main.rs");
    let mut index = FileIndex::from(vec![file.clone()]);

    index.add_file(file);

    assert_eq!(index.len(), 1);
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

// --- drill-down-palette-categories: More rows, counts, category ranking ---

fn synthetic_index(count: usize) -> FileIndex {
    let root = Arc::new(PathBuf::from("/workspace"));
    FileIndex::from(
        (0..count)
            .map(|n| {
                let path = root.join(format!("dir{}/file{n}.rs", n % 7));
                IndexedFile::new(
                    path.clone(),
                    PaletteFileIdentity::canonical(path),
                    Arc::clone(&root),
                )
            })
            .collect::<Vec<_>>(),
    )
}

fn open_tab_for(file: &IndexedFile) -> PaletteFileEntry {
    PaletteFileEntry::new(
        file.name.clone(),
        file.path.display().to_string(),
        file.path.clone(),
        file.identity.clone(),
    )
}

fn more_rows(rows: &[PaletteSearchRow]) -> Vec<(PaletteCategory, usize)> {
    rows.iter()
        .filter_map(|row| match row {
            PaletteSearchRow::More { category, hidden } => Some((*category, *hidden)),
            _ => None,
        })
        .collect()
}

fn grouped(
    index: &FileIndex,
    open_tabs: &[PaletteFileEntry],
    notes: &[PaletteNoteEntry],
    query: &str,
    mode: SearchMode,
    max: usize,
) -> Vec<PaletteSearchRow> {
    let cancellation = PaletteSearchCancellation::default();
    let PaletteSearchOutcome::Complete { value, .. } = grouped_search(
        GroupedSearchInput {
            index,
            open_tabs,
            note_entries: notes,
            workspace_group_label: "All Workspaces",
            query,
            mode,
            max_per_source: max,
        },
        &cancellation,
    ) else {
        panic!("fresh grouped search must complete");
    };
    value
}

#[test]
fn empty_query_workspace_group_ends_with_an_exact_more_row() {
    let index = synthetic_index(1_000);
    let open_tabs: Vec<_> = index.files()[..32].iter().map(open_tab_for).collect();
    let rows = grouped(&index, &open_tabs, &[], "", SearchMode::Files, 64);
    assert_eq!(
        more_rows(&rows),
        vec![(PaletteCategory::WorkspaceFiles, 1_000 - 32 - 64)]
    );
    // The More row is the workspace group's last row.
    assert!(matches!(rows.last(), Some(PaletteSearchRow::More { .. })));
}

#[test]
fn empty_query_without_open_tabs_counts_by_length() {
    let index = synthetic_index(500);
    let rows = grouped(&index, &[], &[], "", SearchMode::Files, 64);
    assert_eq!(
        more_rows(&rows),
        vec![(PaletteCategory::WorkspaceFiles, 500 - 64)]
    );
}

#[test]
fn a_query_states_its_hidden_matches_and_a_group_that_fits_has_no_more_row() {
    let index = synthetic_index(300);
    let rows = grouped(&index, &[], &[], "file", SearchMode::Files, 64);
    assert_eq!(
        more_rows(&rows),
        vec![(PaletteCategory::WorkspaceFiles, 300 - 64)]
    );
    let small = synthetic_index(10);
    let rows = grouped(&small, &[], &[], "file", SearchMode::Files, 64);
    assert!(more_rows(&rows).is_empty());
}

#[test]
fn commands_fit_the_cap_and_the_rule_holds_when_forced() {
    let index = synthetic_index(0);
    let rows = grouped(&index, &[], &[], "", SearchMode::All, 64);
    assert!(
        more_rows(&rows)
            .iter()
            .all(|(category, _)| *category != PaletteCategory::Commands),
        "35 commands fit 64 rows"
    );
    let commands = all_commands().len();
    for mode in [SearchMode::All, SearchMode::Commands] {
        let rows = grouped(&index, &[], &[], "", mode, 4);
        assert_eq!(
            more_rows(&rows),
            vec![(PaletteCategory::Commands, commands - 4)],
            "{mode:?}"
        );
        assert!(matches!(rows.last(), Some(PaletteSearchRow::More { .. })));
    }
}

#[test]
fn open_tabs_and_note_groups_end_with_more_rows_when_forced() {
    let index = synthetic_index(20);
    let open_tabs: Vec<_> = index.files()[..6].iter().map(open_tab_for).collect();
    let notes: Vec<_> = (0..9)
        .map(|n| PaletteNoteEntry {
            category: PaletteNoteCategory::Bookmarks,
            title: format!("note {n}"),
            subtitle: String::new(),
            detail: None,
            note_text: None,
            target: crate::model::palette::PaletteNoteTarget::FolderNote {
                workspace_name: "w".into(),
                folder: PathBuf::from("/workspace"),
            },
        })
        .collect();
    let rows = grouped(&index, &open_tabs, &notes, "", SearchMode::All, 4);
    let more = more_rows(&rows);
    assert!(more.contains(&(PaletteCategory::OpenTabs, 2)));
    assert!(more.contains(&(PaletteCategory::WorkspaceFiles, 20 - 6 - 4)));
    assert!(more.contains(&(PaletteCategory::Note(PaletteNoteCategory::Bookmarks), 5)));
}

fn ranking(
    index: &FileIndex,
    open_tabs: &[PaletteFileEntry],
    notes: &[PaletteNoteEntry],
    category: PaletteCategory,
    query: &str,
    row_limit: usize,
) -> CategoryRanking {
    let cancellation = PaletteSearchCancellation::default();
    let PaletteSearchOutcome::Complete { value, .. } = category_search(
        CategorySearchInput {
            index,
            open_tabs,
            note_entries: notes,
            category,
            query,
            row_limit,
        },
        &cancellation,
    ) else {
        panic!("fresh category search must complete");
    };
    value
}

#[test]
fn the_category_ranking_lists_every_match_and_counts_a_cut() {
    let index = synthetic_index(1_000);
    let open_tabs: Vec<_> = index.files()[..3].iter().map(open_tab_for).collect();
    let whole = ranking(
        &index,
        &open_tabs,
        &[],
        PaletteCategory::WorkspaceFiles,
        "",
        10_000,
    );
    assert_eq!(whole.ordinals.len(), 997);
    assert_eq!(whole.matching, 997);
    assert!(!whole.limited);
    assert_eq!(
        whole.ordinals[0], 3,
        "open tabs are excluded in source order"
    );
    let cut = ranking(
        &index,
        &open_tabs,
        &[],
        PaletteCategory::WorkspaceFiles,
        "file",
        100,
    );
    assert_eq!(cut.ordinals.len(), 100);
    assert_eq!(cut.matching, 997);
    assert!(cut.limited);
    let commands = ranking(&index, &[], &[], PaletteCategory::Commands, "", 10_000);
    assert_eq!(commands.ordinals.len(), all_commands().len());
}

fn file_rows(rows: &[PaletteSearchRow], label: &str) -> Vec<PathBuf> {
    let mut in_group = false;
    let mut out = Vec::new();
    for row in rows {
        match row {
            PaletteSearchRow::Header { label: header } => in_group = header == label,
            PaletteSearchRow::File { file_path, .. } if in_group => out.push(file_path.clone()),
            _ => {}
        }
    }
    out
}

proptest::proptest! {
    /// The category view's first 64 rows are the grouped rows (design D3).
    #[test]
    fn the_category_view_begins_with_the_grouped_rows(
        count in 0usize..400,
        open in 0usize..12,
        query in "(|f|fi|1|2|rs|d3|ile9)",
    ) {
        let index = synthetic_index(count);
        let open_tabs: Vec<_> = index.files()[..open.min(count)].iter().map(open_tab_for).collect();
        let rows = grouped(&index, &open_tabs, &[], &query, SearchMode::Files, 64);
        let grouped_files = file_rows(&rows, "All Workspaces");
        let whole = ranking(&index, &open_tabs, &[], PaletteCategory::WorkspaceFiles, &query, 1 << 20);
        let ranked: Vec<PathBuf> = whole
            .ordinals
            .iter()
            .take(64)
            .map(|ordinal| index.files()[*ordinal as usize].path.clone())
            .collect();
        proptest::prop_assert_eq!(&ranked, &grouped_files);
        let hidden = more_rows(&rows)
            .into_iter()
            .find(|(category, _)| *category == PaletteCategory::WorkspaceFiles)
            .map_or(0, |(_, hidden)| hidden);
        proptest::prop_assert_eq!(whole.matching, grouped_files.len() + hidden);
    }

    /// The prefiltered exclusion equals the full one, symlinked aliases
    /// included (design D1).
    #[test]
    fn the_prefiltered_exclusion_equals_the_full_one(
        names in proptest::collection::vec("(a|b|c)(|x)\\.rs", 1..40),
        open_picks in proptest::collection::vec(0usize..40, 0..10),
        alias_picks in proptest::collection::vec(0usize..40, 0..10),
    ) {
        let root = Arc::new(PathBuf::from("/w"));
        let files: Vec<IndexedFile> = names
            .iter()
            .enumerate()
            .map(|(n, name)| {
                let path = root.join(format!("d{}/{name}", n % 3));
                // Some files are aliases: their canonical path lives elsewhere
                // under a different file name.
                let canonical = if alias_picks.contains(&n) {
                    PathBuf::from(format!("/real/target{}.rs", n % 4))
                } else {
                    path.clone()
                };
                IndexedFile::new(path, PaletteFileIdentity::canonical(canonical), Arc::clone(&root))
            })
            .collect();
        let excluded: std::collections::HashSet<PathBuf> = open_picks
            .iter()
            .filter_map(|pick| files.get(*pick))
            .filter_map(|file| file.identity.canonical_path().map(PathBuf::from))
            .chain(std::iter::once(PathBuf::from("/real/target1.rs")))
            .collect();
        let exclusion = OpenTabExclusion::new(&excluded);
        for file in &files {
            proptest::prop_assert_eq!(exclusion.admits(file), exclusion.admits_prefiltered(file));
        }
    }
}
