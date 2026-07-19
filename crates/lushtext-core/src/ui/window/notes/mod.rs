// SPDX-License-Identifier: GPL-3.0-or-later

//! Bookmark and note workflows for the main window shell.
//!
//! This private facade owns shared callback coordination, note-sidecar migration,
//! and menu availability. Focused bookmark, editor, and browser workflows live
//! in sibling modules without introducing new GTK objects or public APIs.

mod bookmarks;
mod browser;
mod editors;

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::Arc;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_settle::Debounce;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::gio;
use gtk4::prelude::*;
use libadwaita::prelude::AdwDialogExt;

use crate::model::migration_ledger::MigrationKind;
use crate::model::palette::{
    PaletteNoteEntry, PaletteOpenEditorNoteSnapshot, PaletteOpenTabSource,
};
use crate::model::workspace::{WorkspaceConfig, WorkspaceScope};
use crate::services::recovery_metadata::RecoveryDiagnostic;
use crate::services::{
    bookmark_service, document_note_service, folder_note_service, json_store,
    local_history_service, migration_ledger, palette as palette_service,
};
use crate::ui::accessibility;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::markdown_preview::LushtextMarkdownPreview;
use crate::ui::status_bar::MessageKind;

use super::LushtextWindow;

#[cfg(feature = "test-utils")]
pub use crate::services::palette::{
    set_note_source_delay_for_test, set_notes_browser_query_delay_for_test,
};
#[cfg(feature = "test-utils")]
pub use bookmarks::set_bookmark_excerpt_preview_delay_for_test;

/// Maximum note rows materialized into a browser at once.
const NOTES_BROWSER_RENDER_LIMIT: usize = 500;
/// Maximum rows admitted into one Browse Notes source.
const NOTES_BROWSER_SOURCE_ENTRY_LIMIT: usize = 10_000;
/// Maximum aggregate searchable UTF-8 bytes retained by Browse Notes.
const NOTES_BROWSER_SOURCE_TEXT_LIMIT: usize = 64 * 1024 * 1024;
/// Maximum sidecar candidates retained by each Browse Notes directory scan.
const NOTES_BROWSER_SIDECAR_SCAN_LIMIT: usize = 10_000;
/// Maximum recovery diagnostics retained by one Browse Notes load.
const NOTES_BROWSER_DIAGNOSTIC_LIMIT: usize = 1_024;
/// Maximum open-editor snapshots plus bookmark rows captured on GTK.
const NOTES_BROWSER_OPEN_EDITOR_SNAPSHOT_LIMIT: usize = 10_000;
/// Maximum retained live-editor metadata cloned before note-source admission.
const NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT: u64 = 4 * 1024 * 1024;
/// Browser-owned source policy passed into the shared admission engine.
const NOTES_BROWSER_SOURCE_LIMITS: palette_service::NoteSourceLimits =
    palette_service::NoteSourceLimits {
        entries: NOTES_BROWSER_SOURCE_ENTRY_LIMIT,
        searchable_text_bytes: NOTES_BROWSER_SOURCE_TEXT_LIMIT,
        retained_bytes: palette_service::MAX_PALETTE_NOTE_RETAINED_BYTES,
        sidecar_entries: NOTES_BROWSER_SIDECAR_SCAN_LIMIT,
        diagnostics: NOTES_BROWSER_DIAGNOSTIC_LIMIT,
    };
#[cfg(feature = "test-utils")]
static NOTES_BROWSER_SOURCE_ENTRY_LIMIT_FOR_TEST: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(NOTES_BROWSER_SOURCE_ENTRY_LIMIT);

#[cfg(feature = "test-utils")]
fn notes_browser_source_limits() -> palette_service::NoteSourceLimits {
    let entries =
        NOTES_BROWSER_SOURCE_ENTRY_LIMIT_FOR_TEST.load(std::sync::atomic::Ordering::Acquire);
    palette_service::NoteSourceLimits {
        entries,
        sidecar_entries: NOTES_BROWSER_SOURCE_LIMITS.sidecar_entries.min(entries),
        ..NOTES_BROWSER_SOURCE_LIMITS
    }
}

