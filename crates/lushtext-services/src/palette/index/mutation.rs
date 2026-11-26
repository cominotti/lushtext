// SPDX-License-Identifier: GPL-3.0-or-later

//! Incremental mutations of the arena (`scale-file-index` design D5), the key
//! tables (D4) and the reclamation of storage mutations leave behind.
//!
//! A mutation batch is the [`FileIndexMutationLedger`] a caller takes from
//! [`FileIndex::incremental_mutation_ledger`], the `*_for_bounded_batch` calls
//! it makes, and [`FileIndex::finish_mutation_batch`], which must follow them
//! before the index is read or published. Within a batch:
//!
//! - an add finds its directory by walking the path's components from the
//!   longest indexed root through a `(parent, name) → directory` map built
//!   once per batch, creating the directories it lacks, and refuses a path or
//!   identity already indexed (or added earlier in the batch) after one
//!   binary search per key table;
//! - a removal marks a file, or a directory subtree, dead;
//! - a rename rewrites one file record, or one directory record whose
//!   descendants follow it, and re-keys what it moved;
//!
//! and the finish drops dead records in one pass, merges the keys added in
//! the batch into the sorted tables, and compacts the name storage and the
//! directories when dead bytes exceed a quarter of the weight. A growth the
//! ledger refuses while dead bytes exist compacts first and retries.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::hash::{DefaultHasher, Hasher};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::filesystem::metadata as fs_metadata;
use lushtext_model::palette::PaletteFileIdentityFailure;

use super::arena::{
    DEAD, DirCanonical, DirDisplay, DirRecord, FileExtra, FileRecord, FolderRecord, IdentityKind,
    NO_DIR, NameSpan, PathHasher, RAW_NAME, REKEYED, failure_of, folder_bytes, key_ordinal,
    key_range, path_bytes, path_hash, retained, search_steps, split_name,
};
use super::paths::DirHashers;
use super::traversal::HashedIds;
use super::{FileIndex, MAX_FILE_INDEX_RETAINED_BYTES, MAX_INDEXED_FILES};

mod rename;
mod storage;

/// A vector grows during mutations by `capacity / 16`, or by the batch's
/// need when that is larger, never by doubling a vector shrunk to its length
/// (`scale-file-index` D3: doubling a 2^20-record vector would charge about
/// 30 MiB for one added file).
///
/// Kind: out: factor; a divisor, a power of two.
pub const INDEX_INCREMENTAL_GROWTH_DIVISOR: usize = 16;

/// Installed bytes an added or moved file is charged for its keys: a
/// canonical and a display key, the most a file has.
///
/// Kind: derived; two `u64` keys.
const KEY_CHARGE: u64 = 2 * std::mem::size_of::<u64>() as u64;

/// O(1) installed-graph accounting shared across one incremental mutation
/// batch, and the batch's scratch.
pub struct FileIndexMutationLedger {
    retained_bytes: u64,
    peak_retained_bytes: u64,
    truncated: bool,
    path_comparisons: u64,
    canonical_resolutions: u64,
    batch: Batch,
}

/// Per-batch scratch: the directory map, the dead directories, and the
/// records added or re-keyed, whose keys the finish merges.
#[derive(Default)]
struct Batch {
    children: Option<ChildMap>,
    dead_dirs: Vec<bool>,
    any_dead: bool,
    touched: Vec<u32>,
    pending_canonical: HashedIds,
    pending_display: HashedIds,
    hashers: DirHashers,
    last_dir: Option<(PathBuf, u32)>,
    canonical_cache: Option<(u32, Option<PathBuf>)>,
    /// Records not marked dead, counted once and kept current by the
    /// batch's adds; a kill forgets it. A full index refuses each add in
    /// O(1) instead of counting every record (stage-4 review).
    live_files: Option<usize>,
}

/// Directories by `(parent, name hash)`, and the root directories (folder
/// roots and detached directories), live ones only.
#[derive(Default)]
struct ChildMap {
    children: HashMap<(u32, u64), u32>,
    collided: Vec<((u32, u64), u32)>,
    roots: Vec<u32>,
}

