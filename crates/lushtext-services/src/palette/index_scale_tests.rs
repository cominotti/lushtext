// SPDX-License-Identifier: GPL-3.0-or-later

//! Scale and identity tests of the workspace file index (`scale-file-index`
//! tasks 2.1–2.5): 2^20 files within the installed budget, identity derived
//! instead of resolved per file, the construction and installed bytes per
//! file, mutations whose cost follows the change rather than the index, the
//! reclamation of storage mutations leave behind, and the empty query's
//! open-tab count through the index.
//!
//! Written failing first against the per-file representation (recorded red in
//! the change's handoff); the rows helpers below are the only code that
//! follows the representation.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tempfile::TempDir;

use super::workspace_index_fixture::{self, DEFAULT_SEED, ShapeEntry};
use super::*;
use crate::filesystem::fixture;
use crate::filesystem::metadata::canonical_path;
use lushtext_model::palette::{PaletteFileIdentity, PaletteFileIdentityFailure};
use lushtext_model::workspace_visibility::WorkspaceEntryVisibility;

const SYNTHETIC_ROOT: &str = "/synthetic/scale-file-index/workspace";
const MIB: u64 = 1024 * 1024;

/// One indexed file as the tests compare it: display path, relative display,
/// identity and workspace folder.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    path: PathBuf,
    relative: String,
    identity: PaletteFileIdentity,
    folder: PathBuf,
}

fn rows(index: &FileIndex) -> Vec<Row> {
    index
        .files()
        .map(|file| Row {
            path: file.path(),
            relative: file.relative_display(),
            identity: file.identity(),
            folder: file.workspace_folder().as_ref().clone(),
        })
        .collect()
}

fn owned(path: PathBuf, identity: PaletteFileIdentity, folder: &Arc<PathBuf>) -> OwnedIndexedFile {
    OwnedIndexedFile::new(path, identity, Arc::clone(folder))
}

/// The D9 shape of `files` files as in-memory triples under a synthetic
/// folder, each file's identity its own path.
fn synthetic_triples(
    files: usize,
) -> impl Iterator<Item = (PathBuf, PaletteFileIdentity, Arc<PathBuf>)> {
    let folder = Arc::new(PathBuf::from(SYNTHETIC_ROOT));
    workspace_index_fixture::shape(files, DEFAULT_SEED).filter_map(move |entry| match entry {
        ShapeEntry::File(relative) => {
            let path = folder.join(relative);
            Some((
                path.clone(),
                PaletteFileIdentity::canonical(path),
                Arc::clone(&folder),
            ))
        }
        ShapeEntry::Directory(_) => None,
    })
}

fn synthetic_paths(files: usize) -> Vec<PathBuf> {
    synthetic_triples(files).map(|(path, _, _)| path).collect()
}

fn build(folders: &[PathBuf]) -> (FileIndex, FileIndexBuildMetrics) {
    match FileIndex::rebuild_cancellable_with_hint(
        folders,
        0,
        &PaletteSearchCancellation::default(),
        &WorkspaceEntryVisibility::default(),
    ) {
        FileIndexBuildOutcome::Complete { index, metrics } => (index, metrics),
        FileIndexBuildOutcome::Cancelled { .. } => panic!("a fresh token cannot cancel"),
    }
}

fn canonical(path: &Path) -> PathBuf {
    canonical_path(path).expect("canonical fixture path")
}

/// Task 2.1: 2^20 files of the D9 shape fit the unchanged 64 MiB budget.
#[test]
fn a_million_file_index_fits_the_installed_budget() {
    let index = FileIndex::from_paths(synthetic_triples(MAX_INDEXED_FILES));
    assert_eq!(
        index.len(),
        1 << 20,
        "every file is indexed, none truncated"
    );
    let weight = index.retained_byte_weight();
    assert!(
        weight <= MAX_FILE_INDEX_RETAINED_BYTES,
        "{weight} B over the 64 MiB installed budget ({} B/file)",
        weight >> 20
    );
}

