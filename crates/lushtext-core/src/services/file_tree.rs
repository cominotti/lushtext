// SPDX-License-Identifier: GPL-3.0-or-later

//! File tree scanning: read directory contents sorted for sidebar display.
//!
//! Pure I/O service with no GTK dependencies. Returns standard Rust types
//! that the UI layer converts into GObject models.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::fs::DirEntry;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use unicase::UniCase;

/// A single sidebar-visible filesystem entry returned by a directory scan.
///
/// This stays in the service layer as plain Rust data so callers in the UI,
/// benchmarks, and other services can share one documented shape without
/// depending on tuple field positions.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DirectoryEntry {
    /// Absolute filesystem path for the file or directory.
    pub path: PathBuf,
    /// True when the path points to a directory.
    pub is_dir: bool,
    /// Empty-directory hint for sidebar affordances.
    ///
    /// `Some(true)` means the directory was checked and found empty,
    /// `Some(false)` means it was checked and contains visible entries,
    /// and `None` means emptiness was not checked (files, or directories past
    /// the scan's lookahead budget).
    pub is_empty: Option<bool>,
}

/// Result of a bounded directory scan.
#[derive(Debug, Default)]
pub struct DirectoryScan {
    /// Sorted entries for the directory, directories-first then alphabetical.
    pub entries: Vec<DirectoryEntry>,
    /// True if the directory had more entries than `max_entries`.
    pub truncated: bool,
    /// True if the cancellation token was set during scanning.
    pub cancelled: bool,
}

#[derive(Debug, Eq, PartialEq)]
struct SortedEntry {
    path: PathBuf,
    is_dir: bool,
    is_empty: Option<bool>,
}

impl Ord for SortedEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_entries(
            (self.path.as_path(), self.is_dir),
            (other.path.as_path(), other.is_dir),
        )
    }
}

impl PartialOrd for SortedEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Scan a directory and return sorted entries (directories first, then alphabetical).
/// Skips hidden files (starting with `.`).
#[must_use]
pub fn scan_directory(dir_path: &Path) -> Vec<DirectoryEntry> {
    scan_directory_bounded(dir_path, usize::MAX, 1000, None).entries
}

/// Peek into a directory to see if it contains any visible (non-hidden) entries.
#[must_use]
pub fn is_dir_empty(path: &Path) -> bool {
    if let Ok(mut rd) = std::fs::read_dir(path) {
        !rd.any(|e| {
            if let Ok(e) = e {
                e.file_name().as_encoded_bytes().first() != Some(&b'.')
            } else {
                false
            }
        })
    } else {
        false // Assume not empty on error to allow user to try expanding it
    }
}

/// Scan a directory while bounding memory and allowing cooperative cancellation.
///
/// The result is still sorted directories-first and alphabetically, but for
/// very large folders only the best `max_entries` rows are retained in memory.
pub fn scan_directory_bounded(
    dir_path: &Path,
    max_entries: usize,
    lookahead_cap: usize,
    cancel: Option<&AtomicBool>,
) -> DirectoryScan {
    let read_dir = match std::fs::read_dir(dir_path) {
        Ok(rd) => rd,
        Err(e) => {
            tracing::warn!("Cannot read {}: {}", dir_path.display(), e);
            return DirectoryScan::default();
        }
    };

    let mut heap = BinaryHeap::with_capacity(max_entries.saturating_add(1).min(256));
    let mut truncated = false;
    let mut dirs_checked = 0;

    for entry in read_dir.flatten() {
        if cancel.is_some_and(|flag| flag.load(AtomicOrdering::Acquire)) {
            return DirectoryScan {
                entries: drain_sorted_entries(heap),
                truncated,
                cancelled: true,
            };
        }
        if entry.file_name().as_encoded_bytes().first() == Some(&b'.') {
            continue;
        }
        let Some((path, is_dir)) = classify_entry(&entry) else {
            continue;
        };

        let mut is_empty = None;
        if is_dir && dirs_checked < lookahead_cap {
            dirs_checked += 1;
            is_empty = Some(is_dir_empty(&path));
        }

        heap.push(SortedEntry {
            path,
            is_dir,
            is_empty,
        });
        if heap.len() > max_entries {
            heap.pop();
            truncated = true;
        }
    }

    DirectoryScan {
        entries: drain_sorted_entries(heap),
        truncated,
        cancelled: false,
    }
}

/// Classify a DirEntry as file or directory, resolving symlinks.
/// Returns `None` for broken symlinks.
fn classify_entry(entry: &DirEntry) -> Option<(PathBuf, bool)> {
    let path = entry.path();

    match entry.file_type() {
        Ok(file_type) if !file_type.is_symlink() => Some((path, file_type.is_dir())),
        Ok(_) | Err(_) => {
            // Follow symlinks so valid symlinked directories/files are included,
            // while broken targets are skipped.
            let meta = std::fs::metadata(&path).ok()?;
            Some((path, meta.is_dir()))
        }
    }
}

