// SPDX-License-Identifier: GPL-3.0-or-later

//! File tree scanning: read directory contents sorted for sidebar display.
//!
//! Pure I/O service with no GTK dependencies. Returns standard Rust types
//! that the UI layer converts into GObject models.

use std::path::{Path, PathBuf};

/// Scan a directory and return sorted entries (directories first, then alphabetical).
/// Skips hidden files (starting with `.`).
pub fn scan_directory(dir_path: &Path) -> Vec<(PathBuf, bool)> {
    let read_dir = match std::fs::read_dir(dir_path) {
        Ok(rd) => rd,
        Err(e) => {
            tracing::warn!("Cannot read {}: {}", dir_path.display(), e);
            return Vec::new();
        }
    };

    let mut entries: Vec<(String, PathBuf, bool)> = read_dir
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                return None;
            }
            let path = entry.path();
            // Use std::fs::metadata (follows symlinks via stat(2)) to skip
            // broken symlinks. DirEntry::metadata() uses fstatat(AT_SYMLINK_NOFOLLOW)
            // on Unix, which returns the symlink's own metadata even if the
            // target is missing.
            let meta = match std::fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => return None, // broken symlink or permission denied
            };
            Some((name, path, meta.is_dir()))
        })
        .collect();

    entries.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
    });

    entries.into_iter().map(|(_, p, d)| (p, d)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: extract file names from scan results.
    fn names(entries: &[(PathBuf, bool)]) -> Vec<String> {
        entries
            .iter()
            .map(|(p, _)| p.file_name().unwrap().to_string_lossy().to_string())
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
}