/// Tasks 2.2 and 2.3: a 2^17-file tree without symlinks resolves only its
/// folder and stays under 96 B/file at its construction peak and 60 B/file
/// installed.
#[test]
fn a_symlink_free_tree_resolves_only_its_folder_within_its_byte_bars() {
    let dir = TempDir::new().expect("temp directory");
    let root = dir.path().join("workspace");
    let written = workspace_index_fixture::write(&root, 1 << 17, DEFAULT_SEED, false);
    assert_eq!(written.files, 1 << 17);

    let (index, metrics) = build(std::slice::from_ref(&root));
    let files = u64::try_from(index.len()).expect("fits u64");
    assert_eq!(index.len(), 1 << 17);
    assert_eq!(metrics.truncation, None);
    assert_eq!(
        metrics.canonical_resolutions, 1,
        "one resolution for the folder, none per file or directory"
    );
    assert!(
        metrics.peak_build_bytes <= 96 * files,
        "construction peak {} B/file over 96",
        metrics.peak_build_bytes / files
    );
    assert!(
        metrics.retained_index_bytes <= 60 * files,
        "installed {} B/file over 60",
        metrics.retained_index_bytes / files
    );
    // Every derived identity is what resolving that file returns.
    for row in rows(&index).iter().step_by(4099) {
        assert_eq!(
            row.identity,
            PaletteFileIdentity::canonical(canonical(&row.path))
        );
    }
}

/// Build `folders` under explicit byte limits, returning the files published.
fn files_published(folders: &[PathBuf], build_bytes: u64, installed_bytes: u64) -> usize {
    match FileIndex::rebuild_cancellable_with_limits(
        folders,
        MAX_INDEXED_FILES,
        MAX_INDEXED_DIRECTORIES,
        build_bytes,
        installed_bytes,
        &PaletteSearchCancellation::default(),
    ) {
        FileIndexBuildOutcome::Complete { index, metrics } => {
            assert_eq!(metrics.retained_files, index.len());
            index.len()
        }
        FileIndexBuildOutcome::Cancelled { .. } => panic!("a fresh token cannot cancel"),
    }
}

/// Files the per-file reference publishes under a build budget.
fn reference_files_published(folders: &[PathBuf], build_bytes: u64) -> usize {
    match reference::ReferenceFileIndex::rebuild_cancellable_with_limits(
        folders,
        0,
        MAX_INDEXED_FILES,
        MAX_INDEXED_DIRECTORIES,
        build_bytes,
        &PaletteSearchCancellation::default(),
        &WorkspaceEntryVisibility::default(),
    ) {
        reference::ReferenceBuildOutcome::Complete { index, .. } => index.len(),
        reference::ReferenceBuildOutcome::Cancelled { .. } => {
            panic!("a fresh token cannot cancel")
        }
    }
}

/// Stage-4 review: a byte-truncated build publishes the prefix it admitted.
/// The key table and the per-directory hashers the finish builds are paid
/// for while the traversal admits files, so the finish never discards a
/// prefix: a larger budget never publishes fewer files, and under every build
/// budget the arena publishes at least what the per-file reference does (which
/// always kept a usable prefix). A sparse tree, one file per directory and
/// three empty directories beside it, is where the finish's per-directory
/// scratch is largest beside the arena.
#[test]
fn a_byte_truncated_build_publishes_its_prefix_under_every_budget() {
    let dir = TempDir::new().expect("temp directory");
    let root = dir.path().join("sparse");
    for n in 0..1024 {
        let directory = root.join(format!("d{:02}/e{n:04}", n % 32));
        for empty in ["x", "y", "z"] {
            fixture::create_dir_all(&directory.join(empty));
        }
        fixture::write_text(&directory.join("f.rs"), "");
    }
    let folders = std::slice::from_ref(&root);
    let (_, full) = build(folders);
    assert_eq!(full.truncation, None);
    let installed = full.retained_index_bytes;

    let mut previous = 0;
    for step in 1..=64 {
        let budget = full.peak_build_bytes * step / 32;
        let published = files_published(folders, budget, MAX_FILE_INDEX_RETAINED_BYTES);
        let reference = reference_files_published(folders, budget);
        assert!(
            published >= previous,
            "a {budget} B build budget published {published} files, a smaller one {previous}"
        );
        assert!(
            published >= reference.min(1024),
            "a {budget} B build budget published {published} files, the reference {reference}"
        );
        previous = published;
    }
    assert_eq!(previous, 1024, "twice the measured peak admits every file");
    assert!(
        files_published(
            folders,
            full.peak_build_bytes / 2,
            MAX_FILE_INDEX_RETAINED_BYTES
        ) > 0,
        "half the measured peak publishes a prefix"
    );

    let mut previous = 0;
    for step in 1..=64 {
        let budget = installed * step / 32;
        let published = files_published(folders, MAX_FILE_INDEX_BUILD_RETAINED_BYTES, budget);
        assert!(
            published >= previous,
            "a {budget} B installed budget published {published} files, a smaller one {previous}"
        );
        previous = published;
    }
    // The traversal holds growth slack the finish shrinks away, as the
    // per-file representation's capacity charges did.
    assert_eq!(
        previous, 1024,
        "twice the measured weight admits every file"
    );
    assert!(
        files_published(folders, MAX_FILE_INDEX_BUILD_RETAINED_BYTES, installed / 2) > 0,
        "half the measured weight publishes a prefix"
    );
}

