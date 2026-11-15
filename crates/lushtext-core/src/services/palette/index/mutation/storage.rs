// SPDX-License-Identifier: GPL-3.0-or-later

//! Closing a mutation batch (`scale-file-index` design D4, D5): dropping dead
//! records, merging the batch's keys into the sorted tables, and compacting
//! the storage mutations leave behind.

use super::super::arena::{
    DEAD, DirCanonical, DirDisplay, IdentityKind, NO_DIR, NameSpan, REKEYED, key, key_ordinal,
    vec_bytes,
};
use super::super::paths::DirHashers;
use super::super::traversal::BuildLedger;
use super::super::{FileIndex, FileIndexTruncationReason};
use super::{Batch, FileIndexMutationLedger};

impl FileIndex {
    // --- Finish ------------------------------------------------------------

    /// Close a mutation batch begun with [`Self::incremental_mutation_ledger`]:
    /// every `*_for_bounded_batch` call of the batch must be followed by it
    /// before the index is read or published.
    pub(crate) fn finish_mutation_batch(&mut self, ledger: &mut FileIndexMutationLedger) {
        let batch = std::mem::take(&mut ledger.batch);
        let remap = if batch.any_dead {
            // A folder whose root directory was removed is no longer
            // addressable (`workspace_folder_for`), as the per-file index
            // pruned it; an add below its path creates a new root.
            for folder in &mut self.folders {
                if folder.root != NO_DIR
                    && batch
                        .dead_dirs
                        .get(folder.root as usize)
                        .copied()
                        .unwrap_or(false)
                {
                    folder.root = NO_DIR;
                }
            }
            Some(self.drop_dead_records(&batch.dead_dirs))
        } else {
            None
        };
        self.merge_keys(&batch, remap.as_deref());
        if 4 * self.dead_bytes > self.retained_byte_weight() {
            self.compact_arena();
        }
        let actual = self.retained_byte_weight();
        ledger.retained_bytes = actual;
        ledger.peak_retained_bytes = ledger.peak_retained_bytes.max(actual);
    }

    /// Drop dead records and records in dead directories; the ordinal remap.
    pub(super) fn drop_dead_records(&mut self, dead_dirs: &[bool]) -> Vec<u32> {
        let mut remap = Vec::with_capacity(self.files.len());
        let mut next = 0u32;
        for record in &mut self.files {
            if dead_dirs.get(record.dir as usize).copied().unwrap_or(false) {
                record.flags |= DEAD;
                self.dead_bytes = self.dead_bytes.saturating_add(u64::from(record.name_len));
            }
            if record.is_dead() {
                remap.push(u32::MAX);
            } else {
                remap.push(next);
                next += 1;
            }
        }
        self.files.retain(|record| !record.is_dead());
        self.file_extras.retain_mut(|extra| {
            let mapped = remap[extra.ordinal as usize];
            extra.ordinal = mapped;
            mapped != u32::MAX
        });
        remap
    }

    /// Merge the batch's touched records into the sorted key tables.
    pub(super) fn merge_keys(&mut self, batch: &Batch, remap: Option<&[u32]>) {
        let map = |ordinal: u32| remap.map_or(ordinal, |remap| remap[ordinal as usize]);
        let mut touched: Vec<u32> = batch
            .touched
            .iter()
            .map(|ordinal| map(*ordinal))
            .filter(|ordinal| *ordinal != u32::MAX)
            .collect();
        touched.sort_unstable();
        touched.dedup();
        let rekeyed =
            |index: &Self, ordinal: u32| index.files[ordinal as usize].flags & REKEYED != 0;
        let keep = |index: &Self, key_value: u64| -> Option<u64> {
            let ordinal = map(key_ordinal(key_value));
            (ordinal != u32::MAX && !rekeyed(index, ordinal)).then(|| key(key_value, ordinal))
        };
        let mut hashers = DirHashers::default();
        let mut fresh_canonical = Vec::new();
        let mut fresh_display = Vec::new();
        for ordinal in &touched {
            let (canonical, display) = self.file_keys(*ordinal, &mut hashers);
            if let Some(hash) = canonical {
                fresh_canonical.push(key(hash, *ordinal));
            }
            if let Some(hash) = display {
                fresh_display.push(key(hash, *ordinal));
            }
        }
        fresh_canonical.sort_unstable();
        fresh_display.sort_unstable();
        let canonical_keys = std::mem::take(&mut self.canonical_keys);
        let display_keys = std::mem::take(&mut self.display_keys);
        let kept_canonical: Vec<u64> = canonical_keys
            .iter()
            .filter_map(|value| keep(self, *value))
            .collect();
        let kept_display: Vec<u64> = display_keys
            .iter()
            .filter_map(|value| keep(self, *value))
            .collect();
        self.canonical_keys = merge_sorted(&kept_canonical, &fresh_canonical);
        self.display_keys = merge_sorted(&kept_display, &fresh_display);
        for ordinal in touched {
            self.files[ordinal as usize].flags &= !REKEYED;
        }
    }

