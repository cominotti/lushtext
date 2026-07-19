// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared value types returned by the filesystem boundary.
//!
//! These names describe LushText workflows rather than backend calls. Keeping
//! the data plain and GTK-free lets services and tests share the same boundary
//! without pulling widget types into the service layer.

use std::path::PathBuf;

/// Coarse filesystem kind used by callers that should not inspect raw metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileKind {
    /// Regular file.
    File,
    /// Directory.
    Directory,
    /// Symlink, socket, device, or another kind the caller should treat carefully.
    Other,
}

/// Cheap existence and kind status for callers that do not need rich metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathStatus {
    /// The path did not exist when inspected.
    Missing,
    /// The path exists and is a regular file.
    File,
    /// The path exists and is a directory.
    Directory,
    /// The existing target is neither a regular file nor a directory after following symlinks.
    Other,
}

impl PathStatus {
    /// Return whether the path existed when inspected.
    #[must_use]
    pub const fn is_present(self) -> bool {
        !matches!(self, Self::Missing)
    }

    /// Return whether the path existed as a directory.
    #[must_use]
    pub const fn is_directory(self) -> bool {
        matches!(self, Self::Directory)
    }
}

impl From<FileKind> for PathStatus {
    fn from(kind: FileKind) -> Self {
        match kind {
            FileKind::File => Self::File,
            FileKind::Directory => Self::Directory,
            FileKind::Other => Self::Other,
        }
    }
}

/// Metadata facts most LushText workflows need before reading or writing a file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileFacts {
    /// Original path requested by the caller.
    pub path: PathBuf,
    /// Canonical path when the target can be resolved.
    pub canonical_path: Option<PathBuf>,
    /// File kind derived from metadata.
    pub kind: FileKind,
    /// File length in bytes.
    pub byte_size: u64,
    /// Last modification time as seconds since the Unix epoch, if available.
    pub modified_at_secs: Option<u64>,
    /// Full modification timestamp used by freshness-sensitive workflows.
    pub modified_at_nanos: Option<u128>,
    /// Stable platform identity when the backend can provide one.
    pub identity: Option<FileIdentity>,
}

/// Stable identity for one concrete filesystem object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileIdentity {
    /// Filesystem device identifier.
    pub device: u64,
    /// Inode or equivalent object identifier within the device.
    pub inode: u64,
}

/// Bytes plus metadata captured from one file read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileSnapshot {
    /// Metadata facts captured near the read.
    pub facts: FileFacts,
    /// File bytes read from disk.
    pub bytes: Vec<u8>,
}

/// Directory entry shape exposed by tree-oriented scans.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryEntryInfo {
    /// Absolute or caller-relative child path.
    pub path: PathBuf,
    /// Display name for sorting and UI rows.
    pub file_name: String,
    /// Coarse file kind.
    pub kind: FileKind,
}

/// One lexicographically selected directory page with bounded retained rows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryPage {
    /// Entries ordered by filename and strictly after the requested cursor.
    pub entries: Vec<DirectoryEntryInfo>,
    /// Whether at least one additional matching entry followed this page.
    pub has_more: bool,
    /// Whether selection exhausted the cursor suffix and restarted at the beginning.
    pub wrapped: bool,
}

/// Bounded one-pass directory pagination evidence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DirectoryPageVisitMetrics {
    /// Raw backend entries that reached the boundary visitor.
    pub raw_entries_visited: usize,
    /// Filtered entries delivered to page callbacks.
    pub entries_delivered: usize,
    /// Non-empty pages delivered to the caller.
    pub pages_delivered: usize,
    /// Whether traversal reached the directory's natural terminal entry.
    pub reached_terminal: bool,
    /// Whether the raw or filtered entry ceiling stopped traversal.
    pub stopped_by_limit: bool,
    /// Whether the caller stopped after a delivered page.
    pub stopped_by_visitor: bool,
}

/// Readability-first policy for directory scans.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryScanPolicy {
    /// Maximum entries retained by the caller.
    pub max_entries: usize,
    /// Whether hidden dotfiles should be skipped.
    pub include_hidden: bool,
}

impl DirectoryScanPolicy {
    /// Policy for an unbounded visible workspace scan.
    #[must_use]
    pub const fn visible_workspace() -> Self {
        Self {
            max_entries: usize::MAX,
            include_hidden: false,
        }
    }
}

/// Human-readable label used in temp-file names and diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WriteLabel(&'static str);

impl WriteLabel {
    /// Editor save write label.
    pub const SAVE: Self = Self("save");
    /// JSON state write label.
    pub const JSON: Self = Self("json");
    /// Draft persistence write label.
    pub const DRAFT: Self = Self("draft");
    /// Replace All write label.
    pub const REPLACE: Self = Self("replace");
    /// Recovery quarantine copy label.
    ///
    /// Quarantine writes preserve broken app-owned metadata before any caller is
    /// allowed to replace it with a repaired or default file.
    pub const RECOVERY_QUARANTINE: Self = Self("recovery-quarantine");
    /// Local-history snapshot migration copy label.
    pub const LOCAL_HISTORY_COPY: Self = Self("local-history-copy");

    /// Return the stable label string used by durable write helpers.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl From<&'static str> for WriteLabel {
    fn from(value: &'static str) -> Self {
        Self(value)
    }
}

/// Small result summary for mutating filesystem commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationOutcome {
    /// The requested operation changed the filesystem.
    Changed,
    /// The target was already absent and no change was needed.
    AlreadyAbsent,
}
