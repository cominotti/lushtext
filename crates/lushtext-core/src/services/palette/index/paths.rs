// SPDX-License-Identifier: GPL-3.0-or-later

//! Rebuilding paths from the arena, and finding files by path
//! (`scale-file-index` design D1, D4).
//!
//! A directory's display path is its chain of names up to a workspace folder
//! root or a detached path; its canonical path is its chain up to the first
//! resolved directory. A file's paths are its directory's joined with its
//! name. Lookups hash a path and confirm every hit by rebuilding the
//! candidate's path and comparing it exactly.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::model::palette::{PaletteFileIdentity, PaletteFileIdentityFailure};

use super::FileIndex;
use super::arena::{
    DirCanonical, DirDisplay, FileExtra, IdentityKind, NO_DIR, NameSpan, PathHasher, RAW_NAME,
    key_ordinal, key_range, path_hash,
};

impl FileIndex {
    pub(super) fn span_str(&self, span: NameSpan) -> &str {
        let start = span.start as usize;
        &self.names[start..start + span.len as usize]
    }

    /// A directory's exact name.
    pub(super) fn dir_name(&self, dir: u32) -> &OsStr {
        if let Ok(position) = self.dir_raw_names.binary_search_by_key(&dir, |(id, _)| *id) {
            return &self.dir_raw_names[position].1;
        }
        OsStr::new(self.span_str(self.dirs[dir as usize].name))
    }

    pub(super) fn file_extra(&self, ordinal: u32) -> Option<&FileExtra> {
        self.file_extras
            .binary_search_by_key(&ordinal, |extra| extra.ordinal)
            .ok()
            .map(|position| &self.file_extras[position])
    }

    /// A file's lossy UTF-8 name: what fuzzy matching scores.
    pub(super) fn file_name_str(&self, ordinal: u32) -> &str {
        self.span_str(self.files[ordinal as usize].name_span())
    }

    /// A file's exact name.
    pub(super) fn file_name(&self, ordinal: u32) -> &OsStr {
        let record = self.files[ordinal as usize];
        if record.flags & RAW_NAME != 0
            && let Some(name) = self
                .file_extra(ordinal)
                .and_then(|extra| extra.raw_name.as_deref())
        {
            return name;
        }
        OsStr::new(self.span_str(record.name_span()))
    }

    /// The names from `dir` up to (excluding) the first directory `stop`
    /// accepts, and that directory.
    fn chain_to(&self, dir: u32, mut stop: impl FnMut(u32) -> bool) -> (Vec<u32>, u32) {
        let mut chain = Vec::new();
        let mut current = dir;
        while !stop(current) {
            chain.push(current);
            current = self.dirs[current as usize].parent;
            debug_assert_ne!(current, NO_DIR, "a child directory chain ends at a root");
        }
        (chain, current)
    }

    fn push_chain(&self, mut path: PathBuf, chain: &[u32]) -> PathBuf {
        for dir in chain.iter().rev() {
            path.push(self.dir_name(*dir));
        }
        path
    }

    /// The display path of a directory's root (folder root or detached).
    pub(super) fn display_origin(&self, dir: u32) -> &Path {
        let record = self.dirs[dir as usize];
        match record.display {
            DirDisplay::FolderRoot => self.folders[record.folder as usize].display.as_path(),
            DirDisplay::Detached(owned) => self.owned_paths[owned as usize].as_path(),
            DirDisplay::Child => unreachable!("a display root is not a child directory"),
        }
    }

    pub(super) fn dir_display_path(&self, dir: u32) -> PathBuf {
        let (chain, root) = self.chain_to(dir, |id| {
            self.dirs[id as usize].display != DirDisplay::Child
        });
        self.push_chain(self.display_origin(root).to_path_buf(), &chain)
    }

    /// A directory's canonical path, or the failure its chain records.
    pub(super) fn dir_canonical_path(
        &self,
        dir: u32,
    ) -> Result<PathBuf, PaletteFileIdentityFailure> {
        let (chain, root) = self.chain_to(dir, |id| {
            self.dirs[id as usize].canonical != DirCanonical::Derived
        });
        match self.dirs[root as usize].canonical {
            DirCanonical::Resolved(owned) => {
                Ok(self.push_chain(self.owned_paths[owned as usize].clone(), &chain))
            }
            DirCanonical::Unavailable(failure) => Err(failure),
            DirCanonical::Derived => unreachable!("the chain stops at a non-derived directory"),
        }
    }

    pub(super) fn file_display_path(&self, ordinal: u32) -> PathBuf {
        let record = self.files[ordinal as usize];
        let mut path = self.dir_display_path(record.dir);
        path.push(self.file_name(ordinal));
        path
    }

