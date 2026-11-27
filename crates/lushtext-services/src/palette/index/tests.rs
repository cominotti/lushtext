// SPDX-License-Identifier: GPL-3.0-or-later

//! Unit tests of the arena's own machinery: the ledgers' exact limits, scratch
//! released on every build exit, exact comparisons behind every hash hit, and
//! the views' path reconstruction.

use std::path::PathBuf;
use std::sync::Arc;

use super::arena::force_hash_collisions_for_test;
use super::traversal::BuildLedger;
use super::*;
use crate::filesystem::fixture;
use lushtext_model::workspace::palette::{PaletteFileIdentity, PaletteFileIdentityFailure};

fn owned(path: &str, folder: &Arc<PathBuf>) -> OwnedIndexedFile {
    OwnedIndexedFile::new(
        PathBuf::from(path),
        PaletteFileIdentity::canonical(PathBuf::from(path)),
        Arc::clone(folder),
    )
}

fn complete(outcome: FileIndexBuildOutcome) -> (FileIndex, FileIndexBuildMetrics) {
    match outcome {
        FileIndexBuildOutcome::Complete { index, metrics } => (index, metrics),
        FileIndexBuildOutcome::Cancelled { .. } => panic!("expected a complete build"),
    }
}

#[test]
fn build_ledger_accepts_exact_limits_and_rejects_one_byte_over() {
    let mut build = BuildLedger::new(
        MAX_FILE_INDEX_BUILD_RETAINED_BYTES,
        MAX_FILE_INDEX_RETAINED_BYTES,
    );
    assert!(build.try_charge_scratch(MAX_FILE_INDEX_BUILD_RETAINED_BYTES));
    assert!(!build.try_charge_scratch(1));
    build.release_scratch(MAX_FILE_INDEX_BUILD_RETAINED_BYTES);

    let mut installed = BuildLedger::new(
        MAX_FILE_INDEX_BUILD_RETAINED_BYTES,
        MAX_FILE_INDEX_RETAINED_BYTES,
    );
    assert_eq!(
        installed.try_charge_installed(MAX_FILE_INDEX_RETAINED_BYTES),
        Ok(())
    );
    assert_eq!(
        installed.try_charge_installed(1),
        Err(FileIndexTruncationReason::RetainedByteLimit)
    );
    assert_eq!(installed.installed_bytes(), MAX_FILE_INDEX_RETAINED_BYTES);

    let mut build_limited = BuildLedger::new(
        MAX_FILE_INDEX_RETAINED_BYTES - 1,
        MAX_FILE_INDEX_RETAINED_BYTES,
    );
    assert_eq!(
        build_limited.try_charge_installed(MAX_FILE_INDEX_RETAINED_BYTES),
        Err(FileIndexTruncationReason::BuildByteLimit)
    );
    assert_eq!(build_limited.installed_bytes(), 0);
}

#[test]
fn incremental_long_path_batch_never_crosses_the_installed_policy() {
    let folder = Arc::new(PathBuf::from("/synthetic/incremental-byte-policy"));
    let mut index = FileIndex::default();
    let mut ledger = index.incremental_mutation_ledger();
    let long = "x".repeat(250);
    for item in 0..300_000 {
        let path = folder.join(format!("d{}/{long}{item:06}.rs", item % 97));
        let file = OwnedIndexedFile::new(
            path,
            PaletteFileIdentity::Unavailable(PaletteFileIdentityFailure::NotFound),
            Arc::clone(&folder),
        );
        index.add_file_for_bounded_batch(file, &mut ledger);
        assert!(ledger.peak_retained_bytes() <= MAX_FILE_INDEX_RETAINED_BYTES);
        if ledger.truncated() {
            break;
        }
    }
    assert!(ledger.truncated(), "the batch reaches the installed budget");
    index.finish_mutation_batch(&mut ledger);
    assert_eq!(ledger.retained_bytes(), index.retained_byte_weight());
    assert!(index.retained_byte_weight() <= MAX_FILE_INDEX_RETAINED_BYTES);

    let mut ledger = index.incremental_mutation_ledger();
    let long_prefix = PathBuf::from(format!("/renamed/{}", "y".repeat(256 * 1024)));
    index.rename_path_for_bounded_batch(folder.as_path(), &long_prefix, &mut ledger);
    index.finish_mutation_batch(&mut ledger);
    assert_eq!(ledger.retained_bytes(), index.retained_byte_weight());
    assert!(ledger.peak_retained_bytes() <= MAX_FILE_INDEX_RETAINED_BYTES);
    assert!(index.retained_byte_weight() <= MAX_FILE_INDEX_RETAINED_BYTES);
}