impl ChildMap {
    fn insert(&mut self, parent: u32, name_hash: u64, dir: u32) {
        if let std::collections::hash_map::Entry::Vacant(entry) =
            self.children.entry((parent, name_hash))
        {
            entry.insert(dir);
        } else {
            self.collided.push(((parent, name_hash), dir));
        }
    }

    fn remove(&mut self, parent: u32, name_hash: u64, dir: u32) {
        if self.children.get(&(parent, name_hash)) == Some(&dir) {
            self.children.remove(&(parent, name_hash));
            if let Some(position) = self
                .collided
                .iter()
                .position(|(stored, _)| *stored == (parent, name_hash))
            {
                let (_, moved) = self.collided.swap_remove(position);
                self.children.insert((parent, name_hash), moved);
            }
        } else {
            self.collided
                .retain(|entry| *entry != ((parent, name_hash), dir));
        }
        self.roots.retain(|root| *root != dir);
    }

    fn candidates(&self, parent: u32, name_hash: u64) -> impl Iterator<Item = u32> + '_ {
        self.children
            .get(&(parent, name_hash))
            .copied()
            .into_iter()
            .chain(
                self.collided
                    .iter()
                    .filter(move |(stored, _)| *stored == (parent, name_hash))
                    .map(|(_, dir)| *dir),
            )
    }
}

fn name_hash(name: &OsStr) -> u64 {
    let mut state = DefaultHasher::new();
    state.write(name.as_encoded_bytes());
    super::arena::finish_hash(state.finish())
}

