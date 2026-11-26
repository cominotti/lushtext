// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace file indexing for the command palette.
//!
//! The index is an interned path arena (`scale-file-index` design D1): each
//! directory once, each file a 12-byte record over shared name storage, with
//! display path, relative display and canonical identity rebuilt on demand.
//! It owns the bounded traversal ([`traversal`]), the incremental mutations
//! ([`mutation`]), and the queries over it; it remains GTK-free and returns
//! only domain types.
//!
//! - `arena`: the records, the names, the keys and the path hasher.
//! - `paths`: rebuilding and looking up paths in the arena.
//! - `traversal`: the bounded, cancellable build (design D2, D3).
//! - `mutation`: adds, removes, renames and compaction (design D5).
//! - `owned` (test-utils): constructors from owned path triples.
//! - `reference` (test-utils): the per-file representation this replaced,
//!   kept as the parity reference (design D6).

mod arena;
mod mutation;
#[cfg(any(test, feature = "test-utils"))]
mod owned;
mod paths;
#[cfg(any(test, feature = "test-utils"))]
pub mod reference;
#[cfg(test)]
mod tests;
mod traversal;

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::single_flight::{SingleFlightCoordinator, SingleFlightSnapshot, SingleFlightStart};
use lushtext_model::palette::{FileOrdinal, PaletteFileIdentity, ScoredResult, SearchResultItem};
use lushtext_model::workspace_visibility::WorkspaceEntryVisibility;

use self::arena::{
    DirRecord, FileExtra, FileRecord, FolderRecord, NO_DIR, folder_bytes, path_bytes, retained,
    vec_bytes,
};
use super::fuzzy::{
    CountedSelection, RestCount, SearchProgressPolicy, search_items, search_items_cancellable,
    search_items_cancellable_counting, search_items_cancellable_with_progress,
    search_items_full_sort_reference,
};
use super::runtime::{PaletteSearchCancellation, PaletteSearchOutcome};

pub use self::mutation::FileIndexMutationLedger;
pub use self::mutation::INDEX_INCREMENTAL_GROWTH_DIVISOR;
#[cfg(any(test, feature = "test-utils"))]
pub use self::owned::OwnedIndexedFile;

/// Maximum recursion depth to prevent runaway scanning in deeply nested trees.
///
/// Kind: presentation cap; a power of two, never rounded down (`power-of-two-thresholds` D12).
const MAX_SCAN_DEPTH: u32 = 64;
/// Maximum number of files to index: the query-latency bound. A query scores
/// every indexed name once, so its time grows with this count; the 64 MiB
/// installed and 128 MiB construction ledgers are the memory bound, and they
/// stop a tree of unusually long names earlier (`scale-file-index` D3).
///
/// Kind: presentation cap; 1,000,000 rounded up to a power of two
/// (`power-of-two-thresholds` D12; its query bound measured in
/// `scale-file-index` task 4.2).
pub const MAX_INDEXED_FILES: usize = 1024 * 1024;
/// Maximum number of distinct canonical directories retained by one index build.
///
/// This is independent from the file limit because sparse directory forests can
/// otherwise consume unbounded traversal state while admitting almost no files.
///
/// Kind: presentation cap; the file count's power of two (`scale-file-index`
/// D3). The 64 MiB installed and 128 MiB construction ledgers (unchanged)
/// bound its memory.
pub const MAX_INDEXED_DIRECTORIES: usize = 1024 * 1024;
/// Maximum heap ownership retained by one installed file index.
///
/// Kind: presentation cap; a power of two, never rounded down (`power-of-two-thresholds` D12).
pub const MAX_FILE_INDEX_RETAINED_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum installed output plus traversal/deduplication ownership during build.
///
/// Kind: presentation cap; a power of two, never rounded down (`power-of-two-thresholds` D12).
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
    /// Symlink resolutions that failed (ordinary entries derive their identity).
    pub identity_failures: usize,
    /// Canonical-path resolutions (`realpath` calls) the build made: one per
    /// workspace folder and one per symlink entry (`scale-file-index` D2).
    pub canonical_resolutions: usize,
    /// Current conservatively charged construction bytes at terminal publication.
    pub current_build_bytes: u64,
    /// Peak conservatively charged construction bytes across output and scratch.
    pub peak_build_bytes: u64,
    /// Complete installed index graph retained by the terminal result.
    pub retained_index_bytes: u64,
    pub truncation: Option<FileIndexTruncationReason>,
}

/// Typed terminal result from cancellable file-index construction.
#[derive(Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "moved once per build, by value to its one consumer; boxing the arena header buys nothing"
)]
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
    /// Visibility rule captured when the request was built.
    pub visibility: WorkspaceEntryVisibility,
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
pub type FileIndexBuildCoordinator = SingleFlightCoordinator<FileIndexBuildRequest>;

