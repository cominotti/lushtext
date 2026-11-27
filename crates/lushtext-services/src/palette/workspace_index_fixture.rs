// SPDX-License-Identifier: GPL-3.0-or-later

//! Monorepo-shaped synthetic workspace trees for the file index
//! (`scale-file-index` design D9). Test-only: compiled under `cfg(test)` or the
//! `test-utils` feature, never in a shipping build.
//!
//! The shape is the one `scripts/generate-workspace-index-fixture.py` writes,
//! step for step (the same `splitmix64` draws in the same order), so a unit
//! test, a Criterion bench and the responsiveness lane index the same tree for
//! the same file count and seed: directory depth up to ten, a few dozen top-level directories at
//! 2^17 files, uniform 0–19 files
//! per created directory (about ten, so 2^20 files sit in about 110,000
//! directories), names of about 17 bytes, plus one symlinked directory and one
//! symlinked file at the top of the workspace folder. The two implementations are pinned to
//! one digest ([`SHAPE_DIGEST_4096`]) by both test suites.
//!
//! [`shape`] yields relative paths without touching the disk (the in-memory
//! constructors stream it), and [`write()`] creates the tree through the
//! filesystem fixture boundary.

use std::path::Path;

use crate::filesystem::fixture;

/// The seed both implementations default to.
///
/// Kind: out: tool; a fixed fixture seed.
pub const DEFAULT_SEED: u64 = 0x5CA1_EF11E;

/// FNV-1a 64 of the 4,096-file shape for [`DEFAULT_SEED`], one `kind:path\n`
/// line per entry; the Python generator's self-test asserts the same value.
///
/// Kind: out: tool; the pinned shape digest.
pub const SHAPE_DIGEST_4096: u64 = 0xA8CA_F952_4F0E_C2F8;

const WORDS: [&str; 32] = [
    "base",
    "browser",
    "renderer",
    "layout",
    "editing",
    "network",
    "content",
    "views",
    "frame",
    "autofill",
    "storage",
    "service",
    "public",
    "common",
    "core",
    "platform",
    "media",
    "gpu",
    "input",
    "test",
    "utils",
    "android",
    "devtools",
    "metrics",
    "policy",
    "sync",
    "history",
    "bookmarks",
    "ash",
    "wayland",
    "compositor",
    "sandbox",
];
const EXTENSIONS: [&str; 10] = [
    "cc", "h", "rs", "py", "md", "json", "ts", "gn", "mojom", "java",
];
/// Kind: out: tool; the D9 shape's deepest directory.
const MAX_DEPTH: usize = 10;
/// Kind: out: tool; the D9 shape's shallowest branch target.
const MIN_TARGET_DEPTH: usize = 6;
/// Kind: out: tool; files per created directory are uniform in `0..20`.
const FILES_PER_DIRECTORY_SPAN: u64 = 20;
/// A new branch returns to the root with probability `1 / BRANCH_ROLL` and to
/// the first level with probability `SECOND_LEVEL_ROLLS / BRANCH_ROLL`;
/// otherwise it climbs one to three levels.
///
/// Kind: out: tool; a fixture draw range.
const BRANCH_ROLL: u64 = 256;
/// Kind: out: tool; a fixture draw range.
const SECOND_LEVEL_ROLLS: u64 = 8;

/// Name of the symlinked directory [`write()`] adds at the root.
pub const LINKED_DIRECTORY: &str = "linked_dir";
/// Name of the symlinked file [`write()`] adds at the root.
pub const LINKED_FILE: &str = "linked_file.txt";

/// One entry of the shape, parents before children, as a `/`-separated path
/// relative to the workspace folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShapeEntry {
    /// A directory, yielded before its files and subdirectories.
    Directory(String),
    /// A regular file.
    File(String),
}

struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    fn index(&mut self, len: usize) -> usize {
        usize::try_from(self.below(len as u64)).unwrap_or(0)
    }
}