impl FileIndexMutationLedger {
    fn from_index(index: &FileIndex) -> Self {
        let retained_bytes = index.retained_byte_weight();
        Self {
            retained_bytes,
            peak_retained_bytes: retained_bytes,
            truncated: false,
            path_comparisons: 0,
            canonical_resolutions: 0,
            batch: Batch::default(),
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

    const fn headroom(&self) -> u64 {
        MAX_FILE_INDEX_RETAINED_BYTES.saturating_sub(self.retained_bytes)
    }

    fn compare(&mut self, steps: u64) {
        self.path_comparisons = self.path_comparisons.saturating_add(steps);
    }

    #[must_use]
    pub const fn retained_bytes(&self) -> u64 {
        self.retained_bytes
    }

    #[must_use]
    pub const fn peak_retained_bytes(&self) -> u64 {
        self.peak_retained_bytes
    }

    #[cfg(test)]
    pub(crate) const fn truncated(&self) -> bool {
        self.truncated
    }

    /// Binary-search steps and exact path comparisons an add, removal or
    /// rename made to find what it changes (`scale-file-index` D5).
    #[cfg(test)]
    pub(crate) const fn path_comparisons(&self) -> u64 {
        self.path_comparisons
    }

    /// Canonical-path resolutions (`realpath` calls) the batch made.
    #[cfg(test)]
    pub(crate) const fn canonical_resolutions(&self) -> u64 {
        self.canonical_resolutions
    }
}

/// A key table a lookup searches.
#[derive(Clone, Copy)]
enum Table {
    Canonical,
    Display,
}

/// What a growth request grows.
#[derive(Clone, Copy)]
enum Grown {
    Files,
    Dirs,
    Names,
    OwnedPaths,
    DirRawNames,
    FileExtras,
    Folders,
}

impl FileIndex {
    #[must_use]
    pub fn incremental_mutation_ledger(&self) -> FileIndexMutationLedger {
        FileIndexMutationLedger::from_index(self)
    }

    // --- Liveness and lookups ---------------------------------------------

    fn dir_dead(dir: u32, ledger: &FileIndexMutationLedger) -> bool {
        ledger
            .batch
            .dead_dirs
            .get(dir as usize)
            .copied()
            .unwrap_or(false)
    }

    fn file_live(&self, ordinal: u32, ledger: &FileIndexMutationLedger) -> bool {
        let record = self.files[ordinal as usize];
        !record.is_dead() && !Self::dir_dead(record.dir, ledger)
    }

    fn live_len(&self, ledger: &mut FileIndexMutationLedger) -> usize {
        *ledger
            .batch
            .live_files
            .get_or_insert_with(|| self.files.iter().filter(|record| !record.is_dead()).count())
    }

    fn ensure_children(&self, ledger: &mut FileIndexMutationLedger) {
        if ledger.batch.children.is_some() {
            return;
        }
        let mut map = ChildMap::default();
        for (id, record) in self.dirs.iter().enumerate() {
            let id = u32::try_from(id).unwrap_or(u32::MAX);
            if Self::dir_dead(id, ledger) {
                continue;
            }
            match record.display {
                DirDisplay::Child => map.insert(record.parent, name_hash(self.dir_name(id)), id),
                DirDisplay::FolderRoot | DirDisplay::Detached(_) => map.roots.push(id),
            }
        }
        ledger.batch.children = Some(map);
    }

    /// The live directory whose display path is `path`, walking from the
    /// longest indexed root that is a prefix of it.
    /// The live directory whose display path is `path`; with `folder`, only
    /// one of that folder's trees (a moved file or directory stays in its
    /// own folder, as the per-file index kept its folder).
    fn find_dir(
        &self,
        path: &Path,
        folder: Option<u32>,
        ledger: &mut FileIndexMutationLedger,
    ) -> Option<u32> {
        if let Some((cached, dir)) = &ledger.batch.last_dir
            && cached == path
            && !Self::dir_dead(*dir, ledger)
            && folder.is_none_or(|folder| self.dirs[*dir as usize].folder == folder)
        {
            return Some(*dir);
        }
        // Several roots can contain `path` at one depth (a folder's root and
        // another folder's detached root at the same display path), so each
        // is walked, deepest first, until one holds the directory.
        let mut roots = self.roots_containing(path, folder, ledger);
        roots.sort_by_key(|(_, rest)| rest.components().count());
        let dir = roots.into_iter().find_map(|(root, rest)| {
            let mut dir = root;
            for component in rest.components() {
                let Component::Normal(name) = component else {
                    return None;
                };
                dir = self.child(dir, name, ledger)?;
            }
            Some(dir)
        })?;
        ledger.batch.last_dir = Some((path.to_path_buf(), dir));
        Some(dir)
    }

    /// The live roots whose display path is a prefix of `path`, with the rest.
    fn roots_containing<'p>(
        &self,
        path: &'p Path,
        folder: Option<u32>,
        ledger: &mut FileIndexMutationLedger,
    ) -> Vec<(u32, &'p Path)> {
        self.ensure_children(ledger);
        let Some(map) = ledger.batch.children.as_ref() else {
            return Vec::new();
        };
        map.roots
            .iter()
            .filter(|root| folder.is_none_or(|folder| self.dirs[**root as usize].folder == folder))
            .filter(|root| !Self::dir_dead(**root, ledger))
            .filter_map(|root| {
                path.strip_prefix(self.display_origin(*root))
                    .ok()
                    .map(|rest| (*root, rest))
            })
            .collect()
    }

    fn longest_root<'p>(
        &self,
        path: &'p Path,
        folder: Option<u32>,
        ledger: &mut FileIndexMutationLedger,
    ) -> Option<(u32, &'p Path)> {
        self.roots_containing(path, folder, ledger)
            .into_iter()
            .min_by_key(|(_, rest)| rest.components().count())
    }

    fn child(&self, parent: u32, name: &OsStr, ledger: &FileIndexMutationLedger) -> Option<u32> {
        let map = ledger.batch.children.as_ref()?;
        map.candidates(parent, name_hash(name))
            .find(|dir| !Self::dir_dead(*dir, ledger) && self.dir_name(*dir) == name)
    }

    /// The canonical-path hasher of a directory, memoized for the batch.
    fn dir_hasher(&self, dir: u32, ledger: &mut FileIndexMutationLedger) -> Option<PathHasher> {
        ledger.batch.hashers.canonical(self, dir)
    }

