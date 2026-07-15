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
    DirectoryEntryInfo, DirectoryPage, DirectoryScanPolicy, FileFacts, FileIdentity, FileKind,
    FileSnapshot, MutationOutcome, PathStatus, WriteLabel,
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
    fn directory_page_retains_only_the_next_lexicographic_rows() {
        let dir = TempDir::new().expect("temp dir");
        for name in ["delta", "alpha", "echo", "charlie", "bravo"] {
            fixture::write_text(&dir.path().join(name), "");
        }

        let page = tree::scan_directory_page_after(
            dir.path(),
            Some("alpha"),
            DirectoryScanPolicy {
                max_entries: 2,
                include_hidden: false,
            },
        )
        .expect("scan directory page");

        assert_eq!(
            page.entries
                .iter()
                .map(|entry| entry.file_name.as_str())
                .collect::<Vec<_>>(),
            vec!["bravo", "charlie"]
        );
        assert!(page.has_more);
        assert!(!page.wrapped);

        let wrapped = tree::scan_directory_page(
            dir.path(),
            Some("zulu"),
            true,
            DirectoryScanPolicy {
                max_entries: 2,
                include_hidden: false,
            },
        )
        .expect("scan wrapped directory page");
        assert!(wrapped.wrapped);
        assert_eq!(wrapped.entries[0].file_name, "alpha");
        assert!(wrapped.has_more);
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

    #[test]
    fn create_new_empty_file_durable_creates_file_and_refuses_existing_path() {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("created.txt");

        write::create_new_empty_file_durable(&path).expect("create empty file");

        assert_eq!(
            metadata::path_status(&path).expect("status"),
            PathStatus::File
        );
        assert_eq!(metadata::file_facts(&path).expect("facts").byte_size, 0);
        let error =
            write::create_new_empty_file_durable(&path).expect_err("existing file should fail");
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn write_labels_expose_stable_durable_operation_names() {
        assert_eq!(WriteLabel::SAVE.as_str(), "save");
        assert_eq!(WriteLabel::JSON.as_str(), "json");
        assert_eq!(WriteLabel::DRAFT.as_str(), "draft");
        assert_eq!(WriteLabel::REPLACE.as_str(), "replace");
        assert_eq!(
            WriteLabel::RECOVERY_QUARANTINE.as_str(),
            "recovery-quarantine"
        );
        assert_eq!(
            WriteLabel::LOCAL_HISTORY_COPY.as_str(),
            "local-history-copy"
        );
        assert_eq!(WriteLabel::from("custom-op").as_str(), "custom-op");
    }

    #[test]
    fn resolve_target_identity_propagates_non_missing_parent_errors() {
        let dir = TempDir::new().expect("temp dir");
        let file_parent = dir.path().join("file-parent.txt");
        fixture::write_text(&file_parent, "not a directory\n");
        let impossible_child = file_parent.join("child.txt");

        let error = write::resolve_target_identity(&impossible_child)
            .expect_err("file parent should not resolve as a writable directory");

        assert_ne!(error.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn create_and_remove_directory_boundaries_report_changed_absent_and_wrong_kind() {
        let dir = TempDir::new().expect("temp dir");
        let empty_dir = dir.path().join("empty");

        mutate::create_dir(&empty_dir).expect("create directory");
        assert_eq!(
            metadata::path_status(&empty_dir).expect("directory status"),
            PathStatus::Directory
        );
        assert_eq!(
            mutate::remove_dir_if_exists(&empty_dir).expect("remove empty directory"),
            MutationOutcome::Changed
        );
        assert_eq!(
            mutate::remove_dir_if_exists(&empty_dir).expect("remove missing directory"),
            MutationOutcome::AlreadyAbsent
        );

        let file_path = dir.path().join("not-a-dir.txt");
        fixture::write_text(&file_path, "still here\n");
        assert!(mutate::remove_dir_if_exists(&file_path).is_err());
        fixture::assert_text(&file_path, "still here\n");
    }

    #[test]
    fn write_at_start_overwrites_prefix_without_truncating_file() {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("prefix.txt");
        fixture::write_text(&path, "abcdef");

        fixture::write_at_start(&path, b"XY");

        fixture::assert_text(&path, "XYcdef");
    }

    #[test]
    fn remove_dir_all_if_exists_removes_trees_and_preserves_wrong_kind() {
        let dir = TempDir::new().expect("temp dir");
        let tree_path = dir.path().join("tree");
        fixture::create_dir_all(&tree_path.join("nested"));
        fixture::write_text(&tree_path.join("nested/file.txt"), "child\n");

        assert_eq!(
            mutate::remove_dir_all_if_exists(&tree_path).expect("remove tree"),
            MutationOutcome::Changed
        );
        assert_eq!(
            mutate::remove_dir_all_if_exists(&tree_path).expect("remove missing tree"),
            MutationOutcome::AlreadyAbsent
        );

        let file_path = dir.path().join("not-a-tree.txt");
        fixture::write_text(&file_path, "still here\n");
        assert!(mutate::remove_dir_all_if_exists(&file_path).is_err());
        fixture::assert_text(&file_path, "still here\n");
    }

    #[test]
    fn remove_file_if_exists_deletes_present_files_and_preserves_wrong_kind() {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("delete-me.txt");
        fixture::write_text(&path, "delete me\n");

        assert_eq!(
            mutate::remove_file_if_exists(&path).expect("remove file"),
            MutationOutcome::Changed
        );
        assert_eq!(
            mutate::remove_file_if_exists(&path).expect("remove missing file"),
            MutationOutcome::AlreadyAbsent
        );

        let nested = dir.path().join("not-a-file");
        fixture::create_dir_all(&nested);
        assert!(mutate::remove_file_if_exists(&nested).is_err());
        assert_eq!(
            metadata::path_status(&nested).expect("directory still exists"),
            PathStatus::Directory
        );
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

    #[cfg(unix)]
    #[test]
    fn metadata_reports_symlink_identity_mode_and_descriptor_size() {
        let dir = TempDir::new().expect("temp dir");
        let first = dir.path().join("first.txt");
        let second = dir.path().join("second.txt");
        let link = dir.path().join("first-link.txt");
        fixture::write_text(&first, "first body\n");
        fixture::write_text(&second, "second body\n");
        fixture::symlink(&first, &link);
        fixture::set_mode(&first, 0o640);

        assert!(!metadata::is_symlink(&first).expect("regular file symlink status"));
        assert!(metadata::is_symlink(&link).expect("link symlink status"));
        assert_ne!(
            metadata::inode(&first).expect("first inode"),
            metadata::inode(&second).expect("second inode")
        );
        assert_ne!(metadata::inode(&first).expect("first inode"), 0);
        assert_eq!(metadata::mode(&first).expect("file mode") & 0o777, 0o640);
        assert_eq!(
            metadata::file_facts(&first).expect("file facts").byte_size,
            "first body\n".len() as u64
        );
    }

    #[test]
    fn tree_scan_preserves_kinds_limit_and_early_stop() {
        let dir = TempDir::new().expect("temp dir");
        fixture::write_text(&dir.path().join("a.txt"), "a\n");
        fixture::create_dir(&dir.path().join("b-dir"));
        fixture::write_text(&dir.path().join("c.txt"), "c\n");

        let all_entries = tree::scan_directory(
            dir.path(),
            DirectoryScanPolicy {
                max_entries: usize::MAX,
                include_hidden: true,
            },
        )
        .expect("scan all entries");
        assert_eq!(
            all_entries
                .iter()
                .map(|entry| (entry.file_name.as_str(), entry.kind))
                .collect::<Vec<_>>(),
            vec![
                ("a.txt", FileKind::File),
                ("b-dir", FileKind::Directory),
                ("c.txt", FileKind::File),
            ]
        );

        let capped_entries = tree::scan_directory(
            dir.path(),
            DirectoryScanPolicy {
                max_entries: 2,
                include_hidden: true,
            },
        )
        .expect("scan capped entries");
        assert_eq!(capped_entries.len(), 2);

        let mut visited = Vec::new();
        tree::visit_directory(
            dir.path(),
            DirectoryScanPolicy {
                max_entries: usize::MAX,
                include_hidden: true,
            },
            |entry| {
                visited.push(entry.file_name);
                false
            },
        )
        .expect("visit directory");
        assert_eq!(visited.len(), 1);
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
    fn copy_file_durable_wrapper_moves_source_bytes_to_destination() {
        let dir = TempDir::new().expect("temp dir");
        let from = dir.path().join("from.txt");
        let to = dir.path().join("to.txt");
        fixture::write_text(&from, "copy me\n");

        write::copy_file_durable(&from, &to, WriteLabel::LOCAL_HISTORY_COPY).expect("copy file");

        assert!(!metadata::exists(&from));
        fixture::assert_text(&to, "copy me\n");
    }

    #[test]
    fn remove_file_if_exists_reports_absent_paths() {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path().join("missing.txt");

        let outcome = mutate::remove_file_if_exists(&path).expect("remove missing file");

        assert_eq!(outcome, MutationOutcome::AlreadyAbsent);
    }
}