#[cfg(not(feature = "test-utils"))]
fn notes_browser_source_limits() -> palette_service::NoteSourceLimits {
    NOTES_BROWSER_SOURCE_LIMITS
}

/// Override the browser source-entry policy for focused truncation tests.
#[cfg(feature = "test-utils")]
pub fn set_notes_browser_source_entry_limit_for_test(limit: usize) {
    NOTES_BROWSER_SOURCE_ENTRY_LIMIT_FOR_TEST.store(limit, std::sync::atomic::Ordering::Release);
}
/// Stack child name for Markdown/status bookmark and note previews.
const NOTES_PREVIEW_MARKDOWN_CHILD: &str = "markdown";
/// Stack child name for raw-text bookmark previews.
const NOTES_PREVIEW_RAW_CHILD: &str = "raw";
/// Horizontal inset inside raw bookmark previews.
const NOTES_RAW_PREVIEW_TEXT_MARGIN_HORIZONTAL_SP: i32 = 12;
/// Vertical inset inside raw bookmark previews.
const NOTES_RAW_PREVIEW_TEXT_MARGIN_VERTICAL_SP: i32 = 10;

/// Decision for `Open Folder Note...` when the caller has not supplied an exact folder row.
///
/// Folder notes are attached to folders, not workspaces. Naming this decision
/// keeps the zero/one/many rules explicit so command actions and workspace
/// header actions cannot quietly fall back to the first configured folder.
#[derive(Debug, Clone, PartialEq, Eq)]
enum FolderNoteOpenTarget {
    /// The current shared scope is `All workspaces`, so no single folder can be inferred.
    AggregateScope,
    /// A concrete workspace ID was requested but no restored workspace matched it.
    WorkspaceMissing,
    /// The concrete workspace exists but has no folders to attach a note to.
    EmptyWorkspace { workspace_name: String },
    /// The concrete workspace has exactly one folder and can open directly.
    SingleFolder {
        workspace_name: String,
        folder: PathBuf,
    },
    /// The concrete workspace has multiple folders and needs a visible choice.
    ChooseFolder {
        workspace_name: String,
        folders: Vec<PathBuf>,
    },
}

/// One entry shown in the unified notes browser.
type NotesBrowserEntry = PaletteNoteEntry;

/// Bounded live-editor request material plus content-free omission evidence.
struct OpenEditorNoteSnapshots {
    entries: Vec<PaletteOpenEditorNoteSnapshot>,
    retained_bytes: u64,
    truncated: bool,
}

fn open_editor_snapshot_heap_bytes(path: &PathBuf, source: Option<&PaletteOpenTabSource>) -> u64 {
    let bytes = path.capacity().saturating_add(source.map_or(0, |source| {
        source
            .workspace_name
            .as_ref()
            .map_or(0, String::capacity)
            .saturating_add(
                source
                    .workspace_folder
                    .as_ref()
                    .map_or(0, PathBuf::capacity),
            )
    }));
    u64::try_from(bytes).unwrap_or(u64::MAX)
}

