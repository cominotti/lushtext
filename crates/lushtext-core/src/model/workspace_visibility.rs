// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace entry visibility: the one rule deciding whether a directory entry
//! discovered inside a workspace folder is shown, indexed, and searched.
//!
//! The sidebar tree, its empty-folder probes, the command palette file index,
//! and workspace content search all consume [`WorkspaceEntryVisibility::admits`]
//! rather than re-deriving the rule, so the surfaces cannot drift apart. The
//! filesystem boundary's fixed app-data scans reuse the same predicate through
//! [`WorkspaceEntryVisibility::app_data`] and [`WorkspaceEntryVisibility::everything`],
//! so the dot rule is implemented exactly once.
//! Configured workspace folders are always visible; the rule governs only the
//! entries found beneath them.

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::sync::Arc;

/// Default always-excluded names shipped with the application.
pub const DEFAULT_EXCLUDED_NAMES: &[&str] = &[".git"];

/// Maximum excluded names reported through the read-only automation snapshot.
pub const SNAPSHOT_EXCLUDED_NAMES_LIMIT: usize = 64;

/// Why a candidate excluded name was rejected at edit time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExcludedNameRejection {
    /// The trimmed name was empty.
    Empty,
    /// The name contained a path separator.
    ContainsSeparator,
    /// The name was already on the list.
    Duplicate,
}

/// The two-axis visibility rule shared by every workspace surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceEntryVisibility {
    /// Whether entries whose basename begins with `.` are visible.
    show_hidden: bool,
    /// Basenames that are never visible, regardless of `show_hidden`.
    excluded_names: Arc<HashSet<OsString>>,
}

impl Default for WorkspaceEntryVisibility {
    /// Today's shipped behavior: dotfiles hidden, `.git` excluded.
    fn default() -> Self {
        Self::new(
            false,
            DEFAULT_EXCLUDED_NAMES.iter().map(|name| (*name).to_owned()),
        )
    }
}

impl WorkspaceEntryVisibility {
    /// Build a rule from the persisted mode and the raw persisted names.
    ///
    /// Names are normalized: trimmed, empties and names containing `/`
    /// dropped, duplicates collapsed.
    #[must_use]
    pub fn new<I, S>(show_hidden: bool, excluded_names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let excluded_names = excluded_names
            .into_iter()
            .filter_map(|name| normalize_excluded_name(name.as_ref()).ok())
            .map(OsString::from)
            .collect::<HashSet<_>>();
        Self {
            show_hidden,
            excluded_names: Arc::new(excluded_names),
        }
    }

    /// Fixed rule for scans of app-owned data directories.
    ///
    /// App-data scans never observe user preferences: dotfiles stay hidden and
    /// nothing is excluded, exactly as before the preference existed.
    #[must_use]
    pub fn app_data() -> Self {
        Self {
            show_hidden: false,
            excluded_names: Arc::new(HashSet::new()),
        }
    }

    /// Rule that admits every entry: the boundary's `include_hidden: true`
    /// app-data scans (format-upgrade inventory, local-history lineage,
    /// fixture listings) that must see hidden temp leftovers.
    #[must_use]
    pub fn everything() -> Self {
        Self {
            show_hidden: true,
            excluded_names: Arc::new(HashSet::new()),
        }
    }

    /// Whether dotfiles are visible under this rule.
    #[must_use]
    pub const fn show_hidden(&self) -> bool {
        self.show_hidden
    }

    /// Whether a directory entry with this basename is visible.
    ///
    /// Excluded names win over the hidden-files mode; dot-names follow the
    /// mode; everything else is visible. Exact, case-sensitive comparison.
    #[must_use]
    pub fn admits(&self, name: &OsStr) -> bool {
        if self.excluded_names.contains(name) {
            return false;
        }
        self.show_hidden || name.as_encoded_bytes().first() != Some(&b'.')
    }
}

/// Validate one candidate excluded name against the normalization rules.
///
/// # Errors
///
/// Returns the rejection reason for empty names or names containing `/`.
pub fn normalize_excluded_name(raw: &str) -> Result<String, ExcludedNameRejection> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ExcludedNameRejection::Empty);
    }
    if trimmed.contains('/') {
        return Err(ExcludedNameRejection::ContainsSeparator);
    }
    Ok(trimmed.to_owned())
}

/// Plan an addition to the persisted excluded-name array.
///
/// Returns the new array on success. Duplicates are rejected so the editor can
/// focus the existing row instead of adding one.
///
/// # Errors
///
/// Returns the rejection reason when the name is invalid or already present.
pub fn add_excluded_name(
    current: &[String],
    raw: &str,
) -> Result<Vec<String>, ExcludedNameRejection> {
    let name = normalize_excluded_name(raw)?;
    if current.iter().any(|existing| existing == &name) {
        return Err(ExcludedNameRejection::Duplicate);
    }
    let mut next = current.to_vec();
    next.push(name);
    Ok(next)
}