/// Task 2.2: symlinked files reached before and after their target, and a
/// symlinked directory, keep the per-file representation's winners, order and
/// identities, and are the only entries resolved one by one.
#[cfg(unix)]
#[test]
fn symlinks_keep_the_first_reached_path_and_its_resolved_identity() {
    let dir = TempDir::new().expect("temp directory");
    let root = dir.path().join("ws");
    fixture::create_dir_all(&root.join("a"));
    fixture::create_dir_all(&root.join("c"));
    fixture::write_text(&root.join("a/target.rs"), "");
    fixture::write_text(&root.join("c/target_late.rs"), "");
    // The root's files are taken in the root's scan, before any directory:
    // this link is reached before its target.
    fixture::symlink(Path::new("a/target.rs"), &root.join("link_early.rs"));
    // Pending directories are a stack, so `c` is scanned before `a`: this
    // link is reached after its target.
    fixture::symlink(
        Path::new("../c/target_late.rs"),
        &root.join("a/link_late.rs"),
    );
    // A directory alias of `c`, which the visit set skips.
    fixture::symlink(Path::new("c"), &root.join("linked_c"));

    let (index, metrics) = build(std::slice::from_ref(&root));
    let got = rows(&index);
    let expected = vec![
        Row {
            path: root.join("link_early.rs"),
            relative: "link_early.rs".into(),
            identity: PaletteFileIdentity::canonical(canonical(&root.join("a/target.rs"))),
            folder: root.clone(),
        },
        Row {
            path: root.join("c/target_late.rs"),
            relative: "c/target_late.rs".into(),
            identity: PaletteFileIdentity::canonical(canonical(&root.join("c/target_late.rs"))),
            folder: root.clone(),
        },
    ];
    assert_eq!(got, expected);
    assert_eq!(
        metrics.canonical_resolutions, 4,
        "the folder plus the three symlink entries"
    );
}

fn add_one(index: &mut FileIndex, path: &Path, folder: &Arc<PathBuf>) -> FileIndexMutationLedger {
    let mut ledger = index.incremental_mutation_ledger();
    index.add_path_for_bounded_batch(path, folder, &mut ledger);
    index.finish_mutation_batch(&mut ledger);
    ledger
}

/// Task 2.4: one add into a full-size index compares a logarithmic number of
/// paths and is admitted within the installed budget.
#[test]
fn adding_to_a_million_file_index_compares_logarithmically() {
    let mut index = FileIndex::from_paths(synthetic_triples(MAX_INDEXED_FILES - 1));
    assert_eq!(index.len(), MAX_INDEXED_FILES - 1);
    let folder = Arc::new(PathBuf::from(SYNTHETIC_ROOT));
    let existing = synthetic_paths(16)[7].clone();
    let added = existing.with_file_name("added_after_the_build.rs");

    let ledger = add_one(&mut index, &added, &folder);

    assert_eq!(index.len(), MAX_INDEXED_FILES, "the add is admitted");
    assert!(
        ledger.path_comparisons() <= 64,
        "{} path comparisons for one add",
        ledger.path_comparisons()
    );
    assert!(ledger.peak_retained_bytes() <= MAX_FILE_INDEX_RETAINED_BYTES);
    assert_eq!(ledger.retained_bytes(), index.retained_byte_weight());

    // The same path again is a duplicate.
    let again = add_one(&mut index, &added, &folder);
    assert_eq!(index.len(), MAX_INDEXED_FILES);
    assert!(again.path_comparisons() <= 64);
}