    /// Candidates of `hash` in one table: the sorted main table (skipping
    /// records the batch re-keyed) and the batch's pending entries.
    fn table_candidates(
        &self,
        table: Table,
        hash: u64,
        ledger: &mut FileIndexMutationLedger,
    ) -> Vec<u32> {
        let (keys, pending) = match table {
            Table::Canonical => (&self.canonical_keys, &ledger.batch.pending_canonical),
            Table::Display => (&self.display_keys, &ledger.batch.pending_display),
        };
        let mut found: Vec<u32> = key_range(keys, hash)
            .iter()
            .map(|key| key_ordinal(*key))
            .filter(|ordinal| self.files[*ordinal as usize].flags & REKEYED == 0)
            .collect();
        found.extend(pending.candidates(hash));
        let steps = search_steps(keys.len());
        ledger.compare(steps);
        found.retain(|ordinal| self.file_live(*ordinal, ledger));
        found
    }

    /// The live file whose display path is `path`, other than `except`.
    fn find_file(
        &self,
        path: &Path,
        except: Option<u32>,
        ledger: &mut FileIndexMutationLedger,
    ) -> Option<u32> {
        let name = path.file_name()?;
        for ordinal in self.table_candidates(Table::Display, path_hash(path), ledger) {
            ledger.compare(1);
            if Some(ordinal) != except && self.file_display_path(ordinal) == path {
                return Some(ordinal);
            }
        }
        let dir = self.find_dir(path.parent()?, None, ledger)?;
        let hash = self.dir_hasher(dir, ledger)?.child(name.as_encoded_bytes());
        // Another folder's tree can hold a directory at the same display
        // path (a file moved out of a nested folder), so a hit is confirmed
        // by its display path, not its directory.
        self.table_candidates(Table::Canonical, hash, ledger)
            .into_iter()
            .find(|ordinal| {
                let record = self.files[*ordinal as usize];
                Some(*ordinal) != except
                    && record.identity_kind() == IdentityKind::Derived
                    && self.file_name(*ordinal) == name
                    && (record.dir == dir || self.file_display_path(*ordinal) == path)
            })
    }

    /// The live file whose canonical path is `canonical`, other than `except`.
    fn find_canonical(
        &self,
        canonical: &Path,
        except: Option<u32>,
        ledger: &mut FileIndexMutationLedger,
    ) -> Option<u32> {
        let candidates = self.table_candidates(Table::Canonical, path_hash(canonical), ledger);
        candidates.into_iter().find(|ordinal| {
            ledger.compare(1);
            Some(*ordinal) != except
                && self
                    .file_canonical_path(*ordinal)
                    .is_ok_and(|candidate| candidate == canonical)
        })
    }

    // --- Growth ------------------------------------------------------------

    fn grown_capacity(&self, grown: Grown) -> (usize, usize, usize) {
        match grown {
            Grown::Files => (
                self.files.len(),
                self.files.capacity(),
                std::mem::size_of::<FileRecord>(),
            ),
            Grown::Dirs => (
                self.dirs.len(),
                self.dirs.capacity(),
                std::mem::size_of::<DirRecord>(),
            ),
            Grown::Names => (self.names.len(), self.names.capacity(), 1),
            Grown::OwnedPaths => (
                self.owned_paths.len(),
                self.owned_paths.capacity(),
                std::mem::size_of::<PathBuf>(),
            ),
            Grown::DirRawNames => (
                self.dir_raw_names.len(),
                self.dir_raw_names.capacity(),
                std::mem::size_of::<(u32, Box<OsStr>)>(),
            ),
            Grown::FileExtras => (
                self.file_extras.len(),
                self.file_extras.capacity(),
                std::mem::size_of::<FileExtra>(),
            ),
            Grown::Folders => (
                self.folders.len(),
                self.folders.capacity(),
                std::mem::size_of::<FolderRecord>(),
            ),
        }
    }

    fn reserve_exact_for(&mut self, grown: Grown, additional: usize) {
        match grown {
            Grown::Files => self.files.reserve_exact(additional),
            Grown::Dirs => self.dirs.reserve_exact(additional),
            Grown::Names => self.names.reserve_exact(additional),
            Grown::OwnedPaths => self.owned_paths.reserve_exact(additional),
            Grown::DirRawNames => self.dir_raw_names.reserve_exact(additional),
            Grown::FileExtras => self.file_extras.reserve_exact(additional),
            Grown::Folders => self.folders.reserve_exact(additional),
        }
    }

