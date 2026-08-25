// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace file indexing for the command palette.
//!
//! This slice owns directory traversal, folder interning, and incremental path
//! updates. It remains GTK-free and returns only domain types.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::model::palette::{
    IndexedFile, PaletteFileIdentity, PaletteFileIdentityFailure, ScoredResult, SearchResultItem,
};
use crate::services::file_tree;
use crate::services::filesystem::metadata as fs_metadata;
use crate::services::single_flight::{
    SingleFlightCoordinator, SingleFlightSnapshot, SingleFlightStart,
};

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
/// Maximum number of distinct canonical directories retained by one index build.
///
/// This is independent from the file limit because sparse directory forests can
/// otherwise consume unbounded traversal state while admitting almost no files.
pub const MAX_INDEXED_DIRECTORIES: usize = 100_000;
/// Maximum heap ownership retained by one installed file index.
pub const MAX_FILE_INDEX_RETAINED_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum installed output plus traversal/deduplication ownership during build.
pub const MAX_FILE_INDEX_BUILD_RETAINED_BYTES: u64 = 128 * 1024 * 1024;
/// Directory names to skip during file-index scanning.
pub(super) const IGNORED_INDEX_DIRS: &[&str] =
    &["node_modules", "target", "__pycache__", "venv", "vendor"];

/// Reason a completed index intentionally represents only a bounded prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileIndexTruncationReason {
    FileLimit,
    DirectoryRetentionLimit,
    RetainedByteLimit,
    BuildByteLimit,
}

/// Retained-state and traversal evidence for one file-index build.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileIndexBuildMetrics {
    pub examined_directory_entries: usize,
    pub scanned_directories: usize,
    pub peak_retained_directories: usize,
    pub retained_files: usize,
    /// Peak scan batch plus pending directory work retained at the same time.
    pub peak_retained_directory_entries: usize,
    pub identity_failures: usize,
    /// Current conservatively charged construction bytes at terminal publication.
    pub current_build_bytes: u64,
    /// Peak conservatively charged construction bytes across output and scratch.
    pub peak_build_bytes: u64,
    /// Complete installed index graph retained by the terminal result.
    pub retained_index_bytes: u64,
    pub truncation: Option<FileIndexTruncationReason>,
}

/// O(1) build/output byte accounting used before every retained insertion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileIndexBuildLedger {
    current_build_bytes: u64,
    peak_build_bytes: u64,
    installed_bytes: u64,
    build_limit: u64,
}

/// O(1) installed-graph accounting shared across one incremental mutation batch.
pub(crate) struct FileIndexMutationLedger {
    retained_bytes: u64,
    peak_retained_bytes: u64,
    truncated: bool,
}

impl FileIndexMutationLedger {
    fn from_index(index: &FileIndex) -> Self {
        let retained_bytes = index.retained_byte_weight();
        debug_assert!(retained_bytes <= MAX_FILE_INDEX_RETAINED_BYTES);
        Self {
            retained_bytes,
            peak_retained_bytes: retained_bytes,
            truncated: false,
        }
    }

    fn try_add(&mut self, bytes: u64) -> bool {
        let Some(next) = self.retained_bytes.checked_add(bytes) else {
            self.truncated = true;
            return false;
        };
        if next > MAX_FILE_INDEX_RETAINED_BYTES {
            self.truncated = true;
            return false;
        }
        self.retained_bytes = next;
        self.peak_retained_bytes = self.peak_retained_bytes.max(next);
        true
    }

    fn try_replace(&mut self, removed: u64, added: u64) -> bool {
        let retained_without_old = self.retained_bytes.saturating_sub(removed);
        let Some(next) = retained_without_old.checked_add(added) else {
            self.truncated = true;
            return false;
        };
        if next > MAX_FILE_INDEX_RETAINED_BYTES {
            self.truncated = true;
            return false;
        }
        self.retained_bytes = next;
        self.peak_retained_bytes = self.peak_retained_bytes.max(next);
        true
    }

    fn release(&mut self, bytes: u64) {
        self.retained_bytes = self.retained_bytes.saturating_sub(bytes);
    }

    fn sync_after_nonincreasing(&mut self, index: &FileIndex) {
        let actual = index.retained_byte_weight();
        debug_assert!(actual <= self.retained_bytes);
        self.retained_bytes = actual;
    }

    pub(crate) const fn retained_bytes(&self) -> u64 {
        self.retained_bytes
    }

    pub(crate) const fn peak_retained_bytes(&self) -> u64 {
        self.peak_retained_bytes
    }

    #[cfg(test)]
    pub(crate) const fn truncated(&self) -> bool {
        self.truncated
    }
}

impl FileIndexBuildLedger {
    const fn with_build_limit(build_limit: u64) -> Self {
        Self {
            current_build_bytes: 0,
            peak_build_bytes: 0,
            installed_bytes: 0,
            build_limit,
        }
    }

    fn try_charge_installed(&mut self, bytes: u64) -> Result<(), FileIndexTruncationReason> {
        let Some(installed) = self.installed_bytes.checked_add(bytes) else {
            return Err(FileIndexTruncationReason::RetainedByteLimit);
        };
        if installed > MAX_FILE_INDEX_RETAINED_BYTES {
            return Err(FileIndexTruncationReason::RetainedByteLimit);
        }
        if !self.try_charge_build(bytes) {
            return Err(FileIndexTruncationReason::BuildByteLimit);
        }
        self.installed_bytes = installed;
        Ok(())
    }

    fn try_charge_scratch(&mut self, bytes: u64) -> bool {
        self.try_charge_build(bytes)
    }

    fn try_charge_build(&mut self, bytes: u64) -> bool {
        let Some(next) = self.current_build_bytes.checked_add(bytes) else {
            return false;
        };
        if next > self.build_limit {
            return false;
        }
        self.current_build_bytes = next;
        self.peak_build_bytes = self.peak_build_bytes.max(next);
        true
    }

