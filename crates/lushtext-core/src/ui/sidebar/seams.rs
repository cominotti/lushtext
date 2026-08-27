// SPDX-License-Identifier: GPL-3.0-or-later

//! Seam value objects for the workspace tree workflow.
//!
//! Reified in the established Ticket/Facts/predicate shape: a `*Ticket` captures
//! the expectation at dispatch, a `*Facts` captures observed live state at
//! completion, and one predicate validates them together.

use std::path::PathBuf;

/// Identity of the row an inline file operation was issued for.
///
/// # Why this exists
///
/// The rename completion used to re-read the section's **live** `context_target`
/// cell instead of the target the operation was issued for. `context_target` is
/// replaced by any right-click, by a new-item bind, and cleared on row recycling,
/// so when it changed while the rename worker ran, two things went wrong at once:
/// the renamed row's watch mirror kept the old path — silently ending
/// external-change detection for that file, so the next save overwrote another
/// program's edits — and, if the cell pointed at a *different* row, the renamed
/// path was written onto that unrelated row's item and inserted into the
/// directory-row index, so a later right-click Delete targeted the wrong file.
///
/// Capturing the row at dispatch and validating it at completion makes both
/// unrepresentable: the ticket carries the path the operation is *for*, and
/// [`FileOperationFacts`] carries the path the captured row *still has*.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FileOperationTicket {
    /// Path the operation was issued against.
    path: PathBuf,
    /// Whether the target was a directory when the operation was issued.
    ///
    /// Captured rather than re-read, because the live cell can describe another
    /// row by the time the worker finishes, and directory renames take a
    /// different index-maintenance path from file renames.
    is_dir: bool,
    /// Whether this operation is committing a freshly created placeholder.
    is_new: bool,
}

impl FileOperationTicket {
    /// Capture one inline file operation's target at dispatch time.
    pub(super) fn new(path: PathBuf, is_dir: bool, is_new: bool) -> Self {
        Self {
            path,
            is_dir,
            is_new,
        }
    }

    /// Return the path this operation was issued against.
    pub(super) fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Return whether the captured target was a directory.
    pub(super) const fn is_dir(&self) -> bool {
        self.is_dir
    }

    /// Return whether this operation commits a new placeholder.
    pub(super) const fn is_new(&self) -> bool {
        self.is_new
    }

    /// Return whether the captured row still describes this operation's target.
    ///
    /// A `false` answer means the row was recycled, re-bound, or replaced while
    /// the worker ran. The filesystem operation has still happened — this
    /// predicate gates only the **row projection and index maintenance**, which
    /// must never be applied to a row that now describes a different file.
    pub(super) fn row_is_current(&self, facts: &FileOperationFacts) -> bool {
        facts.row_path.as_deref() == Some(self.path.as_path())
    }
}

/// Live state observed at one inline file operation's completion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FileOperationFacts {
    /// Path the captured row's item reports **now**, or `None` if the row or its
    /// item is gone.
    pub(super) row_path: Option<PathBuf>,
}

impl FileOperationFacts {
    /// Record the live row identity one completion must be validated against.
    pub(super) const fn new(row_path: Option<PathBuf>) -> Self {
        Self { row_path }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn ticket() -> FileOperationTicket {
        FileOperationTicket::new(PathBuf::from("/w/notes.md"), false, false)
    }

    #[test]
    fn row_is_current_when_the_captured_row_still_holds_the_issued_path() {
        assert!(
            ticket().row_is_current(&FileOperationFacts::new(Some(PathBuf::from("/w/notes.md"))))
        );
    }

    #[test]
    fn a_recycled_row_is_not_current() {
        // The regression this seam exists for: the row now describes another
        // file, so its projection and index entries must not be rewritten.
        assert!(
            !ticket().row_is_current(&FileOperationFacts::new(Some(PathBuf::from("/w/other.md"))))
        );
    }

    #[test]
    fn a_vanished_row_is_not_current() {
        assert!(!ticket().row_is_current(&FileOperationFacts::new(None)));
    }

    #[test]
    fn the_ticket_reports_the_captured_target_rather_than_live_state() {
        let directory = FileOperationTicket::new(PathBuf::from("/w/sub"), true, true);
        assert_eq!(directory.path(), Path::new("/w/sub"));
        assert!(directory.is_dir());
        assert!(directory.is_new());
        let file = ticket();
        assert!(!file.is_dir());
        assert!(!file.is_new());
    }
}
