// SPDX-License-Identifier: GPL-3.0-or-later

//! Internal filesystem boundary for LushText services and tests.
//!
//! Production callers should use these small operation families instead of
//! importing `std::fs`, Unix extension traits, `libc`, or `rustix` directly.
//! The private backend keeps low-level descriptor and metadata details in one
//! place while call sites stay readable and application-oriented.

pub mod fixture;
pub mod metadata;
pub mod mutate;
pub mod read;
pub(in crate::services) mod sys;
pub mod tree;
pub mod types;
pub mod write;

pub use types::{
    DirectoryEntryInfo, DirectoryScanPolicy, FileFacts, FileKind, FileSnapshot, MutationOutcome,
    PathStatus, WriteLabel,
};

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn snapshot_returns_bytes_and_metadata_facts() {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("note.txt");
        fixture::write_text(&path, "hello\n");

        let snapshot = read::snapshot(&path).expect("snapshot");

        assert_eq!(snapshot.bytes, b"hello\n");
        assert_eq!(snapshot.facts.kind, FileKind::File);
        assert_eq!(snapshot.facts.byte_size, 6);
        assert_eq!(
            snapshot.facts.canonical_path,
            Some(metadata::canonical_path(&path).expect("canonical path"))
        );
    }

    #[test]
    fn tree_scan_keeps_hidden_files_out_by_default() {
        let dir = TempDir::new().expect("temp dir");
        fixture::write_text(&dir.path().join("visible.txt"), "");
        fixture::write_text(&dir.path().join(".hidden"), "");

        let entries = tree::scan_directory(dir.path(), DirectoryScanPolicy::visible_workspace())
            .expect("scan directory");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].file_name, "visible.txt");
    }

    #[test]
    fn path_status_reports_missing_file_directory_and_errors() {
        let dir = TempDir::new().expect("temp dir");
        let file_path = dir.path().join("note.txt");
        let child_of_file = file_path.join("child.txt");
        fixture::write_text(&file_path, "hello\n");

        assert_eq!(
            metadata::path_status(&file_path).expect("file status"),
            PathStatus::File
        );
        assert_eq!(
            metadata::path_status(dir.path()).expect("directory status"),
            PathStatus::Directory
        );
        assert_eq!(
            metadata::path_status(&dir.path().join("missing.txt")).expect("missing status"),
            PathStatus::Missing
        );
        assert!(metadata::path_status(&child_of_file).is_err());
    }

    #[test]
    fn exists_reports_present_paths_without_rich_facts() {
        let dir = TempDir::new().expect("temp dir");
        let file_path = dir.path().join("note.txt");
        fixture::write_text(&file_path, "hello\n");

        assert!(metadata::exists(&file_path));
        assert!(metadata::exists(dir.path()));
        assert!(!metadata::exists(&dir.path().join("missing.txt")));
    }

    #[cfg(unix)]
    #[test]
    fn tree_scan_reports_symlink_targets_through_boundary_kind() {
        let dir = TempDir::new().expect("temp dir");
        let target = dir.path().join("target.txt");
        let link = dir.path().join("link.txt");
        fixture::write_text(&target, "linked\n");
        fixture::symlink(&target, &link);

        let entries = tree::scan_directory(dir.path(), DirectoryScanPolicy::visible_workspace())
            .expect("scan directory");
        let link = entries
            .iter()
            .find(|entry| entry.file_name == "link.txt")
            .expect("symlink entry");

        assert_eq!(link.kind, FileKind::File);
    }

    #[test]
    fn tree_scan_reports_missing_directories_as_errors() {
        let dir = TempDir::new().expect("temp dir");
        let result = tree::scan_directory(
            &dir.path().join("missing"),
            DirectoryScanPolicy::visible_workspace(),
        );

        assert!(result.is_err());
    }

    #[test]
    fn atomic_replace_routes_through_write_boundary() {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("save.txt");
        fixture::write_text(&path, "old\n");

        write::atomic_replace(&path, WriteLabel::SAVE, b"new\n").expect("atomic replace");

        fixture::assert_text(&path, "new\n");
    }

    #[test]
    fn remove_file_if_exists_reports_absent_paths() {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("missing.txt");

        let outcome = mutate::remove_file_if_exists(&path).expect("remove missing file");

        assert_eq!(outcome, MutationOutcome::AlreadyAbsent);
    }
}