/// Task 2.4: a directory rename re-keys its 10,000 descendants with one
/// canonical resolution.
#[test]
fn renaming_a_large_directory_resolves_once() {
    let folder = Arc::new(PathBuf::from(SYNTHETIC_ROOT));
    let big = folder.join("big");
    let mut index = FileIndex::from_paths(
        (0..10_000)
            .map(|item| big.join(format!("nested_{}/file_{item}.rs", item % 7)))
            .chain(std::iter::once(folder.join("outside.rs")))
            .map(|path| {
                (
                    path.clone(),
                    PaletteFileIdentity::canonical(path),
                    Arc::clone(&folder),
                )
            }),
    );
    let renamed = folder.join("bigger");
    let mut ledger = index.incremental_mutation_ledger();
    index.rename_path_for_bounded_batch(&big, &renamed, &mut ledger);
    index.finish_mutation_batch(&mut ledger);

    assert_eq!(ledger.canonical_resolutions(), 1);
    let after = rows(&index);
    assert_eq!(after.len(), 10_001);
    assert_eq!(
        after
            .iter()
            .filter(|row| row.path.starts_with(&renamed))
            .count(),
        10_000
    );
    assert!(after.iter().all(|row| !row.path.starts_with(&big)));
}

fn apply(index: &mut FileIndex, mutate: impl FnOnce(&mut FileIndex, &mut FileIndexMutationLedger)) {
    let mut ledger = index.incremental_mutation_ledger();
    mutate(index, &mut ledger);
    index.finish_mutation_batch(&mut ledger);
    assert_eq!(ledger.retained_bytes(), index.retained_byte_weight());
}

/// Task 2.4, design D5: renaming a workspace folder root keeps the files'
/// folder, so their relative display falls back to the full path.
#[cfg(unix)]
#[test]
fn renaming_a_folder_root_keeps_its_folder_and_displays_full_paths() {
    let dir = TempDir::new().expect("temp directory");
    let root = dir.path().join("ws");
    fixture::create_dir_all(&root.join("sub"));
    fixture::write_text(&root.join("a.rs"), "");
    fixture::write_text(&root.join("sub/b.rs"), "");
    let (mut index, _) = build(std::slice::from_ref(&root));
    let renamed = dir.path().join("ws_renamed");
    fixture::rename(&root, &renamed);

    apply(&mut index, |index, ledger| {
        index.rename_path_for_bounded_batch(&root, &renamed, ledger);
    });

    let expected: Vec<Row> = ["sub/b.rs", "a.rs"]
        .iter()
        .map(|relative| Row {
            path: renamed.join(relative),
            relative: renamed.join(relative).display().to_string(),
            identity: PaletteFileIdentity::canonical(canonical(&renamed.join(relative))),
            folder: root.clone(),
        })
        .collect();
    let mut got = rows(&index);
    got.sort_by(|left, right| right.path.cmp(&left.path));
    assert_eq!(got, expected);
}

