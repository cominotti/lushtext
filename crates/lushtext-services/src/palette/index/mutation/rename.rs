// SPDX-License-Identifier: GPL-3.0-or-later

//! Renames inside a mutation batch (`scale-file-index` design D5): a file
//! rename rewrites one record; a directory rename rewrites one directory
//! record, its descendants follow it, and what moved is re-keyed.

use std::path::{Path, PathBuf};

use crate::filesystem::metadata as fs_metadata;
use lushtext_model::workspace::palette::PaletteFileIdentityFailure;

use super::super::FileIndex;
use super::super::arena::{
    DEAD, DirCanonical, DirDisplay, FileExtra, FileRecord, NO_DIR, NameSpan, RAW_NAME, REKEYED,
    failure_of, retained,
};
use super::{FileIndexMutationLedger, Grown, KEY_CHARGE, Table, name_hash};

impl FileIndex {
    // --- Renames -----------------------------------------------------------

    /// Rename a file or directory in the index.
    pub fn rename_path(&mut self, old_path: &Path, new_path: &Path) {
        let mut ledger = self.incremental_mutation_ledger();
        self.rename_path_for_bounded_batch(old_path, new_path, &mut ledger);
        self.finish_mutation_batch(&mut ledger);
    }

    pub fn rename_path_for_bounded_batch(
        &mut self,
        old_path: &Path,
        new_path: &Path,
        ledger: &mut FileIndexMutationLedger,
    ) {
        if let Some(ordinal) = self.find_file(old_path, None, ledger) {
            self.rename_file(ordinal, new_path, ledger);
            return;
        }
        let dirs = self.dirs_at_or_below(old_path, ledger);
        for dir in dirs {
            let display = self.dir_display_path(dir);
            let Ok(suffix) = display.strip_prefix(old_path) else {
                continue;
            };
            let target = if suffix.as_os_str().is_empty() {
                new_path.to_path_buf()
            } else {
                new_path.join(suffix)
            };
            self.rename_dir(dir, &target, ledger);
        }
    }

    pub(super) fn rename_file(
        &mut self,
        ordinal: u32,
        new_path: &Path,
        ledger: &mut FileIndexMutationLedger,
    ) {
        ledger.canonical_resolutions = ledger.canonical_resolutions.saturating_add(1);
        let identity = fs_metadata::canonical_path(new_path).map_err(|error| failure_of(&error));
        let (Some(name), Some(parent)) = (new_path.file_name(), new_path.parent()) else {
            self.kill_file(ordinal, ledger);
            return;
        };
        let folder = self.file_folder(ordinal).clone();
        let Some(dir) = self.dir_for(parent, &folder, ledger) else {
            self.kill_file(ordinal, ledger);
            return;
        };
        let (kind, explicit) = self.identity_rule(dir, name, identity, ledger);
        let canonical = explicit.map(PathBuf::into_boxed_path);
        let extra_charge = canonical
            .as_ref()
            .map_or(0, |path| retained(path.as_os_str().len()));
        // Every growth before the name (see `push_dir`).
        if !self.grow(Grown::FileExtras, 1, ledger)
            || !self.charge(KEY_CHARGE.saturating_add(extra_charge), ledger)
        {
            self.kill_file(ordinal, ledger);
            return;
        }
        let Some((span, raw)) = self.push_name(name, ledger) else {
            self.kill_file(ordinal, ledger);
            return;
        };
        let Ok(name_len) = u16::try_from(span.len) else {
            self.kill_file(ordinal, ledger);
            return;
        };
        let needs_extra = canonical.is_some() || raw.is_some();
        let position = self
            .file_extras
            .binary_search_by_key(&ordinal, |extra| extra.ordinal);
        let old = self.files[ordinal as usize];
        self.dead_bytes = self.dead_bytes.saturating_add(u64::from(old.name_len));
        let mut flags = (old.flags & (DEAD | REKEYED)) | kind.bits() | REKEYED;
        match (position, needs_extra) {
            (Ok(position), true) => {
                self.file_extras[position].canonical = canonical;
                self.file_extras[position].raw_name = raw.clone();
            }
            (Err(position), true) => self.file_extras.insert(
                position,
                FileExtra {
                    ordinal,
                    canonical,
                    raw_name: raw.clone(),
                },
            ),
            (Ok(position), false) => {
                self.file_extras.remove(position);
            }
            (Err(_), false) => {}
        }
        if raw.is_some() {
            flags |= RAW_NAME;
        }
        self.files[ordinal as usize] = FileRecord {
            dir,
            name_start: span.start,
            name_len,
            flags,
        };
        self.settle_rename_collision(ordinal, ledger);
        self.touch(ordinal, ledger);
    }

