// SPDX-License-Identifier: GPL-3.0-or-later

//! Startup sweep of durable-write crash leftovers in LushText's own app data.
//!
//! *Where* to look comes from the app-data layout
//! (`services::app_data_layout`), which names every location once: this
//! module visits each location the layout marks swept, non-recursively and
//! under one shared per-pass budget, in a fixed order (the budget is spent in
//! visit order):
//!
//! 1. every `Swept` location, in `AppDataLocation::ALL` order;
//! 2. the two families, in `FAMILY_SWEEP_ORDER`: each format-upgrade backup
//!    run with its `items/` child, then each local-history lineage, one level
//!    below their roots;
//! 3. the `SweptLast` location, the legacy folder-note directory, with
//!    whatever budget remains.
//!
//! Both family listings are charged to the same budget. The removal decision
//! itself lives in `services::filesystem::leftovers`.

use std::path::Path;

use crate::filesystem::leftovers::{
    LeftoverScope, LeftoverSweepBudget, LeftoverSweepReport, child_directories, sweep_directory,
};
use crate::persistence::app_data_layout::{
    AppDataKind, AppDataLayout, AppDataLocation, FAMILY_SWEEP_ORDER, SweepCoverage,
};

/// Local-history lineage directories swept per startup pass.
///
/// Lineages are listed in directory order, so a data home with more lineages
/// than this sweeps a subset each launch; leftovers elsewhere simply wait.
///
/// Kind: cap; a power of two, rounded down.
const MAX_LINEAGES_PER_PASS: usize = 1_024;

/// Format-upgrade backup runs swept per startup pass, on the same terms.
///
/// Kind: cap; a power of two, rounded down.
const MAX_BACKUP_RUNS_PER_PASS: usize = 256;

/// Sweep stale durable-write leftovers from every app-data directory under `data_dir`.
///
/// Never recurses beyond the named directories and one level of each family,
/// and never propagates failure: an unreadable directory or a leftover that
/// cannot be removed is counted and left for a later launch.
///
/// **Threading:** Performs blocking filesystem calls. Call from a background
/// thread.
#[must_use]
pub fn sweep_startup_leftovers(data_dir: &Path) -> LeftoverSweepReport {
    let mut budget = LeftoverSweepBudget::startup();
    let mut report = LeftoverSweepReport::default();
    visit_in_sweep_order(
        data_dir,
        &mut budget,
        &mut report,
        &mut |dir, budget, report| {
            sweep_directory(dir, LeftoverScope::AppData, budget, report);
        },
    );
    report
}

/// One directory visit of the sweep, charged to the shared budget.
type SweepVisit<'a> = dyn FnMut(&Path, &mut LeftoverSweepBudget, &mut LeftoverSweepReport) + 'a;