/// Every build exit (completion, a byte refusal mid-scan, cancellation)
/// releases its scratch: `finish` and `cancelled` assert that only the
/// installed arena stays charged.
#[test]
fn every_build_exit_releases_its_scratch() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    fixture::write_text(&dir.path().join("a.rs"), "fn a() {}");
    fixture::write_text(&dir.path().join("b.md"), "# b");
    fixture::create_dir_all(&dir.path().join("nested"));
    fixture::write_text(&dir.path().join("nested/c.toml"), "k = 1");
    fixture::create_dir_all(&dir.path().join("target"));
    fixture::write_text(&dir.path().join("target/skipped.rs"), "");
    let folders = [dir.path().to_path_buf()];

    let (index, metrics) = complete(FileIndex::rebuild_cancellable_with_limits(
        &folders,
        MAX_INDEXED_FILES,
        MAX_INDEXED_DIRECTORIES,
        MAX_FILE_INDEX_BUILD_RETAINED_BYTES,
        MAX_FILE_INDEX_RETAINED_BYTES,
        &PaletteSearchCancellation::default(),
    ));
    assert_eq!(index.len(), 3, "the ignored directory is skipped");
    assert_eq!(metrics.current_build_bytes, metrics.retained_index_bytes);

    for limit in [64, 512, 2048] {
        let _ = FileIndex::rebuild_cancellable_with_limits(
            &folders,
            MAX_INDEXED_FILES,
            MAX_INDEXED_DIRECTORIES,
            limit,
            MAX_FILE_INDEX_RETAINED_BYTES,
            &PaletteSearchCancellation::default(),
        );
    }

    let cancelled = PaletteSearchCancellation::default();
    let _ = cancelled.cancel();
    assert!(matches!(
        FileIndex::rebuild_cancellable_with_limits(
            &folders,
            MAX_INDEXED_FILES,
            MAX_INDEXED_DIRECTORIES,
            MAX_FILE_INDEX_BUILD_RETAINED_BYTES,
            MAX_FILE_INDEX_RETAINED_BYTES,
            &cancelled,
        ),
        FileIndexBuildOutcome::Cancelled { .. }
    ));
}

/// With every path hash equal, the visit set, the alias set and the key
/// tables still find exactly the right entries: every hit is confirmed by an
/// exact comparison.
#[cfg(unix)]
#[test]
fn forced_hash_collisions_change_no_result() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let root = dir.path().join("ws");
    for relative in [
        "a/one.rs",
        "a/two.rs",
        "b/one.rs",
        "b/deep/three.rs",
        "top.rs",
    ] {
        let path = root.join(relative);
        fixture::create_dir_all(path.parent().expect("parent"));
        fixture::write_text(&path, "");
    }
    fixture::symlink(std::path::Path::new("a/one.rs"), &root.join("alias.rs"));
    let folders = [root.clone()];
    let (expected, _) = complete(FileIndex::rebuild_cancellable_with_limits(
        &folders,
        MAX_INDEXED_FILES,
        MAX_INDEXED_DIRECTORIES,
        MAX_FILE_INDEX_BUILD_RETAINED_BYTES,
        MAX_FILE_INDEX_RETAINED_BYTES,
        &PaletteSearchCancellation::default(),
    ));
    force_hash_collisions_for_test(true);
    let (mut collided, _) = complete(FileIndex::rebuild_cancellable_with_limits(
        &folders,
        MAX_INDEXED_FILES,
        MAX_INDEXED_DIRECTORIES,
        MAX_FILE_INDEX_BUILD_RETAINED_BYTES,
        MAX_FILE_INDEX_RETAINED_BYTES,
        &PaletteSearchCancellation::default(),
    ));
    let rows = |index: &FileIndex| {
        index
            .files()
            .map(|file| (file.path(), file.identity()))
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(&collided), rows(&expected));
    let open: std::collections::HashSet<PathBuf> = [fixture_canonical(&root.join("b/one.rs"))]
        .into_iter()
        .collect();
    let exclusion = OpenTabExclusion::new(&collided, &open);
    assert_eq!(exclusion.excluded(), 1);
    // A duplicate add is refused, a new one admitted, under collisions too.
    let folder = Arc::new(root.clone());
    collided.add_path(&root.join("a/two.rs"), &folder);
    assert_eq!(collided.len(), expected.len());
    fixture::write_text(&root.join("b/four.rs"), "");
    collided.add_path(&root.join("b/four.rs"), &folder);
    assert_eq!(collided.len(), expected.len() + 1);
    force_hash_collisions_for_test(false);
}

