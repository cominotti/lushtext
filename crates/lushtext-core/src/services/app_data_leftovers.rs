// SPDX-License-Identifier: GPL-3.0-or-later

//! Startup sweep of durable-write crash leftovers in LushText's own app data.
//!
//! Every app-data directory LushText durably writes into is named here, once,
//! and swept non-recursively under one shared per-pass budget. Local-history
//! lineages are the one nested family: their directories are listed one level
//! below the local-history root, and that listing is charged to the same
//! budget. The removal decision itself lives in
//! `services::filesystem::leftovers`; this module only knows *where* to look.

use std::path::Path;

use crate::services::filesystem::leftovers::{
    LeftoverScope, LeftoverSweepBudget, LeftoverSweepReport, child_directories, sweep_directory,
};
use crate::services::{
    bookmark_service, document_note_service, draft_service, folder_note_service,
    local_history_service, recovery_metadata, search_backup,
};

/// Local-history lineage directories swept per startup pass.
///
/// Lineages are listed in directory order, so a data home with more lineages
/// than this sweeps a subset each launch; leftovers elsewhere simply wait.
const MAX_LINEAGES_PER_PASS: usize = 1_024;

/// Sweep stale durable-write leftovers from every app-data directory under `data_dir`.
///
/// Never recurses beyond the named directories and one level of local-history
/// lineages, and never propagates failure: an unreadable directory or a
/// leftover that cannot be removed is counted and left for a later launch.
///
/// **Threading:** Performs blocking filesystem calls. Call from a background
/// thread.
#[must_use]
pub fn sweep_startup_leftovers(data_dir: &Path) -> LeftoverSweepReport {
    let mut budget = LeftoverSweepBudget::startup();
    let mut report = LeftoverSweepReport::default();
    let history_root = local_history_service::local_history_dir(data_dir);

    for dir in [
        data_dir.to_path_buf(),
        draft_service::drafts_dir(data_dir),
        history_root.clone(),
        bookmark_service::bookmarks_dir(data_dir),
        document_note_service::document_notes_dir(data_dir),
        folder_note_service::folder_notes_dir(data_dir),
        data_dir.join(recovery_metadata::QUARANTINE_DIR),
        search_backup::journal_dir(data_dir),
    ] {
        sweep_directory(&dir, LeftoverScope::AppData, &mut budget, &mut report);
    }

    for lineage in child_directories(
        &history_root,
        MAX_LINEAGES_PER_PASS,
        &mut budget,
        &mut report,
    ) {
        sweep_directory(&lineage, LeftoverScope::AppData, &mut budget, &mut report);
    }
    report
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    use tempfile::TempDir;

    use super::*;
    use crate::services::filesystem::{fixture, metadata};

    fn stale_leftover(dir: &Path, name: &str) -> PathBuf {
        fixture::create_dir_all(dir);
        let path = dir.join(name);
        fixture::write_text(&path, "partial");
        let old = SystemTime::now()
            .checked_sub(Duration::from_hours(48))
            .expect("wall clock is far past the epoch");
        fixture::set_modified(&path, old);
        path
    }

    #[test]
    fn startup_sweep_removes_a_killed_process_leftover_from_drafts_only() {
        let data = TempDir::new().expect("temp dir");
        let drafts = draft_service::drafts_dir(data.path());
        let foreign_pid = std::process::id().wrapping_add(1);
        let leftover = stale_leftover(&drafts, &format!(".manifest.json.json.{foreign_pid}.3.tmp"));
        let manifest = drafts.join("manifest.json");
        let body = drafts.join("draft-1.txt");
        fixture::write_text(&manifest, "{}");
        fixture::write_text(&body, "unsaved work");

        let report = sweep_startup_leftovers(data.path());

        assert_eq!(report.removed, 1);
        assert!(!metadata::exists(&leftover));
        fixture::assert_text(&manifest, "{}");
        fixture::assert_text(&body, "unsaved work");
    }

    #[test]
    fn startup_sweep_reaches_local_history_lineages_and_sidecar_dirs() {
        let data = TempDir::new().expect("temp dir");
        let foreign_pid = std::process::id().wrapping_add(1);
        let lineage =
            local_history_service::local_history_dir(data.path()).join("d9ed75b43b54f9b4");
        let in_lineage = stale_leftover(&lineage, &format!(".index.json.json.{foreign_pid}.1.tmp"));
        let in_bookmarks = stale_leftover(
            &bookmark_service::bookmarks_dir(data.path()),
            &format!(".abc.json.json.{foreign_pid}.2.tmp"),
        );
        let in_root = stale_leftover(
            data.path(),
            &format!(".session.json.json.{foreign_pid}.3.tmp"),
        );

        let report = sweep_startup_leftovers(data.path());

        assert_eq!(report.removed, 3);
        for path in [&in_lineage, &in_bookmarks, &in_root] {
            assert!(!metadata::exists(path), "{} must be swept", path.display());
        }
        assert!(metadata::exists(&lineage));
    }

    #[cfg(unix)]
    #[test]
    fn startup_sweep_never_follows_a_symlinked_lineage_out_of_app_data() {
        let data = TempDir::new().expect("temp dir");
        let outside = TempDir::new().expect("outside dir");
        let foreign_pid = std::process::id().wrapping_add(1);
        let outside_leftover = stale_leftover(
            outside.path(),
            &format!(".notes.md.save.{foreign_pid}.3.tmp"),
        );
        let history = local_history_service::local_history_dir(data.path());
        fixture::create_dir_all(&history);
        fixture::symlink(outside.path(), &history.join("d9ed75b43b54f9b4"));

        let report = sweep_startup_leftovers(data.path());

        assert_eq!(report.removed, 0);
        assert!(metadata::exists(&outside_leftover));
    }

    #[test]
    fn startup_sweep_of_an_empty_data_home_is_a_no_op() {
        let data = TempDir::new().expect("temp dir");

        let report = sweep_startup_leftovers(&data.path().join("never-created"));

        assert_eq!(report.removed, 0);
        assert_eq!(report.failures, 0);
    }
}