/// State for one open unified notes browser dialog.
struct NotesBrowserState {
    /// Window that owns the browser and receives follow-up actions.
    window: LushtextWindow,
    /// Dialog containing the browser widgets.
    dialog: libadwaita::Dialog,
    /// Adaptive split view used for wide and narrow layouts.
    split_view: libadwaita::NavigationSplitView,
    /// Navigation page whose title follows the active inventory mode.
    sidebar_page: libadwaita::NavigationPage,
    /// Search field driving the current filtered row set.
    search_entry: gtk4::SearchEntry,
    /// Adwaita browse rail for bookmarks, folder notes, and document notes.
    sidebar: libadwaita::Sidebar,
    /// Visible notice when the current result set is capped for responsiveness.
    limit_label: gtk4::Label,
    /// Header label for the selected note.
    preview_title: gtk4::Label,
    /// Secondary metadata label for the selected note.
    preview_meta: gtk4::Label,
    /// Stack switching between Markdown/status previews and raw bookmark excerpts.
    preview_stack: gtk4::Stack,
    /// Shared markdown preview widget reused for notes and Markdown bookmark excerpts.
    markdown_preview: LushtextMarkdownPreview,
    /// Backing buffer for raw bookmark excerpts.
    raw_preview_buffer: gtk4::TextBuffer,
    /// Open action for the selected note.
    open_button: gtk4::Button,
    /// Back button shown when the split view collapses.
    back_button: gtk4::Button,
    /// Complete set of notes covered by this browser session.
    all_entries: RefCell<Arc<crate::ui::plain_disposal::DisposalOwned<Box<[NotesBrowserEntry]>>>>,
    /// Entry indexes currently shown in the sidebar's grouped visual order.
    filtered_indices: RefCell<Vec<usize>>,
    /// Debounce used to rebuild browser search rows after typing settles.
    search_debounce: Debounce,
    /// One-active/one-latest ownership for background full-source matching.
    query_runtime: RefCell<palette_service::NotesBrowserQueryCoordinator>,
    /// Generation owner for the initial bounded source construction.
    source_refreshes: RefCell<palette_service::NoteSourceRefreshCoordinator>,
    /// Active compact source request waiting for disposal admission before sidecar I/O.
    source_admission: RefCell<Option<palette_service::NoteSourceRefreshStart>>,
    /// One paced capacity wakeup for the browser source.
    source_capacity_wakeup: crate::ui::plain_disposal::ProgressDisposalCapacityWakeup,
    /// Typed source omissions reported separately from query render truncation.
    source_truncation: RefCell<Vec<palette_service::NoteSourceTruncationReason>>,
    /// Whether bounded source construction has published this dialog's source.
    source_ready: Cell<bool>,
    /// Whether dialog teardown has invalidated all source and query publication.
    disposed: Cell<bool>,
    /// Inventory mode that owns the current source/query generations.
    mode: Cell<palette_service::NotesBrowserMode>,
    /// Generation counter used to ignore stale closed-file bookmark preview loads.
    preview_generation: Cell<u32>,
}

/// Scalar bounded-source and query-ownership evidence for widget tests.
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NotesBrowserRuntimeSnapshot {
    /// Rows retained by the immutable admitted source.
    pub source_entries: usize,
    /// Whether source construction reported any omission reason.
    pub source_truncated: bool,
    /// Whether bounded source construction has completed.
    pub source_ready: bool,
    /// Inventory mode owning the current source and query generations.
    pub mode: palette_service::NotesBrowserMode,
    /// One-active/one-latest query ownership counters.
    pub query: palette_service::PaletteSearchCoordinatorSnapshot,
    /// Initial bounded-source ownership counters.
    pub source: palette_service::NoteSourceRefreshCoordinatorSnapshot,
}

/// Weak handle to the currently visible unified notes browser.
///
/// Window actions use this to drive the same search, selection, and Open button
/// behavior a user sees in the dialog without keeping a closed dialog alive.
#[derive(Clone)]
pub(super) struct ActiveNotesBrowser {
    state: Weak<NotesBrowserState>,
}

impl ActiveNotesBrowser {
    /// Track one newly presented notes browser dialog.
    fn new(state: &Rc<NotesBrowserState>) -> Self {
        Self {
            state: Rc::downgrade(state),
        }
    }

    /// Return whether this handle still points to the same browser state.
    fn same_target(&self, other: &Self) -> bool {
        self.state.ptr_eq(&other.state)
    }

    /// Return whether the dialog state still exists.
    fn is_alive(&self) -> bool {
        self.state.upgrade().is_some()
    }

    fn state(&self) -> Option<Rc<NotesBrowserState>> {
        self.state.upgrade()
    }

    /// Filter the visible notes browser through its normal search entry.
    fn set_query(&self, query: &str) -> bool {
        let Some(state) = self.state.upgrade() else {
            return false;
        };
        state.search_entry.set_text(query);
        true
    }

    /// Select one visible row by zero-based sidebar index.
    fn select_visible_row(&self, index: u32) -> bool {
        let Some(state) = self.state.upgrade() else {
            return false;
        };
        let Ok(index) = usize::try_from(index) else {
            return false;
        };
        if index >= state.filtered_indices.borrow().len() {
            return false;
        }
        let selected = u32::try_from(index).expect("usize originated from u32");
        state.sidebar.set_selected(selected);
        true
    }