    fn release_scratch(&mut self, bytes: u64) {
        self.current_build_bytes = self.current_build_bytes.saturating_sub(bytes);
    }

    fn release_installed(&mut self, bytes: u64) {
        self.installed_bytes = self.installed_bytes.saturating_sub(bytes);
        self.current_build_bytes = self.current_build_bytes.saturating_sub(bytes);
    }

    fn observe_scratch_peak(&mut self, bytes: u64) -> bool {
        let Some(overlap) = self.current_build_bytes.checked_add(bytes) else {
            return false;
        };
        if overlap > self.build_limit {
            return false;
        }
        self.peak_build_bytes = self.peak_build_bytes.max(overlap);
        true
    }

    const fn remaining_build_bytes(self) -> u64 {
        self.build_limit.saturating_sub(self.current_build_bytes)
    }

    const fn installed_bytes(self) -> u64 {
        self.installed_bytes
    }

    fn publish(self, metrics: &mut FileIndexBuildMetrics, retained_index_bytes: u64) {
        metrics.current_build_bytes = retained_index_bytes;
        metrics.peak_build_bytes = self.peak_build_bytes.max(retained_index_bytes);
        metrics.retained_index_bytes = retained_index_bytes;
    }
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
pub type FileIndexBuildStart = SingleFlightStart<FileIndexBuildRequest>;

/// Scalar ownership evidence for file-index rebuild tests and readiness.
///
/// This is the shared single-flight snapshot, which additionally carries the
/// active/pending high-water marks the palette's own snapshot did not track.
pub type FileIndexBuildCoordinatorSnapshot = SingleFlightSnapshot;

/// Retain at most one active file-index build and one latest compact request.
///
/// A palette-named alias over the shared one-active/one-latest coordinator, the
/// way `services::palette::runtime` already aliases `PaletteSearchCoordinator`.
/// The hand-rolled duplicate this replaced had identical submit, finish,
/// invalidate, `is_current`, and `has_work` semantics; the shared type adds
/// `clear_pending()`, `active_generation()`, and the two high-water fields.
pub type FileIndexBuildCoordinator = SingleFlightCoordinator<FileIndexBuildRequest>;

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

    /// Re-scan the folder identities already owned by this installed index.
    ///
    /// Incremental queue overflow uses this worker-only path so a compact scalar
    /// rebuild request can replace an otherwise unbounded mutation backlog.
    pub(crate) fn rebuild_current_workspace_folders(&self) -> Self {
        let folders = self
            .workspace_folders
            .iter()
            .map(|folder| folder.as_ref().clone())
            .collect::<Vec<_>>();
        Self::rebuild(&folders)
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
        Self::rebuild_cancellable_with_limits(
            workspace_folders,
            capacity_hint,
            MAX_INDEXED_FILES,
            MAX_INDEXED_DIRECTORIES,
            MAX_FILE_INDEX_BUILD_RETAINED_BYTES,
            cancellation,
        )
    }

    fn rebuild_cancellable_with_limits(
        workspace_folders: &[PathBuf],
        capacity_hint: usize,
        file_limit: usize,
        directory_limit: usize,
        build_byte_limit: u64,
        cancellation: &PaletteSearchCancellation,
    ) -> FileIndexBuildOutcome {
        let mut ledger = FileIndexBuildLedger::with_build_limit(build_byte_limit);
        let maximum_file_capacity = usize::try_from(MAX_FILE_INDEX_RETAINED_BYTES)
            .unwrap_or(usize::MAX)
            .checked_div(std::mem::size_of::<IndexedFile>().max(1))
            .unwrap_or(0);
        let file_capacity = capacity_hint.min(file_limit).min(maximum_file_capacity);
        let file_shell_charge = vector_shell_bytes::<IndexedFile>(file_capacity);
        if ledger.try_charge_installed(file_shell_charge).is_err() {
            unreachable!("an empty file-index vector must fit its installed budget");
        }
        let folder_capacity = workspace_folders.len();
        let folder_shell_charge = vector_shell_bytes::<Arc<PathBuf>>(folder_capacity);
        if ledger.try_charge_installed(folder_shell_charge).is_err() {
            unreachable!("workspace-folder vector shells must fit the installed budget");
        }
        let mut files = Vec::with_capacity(file_capacity);
        let mut visited_directories = HashSet::new();
        let mut canonical_files = HashSet::new();
        let mut folder_arcs = Vec::with_capacity(folder_capacity);
        let mut visited_charge = 0u64;
        let mut canonical_files_charge = 0u64;
        let mut metrics = FileIndexBuildMetrics::default();
        for folder in workspace_folders {
            if cancellation.is_cancelled() {
                metrics.retained_files = files.len();
                ledger.release_scratch(visited_charge.saturating_add(canonical_files_charge));
                ledger.publish(&mut metrics, 0);
                return FileIndexBuildOutcome::Cancelled { metrics };
            }
            if files.len() >= file_limit {
                metrics.truncation = Some(FileIndexTruncationReason::FileLimit);
                break;
            }
            let Ok(canonical_folder) = fs_metadata::canonical_path(folder) else {
                continue;
            };
            let canonical_folder_charge = owned_path_bytes(&canonical_folder);
            if !ledger.try_charge_scratch(canonical_folder_charge) {
                metrics.truncation = Some(FileIndexTruncationReason::BuildByteLimit);
                break;
            }
            let folder_arc = Arc::new(folder.clone());
            let folder_graph_charge = shared_folder_graph_weight(&folder_arc);
            if let Err(reason) = ledger.try_charge_installed(folder_graph_charge) {
                ledger.release_scratch(canonical_folder_charge);
                metrics.truncation = Some(reason);
                break;
            }
            let completed = {
                let mut traversal = FileIndexTraversal {
                    out: &mut files,
                    visited_directories: &mut visited_directories,
                    canonical_files: &mut canonical_files,
                    canonical_folder: &canonical_folder,
                    file_limit,
                    directory_limit,
                    cancellation,
                    metrics: &mut metrics,
                    ledger: &mut ledger,
                    visited_charge: &mut visited_charge,
                    canonical_files_charge: &mut canonical_files_charge,
                };
                collect_files_bounded(folder, &folder_arc, &mut traversal)
            };
            ledger.release_scratch(canonical_folder_charge);
            if !completed {
                metrics.retained_files = files.len();
                ledger.release_scratch(visited_charge.saturating_add(canonical_files_charge));
                ledger.publish(&mut metrics, 0);
                return FileIndexBuildOutcome::Cancelled { metrics };
            }
            folder_arcs.push(folder_arc);
            if metrics.truncation.is_some_and(is_file_index_byte_limit) {
                break;
            }
        }
        ledger.release_scratch(visited_charge.saturating_add(canonical_files_charge));
        truncate_to_index_limit(&mut files, file_limit);
        if files.len() == file_limit && metrics.truncation.is_some() {
            metrics.truncation = Some(FileIndexTruncationReason::FileLimit);
        }
        if files.is_empty() {
            files.shrink_to_fit();
            ledger.release_installed(file_shell_charge);
        }
        if folder_arcs.is_empty() {
            folder_arcs.shrink_to_fit();
            ledger.release_installed(folder_shell_charge);
        }
        let index = Self {
            files,
            workspace_folders: folder_arcs,
        };
        metrics.retained_files = index.files.len();
        let retained_index_bytes = ledger.installed_bytes();
        debug_assert_eq!(
            retained_index_bytes,
            index.retained_byte_weight(),
            "incremental file-index charges must match the installed ownership graph"
        );
        ledger.publish(&mut metrics, retained_index_bytes);
        debug_assert!(metrics.peak_build_bytes <= MAX_FILE_INDEX_BUILD_RETAINED_BYTES);
        debug_assert!(retained_index_bytes <= MAX_FILE_INDEX_RETAINED_BYTES);
        FileIndexBuildOutcome::Complete { index, metrics }
    }