/// Task 2.4, design D5: a directory renamed to a path outside every folder,
/// and a file renamed into a new directory.
#[cfg(unix)]
#[test]
fn renames_outside_every_folder_and_into_a_new_directory() {
    let dir = TempDir::new().expect("temp directory");
    let root = dir.path().join("ws");
    fixture::create_dir_all(&root.join("moved"));
    fixture::write_text(&root.join("moved/x.rs"), "");
    fixture::write_text(&root.join("stay.rs"), "");
    let (mut index, _) = build(std::slice::from_ref(&root));

    let outside = dir.path().join("elsewhere/moved_out");
    fixture::create_dir_all(&dir.path().join("elsewhere"));
    fixture::rename(&root.join("moved"), &outside);
    fixture::create_dir_all(&root.join("fresh/deeper"));
    fixture::rename(&root.join("stay.rs"), &root.join("fresh/deeper/stay.rs"));
    apply(&mut index, |index, ledger| {
        index.rename_path_for_bounded_batch(&root.join("moved"), &outside, ledger);
        index.rename_path_for_bounded_batch(
            &root.join("stay.rs"),
            &root.join("fresh/deeper/stay.rs"),
            ledger,
        );
    });

    let mut got = rows(&index);
    got.sort_by(|left, right| left.path.cmp(&right.path));
    let mut expected = vec![
        Row {
            path: outside.join("x.rs"),
            relative: outside.join("x.rs").display().to_string(),
            identity: PaletteFileIdentity::canonical(canonical(&outside.join("x.rs"))),
            folder: root.clone(),
        },
        Row {
            path: root.join("fresh/deeper/stay.rs"),
            relative: "fresh/deeper/stay.rs".into(),
            identity: PaletteFileIdentity::canonical(canonical(&root.join("fresh/deeper/stay.rs"))),
            folder: root.clone(),
        },
    ];
    expected.sort_by(|left, right| left.path.cmp(&right.path));
    assert_eq!(got, expected);
}

/// Task 2.4, design D5: the second add of one path in one batch is refused,
/// and a path missing on disk keeps its typed failure.
#[test]
fn a_second_add_of_one_path_in_one_batch_is_refused() {
    let dir = TempDir::new().expect("temp directory");
    let root = dir.path().join("ws");
    fixture::create_dir_all(&root);
    fixture::write_text(&root.join("one.rs"), "");
    let (mut index, _) = build(std::slice::from_ref(&root));
    let folder = Arc::new(root.clone());
    fixture::write_text(&root.join("new.rs"), "");

    apply(&mut index, |index, ledger| {
        assert!(index.add_path_for_bounded_batch(&root.join("new.rs"), &folder, ledger));
        assert!(!index.add_path_for_bounded_batch(&root.join("new.rs"), &folder, ledger));
        assert!(index.add_path_for_bounded_batch(&root.join("gone.rs"), &folder, ledger));
    });

    let got = rows(&index);
    assert_eq!(got.len(), 3);
    assert_eq!(got[1].path, root.join("new.rs"));
    assert_eq!(
        got[2].identity,
        PaletteFileIdentity::Unavailable(PaletteFileIdentityFailure::NotFound)
    );
}

/// Task 2.4, design D5: a directory rename the installed budget refuses drops
/// that subtree and keeps the ledger equal to the installed weight.
#[test]
fn a_refused_directory_rename_drops_its_subtree() {
    let folder = Arc::new(PathBuf::from(SYNTHETIC_ROOT));
    let mut index =
        FileIndex::from_paths(["keep/a.rs", "doomed/b.rs", "doomed/deep/c.rs"].iter().map(
            |relative| {
                let path = folder.join(relative);
                (
                    path.clone(),
                    PaletteFileIdentity::canonical(path),
                    Arc::clone(&folder),
                )
            },
        ));
    let huge = PathBuf::from(format!("/renamed/{}", "y".repeat(65 * 1024 * 1024)));
    let mut ledger = index.incremental_mutation_ledger();
    index.rename_path_for_bounded_batch(&folder.join("doomed"), &huge, &mut ledger);
    index.finish_mutation_batch(&mut ledger);

    let paths: Vec<_> = rows(&index).into_iter().map(|row| row.path).collect();
    assert_eq!(paths, vec![folder.join("keep/a.rs")]);
    assert_eq!(ledger.retained_bytes(), index.retained_byte_weight());
    assert!(ledger.peak_retained_bytes() <= MAX_FILE_INDEX_RETAINED_BYTES);
}

