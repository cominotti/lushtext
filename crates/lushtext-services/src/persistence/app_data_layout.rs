// SPDX-License-Identifier: GPL-3.0-or-later

//! The **one owner** of the app-data directory layout.
//!
//! Every file and directory LushText keeps under its data home
//! (`json_store::data_dir()`) is one [`AppDataLocation`], and its on-disk name
//! is spelled only here. [`AppDataLocation::ALL`] is declared by the same
//! list as the enum, so it holds every location by construction, and the
//! consumers that must cover every location iterate it and decide each one
//! through an exhaustive `match`, so a new location does not compile until
//! each has decided it:
//!
//! - the startup leftover sweep (`app_data_leftovers.rs`) visits every
//!   location whose [`AppDataLocation::sweep`] is [`SweepCoverage::Swept`],
//!   in [`AppDataLocation::ALL`] order, then the two families in
//!   [`FAMILY_SWEEP_ORDER`], then the [`SweepCoverage::SweptLast`] location;
//! - the format-upgrade inventory (`format_upgrade/inventory.rs`) scans,
//!   covers or exempts each location in its own `match`, so this module does
//!   not depend on `format_upgrade`;
//! - each service resolves its paths through [`AppDataLayout`], keeping its
//!   public helpers (`drafts_dir`, `bookmarks_dir`, …) as delegations.
//!
//! Every location holds durable writes: a format upgrade rewrites each
//! inventoried item in place, and every other location is written through the
//! durable-write boundary. So every location is swept, directly or through its
//! parent; a future location that holds none adds a `NotSwept` variant with
//! it. GTK-free and I/O-free.

use std::path::{Path, PathBuf};

/// The file each local-history lineage keeps its snapshot index in.
const LINEAGE_INDEX_FILE: &str = "index.json";

/// The directory each format-upgrade backup run keeps its backups in.
const RUN_ITEMS_DIR: &str = "items";

