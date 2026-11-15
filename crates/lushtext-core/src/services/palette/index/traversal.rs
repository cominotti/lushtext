// SPDX-License-Identifier: GPL-3.0-or-later

//! The bounded, cancellable index build (`scale-file-index` design D2, D3).
//!
//! The traversal is the per-file representation's, in the same order: folders
//! in order, each one's directories from a stack, each directory's entries in
//! the scanner's sorted order. What changes is what each entry costs:
//!
//! - A directory becomes one [`DirRecord`] and a file one [`FileRecord`] over
//!   the shared name storage; nothing per entry is a separate allocation.
//! - Canonical identity is derived for every entry that is not a symlink
//!   (its directory's canonical path joined with its name), so `realpath` runs
//!   once per workspace folder and once per symlink entry.
//! - The visit set keys directories by the 64-bit hash of their canonical
//!   path, confirmed by an exact comparison of rebuilt paths. Only a symlinked
//!   file can alias another file, and the first path reached wins in both
//!   orders: a symlink whose target's directory was already scanned searches
//!   that directory's file range, and an ordinary file is looked up in the
//!   set of symlinked files admitted so far.
//!
//! Every arena growth is charged to the installed ledger before it happens,
//! every scratch structure to the build ledger, and the installed vectors are
//! shrunk to their length before the key table is built and the index
//! published.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::model::workspace_visibility::WorkspaceEntryVisibility;
use crate::services::file_tree;
use crate::services::filesystem::metadata as fs_metadata;

use super::arena::{
    DirCanonical, DirDisplay, DirRecord, FileExtra, FileRecord, FolderRecord, IdentityKind, NO_DIR,
    NameSpan, PathHasher, RAW_NAME, failure_of, folder_bytes, path_bytes, retained, split_name,
    vec_bytes,
};
use super::{
    FileIndex, FileIndexBuildMetrics, FileIndexBuildOutcome, FileIndexTruncationReason,
    MAX_SCAN_DEPTH, PaletteSearchCancellation, is_ignored_index_dir,
};

/// The limits one build runs under (scaled down only by tests).
#[derive(Clone, Copy, Debug)]
pub(super) struct BuildLimits {
    pub(super) capacity_hint: usize,
    pub(super) files: usize,
    pub(super) directories: usize,
    pub(super) build_bytes: u64,
    pub(super) installed_bytes: u64,
}

/// Scratch charged per visited directory: its visit-set entry (key, value,
/// control byte and load-factor slack) and its file range.
///
/// Kind: external fact; a `(u64, u32)` hash-map entry at its 7/8 load factor
/// plus a `(u32, u32)` range, rounded up to whole words.
const VISIT_ENTRY_BYTES: u64 = 40;
/// Scratch charged per symlinked file in the alias set, beyond its path.
///
/// Kind: external fact; a `(u64, u32)` hash-map entry plus its path's
/// `PathBuf` header, rounded up to whole words.
const SYMLINK_ENTRY_BYTES: u64 = 32;
/// Bytes of the name storage reserved per hinted file when the previous
/// index's size is known.
///
/// Kind: out: structural; a capacity hint (the mean name of the D9 shape).
const HINTED_NAME_BYTES: usize = 16;

/// O(1) construction accounting: installed bytes (the arena) and scratch.
pub(super) struct BuildLedger {
    current_build_bytes: u64,
    peak_build_bytes: u64,
    installed_bytes: u64,
    build_limit: u64,
    installed_limit: u64,
}

impl BuildLedger {
    pub(super) const fn new(build_limit: u64, installed_limit: u64) -> Self {
        Self {
            current_build_bytes: 0,
            peak_build_bytes: 0,
            installed_bytes: 0,
            build_limit,
            installed_limit,
        }
    }

    pub(super) fn try_charge_installed(
        &mut self,
        bytes: u64,
    ) -> Result<(), FileIndexTruncationReason> {
        let Some(installed) = self.installed_bytes.checked_add(bytes) else {
            return Err(FileIndexTruncationReason::RetainedByteLimit);
        };
        if installed > self.installed_limit {
            return Err(FileIndexTruncationReason::RetainedByteLimit);
        }
        if !self.try_charge_scratch(bytes) {
            return Err(FileIndexTruncationReason::BuildByteLimit);
        }
        self.installed_bytes = installed;
        Ok(())
    }