/// In-memory index of all files across workspace folders, as an interned path
/// arena (see the module documentation).
#[derive(Debug, Default, Clone)]
pub struct FileIndex {
    folders: Vec<FolderRecord>,
    dirs: Vec<DirRecord>,
    files: Vec<FileRecord>,
    /// Every directory and file name, lossy UTF-8.
    names: String,
    /// Folder canonicals, symlinked or renamed directories' canonicals, and
    /// detached directories' display paths.
    owned_paths: Vec<PathBuf>,
    /// Exact bytes of non-UTF-8 directory names, sorted by directory id.
    dir_raw_names: Vec<(u32, Box<OsStr>)>,
    /// Symlinked files' canonical paths and non-UTF-8 file names, by ordinal.
    file_extras: Vec<FileExtra>,
    /// `(hash << 32) | ordinal` of every canonical path, sorted.
    canonical_keys: Vec<u64>,
    /// The same, by display path, for files whose canonical path is not
    /// derived (symlinks and failures), sorted.
    display_keys: Vec<u64>,
    /// Name bytes and directory records no live file references any more,
    /// which compaction reclaims (design D5).
    dead_bytes: u64,
    /// Visibility rule this index was built under; worker-only rebuilds reuse it.
    visibility: WorkspaceEntryVisibility,
}

/// One indexed file, borrowed from its [`FileIndex`]: the name is a slice of
/// the index's name storage, and every path is rebuilt when asked for.
#[derive(Clone, Copy)]
pub struct IndexedFile<'a> {
    index: &'a FileIndex,
    ordinal: u32,
}

impl std::fmt::Debug for IndexedFile<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("IndexedFile")
            .field("ordinal", &self.ordinal)
            .field("path", &self.path())
            .finish()
    }
}

impl<'a> IndexedFile<'a> {
    /// Position in the index's source order.
    #[must_use]
    pub const fn ordinal(&self) -> FileOrdinal {
        FileOrdinal(self.ordinal)
    }

    /// File name (lossy UTF-8): what fuzzy matching scores.
    #[must_use]
    pub fn name(&self) -> &'a str {
        self.index.file_name_str(self.ordinal)
    }

    /// Absolute path the file is opened at (exact bytes).
    #[must_use]
    pub fn path(&self) -> PathBuf {
        self.index.file_display_path(self.ordinal)
    }

    /// Path relative to the workspace folder, for display purposes; the full
    /// path when the file is not below its folder.
    #[must_use]
    pub fn relative_display(&self) -> String {
        let path = self.path();
        path.strip_prefix(self.workspace_folder().as_path())
            .map_or_else(|_| path.display().to_string(), |p| p.display().to_string())
    }

    /// Canonical identity captured by the build or the mutation that added it.
    #[must_use]
    pub fn identity(&self) -> PaletteFileIdentity {
        self.index.file_identity(self.ordinal)
    }

    /// The workspace folder whose traversal or add reached this file.
    #[must_use]
    pub fn workspace_folder(&self) -> &'a Arc<PathBuf> {
        self.index.file_folder(self.ordinal)
    }

    /// An owned copy of this file's fields.
    #[cfg(any(test, feature = "test-utils"))]
    #[must_use]
    pub fn to_owned_file(&self) -> OwnedIndexedFile {
        OwnedIndexedFile {
            path: self.path(),
            identity: self.identity(),
            name: self.name().to_string(),
            workspace_folder: Arc::clone(self.workspace_folder()),
        }
    }
}

