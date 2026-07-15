// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace file indexing for the command palette.
//!
//! This slice owns directory traversal, folder interning, and incremental path
//! updates. It remains GTK-free and returns only domain types.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::model::palette::{
    IndexedFile, PaletteFileIdentity, PaletteFileIdentityFailure, ScoredResult, SearchResultItem,
};
use crate::services::file_tree;
use crate::services::filesystem::metadata as fs_metadata;

use super::fuzzy::{
    SearchProgressPolicy, search_items, search_items_cancellable,
    search_items_cancellable_with_progress, search_items_full_sort_reference,
};
use super::runtime::{PaletteSearchCancellation, PaletteSearchOutcome};

/// Maximum recursion depth to prevent runaway scanning in deeply nested trees.
const MAX_SCAN_DEPTH: u32 = 64;
/// Maximum number of files to index. Beyond this, linear scan per query
/// starts to exceed the palette's latency budget on one CPU core.
pub const MAX_INDEXED_FILES: usize = 100_000;
/// Directory names to skip during file-index scanning.
pub(super) const IGNORED_INDEX_DIRS: &[&str] =
    &["node_modules", "target", "__pycache__", "venv", "vendor"];

/// Reason a completed index intentionally represents only a bounded prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileIndexTruncationReason {
    FileLimit,
    DirectoryRetentionLimit,
}

/// Retained-state and traversal evidence for one file-index build.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileIndexBuildMetrics {
    pub examined_directory_entries: usize,
    pub scanned_directories: usize,
    pub retained_files: usize,
    pub peak_retained_directory_entries: usize,
    pub identity_failures: usize,
    pub truncation: Option<FileIndexTruncationReason>,
}

/// Typed terminal result from cancellable file-index construction.
#[derive(Debug)]
pub enum FileIndexBuildOutcome {
    Complete {
        index: FileIndex,
        metrics: FileIndexBuildMetrics,
    },
    Cancelled {
        metrics: FileIndexBuildMetrics,
    },
}

/// Compact request retained by the file-index rebuild coordinator.
#[derive(Clone, Debug)]
pub struct FileIndexBuildRequest {
    pub workspace_folders: Arc<[PathBuf]>,
    pub capacity_hint: usize,
}

/// One request admitted as the sole active file-index build.
#[derive(Debug)]
pub struct FileIndexBuildStart {
    pub generation: u64,
    pub request: FileIndexBuildRequest,
    pub cancellation: PaletteSearchCancellation,
}

#[derive(Debug)]
struct ActiveFileIndexBuild {
    generation: u64,
    cancellation: PaletteSearchCancellation,
}

#[derive(Debug)]
struct PendingFileIndexBuild {
    generation: u64,
    request: FileIndexBuildRequest,
}

/// Scalar ownership evidence for file-index rebuild tests and readiness.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileIndexBuildCoordinatorSnapshot {
    pub active: usize,
    pub pending: usize,
    pub started: usize,
    pub cancellation_requests: usize,
}

/// Retain at most one active file-index build and one latest compact request.
#[derive(Debug, Default)]
pub struct FileIndexBuildCoordinator {
    current_generation: u64,
    active: Option<ActiveFileIndexBuild>,
    pending: Option<PendingFileIndexBuild>,
    snapshot: FileIndexBuildCoordinatorSnapshot,
}

impl FileIndexBuildCoordinator {
    pub fn submit(&mut self, request: FileIndexBuildRequest) -> Option<FileIndexBuildStart> {
        self.current_generation = self.current_generation.wrapping_add(1);
        let generation = self.current_generation;
        if let Some(active) = self.active.as_ref() {
            if active.cancellation.cancel() {
                self.snapshot.cancellation_requests =
                    self.snapshot.cancellation_requests.saturating_add(1);
            }
            self.pending = Some(PendingFileIndexBuild {
                generation,
                request,
            });
            None
        } else {
            Some(self.start(generation, request))
        }
    }

    pub fn finish(&mut self, generation: u64) -> Option<FileIndexBuildStart> {
        if self.active.as_ref().map(|active| active.generation) != Some(generation) {
            return None;
        }
        self.active = None;
        self.pending
            .take()
            .map(|pending| self.start(pending.generation, pending.request))
    }

