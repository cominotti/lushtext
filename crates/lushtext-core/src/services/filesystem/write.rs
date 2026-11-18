// SPDX-License-Identifier: GPL-3.0-or-later

//! Durable write operations exposed through the filesystem boundary.
//!
//! This module preserves the existing crash-durable write contract while keeping
//! caller-facing entry points inside `services::filesystem`.

use std::io::Write;
use std::path::Path;

use crate::services::durable_write;

use super::{sys, types::WriteLabel};

pub(crate) use durable_write::parent_or_current;
/// The syncs' wall time in test builds, for the Replace All M9 measurement.
#[cfg(test)]
pub(crate) use durable_write::sync_timing;
pub(crate) use durable_write::temp_name_launch_nonce;
#[cfg(test)]
pub(crate) use durable_write::unique_temp_path;
pub use durable_write::{DurableWriteError, TargetWriteGuard, WriteTargetIdentity};

/// Resolve the stable target identity used by coordinated writes.
///
/// # Errors
///
/// Returns an error when neither the target nor its parent can be resolved.
pub fn resolve_target_identity(path: &Path) -> std::io::Result<WriteTargetIdentity> {
    durable_write::resolve_write_target_identity(path)
}

/// Atomically replace a path with bytes and preserve destination identity metadata.
///
/// # Errors
///
/// Returns a classified durable write error if any write, metadata, rename, or
/// parent-directory sync step fails.
pub fn atomic_replace(
    path: &Path,
    label: WriteLabel,
    bytes: &[u8],
) -> Result<(), DurableWriteError> {
    durable_write::atomic_write_bytes_classified(path, label.as_str(), bytes)
}

/// Atomically replace a file in a user workspace, then sweep its own crash leftovers.
///
/// Identical to [`atomic_replace`], plus one bounded, non-recursive pass over
/// `path`'s directory after the write succeeded that removes stale temp files
/// left by an earlier interrupted write of this same file name. Pass the
/// resolved target path the write used. The sweep never reports failure: a
/// leftover it cannot remove today is kept for a later save.
///
/// # Errors
///
/// Returns the same classified durable write error as [`atomic_replace`]; the
/// sweep runs only when the write succeeded.
pub fn atomic_replace_workspace_file(
    path: &Path,
    label: WriteLabel,
    bytes: &[u8],
) -> Result<(), DurableWriteError> {
    atomic_replace(path, label, bytes)?;
    let report = super::leftovers::sweep_target_leftovers(path);
    if report.removed > 0 {
        tracing::debug!(
            "Removed {} stale durable-write leftover(s) beside {}",
            report.removed,
            path.display()
        );
    }
    Ok(())
}

/// Stream a durable atomic replacement into `path`.
///
/// # Errors
///
/// Returns a classified durable write error if serialization or any durability
/// step fails.
pub fn atomic_replace_stream<F>(
    path: &Path,
    label: WriteLabel,
    write_content: F,
) -> Result<(), DurableWriteError>
where
    F: FnOnce(&mut dyn Write) -> std::io::Result<()>,
{
    durable_write::atomic_write_stream_classified(path, label.as_str(), write_content)
}

/// Stream a durable replacement of a user-workspace file, then sweep its own
/// crash leftovers.
///
/// The streaming counterpart of [`atomic_replace_workspace_file`]: `path` must
/// already be the resolved target identity ([`resolve_target_identity`]), so a
/// symlink keeps writing its target, and the sweep runs only after the write
/// succeeded. A producer error is a `BeforeRename` failure: nothing is renamed
/// and the temp file is removed.
///
/// # Errors
///
/// Returns the classified durable write error of [`atomic_replace_stream`].
pub fn atomic_replace_workspace_file_stream<F>(
    path: &Path,
    label: WriteLabel,
    write_content: F,
) -> Result<(), DurableWriteError>
where
    F: FnOnce(&mut dyn Write) -> std::io::Result<()>,
{
    atomic_replace_stream(path, label, write_content)?;
    let report = super::leftovers::sweep_target_leftovers(path);
    if report.removed > 0 {
        tracing::debug!(
            "Removed {} stale durable-write leftover(s) beside {}",
            report.removed,
            path.display()
        );
    }
    Ok(())
}

/// Create a directory tree and sync newly-created directory entries.
///
/// # Errors
///
/// Returns an error when creation or sync fails.
pub fn create_dir_all_durable(path: &Path) -> std::io::Result<()> {
    durable_write::create_dir_all_durable(path)
}

/// Create one directory and sync its parent directory.
///
/// # Errors
///
/// Returns an error when creation or sync fails.
pub fn create_dir_durable(path: &Path) -> std::io::Result<()> {
    durable_write::create_dir_durable(path)
}

