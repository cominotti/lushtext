// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure decisions owned by the workspace tree workflow.
//!
//! This is the workflow's one pure policy module, at its canonical role home. It
//! imports no GTK-family crate, which is what keeps it inside the default
//! `cargo-mutants` `ui/**/policy.rs` scope — and this workflow's decisions are
//! exactly the ones that most need that coverage, because they **rename and
//! delete the user's own documents**.
//!
//! Policy constants are pinned to concrete literals in the units a reader would
//! sanity-check, and the tests assert against those literals rather than against
//! the constants they came from.

use std::path::{Path, PathBuf};

/// Attempts `create_unique` makes before giving up on a free name.
///
/// 1,000: far past any plausible number of `New File N` siblings a user would
/// accumulate, and small enough that the loop cannot become a visible stall on a
/// slow filesystem.
pub const MAX_UNIQUE_NAME_ATTEMPTS: u32 = 1_000;

/// What an inline rename commit should actually do.
///
/// Naming the decision keeps the three outcomes distinguishable at the call
/// site. Before this existed, "cancel" and "rename" were an `if` chain and the
/// **collision case did not exist at all**: the only validation was
/// empty-or-unchanged, and `rename(2)` silently replaced whatever the typed name
/// already referred to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenameIntent {
    /// The typed name is empty, unchanged, or not a plain sibling name: restore
    /// the row and do nothing.
    Cancel,
    /// The typed name is a new sibling name in the same directory.
    Rename { new_path: PathBuf, new_name: String },
}

/// Decide what an inline rename commit means, without touching the filesystem.
///
/// Whether the destination already exists is deliberately **not** decided here:
/// it is a live filesystem fact that must be checked inside the worker while the
/// write guard is held — and the rename itself uses `RENAME_NOREPLACE` so the
/// check and the rename are one kernel operation. A decision taken on the GTK
/// thread would be stale by the time the rename runs.
///
/// A typed name containing a path separator is **refused**, not silently
/// reinterpreted. `Path::with_file_name("sub/x")` would move the file into a
/// different directory, which is not what an inline rename in a tree row means:
/// the user typed into a cell that shows one name, and the visible affordance
/// promises a rename within that directory. `..` is refused for the same reason.
/// Refusing by cancelling restores the row, which is the same thing an empty name
/// does — the user sees their edit not take rather than a file appear somewhere
/// they did not look.
///
/// Case-folding collisions are **not** refused here: on a case-insensitive
/// filesystem `notes.md` -> `Notes.md` is a legitimate rename whose destination
/// "exists" only in the sense that it is the same file. The kernel's
/// `RENAME_NOREPLACE` answers that correctly for the platform actually in use,
/// which a pure function cannot.
#[must_use]
pub fn rename_intent(old_path: &Path, typed_name: &str) -> RenameIntent {
    let new_name = typed_name.trim();
    let old_name = old_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

    if new_name.is_empty() || new_name == old_name {
        return RenameIntent::Cancel;
    }
    if name_is_not_a_plain_sibling(new_name) {
        return RenameIntent::Cancel;
    }

    RenameIntent::Rename {
        new_path: old_path.with_file_name(new_name),
        new_name: new_name.to_string(),
    }
}

/// Return whether a typed name would leave the row's own directory.
fn name_is_not_a_plain_sibling(name: &str) -> bool {
    name == "."
        || name == ".."
        || name.contains('/')
        || std::path::MAIN_SEPARATOR != '/' && name.contains(std::path::MAIN_SEPARATOR)
}

/// Why a rename could not be performed, in the workflow's own vocabulary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceRenameRefusal {
    /// Something already exists at the typed name.
    ///
    /// The rename is refused rather than performed, because the platform rename
    /// silently replaces a regular destination and the replaced file's contents
    /// are unrecoverable.
    DestinationExists { name: String },
}

impl WorkspaceRenameRefusal {
    /// Return the user-facing explanation for this refusal.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::DestinationExists { name } => {
                format!("A file named '{name}' already exists in this folder")
            }
        }
    }
}

/// Build the candidate name for one `create_unique` attempt.
///
/// Attempt 1 uses the bare base name; later attempts append the attempt number,
/// which is what produces `New File`, `New File 2`, `New File 3`.
#[must_use]
pub fn unique_name_candidate(base: &str, attempt: u32) -> String {
    if attempt <= 1 {
        return base.to_string();
    }
    format!("{base} {attempt}")
}