impl FileIndex {
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
        let FileIndexBuildOutcome::Complete { index, .. } = Self::rebuild_cancellable_with_hint(
            workspace_folders,
            10_000,
            &cancellation,
            visibility,
        ) else {
            unreachable!("a fresh index cancellation token cannot cancel");
        };
        index
    }

    /// Re-scan the folder identities already owned by this installed index.
    ///
    /// Incremental queue overflow uses this worker-only path so a compact scalar
    /// rebuild request can replace an otherwise unbounded mutation backlog.
    #[must_use]
    pub fn rebuild_current_workspace_folders(&self) -> Self {
        let folders = self
            .folders
            .iter()
            .map(|folder| folder.display.as_ref().clone())
            .collect::<Vec<_>>();
        Self::rebuild_with_visibility(&folders, &self.visibility)
    }

    /// Like [`Self::rebuild`], but uses `capacity_hint` for the initial allocation.
    #[must_use]
    pub fn rebuild_with_hint(workspace_folders: &[PathBuf], capacity_hint: usize) -> Self {
        let cancellation = PaletteSearchCancellation::default();
        let FileIndexBuildOutcome::Complete { index, .. } = Self::rebuild_cancellable_with_hint(
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
    ) -> FileIndexBuildOutcome {
        traversal::build(
            workspace_folders,
            traversal::BuildLimits {
                capacity_hint,
                files: MAX_INDEXED_FILES,
                directories: MAX_INDEXED_DIRECTORIES,
                build_bytes: MAX_FILE_INDEX_BUILD_RETAINED_BYTES,
                installed_bytes: MAX_FILE_INDEX_RETAINED_BYTES,
            },
            cancellation,
            visibility,
        )
    }

    /// Build under explicit (scaled) limits: the arena's half of the parity
    /// API the per-file reference mirrors (`reference::ReferenceFileIndex::
    /// rebuild_cancellable_with_limits`), for tests of truncation at small
    /// sizes. A fixture constructor, not a workflow seam.
    #[cfg(any(test, feature = "test-utils"))]
    #[doc(hidden)]
    #[must_use]
    pub fn rebuild_cancellable_with_limits(
        workspace_folders: &[PathBuf],
        file_limit: usize,
        directory_limit: usize,
        build_byte_limit: u64,
        installed_byte_limit: u64,
        cancellation: &PaletteSearchCancellation,
    ) -> FileIndexBuildOutcome {
        traversal::build(
            workspace_folders,
            traversal::BuildLimits {
                capacity_hint: 0,
                files: file_limit,
                directories: directory_limit,
                build_bytes: build_byte_limit,
                installed_bytes: installed_byte_limit,
            },
            cancellation,
            &WorkspaceEntryVisibility::default(),
        )
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
            MAX_INDEXED_DIRECTORIES,
            MAX_FILE_INDEX_BUILD_RETAINED_BYTES,
            MAX_FILE_INDEX_RETAINED_BYTES,
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
            directory_limit,
            MAX_FILE_INDEX_BUILD_RETAINED_BYTES,
            MAX_FILE_INDEX_RETAINED_BYTES,
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
            file_limit,
            directory_limit,
            build_byte_limit,
            MAX_FILE_INDEX_RETAINED_BYTES,
            cancellation,
        )
    }

    /// Every indexed file in source order, as borrowed views.
    #[must_use]
    pub fn files(&self) -> impl ExactSizeIterator<Item = IndexedFile<'_>> + '_ {
        (0..self.ordinal_count()).map(|ordinal| IndexedFile {
            index: self,
            ordinal,
        })
    }

    /// The file at `ordinal`, when there is one.
    #[must_use]
    pub fn file(&self, ordinal: FileOrdinal) -> Option<IndexedFile<'_>> {
        (usize::try_from(ordinal.0).ok()? < self.files.len()).then_some(IndexedFile {
            index: self,
            ordinal: ordinal.0,
        })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    fn ordinal_count(&self) -> u32 {
        u32::try_from(self.files.len()).unwrap_or(u32::MAX)
    }

    /// Return every heap allocation retained by this index owner: each vector's
    /// capacity, the name storage's capacity, every owned path and boxed extra,
    /// and the shared folder paths. O(folders + extras + owned paths), not
    /// O(files).
    #[must_use]
    pub fn retained_byte_weight(&self) -> u64 {
        let folder_graph = self.folders.iter().fold(0u64, |total, folder| {
            total.saturating_add(folder_bytes(&folder.display))
        });
        let owned_graph = self
            .owned_paths
            .iter()
            .fold(0u64, |total, path| total.saturating_add(path_bytes(path)));
        let dir_raw_graph = self.dir_raw_names.iter().fold(0u64, |total, (_, name)| {
            total.saturating_add(retained(name.len()))
        });
        let extras_graph = self.file_extras.iter().fold(0u64, |total, extra| {
            total
                .saturating_add(
                    extra
                        .canonical
                        .as_ref()
                        .map_or(0, |path| retained(path.as_os_str().len())),
                )
                .saturating_add(
                    extra
                        .raw_name
                        .as_ref()
                        .map_or(0, |name| retained(name.len())),
                )
        });
        vec_bytes::<FolderRecord>(self.folders.capacity())
            .saturating_add(folder_graph)
            .saturating_add(vec_bytes::<DirRecord>(self.dirs.capacity()))
            .saturating_add(vec_bytes::<FileRecord>(self.files.capacity()))
            .saturating_add(retained(self.names.capacity()))
            .saturating_add(vec_bytes::<PathBuf>(self.owned_paths.capacity()))
            .saturating_add(owned_graph)
            .saturating_add(vec_bytes::<(u32, Box<OsStr>)>(
                self.dir_raw_names.capacity(),
            ))
            .saturating_add(dir_raw_graph)
            .saturating_add(vec_bytes::<FileExtra>(self.file_extras.capacity()))
            .saturating_add(extras_graph)
            .saturating_add(vec_bytes::<u64>(self.canonical_keys.capacity()))
            .saturating_add(vec_bytes::<u64>(self.display_keys.capacity()))
    }

    /// Find the workspace folder that contains the given path: one whose root
    /// directory the index still holds (a folder removed or renamed away is
    /// not addressable).
    #[must_use]
    pub fn workspace_folder_for(&self, path: &Path) -> Option<Arc<PathBuf>> {
        self.folders
            .iter()
            .find(|folder| folder.root != NO_DIR && path.starts_with(folder.display.as_path()))
            .map(|folder| Arc::clone(&folder.display))
    }

    fn scored_files(&self) -> impl ExactSizeIterator<Item = u32> + '_ {
        0..self.ordinal_count()
    }

    /// Search the file index with a fuzzy query, returning up to `max` scored results.
    pub fn search(&self, query: &str, max: usize) -> Vec<ScoredResult<'_>> {
        search_items(
            self.scored_files(),
            |ordinal| self.file_name_str(*ordinal),
            wrap_file,
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
            self.scored_files(),
            |_| true,
            |ordinal, fuzzy_query| fuzzy_query.score(self.file_name_str(*ordinal)),
            wrap_file,
            query,
            max,
            cancellation,
        )
    }

    /// Search while excluding canonical identities before bounded retention.
    ///
    /// The open paths are looked up in the index once (design D4), so the
    /// exclusion is a bit test per file and the empty query's rest is counted
    /// exactly from the index's length, without a walk.
    pub fn search_cancellable_excluding(
        &self,
        query: &str,
        max: usize,
        excluded_canonical_paths: &HashSet<PathBuf>,
        cancellation: &PaletteSearchCancellation,
    ) -> PaletteSearchOutcome<Vec<ScoredResult<'_>>> {
        let exclusion = OpenTabExclusion::new(self, excluded_canonical_paths);
        let admitted = self.files.len().saturating_sub(exclusion.excluded());
        search_items_cancellable_counting(
            self.scored_files(),
            |ordinal| exclusion.admits(FileOrdinal(*ordinal)),
            |ordinal, fuzzy_query| fuzzy_query.score(self.file_name_str(*ordinal)),
            wrap_file,
            query,
            CountedSelection {
                max,
                cancellation,
                rest: RestCount::Known { admitted },
            },
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
            self.scored_files(),
            |_| true,
            |ordinal, fuzzy_query| fuzzy_query.score(self.file_name_str(*ordinal)),
            wrap_file,
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
            self.scored_files(),
            |_| true,
            |ordinal, fuzzy_query| fuzzy_query.score(self.file_name_str(*ordinal)),
            wrap_file,
            query,
            max,
        )
    }
}