    /// Activate the same Open workflow as the visible notes browser button.
    fn open_selected(&self) -> bool {
        let Some(state) = self.state.upgrade() else {
            return false;
        };
        if state.selected_entry_index().is_none() {
            return false;
        }
        state.open_selected();
        true
    }

    #[cfg(feature = "test-utils")]
    fn runtime_snapshot(&self) -> Option<NotesBrowserRuntimeSnapshot> {
        let state = self.state.upgrade()?;
        Some(NotesBrowserRuntimeSnapshot {
            source_entries: state.all_entries.borrow().len(),
            source_truncated: !state.source_truncation.borrow().is_empty(),
            source_ready: state.source_ready.get(),
            mode: state.mode.get(),
            query: state.query_runtime.borrow().snapshot(),
            source: state.source_refreshes.borrow().snapshot(),
        })
    }
}

impl LushtextWindow {
    /// Wire bookmark and note callbacks for a newly created editor page.
    pub(super) fn wire_note_callbacks(&self, editor: &LushtextEditorPage) {
        let window_weak = self.downgrade();
        let editor_weak = editor.downgrade();
        editor.connect_file_loaded(move || {
            if let Some(window) = window_weak.upgrade()
                && let Some(editor) = editor_weak.upgrade()
                && let Some(path) = editor.file_path()
            {
                window.resolve_notes_for_editor(&editor, &path);
            }
        });

        let window_weak = self.downgrade();
        let editor_weak = editor.downgrade();
        editor.connect_bookmarks_changed(move || {
            if let Some(window) = window_weak.upgrade()
                && let Some(editor) = editor_weak.upgrade()
            {
                window.save_bookmarks_debounced(&editor);
                window.refresh_command_palette_note_source_debounced();
                if window.is_active_editor(&editor) {
                    window.refresh_notes_menu_state();
                }
            }
        });

        // The editor owns the source-mark activation hook, but the window owns
        // dialogs and active-tab checks. Weak refs keep closed tabs/windows from
        // staying alive just because a signal connection still exists.
        let window_weak = self.downgrade();
        let editor_weak = editor.downgrade();
        editor.connect_bookmark_activated(move |bookmark| {
            if let Some(window) = window_weak.upgrade()
                && let Some(editor) = editor_weak.upgrade()
                && window.is_active_editor(&editor)
                && editor.file_path().is_some()
            {
                window.present_bookmark_edit_dialog(&editor, &bookmark);
            }
        });

        let window_weak = self.downgrade();
        let editor_weak = editor.downgrade();
        editor.buffer().connect_mark_set(move |_, _, _| {
            if let Some(window) = window_weak.upgrade()
                && let Some(editor) = editor_weak.upgrade()
                && window.is_active_editor(&editor)
            {
                window.refresh_notes_menu_state();
            }
        });
    }

    /// Reload sidecar notes for the editor after a successful file load or reload.
    pub(super) fn resolve_notes_for_editor(&self, editor: &LushtextEditorPage, path: &Path) {
        let path = path.to_path_buf();
        let path_for_load = path.clone();
        let started_at_generation = editor.bookmark_change_generation();
        let window_weak = self.downgrade();
        spawn_blocking_then(
            editor.clone(),
            move || {
                let data_dir = json_store::data_dir();
                bookmark_service::load_for_path(&data_dir, &path_for_load)
                    .map(|document| document.bookmarks)
            },
            move |editor, result| {
                if editor.file_path().as_deref() != Some(path.as_path()) {
                    return;
                }
                match result {
                    Ok(bookmarks) => {
                        if !editor
                            .load_bookmarks_if_generation_matches(&bookmarks, started_at_generation)
                        {
                            return;
                        }
                        if let Some(window) = window_weak.upgrade() {
                            window.refresh_command_palette_note_source_debounced();
                            window.refresh_status_bar();
                        }
                    }
                    Err(error) => {
                        if editor.bookmark_change_generation() != started_at_generation {
                            return;
                        }
                        tracing::error!("Failed to load notes for {}: {error}", path.display());
                        editor.clear_bookmarks();
                        if let Some(window) = window_weak.upgrade() {
                            window.publish_status_message(
                                "Bookmarks could not be loaded",
                                MessageKind::Warning,
                            );
                        }
                    }
                }
            },
        );
    }