    /// A file's canonical path, when it has one.
    pub(super) fn file_canonical_path(
        &self,
        ordinal: u32,
    ) -> Result<PathBuf, PaletteFileIdentityFailure> {
        let record = self.files[ordinal as usize];
        match record.identity_kind() {
            IdentityKind::Derived => {
                let mut path = self.dir_canonical_path(record.dir)?;
                path.push(self.file_name(ordinal));
                Ok(path)
            }
            IdentityKind::Explicit => self
                .file_extra(ordinal)
                .and_then(|extra| extra.canonical.as_deref())
                .map(Path::to_path_buf)
                .ok_or(PaletteFileIdentityFailure::NotResolved),
            IdentityKind::Unavailable(failure) => Err(failure),
        }
    }

    pub(super) fn file_identity(&self, ordinal: u32) -> PaletteFileIdentity {
        match self.file_canonical_path(ordinal) {
            Ok(path) => PaletteFileIdentity::canonical(path),
            Err(failure) => PaletteFileIdentity::Unavailable(failure),
        }
    }

    pub(super) fn file_folder(&self, ordinal: u32) -> &Arc<PathBuf> {
        let dir = self.files[ordinal as usize].dir;
        &self.folders[self.dirs[dir as usize].folder as usize].display
    }

    /// The live ordinals whose canonical path is `path`.
    pub(super) fn ordinals_with_canonical(&self, path: &Path) -> Vec<u32> {
        let hash = path_hash(path);
        key_range(&self.canonical_keys, hash)
            .iter()
            .map(|key| key_ordinal(*key))
            .filter(|ordinal| {
                !self.files[*ordinal as usize].is_dead()
                    && self
                        .file_canonical_path(*ordinal)
                        .is_ok_and(|candidate| candidate == path)
            })
            .collect()
    }
}

/// Memoized per-directory path hashers: the canonical one (`None` when the
/// directory's canonical chain ends in a failure) and the display one, each
/// computed from its parent's, so a key costs one name write however long
/// the path (a 256 KiB detached prefix is hashed once).
#[derive(Default)]
pub(super) struct DirHashers {
    canonical: Vec<Option<Option<PathHasher>>>,
    display: Vec<Option<PathHasher>>,
}

impl DirHashers {
    pub(super) fn clear(&mut self) {
        self.canonical.clear();
        self.display.clear();
    }

    fn fit(&mut self, dirs: usize) {
        if self.canonical.len() < dirs {
            self.canonical.resize(dirs, None);
            self.display.resize(dirs, None);
        }
    }

    pub(super) fn canonical(&mut self, index: &FileIndex, dir: u32) -> Option<PathHasher> {
        self.fit(index.dirs.len());
        let mut chain = Vec::new();
        let mut current = dir;
        let mut base = loop {
            if let Some(known) = &self.canonical[current as usize] {
                break known.clone();
            }
            let record = index.dirs[current as usize];
            match record.canonical {
                DirCanonical::Derived => {
                    chain.push(current);
                    current = record.parent;
                }
                DirCanonical::Resolved(owned) => {
                    let hasher = Some(PathHasher::new(&index.owned_paths[owned as usize]));
                    self.canonical[current as usize] = Some(hasher.clone());
                    break hasher;
                }
                DirCanonical::Unavailable(_) => {
                    self.canonical[current as usize] = Some(None);
                    break None;
                }
            }
        };
        for id in chain.into_iter().rev() {
            base = base.map(|parent| parent.child_hasher(index.dir_name(id).as_encoded_bytes()));
            self.canonical[id as usize] = Some(base.clone());
        }
        base
    }

    pub(super) fn display(&mut self, index: &FileIndex, dir: u32) -> PathHasher {
        self.fit(index.dirs.len());
        let mut chain = Vec::new();
        let mut current = dir;
        let mut base = loop {
            if let Some(known) = &self.display[current as usize] {
                break known.clone();
            }
            let record = index.dirs[current as usize];
            if record.display == DirDisplay::Child {
                chain.push(current);
                current = record.parent;
            } else {
                let hasher = PathHasher::new(index.display_origin(current));
                self.display[current as usize] = Some(hasher.clone());
                break hasher;
            }
        };
        for id in chain.into_iter().rev() {
            base = base.child_hasher(index.dir_name(id).as_encoded_bytes());
            self.display[id as usize] = Some(base.clone());
        }
        base
    }
}

impl FileIndex {
    /// A file's canonical-key hash and, when its canonical path is not its
    /// directory's plus its name, its display-key hash (design D4).
    pub(super) fn file_keys(
        &self,
        ordinal: u32,
        hashers: &mut DirHashers,
    ) -> (Option<u64>, Option<u64>) {
        let record = self.files[ordinal as usize];
        let name = self.file_name(ordinal).as_encoded_bytes();
        let display = |hashers: &mut DirHashers| hashers.display(self, record.dir).child(name);
        match record.identity_kind() {
            IdentityKind::Derived => match hashers.canonical(self, record.dir) {
                Some(hasher) => (Some(hasher.child(name)), None),
                None => (None, Some(display(hashers))),
            },
            IdentityKind::Explicit => (
                self.file_extra(ordinal)
                    .and_then(|extra| extra.canonical.as_deref())
                    .map(path_hash),
                Some(display(hashers)),
            ),
            IdentityKind::Unavailable(_) => (None, Some(display(hashers))),
        }
    }
}
