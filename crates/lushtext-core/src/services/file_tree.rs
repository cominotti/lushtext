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
            let is_dir = path.is_dir();
            Some((name, path, is_dir))
        })
        .collect();

    entries.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
    });

    entries.into_iter().map(|(_, p, d)| (p, d)).collect()
}