/// Declares [`AppDataLocation`] and [`AppDataLocation::ALL`] from one list,
/// so no location can exist outside `ALL`: the startup sweep and the
/// format-upgrade inventory, which iterate `ALL`, reach every location by
/// construction, and a new variant needs no second edit to be listed.
macro_rules! app_data_locations {
    ($($(#[$meta:meta])* $variant:ident,)*) => {
        /// One file or directory under the app data home.
        ///
        /// The swept directories and families come first, in the order the
        /// startup sweep visits them (its shared budget is spent in that
        /// order), then the legacy folder-note directory, then the files.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum AppDataLocation {
            $($(#[$meta])* $variant,)*
        }

        impl AppDataLocation {
            /// Every location, in declaration order. Declared by the same
            /// list as the enum, so it holds every variant exactly once.
            pub const ALL: [Self; [$(stringify!($variant)),*].len()] = [$(Self::$variant),*];
        }
    };
}

app_data_locations! {
    /// The data home itself, which holds the top-level files.
    Root,
    /// Draft bodies and their manifest.
    DraftsDir,
    /// Local-history lineages, one directory per document.
    LocalHistoryDir,
    /// Bookmark sidecars.
    Bookmarks,
    /// Document-note sidecars.
    DocumentNotes,
    /// Folder-note sidecars.
    FolderNotes,
    /// Byte copies of broken metadata, kept as evidence.
    RecoveryQuarantine,
    /// The Replace All undo journal: its entries, manifest and marker.
    ReplaceJournalDir,
    /// The undo journal's raw bodies.
    ReplaceJournalBodies,
    /// Derived GtkSourceView style schemes.
    StyleSchemes,
    /// Preserved draft bodies set aside instead of deleted.
    SetAside,
    /// Format-upgrade backup runs, each with its `items/` child.
    FormatUpgradeBackups,
    /// The legacy folder-note sidecar directory, read and upgraded in place.
    LegacyFolderNotes,
    /// The workspaces file.
    Workspaces,
    /// The session file.
    Session,
    /// The draft manifest.
    DraftManifest,
    /// Named saved searches.
    SavedSearches,
    /// Recent searches.
    SearchHistory,
    /// Recently opened documents (unversioned, no envelope).
    RecentDocuments,
    /// The post-rename migration ledger.
    MigrationLedger,
    /// The undo journal's manifest.
    ReplaceJournalManifest,
    /// The undo journal's interrupted-cleanup marker.
    ReplaceCleanupMarker,
    /// The retired whole-file Replace All backup.
    RetiredReplaceBackup,
}

/// What kind of entry a location is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppDataKind {
    /// A regular file.
    File,
    /// A directory whose entries are files.
    Directory,
    /// A directory of per-document directories, each holding `index`.
    LineageFamily {
        /// The index file in each child directory.
        index: &'static str,
    },
    /// A directory of run directories, each holding an `items` directory.
    RunFamily {
        /// The backups directory in each run.
        items: &'static str,
    },
}

/// How the startup leftover sweep covers the durable-write temps that can
/// land in a location.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SweepCoverage {
    /// Visited in the sweep's main pass, in [`AppDataLocation::ALL`] order.
    Swept,
    /// Visited after the families, with whatever budget remains, so every
    /// directory swept before keeps its order and share of the budget.
    SweptLast,
    /// A file: its temps land in its parent, which is swept.
    ParentSwept,
}

/// The order the sweep expands the two families in, after the main pass:
/// the format-upgrade runs with their `items/`, then the local-history
/// lineages. This is **not** their order in [`AppDataLocation::ALL`], and it
/// is behaviour: the shared budget is spent in visit order.
pub const FAMILY_SWEEP_ORDER: [AppDataLocation; 2] = [
    AppDataLocation::FormatUpgradeBackups,
    AppDataLocation::LocalHistoryDir,
];

impl AppDataLocation {
    /// The location's path relative to the data home, `/`-separated; empty
    /// for [`Self::Root`].
    #[must_use]
    pub const fn relative_str(self) -> &'static str {
        match self {
            Self::Root => "",
            Self::DraftsDir => "drafts",
            Self::LocalHistoryDir => "local-history",
            Self::Bookmarks => "bookmarks",
            Self::DocumentNotes => "document-notes",
            Self::FolderNotes => "folder-notes",
            Self::RecoveryQuarantine => "recovery-quarantine",
            Self::ReplaceJournalDir => "replace-backup-journal",
            Self::ReplaceJournalBodies => "replace-backup-journal/bodies",
            Self::StyleSchemes => "style-schemes",
            Self::SetAside => "drafts/set-aside",
            Self::FormatUpgradeBackups => "format-upgrade-backups",
            Self::LegacyFolderNotes => "workspace-notes",
            Self::Workspaces => "workspaces.json",
            Self::Session => "session.json",
            Self::DraftManifest => "drafts/manifest.json",
            Self::SavedSearches => "saved-searches.json",
            Self::SearchHistory => "search-history.json",
            Self::RecentDocuments => "recent-documents.json",
            Self::MigrationLedger => "migration-ledger.json",
            Self::ReplaceJournalManifest => "replace-backup-journal/manifest.json",
            Self::ReplaceCleanupMarker => "replace-backup-journal/cleanup-in-progress.json",
            Self::RetiredReplaceBackup => "replace-backup.json",
        }
    }

    /// The location's path relative to the data home.
    #[must_use]
    pub fn relative(self) -> &'static Path {
        Path::new(self.relative_str())
    }

    /// The location's own name, its last path component (empty for
    /// [`Self::Root`]).
    #[must_use]
    pub fn file_name(self) -> &'static str {
        let relative = self.relative_str();
        relative.rsplit_once('/').map_or(relative, |(_, name)| name)
    }

    /// The location that holds this one ([`Self::Root`] holds itself).
    #[must_use]
    pub const fn parent(self) -> Self {
        match self {
            Self::SetAside | Self::DraftManifest => Self::DraftsDir,
            Self::ReplaceJournalBodies
            | Self::ReplaceJournalManifest
            | Self::ReplaceCleanupMarker => Self::ReplaceJournalDir,
            Self::Root
            | Self::DraftsDir
            | Self::LocalHistoryDir
            | Self::Bookmarks
            | Self::DocumentNotes
            | Self::FolderNotes
            | Self::RecoveryQuarantine
            | Self::ReplaceJournalDir
            | Self::StyleSchemes
            | Self::FormatUpgradeBackups
            | Self::LegacyFolderNotes
            | Self::Workspaces
            | Self::Session
            | Self::SavedSearches
            | Self::SearchHistory
            | Self::RecentDocuments
            | Self::MigrationLedger
            | Self::RetiredReplaceBackup => Self::Root,
        }
    }

    /// What kind of entry the location is.
    #[must_use]
    pub const fn kind(self) -> AppDataKind {
        match self {
            Self::LocalHistoryDir => AppDataKind::LineageFamily {
                index: LINEAGE_INDEX_FILE,
            },
            Self::FormatUpgradeBackups => AppDataKind::RunFamily {
                items: RUN_ITEMS_DIR,
            },
            Self::Root
            | Self::DraftsDir
            | Self::Bookmarks
            | Self::DocumentNotes
            | Self::FolderNotes
            | Self::RecoveryQuarantine
            | Self::ReplaceJournalDir
            | Self::ReplaceJournalBodies
            | Self::StyleSchemes
            | Self::SetAside
            | Self::LegacyFolderNotes => AppDataKind::Directory,
            Self::Workspaces
            | Self::Session
            | Self::DraftManifest
            | Self::SavedSearches
            | Self::SearchHistory
            | Self::RecentDocuments
            | Self::MigrationLedger
            | Self::ReplaceJournalManifest
            | Self::ReplaceCleanupMarker
            | Self::RetiredReplaceBackup => AppDataKind::File,
        }
    }

    /// How the startup sweep covers this location's durable-write temps.
    #[must_use]
    pub const fn sweep(self) -> SweepCoverage {
        match self {
            Self::Root
            | Self::DraftsDir
            | Self::LocalHistoryDir
            | Self::Bookmarks
            | Self::DocumentNotes
            | Self::FolderNotes
            | Self::RecoveryQuarantine
            | Self::ReplaceJournalDir
            | Self::ReplaceJournalBodies
            | Self::StyleSchemes
            | Self::SetAside
            | Self::FormatUpgradeBackups => SweepCoverage::Swept,
            Self::LegacyFolderNotes => SweepCoverage::SweptLast,
            Self::Workspaces
            | Self::Session
            | Self::DraftManifest
            | Self::SavedSearches
            | Self::SearchHistory
            | Self::RecentDocuments
            | Self::MigrationLedger
            | Self::ReplaceJournalManifest
            | Self::ReplaceCleanupMarker
            | Self::RetiredReplaceBackup => SweepCoverage::ParentSwept,
        }
    }
}

