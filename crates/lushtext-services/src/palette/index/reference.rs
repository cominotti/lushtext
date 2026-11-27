// SPDX-License-Identifier: GPL-3.0-or-later

//! The per-file index the arena replaced, kept as the behavioural reference
//! (`scale-file-index` design D6): one owned row per file, `realpath` per
//! file and directory, linear deduplication. Test builds only; the property
//! target drives it beside [`super::FileIndex`] and compares the two.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::filesystem::metadata as fs_metadata;
use crate::workspace::file_tree;
use lushtext_model::workspace::palette::{
    FileOrdinal, PaletteFileIdentity, PaletteFileIdentityFailure, ScoredResult, SearchResultItem,
};
use lushtext_model::workspace::workspace_visibility::WorkspaceEntryVisibility;

use super::super::fuzzy::{
    CountedSelection, RestCount, search_items_cancellable, search_items_cancellable_counting,
};
use super::super::runtime::{PaletteSearchCancellation, PaletteSearchOutcome};
use super::{
    FileIndexBuildMetrics, FileIndexTruncationReason, IGNORED_INDEX_DIRS,
    MAX_FILE_INDEX_BUILD_RETAINED_BYTES, MAX_FILE_INDEX_RETAINED_BYTES, MAX_INDEXED_DIRECTORIES,
    MAX_INDEXED_FILES, MAX_SCAN_DEPTH, OwnedIndexedFile,
};

/// Typed terminal result of a reference build.
#[derive(Debug)]
pub enum ReferenceBuildOutcome {
    Complete {
        index: ReferenceFileIndex,
        metrics: FileIndexBuildMetrics,
    },
    Cancelled {
        metrics: FileIndexBuildMetrics,
    },
}

#[derive(Clone, Copy)]
struct ReferenceBuildLedger {
    current_build_bytes: u64,
    peak_build_bytes: u64,
    installed_bytes: u64,
    build_limit: u64,
}

/// O(1) installed-graph accounting shared across one incremental mutation batch.
#[doc(hidden)]
pub struct ReferenceMutationLedger {
    retained_bytes: u64,
    peak_retained_bytes: u64,
    truncated: bool,
}