    // --- Compaction --------------------------------------------------------

    /// Rewrite the name storage to the names live records reference (every
    /// directory's, every live file's), mid-batch: directory ids and file
    /// ordinals are unchanged, so a batch in progress keeps every id it holds.
    pub(super) fn compact_storage(&mut self, ledger: &mut FileIndexMutationLedger) {
        let before = self.retained_byte_weight();
        for record in &mut self.files {
            if ledger
                .batch
                .dead_dirs
                .get(record.dir as usize)
                .copied()
                .unwrap_or(false)
            {
                record.flags |= DEAD;
            }
        }
        ledger.batch.live_files = None;
        let keep = vec![true; self.dirs.len()];
        self.rewrite_names(&keep);
        self.dead_bytes = 0;
        let after = self.retained_byte_weight();
        ledger.retained_bytes = ledger
            .retained_bytes
            .saturating_sub(before.saturating_sub(after));
    }

    /// At a batch's finish: drop the directories no live file reaches (other
    /// than folder roots) and the owned paths only they referenced, then
    /// rewrite the names (design D5). File ordinals and the key tables are
    /// unchanged: no canonical or display path moves.
    pub(super) fn compact_arena(&mut self) {
        let mut keep = vec![false; self.dirs.len()];
        for record in self.files.iter().filter(|record| !record.is_dead()) {
            let mut current = record.dir;
            while current != NO_DIR && !keep[current as usize] {
                keep[current as usize] = true;
                current = self.dirs[current as usize].parent;
            }
        }
        for folder in &self.folders {
            if folder.root != NO_DIR {
                let mut current = folder.root;
                while current != NO_DIR && !keep[current as usize] {
                    keep[current as usize] = true;
                    current = self.dirs[current as usize].parent;
                }
            }
        }
        let dir_map = self.rewrite_names(&keep);
        let mut owned_map = vec![u32::MAX; self.owned_paths.len()];
        let mut owned_paths = Vec::new();
        let source = std::mem::take(&mut self.owned_paths);
        let mut keep_owned = |owned: u32| {
            if owned_map[owned as usize] == u32::MAX {
                owned_map[owned as usize] = u32::try_from(owned_paths.len()).unwrap_or(u32::MAX);
                owned_paths.push(source[owned as usize].clone());
            }
            owned_map[owned as usize]
        };
        let mut dirs = Vec::with_capacity(keep.iter().filter(|kept| **kept).count());
        for (id, record) in self.dirs.iter().enumerate() {
            if !keep[id] {
                continue;
            }
            let mut record = *record;
            if record.parent != NO_DIR {
                record.parent = dir_map[record.parent as usize];
            }
            if let DirDisplay::Detached(owned) = record.display {
                record.display = DirDisplay::Detached(keep_owned(owned));
            }
            if let DirCanonical::Resolved(owned) = record.canonical {
                record.canonical = DirCanonical::Resolved(keep_owned(owned));
            }
            dirs.push(record);
        }
        for record in self.files.iter_mut().filter(|record| !record.is_dead()) {
            record.dir = dir_map[record.dir as usize];
        }
        self.dir_raw_names.retain_mut(|(dir, _)| {
            let mapped = dir_map[*dir as usize];
            *dir = mapped;
            mapped != NO_DIR
        });
        for folder in &mut self.folders {
            if folder.root != NO_DIR {
                folder.root = dir_map[folder.root as usize];
            }
        }
        self.dirs = dirs;
        self.owned_paths = owned_paths;
        self.dead_bytes = 0;
        self.shrink_arena();
    }