    pub(super) fn try_charge_scratch(&mut self, bytes: u64) -> bool {
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

    pub(super) fn release_scratch(&mut self, bytes: u64) {
        self.current_build_bytes = self.current_build_bytes.saturating_sub(bytes);
    }

    /// Settle the installed charge on the arena's actual weight, which is at
    /// most what was charged (vectors shrunk to their length).
    pub(super) fn settle_installed(&mut self, actual: u64) {
        debug_assert!(
            actual <= self.installed_bytes,
            "the arena weighs {actual} B, more than the {} B charged",
            self.installed_bytes
        );
        let released = self.installed_bytes.saturating_sub(actual);
        self.installed_bytes = actual;
        self.current_build_bytes = self.current_build_bytes.saturating_sub(released);
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

    const fn remaining_build_bytes(&self) -> u64 {
        self.build_limit.saturating_sub(self.current_build_bytes)
    }

    pub(super) const fn installed_bytes(&self) -> u64 {
        self.installed_bytes
    }

    fn publish(&self, metrics: &mut FileIndexBuildMetrics, retained_index_bytes: u64) {
        metrics.current_build_bytes = retained_index_bytes;
        metrics.peak_build_bytes = self.peak_build_bytes.max(retained_index_bytes);
        metrics.retained_index_bytes = retained_index_bytes;
    }
}

/// Charge the growth of `values` to hold `additional` more elements, doubling,
/// then reserve it.
pub(super) fn grow_doubling<T>(
    values: &mut Vec<T>,
    additional: usize,
    ledger: &mut BuildLedger,
) -> Result<(), FileIndexTruncationReason> {
    let needed = values.len().saturating_add(additional);
    if needed <= values.capacity() {
        return Ok(());
    }
    let next = values.capacity().max(4).saturating_mul(2).max(needed);
    ledger.try_charge_installed(vec_bytes::<T>(next - values.capacity()))?;
    values.reserve_exact(next - values.len());
    Ok(())
}

fn grow_names_doubling(
    names: &mut String,
    additional: usize,
    ledger: &mut BuildLedger,
) -> Result<(), FileIndexTruncationReason> {
    let needed = names.len().saturating_add(additional);
    if needed <= names.capacity() {
        return Ok(());
    }
    let next = names.capacity().max(64).saturating_mul(2).max(needed);
    ledger.try_charge_installed(retained(next - names.capacity()))?;
    names.reserve_exact(next - names.len());
    Ok(())
}

/// The visit set: directories by the hash of their canonical path, with the
/// rare genuine collision kept beside the map.
#[derive(Default)]
pub(super) struct HashedIds {
    map: HashMap<u64, u32>,
    collided: Vec<(u64, u32)>,
}

impl HashedIds {
    /// Every id stored under `hash`.
    pub(super) fn candidates(&self, hash: u64) -> impl Iterator<Item = u32> + '_ {
        self.map.get(&hash).copied().into_iter().chain(
            self.collided
                .iter()
                .filter(move |(stored, _)| *stored == hash)
                .map(|(_, id)| *id),
        )
    }

    pub(super) fn insert(&mut self, hash: u64, id: u32) {
        if let std::collections::hash_map::Entry::Vacant(entry) = self.map.entry(hash) {
            entry.insert(id);
        } else {
            self.collided.push((hash, id));
        }
    }

    pub(super) fn len(&self) -> usize {
        self.map.len() + self.collided.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.map.is_empty() && self.collided.is_empty()
    }
}

struct Traversal<'a> {
    index: FileIndex,
    ledger: BuildLedger,
    metrics: FileIndexBuildMetrics,
    limits: BuildLimits,
    cancellation: &'a PaletteSearchCancellation,
    visibility: &'a WorkspaceEntryVisibility,
    /// Directories visited so far, by canonical-path hash.
    visited: HashedIds,
    /// Each directory's file range, open while it is scanned (`u32::MAX`
    /// start before).
    ranges: Vec<(u32, u32)>,
    /// Symlinked files admitted so far, by canonical-path hash.
    symlinks: HashedIds,
    scratch: u64,
}

/// What ended one folder's traversal.
enum FolderEnd {
    Done,
    Cancelled,
}