impl ReferenceMutationLedger {
    fn from_index(index: &ReferenceFileIndex) -> Self {
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

    fn sync_after_nonincreasing(&mut self, index: &ReferenceFileIndex) {
        let actual = index.retained_byte_weight();
        debug_assert!(actual <= self.retained_bytes);
        self.retained_bytes = actual;
    }

    #[must_use]
    pub const fn retained_bytes(&self) -> u64 {
        self.retained_bytes
    }

    #[must_use]
    pub const fn peak_retained_bytes(&self) -> u64 {
        self.peak_retained_bytes
    }

    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

impl ReferenceBuildLedger {
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

/// In-memory index of all files across workspace folders.
#[derive(Debug, Default, Clone)]
pub struct ReferenceFileIndex {
    files: Vec<OwnedIndexedFile>,
    /// Deduplicated workspace folders for O(k) prefix lookups (k is usually small).
    workspace_folders: Vec<Arc<PathBuf>>,
}

impl ReferenceFileIndex {
    /// Build a file index by recursively scanning all workspace folders.
    #[must_use]
    pub fn rebuild(workspace_folders: &[PathBuf]) -> Self {
        Self::rebuild_with_hint(workspace_folders, 10_000)
    }

    /// Build a file index under an explicit workspace visibility rule.
    #[must_use]
    pub fn rebuild_with_visibility(
        workspace_folders: &[PathBuf],
        visibility: &WorkspaceEntryVisibility,
    ) -> Self {
        let cancellation = PaletteSearchCancellation::default();
        let ReferenceBuildOutcome::Complete { index, .. } = Self::rebuild_cancellable_with_hint(
            workspace_folders,
            10_000,
            &cancellation,
            visibility,
        ) else {
            unreachable!("a fresh index cancellation token cannot cancel");
        };
        index
    }

    /// Like [`Self::rebuild`], but uses `capacity_hint` for the initial `Vec` allocation.
    #[must_use]
    pub fn rebuild_with_hint(workspace_folders: &[PathBuf], capacity_hint: usize) -> Self {
        let cancellation = PaletteSearchCancellation::default();
        let ReferenceBuildOutcome::Complete { index, .. } = Self::rebuild_cancellable_with_hint(
            workspace_folders,
            capacity_hint,
            &cancellation,
            &WorkspaceEntryVisibility::default(),
        ) else {
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
        visibility: &WorkspaceEntryVisibility,
    ) -> ReferenceBuildOutcome {
        Self::rebuild_cancellable_with_limits(
            workspace_folders,
            capacity_hint,
            MAX_INDEXED_FILES,
            MAX_INDEXED_DIRECTORIES,
            MAX_FILE_INDEX_BUILD_RETAINED_BYTES,
            cancellation,
            visibility,
        )
    }

    /// Build under explicit (scaled) limits.
    pub fn rebuild_cancellable_with_limits(
        workspace_folders: &[PathBuf],
        capacity_hint: usize,
        file_limit: usize,
        directory_limit: usize,
        build_byte_limit: u64,
        cancellation: &PaletteSearchCancellation,
        visibility: &WorkspaceEntryVisibility,
    ) -> ReferenceBuildOutcome {
        let mut ledger = ReferenceBuildLedger::with_build_limit(build_byte_limit);
        let maximum_file_capacity = usize::try_from(MAX_FILE_INDEX_RETAINED_BYTES)
            .unwrap_or(usize::MAX)
            .checked_div(std::mem::size_of::<OwnedIndexedFile>().max(1))
            .unwrap_or(0);
        let file_capacity = capacity_hint.min(file_limit).min(maximum_file_capacity);
        let file_shell_charge = vector_shell_bytes::<OwnedIndexedFile>(file_capacity);
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
                return ReferenceBuildOutcome::Cancelled { metrics };
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
                let mut traversal = ReferenceTraversal {
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
                    visibility,
                };
                collect_files_bounded(folder, &folder_arc, &mut traversal)
            };
            ledger.release_scratch(canonical_folder_charge);
            if !completed {
                metrics.retained_files = files.len();
                ledger.release_scratch(visited_charge.saturating_add(canonical_files_charge));
                ledger.publish(&mut metrics, 0);
                return ReferenceBuildOutcome::Cancelled { metrics };
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
        ReferenceBuildOutcome::Complete { index, metrics }
    }

    #[must_use]
    pub fn files(&self) -> &[OwnedIndexedFile] {
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
                .saturating_mul(std::mem::size_of::<OwnedIndexedFile>()),
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
                        .saturating_mul(std::mem::size_of::<OwnedIndexedFile>()),
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
    pub fn add_file(&mut self, file: OwnedIndexedFile) {
        let mut ledger = self.incremental_mutation_ledger();
        self.add_file_for_bounded_batch(file, &mut ledger);
    }

    #[must_use]
    pub fn incremental_mutation_ledger(&self) -> ReferenceMutationLedger {
        ReferenceMutationLedger::from_index(self)
    }

    pub fn add_file_for_bounded_batch(
        &mut self,
        file: OwnedIndexedFile,
        ledger: &mut ReferenceMutationLedger,
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

    pub fn add_path_for_bounded_batch(
        &mut self,
        path: PathBuf,
        workspace_folder: Arc<PathBuf>,
        ledger: &mut ReferenceMutationLedger,
    ) -> bool {
        self.add_file_for_bounded_batch(indexed_file_from_path(path, workspace_folder), ledger)
    }

    /// Remove a file (or all files under a directory) from the index.
    pub fn remove_path(&mut self, path: &Path) {
        let mut ledger = self.incremental_mutation_ledger();
        self.remove_path_for_bounded_batch(path, &mut ledger);
    }

    pub fn remove_path_for_bounded_batch(
        &mut self,
        path: &Path,
        ledger: &mut ReferenceMutationLedger,
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

    pub fn rename_path_for_bounded_batch(
        &mut self,
        old_path: &Path,
        new_path: &Path,
        ledger: &mut ReferenceMutationLedger,
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

    fn ordinals(&self) -> impl ExactSizeIterator<Item = u32> + '_ {
        0..u32::try_from(self.files.len()).unwrap_or(u32::MAX)
    }

    /// Search with cooperative cancellation, as the arena's search does.
    #[must_use]
    pub fn search_cancellable(
        &self,
        query: &str,
        max: usize,
        cancellation: &PaletteSearchCancellation,
    ) -> PaletteSearchOutcome<Vec<ScoredResult<'_>>> {
        search_items_cancellable(
            self.ordinals(),
            |_| true,
            |ordinal, fuzzy_query| fuzzy_query.score(&self.files[*ordinal as usize].name),
            |ordinal| SearchResultItem::File(FileOrdinal(ordinal)),
            query,
            max,
            cancellation,
        )
    }

    /// Search while excluding canonical identities, counting the empty
    /// query's rest by walking every file (the arena counts it from its key
    /// table).
    #[must_use]
    pub fn search_cancellable_excluding(
        &self,
        query: &str,
        max: usize,
        excluded_canonical_paths: &HashSet<PathBuf>,
        cancellation: &PaletteSearchCancellation,
    ) -> PaletteSearchOutcome<Vec<ScoredResult<'_>>> {
        let exclusion = ReferenceExclusion::new(excluded_canonical_paths);
        let mut prefiltered =
            |ordinal: &u32| exclusion.admits_prefiltered(&self.files[*ordinal as usize]);
        let rest = if excluded_canonical_paths.is_empty() {
            RestCount::Length
        } else {
            RestCount::WalkWith(&mut prefiltered)
        };
        search_items_cancellable_counting(
            self.ordinals(),
            |ordinal| exclusion.admits(&self.files[*ordinal as usize]),
            |ordinal, fuzzy_query| fuzzy_query.score(&self.files[*ordinal as usize].name),
            |ordinal| SearchResultItem::File(FileOrdinal(ordinal)),
            query,
            CountedSelection {
                max,
                cancellation,
                rest,
            },
        )
    }
}

/// The workspace group's exclusion of files open in tabs, by canonical path
/// (`drill-down-palette-categories` D1).
///
/// [`ReferenceExclusion::admits`] hashes every file's canonical path against
/// the open set; the empty query's count continuation uses
/// [`ReferenceExclusion::admits_prefiltered`], which first looks the canonical
/// path's **length and last eight bytes** up among the open paths' (a binary
/// search over at most the open tabs) and hashes the whole path only on a
/// match. Both admit exactly the same files: an excluded path's key is always
/// among the open keys, and the test reads the canonical path, never the
/// display name, so a symlinked alias cannot slip through. (A first version
/// prefiltered by the canonical file name, `Path::file_name` plus an `OsStr`
/// hash: 5.4–5.6 ms over 131,072 files with 32 or 256 open tabs.)
struct ReferenceExclusion<'a> {
    paths: &'a HashSet<PathBuf>,
    keys: Vec<(usize, u64)>,
}

/// A canonical path's prefilter key: its byte length and its last eight
/// bytes (fewer for a shorter path, zero-padded).
fn exclusion_key(path: &Path) -> (usize, u64) {
    let bytes = path.as_os_str().as_encoded_bytes();
    let tail = &bytes[bytes.len().saturating_sub(8)..];
    let mut word = [0u8; 8];
    word[..tail.len()].copy_from_slice(tail);
    (bytes.len(), u64::from_le_bytes(word))
}

impl<'a> ReferenceExclusion<'a> {
    /// The exclusion of `paths`.
    #[must_use]
    pub fn new(paths: &'a HashSet<PathBuf>) -> Self {
        let mut keys: Vec<(usize, u64)> = paths.iter().map(|path| exclusion_key(path)).collect();
        keys.sort_unstable();
        keys.dedup();
        Self { paths, keys }
    }

    /// Whether `file` stays in the workspace group (full-path test).
    #[must_use]
    pub fn admits(&self, file: &OwnedIndexedFile) -> bool {
        file.identity
            .canonical_path()
            .is_none_or(|path| !self.paths.contains(path))
    }

    /// The same decision as [`Self::admits`], hashing the path only when its
    /// length and last eight bytes are an open tab's.
    #[must_use]
    pub fn admits_prefiltered(&self, file: &OwnedIndexedFile) -> bool {
        let Some(path) = file.identity.canonical_path() else {
            return true;
        };
        self.keys.binary_search(&exclusion_key(path)).is_err() || !self.paths.contains(path)
    }
}

/// Truncate oversized index results after scanning all folders.
///
/// The limit is parameterized so unit tests can exercise the threshold without
/// constructing a six-figure temporary directory tree.
fn truncate_to_index_limit(files: &mut Vec<OwnedIndexedFile>, limit: usize) {
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
fn should_compact_after_removal(before: usize, after: usize) -> bool {
    after < before * 3 / 4
}

impl From<Vec<OwnedIndexedFile>> for ReferenceFileIndex {
    fn from(files: Vec<OwnedIndexedFile>) -> Self {
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

fn indexed_file_graph_weight(file: &OwnedIndexedFile) -> u64 {
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

struct ReferenceTraversal<'a> {
    out: &'a mut Vec<OwnedIndexedFile>,
    visited_directories: &'a mut HashSet<PathBuf>,
    canonical_files: &'a mut HashSet<PathBuf>,
    canonical_folder: &'a Path,
    file_limit: usize,
    directory_limit: usize,
    cancellation: &'a PaletteSearchCancellation,
    metrics: &'a mut FileIndexBuildMetrics,
    ledger: &'a mut ReferenceBuildLedger,
    visited_charge: &'a mut u64,
    canonical_files_charge: &'a mut u64,
    visibility: &'a WorkspaceEntryVisibility,
}

fn collect_files_bounded(
    dir: &Path,
    workspace_folder: &Arc<PathBuf>,
    traversal: &mut ReferenceTraversal<'_>,
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
    traversal: &mut ReferenceTraversal<'_>,
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
            traversal.visibility,
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
    ledger: &mut ReferenceBuildLedger,
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
    ledger: &mut ReferenceBuildLedger,
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

fn indexed_file_from_path(path: PathBuf, workspace_folder: Arc<PathBuf>) -> OwnedIndexedFile {
    let identity = match fs_metadata::canonical_path(&path) {
        Ok(canonical) => PaletteFileIdentity::canonical(canonical),
        Err(error) => {
            PaletteFileIdentity::Unavailable(PaletteFileIdentityFailure::from(error.kind()))
        }
    };
    OwnedIndexedFile::new(path, identity, workspace_folder)
}

fn canonical_identities_match(left: &PaletteFileIdentity, right: &PaletteFileIdentity) -> bool {
    matches!(
        (left.canonical_path(), right.canonical_path()),
        (Some(left), Some(right)) if left == right
    )
}

fn deduplicate_files(files: &mut Vec<OwnedIndexedFile>) {
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