/// The app-data layout resolved against one data home.
#[derive(Clone, Copy, Debug)]
pub struct AppDataLayout<'a> {
    root: &'a Path,
}

impl<'a> AppDataLayout<'a> {
    /// The layout under `root`, the data home.
    #[must_use]
    pub const fn new(root: &'a Path) -> Self {
        Self { root }
    }

    /// The absolute path of `location`; [`AppDataLocation::Root`] is the data
    /// home itself, unchanged.
    #[must_use]
    pub fn path(self, location: AppDataLocation) -> PathBuf {
        match location {
            AppDataLocation::Root => self.root.to_path_buf(),
            _ => self.root.join(location.relative()),
        }
    }

    /// The index file of the local-history lineage directory `lineage`.
    #[must_use]
    pub fn lineage_index(lineage: &Path) -> PathBuf {
        lineage.join(LINEAGE_INDEX_FILE)
    }

    /// The backups directory of the format-upgrade run directory `run`.
    #[must_use]
    pub fn run_items(run: &Path) -> PathBuf {
        run.join(RUN_ITEMS_DIR)
    }
}

/// The lineage index file name, for callers that compare entry names.
#[must_use]
pub const fn lineage_index_file_name() -> &'static str {
    LINEAGE_INDEX_FILE
}

/// The run backups directory name, for callers that compare entry names.
#[must_use]
pub const fn run_items_dir_name() -> &'static str {
    RUN_ITEMS_DIR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_location_is_swept_directly_last_or_through_a_swept_parent() {
        for location in AppDataLocation::ALL {
            match location.sweep() {
                SweepCoverage::Swept | SweepCoverage::SweptLast => {
                    assert_ne!(location.kind(), AppDataKind::File, "{location:?}");
                }
                SweepCoverage::ParentSwept => {
                    assert_eq!(location.kind(), AppDataKind::File, "{location:?}");
                    assert_eq!(
                        location.parent().sweep(),
                        SweepCoverage::Swept,
                        "{location:?}'s parent is swept"
                    );
                }
            }
        }
        let last: Vec<_> = AppDataLocation::ALL
            .into_iter()
            .filter(|location| location.sweep() == SweepCoverage::SweptLast)
            .collect();
        assert_eq!(last, vec![AppDataLocation::LegacyFolderNotes]);
    }

    #[test]
    fn every_entry_lives_in_its_parent_and_all_lists_each_location_once() {
        let mut seen = std::collections::HashSet::new();
        for location in AppDataLocation::ALL {
            assert!(seen.insert(location), "{location:?} listed twice");
            if location == AppDataLocation::Root {
                continue;
            }
            let parent = location.parent();
            let expected = if parent == AppDataLocation::Root {
                Path::new("")
            } else {
                parent.relative()
            };
            assert_eq!(location.relative().parent(), Some(expected), "{location:?}");
            assert_eq!(
                location
                    .relative()
                    .file_name()
                    .and_then(std::ffi::OsStr::to_str),
                Some(location.file_name())
            );
        }
    }

    #[test]
    fn the_families_are_expanded_runs_first_and_are_both_swept() {
        assert_eq!(
            FAMILY_SWEEP_ORDER,
            [
                AppDataLocation::FormatUpgradeBackups,
                AppDataLocation::LocalHistoryDir
            ]
        );
        for family in FAMILY_SWEEP_ORDER {
            assert_eq!(family.sweep(), SweepCoverage::Swept);
            assert!(matches!(
                family.kind(),
                AppDataKind::LineageFamily { .. } | AppDataKind::RunFamily { .. }
            ));
        }
        let families = AppDataLocation::ALL
            .into_iter()
            .filter(|location| {
                matches!(
                    location.kind(),
                    AppDataKind::LineageFamily { .. } | AppDataKind::RunFamily { .. }
                )
            })
            .count();
        assert_eq!(families, FAMILY_SWEEP_ORDER.len());
    }

    #[test]
    fn the_root_resolves_to_the_data_home_itself() {
        let layout = AppDataLayout::new(Path::new("/data/lushtext"));
        assert_eq!(
            layout.path(AppDataLocation::Root),
            Path::new("/data/lushtext")
        );
        assert_eq!(
            layout
                .path(AppDataLocation::Root)
                .as_os_str()
                .as_encoded_bytes(),
            b"/data/lushtext"
        );
    }
}