/// Rename a path and sync the affected parent directories.
///
/// # Errors
///
/// Returns an error when the rename or directory sync fails.
pub fn rename_durable(from: &Path, to: &Path) -> std::io::Result<()> {
    durable_write::rename_durable(from, to)
}

/// Rename durably, refusing atomically when the destination already exists.
///
/// Use this instead of [`rename_durable`] wherever replacing the destination
/// would destroy user data. See `durable_write::rename_durable_no_replace` for
/// why an existence check plus a rename is not equivalent.
///
/// # Errors
///
/// [`DurableWriteError::BeforeRename`] when nothing moved, carrying
/// [`std::io::ErrorKind::AlreadyExists`] when the destination exists and
/// [`std::io::ErrorKind::Unsupported`] when the platform lacks the flag;
/// [`DurableWriteError::AfterRename`] when the rename took effect but a
/// directory sync failed (whatever that error's kind).
pub fn rename_durable_no_replace(from: &Path, to: &Path) -> Result<(), DurableWriteError> {
    durable_write::rename_durable_no_replace(from, to)
}

/// [`rename_durable_no_replace`], falling back where the platform lacks the
/// flag to an existence check plus [`rename_durable`].
///
/// The fallback is two syscalls, so it is only best-effort against another
/// process creating the destination in between; callers serialize LushText's
/// own writers with [`TargetWriteGuard`]. It runs **only** when the rename step
/// itself reported `Unsupported` (a [`DurableWriteError::BeforeRename`]): a
/// directory `fsync` that fails with `EOPNOTSUPP` after a completed rename is
/// `Unsupported` too, and reading it as a missing flag made the fallback find
/// the just-moved file and report a name collision for a rename that happened.
///
/// # Errors
///
/// [`DurableWriteError::BeforeRename`] with [`std::io::ErrorKind::AlreadyExists`]
/// when the destination exists, by either route, or with the rename's own
/// error; [`DurableWriteError::AfterRename`] when either route's rename took
/// effect and a directory sync failed. Callers treat the latter as a completed
/// rename whose durability is unconfirmed.
pub fn rename_durable_no_replace_or_checked(
    from: &Path,
    to: &Path,
) -> Result<(), DurableWriteError> {
    match rename_durable_no_replace(from, to) {
        Err(DurableWriteError::BeforeRename(error))
            if error.kind() == std::io::ErrorKind::Unsupported =>
        {
            if super::metadata::exists(to) {
                return Err(DurableWriteError::BeforeRename(std::io::Error::from(
                    std::io::ErrorKind::AlreadyExists,
                )));
            }
            durable_write::rename_durable_classified(from, to)
        }
        other => other,
    }
}

/// Whether a rename failed only because source and target are on different filesystems.
///
/// This is the one rename failure a copy fallback may answer. Every other
/// error — including a parent-directory sync failure after the rename already
/// took effect, when the source is gone — must propagate so the caller can
/// retry from what is actually on disk.
#[must_use]
pub fn is_cross_device(error: &std::io::Error) -> bool {
    sys::is_cross_device(error)
}

/// Copy a file durably, keeping the source; see [`move_durable`] for the
/// variant that removes it.
///
/// # Errors
///
/// Returns an error when the source cannot be read or the destination cannot be
/// written durably with the source's metadata.
pub fn copy_durable(from: &Path, to: &Path, label: WriteLabel) -> std::io::Result<()> {
    durable_write::copy_durable(from, to, label.as_str())
}

/// Move a file by a durable copy, removing the source only after the
/// destination is durable: the cross-filesystem fallback for
/// [`rename_durable`].
///
/// # Errors
///
/// Returns an error when the copy fails or the source cannot be removed and
/// its directory synced.
pub fn move_durable(from: &Path, to: &Path, label: WriteLabel) -> std::io::Result<()> {
    durable_write::move_durable(from, to, label.as_str())
}

/// Create an empty file only when the destination is absent, then sync its parent.
///
/// # Errors
///
/// Returns an error when the file already exists, cannot be created, cannot be
/// synced, or the parent directory cannot be synced.
pub fn create_new_empty_file_durable(path: &Path) -> std::io::Result<()> {
    sys::create_new_empty_file(path)?;
    sync_parent_dir(path)
}

/// Inject one post-rename parent-sync failure of `kind` through the filesystem boundary.
#[cfg(test)]
pub(crate) fn fail_next_parent_sync_for_test(kind: std::io::ErrorKind) {
    durable_write::fail_next_parent_sync_for_test(kind);
}

/// Inject one cross-device (`EXDEV`) durable-rename failure through the filesystem boundary.
#[cfg(test)]
pub(in crate::services) fn fail_next_rename_cross_device_for_test() {
    durable_write::fail_next_rename_cross_device_for_test();
}