    #[cfg(test)]
    pub(super) fn rebuild_cancellable_for_test(
        workspace_folders: &[PathBuf],
        file_limit: usize,
        cancellation: &PaletteSearchCancellation,
    ) -> FileIndexBuildOutcome {
        Self::rebuild_cancellable_with_limits(
            workspace_folders,
            file_limit,
            file_limit,
            MAX_INDEXED_DIRECTORIES,
            MAX_FILE_INDEX_BUILD_RETAINED_BYTES,
            cancellation,
        )
    }

    #[cfg(test)]
    pub(super) fn rebuild_cancellable_with_limits_for_test(
        workspace_folders: &[PathBuf],
        file_limit: usize,
        directory_limit: usize,
        cancellation: &PaletteSearchCancellation,
    ) -> FileIndexBuildOutcome {
        Self::rebuild_cancellable_with_limits(
            workspace_folders,
            file_limit,
            file_limit,
            directory_limit,
            MAX_FILE_INDEX_BUILD_RETAINED_BYTES,
            cancellation,
        )
    }

    #[cfg(test)]
    pub(super) fn rebuild_cancellable_with_build_limit_for_test(
        workspace_folders: &[PathBuf],
        file_limit: usize,
        directory_limit: usize,
        build_byte_limit: u64,
        cancellation: &PaletteSearchCancellation,
    ) -> FileIndexBuildOutcome {
        Self::rebuild_cancellable_with_limits(
            workspace_folders,
            0,
            file_limit,
            directory_limit,
            build_byte_limit,
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

    /// Return every heap allocation retained by this index owner.
    #[must_use]
    pub fn retained_byte_weight(&self) -> u64 {
        let file_shells = retained_bytes(
            self.files
                .capacity()
                .saturating_mul(std::mem::size_of::<IndexedFile>()),
        );
        let folder_shells = retained_bytes(
            self.workspace_folders
                .capacity()
                .saturating_mul(std::mem::size_of::<Arc<PathBuf>>()),
        );
        let file_graph = self.files.iter().fold(0u64, |total, file| {
            total.saturating_add(indexed_file_graph_weight(file))
        });
        let folder_graph = self.workspace_folders.iter().fold(0u64, |total, folder| {
            total.saturating_add(shared_folder_graph_weight(folder))
        });
        file_shells
            .saturating_add(folder_shells)
            .saturating_add(file_graph)
            .saturating_add(folder_graph)
    }

    /// Truncate to a prefix whose complete retained heap graph fits the lane budget.
    ///
    /// Returns whether any folder identity or file row was omitted.
    pub fn enforce_retained_byte_limit(&mut self) -> bool {
        if self.files.is_empty() {
            self.files.shrink_to_fit();
        }
        if self.workspace_folders.is_empty() {
            self.workspace_folders.shrink_to_fit();
        }
        if self.retained_byte_weight() <= MAX_FILE_INDEX_RETAINED_BYTES {
            return false;
        }
        let original_file_count = self.files.len();
        let original_folder_count = self.workspace_folders.len();

        let mut retained_folders = Vec::new();
        let mut retained_folder_ptrs = HashSet::new();
        let mut folder_graph = 0u64;
        for folder in self.workspace_folders.drain(..) {
            let next_graph = folder_graph
                .saturating_add(shared_folder_graph_weight(&folder))
                .saturating_add(retained_bytes(std::mem::size_of::<Arc<PathBuf>>()));
            if next_graph > MAX_FILE_INDEX_RETAINED_BYTES {
                continue;
            }
            folder_graph = next_graph;
            retained_folder_ptrs.insert(Arc::as_ptr(&folder) as usize);
            retained_folders.push(folder);
        }
        self.workspace_folders = retained_folders.into_boxed_slice().into_vec();

        let mut retained_files = Vec::new();
        let mut file_graph = 0u64;
        for file in self.files.drain(..) {
            if !retained_folder_ptrs.contains(&(Arc::as_ptr(&file.workspace_folder) as usize)) {
                continue;
            }
            let next_graph = folder_graph
                .saturating_add(file_graph)
                .saturating_add(indexed_file_graph_weight(&file))
                .saturating_add(retained_bytes(
                    retained_files
                        .len()
                        .saturating_add(1)
                        .saturating_mul(std::mem::size_of::<IndexedFile>()),
                ));
            if next_graph > MAX_FILE_INDEX_RETAINED_BYTES {
                continue;
            }
            file_graph = file_graph.saturating_add(indexed_file_graph_weight(&file));
            retained_files.push(file);
        }
        self.files = retained_files.into_boxed_slice().into_vec();

        debug_assert!(self.retained_byte_weight() <= MAX_FILE_INDEX_RETAINED_BYTES);
        self.files.len() != original_file_count
            || self.workspace_folders.len() != original_folder_count
    }

    /// Add a single file to the index. Used for incremental sidebar updates.
    pub fn add_file(&mut self, file: IndexedFile) {
        let mut ledger = self.incremental_mutation_ledger();
        self.add_file_for_bounded_batch(file, &mut ledger);
    }

    pub(crate) fn incremental_mutation_ledger(&self) -> FileIndexMutationLedger {
        FileIndexMutationLedger::from_index(self)
    }

    pub(crate) fn add_file_for_bounded_batch(
        &mut self,
        file: IndexedFile,
        ledger: &mut FileIndexMutationLedger,
    ) -> bool {
        if self.files.len() >= MAX_INDEXED_FILES
            || self.files.iter().any(|existing| {
                existing.path == file.path
                    || canonical_identities_match(&existing.identity, &file.identity)
            })
        {
            return false;
        }
        let needs_folder = !self
            .workspace_folders
            .iter()
            .any(|existing| Arc::ptr_eq(existing, &file.workspace_folder));
        let file_shell_growth = vector_shell_growth_for_one(&self.files);
        let folder_shell_growth = if needs_folder {
            vector_shell_growth_for_one(&self.workspace_folders)
        } else {
            0
        };
        let retained_growth = file_shell_growth
            .saturating_add(folder_shell_growth)
            .saturating_add(indexed_file_graph_weight(&file))
            .saturating_add(if needs_folder {
                shared_folder_graph_weight(&file.workspace_folder)
            } else {
                0
            });
        if !ledger.try_add(retained_growth) {
            return false;
        }
        if needs_folder {
            reserve_exactly_one(&mut self.workspace_folders);
            self.workspace_folders
                .push(Arc::clone(&file.workspace_folder));
        }
        reserve_exactly_one(&mut self.files);
        self.files.push(file);
        debug_assert_eq!(ledger.retained_bytes(), self.retained_byte_weight());
        true
    }

    /// Resolve and add one filesystem path through the metadata boundary.
    ///
    /// Callers must keep this operation on a background worker.
    pub fn add_path(&mut self, path: PathBuf, workspace_folder: Arc<PathBuf>) {
        self.add_file(indexed_file_from_path(path, workspace_folder));
    }

    pub(crate) fn add_path_for_bounded_batch(
        &mut self,
        path: PathBuf,
        workspace_folder: Arc<PathBuf>,
        ledger: &mut FileIndexMutationLedger,
    ) -> bool {
        self.add_file_for_bounded_batch(indexed_file_from_path(path, workspace_folder), ledger)
    }

    /// Remove a file (or all files under a directory) from the index.
    pub fn remove_path(&mut self, path: &Path) {
        let mut ledger = self.incremental_mutation_ledger();
        self.remove_path_for_bounded_batch(path, &mut ledger);
    }

    pub(crate) fn remove_path_for_bounded_batch(
        &mut self,
        path: &Path,
        ledger: &mut FileIndexMutationLedger,
    ) {
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
        ledger.sync_after_nonincreasing(self);
    }

    /// Rename a file or directory in the index.
    pub fn rename_path(&mut self, old_path: &Path, new_path: &Path) {
        let mut ledger = self.incremental_mutation_ledger();
        self.rename_path_for_bounded_batch(old_path, new_path, &mut ledger);
    }

    pub(crate) fn rename_path_for_bounded_batch(
        &mut self,
        old_path: &Path,
        new_path: &Path,
        ledger: &mut FileIndexMutationLedger,
    ) {
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
                let replacement =
                    indexed_file_from_path(replacement_path, Arc::clone(&file.workspace_folder));
                let previous_weight = indexed_file_graph_weight(file);
                let replacement_weight = indexed_file_graph_weight(&replacement);
                if ledger.try_replace(previous_weight, replacement_weight) {
                    *file = replacement;
                } else {
                    ledger.release(previous_weight);
                    file.path.clear();
                    file.name.clear();
                    file.identity =
                        PaletteFileIdentity::Unavailable(PaletteFileIdentityFailure::NotResolved);
                }
            }
        }
        self.files.retain(|file| !file.path.as_os_str().is_empty());
        deduplicate_files(&mut self.files);
        ledger.sync_after_nonincreasing(self);
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
        let mut index = Self {
            files: retained,
            workspace_folders,
        };
        index.enforce_retained_byte_limit();
        index
    }
}