    pub fn invalidate(&mut self) {
        self.current_generation = self.current_generation.wrapping_add(1);
        if let Some(active) = self.active.as_ref()
            && active.cancellation.cancel()
        {
            self.snapshot.cancellation_requests =
                self.snapshot.cancellation_requests.saturating_add(1);
        }
        self.pending = None;
    }

    #[must_use]
    pub fn is_current(&self, generation: u64) -> bool {
        self.current_generation == generation
    }

    #[must_use]
    pub fn has_work(&self) -> bool {
        self.active.is_some() || self.pending.is_some()
    }

    #[must_use]
    pub fn snapshot(&self) -> FileIndexBuildCoordinatorSnapshot {
        FileIndexBuildCoordinatorSnapshot {
            active: usize::from(self.active.is_some()),
            pending: usize::from(self.pending.is_some()),
            ..self.snapshot
        }
    }

    fn start(&mut self, generation: u64, request: FileIndexBuildRequest) -> FileIndexBuildStart {
        let cancellation = PaletteSearchCancellation::default();
        self.active = Some(ActiveFileIndexBuild {
            generation,
            cancellation: cancellation.clone(),
        });
        self.snapshot.started = self.snapshot.started.saturating_add(1);
        FileIndexBuildStart {
            generation,
            request,
            cancellation,
        }
    }
}

/// In-memory index of all files across workspace folders.
#[derive(Debug, Default, Clone)]
pub struct FileIndex {
    files: Vec<IndexedFile>,
    /// Deduplicated workspace folders for O(k) prefix lookups (k is usually small).
    workspace_folders: Vec<Arc<PathBuf>>,
}

impl FileIndex {
    /// Build a file index by recursively scanning all workspace folders.
    #[must_use]
    pub fn rebuild(workspace_folders: &[PathBuf]) -> Self {
        Self::rebuild_with_hint(workspace_folders, 10_000)
    }

    /// Like [`Self::rebuild`], but uses `capacity_hint` for the initial `Vec` allocation.
    #[must_use]
    pub fn rebuild_with_hint(workspace_folders: &[PathBuf], capacity_hint: usize) -> Self {
        let cancellation = PaletteSearchCancellation::default();
        let FileIndexBuildOutcome::Complete { index, .. } =
            Self::rebuild_cancellable_with_hint(workspace_folders, capacity_hint, &cancellation)
        else {
            unreachable!("a fresh index cancellation token cannot cancel");
        };
        index
    }

    /// Build an index with cooperative traversal cancellation and bounded evidence.
    #[must_use]
    pub fn rebuild_cancellable_with_hint(
        workspace_folders: &[PathBuf],
        capacity_hint: usize,
        cancellation: &PaletteSearchCancellation,
    ) -> FileIndexBuildOutcome {
        Self::rebuild_cancellable_with_limit(
            workspace_folders,
            capacity_hint,
            MAX_INDEXED_FILES,
            cancellation,
        )
    }

    fn rebuild_cancellable_with_limit(
        workspace_folders: &[PathBuf],
        capacity_hint: usize,
        file_limit: usize,
        cancellation: &PaletteSearchCancellation,
    ) -> FileIndexBuildOutcome {
        let mut files = Vec::with_capacity(capacity_hint.min(file_limit));
        let mut visited_directories = HashSet::new();
        let mut canonical_files = HashSet::new();
        let mut folder_arcs = Vec::new();
        let mut metrics = FileIndexBuildMetrics::default();
        for folder in workspace_folders {
            if cancellation.is_cancelled() {
                metrics.retained_files = files.len();
                return FileIndexBuildOutcome::Cancelled { metrics };
            }
            if files.len() >= file_limit {
                metrics.truncation = Some(FileIndexTruncationReason::FileLimit);
                break;
            }
            let Ok(canonical_folder) = fs_metadata::canonical_path(folder) else {
                continue;
            };
            let folder_arc = Arc::new(folder.clone());
            let completed = {
                let mut traversal = FileIndexTraversal {
                    out: &mut files,
                    visited_directories: &mut visited_directories,
                    canonical_files: &mut canonical_files,
                    canonical_folder: &canonical_folder,
                    file_limit,
                    cancellation,
                    metrics: &mut metrics,
                };
                collect_files_recursive(folder, &folder_arc, 0, &mut traversal)
            };
            if !completed {
                metrics.retained_files = files.len();
                return FileIndexBuildOutcome::Cancelled { metrics };
            }
            folder_arcs.push(folder_arc);
        }
        truncate_to_index_limit(&mut files, file_limit);
        if files.len() == file_limit && metrics.truncation.is_some() {
            metrics.truncation = Some(FileIndexTruncationReason::FileLimit);
        }
        metrics.retained_files = files.len();
        FileIndexBuildOutcome::Complete {
            index: Self {
                files,
                workspace_folders: folder_arcs,
            },
            metrics,
        }
    }