fn wrap_file<'a>(ordinal: u32) -> SearchResultItem<'a> {
    SearchResultItem::File(FileOrdinal(ordinal))
}

/// The workspace group's exclusion of files open in tabs, by canonical path
/// (`drill-down-palette-categories` D1, `scale-file-index` D4).
///
/// Each open canonical path is looked up once in the index's canonical-key
/// table (a binary search confirmed by an exact comparison), so the exclusion
/// knows the ordinals it removes: [`Self::admits`] is a bit test, and
/// [`Self::excluded`] makes the empty query's count exact without reading
/// any file's canonical path. A symlinked alias is excluded by its resolved
/// identity, never by its display name.
pub struct OpenTabExclusion {
    /// Excluded ordinals, sorted and distinct.
    ordinals: Vec<u32>,
    /// One bit per indexed file, set for an excluded ordinal (empty when none).
    mask: Vec<u64>,
}

impl OpenTabExclusion {
    /// The exclusion of `paths` from `index`.
    #[must_use]
    pub fn new(index: &FileIndex, paths: &HashSet<PathBuf>) -> Self {
        let mut ordinals: Vec<u32> = paths
            .iter()
            .flat_map(|path| index.ordinals_with_canonical(path))
            .collect();
        ordinals.sort_unstable();
        ordinals.dedup();
        let mut mask = Vec::new();
        if !ordinals.is_empty() {
            mask = vec![0u64; index.len().div_ceil(64)];
            for ordinal in &ordinals {
                let ordinal = *ordinal as usize;
                mask[ordinal / 64] |= 1 << (ordinal % 64);
            }
        }
        Self { ordinals, mask }
    }

    /// Whether the file at `ordinal` stays in the workspace group.
    #[must_use]
    pub fn admits(&self, ordinal: FileOrdinal) -> bool {
        let ordinal = ordinal.0 as usize;
        self.mask
            .get(ordinal / 64)
            .is_none_or(|word| word & (1 << (ordinal % 64)) == 0)
    }

    /// How many indexed files the open tabs exclude.
    #[must_use]
    pub fn excluded(&self) -> usize {
        self.ordinals.len()
    }
}

fn is_ignored_index_dir(name: &OsStr) -> bool {
    name.to_str()
        .is_some_and(|name| IGNORED_INDEX_DIRS.contains(&name))
}