fn fixture_canonical(path: &std::path::Path) -> PathBuf {
    crate::filesystem::metadata::canonical_path(path).expect("canonical")
}

#[test]
fn views_rebuild_display_relative_and_identity() {
    let folder = Arc::new(PathBuf::from("/home/user/project"));
    let index = FileIndex::from(vec![
        owned("/home/user/project/src/main.rs", &folder),
        owned("/home/user/project/Cargo.toml", &folder),
        owned("/other/path/file.rs", &folder),
    ]);
    let rows: Vec<_> = index
        .files()
        .map(|file| (file.name().to_string(), file.relative_display()))
        .collect();
    assert_eq!(
        rows,
        vec![
            ("main.rs".to_string(), "src/main.rs".to_string()),
            ("Cargo.toml".to_string(), "Cargo.toml".to_string()),
            ("file.rs".to_string(), "/other/path/file.rs".to_string()),
        ]
    );
    let file = index.file(FileOrdinal(0)).expect("first file");
    assert_eq!(file.path(), PathBuf::from("/home/user/project/src/main.rs"));
    assert_eq!(
        file.identity(),
        PaletteFileIdentity::canonical("/home/user/project/src/main.rs".into())
    );
    assert_eq!(file.workspace_folder().as_path(), folder.as_path());
    assert!(index.file(FileOrdinal(3)).is_none());
}

#[cfg(unix)]
#[test]
fn a_non_utf8_name_keeps_its_exact_bytes() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let folder = Arc::new(PathBuf::from("/synthetic/raw"));
    let first = folder.join(OsStr::from_bytes(b"caf\xe9.rs"));
    let second = folder.join(OsStr::from_bytes(b"caf\xea.rs"));
    let index = FileIndex::from_paths([first.clone(), second.clone()].into_iter().map(|path| {
        (
            path.clone(),
            PaletteFileIdentity::canonical(path),
            Arc::clone(&folder),
        )
    }));
    let paths: Vec<_> = index.files().map(|file| file.path()).collect();
    assert_eq!(
        paths,
        vec![first, second],
        "two names sharing one lossy string"
    );
    let names: Vec<_> = index.files().map(|file| file.name().to_string()).collect();
    assert_eq!(names[0], names[1]);
}

/// A nested folder's root renamed, then a file renamed out of it into the
/// parent folder's tree: the file keeps its folder (the per-file index's
/// result), found by the property test `index_parity`.
#[test]
fn a_file_moved_out_of_a_renamed_nested_folder_keeps_its_folder() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let root = dir.path().join("ws");
    let nested = root.join("alpha");
    fixture::create_dir_all(&nested);
    fixture::write_text(&root.join("one.rs"), "");
    fixture::write_text(&nested.join("one.rs"), "");
    let folders = [nested.clone(), root.clone()];
    let (mut index, _) = complete(FileIndex::rebuild_cancellable_with_limits(
        &folders,
        MAX_INDEXED_FILES,
        MAX_INDEXED_DIRECTORIES,
        MAX_FILE_INDEX_BUILD_RETAINED_BYTES,
        MAX_FILE_INDEX_RETAINED_BYTES,
        &PaletteSearchCancellation::default(),
    ));
    let renamed = root.join("renamed_alpha");
    fixture::rename(&nested, &renamed);
    index.rename_path(&nested, &renamed);
    let moved = root.join("moved_one.rs");
    fixture::rename(&renamed.join("one.rs"), &moved);
    index.rename_path(&renamed.join("one.rs"), &moved);
    let rows: Vec<_> = index
        .files()
        .map(|file| (file.path(), file.workspace_folder().as_ref().clone()))
        .collect();
    assert_eq!(
        rows,
        vec![(moved, nested.clone()), (root.join("one.rs"), root.clone())]
    );
}