#[cfg(test)]
mod characterization {
    //! Every app-data path, as the services resolved it before the layout
    //! owned them. The literals are the on-disk names: changing one moves
    //! user data out of sight, so this table only ever grows.

    use std::path::{Path, PathBuf};

    use crate::filesystem::fixture;
    use crate::{
        notes::bookmark_service, notes::document_note_service, notes::folder_note_service,
        persistence::draft_service, persistence::format_upgrade, persistence::json_store,
        persistence::local_history_service, persistence::migration_ledger,
        persistence::recent_documents, persistence::recovery_metadata, search::search_backup,
    };

    const ROOT: &str = "/data/lushtext";

    fn under(relative: &str) -> PathBuf {
        Path::new(ROOT).join(relative)
    }

    #[test]
    fn every_path_helper_resolves_its_location_as_before() {
        let root = Path::new(ROOT);
        let helpers: [(&str, PathBuf, &str); 10] = [
            ("drafts_dir", draft_service::drafts_dir(root), "drafts"),
            (
                "set_aside_dir",
                draft_service::set_aside_dir(root),
                "drafts/set-aside",
            ),
            (
                "local_history_dir",
                local_history_service::local_history_dir(root),
                "local-history",
            ),
            (
                "bookmarks_dir",
                bookmark_service::bookmarks_dir(root),
                "bookmarks",
            ),
            (
                "document_notes_dir",
                document_note_service::document_notes_dir(root),
                "document-notes",
            ),
            (
                "folder_notes_dir",
                folder_note_service::folder_notes_dir(root),
                "folder-notes",
            ),
            (
                "style_schemes_dir",
                json_store::style_schemes_dir(root),
                "style-schemes",
            ),
            (
                "journal_dir",
                search_backup::journal_dir(root),
                "replace-backup-journal",
            ),
            (
                "bodies_dir",
                search_backup::bodies_dir(root),
                "replace-backup-journal/bodies",
            ),
            (
                "ledger_path",
                migration_ledger::ledger_path(root),
                "migration-ledger.json",
            ),
        ];
        for (helper, resolved, relative) in helpers {
            assert_eq!(
                resolved.as_os_str().as_encoded_bytes(),
                under(relative).as_os_str().as_encoded_bytes(),
                "{helper}"
            );
        }
    }