pub(super) fn build(
    workspace_folders: &[PathBuf],
    limits: BuildLimits,
    cancellation: &PaletteSearchCancellation,
    visibility: &WorkspaceEntryVisibility,
) -> FileIndexBuildOutcome {
    let mut traversal = Traversal {
        index: FileIndex {
            visibility: visibility.clone(),
            ..FileIndex::default()
        },
        ledger: BuildLedger::new(limits.build_bytes, limits.installed_bytes),
        metrics: FileIndexBuildMetrics::default(),
        limits,
        cancellation,
        visibility,
        visited: HashedIds::default(),
        ranges: Vec::new(),
        symlinks: HashedIds::default(),
        scratch: 0,
    };
    traversal.reserve_from_hint(workspace_folders.len());
    for folder in workspace_folders {
        if cancellation.is_cancelled() {
            return traversal.cancelled();
        }
        if traversal.index.files.len() >= limits.files {
            traversal.metrics.truncation = Some(FileIndexTruncationReason::FileLimit);
            break;
        }
        traversal.metrics.canonical_resolutions += 1;
        let Ok(canonical_folder) = fs_metadata::canonical_path(folder) else {
            continue;
        };
        match traversal.folder(folder, &canonical_folder) {
            Ok(FolderEnd::Done) => {}
            Ok(FolderEnd::Cancelled) => return traversal.cancelled(),
            Err(reason) => {
                traversal.metrics.truncation = Some(reason);
                break;
            }
        }
        if traversal.metrics.truncation.is_some_and(is_byte_limit) {
            break;
        }
    }
    traversal.finish()
}

fn is_byte_limit(reason: FileIndexTruncationReason) -> bool {
    matches!(
        reason,
        FileIndexTruncationReason::RetainedByteLimit | FileIndexTruncationReason::BuildByteLimit
    )
}