    /// Reset live note state after Save As so the new path starts from its own identity.
    pub(super) fn reset_notes_after_save_as(&self, editor: &LushtextEditorPage, path: &Path) {
        editor.clear_bookmarks();
        self.resolve_notes_for_editor(editor, path);
        self.refresh_command_palette_note_source_debounced();
    }

    /// Migrate sidecar documents after an in-app sidebar rename.
    ///
    /// Pending ledger state is recorded before sidecar moves begin so interrupted
    /// partial work can retry on startup by generation.
    pub(super) fn migrate_note_sidecars_after_rename(&self, old_path: &Path, new_path: &Path) {
        let old_path = old_path.to_path_buf();
        let new_path = new_path.to_path_buf();
        let old_path_for_move = old_path.clone();
        let new_path_for_move = new_path.clone();
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                let data_dir = json_store::data_dir();
                let generation = migration_ledger::record_pending(
                    &data_dir,
                    &old_path_for_move,
                    &new_path_for_move,
                    &[
                        MigrationKind::Bookmarks,
                        MigrationKind::DocumentNotes,
                        MigrationKind::FolderNotes,
                    ],
                )?;
                let bookmark_count = migration_ledger::run_tracked_kind(
                    &data_dir,
                    generation,
                    MigrationKind::Bookmarks,
                    || {
                        bookmark_service::move_path_tree(
                            &data_dir,
                            &old_path_for_move,
                            &new_path_for_move,
                        )
                    },
                )?;
                let document_note_count = migration_ledger::run_tracked_kind(
                    &data_dir,
                    generation,
                    MigrationKind::DocumentNotes,
                    || {
                        document_note_service::move_path_tree(
                            &data_dir,
                            &old_path_for_move,
                            &new_path_for_move,
                        )
                    },
                )?;
                let folder_note_count = migration_ledger::run_tracked_kind(
                    &data_dir,
                    generation,
                    MigrationKind::FolderNotes,
                    || {
                        folder_note_service::move_folder_tree(
                            &data_dir,
                            &old_path_for_move,
                            &new_path_for_move,
                        )
                    },
                )?;
                Ok::<_, anyhow::Error>((bookmark_count, document_note_count, folder_note_count))
            },
            move |(), result| {
                if let Err(error) = result {
                    tracing::error!(
                        "Failed to migrate note sidecars for {} -> {}: {error}",
                        old_path.display(),
                        new_path.display()
                    );
                    if let Some(window) = window_weak.upgrade() {
                        window.publish_status_message(
                            "Rename succeeded, but note sidecars could not be moved",
                            MessageKind::Warning,
                        );
                    }
                } else if let Some(window) = window_weak.upgrade() {
                    window.refresh_command_palette_note_source_debounced();
                }
            },
        );
    }

    /// Retry persisted sidecar or local-history migrations left by an
    /// interrupted rename flow.
    pub(super) fn reconcile_pending_migrations_on_startup(&self) {
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                let data_dir = json_store::data_dir();
                let migration_report = migration_ledger::reconcile_pending(&data_dir)?;
                let local_history_report = local_history_service::reconcile_lineages(&data_dir)?;
                Ok::<_, anyhow::Error>((migration_report, local_history_report))
            },
            move |(), result| match result {
                Ok((migration_report, local_history_report)) => {
                    if migration_report.completed > 0 {
                        tracing::info!(
                            "Recovered {} pending migration kind(s)",
                            migration_report.completed
                        );
                    }
                    if local_history_report.reconciled_lineages > 0 {
                        tracing::info!(
                            "Reconciled {} local-history lineage(s)",
                            local_history_report.reconciled_lineages
                        );
                    }
                    if local_history_report.has_deferred_work() {
                        tracing::warn!(
                            "Deferred local-history reconciliation after scanning {} lineage(s)",
                            local_history_report.scanned_lineages
                        );
                    }
                    if !migration_report.diagnostics.is_empty()
                        || !local_history_report.diagnostics.is_empty()
                        || local_history_report.has_deferred_work()
                    {
                        for diagnostic in &migration_report.diagnostics {
                            tracing::warn!(
                                "Migration recovery {} generation {}: {}",
                                diagnostic.kind.label(),
                                diagnostic.generation,
                                diagnostic.message
                            );
                        }
                        for diagnostic in &local_history_report.diagnostics {
                            tracing::warn!(
                                "Local-history recovery diagnostic: {}",
                                diagnostic.summary()
                            );
                        }
                        if let Some(window) = window_weak.upgrade() {
                            window.publish_status_message(
                                "Some rename recovery work still needs attention",
                                MessageKind::Warning,
                            );
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!("Failed to reconcile pending migrations: {error}");
                    if let Some(window) = window_weak.upgrade() {
                        window.publish_status_message(
                            "Rename recovery state could not be checked",
                            MessageKind::Warning,
                        );
                    }
                }
            },
        );
    }
    fn trace_browse_recovery_diagnostics(diagnostics: &[RecoveryDiagnostic]) {
        for diagnostic in diagnostics {
            tracing::warn!("{}", diagnostic.summary());
        }
    }
    /// Snapshot open saved-editor note state without touching the filesystem.
    ///
    /// This runs on the GTK main thread because `bookmark_records()` reads the
    /// live `GtkSourceMark` projection. Sidecar loading and identity
    /// deduplication stay in the existing background browse task.
    fn open_editor_note_snapshots_bounded(
        &self,
        scope_folders: &[PathBuf],
        all_workspaces: &[WorkspaceConfig],
        max_snapshots_and_bookmarks: usize,
        max_retained_bytes: u64,
    ) -> OpenEditorNoteSnapshots {
        let tab_view = &self.imp().tab_view;
        let snapshot_size = std::mem::size_of::<PaletteOpenEditorNoteSnapshot>();
        let byte_limited_snapshots = usize::try_from(
            max_retained_bytes / u64::try_from(snapshot_size.max(1)).unwrap_or(u64::MAX),
        )
        .unwrap_or(usize::MAX);
        let page_count = usize::try_from(tab_view.n_pages()).unwrap_or(usize::MAX);
        let capacity = max_snapshots_and_bookmarks
            .min(page_count)
            .min(byte_limited_snapshots);
        let mut snapshots = Vec::with_capacity(capacity);
        let mut retained_bytes =
            u64::try_from(capacity.saturating_mul(snapshot_size)).unwrap_or(u64::MAX);
        let mut retained_bookmarks = 0usize;
        let mut truncated = false;
        for index in 0..tab_view.n_pages() {
            let retained_items = snapshots.len().saturating_add(retained_bookmarks);
            if retained_items >= max_snapshots_and_bookmarks || snapshots.len() == capacity {
                truncated = true;
                break;
            }
            let page = tab_view.nth_page(index);
            let child = page.child();
            let Some(editor) = child.downcast_ref::<LushtextEditorPage>() else {
                continue;
            };
            let Some(path) = editor.file_path() else {
                continue;
            };
            let open_tab_source = (!palette_service::path_is_in_folders(&path, scope_folders))
                .then(|| palette_service::open_tab_source_for_path(all_workspaces, &path));
            let snapshot_heap_bytes =
                open_editor_snapshot_heap_bytes(&path, open_tab_source.as_ref());
            if retained_bytes.saturating_add(snapshot_heap_bytes) > max_retained_bytes {
                truncated = true;
                break;
            }
            let bookmark_byte_limit = max_retained_bytes
                .saturating_sub(retained_bytes)
                .saturating_sub(snapshot_heap_bytes);
            let (bookmarks, bookmark_bytes, bookmarks_truncated) = editor
                .bookmark_records_bounded_by_retained_bytes(
                    max_snapshots_and_bookmarks
                        .saturating_sub(retained_items)
                        .saturating_sub(1),
                    bookmark_byte_limit,
                );
            retained_bookmarks = retained_bookmarks.saturating_add(bookmarks.len());
            retained_bytes = retained_bytes
                .saturating_add(snapshot_heap_bytes)
                .saturating_add(bookmark_bytes);
            snapshots.push(PaletteOpenEditorNoteSnapshot {
                path,
                bookmarks,
                open_tab_source,
            });
            if bookmarks_truncated {
                truncated = true;
                break;
            }
        }
        OpenEditorNoteSnapshots {
            entries: snapshots,
            retained_bytes,
            truncated,
        }
    }
    /// Find an already-open saved editor for a concrete path.
    fn open_editor_for_path(&self, path: &Path) -> Option<LushtextEditorPage> {
        let tab_view = &self.imp().tab_view;
        for index in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(index);
            let child = page.child();
            let Some(editor) = child.downcast_ref::<LushtextEditorPage>() else {
                continue;
            };
            if editor.file_path().as_deref() == Some(path) {
                return Some(editor.clone());
            }
        }
        None
    }

    /// Return the active editor only when it has a stable saved file path.
    fn require_saved_editor(&self, missing_path_message: &str) -> Option<LushtextEditorPage> {
        let Some(editor) = self.active_editor() else {
            self.publish_status_message(missing_path_message, MessageKind::Warning);
            return None;
        };
        if editor.file_path().is_some() {
            return Some(editor);
        }

        self.publish_status_message(missing_path_message, MessageKind::Warning);
        None
    }

    /// Collect current workspace folders for bookmark and note workflows.
    fn workspace_folder_paths_for_notes(&self) -> Vec<PathBuf> {
        self.current_workspace_folder_paths()
    }

    /// Decide what `Open Folder Note...` can do in the current shared scope.
    fn current_folder_note_open_target(&self) -> FolderNoteOpenTarget {
        let workspaces_file = self.imp().sidebar.workspaces_file();
        let WorkspaceScope::Workspace(workspace_id) = workspaces_file.current_scope() else {
            return FolderNoteOpenTarget::AggregateScope;
        };
        workspaces_file
            .workspaces
            .into_iter()
            .find(|workspace| workspace.id == workspace_id)
            .map_or(
                FolderNoteOpenTarget::WorkspaceMissing,
                editors::folder_note_target_for_workspace,
            )
    }

    /// Return whether the header menu can start a folder-note workflow immediately.
    fn current_folder_note_action_available(&self) -> bool {
        matches!(
            self.current_folder_note_open_target(),
            FolderNoteOpenTarget::SingleFolder { .. } | FolderNoteOpenTarget::ChooseFolder { .. }
        )
    }

    /// Refresh the window-scoped Notes menu label and menu-only action state.
    ///
    /// The header button and `Browse Notes…` stay window-scoped so the browser
    /// can show workspace rows, open-tab rows, or its empty state even when no
    /// editor tab is active. Target-specific rows still use sensitivity below.
    ///
    /// The dedicated menu uses its own `notes-*` actions so it can become
    /// insensitive without disabling the existing shortcuts or command-palette
    /// commands that still rely on the workflow guards below.
    pub(super) fn refresh_notes_menu_state(&self) {
        let active_editor = self.active_editor();
        let saved_editor = active_editor
            .as_ref()
            .filter(|editor| editor.file_path().is_some());
        let bookmark_label = if saved_editor
            .as_ref()
            .is_some_and(|editor| editor.current_bookmark().is_some())
        {
            "Remove Bookmark"
        } else {
            "Add Bookmark"
        };

        if !self.notes_menu_uses_bookmark_label(bookmark_label) {
            self.rebuild_notes_menu(bookmark_label);
        }

        self.imp().notes_menu_button.set_visible(true);

        self.set_notes_menu_action_enabled("notes-toggle-bookmark", saved_editor.is_some());
        self.set_notes_menu_action_enabled("notes-open-document-note", saved_editor.is_some());
        self.set_notes_menu_action_enabled(
            "notes-open-folder-note",
            self.current_folder_note_action_available(),
        );
        self.set_notes_menu_action_enabled("notes-show-notes", true);
    }

    /// Check the existing menu model before replacing it during ordinary state refreshes.
    ///
    /// The menu is small, and avoiding no-op replacements keeps GTK's popup
    /// lifecycle stable if a refresh races with user activation.
    fn notes_menu_uses_bookmark_label(&self, bookmark_label: &'static str) -> bool {
        let Some(menu) = self.imp().notes_menu_button.menu_model() else {
            return false;
        };

        Self::menu_label_for_action(&menu, "win.notes-toggle-bookmark")
            .is_some_and(|label| label == bookmark_label)
    }

    /// Find the label for one action in a possibly sectioned menu model.
    ///
    /// Searching by action keeps the bookmark-label guard independent from the
    /// visual section order, which is allowed to change as the menu evolves.
    fn menu_label_for_action(model: &gio::MenuModel, action_name: &str) -> Option<String> {
        for index in 0..model.n_items() {
            let action = model
                .item_attribute_value(index, "action", Some(glib::VariantTy::STRING))
                .and_then(|variant| variant.get::<String>());
            if action.as_deref() == Some(action_name) {
                return model
                    .item_attribute_value(index, "label", Some(glib::VariantTy::STRING))
                    .and_then(|variant| variant.get::<String>());
            }

            for link_name in ["section", "submenu"] {
                if let Some(link) = model.item_link(index, link_name)
                    && let Some(label) = Self::menu_label_for_action(&link, action_name)
                {
                    return Some(label);
                }
            }
        }
        None
    }

    /// Rebuild the small header-bar Notes menu so its bookmark row can use
    /// the active cursor context without disabling the expert command actions.
    fn rebuild_notes_menu(&self, bookmark_label: &'static str) {
        let menu = gio::Menu::new();

        let browse_section = gio::Menu::new();
        browse_section.append(Some("Browse Notes…"), Some("win.notes-show-notes"));
        menu.append_section(None, &browse_section);

        let document_section = gio::Menu::new();
        document_section.append(Some(bookmark_label), Some("win.notes-toggle-bookmark"));
        document_section.append(
            Some("Open Document Note…"),
            Some("win.notes-open-document-note"),
        );
        menu.append_section(None, &document_section);

        let workspace_section = gio::Menu::new();
        workspace_section.append(
            Some("Open Folder Note…"),
            Some("win.notes-open-folder-note"),
        );
        menu.append_section(None, &workspace_section);

        self.imp().notes_menu_button.set_menu_model(Some(&menu));
    }

    /// Update one Notes-menu-only action without affecting shortcut actions.
    fn set_notes_menu_action_enabled(&self, action_name: &str, enabled: bool) {
        if let Some(action) = self.lookup_action(action_name)
            && let Some(simple) = action.downcast_ref::<gio::SimpleAction>()
        {
            simple.set_enabled(enabled);
        }
    }
}