fn retained_bytes(bytes: usize) -> u64 {
    u64::try_from(bytes).unwrap_or(u64::MAX)
}

fn vector_shell_bytes<T>(capacity: usize) -> u64 {
    retained_bytes(capacity.saturating_mul(std::mem::size_of::<T>()))
}

fn vector_shell_growth_for_one<T>(values: &Vec<T>) -> u64 {
    if values.len() < values.capacity() {
        return 0;
    }
    vector_shell_bytes::<T>(next_incremental_vec_capacity(values.capacity()))
        .saturating_sub(vector_shell_bytes::<T>(values.capacity()))
}

fn reserve_exactly_one<T>(values: &mut Vec<T>) {
    if values.len() == values.capacity() {
        let next_capacity = next_incremental_vec_capacity(values.capacity());
        values.reserve_exact(next_capacity.saturating_sub(values.capacity()));
    }
}

fn next_incremental_vec_capacity(current: usize) -> usize {
    current.max(4).saturating_mul(2)
}

fn owned_path_bytes(path: &PathBuf) -> u64 {
    retained_bytes(
        std::mem::size_of::<PathBuf>()
            .saturating_add(path.capacity())
            .saturating_add(std::mem::size_of::<usize>().saturating_mul(2)),
    )
}

/// Conservative per-key weight for a retained path plus its hash-table bucket,
/// control bytes, and allocator slack. Charging per key avoids rescanning set
/// capacity while still preceding every insertion.
fn hashed_path_bytes(path: &PathBuf) -> u64 {
    owned_path_bytes(path).saturating_add(retained_bytes(
        std::mem::size_of::<PathBuf>()
            .saturating_add(std::mem::size_of::<usize>().saturating_mul(2)),
    ))
}

