// SPDX-License-Identifier: GPL-3.0-or-later

//! Typed monotonic workflow generation counters generated via [`crate::define_generation!`].

crate::define_generation!(
    /// Editor MRU access sequence counter (`editor_memory`).
    pub AccessGeneration(u64)
);
crate::define_generation!(
    /// Editor memory policy evaluation epoch (`editor_memory`).
    pub PolicyGeneration(u64)
);
crate::define_generation!(
    /// Tab file-load workflow generation.
    pub LoadGeneration(u64)
);
crate::define_generation!(
    /// Buffer content mutation generation.
    pub EditGeneration(u64)
);
crate::define_generation!(
    /// Tab save workflow generation.
    pub SaveGeneration(u64)
);
crate::define_generation!(
    /// Tab bound path identity generation.
    pub PathGeneration(u64)
);
crate::define_generation!(
    /// Editor identity epoch across rebinds/restores (`local_history`, `editor_page`).
    pub EditorGeneration(u64)
);
crate::define_generation!(
    /// Draft dirty-edit generation (`DraftState`, `CloseSafetyEditorFingerprint`).
    pub DraftGeneration(u64)
);
crate::define_generation!(
    /// Whole-process draft journal session generation.
    pub SessionGeneration(u64)
);
crate::define_generation!(
    /// Sidebar child-cache mirror generation.
    pub MirrorGeneration(u64)
);
crate::define_generation!(
    /// Sidebar directory scan flight generation.
    pub ScanGeneration(u64)
);
crate::define_generation!(
    /// Sidebar watcher target / scan request generation.
    pub TargetGeneration(u64)
);
crate::define_generation!(
    /// Sidebar persistence request/durable epoch.
    pub PersistenceGeneration(u64)
);
crate::define_generation!(
    /// Workspace tree path reveal flight generation.
    pub RevealGeneration(u64)
);
crate::define_generation!(
    /// Markdown preview render session / image work generation.
    pub RenderGeneration(u64)
);
crate::define_generation!(
    /// Markdown preview anchored code-block layout refresh generation.
    pub CodeBlockRefreshGeneration(u32)
);
crate::define_generation!(
    /// Markdown preview rendered embed ownership generation.
    pub EmbedGeneration(u64)
);
crate::define_generation!(
    /// Session-restore coordinator run generation.
    pub RestoreGeneration(u64)
);
crate::define_generation!(
    /// Session-restore selection deferral generation.
    pub SelectionGeneration(u64)
);
crate::define_generation!(
    /// Last session-restore generation that recorded a partial failure.
    pub FailedRestoreGeneration(u32)
);
crate::define_generation!(
    /// Command palette background file-index snapshot generation.
    pub FileIndexGeneration(u64)
);
crate::define_generation!(
    /// Workspace or command-palette search query generation.
    pub SearchGeneration(u64)
);
crate::define_generation!(
    /// Document profile / minimap background analysis generation.
    pub AnalysisGeneration(u64)
);
crate::define_generation!(
    /// Lossy-decoding inline analysis notification generation.
    pub LossyAnalysisGeneration(u32)
);
crate::define_generation!(
    /// Minimap marker binning generation.
    pub MarkerGeneration(u64)
);
crate::define_generation!(
    /// Minimap modified-line tag epoch.
    pub TagGeneration(u64)
);
crate::define_generation!(
    /// Minimap or search-panel sliced retirement epoch.
    pub RetirementGeneration(u64)
);
crate::define_generation!(
    /// Tab strip overflow focus-restoration epoch.
    pub FocusRestoreGeneration(u32)
);
crate::define_generation!(
    /// Window display font/zoom CSS class generation.
    pub FontZoomGeneration(u64)
);
crate::define_generation!(
    /// Notification bus inline-alert generation.
    pub NotificationGeneration(u64)
);
crate::define_generation!(
    /// Sidecar migration ledger record/sequence generation.
    pub MigrationGeneration(u64)
);
crate::define_generation!(
    /// Local-history periodic capture cadence generation.
    pub PeriodicHistoryGeneration(u32)
);
crate::define_generation!(
    /// Local-history clean-baseline snapshot generation.
    pub CleanBaselineGeneration(u64)
);
crate::define_generation!(
    /// Local-history preview flight / install session generation.
    pub HistoryPreviewGeneration(u64)
);
crate::define_generation!(
    /// Notes/bookmarks sidecar generation.
    pub BookmarkGeneration(u64)
);
crate::define_generation!(
    /// Notes browser source snapshot/query/row projection generation.
    pub NotesSourceGeneration(u64)
);
crate::define_generation!(
    /// Multi-file Replace All preview generation.
    pub ReplacePreviewGeneration(u32)
);
crate::define_generation!(
    /// Workspace Replace All transaction reservation generation.
    pub ReplaceTransactionGeneration(u32)
);
crate::define_generation!(
    /// Workspace Replace All backup/undo journal generation.
    pub UndoBackupGeneration(u32)
);
crate::define_generation!(
    /// Single-tab whole-buffer replacement generation.
    pub BufferReplacementGeneration(u64)
);
crate::define_generation!(
    /// Deferred GtkSourceLanguage attach generation.
    pub LanguageAttachGeneration(u64)
);
crate::define_generation!(
    /// Paced restyle generation (`ui/editor_page/restyle.rs`).
    pub RestyleGeneration(u64)
);
crate::define_generation!(
    /// Paced syntax/search reanalysis generation.
    pub ReanalysisGeneration(u64)
);
crate::define_generation!(
    /// Overscroll adjustment re-pin generation.
    pub OverscrollGeneration(u32)
);
crate::define_generation!(
    /// Generic service single-flight generation (`SingleFlightCoordinator`).
    pub FlightGeneration(u64)
);
crate::define_generation!(
    /// Protected-view line-model / search / inspector session generation.
    pub ProtectedViewGeneration(u64)
);
crate::define_generation!(
    /// Protected-view incremental find generation.
    pub ProtectedFindGeneration(u64)
);
crate::define_generation!(
    /// Line-inspector popover refresh generation.
    pub LineInspectorGeneration(u64)
);
crate::define_generation!(
    /// Recent-documents journal / open popover generation.
    pub RecentDocumentsGeneration(u64)
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_wraps_at_max_and_distinct_generations_are_zero_cost() {
        assert_eq!(LoadGeneration::ZERO.next(), LoadGeneration::new(1));
        assert_eq!(LoadGeneration::new(u64::MAX).next(), LoadGeneration::ZERO);
        assert_eq!(
            OverscrollGeneration::new(u32::MAX).next(),
            OverscrollGeneration::ZERO
        );
        assert_eq!(
            serde_json::to_string(&SaveGeneration::new(42)).expect("serialize transparent"),
            "42"
        );
    }
}