    /// The layout spells every location exactly as the services did. The
    /// literals were checked against the services' own constants before the
    /// layout took them over (`apply-verification-altitude-redesigns`, task
    /// 2.1).
    #[test]
    fn the_layout_resolves_every_location_as_the_services_did() {
        use crate::persistence::app_data_layout::{AppDataLayout, AppDataLocation as L};
        let expected: [(L, &str); 23] = [
            (L::Root, ""),
            (L::DraftsDir, "drafts"),
            (L::LocalHistoryDir, "local-history"),
            (L::Bookmarks, "bookmarks"),
            (L::DocumentNotes, "document-notes"),
            (L::FolderNotes, "folder-notes"),
            (L::RecoveryQuarantine, "recovery-quarantine"),
            (L::ReplaceJournalDir, "replace-backup-journal"),
            (L::ReplaceJournalBodies, "replace-backup-journal/bodies"),
            (L::StyleSchemes, "style-schemes"),
            (L::SetAside, "drafts/set-aside"),
            (L::FormatUpgradeBackups, "format-upgrade-backups"),
            (L::LegacyFolderNotes, "workspace-notes"),
            (L::Workspaces, "workspaces.json"),
            (L::Session, "session.json"),
            (L::DraftManifest, "drafts/manifest.json"),
            (L::SavedSearches, "saved-searches.json"),
            (L::SearchHistory, "search-history.json"),
            (L::RecentDocuments, "recent-documents.json"),
            (L::MigrationLedger, "migration-ledger.json"),
            (
                L::ReplaceJournalManifest,
                "replace-backup-journal/manifest.json",
            ),
            (
                L::ReplaceCleanupMarker,
                "replace-backup-journal/cleanup-in-progress.json",
            ),
            (L::RetiredReplaceBackup, "replace-backup.json"),
        ];
        assert_eq!(expected.map(|(location, _)| location), L::ALL);
        let layout = AppDataLayout::new(Path::new(ROOT));
        for (location, relative) in expected {
            let resolved = layout.path(location);
            let want = if relative.is_empty() {
                PathBuf::from(ROOT)
            } else {
                under(relative)
            };
            assert_eq!(
                resolved.as_os_str().as_encoded_bytes(),
                want.as_os_str().as_encoded_bytes(),
                "{location:?}"
            );
        }
        assert_eq!(
            AppDataLayout::lineage_index(&under("local-history/abc")),
            under("local-history/abc/index.json")
        );
        assert_eq!(
            AppDataLayout::run_items(&under("format-upgrade-backups/run")),
            under("format-upgrade-backups/run/items")
        );
    }