fn path_capacity(path: &PathBuf) -> usize {
    path.capacity()
}

fn indexed_file_graph_weight(file: &IndexedFile) -> u64 {
    let canonical_capacity = match &file.identity {
        PaletteFileIdentity::Canonical(path) => path_capacity(path),
        PaletteFileIdentity::Unavailable(_) => 0,
    };
    retained_bytes(
        path_capacity(&file.path)
            .saturating_add(canonical_capacity)
            .saturating_add(file.name.capacity()),
    )
}

fn shared_folder_graph_weight(folder: &Arc<PathBuf>) -> u64 {
    retained_bytes(
        std::mem::size_of::<PathBuf>()
            .saturating_add(std::mem::size_of::<usize>().saturating_mul(2))
            .saturating_add(path_capacity(folder.as_ref())),
    )
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
        .and_then(OsStr::to_str)
        .is_some_and(|name| IGNORED_INDEX_DIRS.contains(&name))
}

struct FileIndexTraversal<'a> {
    out: &'a mut Vec<IndexedFile>,
    visited_directories: &'a mut HashSet<PathBuf>,
    canonical_files: &'a mut HashSet<PathBuf>,
    canonical_folder: &'a Path,
    file_limit: usize,
    directory_limit: usize,
    cancellation: &'a PaletteSearchCancellation,
    metrics: &'a mut FileIndexBuildMetrics,
    ledger: &'a mut FileIndexBuildLedger,
    visited_charge: &'a mut u64,
    canonical_files_charge: &'a mut u64,
}

fn collect_files_bounded(
    dir: &Path,
    workspace_folder: &Arc<PathBuf>,
    traversal: &mut FileIndexTraversal<'_>,
) -> bool {
    let Ok(canonical_directory) = fs_metadata::canonical_path(dir) else {
        return true;
    };
    if !canonical_directory.starts_with(traversal.canonical_folder)
        || traversal.visited_directories.contains(&canonical_directory)
    {
        return true;
    }
    if traversal.visited_directories.len() >= traversal.directory_limit {
        traversal
            .metrics
            .truncation
            .get_or_insert(FileIndexTruncationReason::DirectoryRetentionLimit);
        return true;
    }
    let root_visited_charge = hashed_path_bytes(&canonical_directory);
    if !traversal.ledger.try_charge_scratch(root_visited_charge) {
        traversal.metrics.truncation = Some(FileIndexTruncationReason::BuildByteLimit);
        return true;
    }
    *traversal.visited_charge = traversal.visited_charge.saturating_add(root_visited_charge);
    traversal.visited_directories.insert(canonical_directory);

    // Scope-own the pending-stack scratch: whatever graph/shell scratch remains
    // charged when the traversal loop exits — for any reason — is released
    // exactly once here, instead of on each of the loop's early-exit paths.
    // (`release_scratch` never changes the peak, so this single release is
    // accounting-equivalent to the per-exit releases it replaces.)
    let mut pending: Vec<(PathBuf, u32)> = Vec::new();
    let mut pending_shell_charge = 0u64;
    let mut pending_graph_charge = 0u64;
    let result = collect_pending_directories(
        dir,
        workspace_folder,
        traversal,
        &mut pending,
        &mut pending_shell_charge,
        &mut pending_graph_charge,
    );
    traversal
        .ledger
        .release_scratch(pending_graph_charge.saturating_add(pending_shell_charge));
    result
}

