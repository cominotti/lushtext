// SPDX-License-Identifier: GPL-3.0-or-later

//! The file index's interned storage (`scale-file-index` design D1).
//!
//! Every directory is one [`DirRecord`] (its parent, its name, how its display
//! path and its canonical path are found) and every file one 12-byte
//! [`FileRecord`] (its directory, the span of its name, identity flags), over
//! one `names` buffer holding every directory and file name as UTF-8. Display
//! path, relative display and canonical path are rebuilt on demand, for the
//! rows a view shows and for the exact comparisons hash lookups confirm.
//!
//! Identity (design D2): a non-symlink entry's canonical path is its
//! directory's canonical path joined with its name, so a file record whose
//! identity is [`IdentityKind::Derived`] stores nothing more. A symlinked file
//! keeps the path `realpath` returned in [`FileExtra`]; a file whose
//! resolution failed keeps the typed failure in its flags.
//!
//! Keys (design D4): `canonical_keys` holds, sorted, one `u64` per file with a
//! canonical path, its top 32 bits the path's hash and its low 32 bits the file
//! ordinal; `display_keys` holds the same for the few files whose canonical
//! path is not their directory's plus their name (symlinks, failures), keyed by
//! the display path, so an added path finds an equal display path without a
//! scan. Every hash hit is confirmed by an exact comparison of rebuilt paths.

use std::ffi::{OsStr, OsString};
use std::hash::{DefaultHasher, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::model::palette::PaletteFileIdentityFailure;

/// No directory: the parent of a root directory record.
///
/// Kind: out: identifier; a sentinel.
pub(super) const NO_DIR: u32 = u32::MAX;

/// A span of `FileIndex::names`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct NameSpan {
    pub(super) start: u32,
    pub(super) len: u32,
}

/// Where a directory's display path comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DirDisplay {
    /// Its parent's display path joined with its name.
    Child,
    /// Its workspace folder's display path (a folder's root directory).
    FolderRoot,
    /// An owned path (`FileIndex::owned_paths`): a directory a rename moved to
    /// a path that is not below a directory of its folder (design D5).
    Detached(u32),
}

/// Where a directory's canonical path comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DirCanonical {
    /// Its parent's canonical path joined with its name (a non-symlink child).
    Derived,
    /// An owned path (`FileIndex::owned_paths`): a workspace folder, a
    /// symlinked directory, or a directory whose path a rename resolved.
    Resolved(u32),
    /// Resolution failed: every derived identity below it is unavailable.
    Unavailable(PaletteFileIdentityFailure),
}

/// One interned directory.
#[derive(Clone, Copy, Debug)]
pub(super) struct DirRecord {
    pub(super) parent: u32,
    pub(super) name: NameSpan,
    pub(super) display: DirDisplay,
    pub(super) canonical: DirCanonical,
    /// The workspace folder whose traversal or add reached this directory.
    pub(super) folder: u32,
}

/// One indexed file: 12 bytes.
#[derive(Clone, Copy, Debug)]
pub(super) struct FileRecord {
    pub(super) dir: u32,
    pub(super) name_start: u32,
    pub(super) name_len: u16,
    pub(super) flags: u8,
}

const _: () = assert!(std::mem::size_of::<FileRecord>() == 12);

/// The flag bits holding the record's [`IdentityKind`].
///
/// Kind: out: identifier; a flag mask.
const IDENTITY_MASK: u8 = 0b0000_0111;
/// The record's exact name bytes are in its [`FileExtra`] (not UTF-8).
///
/// Kind: out: identifier; a flag bit.
pub(super) const RAW_NAME: u8 = 0b0000_1000;
/// The record's keys changed in the current mutation batch.
///
/// Kind: out: identifier; a flag bit.
pub(super) const REKEYED: u8 = 0b0100_0000;
/// The record is removed when the current mutation batch finishes.
///
/// Kind: out: identifier; a flag bit.
pub(super) const DEAD: u8 = 0b1000_0000;

/// How a file's identity is found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum IdentityKind {
    /// Its directory's canonical path joined with its name.
    Derived,
    /// The path `realpath` returned, kept in its [`FileExtra`].
    Explicit,
    /// Resolution failed.
    Unavailable(PaletteFileIdentityFailure),
}

const FAILURES: [PaletteFileIdentityFailure; 5] = [
    PaletteFileIdentityFailure::NotResolved,
    PaletteFileIdentityFailure::NotFound,
    PaletteFileIdentityFailure::PermissionDenied,
    PaletteFileIdentityFailure::InvalidInput,
    PaletteFileIdentityFailure::Other,
];

impl IdentityKind {
    pub(super) fn bits(self) -> u8 {
        match self {
            Self::Derived => 0,
            Self::Explicit => 1,
            Self::Unavailable(failure) => {
                2 + u8::try_from(
                    FAILURES
                        .iter()
                        .position(|known| *known == failure)
                        .unwrap_or(4),
                )
                .unwrap_or(6)
            }
        }
    }

    pub(super) fn from_flags(flags: u8) -> Self {
        match flags & IDENTITY_MASK {
            0 => Self::Derived,
            1 => Self::Explicit,
            bits => Self::Unavailable(FAILURES[usize::from(bits - 2).min(4)]),
        }
    }
}

impl FileRecord {
    pub(super) fn identity_kind(self) -> IdentityKind {
        IdentityKind::from_flags(self.flags)
    }

    pub(super) const fn is_dead(self) -> bool {
        self.flags & DEAD != 0
    }