    /// A rename onto an indexed path or identity keeps the first ordinal,
    /// as the per-file representation's deduplication does. Candidates come
    /// from the key hashes; paths are rebuilt only for a hash hit.
    pub(super) fn settle_rename_collision(
        &mut self,
        ordinal: u32,
        ledger: &mut FileIndexMutationLedger,
    ) {
        let (canonical, display) = self.file_keys(ordinal, &mut ledger.batch.hashers);
        let mut other = None;
        if let Some(hash) = canonical {
            let candidates = self.table_candidates(Table::Canonical, hash, ledger);
            if candidates.iter().any(|candidate| *candidate != ordinal) {
                let path = self.file_canonical_path(ordinal).ok();
                other = candidates.into_iter().find(|candidate| {
                    *candidate != ordinal
                        && path.is_some()
                        && self.file_canonical_path(*candidate).ok() == path
                });
            }
        }
        if other.is_none() {
            let display_hash = display.unwrap_or_else(|| {
                let record = self.files[ordinal as usize];
                ledger
                    .batch
                    .hashers
                    .display(self, record.dir)
                    .child(self.file_name(ordinal).as_encoded_bytes())
            });
            let candidates = self.table_candidates(Table::Display, display_hash, ledger);
            if candidates.iter().any(|candidate| *candidate != ordinal) {
                let path = self.file_display_path(ordinal);
                other = candidates.into_iter().find(|candidate| {
                    *candidate != ordinal && self.file_display_path(*candidate) == path
                });
            }
        }
        if let Some(other) = other {
            self.kill_file(ordinal.max(other), ledger);
        }
    }

    pub(super) fn rename_dir(
        &mut self,
        dir: u32,
        new_path: &Path,
        ledger: &mut FileIndexMutationLedger,
    ) {
        ledger.canonical_resolutions = ledger.canonical_resolutions.saturating_add(1);
        let resolved = fs_metadata::canonical_path(new_path).map_err(|error| failure_of(&error));
        let subtree = self.subtree(dir, ledger);
        let old_canonical = self.dir_canonical_path(dir).ok();
        let old = self.dirs[dir as usize];
        self.ensure_children(ledger);
        if let Some(map) = ledger.batch.children.as_mut() {
            map.remove(old.parent, name_hash(self.dir_name(dir)), dir);
        }
        // Detach while the new parent is found, so a walk cannot pass
        // through the directory being moved.
        ledger.batch.dead_dirs.resize(self.dirs.len(), false);
        ledger.batch.dead_dirs[dir as usize] = true;
        ledger.batch.last_dir = None;
        let parent = new_path.parent().and_then(|parent| {
            self.longest_root(parent, None, ledger)?;
            let folder = self.folders[old.folder as usize].display.clone();
            self.dir_for(parent, &folder, ledger)
        });
        ledger.batch.dead_dirs[dir as usize] = false;
        let placed = self.place_dir(dir, new_path, parent, resolved, ledger);
        ledger.batch.hashers.clear();
        ledger.batch.canonical_cache = None;
        ledger.batch.last_dir = None;
        if !placed {
            self.kill_subtree_marked(&subtree, ledger);
            return;
        }
        self.dead_bytes = self.dead_bytes.saturating_add(u64::from(old.name.len));
        let new_canonical = self.dir_canonical_path(dir).ok();
        // Re-key every file of the subtree; symlinked files that resolved
        // into it follow it.
        let moved: Vec<u32> = (0..u32::try_from(self.files.len()).unwrap_or(u32::MAX))
            .filter(|ordinal| {
                let record = self.files[*ordinal as usize];
                !record.is_dead() && subtree.get(record.dir as usize).copied().unwrap_or(false)
            })
            .collect();
        for ordinal in &moved {
            self.files[*ordinal as usize].flags |= REKEYED;
            if let (Some(old_canonical), Some(new_canonical)) = (&old_canonical, &new_canonical)
                && let Ok(position) = self
                    .file_extras
                    .binary_search_by_key(ordinal, |extra| extra.ordinal)
                && let Some(target) = self.file_extras[position].canonical.as_deref()
                && let Ok(suffix) = target.strip_prefix(old_canonical)
            {
                let rewritten = new_canonical.join(suffix).into_boxed_path();
                let grown = retained(rewritten.as_os_str().len());
                if self.charge(grown, ledger) {
                    self.file_extras[position].canonical = Some(rewritten);
                }
            }
        }
        for ordinal in &moved {
            if !self.files[*ordinal as usize].is_dead() {
                self.settle_rename_collision(*ordinal, ledger);
            }
            self.touch(*ordinal, ledger);
        }
    }