/// Run the pending-directory traversal loop.
///
/// Every exit path here is a bare `return`; the pending-stack scratch
/// (`pending_graph_charge` + `pending_shell_charge`) is released once by the
/// scope owner [`collect_files_bounded`]. The per-directory scan scratch is
/// released at a single point per iteration, whatever exit the entry loop takes.
fn collect_pending_directories(
    root: &Path,
    workspace_folder: &Arc<PathBuf>,
    traversal: &mut FileIndexTraversal<'_>,
    pending: &mut Vec<(PathBuf, u32)>,
    pending_shell_charge: &mut u64,
    pending_graph_charge: &mut u64,
) -> bool {
    if !ensure_scratch_vec_slot(pending, traversal.ledger, pending_shell_charge) {
        traversal.metrics.truncation = Some(FileIndexTruncationReason::BuildByteLimit);
        return true;
    }
    let root_path = root.to_path_buf();
    let root_pending_charge = owned_path_bytes(&root_path);
    if !traversal.ledger.try_charge_scratch(root_pending_charge) {
        traversal.metrics.truncation = Some(FileIndexTruncationReason::BuildByteLimit);
        return true;
    }
    *pending_graph_charge = pending_graph_charge.saturating_add(root_pending_charge);
    pending.push((root_path, 0u32));

    while let Some((dir, depth)) = pending.pop() {
        let popped_charge = owned_path_bytes(&dir);
        *pending_graph_charge = pending_graph_charge.saturating_sub(popped_charge);
        traversal.ledger.release_scratch(popped_charge);
        if traversal.cancellation.is_cancelled() {
            return false;
        }
        if traversal.out.len() >= traversal.file_limit {
            traversal.metrics.truncation = Some(FileIndexTruncationReason::FileLimit);
            return true;
        }

        traversal.metrics.peak_retained_directories = traversal
            .metrics
            .peak_retained_directories
            .max(traversal.visited_directories.len());
        let remaining = traversal.file_limit.saturating_sub(traversal.out.len());
        let scan = file_tree::scan_directory_bounded_with_cancel_and_bytes(
            &dir,
            remaining,
            0,
            traversal.ledger.remaining_build_bytes(),
            || traversal.cancellation.is_cancelled(),
        );
        if !traversal
            .ledger
            .observe_scratch_peak(scan.peak_retained_bytes)
        {
            traversal.metrics.truncation = Some(FileIndexTruncationReason::BuildByteLimit);
            return true;
        }
        traversal.metrics.scanned_directories =
            traversal.metrics.scanned_directories.saturating_add(1);
        traversal.metrics.examined_directory_entries = traversal
            .metrics
            .examined_directory_entries
            .saturating_add(scan.examined_entries);
        traversal.metrics.peak_retained_directory_entries = traversal
            .metrics
            .peak_retained_directory_entries
            .max(pending.len().saturating_add(scan.peak_retained_entries));
        if scan.cancelled {
            return false;
        }
        if scan.truncated {
            traversal
                .metrics
                .truncation
                .get_or_insert(FileIndexTruncationReason::DirectoryRetentionLimit);
        }
        if !traversal.ledger.try_charge_scratch(scan.retained_bytes) {
            traversal.metrics.truncation = Some(FileIndexTruncationReason::BuildByteLimit);
            return true;
        }
        let scan_charge = scan.retained_bytes;
        let scan_byte_truncated = scan.byte_truncated;

        // The entry loop signals a whole-traversal exit through `early_return`
        // rather than releasing scan scratch on each exit path: `scan_charge`
        // is released once below, whatever exit the loop takes.
        let mut early_return: Option<bool> = None;
        for entry in scan.entries {
            if traversal.cancellation.is_cancelled() {
                early_return = Some(false);
                break;
            }
            if traversal.out.len() >= traversal.file_limit {
                traversal.metrics.truncation = Some(FileIndexTruncationReason::FileLimit);
                early_return = Some(true);
                break;
            }
            if entry.is_dir {
                if is_ignored_index_dir(&entry.path) {
                    continue;
                }
                let child_depth = depth.saturating_add(1);
                if child_depth > MAX_SCAN_DEPTH {
                    tracing::warn!(
                        "Skipping deeply nested directory (depth > {MAX_SCAN_DEPTH}): {}",
                        entry.path.display()
                    );
                    continue;
                }
                let Ok(canonical) = fs_metadata::canonical_path(&entry.path) else {
                    continue;
                };
                if !canonical.starts_with(traversal.canonical_folder)
                    || traversal.visited_directories.contains(&canonical)
                {
                    continue;
                }
                if traversal.visited_directories.len() >= traversal.directory_limit {
                    traversal
                        .metrics
                        .truncation
                        .get_or_insert(FileIndexTruncationReason::DirectoryRetentionLimit);
                    continue;
                }
                let visited_charge = hashed_path_bytes(&canonical);
                if !traversal.ledger.try_charge_scratch(visited_charge) {
                    traversal.metrics.truncation = Some(FileIndexTruncationReason::BuildByteLimit);
                    break;
                }
                *traversal.visited_charge = traversal.visited_charge.saturating_add(visited_charge);
                traversal.visited_directories.insert(canonical);
                if !ensure_scratch_vec_slot(pending, traversal.ledger, pending_shell_charge) {
                    traversal.metrics.truncation = Some(FileIndexTruncationReason::BuildByteLimit);
                    break;
                }
                let pending_charge = owned_path_bytes(&entry.path);
                if !traversal.ledger.try_charge_scratch(pending_charge) {
                    traversal.metrics.truncation = Some(FileIndexTruncationReason::BuildByteLimit);
                    break;
                }
                *pending_graph_charge = pending_graph_charge.saturating_add(pending_charge);
                pending.push((entry.path, child_depth));
            } else {
                let file = indexed_file_from_path(entry.path, Arc::clone(workspace_folder));
                if matches!(file.identity, PaletteFileIdentity::Unavailable(_)) {
                    traversal.metrics.identity_failures =
                        traversal.metrics.identity_failures.saturating_add(1);
                }
                if let Some(canonical) = file.identity.canonical_path() {
                    if traversal.canonical_files.contains(canonical) {
                        continue;
                    }
                    let canonical = canonical.to_path_buf();
                    let canonical_charge = hashed_path_bytes(&canonical);
                    if !traversal.ledger.try_charge_scratch(canonical_charge) {
                        traversal.metrics.truncation =
                            Some(FileIndexTruncationReason::BuildByteLimit);
                        break;
                    }
                    *traversal.canonical_files_charge = traversal
                        .canonical_files_charge
                        .saturating_add(canonical_charge);
                    let inserted = traversal.canonical_files.insert(canonical);
                    debug_assert!(inserted, "canonical identity changed during insertion");
                }
                if let Err(reason) = ensure_installed_vec_slot(traversal.out, traversal.ledger) {
                    traversal.metrics.truncation = Some(reason);
                    break;
                }
                let graph_charge = indexed_file_graph_weight(&file);
                if let Err(reason) = traversal.ledger.try_charge_installed(graph_charge) {
                    traversal.metrics.truncation = Some(reason);
                    break;
                }
                traversal.out.push(file);
            }
        }
        traversal.ledger.release_scratch(scan_charge);
        if let Some(result) = early_return {
            return result;
        }
        if scan_byte_truncated {
            traversal.metrics.truncation = Some(FileIndexTruncationReason::BuildByteLimit);
        }
        if traversal
            .metrics
            .truncation
            .is_some_and(is_file_index_byte_limit)
        {
            break;
        }
    }
    true
}