    #[cfg(test)]
    pub(super) fn rebuild_cancellable_for_test(
        workspace_folders: &[PathBuf],
        file_limit: usize,
        cancellation: &PaletteSearchCancellation,
    ) -> FileIndexBuildOutcome {
        Self::rebuild_cancellable_with_limit(
            workspace_folders,
            file_limit,
            file_limit,
            cancellation,
        )
    }

    #[must_use]
    pub fn files(&self) -> &[IndexedFile] {
        &self.files
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Add a single file to the index. Used for incremental sidebar updates.
    pub fn add_file(&mut self, file: IndexedFile) {
        if self.files.len() >= MAX_INDEXED_FILES
            || self.files.iter().any(|existing| {
                existing.path == file.path
                    || canonical_identities_match(&existing.identity, &file.identity)
            })
        {
            return;
        }
        intern_folder(&mut self.workspace_folders, &file.workspace_folder);
        self.files.push(file);
    }

    /// Resolve and add one filesystem path through the metadata boundary.
    ///
    /// Callers must keep this operation on a background worker.
    pub fn add_path(&mut self, path: PathBuf, workspace_folder: Arc<PathBuf>) {
        self.add_file(indexed_file_from_path(path, workspace_folder));
    }

    /// Remove a file (or all files under a directory) from the index.
    pub fn remove_path(&mut self, path: &Path) {
        let before = self.files.len();
        self.files
            .retain(|file| file.path != path && !file.path.starts_with(path));
        if should_compact_after_removal(before, self.files.len()) {
            self.files.shrink_to_fit();
            self.workspace_folders.retain(|folder| {
                self.files
                    .iter()
                    .any(|file| Arc::ptr_eq(&file.workspace_folder, folder))
            });
        }
    }

    /// Rename a file or directory in the index.
    pub fn rename_path(&mut self, old_path: &Path, new_path: &Path) {
        for file in &mut self.files {
            let replacement_path = if file.path == old_path {
                Some(new_path.to_path_buf())
            } else {
                file.path
                    .strip_prefix(old_path)
                    .ok()
                    .map(|suffix| new_path.join(suffix))
            };
            if let Some(replacement_path) = replacement_path {
                *file =
                    indexed_file_from_path(replacement_path, Arc::clone(&file.workspace_folder));
            }
        }
        deduplicate_files(&mut self.files);
    }

    /// Find the workspace folder that contains the given path.
    pub fn workspace_folder_for(&self, path: &Path) -> Option<Arc<PathBuf>> {
        self.workspace_folders
            .iter()
            .find(|folder| path.starts_with(folder.as_path()))
            .map(Arc::clone)
    }

    /// Search the file index with a fuzzy query, returning up to `max` scored results.
    pub fn search(&self, query: &str, max: usize) -> Vec<ScoredResult<'_>> {
        search_items(
            self.files.iter(),
            |file| &file.name,
            SearchResultItem::File,
            query,
            max,
        )
    }