    /// Give a directory its new display and canonical rules; `false` when
    /// the ledger refuses them.
    pub(super) fn place_dir(
        &mut self,
        dir: u32,
        new_path: &Path,
        parent: Option<u32>,
        resolved: Result<PathBuf, PaletteFileIdentityFailure>,
        ledger: &mut FileIndexMutationLedger,
    ) -> bool {
        let Some(name) = new_path.file_name() else {
            return false;
        };
        let display = if parent.is_some() {
            DirDisplay::Child
        } else {
            let Some(owned) = self.push_owned(new_path.to_path_buf(), ledger) else {
                return false;
            };
            DirDisplay::Detached(owned)
        };
        let parent = parent.unwrap_or(NO_DIR);
        let canonical = match resolved {
            Ok(path) => {
                let derived = display == DirDisplay::Child
                    && path.file_name() == Some(name)
                    && self
                        .dir_canonical_path(parent)
                        .is_ok_and(|parent_canonical| {
                            Some(parent_canonical.as_path()) == path.parent()
                        });
                if derived {
                    DirCanonical::Derived
                } else {
                    let Some(owned) = self.push_owned(path, ledger) else {
                        return false;
                    };
                    DirCanonical::Resolved(owned)
                }
            }
            Err(failure) => DirCanonical::Unavailable(failure),
        };
        // Every growth before the name (see `push_dir`).
        if !self.grow(Grown::DirRawNames, 1, ledger) {
            return false;
        }
        let (span, raw) = if display == DirDisplay::Child {
            let Some(pushed) = self.push_name(name, ledger) else {
                return false;
            };
            pushed
        } else {
            (NameSpan::default(), None)
        };
        let position = self.dir_raw_names.binary_search_by_key(&dir, |(id, _)| *id);
        match (position, raw) {
            (Ok(position), Some(raw)) => self.dir_raw_names[position].1 = raw,
            (Err(position), Some(raw)) => self.dir_raw_names.insert(position, (dir, raw)),
            (Ok(position), None) => {
                self.dir_raw_names.remove(position);
            }
            (Err(_), None) => {}
        }
        let record = &mut self.dirs[dir as usize];
        record.parent = parent;
        record.name = span;
        record.display = display;
        record.canonical = canonical;
        let folder = record.folder;
        if self.folders[folder as usize].root == dir {
            // The folder's display path no longer has a directory; a later
            // add below it creates a new root (design D5).
            self.folders[folder as usize].root = NO_DIR;
        }
        if let Some(map) = ledger.batch.children.as_mut() {
            match display {
                DirDisplay::Child => map.insert(parent, name_hash(self.dir_name(dir)), dir),
                DirDisplay::FolderRoot | DirDisplay::Detached(_) => map.roots.push(dir),
            }
        }
        true
    }

    pub(super) fn kill_subtree_marked(
        &mut self,
        subtree: &[bool],
        ledger: &mut FileIndexMutationLedger,
    ) {
        ledger.batch.dead_dirs.resize(self.dirs.len(), false);
        for (id, below) in subtree.iter().enumerate() {
            if *below {
                ledger.batch.dead_dirs[id] = true;
                if let Some(map) = ledger.batch.children.as_mut() {
                    let record = self.dirs[id];
                    let id = u32::try_from(id).unwrap_or(u32::MAX);
                    map.remove(record.parent, name_hash(self.dir_name(id)), id);
                }
            }
        }
        ledger.batch.any_dead = true;
    }
}