    /// Make room for `additional` more elements, charged first: by
    /// `max(capacity / 16, additional)`; when that is refused, after
    /// compacting dead storage, and then by what the remaining budget allows.
    fn grow(
        &mut self,
        grown: Grown,
        additional: usize,
        ledger: &mut FileIndexMutationLedger,
    ) -> bool {
        let (len, capacity, size) = self.grown_capacity(grown);
        if len.saturating_add(additional) <= capacity {
            return true;
        }
        let need = len.saturating_add(additional) - capacity;
        let step = (capacity / INDEX_INCREMENTAL_GROWTH_DIVISOR)
            .max(need)
            .max(4);
        let charge = |elements: usize| retained(elements.saturating_mul(size));
        let attempt = |index: &mut Self, ledger: &mut FileIndexMutationLedger, elements| {
            if ledger.try_add(charge(elements)) {
                let (len, capacity, _) = index.grown_capacity(grown);
                index.reserve_exact_for(grown, capacity + elements - len);
                true
            } else {
                false
            }
        };
        if attempt(self, ledger, step) {
            return true;
        }
        if self.dead_bytes > 0 {
            self.compact_storage(ledger);
            let (len, capacity, _) = self.grown_capacity(grown);
            if len.saturating_add(additional) <= capacity {
                return true;
            }
            let need = len.saturating_add(additional) - capacity;
            let step = (capacity / INDEX_INCREMENTAL_GROWTH_DIVISOR)
                .max(need)
                .max(4);
            if attempt(self, ledger, step) {
                return true;
            }
        }
        let (len, capacity, _) = self.grown_capacity(grown);
        let need = len.saturating_add(additional).saturating_sub(capacity);
        let affordable =
            usize::try_from(ledger.headroom() / 2 / retained(size).max(1)).unwrap_or(usize::MAX);
        affordable >= need && attempt(self, ledger, affordable.max(need))
    }

    fn charge(&mut self, bytes: u64, ledger: &mut FileIndexMutationLedger) -> bool {
        if ledger.try_add(bytes) {
            return true;
        }
        if self.dead_bytes > 0 {
            self.compact_storage(ledger);
            return ledger.try_add(bytes);
        }
        false
    }

    fn push_name(
        &mut self,
        name: &OsStr,
        ledger: &mut FileIndexMutationLedger,
    ) -> Option<(NameSpan, Option<Box<OsStr>>)> {
        let (text, raw) = split_name(name);
        if let Some(raw) = &raw
            && !self.charge(retained(raw.len()), ledger)
        {
            return None;
        }
        if !self.grow(Grown::Names, text.len(), ledger) {
            return None;
        }
        let start = u32::try_from(self.names.len()).ok()?;
        let len = u32::try_from(text.len()).ok()?;
        self.names.push_str(&text);
        Some((NameSpan { start, len }, raw))
    }

    fn push_owned(&mut self, path: PathBuf, ledger: &mut FileIndexMutationLedger) -> Option<u32> {
        if !self.grow(Grown::OwnedPaths, 1, ledger) || !self.charge(path_bytes(&path), ledger) {
            return None;
        }
        let id = u32::try_from(self.owned_paths.len()).ok()?;
        self.owned_paths.push(path);
        Some(id)
    }

    /// Push a directory record named `name` (none for a root). Every growth
    /// comes before the name is pushed, because a growth may compact the
    /// name storage and would drop a name no record references yet.
    fn push_dir(
        &mut self,
        mut record: DirRecord,
        name: Option<&OsStr>,
        ledger: &mut FileIndexMutationLedger,
    ) -> Option<u32> {
        if !self.grow(Grown::Dirs, 1, ledger) || !self.grow(Grown::DirRawNames, 1, ledger) {
            return None;
        }
        let raw = match name {
            Some(name) => {
                let (span, raw) = self.push_name(name, ledger)?;
                record.name = span;
                raw
            }
            None => None,
        };
        let id = u32::try_from(self.dirs.len()).ok()?;
        if let Some(raw) = raw {
            self.dir_raw_names.push((id, raw));
        }
        self.dirs.push(record);
        if let Some(map) = ledger.batch.children.as_mut() {
            match record.display {
                DirDisplay::Child => map.insert(record.parent, name_hash(self.dir_name(id)), id),
                DirDisplay::FolderRoot | DirDisplay::Detached(_) => map.roots.push(id),
            }
        }
        Some(id)
    }