    /// Search with cooperative cancellation and bounded retention evidence.
    pub fn search_cancellable(
        &self,
        query: &str,
        max: usize,
        cancellation: &PaletteSearchCancellation,
    ) -> PaletteSearchOutcome<Vec<ScoredResult<'_>>> {
        search_items_cancellable(
            self.files.iter(),
            |_| true,
            |file, fuzzy_query| fuzzy_query.score(&file.name),
            SearchResultItem::File,
            query,
            max,
            cancellation,
        )
    }

    /// Search while excluding canonical identities before bounded retention.
    pub fn search_cancellable_excluding(
        &self,
        query: &str,
        max: usize,
        excluded_canonical_paths: &HashSet<PathBuf>,
        cancellation: &PaletteSearchCancellation,
    ) -> PaletteSearchOutcome<Vec<ScoredResult<'_>>> {
        search_items_cancellable(
            self.files.iter(),
            |file| {
                file.identity
                    .canonical_path()
                    .is_none_or(|path| !excluded_canonical_paths.contains(path))
            },
            |file, fuzzy_query| fuzzy_query.score(&file.name),
            SearchResultItem::File,
            query,
            max,
            cancellation,
        )
    }

    /// Search with a deterministic checkpoint hook for tests and benchmarks.
    #[doc(hidden)]
    pub fn search_cancellable_with_progress(
        &self,
        query: &str,
        max: usize,
        cancellation: &PaletteSearchCancellation,
        progress: &dyn Fn(usize),
    ) -> PaletteSearchOutcome<Vec<ScoredResult<'_>>> {
        search_items_cancellable_with_progress(
            self.files.iter(),
            |_| true,
            |file, fuzzy_query| fuzzy_query.score(&file.name),
            SearchResultItem::File,
            query,
            SearchProgressPolicy {
                max,
                cancellation,
                progress,
            },
        )
    }

    /// Full-sort reference used only by equivalence tests and Criterion comparison.
    #[doc(hidden)]
    pub fn search_full_sort_reference(&self, query: &str, max: usize) -> Vec<ScoredResult<'_>> {
        search_items_full_sort_reference(
            self.files.iter(),
            |_| true,
            |file, fuzzy_query| fuzzy_query.score(&file.name),
            SearchResultItem::File,
            query,
            max,
        )
    }
}

/// Truncate oversized index results after scanning all folders.
///
/// The limit is parameterized so unit tests can exercise the threshold without
/// constructing a six-figure temporary directory tree.
pub(super) fn truncate_to_index_limit(files: &mut Vec<IndexedFile>, limit: usize) {
    if files.len() > limit {
        tracing::warn!(
            "File index truncated: {} files exceeds {} limit",
            files.len(),
            limit
        );
        files.truncate(limit);
    }
}

/// Return whether a removal changed the index enough to justify compacting folders.
///
/// Folder compaction scans the remaining files, so it is reserved for larger
/// removals; small deletions keep the folder cache intact for cheap lookups.
pub(super) fn should_compact_after_removal(before: usize, after: usize) -> bool {
    after < before * 3 / 4
}

impl From<Vec<IndexedFile>> for FileIndex {
    fn from(files: Vec<IndexedFile>) -> Self {
        let mut retained = Vec::with_capacity(files.len().min(MAX_INDEXED_FILES));
        let mut workspace_folders = Vec::new();
        let mut raw_paths = HashSet::with_capacity(retained.capacity());
        let mut canonical_paths = HashSet::with_capacity(retained.capacity());
        for file in files {
            if retained.len() == MAX_INDEXED_FILES {
                break;
            }
            let raw_is_new = raw_paths.insert(file.path.clone());
            let canonical_is_new = file
                .identity
                .canonical_path()
                .is_none_or(|path| canonical_paths.insert(path.to_path_buf()));
            if raw_is_new && canonical_is_new {
                intern_folder(&mut workspace_folders, &file.workspace_folder);
                retained.push(file);
            }
        }
        Self {
            files: retained,
            workspace_folders,
        }
    }
}

fn intern_folder(workspace_folders: &mut Vec<Arc<PathBuf>>, folder: &Arc<PathBuf>) {
    if !workspace_folders
        .iter()
        .any(|existing| Arc::ptr_eq(existing, folder))
    {
        workspace_folders.push(Arc::clone(folder));
    }
}

fn is_ignored_index_dir(dir: &Path) -> bool {
    dir.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| IGNORED_INDEX_DIRS.contains(&name))
}