/// Sync the parent directory of a path after a namespace mutation.
///
/// # Errors
///
/// Returns an error when the parent directory cannot be opened or synced.
pub fn sync_parent_dir(path: &Path) -> std::io::Result<()> {
    durable_write::sync_parent_dir(path)
}

/// An append-only log for smoke-only measurement evidence (the
/// `responsiveness-probes` feature's JSONL stream).
///
/// Each [`EvidenceAppendLog::append`] is one unbuffered `write` of a whole
/// record, so a process killed with `SIGKILL` loses at most the record in
/// flight. It is **not** a durable persistence path: nothing is synced, and no
/// application data may use it. It exists so measurement code writes through
/// the filesystem boundary like everything else.
#[cfg(any(test, feature = "responsiveness-probes"))]
#[derive(Debug)]
pub struct EvidenceAppendLog {
    file: sys::File,
}

#[cfg(any(test, feature = "responsiveness-probes"))]
impl EvidenceAppendLog {
    /// Open `path` for appending, creating it owner-only when absent.
    ///
    /// # Errors
    ///
    /// Returns the open error.
    pub fn open(path: &Path) -> std::io::Result<Self> {
        Ok(Self {
            file: sys::open_append_create(path)?,
        })
    }

    /// Append `record` in one write.
    ///
    /// # Errors
    ///
    /// Returns the write error.
    pub fn append(&mut self, record: &[u8]) -> std::io::Result<()> {
        self.file.write_all(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::fixture;
    use std::io::ErrorKind;

    fn renamed_pair() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let from = dir.path().join("from.txt");
        let to = dir.path().join("to.txt");
        fixture::write_text(&from, "body");
        (dir, from, to)
    }

    /// `rust-1-100-adoption` A8: an `EOPNOTSUPP`/`ENOSYS` directory `fsync`
    /// after a completed rename is `Unsupported` too; it must not be read as
    /// "the kernel lacks `RENAME_NOREPLACE`", whose fallback then finds the
    /// just-moved file at the destination and reports a name collision.
    #[test]
    fn an_unsupported_sync_after_the_rename_is_not_a_name_collision() {
        let (_dir, from, to) = renamed_pair();
        fail_next_parent_sync_for_test(ErrorKind::Unsupported);

        let error = rename_durable_no_replace_or_checked(&from, &to)
            .expect_err("the directory sync failed");

        assert!(
            matches!(&error, DurableWriteError::AfterRename(e) if e.kind() == ErrorKind::Unsupported),
            "{error:?}"
        );
        assert!(!fixture::exists(&from), "the rename took effect");
        fixture::assert_text(&to, "body");
    }

    #[test]
    fn any_sync_failure_after_the_rename_reports_the_moved_file() {
        let (_dir, from, to) = renamed_pair();
        fail_next_parent_sync_for_test(ErrorKind::Other);

        let error = rename_durable_no_replace_or_checked(&from, &to)
            .expect_err("the directory sync failed");

        assert!(
            matches!(&error, DurableWriteError::AfterRename(e) if e.kind() == ErrorKind::Other),
            "{error:?}"
        );
        assert!(!fixture::exists(&from), "the rename took effect");
        fixture::assert_text(&to, "body");
    }

    #[test]
    fn the_fallback_rename_classifies_its_sync_failure() {
        let (_dir, from, to) = renamed_pair();
        durable_write::next_no_replace_unsupported_for_test();
        fail_next_parent_sync_for_test(ErrorKind::Unsupported);

        let error = rename_durable_no_replace_or_checked(&from, &to)
            .expect_err("the fallback's directory sync failed");

        assert!(
            matches!(&error, DurableWriteError::AfterRename(e) if e.kind() == ErrorKind::Unsupported),
            "{error:?}"
        );
        assert!(!fixture::exists(&from), "the fallback rename took effect");
        fixture::assert_text(&to, "body");
    }

    /// Regression: both routes still refuse an existing destination and
    /// leave it untouched.
    #[test]
    fn an_existing_destination_is_refused_by_either_route() {
        for unsupported in [false, true] {
            let (_dir, from, to) = renamed_pair();
            fixture::write_text(&to, "kept");
            if unsupported {
                durable_write::next_no_replace_unsupported_for_test();
            }

            let error = rename_durable_no_replace_or_checked(&from, &to)
                .expect_err("the destination exists");

            assert!(
                matches!(&error, DurableWriteError::BeforeRename(e) if e.kind() == ErrorKind::AlreadyExists),
                "{error:?}"
            );
            fixture::assert_text(&from, "body");
            fixture::assert_text(&to, "kept");
        }
    }
}