fn ensure_scratch_vec_slot<T>(
    values: &mut Vec<T>,
    ledger: &mut FileIndexBuildLedger,
    charged_shell_bytes: &mut u64,
) -> bool {
    if values.len() < values.capacity() {
        return true;
    }
    let next_capacity = values.capacity().max(4).saturating_mul(2);
    let next_shell_bytes = vector_shell_bytes::<T>(next_capacity);
    let extra = next_shell_bytes.saturating_sub(*charged_shell_bytes);
    if !ledger.try_charge_scratch(extra) {
        return false;
    }
    values.reserve(next_capacity.saturating_sub(values.capacity()));
    *charged_shell_bytes = next_shell_bytes;
    true
}

fn ensure_installed_vec_slot<T>(
    values: &mut Vec<T>,
    ledger: &mut FileIndexBuildLedger,
) -> Result<(), FileIndexTruncationReason> {
    if values.len() < values.capacity() {
        return Ok(());
    }
    let old_capacity = values.capacity();
    let next_capacity = old_capacity.max(4).saturating_mul(2);
    let extra = vector_shell_bytes::<T>(next_capacity.saturating_sub(old_capacity));
    ledger.try_charge_installed(extra)?;
    values.reserve(next_capacity.saturating_sub(old_capacity));
    Ok(())
}