    /// The folder record of `folder`, created when the index lacks it.
    fn folder_id(
        &mut self,
        folder: &Arc<PathBuf>,
        ledger: &mut FileIndexMutationLedger,
    ) -> Option<u32> {
        if let Some(position) = self
            .folders
            .iter()
            .position(|known| known.display.as_path() == folder.as_path())
        {
            return u32::try_from(position).ok();
        }
        if !self.grow(Grown::Folders, 1, ledger) || !self.charge(folder_bytes(folder), ledger) {
            return None;
        }
        let id = u32::try_from(self.folders.len()).ok()?;
        self.folders.push(FolderRecord {
            display: Arc::clone(folder),
            root: NO_DIR,
        });
        Some(id)
    }

    /// The directory whose display path is `path`, created (with its missing
    /// ancestors) when the index lacks it. A path below no indexed root
    /// becomes a folder root (under `folder`) or a detached directory.
    fn dir_for(
        &mut self,
        path: &Path,
        folder: &Arc<PathBuf>,
        ledger: &mut FileIndexMutationLedger,
    ) -> Option<u32> {
        let folder_filter = self
            .folders
            .iter()
            .position(|known| known.display.as_path() == folder.as_path())
            .and_then(|position| u32::try_from(position).ok())
            .unwrap_or(u32::MAX);
        if let Some(dir) = self.find_dir(path, Some(folder_filter), ledger) {
            return Some(dir);
        }
        let (mut dir, rest) =
            if let Some((root, rest)) = self.longest_root(path, Some(folder_filter), ledger) {
                (root, rest.to_path_buf())
            } else {
                let folder_id = self.folder_id(folder, ledger)?;
                let (display, root_path) = if path.starts_with(folder.as_path()) {
                    (DirDisplay::FolderRoot, folder.as_path().to_path_buf())
                } else {
                    let owned = self.push_owned(path.to_path_buf(), ledger)?;
                    (DirDisplay::Detached(owned), path.to_path_buf())
                };
                let canonical = self.push_owned(root_path.clone(), ledger)?;
                let root = self.push_dir(
                    DirRecord {
                        parent: NO_DIR,
                        name: NameSpan::default(),
                        display,
                        canonical: DirCanonical::Resolved(canonical),
                        folder: folder_id,
                    },
                    None,
                    ledger,
                )?;
                if display == DirDisplay::FolderRoot {
                    self.folders[folder_id as usize].root = root;
                }
                (root, path.strip_prefix(&root_path).ok()?.to_path_buf())
            };
        for component in rest.components() {
            let Component::Normal(name) = component else {
                return None;
            };
            dir = if let Some(child) = self.child(dir, name, ledger) {
                child
            } else {
                let folder_id = self.dirs[dir as usize].folder;
                self.push_dir(
                    DirRecord {
                        parent: dir,
                        name: NameSpan::default(),
                        display: DirDisplay::Child,
                        canonical: DirCanonical::Derived,
                        folder: folder_id,
                    },
                    Some(name),
                    ledger,
                )?
            };
        }
        ledger.batch.last_dir = Some((path.to_path_buf(), dir));
        Some(dir)
    }

    // --- Adds --------------------------------------------------------------

    /// Resolve and add one filesystem path through the metadata boundary.
    ///
    /// Callers must keep this operation on a background worker.
    pub fn add_path(&mut self, path: &Path, workspace_folder: &Arc<PathBuf>) {
        let mut ledger = self.incremental_mutation_ledger();
        self.add_path_for_bounded_batch(path, workspace_folder, &mut ledger);
        self.finish_mutation_batch(&mut ledger);
    }

    pub fn add_path_for_bounded_batch(
        &mut self,
        path: &Path,
        workspace_folder: &Arc<PathBuf>,
        ledger: &mut FileIndexMutationLedger,
    ) -> bool {
        ledger.canonical_resolutions = ledger.canonical_resolutions.saturating_add(1);
        let identity = fs_metadata::canonical_path(path).map_err(|error| failure_of(&error));
        self.add_resolved(path, identity, workspace_folder, ledger)
    }