impl Traversal<'_> {
    fn reserve_from_hint(&mut self, folders: usize) {
        let maximum_files = usize::try_from(self.limits.installed_bytes)
            .unwrap_or(usize::MAX)
            .checked_div(std::mem::size_of::<FileRecord>() + HINTED_NAME_BYTES)
            .unwrap_or(0);
        let files = self
            .limits
            .capacity_hint
            .min(self.limits.files)
            .min(maximum_files);
        let charge = vec_bytes::<FileRecord>(files)
            .saturating_add(retained(files.saturating_mul(HINTED_NAME_BYTES)))
            .saturating_add(vec_bytes::<FolderRecord>(folders));
        if self.ledger.try_charge_installed(charge).is_ok() {
            self.index.files.reserve_exact(files);
            self.index.names.reserve_exact(files * HINTED_NAME_BYTES);
            self.index.folders.reserve_exact(folders);
        } else {
            let folder_shells = vec_bytes::<FolderRecord>(folders);
            if self.ledger.try_charge_installed(folder_shells).is_ok() {
                self.index.folders.reserve_exact(folders);
            }
        }
    }

    fn charge_scratch(&mut self, bytes: u64) -> Result<(), FileIndexTruncationReason> {
        if self.ledger.try_charge_scratch(bytes) {
            self.scratch = self.scratch.saturating_add(bytes);
            Ok(())
        } else {
            Err(FileIndexTruncationReason::BuildByteLimit)
        }
    }

    fn cancelled(mut self) -> FileIndexBuildOutcome {
        self.metrics.retained_files = self.index.files.len();
        self.ledger.release_scratch(self.scratch);
        self.ledger.publish(&mut self.metrics, 0);
        FileIndexBuildOutcome::Cancelled {
            metrics: self.metrics,
        }
    }

    /// Intern a name: its span in the name storage and, when it is not UTF-8,
    /// its exact bytes.
    fn push_name(
        &mut self,
        name: &OsStr,
    ) -> Result<(NameSpan, Option<Box<OsStr>>), FileIndexTruncationReason> {
        let (text, raw) = split_name(name);
        let raw_charge = raw.as_ref().map_or(0, |raw| retained(raw.len()));
        self.ledger.try_charge_installed(raw_charge)?;
        if let Err(reason) =
            grow_names_doubling(&mut self.index.names, text.len(), &mut self.ledger)
        {
            self.ledger
                .settle_installed(self.ledger.installed_bytes() - raw_charge);
            return Err(reason);
        }
        let start = u32::try_from(self.index.names.len())
            .map_err(|_| FileIndexTruncationReason::RetainedByteLimit)?;
        let len =
            u32::try_from(text.len()).map_err(|_| FileIndexTruncationReason::RetainedByteLimit)?;
        self.index.names.push_str(&text);
        Ok((NameSpan { start, len }, raw))
    }

    /// Push an owned path (a folder or symlink canonical), charged first.
    fn push_owned(&mut self, path: PathBuf) -> Result<u32, FileIndexTruncationReason> {
        let id = u32::try_from(self.index.owned_paths.len())
            .map_err(|_| FileIndexTruncationReason::RetainedByteLimit)?;
        grow_doubling(&mut self.index.owned_paths, 1, &mut self.ledger)?;
        self.ledger.try_charge_installed(path_bytes(&path))?;
        self.index.owned_paths.push(path);
        Ok(id)
    }

    /// Admit one directory record and its visit-set entry.
    fn push_dir(
        &mut self,
        record: DirRecord,
        raw_name: Option<Box<OsStr>>,
        canonical_hash: u64,
    ) -> Result<u32, FileIndexTruncationReason> {
        let id = u32::try_from(self.index.dirs.len())
            .map_err(|_| FileIndexTruncationReason::RetainedByteLimit)?;
        self.charge_scratch(VISIT_ENTRY_BYTES)?;
        grow_doubling(&mut self.index.dirs, 1, &mut self.ledger)?;
        if let Some(raw_name) = raw_name {
            grow_doubling(&mut self.index.dir_raw_names, 1, &mut self.ledger)?;
            self.index.dir_raw_names.push((id, raw_name));
        }
        self.index.dirs.push(record);
        self.ranges.push((u32::MAX, 0));
        self.visited.insert(canonical_hash, id);
        Ok(id)
    }

    /// The visited directory whose canonical path is `canonical`, if any.
    fn visited_dir(&self, hash: u64, canonical: &Path) -> Option<u32> {
        self.visited.candidates(hash).find(|dir| {
            self.index
                .dir_canonical_path(*dir)
                .is_ok_and(|candidate| candidate == canonical)
        })
    }

    fn folder(
        &mut self,
        folder: &Path,
        canonical_folder: &Path,
    ) -> Result<FolderEnd, FileIndexTruncationReason> {
        let display = Arc::new(folder.to_path_buf());
        grow_doubling(&mut self.index.folders, 1, &mut self.ledger)?;
        self.ledger.try_charge_installed(folder_bytes(&display))?;
        let folder_id = u32::try_from(self.index.folders.len())
            .map_err(|_| FileIndexTruncationReason::RetainedByteLimit)?;
        self.index.folders.push(FolderRecord {
            display,
            root: NO_DIR,
        });
        let hash = PathHasher::new(canonical_folder).finish();
        if self.visited_dir(hash, canonical_folder).is_some() {
            return Ok(FolderEnd::Done);
        }
        if self.visited.len() >= self.limits.directories {
            self.metrics
                .truncation
                .get_or_insert(FileIndexTruncationReason::DirectoryRetentionLimit);
            return Ok(FolderEnd::Done);
        }
        let owned = self.push_owned(canonical_folder.to_path_buf())?;
        let root = self.push_dir(
            DirRecord {
                parent: NO_DIR,
                name: NameSpan::default(),
                display: DirDisplay::FolderRoot,
                canonical: DirCanonical::Resolved(owned),
                folder: folder_id,
            },
            None,
            hash,
        )?;
        self.index.folders[folder_id as usize].root = root;
        let pending_charge = vec_bytes::<(u32, u32)>(1);
        self.charge_scratch(pending_charge)?;
        let mut pending: Vec<(u32, u32)> = vec![(root, 0)];
        let mut pending_shell = pending_charge;
        let end = self.drain(
            &mut pending,
            &mut pending_shell,
            canonical_folder,
            folder_id,
        );
        self.ledger.release_scratch(pending_shell);
        self.scratch = self.scratch.saturating_sub(pending_shell);
        end
    }

    /// Scan directories until the stack is empty, a limit stops the build,
    /// or cancellation is observed.
    fn drain(
        &mut self,
        pending: &mut Vec<(u32, u32)>,
        pending_shell: &mut u64,
        canonical_folder: &Path,
        folder_id: u32,
    ) -> Result<FolderEnd, FileIndexTruncationReason> {
        while let Some((dir, depth)) = pending.pop() {
            if self.cancellation.is_cancelled() {
                return Ok(FolderEnd::Cancelled);
            }
            if self.index.files.len() >= self.limits.files {
                self.metrics.truncation = Some(FileIndexTruncationReason::FileLimit);
                return Ok(FolderEnd::Done);
            }
            self.metrics.peak_retained_directories = self
                .metrics
                .peak_retained_directories
                .max(self.visited.len());
            let display = self.index.dir_display_path(dir);
            let remaining = self.limits.files.saturating_sub(self.index.files.len());
            let cancellation = self.cancellation;
            let scan = file_tree::scan_directory_bounded_with_cancel_and_bytes(
                &display,
                remaining,
                0,
                self.ledger.remaining_build_bytes(),
                || cancellation.is_cancelled(),
                self.visibility,
            );
            if !self.ledger.observe_scratch_peak(scan.peak_retained_bytes) {
                return Err(FileIndexTruncationReason::BuildByteLimit);
            }
            self.metrics.scanned_directories = self.metrics.scanned_directories.saturating_add(1);
            self.metrics.examined_directory_entries = self
                .metrics
                .examined_directory_entries
                .saturating_add(scan.examined_entries);
            self.metrics.peak_retained_directory_entries = self
                .metrics
                .peak_retained_directory_entries
                .max(pending.len().saturating_add(scan.peak_retained_entries));
            if scan.cancelled {
                return Ok(FolderEnd::Cancelled);
            }
            if scan.truncated {
                self.metrics
                    .truncation
                    .get_or_insert(FileIndexTruncationReason::DirectoryRetentionLimit);
            }
            if !self.ledger.try_charge_scratch(scan.retained_bytes) {
                return Err(FileIndexTruncationReason::BuildByteLimit);
            }
            let scan_charge = scan.retained_bytes;
            let byte_truncated = scan.byte_truncated;
            let result = self.entries(
                &scan.entries,
                &DirScan {
                    dir,
                    depth,
                    folder_id,
                    canonical_folder,
                },
                pending,
                pending_shell,
            );
            self.ledger.release_scratch(scan_charge);
            match result {
                Ok(None) => {}
                Ok(Some(end)) => return Ok(end),
                Err(reason) => return Err(reason),
            }
            if byte_truncated {
                return Err(FileIndexTruncationReason::BuildByteLimit);
            }
        }
        Ok(FolderEnd::Done)
    }

    /// Admit one scanned directory's entries; `Some` ends the traversal.
    fn entries(
        &mut self,
        entries: &[file_tree::DirectoryEntry],
        scan: &DirScan<'_>,
        pending: &mut Vec<(u32, u32)>,
        pending_shell: &mut u64,
    ) -> Result<Option<FolderEnd>, FileIndexTruncationReason> {
        let canonical = self.index.dir_canonical_path(scan.dir).ok();
        let hasher = canonical.as_deref().map(PathHasher::new);
        let start = u32::try_from(self.index.files.len()).unwrap_or(u32::MAX);
        // The range is open while the directory is scanned, so a symlink in
        // it finds a target admitted earlier in the same directory.
        self.ranges[scan.dir as usize] = (start, start);
        let mut end = None;
        for entry in entries {
            if self.cancellation.is_cancelled() {
                end = Some(FolderEnd::Cancelled);
                break;
            }
            if self.index.files.len() >= self.limits.files {
                self.metrics.truncation = Some(FileIndexTruncationReason::FileLimit);
                end = Some(FolderEnd::Done);
                break;
            }
            let Some(name) = entry.path.file_name() else {
                continue;
            };
            let step = if entry.is_dir {
                self.directory_entry(entry, name, scan, canonical.as_deref(), hasher.as_ref())
                    .map(|child| child.map(|child| (child, scan.depth.saturating_add(1))))
            } else {
                self.file_entry(entry, name, scan.dir, canonical.as_deref(), hasher.as_ref())
                    .map(|()| None)
            };
            match step {
                Ok(Some(child)) => {
                    if pending.len() == pending.capacity() {
                        let next = pending.capacity().max(4).saturating_mul(2);
                        let extra = vec_bytes::<(u32, u32)>(next - pending.capacity());
                        if let Err(reason) = self.charge_scratch(extra) {
                            self.close_range(scan.dir, start);
                            return Err(reason);
                        }
                        *pending_shell = pending_shell.saturating_add(extra);
                        pending.reserve_exact(next - pending.len());
                    }
                    pending.push(child);
                }
                Ok(None) => {}
                Err(reason) => {
                    self.close_range(scan.dir, start);
                    return Err(reason);
                }
            }
        }
        self.close_range(scan.dir, start);
        Ok(end)
    }

    fn close_range(&mut self, dir: u32, start: u32) {
        let end = u32::try_from(self.index.files.len()).unwrap_or(u32::MAX);
        self.ranges[dir as usize] = (start, end);
    }

    /// Visit one directory entry: `Some` is a new directory to scan.
    fn directory_entry(
        &mut self,
        entry: &file_tree::DirectoryEntry,
        name: &OsStr,
        scan: &DirScan<'_>,
        parent_canonical: Option<&Path>,
        parent_hasher: Option<&PathHasher>,
    ) -> Result<Option<u32>, FileIndexTruncationReason> {
        if is_ignored_index_dir(name) {
            return Ok(None);
        }
        if scan.depth.saturating_add(1) > MAX_SCAN_DEPTH {
            tracing::warn!(
                "Skipping deeply nested directory (depth > {MAX_SCAN_DEPTH}): {}",
                entry.path.display()
            );
            return Ok(None);
        }
        let (canonical, hash, resolved) = if entry.is_symlink {
            self.metrics.canonical_resolutions += 1;
            let Ok(canonical) = fs_metadata::canonical_path(&entry.path) else {
                return Ok(None);
            };
            if !canonical.starts_with(scan.canonical_folder) {
                return Ok(None);
            }
            let hash = PathHasher::new(&canonical).finish();
            (canonical, hash, true)
        } else {
            let (Some(parent), Some(hasher)) = (parent_canonical, parent_hasher) else {
                return Ok(None);
            };
            (
                parent.join(name),
                hasher.child(name.as_encoded_bytes()),
                false,
            )
        };
        if self.visited_dir(hash, &canonical).is_some() {
            return Ok(None);
        }
        if self.visited.len() >= self.limits.directories {
            self.metrics
                .truncation
                .get_or_insert(FileIndexTruncationReason::DirectoryRetentionLimit);
            return Ok(None);
        }
        let canonical_rule = if resolved {
            DirCanonical::Resolved(self.push_owned(canonical)?)
        } else {
            DirCanonical::Derived
        };
        let (span, raw) = self.push_name(name)?;
        let id = self.push_dir(
            DirRecord {
                parent: scan.dir,
                name: span,
                display: DirDisplay::Child,
                canonical: canonical_rule,
                folder: scan.folder_id,
            },
            raw,
            hash,
        )?;
        Ok(Some(id))
    }

    /// Admit one file entry unless an alias of it was admitted first.
    fn file_entry(
        &mut self,
        entry: &file_tree::DirectoryEntry,
        name: &OsStr,
        dir: u32,
        dir_canonical: Option<&Path>,
        dir_hasher: Option<&PathHasher>,
    ) -> Result<(), FileIndexTruncationReason> {
        let (kind, explicit, symlink_hash) = if entry.is_symlink {
            self.metrics.canonical_resolutions += 1;
            match fs_metadata::canonical_path(&entry.path) {
                Ok(canonical) => {
                    if self.symlink_target_admitted(&canonical) {
                        return Ok(());
                    }
                    let hash = PathHasher::new(&canonical).finish();
                    (IdentityKind::Explicit, Some(canonical), Some(hash))
                }
                Err(error) => {
                    self.metrics.identity_failures =
                        self.metrics.identity_failures.saturating_add(1);
                    (IdentityKind::Unavailable(failure_of(&error)), None, None)
                }
            }
        } else {
            if !self.symlinks.is_empty()
                && let (Some(dir_canonical), Some(hasher)) = (dir_canonical, dir_hasher)
            {
                let hash = hasher.child(name.as_encoded_bytes());
                let derived = dir_canonical.join(name);
                if self.symlink_admitted(hash, &derived) {
                    return Ok(());
                }
            }
            (IdentityKind::Derived, None, None)
        };
        let ordinal = self.push_file(dir, name, kind, explicit)?;
        self.ranges[dir as usize].1 = ordinal + 1;
        if let Some(hash) = symlink_hash {
            self.charge_scratch(SYMLINK_ENTRY_BYTES)?;
            self.symlinks.insert(hash, ordinal);
        }
        Ok(())
    }

    /// Whether a symlinked file already admitted resolves to `canonical`.
    fn symlink_admitted(&self, hash: u64, canonical: &Path) -> bool {
        self.symlinks.candidates(hash).any(|ordinal| {
            self.index
                .file_extra(ordinal)
                .and_then(|extra| extra.canonical.as_deref())
                .is_some_and(|candidate| candidate == canonical)
        })
    }

    /// Whether the file a symlink resolves to was admitted first: an ordinary
    /// file in an already scanned directory, or another symlink.
    fn symlink_target_admitted(&self, canonical: &Path) -> bool {
        if let (Some(parent), Some(name)) = (canonical.parent(), canonical.file_name()) {
            let parent_hash = PathHasher::new(parent).finish();
            if let Some(dir) = self.visited_dir(parent_hash, parent) {
                let (start, end) = self.ranges[dir as usize];
                if start != u32::MAX
                    && (start..end).any(|ordinal| {
                        self.index.files[ordinal as usize].identity_kind() == IdentityKind::Derived
                            && self.index.file_name(ordinal) == name
                    })
                {
                    return true;
                }
            }
        }
        self.symlink_admitted(PathHasher::new(canonical).finish(), canonical)
    }

    fn push_file(
        &mut self,
        dir: u32,
        name: &OsStr,
        kind: IdentityKind,
        explicit: Option<PathBuf>,
    ) -> Result<u32, FileIndexTruncationReason> {
        let ordinal = u32::try_from(self.index.files.len())
            .map_err(|_| FileIndexTruncationReason::FileLimit)?;
        grow_doubling(&mut self.index.files, 1, &mut self.ledger)?;
        let (span, raw) = self.push_name(name)?;
        let name_len =
            u16::try_from(span.len).map_err(|_| FileIndexTruncationReason::RetainedByteLimit)?;
        let mut flags = kind.bits();
        if explicit.is_some() || raw.is_some() {
            if raw.is_some() {
                flags |= RAW_NAME;
            }
            let canonical = explicit.map(PathBuf::into_boxed_path);
            let canonical_charge = canonical
                .as_ref()
                .map_or(0, |path| retained(path.as_os_str().len()));
            grow_doubling(&mut self.index.file_extras, 1, &mut self.ledger)?;
            self.ledger.try_charge_installed(canonical_charge)?;
            self.index.file_extras.push(FileExtra {
                ordinal,
                canonical,
                raw_name: raw,
            });
        }
        self.index.files.push(FileRecord {
            dir,
            name_start: span.start,
            name_len,
            flags,
        });
        Ok(ordinal)
    }

    fn finish(mut self) -> FileIndexBuildOutcome {
        self.ledger.release_scratch(self.scratch);
        self.scratch = 0;
        drop(std::mem::take(&mut self.visited));
        drop(std::mem::take(&mut self.symlinks));
        drop(std::mem::take(&mut self.ranges));
        if self.index.files.len() > self.limits.files {
            self.index.files.truncate(self.limits.files);
        }
        if self.index.files.len() == self.limits.files && self.metrics.truncation.is_some() {
            self.metrics.truncation = Some(FileIndexTruncationReason::FileLimit);
        }
        self.index.shrink_arena();
        self.ledger
            .settle_installed(self.index.retained_byte_weight());
        if let Err(reason) = self.index.rebuild_keys(&mut self.ledger) {
            // The key table did not fit: publish nothing rather than an index
            // whose lookups would miss files.
            self.metrics.truncation = Some(reason);
            self.index = FileIndex {
                visibility: self.index.visibility.clone(),
                ..FileIndex::default()
            };
            self.ledger
                .settle_installed(self.index.retained_byte_weight());
        }
        self.metrics.retained_files = self.index.files.len();
        let retained_index_bytes = self.ledger.installed_bytes();
        debug_assert_eq!(
            retained_index_bytes,
            self.index.retained_byte_weight(),
            "the installed charge must match the arena's weight"
        );
        self.ledger.publish(&mut self.metrics, retained_index_bytes);
        FileIndexBuildOutcome::Complete {
            index: self.index,
            metrics: self.metrics,
        }
    }
}

/// The directory a scan batch belongs to.
struct DirScan<'a> {
    dir: u32,
    depth: u32,
    folder_id: u32,
    canonical_folder: &'a Path,
}