/// Task 2.4, design D5: removing and re-adding every file of a 2^17 index in
/// batches of 1,024 admits every add, and storage left behind is reclaimed:
/// the weight never exceeds four thirds of the start (dead bytes stay below a
/// quarter of the weight) plus one growth step, and returns within one growth
/// step of the start when compaction runs.
#[test]
fn storage_left_by_removals_is_reclaimed() {
    let folder = Arc::new(PathBuf::from(SYNTHETIC_ROOT));
    let paths = synthetic_paths(1 << 17);
    let mut index = FileIndex::from_paths(synthetic_triples(1 << 17));
    let start = index.retained_byte_weight();
    // Dead bytes stay below a quarter of the weight, so the weight stays below
    // four thirds of the live size, plus one growth step.
    let bound = start + start / 3 + start / 16;
    let step = start + start / 16;
    let mut returned = false;
    for batch in paths.chunks(1024) {
        apply(&mut index, |index, ledger| {
            for path in batch {
                index.remove_path_for_bounded_batch(path, ledger);
            }
            for path in batch {
                assert!(index.add_file_for_bounded_batch(
                    owned(
                        path.clone(),
                        PaletteFileIdentity::canonical(path.clone()),
                        &folder,
                    ),
                    ledger,
                ));
            }
        });
        assert_eq!(index.len(), 1 << 17);
        let weight = index.retained_byte_weight();
        assert!(weight <= bound, "{weight} B over {bound} B");
        returned |= weight <= step;
    }
    assert!(
        returned,
        "compaction brings the weight back within one growth step ({step} B)"
    );
    assert!(index.retained_byte_weight() <= bound);
}

/// Task 2.4, design D5: near the 64 MiB cap, with dead bytes below a quarter
/// of the weight, an add that fits the live size is still admitted.
#[test]
fn an_add_near_the_cap_reclaims_dead_bytes_first() {
    let folder = Arc::new(PathBuf::from(SYNTHETIC_ROOT));
    let long = "n".repeat(200);
    let path_of = |item: usize| folder.join(format!("d{}/{long}_{item}.rs", item % 512));
    let triple = |item: usize| {
        let path = path_of(item);
        (
            path.clone(),
            PaletteFileIdentity::canonical(path),
            Arc::clone(&folder),
        )
    };
    let probe = FileIndex::from_paths((0..4096).map(triple));
    let per_file = probe.retained_byte_weight() / 4096;
    // Leave 2 MiB of headroom: less than one growth step of the name storage,
    // more than the 1% removed and re-added below.
    let files =
        usize::try_from((MAX_FILE_INDEX_RETAINED_BYTES - 2 * MIB) / per_file).expect("fits usize");
    let mut index = FileIndex::from_paths((0..files).map(triple));
    assert_eq!(index.len(), files, "the fixture fits the budget");
    let removed = files / 100;
    apply(&mut index, |index, ledger| {
        for item in 0..removed {
            index.remove_path_for_bounded_batch(&path_of(item), ledger);
        }
    });
    let mut admitted = 0;
    apply(&mut index, |index, ledger| {
        for item in files..files + removed {
            let (path, identity, folder) = triple(item);
            if index.add_file_for_bounded_batch(owned(path, identity, &folder), ledger) {
                admitted += 1;
            }
        }
    });
    assert_eq!(admitted, removed, "every add that fits the live size");
    assert_eq!(index.len(), files);
    assert!(index.retained_byte_weight() <= MAX_FILE_INDEX_RETAINED_BYTES);
}

/// Task 2.5: the empty query's count at 2^20 files with 256 open tabs is
/// exact and walks no file after the shown rows.
#[test]
fn the_empty_query_counts_open_tabs_through_the_index() {
    let index = FileIndex::from_paths(synthetic_triples(MAX_INDEXED_FILES));
    let open: HashSet<PathBuf> = synthetic_paths(MAX_INDEXED_FILES)
        .into_iter()
        .step_by(4096)
        .take(256)
        .collect();
    assert_eq!(open.len(), 256);
    let PaletteSearchOutcome::Complete { value, metrics } =
        index.search_cancellable_excluding("", 64, &open, &PaletteSearchCancellation::default())
    else {
        panic!("a fresh token cannot cancel");
    };
    assert_eq!(value.len(), 64);
    assert_eq!(metrics.matching_candidates, (1 << 20) - 256);
    assert_eq!(metrics.candidates_counted, 0, "no walk over the rest");
}