    /// Add one file whose identity is already known, unless its display path
    /// or canonical path is indexed (or was added earlier in the batch).
    pub(super) fn add_resolved(
        &mut self,
        path: &Path,
        identity: Result<PathBuf, PaletteFileIdentityFailure>,
        folder: &Arc<PathBuf>,
        ledger: &mut FileIndexMutationLedger,
    ) -> bool {
        let (Some(name), Some(parent)) = (path.file_name(), path.parent()) else {
            return false;
        };
        if self.files.len() >= MAX_INDEXED_FILES && self.live_len(ledger) >= MAX_INDEXED_FILES {
            return false;
        }
        if self.find_file(path, None, ledger).is_some() {
            return false;
        }
        if let Ok(canonical) = &identity
            && self.find_canonical(canonical, None, ledger).is_some()
        {
            return false;
        }
        let Some(dir) = self.dir_for(parent, folder, ledger) else {
            return false;
        };
        let Some(ordinal) = self.push_file(dir, name, identity, ledger) else {
            return false;
        };
        if let Some(live) = &mut ledger.batch.live_files {
            *live += 1;
        }
        self.touch(ordinal, ledger);
        true
    }

    /// The identity rule a file record gets in `dir` (design D2): derived
    /// when its canonical path is its directory's joined with its name.
    fn identity_rule(
        &self,
        dir: u32,
        name: &OsStr,
        identity: Result<PathBuf, PaletteFileIdentityFailure>,
        ledger: &mut FileIndexMutationLedger,
    ) -> (IdentityKind, Option<PathBuf>) {
        match identity {
            Ok(canonical) => {
                let derived = canonical.file_name() == Some(name)
                    && canonical.parent().is_some_and(|parent| {
                        if !matches!(&ledger.batch.canonical_cache, Some((cached, _)) if *cached == dir)
                        {
                            ledger.batch.canonical_cache =
                                Some((dir, self.dir_canonical_path(dir).ok()));
                        }
                        matches!(
                            &ledger.batch.canonical_cache,
                            Some((_, Some(dir_canonical))) if dir_canonical == parent
                        )
                    });
                if derived {
                    (IdentityKind::Derived, None)
                } else {
                    (IdentityKind::Explicit, Some(canonical))
                }
            }
            Err(failure) => (IdentityKind::Unavailable(failure), None),
        }
    }

    fn push_file(
        &mut self,
        dir: u32,
        name: &OsStr,
        identity: Result<PathBuf, PaletteFileIdentityFailure>,
        ledger: &mut FileIndexMutationLedger,
    ) -> Option<u32> {
        let (kind, explicit) = self.identity_rule(dir, name, identity, ledger);
        let ordinal = u32::try_from(self.files.len()).ok()?;
        let canonical = explicit.map(PathBuf::into_boxed_path);
        let extra_charge = canonical
            .as_ref()
            .map_or(0, |path| retained(path.as_os_str().len()));
        // Every growth before the name (see `push_dir`); the key charge
        // covers a canonical and a display key.
        if !self.grow(Grown::Files, 1, ledger)
            || !self.grow(Grown::FileExtras, 1, ledger)
            || !self.charge(KEY_CHARGE.saturating_add(extra_charge), ledger)
        {
            return None;
        }
        let (span, raw) = self.push_name(name, ledger)?;
        let name_len = u16::try_from(span.len).ok()?;
        let mut flags = kind.bits();
        if canonical.is_some() || raw.is_some() {
            if raw.is_some() {
                flags |= RAW_NAME;
            }
            self.file_extras.push(FileExtra {
                ordinal,
                canonical,
                raw_name: raw,
            });
        }
        self.files.push(FileRecord {
            dir,
            name_start: span.start,
            name_len,
            flags,
        });
        Some(ordinal)
    }

    /// Record a file's current keys for the batch's lookups and the finish.
    fn touch(&self, ordinal: u32, ledger: &mut FileIndexMutationLedger) {
        ledger.batch.touched.push(ordinal);
        let (canonical, display) = self.file_keys(ordinal, &mut ledger.batch.hashers);
        if let Some(hash) = canonical {
            ledger.batch.pending_canonical.insert(hash, ordinal);
        }
        if let Some(hash) = display {
            ledger.batch.pending_display.insert(hash, ordinal);
        }
    }