fn drain_sorted_entries(heap: BinaryHeap<SortedEntry>) -> Vec<DirectoryEntry> {
    heap.into_sorted_vec()
        .into_iter()
        .map(|entry| DirectoryEntry {
            path: entry.path,
            is_dir: entry.is_dir,
            is_empty: entry.is_empty,
        })
        .collect()
}

/// Sort order: directories before files, then case-insensitive alphabetical.
/// Uses `UniCase` for Unicode-aware comparison without allocation.
fn compare_entries(
    (path_a, is_dir_a): (&Path, bool),
    (path_b, is_dir_b): (&Path, bool),
) -> Ordering {
    is_dir_b.cmp(&is_dir_a).then_with(|| {
        let a = path_a
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        let b = path_b
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        UniCase::new(a).cmp(&UniCase::new(b))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: extract file names from scan results.
    fn names(entries: &[DirectoryEntry]) -> Vec<String> {
        entries
            .iter()
            .map(|entry| {
                entry
                    .path
                    .file_name()
                    .expect("expected operation to succeed")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    #[test]
    fn test_empty_directory() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let entries = scan_directory(dir.path());
        assert!(entries.is_empty());
    }

    #[test]
    fn test_hidden_files_skipped() {
        let dir = TempDir::new().expect("expected operation to succeed");
        std::fs::write(dir.path().join(".hidden"), "").expect("expected operation to succeed");
        std::fs::write(dir.path().join(".gitignore"), "").expect("expected operation to succeed");
        std::fs::write(dir.path().join("visible.txt"), "").expect("expected operation to succeed");

        let entries = scan_directory(dir.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(names(&entries), vec!["visible.txt"]);
    }

    #[test]
    fn test_hidden_directories_skipped() {
        let dir = TempDir::new().expect("expected operation to succeed");
        std::fs::create_dir(dir.path().join(".git")).expect("expected operation to succeed");
        std::fs::create_dir(dir.path().join("src")).expect("expected operation to succeed");

        let entries = scan_directory(dir.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(names(&entries), vec!["src"]);
    }

    #[test]
    fn test_files_marked_not_dir() {
        let dir = TempDir::new().expect("expected operation to succeed");
        std::fs::write(dir.path().join("file.txt"), "hello")
            .expect("expected operation to succeed");

        let entries = scan_directory(dir.path());
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].is_dir);
    }

    #[test]
    fn test_directories_marked_as_dir() {
        let dir = TempDir::new().expect("expected operation to succeed");
        std::fs::create_dir(dir.path().join("subdir")).expect("expected operation to succeed");

        let entries = scan_directory(dir.path());
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_dir);
    }

    #[test]
    fn test_directories_sorted_before_files() {
        let dir = TempDir::new().expect("expected operation to succeed");
        // File sorts alphabetically before directory, but dirs should come first
        std::fs::write(dir.path().join("aaa.txt"), "").expect("expected operation to succeed");
        std::fs::create_dir(dir.path().join("zzz_dir")).expect("expected operation to succeed");

        let entries = scan_directory(dir.path());
        assert_eq!(entries.len(), 2);
        assert!(entries[0].is_dir, "first entry should be directory");
        assert!(!entries[1].is_dir, "second entry should be file");
    }

    #[test]
    fn test_alphabetical_case_insensitive_sort() {
        let dir = TempDir::new().expect("expected operation to succeed");
        std::fs::write(dir.path().join("Banana.txt"), "").expect("expected operation to succeed");
        std::fs::write(dir.path().join("apple.txt"), "").expect("expected operation to succeed");
        std::fs::write(dir.path().join("Cherry.txt"), "").expect("expected operation to succeed");

        let entries = scan_directory(dir.path());
        assert_eq!(
            names(&entries),
            vec!["apple.txt", "Banana.txt", "Cherry.txt"]
        );
    }

    #[test]
    fn test_nonexistent_directory_returns_empty() {
        let entries = scan_directory(Path::new("/nonexistent/path/that/does/not/exist"));
        assert!(entries.is_empty());
    }

    #[test]
    fn test_mixed_entries_sorted_correctly() {
        let dir = TempDir::new().expect("expected operation to succeed");
        std::fs::write(dir.path().join("readme.md"), "").expect("expected operation to succeed");
        std::fs::create_dir(dir.path().join("src")).expect("expected operation to succeed");
        std::fs::write(dir.path().join("Cargo.toml"), "").expect("expected operation to succeed");
        std::fs::create_dir(dir.path().join("docs")).expect("expected operation to succeed");

        let entries = scan_directory(dir.path());
        assert_eq!(
            names(&entries),
            vec!["docs", "src", "Cargo.toml", "readme.md"]
        );
    }

    #[test]
    fn test_all_hidden_entries_produces_empty() {
        let dir = TempDir::new().expect("expected operation to succeed");
        std::fs::write(dir.path().join(".gitignore"), "").expect("expected operation to succeed");
        std::fs::create_dir(dir.path().join(".git")).expect("expected operation to succeed");
        std::fs::write(dir.path().join(".env"), "").expect("expected operation to succeed");

        let entries = scan_directory(dir.path());
        assert!(entries.is_empty());
    }

    #[test]
    fn test_is_dir_empty_ignores_hidden_entries_and_detects_visible_entries() {
        let dir = TempDir::new().expect("expected operation to succeed");

        assert!(is_dir_empty(dir.path()));
        std::fs::write(dir.path().join(".hidden"), "").expect("expected operation to succeed");
        assert!(is_dir_empty(dir.path()), "hidden files should not count");
        std::fs::write(dir.path().join("visible.txt"), "").expect("expected operation to succeed");
        assert!(!is_dir_empty(dir.path()), "visible files should count");
        assert!(!is_dir_empty(&dir.path().join("missing")));
    }

    #[test]
    fn test_paths_are_absolute() {
        let dir = TempDir::new().expect("expected operation to succeed");
        std::fs::write(dir.path().join("file.txt"), "").expect("expected operation to succeed");

        let entries = scan_directory(dir.path());
        assert!(entries[0].path.is_absolute());
    }

    #[cfg(unix)]
    #[test]
    fn test_broken_symlinks_skipped() {
        let dir = TempDir::new().expect("expected operation to succeed");
        std::fs::write(dir.path().join("real.txt"), "").expect("expected operation to succeed");
        std::os::unix::fs::symlink("/nonexistent/target", dir.path().join("broken"))
            .expect("expected operation to succeed");

        let entries = scan_directory(dir.path());
        let result_names = names(&entries);
        assert_eq!(result_names, vec!["real.txt"]);
    }

    #[cfg(unix)]
    #[test]
    fn test_valid_symlinks_included() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let real = dir.path().join("real.txt");
        std::fs::write(&real, "content").expect("expected operation to succeed");
        std::os::unix::fs::symlink(&real, dir.path().join("link.txt"))
            .expect("expected operation to succeed");

        let entries = scan_directory(dir.path());
        assert_eq!(entries.len(), 2);
        let result_names = names(&entries);
        assert!(result_names.contains(&"real.txt".to_string()));
        assert!(result_names.contains(&"link.txt".to_string()));
    }

    #[test]
    fn test_bounded_scan_keeps_sorted_top_entries_and_marks_truncated() {
        let dir = TempDir::new().expect("expected operation to succeed");
        for name in ["zeta.txt", "alpha.txt", "docs", "src", "notes.md"] {
            let path = dir.path().join(name);
            if name.contains('.') {
                std::fs::write(path, "").expect("expected operation to succeed");
            } else {
                std::fs::create_dir(path).expect("expected operation to succeed");
            }
        }

        let scan = scan_directory_bounded(dir.path(), 3, 1000, None);
        assert!(scan.truncated);
        assert!(!scan.cancelled);
        assert_eq!(names(&scan.entries), vec!["docs", "src", "alpha.txt"]);
    }

    #[test]
    fn test_bounded_scan_checks_only_directory_lookahead_cap() {
        let dir = TempDir::new().expect("expected operation to succeed");
        std::fs::create_dir(dir.path().join("alpha")).expect("expected operation to succeed");
        std::fs::create_dir(dir.path().join("beta")).expect("expected operation to succeed");
        std::fs::write(dir.path().join("file.txt"), "").expect("expected operation to succeed");

        let scan = scan_directory_bounded(dir.path(), 10, 1, None);
        let checked_dirs = scan
            .entries
            .iter()
            .filter(|entry| entry.is_dir && entry.is_empty.is_some())
            .count();

        assert_eq!(checked_dirs, 1);
        assert!(
            scan.entries
                .iter()
                .filter(|entry| !entry.is_dir)
                .all(|entry| entry.is_empty.is_none()),
            "file entries should never consume directory lookahead"
        );
    }

    #[test]
    fn test_bounded_scan_honors_cancel_token() {
        let dir = TempDir::new().expect("expected operation to succeed");
        std::fs::write(dir.path().join("visible.txt"), "").expect("expected operation to succeed");
        let cancel = AtomicBool::new(true);

        let scan = scan_directory_bounded(dir.path(), 10, 1000, Some(&cancel));
        assert!(scan.cancelled);
        assert!(scan.entries.is_empty());
    }
}