fn is_file_index_byte_limit(reason: FileIndexTruncationReason) -> bool {
    matches!(
        reason,
        FileIndexTruncationReason::RetainedByteLimit | FileIndexTruncationReason::BuildByteLimit
    )
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

#[cfg(test)]
mod build_ledger_tests {
    use super::*;

    #[test]
    fn build_ledger_accepts_exact_limits_and_rejects_one_byte_over() {
        let mut build = FileIndexBuildLedger::with_build_limit(MAX_FILE_INDEX_BUILD_RETAINED_BYTES);
        assert!(build.try_charge_scratch(MAX_FILE_INDEX_BUILD_RETAINED_BYTES));
        assert!(!build.try_charge_scratch(1));
        build.release_scratch(MAX_FILE_INDEX_BUILD_RETAINED_BYTES);

        let mut installed =
            FileIndexBuildLedger::with_build_limit(MAX_FILE_INDEX_BUILD_RETAINED_BYTES);
        assert_eq!(
            installed.try_charge_installed(MAX_FILE_INDEX_RETAINED_BYTES),
            Ok(())
        );
        assert_eq!(
            installed.try_charge_installed(1),
            Err(FileIndexTruncationReason::RetainedByteLimit)
        );
        assert_eq!(installed.installed_bytes, MAX_FILE_INDEX_RETAINED_BYTES);
        assert_eq!(installed.peak_build_bytes, MAX_FILE_INDEX_RETAINED_BYTES);

        let mut build_limited =
            FileIndexBuildLedger::with_build_limit(MAX_FILE_INDEX_RETAINED_BYTES - 1);
        assert_eq!(
            build_limited.try_charge_installed(MAX_FILE_INDEX_RETAINED_BYTES),
            Err(FileIndexTruncationReason::BuildByteLimit)
        );
        assert_eq!(build_limited.installed_bytes, 0);
    }

    #[test]
    fn incremental_long_path_batch_never_crosses_the_installed_policy() {
        let folder = Arc::new(PathBuf::from("/synthetic/incremental-byte-policy"));
        let mut index = FileIndex::default();
        let mut ledger = index.incremental_mutation_ledger();

        for item in 0..512 {
            let mut path = folder.join(format!("file-{item:05}.rs"));
            path.reserve(256 * 1024);
            let file = IndexedFile::new(
                path,
                PaletteFileIdentity::Unavailable(PaletteFileIdentityFailure::NotFound),
                Arc::clone(&folder),
            );
            index.add_file_for_bounded_batch(file, &mut ledger);
            assert!(ledger.retained_bytes() <= MAX_FILE_INDEX_RETAINED_BYTES);
            assert!(ledger.peak_retained_bytes() <= MAX_FILE_INDEX_RETAINED_BYTES);
        }

        assert!(ledger.truncated());
        assert_eq!(ledger.retained_bytes(), index.retained_byte_weight());
        assert!(index.retained_byte_weight() <= MAX_FILE_INDEX_RETAINED_BYTES);

        let long_prefix = PathBuf::from(format!("/renamed/{}", "y".repeat(256 * 1024)));
        index.rename_path_for_bounded_batch(folder.as_path(), &long_prefix, &mut ledger);
        assert_eq!(ledger.retained_bytes(), index.retained_byte_weight());
        assert!(ledger.peak_retained_bytes() <= MAX_FILE_INDEX_RETAINED_BYTES);
        assert!(index.retained_byte_weight() <= MAX_FILE_INDEX_RETAINED_BYTES);
    }

    /// Drive the traversal on a real fixture and return the ledger plus the
    /// scratch the *caller* (not the traversal) still owns after it returns.
    fn run_traversal(
        root: &Path,
        build_byte_limit: u64,
        file_limit: usize,
        cancellation: &PaletteSearchCancellation,
    ) -> (bool, FileIndexBuildLedger, Vec<IndexedFile>, u64, u64) {
        let mut ledger = FileIndexBuildLedger::with_build_limit(build_byte_limit);
        // Pre-size `out` so `ensure_installed_vec_slot` never charges an
        // incremental installed shell, keeping the leak assertion exact.
        let mut files = Vec::with_capacity(1024);
        let mut visited_directories = HashSet::new();
        let mut canonical_files = HashSet::new();
        let mut metrics = FileIndexBuildMetrics::default();
        let mut visited_charge = 0u64;
        let mut canonical_files_charge = 0u64;
        let canonical_folder = fs_metadata::canonical_path(root).expect("canonical root");
        let folder_arc = Arc::new(root.to_path_buf());
        let completed = {
            let mut traversal = FileIndexTraversal {
                out: &mut files,
                visited_directories: &mut visited_directories,
                canonical_files: &mut canonical_files,
                canonical_folder: &canonical_folder,
                file_limit,
                directory_limit: MAX_INDEXED_DIRECTORIES,
                cancellation,
                metrics: &mut metrics,
                ledger: &mut ledger,
                visited_charge: &mut visited_charge,
                canonical_files_charge: &mut canonical_files_charge,
            };
            collect_files_bounded(root, &folder_arc, &mut traversal)
        };
        (
            completed,
            ledger,
            files,
            visited_charge,
            canonical_files_charge,
        )
    }

    /// After the traversal returns for any reason, releasing the caller-owned
    /// scratch (`visited`/`canonical`) and the installed graph weight of every
    /// admitted file must bring the build ledger back to zero. Any residual
    /// means pending-stack or scan scratch leaked on that exit path.
    fn assert_scratch_fully_released(
        mut ledger: FileIndexBuildLedger,
        files: &[IndexedFile],
        visited_charge: u64,
        canonical_files_charge: u64,
    ) {
        ledger.release_scratch(visited_charge.saturating_add(canonical_files_charge));
        let installed: u64 = files.iter().map(indexed_file_graph_weight).sum();
        ledger.release_installed(installed);
        assert_eq!(
            ledger.current_build_bytes, 0,
            "pending-stack or scan scratch leaked on the traversal's exit path"
        );
    }

    #[test]
    fn traversal_normal_completion_releases_all_scratch_and_skips_ignored_dirs() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        crate::services::filesystem::fixture::write_text(&dir.path().join("a.rs"), "fn a() {}");
        crate::services::filesystem::fixture::write_text(&dir.path().join("b.md"), "# b");
        crate::services::filesystem::fixture::create_dir_all(&dir.path().join("nested"));
        crate::services::filesystem::fixture::write_text(
            &dir.path().join("nested/c.toml"),
            "k = 1",
        );
        // A filtered batch: an ignored directory whose contents must be skipped.
        crate::services::filesystem::fixture::create_dir_all(&dir.path().join(".git"));
        crate::services::filesystem::fixture::write_text(&dir.path().join(".git/config"), "x");

        let cancellation = PaletteSearchCancellation::default();
        let (completed, ledger, files, visited, canonical) = run_traversal(
            dir.path(),
            MAX_FILE_INDEX_BUILD_RETAINED_BYTES,
            MAX_INDEXED_FILES,
            &cancellation,
        );

        assert!(completed, "an uncancelled traversal reports completion");
        assert!(
            files.iter().all(|file| !file
                .path
                .components()
                .any(|component| component.as_os_str() == ".git")),
            "ignored directory contents must not be indexed"
        );
        assert_eq!(files.len(), 3, "three non-ignored files are indexed");
        assert_scratch_fully_released(ledger, &files, visited, canonical);
    }

    #[test]
    fn traversal_budget_rejection_releases_all_scratch() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        for index in 0..8 {
            crate::services::filesystem::fixture::write_text(
                &dir.path().join(format!("file-{index}.rs")),
                "fn placeholder() {}",
            );
        }

        // A build-byte limit large enough to admit the root scratch but small
        // enough to reject partway through the scan, exercising a mid-loop
        // BuildByteLimit exit.
        let cancellation = PaletteSearchCancellation::default();
        let (_completed, ledger, files, visited, canonical) =
            run_traversal(dir.path(), 512, MAX_INDEXED_FILES, &cancellation);

        assert_scratch_fully_released(ledger, &files, visited, canonical);
    }

    #[test]
    fn traversal_cancellation_releases_all_scratch() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        crate::services::filesystem::fixture::write_text(&dir.path().join("a.rs"), "fn a() {}");
        crate::services::filesystem::fixture::create_dir_all(&dir.path().join("nested"));
        crate::services::filesystem::fixture::write_text(
            &dir.path().join("nested/b.rs"),
            "fn b() {}",
        );

        // Supersession/cancellation: a pre-cancelled token exits the loop the
        // first time it is observed.
        let cancellation = PaletteSearchCancellation::default();
        let _ = cancellation.cancel();
        let (completed, ledger, files, visited, canonical) = run_traversal(
            dir.path(),
            MAX_FILE_INDEX_BUILD_RETAINED_BYTES,
            MAX_INDEXED_FILES,
            &cancellation,
        );

        assert!(!completed, "a cancelled traversal reports non-completion");
        assert_scratch_fully_released(ledger, &files, visited, canonical);
    }

    #[cfg(feature = "property-tests")]
    proptest::proptest! {
        #[test]
        fn build_ledger_never_crosses_its_ceiling(
            operations in proptest::collection::vec((proptest::prelude::any::<bool>(), 0u64..=4 * 1024 * 1024), 0..256),
        ) {
            let mut ledger = FileIndexBuildLedger::with_build_limit(
                MAX_FILE_INDEX_BUILD_RETAINED_BYTES,
            );
            let mut scratch = 0u64;
            for (charge, bytes) in operations {
                if charge {
                    if ledger.try_charge_scratch(bytes) {
                        scratch = scratch.saturating_add(bytes);
                    }
                } else {
                    let released = scratch.min(bytes);
                    scratch -= released;
                    ledger.release_scratch(released);
                }
                proptest::prop_assert!(
                    ledger.current_build_bytes <= MAX_FILE_INDEX_BUILD_RETAINED_BYTES
                );
                proptest::prop_assert!(
                    ledger.peak_build_bytes <= MAX_FILE_INDEX_BUILD_RETAINED_BYTES
                );
            }
        }
    }
}