/// Whether a directory operation on `changed` affects an open tab at `open`.
///
/// Prefix matching, not equality: renaming or deleting a directory must reach
/// every open tab beneath it.
#[must_use]
pub fn directory_operation_affects_open_path(changed: &Path, open: &Path) -> bool {
    open.starts_with(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_intent_cancels_an_empty_or_unchanged_name() {
        let path = Path::new("/w/notes.md");
        assert_eq!(rename_intent(path, ""), RenameIntent::Cancel);
        assert_eq!(rename_intent(path, "   "), RenameIntent::Cancel);
        assert_eq!(rename_intent(path, "notes.md"), RenameIntent::Cancel);
        // Trimming happens before the unchanged comparison.
        assert_eq!(rename_intent(path, "  notes.md  "), RenameIntent::Cancel);
    }

    #[test]
    fn rename_intent_keeps_the_new_name_in_the_same_directory() {
        assert_eq!(
            rename_intent(Path::new("/w/sub/notes.md"), " final.md "),
            RenameIntent::Rename {
                new_path: PathBuf::from("/w/sub/final.md"),
                new_name: "final.md".to_string(),
            }
        );
    }

    #[test]
    fn rename_intent_refuses_a_name_that_would_leave_the_directory() {
        let path = Path::new("/w/notes.md");
        // A separator would turn a rename into a move.
        assert_eq!(rename_intent(path, "sub/final.md"), RenameIntent::Cancel);
        assert_eq!(rename_intent(path, "/absolute.md"), RenameIntent::Cancel);
        assert_eq!(rename_intent(path, "../escape.md"), RenameIntent::Cancel);
        assert_eq!(rename_intent(path, ".."), RenameIntent::Cancel);
        assert_eq!(rename_intent(path, "."), RenameIntent::Cancel);
        // Trailing separators are refused too, not silently trimmed.
        assert_eq!(rename_intent(path, "final.md/"), RenameIntent::Cancel);
        // A plain sibling name with dots in it is still fine.
        assert_eq!(
            rename_intent(path, "final.tar.gz"),
            RenameIntent::Rename {
                new_path: PathBuf::from("/w/final.tar.gz"),
                new_name: "final.tar.gz".to_string(),
            }
        );
        // A leading dot is a hidden file, not an escape.
        assert_eq!(
            rename_intent(path, ".hidden"),
            RenameIntent::Rename {
                new_path: PathBuf::from("/w/.hidden"),
                new_name: ".hidden".to_string(),
            }
        );
    }

    #[test]
    fn rename_intent_allows_a_case_only_change() {
        // Deliberately not refused: on a case-insensitive filesystem this is a
        // real rename whose destination "exists" only as the same file, and the
        // kernel's RENAME_NOREPLACE answers that for the platform in use.
        assert_eq!(
            rename_intent(Path::new("/w/notes.md"), "Notes.md"),
            RenameIntent::Rename {
                new_path: PathBuf::from("/w/Notes.md"),
                new_name: "Notes.md".to_string(),
            }
        );
    }

    #[test]
    fn rename_intent_of_a_root_like_path_does_not_panic() {
        // `file_name()` is `None` for `/`, so the old name is empty and any typed
        // name is a change.
        assert_eq!(
            rename_intent(Path::new("/"), "anything"),
            RenameIntent::Rename {
                new_path: PathBuf::from("/anything"),
                new_name: "anything".to_string(),
            }
        );
        assert_eq!(rename_intent(Path::new("/"), ""), RenameIntent::Cancel);
    }

    #[test]
    fn destination_collision_names_the_file_the_user_typed() {
        let refusal = WorkspaceRenameRefusal::DestinationExists {
            name: "final.md".to_string(),
        };
        assert_eq!(
            refusal.message(),
            "A file named 'final.md' already exists in this folder"
        );
    }

    #[test]
    fn unique_name_candidates_produce_the_documented_sequence() {
        assert_eq!(unique_name_candidate("New File", 0), "New File");
        assert_eq!(unique_name_candidate("New File", 1), "New File");
        assert_eq!(unique_name_candidate("New File", 2), "New File 2");
        assert_eq!(unique_name_candidate("New File", 17), "New File 17");
        assert_eq!(unique_name_candidate("New Folder", 3), "New Folder 3");
    }

    #[test]
    fn unique_name_attempt_ceiling_is_pinned() {
        assert_eq!(MAX_UNIQUE_NAME_ATTEMPTS, 1_000);
    }

    #[test]
    fn directory_operations_match_open_tabs_by_prefix_not_equality() {
        let dir = Path::new("/w/sub");
        assert!(directory_operation_affects_open_path(dir, dir));
        assert!(directory_operation_affects_open_path(
            dir,
            Path::new("/w/sub/deep/notes.md")
        ));
        assert!(!directory_operation_affects_open_path(
            dir,
            Path::new("/w/other/notes.md")
        ));
        // A sibling whose name merely starts with the same characters must not
        // match: `starts_with` is component-wise, not byte-wise.
        assert!(!directory_operation_affects_open_path(
            dir,
            Path::new("/w/subtle/notes.md")
        ));
    }
}