    pub(super) fn name_span(self) -> NameSpan {
        NameSpan {
            start: self.name_start,
            len: u32::from(self.name_len),
        }
    }
}

/// The rare per-file data a record cannot hold, keyed by file ordinal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FileExtra {
    pub(super) ordinal: u32,
    /// A symlinked file's resolved canonical path.
    pub(super) canonical: Option<Box<Path>>,
    /// The exact bytes of a name that is not UTF-8.
    pub(super) raw_name: Option<Box<OsStr>>,
}

/// A workspace folder: its display path (shared with callers, as
/// `workspace_folder_for` returns it) and its root directory record.
#[derive(Clone, Debug)]
pub(super) struct FolderRecord {
    pub(super) display: Arc<PathBuf>,
    pub(super) root: u32,
}

// --- Hashing ---------------------------------------------------------------

#[cfg(test)]
thread_local! {
    static FORCE_HASH_COLLISIONS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Make every path hash equal on this thread, so tests can prove that each
/// hash hit is confirmed by an exact comparison.
#[cfg(test)]
pub(super) fn force_hash_collisions_for_test(enabled: bool) {
    FORCE_HASH_COLLISIONS.with(|forced| forced.set(enabled));
}

pub(super) fn finish_hash(hash: u64) -> u64 {
    #[cfg(test)]
    if FORCE_HASH_COLLISIONS.with(std::cell::Cell::get) {
        return 0;
    }
    hash
}

/// A streaming path hasher: the hash of `dir.join(name)` computed from a
/// clone of the state after `dir`, without building the joined path.
#[derive(Clone)]
pub(super) struct PathHasher {
    state: DefaultHasher,
    ends_with_separator: bool,
}

impl PathHasher {
    pub(super) fn new(path: &Path) -> Self {
        let bytes = path.as_os_str().as_encoded_bytes();
        let mut state = DefaultHasher::new();
        state.write(bytes);
        Self {
            state,
            ends_with_separator: bytes.last() == Some(&b'/'),
        }
    }

    /// The hash of the full path this hasher was built from.
    pub(super) fn finish(&self) -> u64 {
        finish_hash(self.state.finish())
    }

    /// The hasher of this path joined with `name`.
    pub(super) fn child_hasher(&self, name: &[u8]) -> Self {
        let mut state = self.state.clone();
        if !self.ends_with_separator {
            state.write(b"/");
        }
        state.write(name);
        Self {
            state,
            ends_with_separator: name.last() == Some(&b'/'),
        }
    }

    /// The hash of this path joined with `name`.
    pub(super) fn child(&self, name: &[u8]) -> u64 {
        let mut state = self.state.clone();
        if !self.ends_with_separator {
            state.write(b"/");
        }
        state.write(name);
        finish_hash(state.finish())
    }
}

/// The hash of `path`, equal to the [`PathHasher::child`] composition.
pub(super) fn path_hash(path: &Path) -> u64 {
    PathHasher::new(path).finish()
}

/// A sorted key: the hash's top 32 bits, then the file ordinal.
pub(super) const fn key(hash: u64, ordinal: u32) -> u64 {
    (hash & 0xFFFF_FFFF_0000_0000) | ordinal as u64
}

pub(super) const fn key_ordinal(key: u64) -> u32 {
    (key & 0xFFFF_FFFF) as u32
}

/// The keys of `keys` (sorted) whose hash is `hash`'s.
pub(super) fn key_range(keys: &[u64], hash: u64) -> &[u64] {
    let high = hash & 0xFFFF_FFFF_0000_0000;
    let start = keys.partition_point(|key| (key & 0xFFFF_FFFF_0000_0000) < high);
    let end = start + keys[start..].partition_point(|key| (key & 0xFFFF_FFFF_0000_0000) == high);
    &keys[start..end]
}

/// Binary-search steps a lookup of `len` sorted keys costs: the mutation
/// ledger's path-comparison evidence counts these with the exact comparisons.
pub(super) fn search_steps(len: usize) -> u64 {
    u64::from(usize::BITS - len.leading_zeros())
}

// --- Names -----------------------------------------------------------------

/// The lossy UTF-8 form of `name` (what nucleo scores) and, when it is not
/// UTF-8, its exact bytes.
pub(super) fn split_name(name: &OsStr) -> (std::borrow::Cow<'_, str>, Option<Box<OsStr>>) {
    match name.to_str() {
        Some(text) => (std::borrow::Cow::Borrowed(text), None),
        None => (
            name.to_string_lossy(),
            Some(OsString::from(name).into_boxed_os_str()),
        ),
    }
}

pub(super) fn retained(bytes: usize) -> u64 {
    u64::try_from(bytes).unwrap_or(u64::MAX)
}

pub(super) fn vec_bytes<T>(capacity: usize) -> u64 {
    retained(capacity.saturating_mul(std::mem::size_of::<T>()))
}

/// Heap bytes of an owned path.
pub(super) fn path_bytes(path: &PathBuf) -> u64 {
    retained(path.capacity())
}

/// Heap bytes of a shared workspace folder (the `Arc` block and its path).
pub(super) fn folder_bytes(folder: &Arc<PathBuf>) -> u64 {
    retained(
        std::mem::size_of::<PathBuf>()
            .saturating_add(std::mem::size_of::<usize>().saturating_mul(2))
            .saturating_add(folder.capacity()),
    )
}

/// The failure an I/O error maps to.
pub(super) fn failure_of(error: &std::io::Error) -> PaletteFileIdentityFailure {
    PaletteFileIdentityFailure::from(error.kind())
}