/// Visit every swept app-data directory in the sweep's fixed order.
fn visit_in_sweep_order(
    data_dir: &Path,
    budget: &mut LeftoverSweepBudget,
    report: &mut LeftoverSweepReport,
    visit: &mut SweepVisit<'_>,
) {
    let layout = AppDataLayout::new(data_dir);
    for location in AppDataLocation::ALL {
        match location.sweep() {
            SweepCoverage::Swept => visit(&layout.path(location), budget, report),
            SweepCoverage::SweptLast | SweepCoverage::ParentSwept => {}
        }
    }

    for family in FAMILY_SWEEP_ORDER {
        let root = layout.path(family);
        match family.kind() {
            // Each format-upgrade run writes its manifest in the run directory
            // and its backups in `items/` below it (hashed leaf names, never
            // deeper).
            AppDataKind::RunFamily { items } => {
                for run in child_directories(&root, MAX_BACKUP_RUNS_PER_PASS, budget, report) {
                    visit(&run, budget, report);
                    visit(&run.join(items), budget, report);
                }
            }
            AppDataKind::LineageFamily { .. } => {
                for lineage in child_directories(&root, MAX_LINEAGES_PER_PASS, budget, report) {
                    visit(&lineage, budget, report);
                }
            }
            AppDataKind::File | AppDataKind::Directory => {}
        }
    }

    for location in AppDataLocation::ALL {
        match location.sweep() {
            SweepCoverage::SweptLast => visit(&layout.path(location), budget, report),
            SweepCoverage::Swept | SweepCoverage::ParentSwept => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    use tempfile::TempDir;

    use super::*;
    use crate::filesystem::{fixture, metadata};
    use crate::{
        notes::bookmark_service, persistence::draft_service, persistence::local_history_service,
        search::search_backup,
    };

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
    fn startup_sweep_reaches_style_schemes_format_backups_and_the_set_aside_area() {
        let data = TempDir::new().expect("temp dir");
        let foreign_pid = std::process::id().wrapping_add(1);
        let in_schemes = stale_leftover(
            &crate::persistence::json_store::style_schemes_dir(data.path()),
            &format!(".lushtext-opacity-adwaita-90.xml.style-scheme.{foreign_pid}.1.tmp"),
        );
        let run = data
            .path()
            .join(crate::persistence::format_upgrade::FORMAT_UPGRADE_BACKUP_DIR)
            .join("1700000000-upgrade-00");
        let in_run = stale_leftover(
            &run,
            &format!(".manifest.json.format-upgrade-manifest.{foreign_pid}.2.tmp"),
        );
        let in_items = stale_leftover(
            &run.join(crate::persistence::format_upgrade::FORMAT_UPGRADE_BACKUP_ITEMS_DIR),
            &format!(".abcdef.json.format-upgrade-backup.{foreign_pid}.3.tmp"),
        );
        let in_set_aside = stale_leftover(
            &draft_service::set_aside_dir(data.path()),
            &format!(".abcdef.7.draft.draft.{foreign_pid}.4.tmp"),
        );
        let kept_backup = run
            .join(crate::persistence::format_upgrade::FORMAT_UPGRADE_BACKUP_ITEMS_DIR)
            .join("abcdef.json");
        fixture::write_text(&kept_backup, "{}");

        let report = sweep_startup_leftovers(data.path());

        for path in [&in_schemes, &in_run, &in_items, &in_set_aside] {
            assert!(!metadata::exists(path), "{} must be swept", path.display());
        }
        assert_eq!(report.removed, 4);
        fixture::assert_text(&kept_backup, "{}");
    }

    #[test]
    fn startup_sweep_reaches_the_replace_journal_entries_and_bodies() {
        // stream-replace-all task 7.5: an undo body is written in the
        // journal's `bodies/` directory, so its durable-write temp lands there.
        let data = TempDir::new().expect("temp dir");
        let foreign_pid = std::process::id().wrapping_add(1);
        let journal = search_backup::journal_dir(data.path());
        let in_entries = stale_leftover(
            &journal,
            &format!(".0123abcd.json.json.{foreign_pid}.1.tmp"),
        );
        let in_bodies = stale_leftover(
            &search_backup::bodies_dir(data.path()),
            &format!(".0123abcd.body.replace-undo-body.{foreign_pid}.2.tmp"),
        );

        let report = sweep_startup_leftovers(data.path());

        for path in [&in_entries, &in_bodies] {
            assert!(!metadata::exists(path), "{} must be swept", path.display());
        }
        assert_eq!(report.removed, 2);
    }

    /// A format upgrade rewrites a legacy folder-note sidecar in place, so a
    /// killed upgrade can leave a durable-write temp in `workspace-notes/`.
    /// The sweep reaches it, last, through the same stale-only predicate.
    #[test]
    fn startup_sweep_reaches_the_legacy_folder_note_directory() {
        let data = TempDir::new().expect("temp dir");
        let foreign_pid = std::process::id().wrapping_add(1);
        let legacy = data.path().join("workspace-notes");
        let stale = stale_leftover(
            &legacy,
            &format!(".abcdef.json.format-upgrade-convert.{foreign_pid}.1.tmp"),
        );
        let fresh = legacy.join(format!(
            ".fedcba.json.format-upgrade-convert.{foreign_pid}.2.tmp"
        ));
        fixture::write_text(&fresh, "in flight");
        let sidecar = legacy.join("abcdef.json");
        fixture::write_text(&sidecar, "{}");

        let report = sweep_startup_leftovers(data.path());

        assert!(!metadata::exists(&stale), "the stale temp is swept");
        assert!(metadata::exists(&fresh), "a fresh temp is kept");
        fixture::assert_text(&sidecar, "{}");
        assert_eq!(report.removed, 1);
    }

    /// The whole visit order, which is how the shared budget is spent: the
    /// flat directories in `ALL` order, each format-upgrade run with its
    /// `items/`, each local-history lineage, then the legacy directory.
    #[test]
    fn the_sweep_visits_app_data_in_its_fixed_order() {
        let data = TempDir::new().expect("temp dir");
        fixture::create_dir_all(&data.path().join("format-upgrade-backups/run-a/items"));
        fixture::create_dir_all(&data.path().join("local-history/lineage-a"));
        let mut visited = Vec::new();
        let mut budget = LeftoverSweepBudget::startup();
        let mut report = LeftoverSweepReport::default();

        visit_in_sweep_order(data.path(), &mut budget, &mut report, &mut |dir, _, _| {
            visited.push(
                dir.strip_prefix(data.path())
                    .expect("inside the data home")
                    .display()
                    .to_string(),
            );
        });

        assert_eq!(
            visited,
            [
                "",
                "drafts",
                "local-history",
                "bookmarks",
                "document-notes",
                "folder-notes",
                "recovery-quarantine",
                "replace-backup-journal",
                "replace-backup-journal/bodies",
                "style-schemes",
                "drafts/set-aside",
                "format-upgrade-backups",
                "format-upgrade-backups/run-a",
                "format-upgrade-backups/run-a/items",
                "local-history/lineage-a",
                "workspace-notes",
            ]
        );
    }

    #[test]
    fn startup_sweep_of_an_empty_data_home_is_a_no_op() {
        let data = TempDir::new().expect("temp dir");

        let report = sweep_startup_leftovers(&data.path().join("never-created"));

        assert_eq!(report.removed, 0);
        assert_eq!(report.failures, 0);
    }
}