/// Plan a removal from the persisted excluded-name array.
#[must_use]
pub fn remove_excluded_name(current: &[String], name: &str) -> Vec<String> {
    current
        .iter()
        .filter(|existing| existing.as_str() != name)
        .cloned()
        .collect()
}

/// Bound the excluded-name count for the automation snapshot.
///
/// Returns the reported names and whether the list was truncated. Per-name
/// byte bounding is the snapshot projector's job, with its shared text cap.
#[must_use]
pub fn snapshot_excluded_names(names: &[String]) -> (Vec<String>, bool) {
    let truncated = names.len() > SNAPSHOT_EXCLUDED_NAMES_LIMIT;
    let reported = names
        .iter()
        .take(SNAPSHOT_EXCLUDED_NAMES_LIMIT)
        .cloned()
        .collect();
    (reported, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(name: &str) -> OsString {
        OsString::from(name)
    }

    #[test]
    fn default_hides_dotfiles_and_excludes_git() {
        let rule = WorkspaceEntryVisibility::default();
        assert!(!rule.show_hidden());
        assert!(!rule.admits(&os(".env")));
        assert!(!rule.admits(&os(".git")));
        assert!(rule.admits(&os("src")));
        assert!(
            !rule.admits(&os(".gitignore")),
            "dot-names stay hidden by default"
        );
    }

    #[test]
    fn show_hidden_reveals_dotfiles_but_not_excluded_names() {
        let rule = WorkspaceEntryVisibility::new(true, [".git"]);
        assert!(rule.admits(&os(".env")));
        assert!(rule.admits(&os(".github")));
        assert!(!rule.admits(&os(".git")));
    }

    #[test]
    fn excluded_names_hide_non_dot_entries_in_both_modes() {
        for show_hidden in [false, true] {
            let rule = WorkspaceEntryVisibility::new(show_hidden, ["node_modules"]);
            assert!(!rule.admits(&os("node_modules")));
            assert!(rule.admits(&os("src")));
        }
    }

    #[test]
    fn matching_is_exact_and_case_sensitive() {
        let rule = WorkspaceEntryVisibility::new(true, [".git"]);
        assert!(rule.admits(&os(".gitignore")));
        assert!(rule.admits(&os(".Git")));
        assert!(rule.admits(&os("git")));
    }

    #[test]
    fn app_data_matches_previous_behavior_and_everything_admits_all() {
        let app_data = WorkspaceEntryVisibility::app_data();
        assert!(!app_data.admits(&os(".hidden")));
        assert!(!app_data.admits(&os(".git-adjacent")));
        assert!(app_data.admits(&os("visible")));
        let everything = WorkspaceEntryVisibility::everything();
        assert!(everything.admits(&os(".hidden")));
        assert!(everything.admits(&os(".git")));
    }

    #[test]
    fn constructor_normalizes_names() {
        let rule = WorkspaceEntryVisibility::new(true, [" build ", "", "a/b", "build"]);
        assert!(!rule.admits(&os("build")), "trimmed and deduplicated");
        assert!(rule.admits(&os("a/b")), "names with separators are dropped");
        assert!(rule.admits(&os("")), "empty names are dropped");
    }

    #[test]
    fn add_excluded_name_rejects_invalid_and_duplicate() {
        let current = vec![".git".to_owned()];
        assert_eq!(
            add_excluded_name(&current, "  "),
            Err(ExcludedNameRejection::Empty)
        );
        assert_eq!(
            add_excluded_name(&current, "a/b"),
            Err(ExcludedNameRejection::ContainsSeparator)
        );
        assert_eq!(
            add_excluded_name(&current, " .git "),
            Err(ExcludedNameRejection::Duplicate)
        );
        assert_eq!(
            add_excluded_name(&current, " build "),
            Ok(vec![".git".to_owned(), "build".to_owned()])
        );
    }

    #[test]
    fn remove_excluded_name_filters_exact_match() {
        let current = vec![".git".to_owned(), "build".to_owned()];
        assert_eq!(
            remove_excluded_name(&current, ".git"),
            vec!["build".to_owned()]
        );
        assert_eq!(remove_excluded_name(&current, ".Git"), current);
    }

    #[test]
    fn snapshot_names_are_bounded() {
        let names = (0..70).map(|index| format!("n{index}")).collect::<Vec<_>>();
        let (reported, truncated) = snapshot_excluded_names(&names);
        assert_eq!(reported.len(), SNAPSHOT_EXCLUDED_NAMES_LIMIT);
        assert!(truncated);
        let (reported, truncated) = snapshot_excluded_names(&[".git".to_owned()]);
        assert!(!truncated);
        assert_eq!(reported, vec![".git".to_owned()]);
    }
}