    #[test]
    fn every_public_name_resolves_its_location_as_before() {
        assert_eq!(recovery_metadata::QUARANTINE_DIR, "recovery-quarantine");
        assert_eq!(
            format_upgrade::FORMAT_UPGRADE_BACKUP_DIR,
            "format-upgrade-backups"
        );
        assert_eq!(format_upgrade::FORMAT_UPGRADE_BACKUP_ITEMS_DIR, "items");
        assert_eq!(
            recent_documents::RECENT_DOCUMENTS_FILE,
            "recent-documents.json"
        );
    }

    /// One file in every location the format-upgrade inventory scans, and
    /// the relative paths and kinds `scan` records for them, in its order.
    /// Upgrade backups are named by these relative paths
    /// (`format_upgrade/backup.rs`), so they are on-disk names too.
    #[test]
    fn the_inventory_records_the_same_relative_paths_as_before() {
        let data = tempfile::TempDir::new().expect("temp dir");
        for relative in [
            "workspaces.json",
            "session.json",
            "drafts/manifest.json",
            "drafts/a.draft",
            "saved-searches.json",
            "search-history.json",
            "bookmarks/a.json",
            "document-notes/a.json",
            "folder-notes/a.json",
            "workspace-notes/a.json",
            "local-history/abc/index.json",
            "migration-ledger.json",
            "replace-backup-journal/manifest.json",
            "replace-backup-journal/cleanup-in-progress.json",
            "replace-backup-journal/e.json",
            "replace-backup-journal/bodies/e.body",
            "replace-backup.json",
        ] {
            let path = data.path().join(relative);
            fixture::create_dir_all(path.parent().expect("a parent"));
            fixture::write_text(&path, "{}");
        }

        let inventory = format_upgrade::scan(data.path());

        let recorded: Vec<(String, String)> = inventory
            .items
            .iter()
            .map(|item| {
                (
                    item.path.relative().display().to_string(),
                    format!("{:?}", item.kind),
                )
            })
            .collect();
        let expected: Vec<(String, String)> = [
            ("bookmarks/a.json", "BookmarkSidecar"),
            ("document-notes/a.json", "DocumentNoteSidecar"),
            ("drafts/a.draft", "DraftBody"),
            ("drafts/manifest.json", "DraftManifest"),
            ("folder-notes/a.json", "FolderNoteSidecar"),
            ("local-history/abc/index.json", "LocalHistoryIndex"),
            ("migration-ledger.json", "MigrationLedger"),
            ("replace-backup-journal/bodies/e.body", "ReplaceUndoBody"),
            (
                "replace-backup-journal/cleanup-in-progress.json",
                "ReplaceUndoCleanupMarker",
            ),
            ("replace-backup-journal/e.json", "ReplaceUndoStreamEntry"),
            (
                "replace-backup-journal/manifest.json",
                "ReplaceUndoManifest",
            ),
            ("replace-backup.json", "RetiredReplaceUndoBackup"),
            ("saved-searches.json", "SavedSearches"),
            ("search-history.json", "SearchHistory"),
            ("session.json", "Session"),
            ("workspace-notes/a.json", "LegacyFolderNoteSidecar"),
            ("workspaces.json", "WorkspaceState"),
        ]
        .iter()
        .map(|(path, kind)| ((*path).to_owned(), (*kind).to_owned()))
        .collect();
        assert_eq!(recorded, expected);
        assert!(
            inventory.diagnostics.is_empty(),
            "{:?}",
            inventory.diagnostics
        );
    }
}