/// Build one compact close affordance for browser-style dialogs.
fn build_dialog_close_button(dialog: &libadwaita::Dialog) -> gtk4::Button {
    let close_button = gtk4::Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text("Close")
        .build();
    accessibility::set_labelled_description(
        &close_button,
        "Close",
        "Close this dialog and return to the editor",
    );
    let dialog_weak = dialog.downgrade();
    close_button.connect_clicked(move |_| {
        if let Some(dialog) = dialog_weak.upgrade() {
            dialog.close();
        }
    });
    close_button
}

/// Close dialog content on Escape even when the focused child owns key handling.
fn install_dialog_escape_close(dialog: &libadwaita::Dialog, widget: &impl IsA<gtk4::Widget>) {
    let controller = gtk4::EventControllerKey::new();
    controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let dialog_weak = dialog.downgrade();
    controller.connect_key_pressed(move |_, key, _, _| {
        if key != gtk4::gdk::Key::Escape {
            return glib::Propagation::Proceed;
        }
        if let Some(dialog) = dialog_weak.upgrade() {
            dialog.close();
        }
        glib::Propagation::Stop
    });
    widget.as_ref().add_controller(controller);
}

/// Defer focus until after `AdwDialog::present()` realizes its child tree.
fn focus_after_present(widget: &impl IsA<gtk4::Widget>) {
    let widget_weak = widget.as_ref().downgrade();
    glib::idle_add_local_once(move || {
        if let Some(widget) = widget_weak.upgrade() {
            widget.grab_focus();
        }
    });
}

/// Build the empty-state label shown when a browser search has no matches.
#[must_use]
fn empty_browser_label(text: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.set_halign(gtk4::Align::Center);
    label.add_css_class("dim-label");
    accessibility::set_role(&label, gtk4::AccessibleRole::Status);
    accessibility::set_label(&label, text);
    label
}