    // --- Removals ----------------------------------------------------------

    /// Remove a file (or all files under a directory) from the index.
    pub fn remove_path(&mut self, path: &Path) {
        let mut ledger = self.incremental_mutation_ledger();
        self.remove_path_for_bounded_batch(path, &mut ledger);
        self.finish_mutation_batch(&mut ledger);
    }

    pub fn remove_path_for_bounded_batch(
        &mut self,
        path: &Path,
        ledger: &mut FileIndexMutationLedger,
    ) {
        if let Some(ordinal) = self.find_file(path, None, ledger) {
            self.kill_file(ordinal, ledger);
            return;
        }
        for dir in self.dirs_at_or_below(path, ledger) {
            self.kill_subtree(dir, ledger);
        }
    }

    fn kill_file(&mut self, ordinal: u32, ledger: &mut FileIndexMutationLedger) {
        let record = &mut self.files[ordinal as usize];
        record.flags |= DEAD;
        self.dead_bytes = self.dead_bytes.saturating_add(u64::from(record.name_len));
        ledger.batch.any_dead = true;
        ledger.batch.live_files = None;
    }

    /// The directories whose display path is `path` and the roots below it.
    fn dirs_at_or_below(&self, path: &Path, ledger: &mut FileIndexMutationLedger) -> Vec<u32> {
        // Every folder's tree may hold a directory at `path` (a directory
        // moved out of a nested folder keeps its folder), so each folder
        // holding a root is searched.
        let mut found: Vec<u32> = Vec::new();
        for folder in 0..self.folders.len() {
            let folder = u32::try_from(folder).unwrap_or(u32::MAX);
            found.extend(self.find_dir(path, Some(folder), ledger));
        }
        self.ensure_children(ledger);
        if let Some(map) = &ledger.batch.children {
            found.extend(
                map.roots
                    .iter()
                    .copied()
                    .filter(|root| self.display_origin(*root).starts_with(path)),
            );
        }
        found.sort_unstable();
        found.dedup();
        found
    }

    /// Every live directory in `dir`'s subtree, `dir` included.
    fn subtree(&self, dir: u32, ledger: &FileIndexMutationLedger) -> Vec<bool> {
        let mut state = vec![0u8; self.dirs.len()];
        state[dir as usize] = 2;
        for start in 0..self.dirs.len() {
            if state[start] != 0 {
                continue;
            }
            let mut chain = Vec::new();
            let mut current = start;
            let below = loop {
                match state[current] {
                    2 => break true,
                    1 => break false,
                    _ => {}
                }
                chain.push(current);
                let parent = self.dirs[current].parent;
                if parent == NO_DIR || self.dirs[current].display != DirDisplay::Child {
                    break false;
                }
                current = parent as usize;
            };
            for id in chain {
                state[id] = if below { 2 } else { 1 };
            }
        }
        state
            .iter()
            .enumerate()
            .map(|(id, state)| {
                *state == 2 && !Self::dir_dead(u32::try_from(id).unwrap_or(u32::MAX), ledger)
            })
            .collect()
    }

    fn kill_subtree(&mut self, dir: u32, ledger: &mut FileIndexMutationLedger) {
        let subtree = self.subtree(dir, ledger);
        ledger.batch.dead_dirs.resize(self.dirs.len(), false);
        for (id, below) in subtree.iter().enumerate() {
            if *below {
                ledger.batch.dead_dirs[id] = true;
                let id = u32::try_from(id).unwrap_or(u32::MAX);
                self.dead_bytes = self.dead_bytes.saturating_add(
                    retained(std::mem::size_of::<DirRecord>())
                        .saturating_add(u64::from(self.dirs[id as usize].name.len)),
                );
                if let Some(map) = ledger.batch.children.as_mut() {
                    let record = self.dirs[id as usize];
                    map.remove(record.parent, name_hash(self.dir_name(id)), id);
                }
            }
        }
        ledger.batch.any_dead = true;
        ledger.batch.last_dir = None;
    }
}