fn base36(mut value: u64) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_string();
    }
    let mut out = Vec::new();
    while value > 0 {
        out.push(DIGITS[usize::try_from(value % 36).unwrap_or(0)]);
        value /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// Streaming iterator over the shape; see [`shape`].
pub struct WorkspaceIndexShape {
    rng: SplitMix64,
    stack: Vec<String>,
    serial: u64,
    files: usize,
    emitted: usize,
    target: usize,
    directory: String,
    remaining_in_directory: u64,
    next_in_directory: u64,
}

/// The shape of `files` regular files for `seed`, streamed: the iterator holds
/// one directory stack, never the whole tree.
#[must_use]
pub fn shape(files: usize, seed: u64) -> WorkspaceIndexShape {
    WorkspaceIndexShape {
        rng: SplitMix64(seed),
        stack: Vec::new(),
        serial: 0,
        files,
        emitted: 0,
        target: 0,
        directory: String::new(),
        remaining_in_directory: 0,
        next_in_directory: 0,
    }
}

impl Iterator for WorkspaceIndexShape {
    type Item = ShapeEntry;

    fn next(&mut self) -> Option<ShapeEntry> {
        if self.remaining_in_directory > 0 {
            self.remaining_in_directory -= 1;
            let ordinal = self.next_in_directory;
            self.next_in_directory += 1;
            let mut stem = WORDS[self.rng.index(WORDS.len())].to_string();
            if self.rng.below(4) != 0 {
                stem.push('_');
                stem.push_str(WORDS[self.rng.index(WORDS.len())]);
            }
            let extension = EXTENSIONS[self.rng.index(EXTENSIONS.len())];
            self.emitted += 1;
            return Some(ShapeEntry::File(format!(
                "{}/{stem}_{ordinal}.{extension}",
                self.directory
            )));
        }
        if self.emitted >= self.files {
            return None;
        }
        if self.stack.len() >= self.target {
            let roll = self.rng.below(BRANCH_ROLL);
            let keep = if roll == 0 {
                0
            } else if roll <= SECOND_LEVEL_ROLLS {
                self.stack.len().min(1)
            } else {
                let pop = 1 + usize::try_from(self.rng.below(3)).unwrap_or(0);
                self.stack.len().saturating_sub(pop)
            };
            self.stack.truncate(keep);
            let span = (MAX_DEPTH - MIN_TARGET_DEPTH + 1) as u64;
            let drawn = MIN_TARGET_DEPTH + usize::try_from(self.rng.below(span)).unwrap_or(0);
            self.target = drawn.max(keep + 1);
        }
        let word = WORDS[self.rng.index(WORDS.len())];
        self.stack.push(format!("{word}_{}", base36(self.serial)));
        self.serial += 1;
        self.directory = self.stack.join("/");
        let remaining = (self.files - self.emitted) as u64;
        self.remaining_in_directory = self.rng.below(FILES_PER_DIRECTORY_SPAN).min(remaining);
        self.next_in_directory = 0;
        Some(ShapeEntry::Directory(self.directory.clone()))
    }
}

/// What [`write()`] created.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkspaceIndexFixtureSummary {
    /// Regular files written.
    pub files: usize,
    /// Directories created (the root and the symlinks excluded).
    pub directories: usize,
}

/// Write the shape under `root` (created if missing), with the two root
/// symlinks when `symlinks` is set: [`LINKED_DIRECTORY`] to the first top-level
/// directory and [`LINKED_FILE`] to the first file, both relative.
///
/// # Panics
///
/// When the fixture boundary cannot create an entry.
#[must_use]
pub fn write(root: &Path, files: usize, seed: u64, symlinks: bool) -> WorkspaceIndexFixtureSummary {
    fixture::create_dir_all(root);
    let mut summary = WorkspaceIndexFixtureSummary {
        files: 0,
        directories: 0,
    };
    let mut first_directory: Option<String> = None;
    let mut first_file: Option<String> = None;
    for entry in shape(files, seed) {
        match entry {
            ShapeEntry::Directory(relative) => {
                fixture::create_dir(&root.join(&relative));
                summary.directories += 1;
                first_directory.get_or_insert(relative);
            }
            ShapeEntry::File(relative) => {
                fixture::write_bytes(&root.join(&relative), b"");
                summary.files += 1;
                first_file.get_or_insert(relative);
            }
        }
    }
    if symlinks && let (Some(directory), Some(file)) = (first_directory, first_file) {
        let top = directory
            .split('/')
            .next()
            .unwrap_or(&directory)
            .to_string();
        fixture::symlink(Path::new(&top), &root.join(LINKED_DIRECTORY));
        fixture::symlink(Path::new(&file), &root.join(LINKED_FILE));
    }
    summary
}

/// FNV-1a 64 of the shape's `kind:path\n` lines (see [`SHAPE_DIGEST_4096`]).
#[must_use]
pub fn shape_digest(files: usize, seed: u64) -> u64 {
    let mut value: u64 = 0xCBF2_9CE4_8422_2325;
    for entry in shape(files, seed) {
        let line = match entry {
            ShapeEntry::Directory(path) => format!("dir:{path}\n"),
            ShapeEntry::File(path) => format!("file:{path}\n"),
        };
        for byte in line.bytes() {
            value ^= u64::from(byte);
            value = value.wrapping_mul(0x0000_0100_0000_01B3);
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shape_matches_the_python_generator() {
        assert_eq!(shape_digest(4096, DEFAULT_SEED), SHAPE_DIGEST_4096);
        let first: Vec<_> = shape(4096, DEFAULT_SEED).take(3).collect();
        assert_eq!(
            first,
            vec![
                ShapeEntry::Directory("common_0".into()),
                ShapeEntry::File("common_0/bookmarks_0.h".into()),
                ShapeEntry::File("common_0/editing_browser_1.ts".into()),
            ]
        );
    }

    #[test]
    fn the_shape_has_the_requested_files_and_depth() {
        let mut files = 0;
        let mut directories = 0;
        for entry in shape(10_000, DEFAULT_SEED) {
            match entry {
                ShapeEntry::Directory(path) => {
                    directories += 1;
                    assert!(path.split('/').count() <= MAX_DEPTH);
                }
                ShapeEntry::File(_) => files += 1,
            }
        }
        assert_eq!(files, 10_000);
        let per_directory = f64::from(files) / f64::from(directories);
        assert!((7.0..=12.0).contains(&per_directory), "{per_directory}");
    }
}
