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
    WriteLabel,
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