struct FileIndexTraversal<'a> {
    out: &'a mut Vec<IndexedFile>,
    visited_directories: &'a mut HashSet<PathBuf>,
    canonical_files: &'a mut HashSet<PathBuf>,
    canonical_folder: &'a Path,
    file_limit: usize,
    cancellation: &'a PaletteSearchCancellation,
    metrics: &'a mut FileIndexBuildMetrics,
}

fn collect_files_recursive(
    dir: &Path,
    workspace_folder: &Arc<PathBuf>,
    depth: u32,
    traversal: &mut FileIndexTraversal<'_>,
) -> bool {
    if traversal.cancellation.is_cancelled() {
        return false;
    }
    if traversal.out.len() >= traversal.file_limit {
        traversal.metrics.truncation = Some(FileIndexTruncationReason::FileLimit);
        return true;
    }

    if depth > MAX_SCAN_DEPTH {
        tracing::warn!(
            "Skipping deeply nested directory (depth > {MAX_SCAN_DEPTH}): {}",
            dir.display()
        );
        return true;
    }

    let Ok(canonical) = fs_metadata::canonical_path(dir) else {
        return true;
    };

    if !canonical.starts_with(traversal.canonical_folder) {
        return true;
    }

    if !traversal.visited_directories.insert(canonical) {
        return true;
    }

    let remaining = traversal.file_limit.saturating_sub(traversal.out.len());
    let scan = file_tree::scan_directory_bounded_with_cancel(dir, remaining, 0, || {
        traversal.cancellation.is_cancelled()
    });
    traversal.metrics.scanned_directories = traversal.metrics.scanned_directories.saturating_add(1);
    traversal.metrics.examined_directory_entries = traversal
        .metrics
        .examined_directory_entries
        .saturating_add(scan.examined_entries);
    traversal.metrics.peak_retained_directory_entries = traversal
        .metrics
        .peak_retained_directory_entries
        .max(scan.peak_retained_entries);
    if scan.cancelled {
        return false;
    }
    if scan.truncated {
        traversal
            .metrics
            .truncation
            .get_or_insert(FileIndexTruncationReason::DirectoryRetentionLimit);
    }

    for entry in scan.entries {
        if traversal.cancellation.is_cancelled() {
            return false;
        }
        if traversal.out.len() >= traversal.file_limit {
            traversal.metrics.truncation = Some(FileIndexTruncationReason::FileLimit);
            return true;
        }
        if entry.is_dir {
            if !is_ignored_index_dir(&entry.path)
                && !collect_files_recursive(&entry.path, workspace_folder, depth + 1, traversal)
            {
                return false;
            }
        } else {
            let file = indexed_file_from_path(entry.path, Arc::clone(workspace_folder));
            if matches!(file.identity, PaletteFileIdentity::Unavailable(_)) {
                traversal.metrics.identity_failures =
                    traversal.metrics.identity_failures.saturating_add(1);
            }
            if file
                .identity
                .canonical_path()
                .is_none_or(|canonical| traversal.canonical_files.insert(canonical.to_path_buf()))
            {
                traversal.out.push(file);
            }
        }
    }
    true
}

fn indexed_file_from_path(path: PathBuf, workspace_folder: Arc<PathBuf>) -> IndexedFile {
    let identity = match fs_metadata::canonical_path(&path) {
        Ok(canonical) => PaletteFileIdentity::canonical(canonical),
        Err(error) => {
            PaletteFileIdentity::Unavailable(PaletteFileIdentityFailure::from(error.kind()))
        }
    };
    IndexedFile::new(path, identity, workspace_folder)
}

fn canonical_identities_match(left: &PaletteFileIdentity, right: &PaletteFileIdentity) -> bool {
    matches!(
        (left.canonical_path(), right.canonical_path()),
        (Some(left), Some(right)) if left == right
    )
}

fn deduplicate_files(files: &mut Vec<IndexedFile>) {
    let mut raw_paths = HashSet::new();
    let mut canonical_paths = HashSet::new();
    files.retain(|file| {
        raw_paths.insert(file.path.clone())
            && file
                .identity
                .canonical_path()
                .is_none_or(|path| canonical_paths.insert(path.to_path_buf()))
    });
}
