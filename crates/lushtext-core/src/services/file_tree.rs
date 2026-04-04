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

/// Result of a bounded directory scan.
#[derive(Debug, Default)]
pub struct DirectoryScan {
    /// Sorted entries: directories first, then alphabetical (case-insensitive).
    pub entries: Vec<(PathBuf, bool)>,
    /// True if the directory had more entries than `max_entries`.
    pub truncated: bool,
    /// True if the cancellation token was set during scanning.
    pub cancelled: bool,
}

#[derive(Debug, Eq, PartialEq)]
struct SortedEntry {
    path: PathBuf,
    is_dir: bool,
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
pub fn scan_directory(dir_path: &Path) -> Vec<(PathBuf, bool)> {
    scan_directory_bounded(dir_path, usize::MAX, None).entries
}

/// Scan a directory while bounding memory and allowing cooperative cancellation.
///
/// The result is still sorted directories-first and alphabetically, but for
/// very large folders only the best `max_entries` rows are retained in memory.
pub fn scan_directory_bounded(
    dir_path: &Path,
    max_entries: usize,
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
        let Some((path, is_dir)) = classify_entry(entry) else {
            continue;
        };

        heap.push(SortedEntry { path, is_dir });
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
fn classify_entry(entry: DirEntry) -> Option<(PathBuf, bool)> {
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

fn drain_sorted_entries(heap: BinaryHeap<SortedEntry>) -> Vec<(PathBuf, bool)> {
    heap.into_sorted_vec()
        .into_iter()
        .map(|entry| (entry.path, entry.is_dir))
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
    fn names(entries: &[(PathBuf, bool)]) -> Vec<String> {
        entries
            .iter()
            .map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn test_empty_directory() {
        let dir = TempDir::new().unwrap();
        let entries = scan_directory(dir.path());
        assert!(entries.is_empty());
    }

    #[test]
    fn test_hidden_files_skipped() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(".hidden"), "").unwrap();
        std::fs::write(dir.path().join(".gitignore"), "").unwrap();
        std::fs::write(dir.path().join("visible.txt"), "").unwrap();

        let entries = scan_directory(dir.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(names(&entries), vec!["visible.txt"]);
    }

    #[test]
    fn test_hidden_directories_skipped() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();

        let entries = scan_directory(dir.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(names(&entries), vec!["src"]);
    }

    #[test]
    fn test_files_marked_not_dir() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("file.txt"), "hello").unwrap();

        let entries = scan_directory(dir.path());
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].1);
    }

    #[test]
    fn test_directories_marked_as_dir() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();

        let entries = scan_directory(dir.path());
        assert_eq!(entries.len(), 1);
        assert!(entries[0].1);
    }

    #[test]
    fn test_directories_sorted_before_files() {
        let dir = TempDir::new().unwrap();
        // File sorts alphabetically before directory, but dirs should come first
        std::fs::write(dir.path().join("aaa.txt"), "").unwrap();
        std::fs::create_dir(dir.path().join("zzz_dir")).unwrap();

        let entries = scan_directory(dir.path());
        assert_eq!(entries.len(), 2);
        assert!(entries[0].1, "first entry should be directory");
        assert!(!entries[1].1, "second entry should be file");
    }

    #[test]
    fn test_alphabetical_case_insensitive_sort() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("Banana.txt"), "").unwrap();
        std::fs::write(dir.path().join("apple.txt"), "").unwrap();
        std::fs::write(dir.path().join("Cherry.txt"), "").unwrap();

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
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("readme.md"), "").unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        std::fs::create_dir(dir.path().join("docs")).unwrap();

        let entries = scan_directory(dir.path());
        assert_eq!(
            names(&entries),
            vec!["docs", "src", "Cargo.toml", "readme.md"]
        );
    }

    #[test]
    fn test_all_hidden_entries_produces_empty() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "").unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".env"), "").unwrap();

        let entries = scan_directory(dir.path());
        assert!(entries.is_empty());
    }

    #[test]
    fn test_paths_are_absolute() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("file.txt"), "").unwrap();

        let entries = scan_directory(dir.path());
        assert!(entries[0].0.is_absolute());
    }

    #[cfg(unix)]
    #[test]
    fn test_broken_symlinks_skipped() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("real.txt"), "").unwrap();
        std::os::unix::fs::symlink("/nonexistent/target", dir.path().join("broken")).unwrap();

        let entries = scan_directory(dir.path());
        let result_names = names(&entries);
        assert_eq!(result_names, vec!["real.txt"]);
    }

    #[cfg(unix)]
    #[test]
    fn test_valid_symlinks_included() {
        let dir = TempDir::new().unwrap();
        let real = dir.path().join("real.txt");
        std::fs::write(&real, "content").unwrap();
        std::os::unix::fs::symlink(&real, dir.path().join("link.txt")).unwrap();

        let entries = scan_directory(dir.path());
        assert_eq!(entries.len(), 2);
        let result_names = names(&entries);
        assert!(result_names.contains(&"real.txt".to_string()));
        assert!(result_names.contains(&"link.txt".to_string()));
    }

    #[test]
    fn test_bounded_scan_keeps_sorted_top_entries_and_marks_truncated() {
        let dir = TempDir::new().unwrap();
        for name in ["zeta.txt", "alpha.txt", "docs", "src", "notes.md"] {
            let path = dir.path().join(name);
            if name.contains('.') {
                std::fs::write(path, "").unwrap();
            } else {
                std::fs::create_dir(path).unwrap();
            }
        }

        let scan = scan_directory_bounded(dir.path(), 3, None);
        assert!(scan.truncated);
        assert!(!scan.cancelled);
        assert_eq!(names(&scan.entries), vec!["docs", "src", "alpha.txt"]);
    }

    #[test]
    fn test_bounded_scan_honors_cancel_token() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("visible.txt"), "").unwrap();
        let cancel = AtomicBool::new(true);

        let scan = scan_directory_bounded(dir.path(), 10, Some(&cancel));
        assert!(scan.cancelled);
        assert!(scan.entries.is_empty());
    }
}