    /// Copy the names of the kept directories and of live files into a new
    /// name storage of exact size; the map from old to new directory ids
    /// (`NO_DIR` for a directory not kept).
    pub(super) fn rewrite_names(&mut self, keep: &[bool]) -> Vec<u32> {
        let live_bytes: usize = self
            .files
            .iter()
            .filter(|record| !record.is_dead())
            .map(|record| usize::from(record.name_len))
            .sum::<usize>()
            + self
                .dirs
                .iter()
                .zip(keep)
                .filter(|(_, kept)| **kept)
                .map(|(record, _)| record.name.len as usize)
                .sum::<usize>();
        let mut names = String::with_capacity(live_bytes);
        let copy = |names: &mut String, span: NameSpan, source: &str| -> NameSpan {
            let start = u32::try_from(names.len()).unwrap_or(u32::MAX);
            names.push_str(&source[span.start as usize..(span.start + span.len) as usize]);
            NameSpan {
                start,
                len: span.len,
            }
        };
        let mut dir_map = vec![NO_DIR; self.dirs.len()];
        let mut next = 0u32;
        for (id, record) in self.dirs.iter_mut().enumerate() {
            if keep[id] {
                record.name = copy(&mut names, record.name, &self.names);
                dir_map[id] = next;
                next += 1;
            }
        }
        for record in &mut self.files {
            if record.is_dead() {
                record.name_start = 0;
                record.name_len = 0;
            } else {
                record.name_start = copy(&mut names, record.name_span(), &self.names).start;
            }
        }
        self.names = names;
        dir_map
    }

    /// Shrink every arena vector to its length (the transient copy
    /// `shrink_to_fit` makes is the only uncharged allocation).
    pub(in super::super) fn shrink_arena(&mut self) {
        self.folders.shrink_to_fit();
        self.dirs.shrink_to_fit();
        self.files.shrink_to_fit();
        self.names.shrink_to_fit();
        self.owned_paths.shrink_to_fit();
        self.dir_raw_names.shrink_to_fit();
        self.file_extras.shrink_to_fit();
        self.canonical_keys.shrink_to_fit();
        self.display_keys.shrink_to_fit();
    }

    // --- Keys --------------------------------------------------------------

    /// Build both key tables from scratch (construction), charging the
    /// directory hashers as scratch (less the `prepaid` bytes the caller
    /// already holds for them, which this releases) and the tables as
    /// installed bytes.
    pub(in super::super) fn rebuild_keys(
        &mut self,
        ledger: &mut BuildLedger,
        prepaid: u64,
    ) -> Result<(), FileIndexTruncationReason> {
        let hashers_charge = DirHashers::charge(self.dirs.len()).max(prepaid);
        if !ledger.try_charge_scratch(hashers_charge - prepaid) {
            ledger.release_scratch(prepaid);
            return Err(FileIndexTruncationReason::BuildByteLimit);
        }
        let mut hashers = DirHashers::default();
        let mut canonical_keys = Vec::new();
        let mut display_keys = Vec::new();
        let result = (|| {
            let mut canonical_count = 0usize;
            let mut display_count = 0usize;
            for ordinal in 0..self.files.len() {
                let record = self.files[ordinal];
                match record.identity_kind() {
                    IdentityKind::Derived if hashers.canonical(self, record.dir).is_some() => {
                        canonical_count += 1;
                    }
                    IdentityKind::Derived | IdentityKind::Unavailable(_) => display_count += 1,
                    IdentityKind::Explicit => {
                        canonical_count += 1;
                        display_count += 1;
                    }
                }
            }
            ledger.try_charge_installed(vec_bytes::<u64>(canonical_count + display_count))?;
            canonical_keys.reserve_exact(canonical_count);
            display_keys.reserve_exact(display_count);
            for ordinal in 0..u32::try_from(self.files.len()).unwrap_or(u32::MAX) {
                let (canonical, display) = self.file_keys(ordinal, &mut hashers);
                if let Some(hash) = canonical {
                    canonical_keys.push(key(hash, ordinal));
                }
                if let Some(hash) = display {
                    display_keys.push(key(hash, ordinal));
                }
            }
            canonical_keys.sort_unstable();
            display_keys.sort_unstable();
            Ok(())
        })();
        drop(hashers);
        ledger.release_scratch(hashers_charge);
        result?;
        self.canonical_keys = canonical_keys;
        self.display_keys = display_keys;
        Ok(())
    }
}

/// Merge two sorted key vectors into one of exact capacity.
pub(super) fn merge_sorted(left: &[u64], right: &[u64]) -> Vec<u64> {
    let mut merged = Vec::with_capacity(left.len() + right.len());
    let (mut i, mut j) = (0, 0);
    while i < left.len() && j < right.len() {
        if left[i] <= right[j] {
            merged.push(left[i]);
            i += 1;
        } else {
            merged.push(right[j]);
            j += 1;
        }
    }
    merged.extend_from_slice(&left[i..]);
    merged.extend_from_slice(&right[j..]);
    merged
}
