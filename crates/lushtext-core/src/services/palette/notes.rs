// SPDX-License-Identifier: GPL-3.0-or-later

//! GTK-free note row construction and search for command-palette consumers.
//!
//! The command palette and Browse Notes surface both need the same note taxonomy:
//! bookmarks, folder notes, document notes, and saved open-tab notes. This
//! service owns that source policy while GTK adapters decide how to render and
//! activate the rows.

use anyhow::{Context, Result};
#[cfg(test)]
use std::collections::HashMap;
use std::collections::HashSet;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::model::note::{RichNoteBody, note_preview_line};
use crate::model::palette::{
    PaletteNoteCategory, PaletteNoteEntry, PaletteNoteTarget, PaletteOpenEditorNoteSnapshot,
    PaletteOpenTabSource,
};
use crate::model::workspace::{WorkspaceConfig, WorkspaceScopeSnapshot};
use crate::model::{bookmark::BookmarkDocument, document_note::DocumentNoteDocument};
use crate::services::filesystem::metadata as fs_metadata;
use crate::services::fuzzy::FuzzyQuery;
use crate::services::recovery_metadata::{RecoveryDiagnostic, RecoveryMetadataClass};
use crate::services::{
    bookmark_service, document_note_service, file_tree, folder_note_service, note_storage,
};

use super::fuzzy::search_items_cancellable;
#[cfg(any(test, feature = "property-tests"))]
use super::fuzzy::search_items_full_sort_reference;
use super::runtime::{
    PaletteSearchCancellation, PaletteSearchCoordinator, PaletteSearchMetrics, PaletteSearchOutcome,
};

#[cfg(feature = "test-utils")]
static NOTES_BROWSER_QUERY_DELAY_MS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
#[cfg(feature = "test-utils")]
static NOTE_SOURCE_DELAY_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Maximum UTF-8 bytes passed to nucleo for one note metadata field.
const MAX_NOTE_FUZZY_SCORE_BYTES: usize = 4 * 1024;
/// Sound upper bound returned by one `nucleo_matcher::pattern::Atom` score.
///
/// The pinned matcher returns `Option<u16>` for a single atom. Keeping the
/// bound beside the field policy makes the body-pruning proof independent of
/// observed scores or host timing.
const MAX_NOTE_FIELD_FUZZY_SCORE: u32 = u16::MAX as u32;
/// Character interval between cancellation checks while scanning note text.
const NOTE_TEXT_CANCEL_CHECK_INTERVAL: usize = 1_024;
/// Maximum note and bookmark rows retained for command-palette search.
pub const MAX_PALETTE_NOTE_ENTRIES: usize = 10_000;
/// Maximum aggregate searchable UTF-8 bytes retained for palette note rows.
pub const MAX_PALETTE_NOTE_TEXT_BYTES: usize = 64 * 1024 * 1024;
/// Maximum complete heap graph retained by one palette note source.
pub const MAX_PALETTE_NOTE_RETAINED_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum complete sidecar-path ownership retained by one source scan.
pub const MAX_PALETTE_NOTE_SIDECAR_PATH_BYTES: u64 = 8 * 1024 * 1024;
/// Maximum concurrently retained final rows plus construction-only scratch.
///
/// This preserves the 64 MiB final source while leaving room for one maximum
/// 16 MiB recovery-metadata input and bounded path, diagnostic, and category
/// construction ownership.
pub const MAX_PALETTE_NOTE_CONSTRUCTION_BYTES: u64 = 96 * 1024 * 1024;
/// Conservative parsed-model heap expansion relative to compact JSON input.
const NOTE_SIDECAR_MODEL_EXPANSION_MULTIPLIER: u64 = 4;
/// Fixed allowance for one returned recovery diagnostic graph.
const NOTE_SIDECAR_DIAGNOSTIC_RESERVATION_BYTES: u64 = 64 * 1024;
/// Maximum recovery diagnostics retained with one bounded palette source.
const MAX_PALETTE_NOTE_DIAGNOSTICS: usize = 1_024;

/// Explicit aggregate limits for one immutable note-search source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteSourceLimits {
    /// Maximum admitted note and bookmark rows.
    pub entries: usize,
    /// Maximum aggregate searchable UTF-8 bytes.
    pub searchable_text_bytes: usize,
    /// Maximum complete retained heap graph, including activation targets.
    pub retained_bytes: u64,
    /// Maximum sidecar candidates retained by each directory scan.
    pub sidecar_entries: usize,
    /// Maximum complete path ownership retained by each sidecar scan.
    pub sidecar_path_bytes: u64,
    /// Maximum aggregate concurrently retained construction graph.
    pub construction_bytes: u64,
    /// Maximum recovery diagnostics retained for UI reporting.
    pub diagnostics: usize,
}

/// Command-palette note-source policy.
pub const PALETTE_NOTE_SOURCE_LIMITS: NoteSourceLimits = NoteSourceLimits {
    entries: MAX_PALETTE_NOTE_ENTRIES,
    searchable_text_bytes: MAX_PALETTE_NOTE_TEXT_BYTES,
    retained_bytes: MAX_PALETTE_NOTE_RETAINED_BYTES,
    sidecar_entries: MAX_PALETTE_NOTE_ENTRIES,
    sidecar_path_bytes: MAX_PALETTE_NOTE_SIDECAR_PATH_BYTES,
    construction_bytes: MAX_PALETTE_NOTE_CONSTRUCTION_BYTES,
    diagnostics: MAX_PALETTE_NOTE_DIAGNOSTICS,
};

/// Inventory mode shared by bounded source construction and browser queries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NotesBrowserMode {
    /// Bookmarks, folder notes, document notes, and open-tab rows.
    #[default]
    AllNotes,
    /// Bookmark activation targets only, including live scoped editor state.
    Bookmarks,
}

impl NotesBrowserMode {
    #[must_use]
    fn includes_entry(self, entry: &PaletteNoteEntry) -> bool {
        self == Self::AllNotes || matches!(entry.target, PaletteNoteTarget::Bookmark { .. })
    }
}

/// Compact text request retained by the Notes browser's latest-query slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotesBrowserQueryRequest {
    /// User-entered filter text captured on GTK.
    pub query: String,
    /// Surface mode captured with the query generation.
    pub mode: NotesBrowserMode,
}

/// Bounded ordered indexes produced by one Notes browser query.
#[derive(Debug, PartialEq, Eq)]
pub struct NotesBrowserQueryResult {
    /// Source indexes in deterministic admission order.
    pub matching_indices: Vec<usize>,
    /// Whether at least one later match was omitted by the render cap.
    pub truncated: bool,
}

/// One-active/one-latest ownership for browser query requests.
pub type NotesBrowserQueryCoordinator = PaletteSearchCoordinator<NotesBrowserQueryRequest>;

/// Delay Notes browser query workers for deterministic supersession tests.
#[cfg(feature = "test-utils")]
pub fn set_notes_browser_query_delay_for_test(delay_ms: u64) {
    NOTES_BROWSER_QUERY_DELAY_MS.store(delay_ms, std::sync::atomic::Ordering::Release);
}

/// Delay bounded note-source workers for deterministic disposal tests.
#[cfg(feature = "test-utils")]
pub fn set_note_source_delay_for_test(delay_ms: u64) {
    NOTE_SOURCE_DELAY_MS.store(delay_ms, std::sync::atomic::Ordering::Release);
}

fn delay_notes_browser_query_for_test() {
    #[cfg(feature = "test-utils")]
    {
        let delay_ms = NOTES_BROWSER_QUERY_DELAY_MS.load(std::sync::atomic::Ordering::Acquire);
        if delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
    }
}

fn delay_note_source_for_test() {
    #[cfg(feature = "test-utils")]
    {
        let delay_ms = NOTE_SOURCE_DELAY_MS.load(std::sync::atomic::Ordering::Acquire);
        if delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
    }
}

/// Typed reason a complete note source intentionally omitted later rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteSourceTruncationReason {
    /// The aggregate row budget was exhausted.
    EntryLimit,
    /// The aggregate searchable-text byte budget was exhausted.
    TextByteLimit,
    /// The complete row and activation-target heap budget was exhausted.
    RetainedByteLimit,
    /// A sidecar directory contained more candidates than the bounded scan retained.
    SidecarLimit,
    /// Complete sidecar paths reached their conservative byte ceiling.
    SidecarPathByteLimit,
    /// Concurrent construction ownership reached its aggregate byte ceiling.
    ConstructionByteLimit,
    /// Recovery diagnostics exceeded their bounded evidence budget.
    DiagnosticLimit,
    /// Live editor paths or bookmark metadata exceeded the pre-admission request budget.
    OpenEditorSnapshotLimit,
}

/// Bounded source-construction evidence without note or diagnostic contents.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NoteSourceMetrics {
    /// Number of admitted note rows.
    pub retained_entries: usize,
    /// Aggregate bytes reachable through admitted searchable metadata and bodies.
    pub retained_searchable_bytes: usize,
    /// Complete retained bytes including row shells and activation targets.
    pub retained_bytes: u64,
    /// Number of sidecars loaded one at a time.
    pub loaded_sidecars: usize,
    /// Sidecar path bytes currently retained during construction.
    pub current_sidecar_path_bytes: u64,
    /// Highest sidecar path ownership observed during construction.
    pub peak_sidecar_path_bytes: u64,
    /// Aggregate final-row and scratch bytes currently retained during construction.
    pub current_construction_bytes: u64,
    /// Highest aggregate construction ownership observed.
    pub peak_construction_bytes: u64,
    /// Highest aggregate row count retained during construction.
    pub peak_retained_entries: usize,
    /// Stable reasons why later source material was omitted.
    pub truncation_reasons: Vec<NoteSourceTruncationReason>,
}

/// Complete palette note source plus diagnostics from partially recovered sidecars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteNoteSourceLoad {
    /// Rows safe to show in note search surfaces.
    pub entries: Vec<PaletteNoteEntry>,
    /// Recovery diagnostics for malformed or unreadable note/bookmark sidecars.
    pub diagnostics: Vec<RecoveryDiagnostic>,
    /// Typed boundedness evidence safe to surface without note contents.
    pub truncation_reasons: Vec<NoteSourceTruncationReason>,
}

/// Typed terminal result from cancellable palette note-source construction.
#[derive(Debug)]
pub enum PaletteNoteSourceOutcome {
    /// The source reached a deterministic terminal boundary and may be published.
    Complete {
        /// Bounded row and recovery payload.
        load: PaletteNoteSourceLoad,
        /// Scalar ownership and truncation evidence.
        metrics: NoteSourceMetrics,
    },
    /// Supersession stopped construction; no partial rows may be published.
    Cancelled {
        /// Scalar work completed before cancellation was observed.
        metrics: NoteSourceMetrics,
    },
}

/// Compact scope/editor snapshot retained by the latest note-source slot.
#[derive(Clone, Debug)]
pub struct NoteSourceRefreshRequest {
    /// App-data root used for note sidecars.
    pub data_dir: PathBuf,
    /// Immutable workspace selection captured on GTK.
    pub scope_snapshot: WorkspaceScopeSnapshot,
    /// Bounded live-editor metadata captured on GTK.
    pub open_editor_snapshots: Arc<[PaletteOpenEditorNoteSnapshot]>,
    /// Whether later live-editor metadata was omitted before worker admission.
    pub open_editor_snapshots_truncated: bool,
    /// Inventory mode captured with this source generation.
    pub mode: NotesBrowserMode,
    /// Aggregate admission policy owned by the requesting surface.
    pub limits: NoteSourceLimits,
}

/// One request admitted as the sole active note-source refresh.
#[derive(Debug)]
pub struct NoteSourceRefreshStart {
    /// Monotonic acceptance generation.
    pub generation: u64,
    /// Compact request owned by the active worker.
    pub request: NoteSourceRefreshRequest,
    /// Cooperative supersession token.
    pub cancellation: PaletteSearchCancellation,
}

#[derive(Debug)]
struct ActiveNoteSourceRefresh {
    generation: u64,
    cancellation: PaletteSearchCancellation,
}

#[derive(Debug)]
struct PendingNoteSourceRefresh {
    generation: u64,
    request: NoteSourceRefreshRequest,
}

/// Scalar ownership evidence for note-source refresh tests and readiness.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoteSourceRefreshCoordinatorSnapshot {
    /// Active worker count, always zero or one.
    pub active: usize,
    /// Retained latest-request count, always zero or one.
    pub pending: usize,
    /// Total workers started over the coordinator lifetime.
    pub started: usize,
    /// Total successful cancellation transitions requested.
    pub cancellation_requests: usize,
}

/// Retain at most one active note refresh and one latest compact request.
#[derive(Debug, Default)]
pub struct NoteSourceRefreshCoordinator {
    current_generation: u64,
    active: Option<ActiveNoteSourceRefresh>,
    pending: Option<PendingNoteSourceRefresh>,
    snapshot: NoteSourceRefreshCoordinatorSnapshot,
}

impl NoteSourceRefreshCoordinator {
    /// Submit a compact refresh, starting immediately or replacing the pending slot.
    pub fn submit(&mut self, request: NoteSourceRefreshRequest) -> Option<NoteSourceRefreshStart> {
        self.current_generation = self.current_generation.wrapping_add(1);
        let generation = self.current_generation;
        if let Some(active) = self.active.as_ref() {
            if active.cancellation.cancel() {
                self.snapshot.cancellation_requests =
                    self.snapshot.cancellation_requests.saturating_add(1);
            }
            self.pending = Some(PendingNoteSourceRefresh {
                generation,
                request,
            });
            None
        } else {
            Some(self.start(generation, request))
        }
    }

    /// Finish the matching active refresh and start the latest pending request, if any.
    pub fn finish(&mut self, generation: u64) -> Option<NoteSourceRefreshStart> {
        if self.active.as_ref().map(|active| active.generation) != Some(generation) {
            return None;
        }
        self.active = None;
        self.pending
            .take()
            .map(|pending| self.start(pending.generation, pending.request))
    }

    /// Reject current results, cancel active work, and discard the pending request.
    pub fn invalidate(&mut self) {
        self.current_generation = self.current_generation.wrapping_add(1);
        if let Some(active) = self.active.as_ref()
            && active.cancellation.cancel()
        {
            self.snapshot.cancellation_requests =
                self.snapshot.cancellation_requests.saturating_add(1);
        }
        self.pending = None;
    }

    #[must_use]
    /// Return whether this generation is still the only publishable result.
    pub fn is_current(&self, generation: u64) -> bool {
        self.current_generation == generation
    }

    #[must_use]
    /// Return whether active or pending work remains.
    pub fn has_work(&self) -> bool {
        self.active.is_some() || self.pending.is_some()
    }

    #[must_use]
    /// Return scalar ownership evidence without retaining request payloads.
    pub fn snapshot(&self) -> NoteSourceRefreshCoordinatorSnapshot {
        NoteSourceRefreshCoordinatorSnapshot {
            active: usize::from(self.active.is_some()),
            pending: usize::from(self.pending.is_some()),
            ..self.snapshot
        }
    }

    fn start(
        &mut self,
        generation: u64,
        request: NoteSourceRefreshRequest,
    ) -> NoteSourceRefreshStart {
        let cancellation = PaletteSearchCancellation::default();
        self.active = Some(ActiveNoteSourceRefresh {
            generation,
            cancellation: cancellation.clone(),
        });
        self.snapshot.started = self.snapshot.started.saturating_add(1);
        NoteSourceRefreshStart {
            generation,
            request,
            cancellation,
        }
    }
}

/// Load all note rows covered by the current workspace scope.
///
/// # Errors
///
/// Returns an error only when a sidecar directory cannot be scanned or a
/// workspace folder identity cannot be resolved.
#[cfg(test)]
pub fn load_note_entries_for_scope(
    data_dir: &Path,
    scope_snapshot: &WorkspaceScopeSnapshot,
    open_editor_snapshots: Vec<PaletteOpenEditorNoteSnapshot>,
) -> Result<PaletteNoteSourceLoad> {
    let visible_workspaces = scope_snapshot.visible_workspaces();
    let scope_folders = scope_snapshot.folder_paths();
    let folder_notes = if visible_workspaces.is_empty() {
        folder_note_service::FolderNoteListing {
            notes: Vec::new(),
            diagnostics: Vec::new(),
        }
    } else {
        folder_note_service::list_folder_notes_for_scope_recovering(
            data_dir,
            visible_workspaces,
            scope_snapshot.scope(),
        )?
    };
    let bookmark_listing = if scope_folders.is_empty() {
        bookmark_service::WorkspaceBookmarkListing {
            bookmarks: Vec::new(),
            diagnostics: Vec::new(),
        }
    } else {
        bookmark_service::list_workspace_bookmarks_recovering(data_dir, scope_folders)?
    };
    let live_bookmarks = open_editor_snapshots
        .iter()
        .filter(|snapshot| snapshot.open_tab_source.is_none())
        .map(|snapshot| PaletteOpenEditorNoteSnapshot {
            path: snapshot.path.clone(),
            bookmarks: snapshot.bookmarks.clone(),
            open_tab_source: None,
        })
        .collect();
    let bookmarks = merge_live_bookmark_snapshots(bookmark_listing.bookmarks, live_bookmarks);
    let document_notes = if scope_folders.is_empty() {
        document_note_service::WorkspaceDocumentNoteListing {
            notes: Vec::new(),
            diagnostics: Vec::new(),
        }
    } else {
        document_note_service::list_workspace_document_notes_recovering(data_dir, scope_folders)?
    };

    let mut diagnostics = Vec::new();
    diagnostics.extend(folder_notes.diagnostics);
    diagnostics.extend(bookmark_listing.diagnostics);
    diagnostics.extend(document_notes.diagnostics);
    let entries = build_note_entries(
        visible_workspaces,
        bookmarks,
        folder_notes.notes,
        document_notes.notes,
        open_editor_snapshots,
        data_dir,
    );

    Ok(PaletteNoteSourceLoad {
        entries,
        diagnostics,
        truncation_reasons: Vec::new(),
    })
}

/// Load the command-palette note inventory with aggregate admission and cancellation.
///
/// Sidecars are loaded one at a time in deterministic source order. A rejected
/// body is dropped before the next sidecar is read, so the returned inventory is
/// the only retained body set after this function completes.
///
/// # Errors
///
/// Returns an error when a covered sidecar directory cannot be scanned or a
/// workspace folder identity cannot be resolved.
pub fn load_palette_note_entries_for_scope(
    data_dir: &Path,
    scope_snapshot: &WorkspaceScopeSnapshot,
    open_editor_snapshots: &[PaletteOpenEditorNoteSnapshot],
    cancellation: &PaletteSearchCancellation,
) -> Result<PaletteNoteSourceOutcome> {
    load_note_entries_bounded_for_scope(
        data_dir,
        scope_snapshot,
        open_editor_snapshots,
        false,
        NotesBrowserMode::AllNotes,
        PALETTE_NOTE_SOURCE_LIMITS,
        cancellation,
    )
}

/// Load one immutable note inventory under a caller-owned aggregate policy.
///
/// This is shared by the command palette and Browse Notes so sidecar ordering,
/// canonical de-duplication, cancellation, and typed truncation stay identical.
///
/// # Errors
///
/// Returns an error when a covered sidecar directory cannot be scanned or a
/// workspace folder identity cannot be resolved.
pub fn load_note_entries_bounded_for_scope(
    data_dir: &Path,
    scope_snapshot: &WorkspaceScopeSnapshot,
    open_editor_snapshots: &[PaletteOpenEditorNoteSnapshot],
    open_editor_snapshots_truncated: bool,
    mode: NotesBrowserMode,
    limits: NoteSourceLimits,
    cancellation: &PaletteSearchCancellation,
) -> Result<PaletteNoteSourceOutcome> {
    delay_note_source_for_test();
    if cancellation.is_cancelled() {
        return Ok(PaletteNoteSourceOutcome::Cancelled {
            metrics: NoteSourceMetrics::default(),
        });
    }
    let visible_workspaces = scope_snapshot.visible_workspaces();
    let scope_folders = scope_snapshot.folder_paths();
    let mut admission = NoteSourceAdmission::with_limits(limits);
    if open_editor_snapshots_truncated {
        admission.add_truncation(NoteSourceTruncationReason::OpenEditorSnapshotLimit);
    }

    let mut live_scoped_document_ids = HashSet::new();
    for snapshot in open_editor_snapshots
        .iter()
        .filter(|snapshot| snapshot.open_tab_source.is_none())
    {
        if let Ok(identity) = bookmark_service::resolve_document_identity(&snapshot.path) {
            live_scoped_document_ids.insert(identity.sidecar_id);
        }
    }
    let live_identity_bytes = string_set_retained_byte_weight(&live_scoped_document_ids);
    if !admission.try_charge_construction(live_identity_bytes) {
        return Ok(admission.complete());
    }

    if !scope_folders.is_empty() {
        let canonical_folders = note_storage::canonicalize_folders(scope_folders);
        let canonical_folder_bytes = path_slice_retained_byte_weight(&canonical_folders);
        if !admission.try_charge_construction(canonical_folder_bytes) {
            return Ok(admission.complete());
        }
        let dir = bookmark_service::bookmarks_dir(data_dir);
        let BoundedSidecarEntries::Admitted {
            entries: sidecars,
            retained_bytes: sidecar_path_bytes,
        } = bounded_sidecar_entries(&dir, limits.sidecar_entries, cancellation, &mut admission)?
        else {
            return Ok(admission.complete());
        };
        if cancellation.is_cancelled() {
            return Ok(admission.cancelled());
        }
        for entry in sidecars {
            if cancellation.is_cancelled() {
                return Ok(admission.cancelled());
            }
            let Some(parse) =
                reserve_sidecar_parse(std::slice::from_ref(&entry.path), &mut admission)?
            else {
                return Ok(admission.complete());
            };
            let load = note_storage::load_json_file_recovering_with_max_bytes::<BookmarkDocument>(
                data_dir,
                &entry.path,
                RecoveryMetadataClass::BookmarkSidecar,
                parse.max_read_bytes,
            );
            let document_bytes = load
                .value
                .as_ref()
                .map_or(0, BookmarkDocument::retained_heap_byte_weight);
            if !admit_parsed_sidecar(&parse, document_bytes, &load.diagnostics, &mut admission) {
                return Ok(admission.complete());
            }
            let Some(document) = load.value else {
                continue;
            };
            let outcome = admission.with_construction_charge(document_bytes, |admission| {
                if !note_storage::matches_any_folder(&document.identity, &canonical_folders)
                    || live_scoped_document_ids.contains(&document.identity.sidecar_id)
                {
                    return ControlFlow::Continue(());
                }
                let path = document.identity.display_path;
                let Some(workspace) = workspace_for_path(visible_workspaces, &path) else {
                    return ControlFlow::Continue(());
                };
                let workspace_folder =
                    workspace_folder_for_path(workspace, &path).unwrap_or_else(|| path.clone());
                let source = PaletteNoteDocumentSource::Workspace {
                    workspace_name: workspace.name.clone(),
                    workspace_folder,
                };
                for bookmark in document.bookmarks {
                    if !admission.admit(bookmark_entry(
                        &source,
                        path.clone(),
                        bookmark.line,
                        bookmark.label.as_deref(),
                    )) {
                        return ControlFlow::Break(());
                    }
                }
                ControlFlow::Continue(())
            });
            if !matches!(outcome, ChargeOutcome::Ran) {
                return Ok(admission.complete());
            }
        }
        admission.release_sidecar_paths(sidecar_path_bytes);
        admission.release_construction(canonical_folder_bytes);
    }
    admission.release_construction(live_identity_bytes);

    for snapshot in open_editor_snapshots
        .iter()
        .filter(|snapshot| snapshot.open_tab_source.is_none())
    {
        let Some(workspace) = workspace_for_path(visible_workspaces, &snapshot.path) else {
            continue;
        };
        let workspace_folder = workspace_folder_for_path(workspace, &snapshot.path)
            .unwrap_or_else(|| snapshot.path.clone());
        let source = PaletteNoteDocumentSource::Workspace {
            workspace_name: workspace.name.clone(),
            workspace_folder,
        };
        for bookmark in &snapshot.bookmarks {
            if !admission.admit(bookmark_entry(
                &source,
                snapshot.path.clone(),
                bookmark.line,
                bookmark.label.as_deref(),
            )) {
                return Ok(admission.complete());
            }
        }
    }

    if mode == NotesBrowserMode::AllNotes {
        for workspace in visible_workspaces {
            for folder in workspace.folder_paths() {
                if cancellation.is_cancelled() {
                    return Ok(admission.cancelled());
                }
                let (identity, paths) =
                    folder_note_service::sidecar_lookup_for_folder(data_dir, &folder)?;
                let Some(parse) = reserve_sidecar_parse(&paths, &mut admission)? else {
                    return Ok(admission.complete());
                };
                let load = folder_note_service::load_for_identity_recovering(
                    data_dir,
                    &identity,
                    parse.max_read_bytes,
                );
                let document_bytes = load.document.as_ref().map_or(
                    0,
                    crate::model::folder_note::FolderNoteDocument::retained_heap_byte_weight,
                );
                if !admit_parsed_sidecar(&parse, document_bytes, &load.diagnostics, &mut admission)
                {
                    return Ok(admission.complete());
                }
                if let Some(document) = load.document {
                    let outcome = admission.with_construction_charge(document_bytes, |admission| {
                        if admission.admit(folder_note_entry(
                            workspace.name.clone(),
                            folder,
                            document.note,
                        )) {
                            ControlFlow::Continue(())
                        } else {
                            ControlFlow::Break(())
                        }
                    });
                    if !matches!(outcome, ChargeOutcome::Ran) {
                        return Ok(admission.complete());
                    }
                }
            }
        }
    }

    if mode == NotesBrowserMode::AllNotes && !scope_folders.is_empty() {
        let canonical_folders = note_storage::canonicalize_folders(scope_folders);
        let canonical_folder_bytes = path_slice_retained_byte_weight(&canonical_folders);
        if !admission.try_charge_construction(canonical_folder_bytes) {
            return Ok(admission.complete());
        }
        let dir = document_note_service::document_notes_dir(data_dir);
        let BoundedSidecarEntries::Admitted {
            entries: sidecars,
            retained_bytes: sidecar_path_bytes,
        } = bounded_sidecar_entries(&dir, limits.sidecar_entries, cancellation, &mut admission)?
        else {
            return Ok(admission.complete());
        };
        if cancellation.is_cancelled() {
            return Ok(admission.cancelled());
        }
        for entry in sidecars {
            if cancellation.is_cancelled() {
                return Ok(admission.cancelled());
            }
            let Some(parse) =
                reserve_sidecar_parse(std::slice::from_ref(&entry.path), &mut admission)?
            else {
                return Ok(admission.complete());
            };
            let load = note_storage::load_json_file_recovering_with_max_bytes::<DocumentNoteDocument>(
                data_dir,
                &entry.path,
                RecoveryMetadataClass::DocumentNoteSidecar,
                parse.max_read_bytes,
            );
            let document_bytes = load
                .value
                .as_ref()
                .map_or(0, DocumentNoteDocument::retained_heap_byte_weight);
            if !admit_parsed_sidecar(&parse, document_bytes, &load.diagnostics, &mut admission) {
                return Ok(admission.complete());
            }
            let Some(document) = load.value else {
                continue;
            };
            let outcome = admission.with_construction_charge(document_bytes, |admission| {
                if !note_storage::matches_any_folder(&document.identity, &canonical_folders) {
                    return ControlFlow::Continue(());
                }
                let path = document.identity.display_path;
                let Some(workspace) = workspace_for_path(visible_workspaces, &path) else {
                    return ControlFlow::Continue(());
                };
                let workspace_folder =
                    workspace_folder_for_path(workspace, &path).unwrap_or_else(|| path.clone());
                let source = PaletteNoteDocumentSource::Workspace {
                    workspace_name: workspace.name.clone(),
                    workspace_folder,
                };
                if admission.admit(document_note_entry(&source, path, document.note)) {
                    ControlFlow::Continue(())
                } else {
                    ControlFlow::Break(())
                }
            });
            if !matches!(outcome, ChargeOutcome::Ran) {
                return Ok(admission.complete());
            }
        }
        admission.release_sidecar_paths(sidecar_path_bytes);
        admission.release_construction(canonical_folder_bytes);
    }

    for snapshot in open_editor_snapshots {
        let Some(open_tab_source) = snapshot.open_tab_source.clone() else {
            continue;
        };
        if cancellation.is_cancelled() {
            return Ok(admission.cancelled());
        }
        let source = PaletteNoteDocumentSource::OpenTab(open_tab_source);
        for bookmark in &snapshot.bookmarks {
            if !admission.admit(bookmark_entry(
                &source,
                snapshot.path.clone(),
                bookmark.line,
                bookmark.label.as_deref(),
            )) {
                return Ok(admission.complete());
            }
        }
        if mode == NotesBrowserMode::AllNotes
            && let Ok(identity) = note_storage::resolve_document_identity(&snapshot.path)
        {
            let sidecar_path = document_note_service::document_notes_dir(data_dir)
                .join(note_storage::sidecar_filename(&identity.sidecar_id));
            let Some(parse) = reserve_sidecar_parse(&[sidecar_path], &mut admission)? else {
                return Ok(admission.complete());
            };
            let document = document_note_service::load_for_identity(
                data_dir,
                &identity,
                parse.max_read_bytes,
            )?;
            let document_bytes = document
                .as_ref()
                .map_or(0, DocumentNoteDocument::retained_heap_byte_weight);
            if !admit_parsed_sidecar(&parse, document_bytes, &[], &mut admission) {
                return Ok(admission.complete());
            }
            let Some(document) = document else {
                continue;
            };
            let outcome = admission.with_construction_charge(document_bytes, |admission| {
                if admission.admit(document_note_entry(
                    &source,
                    snapshot.path.clone(),
                    document.note,
                )) {
                    ControlFlow::Continue(())
                } else {
                    ControlFlow::Break(())
                }
            });
            if !matches!(outcome, ChargeOutcome::Ran) {
                return Ok(admission.complete());
            }
        }
    }

    Ok(admission.complete())
}

enum BoundedSidecarEntries {
    Admitted {
        entries: Vec<file_tree::DirectoryEntry>,
        retained_bytes: u64,
    },
    LimitReached,
}

/// Result of one `with_construction_charge` scope.
#[derive(Debug, PartialEq, Eq)]
enum ChargeOutcome<T> {
    /// The body ran to completion; the charge was released by scope.
    Ran,
    /// The body exited early with a value; the charge was still released.
    Broke(T),
    /// The budget rejected the charge, so the body never ran.
    BudgetExhausted,
}

struct SidecarParseReservation {
    charged_bytes: u64,
    model_byte_limit: u64,
    max_read_bytes: u64,
}

fn reserve_sidecar_parse(
    paths: &[PathBuf],
    admission: &mut NoteSourceAdmission,
) -> Result<Option<SidecarParseReservation>> {
    let default_max = crate::services::recovery_metadata::DEFAULT_MAX_METADATA_BYTES;
    let mut readable_bytes = 0u64;
    let mut has_oversized = false;
    for path in paths {
        match fs_metadata::file_facts(path) {
            Ok(facts) if facts.byte_size <= default_max => {
                readable_bytes = readable_bytes.max(facts.byte_size);
            }
            Ok(_) => has_oversized = true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            // The recovery-aware loader owns unreadable-sidecar diagnostics.
            // Keep this preflight diagnostic-only when metadata is unavailable.
            Err(_) => {}
        }
    }
    let model_byte_limit = readable_bytes.saturating_mul(NOTE_SIDECAR_MODEL_EXPANSION_MULTIPLIER);
    let charged_bytes = readable_bytes
        .saturating_add(model_byte_limit)
        .saturating_add(NOTE_SIDECAR_DIAGNOSTIC_RESERVATION_BYTES);
    if !admission.try_charge_construction(charged_bytes) {
        return Ok(None);
    }
    Ok(Some(SidecarParseReservation {
        charged_bytes,
        model_byte_limit,
        max_read_bytes: if readable_bytes == 0 && has_oversized {
            default_max
        } else {
            readable_bytes
        },
    }))
}

fn admit_parsed_sidecar(
    reservation: &SidecarParseReservation,
    document_bytes: u64,
    diagnostics: &[RecoveryDiagnostic],
    admission: &mut NoteSourceAdmission,
) -> bool {
    let diagnostic_bytes = diagnostics_retained_byte_weight(diagnostics);
    let actual_bytes = document_bytes.saturating_add(diagnostic_bytes);
    if document_bytes > reservation.model_byte_limit || actual_bytes > reservation.charged_bytes {
        admission.add_truncation(NoteSourceTruncationReason::ConstructionByteLimit);
        admission.release_construction(reservation.charged_bytes);
        return false;
    }
    admission.release_construction(reservation.charged_bytes.saturating_sub(actual_bytes));
    if !admission.loaded_sidecar(diagnostics) {
        admission.release_construction(actual_bytes);
        return false;
    }
    // Settle the full parse reservation here: callers take a fresh scope-owned
    // charge for the parsed document via `with_construction_charge`, so no
    // residual manual charge can leak across an early item exit.
    admission.release_construction(actual_bytes);
    true
}

fn diagnostics_retained_byte_weight(diagnostics: &[RecoveryDiagnostic]) -> u64 {
    diagnostics.iter().fold(0u64, |total, diagnostic| {
        total
            .saturating_add(
                u64::try_from(std::mem::size_of::<RecoveryDiagnostic>()).unwrap_or(u64::MAX),
            )
            .saturating_add(diagnostic.retained_heap_byte_weight())
    })
}

fn bounded_sidecar_entries(
    dir: &Path,
    sidecar_limit: usize,
    cancellation: &PaletteSearchCancellation,
    admission: &mut NoteSourceAdmission,
) -> Result<BoundedSidecarEntries> {
    if !fs_metadata::path_status(dir)?.is_present() {
        return Ok(BoundedSidecarEntries::Admitted {
            entries: Vec::new(),
            retained_bytes: 0,
        });
    }
    let remaining_construction = admission.remaining_construction_bytes();
    let scan_byte_limit = admission
        .sidecar_path_byte_limit
        .min(remaining_construction);
    let scan = file_tree::scan_directory_bounded_with_cancel_and_bytes(
        dir,
        sidecar_limit,
        0,
        scan_byte_limit,
        || cancellation.is_cancelled(),
    );
    if scan.cancelled {
        return Ok(BoundedSidecarEntries::Admitted {
            entries: Vec::new(),
            retained_bytes: 0,
        });
    }
    if scan.truncated {
        admission.add_truncation(NoteSourceTruncationReason::SidecarLimit);
    }
    if scan.byte_truncated {
        if scan_byte_limit == admission.sidecar_path_byte_limit {
            admission.add_truncation(NoteSourceTruncationReason::SidecarPathByteLimit);
        }
        if scan_byte_limit == remaining_construction {
            admission.add_truncation(NoteSourceTruncationReason::ConstructionByteLimit);
        }
    }
    if let Some(error) = scan.error {
        return Err(anyhow::anyhow!(error))
            .with_context(|| format!("failed to read {}", dir.display()));
    }
    if !admission.observe_construction_overlap(scan.peak_retained_bytes)
        || !admission.try_charge_sidecar_paths(scan.retained_bytes)
    {
        return Ok(BoundedSidecarEntries::LimitReached);
    }
    Ok(BoundedSidecarEntries::Admitted {
        entries: scan.entries,
        retained_bytes: scan.retained_bytes,
    })
}

struct NoteSourceAdmission {
    bookmark_entries: Vec<PaletteNoteEntry>,
    folder_entries: Vec<PaletteNoteEntry>,
    document_entries: Vec<PaletteNoteEntry>,
    open_tab_entries: Vec<PaletteNoteEntry>,
    diagnostics: Vec<RecoveryDiagnostic>,
    metrics: NoteSourceMetrics,
    entry_limit: usize,
    text_byte_limit: usize,
    retained_byte_limit: u64,
    sidecar_path_byte_limit: u64,
    construction_byte_limit: u64,
    diagnostic_limit: usize,
    diagnostic_construction_bytes: u64,
}

impl Default for NoteSourceAdmission {
    fn default() -> Self {
        Self::with_limits(PALETTE_NOTE_SOURCE_LIMITS)
    }
}

impl NoteSourceAdmission {
    fn with_limits(limits: NoteSourceLimits) -> Self {
        Self {
            bookmark_entries: Vec::new(),
            folder_entries: Vec::new(),
            document_entries: Vec::new(),
            open_tab_entries: Vec::new(),
            diagnostics: Vec::new(),
            metrics: NoteSourceMetrics::default(),
            entry_limit: limits.entries,
            text_byte_limit: limits.searchable_text_bytes,
            retained_byte_limit: limits.retained_bytes,
            sidecar_path_byte_limit: limits.sidecar_path_bytes,
            construction_byte_limit: limits.construction_bytes,
            diagnostic_limit: limits.diagnostics,
            diagnostic_construction_bytes: 0,
        }
    }

    #[cfg(test)]
    fn with_test_limits(
        entry_limit: usize,
        text_byte_limit: usize,
        diagnostic_limit: usize,
    ) -> Self {
        Self::with_limits(NoteSourceLimits {
            entries: entry_limit,
            searchable_text_bytes: text_byte_limit,
            retained_bytes: u64::MAX,
            sidecar_entries: entry_limit,
            sidecar_path_bytes: u64::MAX,
            construction_bytes: u64::MAX,
            diagnostics: diagnostic_limit,
        })
    }

    fn admit(&mut self, entry: PaletteNoteEntry) -> bool {
        if self.metrics.retained_entries == self.entry_limit {
            self.add_truncation(NoteSourceTruncationReason::EntryLimit);
            return false;
        }
        let searchable_bytes = palette_note_searchable_bytes(&entry);
        if self
            .metrics
            .retained_searchable_bytes
            .saturating_add(searchable_bytes)
            > self.text_byte_limit
        {
            self.add_truncation(NoteSourceTruncationReason::TextByteLimit);
            return false;
        }
        let retained_bytes = u64::try_from(std::mem::size_of::<PaletteNoteEntry>())
            .unwrap_or(u64::MAX)
            .saturating_add(entry.retained_heap_byte_weight());
        if self.metrics.retained_bytes.saturating_add(retained_bytes) > self.retained_byte_limit {
            self.add_truncation(NoteSourceTruncationReason::RetainedByteLimit);
            return false;
        }
        // Geometric category capacity can retain almost two row shells, and
        // final assembly allocates one more shell array before those category
        // vectors drop. Charging both overlaps before push keeps peak evidence
        // conservative without depending on allocator growth details.
        let vector_overlap = u64::try_from(std::mem::size_of::<PaletteNoteEntry>())
            .unwrap_or(u64::MAX)
            .saturating_mul(2);
        if !self.try_charge_construction(retained_bytes.saturating_add(vector_overlap)) {
            return false;
        }

        self.metrics.retained_entries = self.metrics.retained_entries.saturating_add(1);
        self.metrics.retained_searchable_bytes = self
            .metrics
            .retained_searchable_bytes
            .saturating_add(searchable_bytes);
        self.metrics.retained_bytes = self.metrics.retained_bytes.saturating_add(retained_bytes);
        self.metrics.peak_retained_entries = self
            .metrics
            .peak_retained_entries
            .max(self.metrics.retained_entries);
        match entry.category {
            PaletteNoteCategory::Bookmarks => self.bookmark_entries.push(entry),
            PaletteNoteCategory::FolderNotes => self.folder_entries.push(entry),
            PaletteNoteCategory::DocumentNotes => self.document_entries.push(entry),
            PaletteNoteCategory::OpenTabs => self.open_tab_entries.push(entry),
        }
        true
    }

    fn loaded_sidecar(&mut self, diagnostics: &[RecoveryDiagnostic]) -> bool {
        self.metrics.loaded_sidecars = self.metrics.loaded_sidecars.saturating_add(1);
        let remaining = self.diagnostic_limit.saturating_sub(self.diagnostics.len());
        for diagnostic in diagnostics.iter().take(remaining) {
            let bytes = u64::try_from(std::mem::size_of::<RecoveryDiagnostic>())
                .unwrap_or(u64::MAX)
                .saturating_add(diagnostic.retained_heap_byte_weight());
            if !self.try_charge_construction(bytes) {
                return false;
            }
            self.diagnostic_construction_bytes =
                self.diagnostic_construction_bytes.saturating_add(bytes);
            self.diagnostics.push(diagnostic.clone());
        }
        if diagnostics.len() > remaining {
            self.add_truncation(NoteSourceTruncationReason::DiagnosticLimit);
        }
        true
    }

    fn add_truncation(&mut self, reason: NoteSourceTruncationReason) {
        if !self.metrics.truncation_reasons.contains(&reason) {
            self.metrics.truncation_reasons.push(reason);
        }
    }

    fn try_charge_sidecar_paths(&mut self, bytes: u64) -> bool {
        let next_paths = self
            .metrics
            .current_sidecar_path_bytes
            .saturating_add(bytes);
        if next_paths > self.sidecar_path_byte_limit {
            self.add_truncation(NoteSourceTruncationReason::SidecarPathByteLimit);
            return false;
        }
        if !self.try_charge_construction(bytes) {
            return false;
        }
        self.metrics.current_sidecar_path_bytes = next_paths;
        self.metrics.peak_sidecar_path_bytes = self
            .metrics
            .peak_sidecar_path_bytes
            .max(self.metrics.current_sidecar_path_bytes);
        true
    }

    fn release_sidecar_paths(&mut self, bytes: u64) {
        self.metrics.current_sidecar_path_bytes = self
            .metrics
            .current_sidecar_path_bytes
            .saturating_sub(bytes);
        self.release_construction(bytes);
    }

    fn try_charge_construction(&mut self, bytes: u64) -> bool {
        let next = self
            .metrics
            .current_construction_bytes
            .saturating_add(bytes);
        if next > self.construction_byte_limit {
            self.add_truncation(NoteSourceTruncationReason::ConstructionByteLimit);
            return false;
        }
        self.metrics.current_construction_bytes = next;
        self.metrics.peak_construction_bytes = self.metrics.peak_construction_bytes.max(next);
        true
    }

    fn remaining_construction_bytes(&self) -> u64 {
        self.construction_byte_limit
            .saturating_sub(self.metrics.current_construction_bytes)
    }

    fn observe_construction_overlap(&mut self, scratch_bytes: u64) -> bool {
        let peak = self
            .metrics
            .current_construction_bytes
            .saturating_add(scratch_bytes);
        if peak > self.construction_byte_limit {
            self.add_truncation(NoteSourceTruncationReason::ConstructionByteLimit);
            return false;
        }
        self.metrics.peak_construction_bytes = self.metrics.peak_construction_bytes.max(peak);
        true
    }

    fn release_construction(&mut self, bytes: u64) {
        self.metrics.current_construction_bytes = self
            .metrics
            .current_construction_bytes
            .saturating_sub(bytes);
    }

    /// Run one item's admission body under a scope-owned construction charge.
    ///
    /// The charge is taken before `body` runs and released exactly once on
    /// every return path — item admitted, filtered out (`Continue`), or early
    /// loop exit (`Break`). Callers cannot leak or double-release the charge
    /// because they never see it; a new early exit inside `body` releases by
    /// scope instead of by a manual call it could forget.
    fn with_construction_charge<T>(
        &mut self,
        bytes: u64,
        body: impl FnOnce(&mut Self) -> ControlFlow<T, ()>,
    ) -> ChargeOutcome<T> {
        if !self.try_charge_construction(bytes) {
            return ChargeOutcome::BudgetExhausted;
        }
        let flow = body(self);
        self.release_construction(bytes);
        match flow {
            ControlFlow::Continue(()) => ChargeOutcome::Ran,
            ControlFlow::Break(value) => ChargeOutcome::Broke(value),
        }
    }

    fn cancelled(mut self) -> PaletteNoteSourceOutcome {
        self.metrics.current_sidecar_path_bytes = 0;
        self.metrics.current_construction_bytes = 0;
        PaletteNoteSourceOutcome::Cancelled {
            metrics: self.metrics,
        }
    }

    fn complete(mut self) -> PaletteNoteSourceOutcome {
        sort_note_entries_by_label(&mut self.bookmark_entries);
        sort_note_entries_by_label(&mut self.document_entries);
        sort_note_entries_by_label(&mut self.open_tab_entries);
        let construction_row_bytes = self.metrics.retained_bytes;
        let vector_overlap = u64::try_from(std::mem::size_of::<PaletteNoteEntry>())
            .unwrap_or(u64::MAX)
            .saturating_mul(2)
            .saturating_mul(u64::try_from(self.metrics.retained_entries).unwrap_or(u64::MAX));
        self.release_construction(
            construction_row_bytes
                .saturating_add(vector_overlap)
                .saturating_add(self.diagnostic_construction_bytes),
        );
        let mut entries = Vec::with_capacity(self.metrics.retained_entries);
        entries.extend(self.bookmark_entries);
        entries.extend(self.folder_entries);
        entries.extend(self.document_entries);
        entries.extend(self.open_tab_entries);
        entries.shrink_to_fit();
        self.metrics.retained_bytes =
            crate::model::palette::palette_note_entries_retained_byte_weight(&entries);
        self.metrics.current_sidecar_path_bytes = 0;
        self.metrics.current_construction_bytes = 0;
        debug_assert!(self.metrics.retained_bytes <= self.retained_byte_limit);
        let truncation_reasons = self.metrics.truncation_reasons.clone();
        PaletteNoteSourceOutcome::Complete {
            load: PaletteNoteSourceLoad {
                entries,
                diagnostics: self.diagnostics,
                truncation_reasons,
            },
            metrics: self.metrics,
        }
    }
}

fn path_slice_retained_byte_weight(paths: &[PathBuf]) -> u64 {
    u64::try_from(paths.len().saturating_mul(std::mem::size_of::<PathBuf>()))
        .unwrap_or(u64::MAX)
        .saturating_add(paths.iter().fold(0u64, |total, path| {
            total.saturating_add(u64::try_from(path.capacity()).unwrap_or(u64::MAX))
        }))
}

fn string_set_retained_byte_weight(values: &HashSet<String>) -> u64 {
    let buckets = values
        .capacity()
        .saturating_mul(std::mem::size_of::<String>().saturating_add(1));
    u64::try_from(buckets).unwrap_or(u64::MAX).saturating_add(
        values.iter().fold(0u64, |total, value| {
            total.saturating_add(u64::try_from(value.capacity()).unwrap_or(u64::MAX))
        }),
    )
}

/// Exercise the production aggregate note admission policy with synthetic bodies.
///
/// This hidden benchmark seam avoids sidecar serialization dominating the
/// source-construction measurement while retaining the real entry/byte budgets,
/// body ownership, cancellation token, and typed terminal outcome.
#[doc(hidden)]
#[must_use]
pub fn admit_synthetic_note_bodies_for_benchmark(
    bodies: &[String],
    cancel_after_entries: Option<usize>,
) -> PaletteNoteSourceOutcome {
    let cancellation = PaletteSearchCancellation::default();
    let mut admission = NoteSourceAdmission::default();
    for (index, body) in bodies.iter().enumerate() {
        if cancel_after_entries == Some(index) {
            let _ = cancellation.cancel();
        }
        if cancellation.is_cancelled() {
            return admission.cancelled();
        }
        let note = RichNoteBody {
            text: body.clone(),
            created_at_secs: 0,
            updated_at_secs: 0,
        };
        let entry = folder_note_entry(
            "Benchmark workspace".to_string(),
            PathBuf::from(format!("/benchmark/folder-{index:05}")),
            note,
        );
        if !admission.admit(entry) {
            break;
        }
    }
    admission.complete()
}

fn palette_note_searchable_bytes(entry: &PaletteNoteEntry) -> usize {
    entry
        .title
        .len()
        .saturating_add(entry.subtitle.len())
        .saturating_add(entry.detail.as_deref().map_or(0, str::len))
        .saturating_add(entry.note_text.as_deref().map_or(0, str::len))
}

/// Merge bookmarks plus folder and document notes into one section-ordered row list.
#[must_use]
pub fn build_note_entries(
    visible_workspaces: &[WorkspaceConfig],
    bookmarks: Vec<bookmark_service::WorkspaceBookmark>,
    folder_notes: Vec<folder_note_service::ListedFolderNote>,
    document_notes: Vec<document_note_service::WorkspaceDocumentNote>,
    open_editor_snapshots: Vec<PaletteOpenEditorNoteSnapshot>,
    data_dir: &Path,
) -> Vec<PaletteNoteEntry> {
    let mut bookmark_entries = Vec::new();
    let mut folder_note_entries = Vec::new();
    let mut document_entries = Vec::new();
    let mut scoped_document_ids = HashSet::new();

    for bookmark in bookmarks {
        if let Some(workspace) = workspace_for_path(visible_workspaces, &bookmark.path) {
            remember_document_identity(&mut scoped_document_ids, &bookmark.path);
            let workspace_folder = workspace_folder_for_path(workspace, &bookmark.path)
                .unwrap_or_else(|| bookmark.path.clone());
            let source = PaletteNoteDocumentSource::Workspace {
                workspace_name: workspace.name.clone(),
                workspace_folder,
            };
            let bookmark_service::WorkspaceBookmark {
                path, line, label, ..
            } = bookmark;
            bookmark_entries.push(bookmark_entry(&source, path, line, label.as_deref()));
        }
    }

    folder_note_entries.extend(
        folder_notes
            .into_iter()
            .map(|note| folder_note_entry(note.workspace_name, note.folder, note.note)),
    );

    for note in document_notes {
        if let Some(workspace) = workspace_for_path(visible_workspaces, &note.path) {
            remember_document_identity(&mut scoped_document_ids, &note.path);
            let workspace_folder = workspace_folder_for_path(workspace, &note.path)
                .unwrap_or_else(|| note.path.clone());
            let source = PaletteNoteDocumentSource::Workspace {
                workspace_name: workspace.name.clone(),
                workspace_folder,
            };
            document_entries.push(document_note_entry(&source, note.path, note.note));
        }
    }

    let mut open_tab_entries =
        build_open_tab_note_entries(data_dir, open_editor_snapshots, &scoped_document_ids);

    sort_note_entries_by_label(&mut bookmark_entries);
    sort_note_entries_by_label(&mut document_entries);
    sort_note_entries_by_label(&mut open_tab_entries);

    let mut entries = Vec::new();
    entries.extend(bookmark_entries);
    entries.extend(folder_note_entries);
    entries.extend(document_entries);
    entries.extend(open_tab_entries);
    entries
}

/// Match one immutable Notes browser source in admission order.
///
/// Cancellation is checked between rows and within large UTF-8 bodies. The
/// returned indexes never exceed `max`, while `truncated` distinguishes a full
/// current result from an admission-truncated source.
#[must_use]
pub fn query_notes_browser_source(
    entries: &[PaletteNoteEntry],
    request: &NotesBrowserQueryRequest,
    max: usize,
    cancellation: &PaletteSearchCancellation,
) -> PaletteSearchOutcome<NotesBrowserQueryResult> {
    delay_notes_browser_query_for_test();
    let prepared = PaletteNoteTextQuery::new(&request.query);
    let mut metrics = PaletteSearchMetrics::default();
    let mut matching_indices = Vec::with_capacity(entries.len().min(max));
    let mut truncated = false;

    for (index, entry) in entries.iter().enumerate() {
        if cancellation.is_cancelled() {
            return PaletteSearchOutcome::Cancelled { metrics };
        }
        metrics.candidates_examined = metrics.candidates_examined.saturating_add(1);
        if !request.mode.includes_entry(entry) {
            continue;
        }
        let matches = match prepared.as_ref() {
            None => true,
            Some(query) => {
                let mut matched = false;
                for candidate in [
                    Some(entry.title.as_str()),
                    Some(entry.subtitle.as_str()),
                    entry.detail.as_deref(),
                    entry.note_text.as_deref(),
                ]
                .into_iter()
                .flatten()
                {
                    match query.matches_cancellable(candidate, cancellation) {
                        Some(true) => {
                            matched = true;
                            break;
                        }
                        Some(false) => {}
                        None => return PaletteSearchOutcome::Cancelled { metrics },
                    }
                }
                matched
            }
        };
        if !matches {
            continue;
        }
        metrics.matching_candidates = metrics.matching_candidates.saturating_add(1);
        if matching_indices.len() == max {
            truncated = true;
            break;
        }
        matching_indices.push(index);
        metrics.peak_retained_per_source =
            metrics.peak_retained_per_source.max(matching_indices.len());
    }

    PaletteSearchOutcome::Complete {
        value: NotesBrowserQueryResult {
            matching_indices,
            truncated,
        },
        metrics,
    }
}

/// Search prepared note rows by visible metadata and stored note body text.
#[must_use]
pub fn search_note_entries<'a>(
    entries: &'a [PaletteNoteEntry],
    query: &str,
    max: usize,
) -> Vec<&'a PaletteNoteEntry> {
    let cancellation = PaletteSearchCancellation::default();
    completed_note_refs(search_note_entries_cancellable(
        entries,
        None,
        query,
        max,
        &cancellation,
    ))
}

/// Search prepared note rows within one semantic Notes category.
#[must_use]
pub fn search_note_entries_in_category<'a>(
    entries: &'a [PaletteNoteEntry],
    category: PaletteNoteCategory,
    query: &str,
    max: usize,
) -> Vec<&'a PaletteNoteEntry> {
    let cancellation = PaletteSearchCancellation::default();
    completed_note_refs(search_note_entries_cancellable(
        entries,
        Some(category),
        query,
        max,
        &cancellation,
    ))
}

pub(super) fn search_note_entries_cancellable<'a>(
    entries: &'a [PaletteNoteEntry],
    category: Option<PaletteNoteCategory>,
    query: &str,
    max: usize,
    cancellation: &PaletteSearchCancellation,
) -> PaletteSearchOutcome<Vec<crate::model::palette::ScoredResult<'a>>> {
    let query = query.trim();
    let text_query = PaletteNoteTextQuery::new(query);
    let mut scoring_work = NoteScoringWork::default();
    let mut outcome = search_items_cancellable(
        entries.iter(),
        |entry| category.is_none_or(|category| entry.category == category),
        |entry, fuzzy_query| {
            text_query.as_ref().and_then(|text_query| {
                note_entry_score(
                    entry,
                    text_query,
                    fuzzy_query,
                    cancellation,
                    &mut scoring_work,
                    NoteBodyPolicy::PruneWhenDominated,
                )
            })
        },
        crate::model::palette::SearchResultItem::Note,
        query,
        max,
        cancellation,
    );
    scoring_work.attach(&mut outcome);
    outcome
}

/// Owned rank identity used by generated optimized/reference scoring checks.
#[cfg(any(test, feature = "property-tests"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteScoredIdentity {
    /// Stable input-slice identity for the selected row.
    pub source_ordinal: usize,
    /// Final maximum contribution across the row's searchable fields.
    pub score: u32,
}

/// Direct optimized/unpruned equivalence evidence for generated corpora.
#[cfg(any(test, feature = "property-tests"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteScoringEquivalenceEvidence {
    /// Bounded production selection in final rank order.
    pub optimized: Vec<NoteScoredIdentity>,
    /// Full-sort selection whose scorer always examines the body.
    pub unpruned_reference: Vec<NoteScoredIdentity>,
    /// Production work and retention high-water evidence.
    pub optimized_metrics: PaletteSearchMetrics,
    /// Bodies examined by the reference scorer.
    pub reference_bodies_examined: usize,
}

/// Compare bounded production scoring with an unpruned full-sort reference.
#[cfg(any(test, feature = "property-tests"))]
#[must_use]
pub fn note_scoring_equivalence_for_property_test(
    entries: &[PaletteNoteEntry],
    category: Option<PaletteNoteCategory>,
    query: &str,
    max: usize,
) -> NoteScoringEquivalenceEvidence {
    let query = query.trim();
    let cancellation = PaletteSearchCancellation::default();
    let optimized_outcome =
        search_note_entries_cancellable(entries, category, query, max, &cancellation);
    let PaletteSearchOutcome::Complete {
        value: optimized,
        metrics: optimized_metrics,
    } = optimized_outcome
    else {
        unreachable!("a fresh equivalence token cannot cancel");
    };

    let text_query = PaletteNoteTextQuery::new(query);
    let mut reference_work = NoteScoringWork::default();
    let reference = search_items_full_sort_reference(
        entries.iter(),
        |entry| category.is_none_or(|category| entry.category == category),
        |entry, fuzzy_query| {
            text_query.as_ref().and_then(|text_query| {
                note_entry_score(
                    entry,
                    text_query,
                    fuzzy_query,
                    &cancellation,
                    &mut reference_work,
                    NoteBodyPolicy::AlwaysExamine,
                )
            })
        },
        crate::model::palette::SearchResultItem::Note,
        query,
        max,
    );

    let ranks = |results: Vec<crate::model::palette::ScoredResult<'_>>| {
        results
            .into_iter()
            .map(|result| NoteScoredIdentity {
                source_ordinal: result.source_ordinal,
                score: result.score,
            })
            .collect()
    };
    NoteScoringEquivalenceEvidence {
        optimized: ranks(optimized),
        unpruned_reference: ranks(reference),
        optimized_metrics,
        reference_bodies_examined: reference_work.bodies_examined,
    }
}

fn completed_note_refs(
    outcome: PaletteSearchOutcome<Vec<crate::model::palette::ScoredResult<'_>>>,
) -> Vec<&PaletteNoteEntry> {
    let PaletteSearchOutcome::Complete { value, .. } = outcome else {
        unreachable!("fresh token cannot cancel");
    };
    value
        .into_iter()
        .filter_map(|result| match result.item {
            crate::model::palette::SearchResultItem::Note(entry) => Some(entry),
            crate::model::palette::SearchResultItem::OpenFile(_)
            | crate::model::palette::SearchResultItem::File(_)
            | crate::model::palette::SearchResultItem::Command(_) => None,
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct NoteScoringWork {
    candidates_scored: usize,
    bodies_examined: usize,
    bodies_safely_pruned: usize,
}

impl NoteScoringWork {
    fn attach<T>(self, outcome: &mut PaletteSearchOutcome<T>) {
        let metrics = match outcome {
            PaletteSearchOutcome::Complete { metrics, .. }
            | PaletteSearchOutcome::Cancelled { metrics } => metrics,
        };
        metrics.candidates_scored = metrics
            .candidates_scored
            .saturating_add(self.candidates_scored);
        metrics.note_bodies_examined = metrics
            .note_bodies_examined
            .saturating_add(self.bodies_examined);
        metrics.note_bodies_safely_pruned = metrics
            .note_bodies_safely_pruned
            .saturating_add(self.bodies_safely_pruned);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NoteBodyPolicy {
    PruneWhenDominated,
    #[cfg(any(test, feature = "property-tests"))]
    AlwaysExamine,
}

fn note_field_score_upper_bound(candidate: &str) -> u32 {
    if candidate.len() <= MAX_NOTE_FUZZY_SCORE_BYTES {
        MAX_NOTE_FIELD_FUZZY_SCORE
    } else {
        // Oversized fields remain substring-eligible but intentionally
        // contribute zero because they are never passed to nucleo.
        0
    }
}

fn score_note_field(
    candidate: &str,
    text_query: &PaletteNoteTextQuery,
    fuzzy_query: &mut FuzzyQuery,
    cancellation: &PaletteSearchCancellation,
) -> Result<Option<u32>, ()> {
    match text_query.matches_cancellable(candidate, cancellation) {
        Some(true) if candidate.len() <= MAX_NOTE_FUZZY_SCORE_BYTES => {
            Ok(Some(fuzzy_query.score(candidate).unwrap_or(0)))
        }
        Some(true) => Ok(Some(0)),
        Some(false) => Ok(None),
        None => Err(()),
    }
}

fn note_entry_score(
    entry: &PaletteNoteEntry,
    text_query: &PaletteNoteTextQuery,
    fuzzy_query: &mut FuzzyQuery,
    cancellation: &PaletteSearchCancellation,
    work: &mut NoteScoringWork,
    body_policy: NoteBodyPolicy,
) -> Option<u32> {
    work.candidates_scored = work.candidates_scored.saturating_add(1);
    let mut best = None;
    for candidate in [
        Some(entry.title.as_str()),
        Some(entry.subtitle.as_str()),
        entry.detail.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        match score_note_field(candidate, text_query, fuzzy_query, cancellation) {
            Ok(score) => best = best.max(score),
            Err(()) => return None,
        }
    }

    if let Some(note_text) = entry.note_text.as_deref() {
        let body_is_dominated = body_policy == NoteBodyPolicy::PruneWhenDominated
            && best.is_some_and(|score| score >= note_field_score_upper_bound(note_text));
        if body_is_dominated {
            work.bodies_safely_pruned = work.bodies_safely_pruned.saturating_add(1);
        } else {
            work.bodies_examined = work.bodies_examined.saturating_add(1);
            match score_note_field(note_text, text_query, fuzzy_query, cancellation) {
                Ok(score) => best = best.max(score),
                Err(()) => return None,
            }
        }
    }
    best
}

/// Case-insensitive full-text query used to decide note-row eligibility.
struct PaletteNoteTextQuery {
    /// Query represented as Unicode scalar values so note bodies do not need to
    /// allocate their own lowercased copies on every keystroke.
    needle: Vec<char>,
    /// Knuth-Morris-Pratt prefix table for streaming substring matching.
    prefix: Vec<usize>,
}

impl PaletteNoteTextQuery {
    /// Prepare one non-empty query for repeated row checks.
    #[must_use]
    fn new(query: &str) -> Option<Self> {
        let lower_text = query.trim().to_lowercase();
        if lower_text.is_empty() {
            return None;
        }

        let needle: Vec<_> = lower_text.chars().collect();
        let prefix = Self::prefix_table(&needle);
        Some(Self { needle, prefix })
    }

    /// Build the KMP prefix table once per query instead of once per note body.
    fn prefix_table(needle: &[char]) -> Vec<usize> {
        let mut prefix = vec![0; needle.len()];
        let mut matched = 0;
        for index in 1..needle.len() {
            while matched > 0 && needle[index] != needle[matched] {
                matched = prefix[matched - 1];
            }
            if needle[index] == needle[matched] {
                matched += 1;
                prefix[index] = matched;
            }
        }
        prefix
    }

    /// Match without allocating a lowercased copy of large note bodies.
    #[cfg(test)]
    fn matches(&self, haystack: &str) -> bool {
        self.matches_cancellable(haystack, &PaletteSearchCancellation::default())
            .unwrap_or(false)
    }

    /// Match while bounding superseded work within one large note body.
    fn matches_cancellable(
        &self,
        haystack: &str,
        cancellation: &PaletteSearchCancellation,
    ) -> Option<bool> {
        if haystack.is_empty() {
            return Some(false);
        }

        let mut matched = 0;
        for (index, character) in haystack.chars().flat_map(char::to_lowercase).enumerate() {
            if index % NOTE_TEXT_CANCEL_CHECK_INTERVAL == 0 && cancellation.is_cancelled() {
                return None;
            }
            while matched > 0 && character != self.needle[matched] {
                matched = self.prefix[matched - 1];
            }
            if character == self.needle[matched] {
                matched += 1;
                if matched == self.needle.len() {
                    return Some(true);
                }
            }
        }
        Some(false)
    }
}

/// Return whether one path is inside any folder in the current browse scope.
#[must_use]
pub fn path_is_in_folders(path: &Path, folders: &[PathBuf]) -> bool {
    folders.iter().any(|folder| path.starts_with(folder))
}

/// Classify an out-of-scope saved open tab for note source metadata.
#[must_use]
pub fn open_tab_source_for_path(
    all_workspaces: &[WorkspaceConfig],
    path: &Path,
) -> PaletteOpenTabSource {
    let owning_workspace = workspace_for_path(all_workspaces, path);
    PaletteOpenTabSource {
        workspace_name: owning_workspace.map(|workspace| workspace.name.clone()),
        workspace_folder: owning_workspace
            .and_then(|workspace| workspace_folder_for_path(workspace, path)),
    }
}

/// Origin of a row that is attached to a saved document path.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PaletteNoteDocumentSource {
    /// The row belongs to the currently browsed workspace scope.
    Workspace {
        /// User-visible workspace label.
        workspace_name: String,
        /// Workspace folder used for Markdown context and document-note actions.
        workspace_folder: PathBuf,
    },
    /// The row comes from a saved open tab outside the current scope.
    OpenTab(PaletteOpenTabSource),
}

impl PaletteOpenTabSource {
    /// User-facing source label for rows that come from a saved open tab.
    #[must_use]
    pub fn row_label(&self) -> String {
        match (&self.workspace_name, &self.workspace_folder) {
            (Some(workspace_name), Some(folder)) => {
                format!("Open tab · {workspace_name} · {}", folder.display())
            }
            (Some(workspace_name), None) => format!("Open tab · {workspace_name}"),
            (None, _) => "Open tab · Outside workspace".to_string(),
        }
    }
}

impl PaletteNoteDocumentSource {
    /// User-facing source label shown in row subtitles and preview metadata.
    #[must_use]
    fn row_label(&self) -> String {
        match self {
            Self::Workspace {
                workspace_name,
                workspace_folder,
            } => format!("{workspace_name} · {}", workspace_folder.display()),
            Self::OpenTab(source) => source.row_label(),
        }
    }

    /// Return whether this row belongs to the supplemental open-tab section.
    #[must_use]
    fn is_open_tab(&self) -> bool {
        matches!(self, Self::OpenTab(_))
    }

    /// Real workspace folders available for Markdown rendering and note actions.
    #[must_use]
    fn workspace_folders(&self) -> Vec<PathBuf> {
        match self {
            Self::Workspace {
                workspace_folder, ..
            } => vec![workspace_folder.clone()],
            Self::OpenTab(source) => source.workspace_folder.iter().cloned().collect(),
        }
    }
}

fn bookmark_entry(
    source: &PaletteNoteDocumentSource,
    path: PathBuf,
    line: u32,
    label: Option<&str>,
) -> PaletteNoteEntry {
    let category = if source.is_open_tab() {
        PaletteNoteCategory::OpenTabs
    } else {
        PaletteNoteCategory::Bookmarks
    };
    PaletteNoteEntry {
        category,
        title: format!("Bookmark · {}", bookmark_display_label(label, line)),
        subtitle: format!(
            "{} · {} · {}",
            source.row_label(),
            path.display(),
            format_line_label(line)
        ),
        detail: None,
        note_text: None,
        target: PaletteNoteTarget::Bookmark {
            path,
            line,
            workspace_folders: source.workspace_folders(),
        },
    }
}

fn folder_note_entry(
    workspace_name: String,
    folder: PathBuf,
    note: RichNoteBody,
) -> PaletteNoteEntry {
    PaletteNoteEntry {
        category: PaletteNoteCategory::FolderNotes,
        title: format!("Folder Note · {workspace_name}"),
        subtitle: format!("{workspace_name} · {}", folder.display()),
        detail: note_detail(&note.text),
        note_text: Some(note.text),
        target: PaletteNoteTarget::FolderNote {
            workspace_name,
            folder,
        },
    }
}

fn document_note_entry(
    source: &PaletteNoteDocumentSource,
    path: PathBuf,
    note: RichNoteBody,
) -> PaletteNoteEntry {
    let category = if source.is_open_tab() {
        PaletteNoteCategory::OpenTabs
    } else {
        PaletteNoteCategory::DocumentNotes
    };
    let file_name = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let workspace_folders = source.workspace_folders();
    PaletteNoteEntry {
        category,
        title: format!("Document Note · {file_name}"),
        subtitle: format!("{} · {}", source.row_label(), path.display()),
        detail: note_detail(&note.text),
        note_text: Some(note.text),
        target: PaletteNoteTarget::DocumentNote {
            path,
            workspace_folders,
        },
    }
}

fn note_detail(text: &str) -> Option<String> {
    let preview = note_preview_line(text);
    (!preview.is_empty()).then_some(preview)
}

/// Add a resolved document identity to the defensive dedupe set when possible.
fn remember_document_identity(document_ids: &mut HashSet<String>, path: &Path) {
    if let Ok(identity) = bookmark_service::resolve_document_identity(path) {
        document_ids.insert(identity.sidecar_id);
    }
}

/// Build supplemental rows for saved open tabs outside the current workspace scope.
fn build_open_tab_note_entries(
    data_dir: &Path,
    open_editor_snapshots: Vec<PaletteOpenEditorNoteSnapshot>,
    scoped_document_ids: &HashSet<String>,
) -> Vec<PaletteNoteEntry> {
    let mut entries = Vec::new();
    for snapshot in open_editor_snapshots {
        let Some(open_tab_source) = snapshot.open_tab_source else {
            continue;
        };
        if bookmark_service::resolve_document_identity(&snapshot.path)
            .is_ok_and(|identity| scoped_document_ids.contains(&identity.sidecar_id))
        {
            continue;
        }

        let source = PaletteNoteDocumentSource::OpenTab(open_tab_source);
        entries.extend(snapshot.bookmarks.into_iter().map(|bookmark| {
            bookmark_entry(
                &source,
                snapshot.path.clone(),
                bookmark.line,
                bookmark.label.as_deref(),
            )
        }));

        if let Ok(Some(document)) = document_note_service::load_for_path(data_dir, &snapshot.path) {
            entries.push(document_note_entry(&source, snapshot.path, document.note));
        }
    }
    entries
}

/// Overlay sidecar bookmark rows with current open-editor rows for the same file.
#[cfg(test)]
fn merge_live_bookmark_snapshots(
    persisted: Vec<bookmark_service::WorkspaceBookmark>,
    live_snapshots: Vec<PaletteOpenEditorNoteSnapshot>,
) -> Vec<bookmark_service::WorkspaceBookmark> {
    if live_snapshots.is_empty() {
        return persisted;
    }

    let mut live_document_ids = HashSet::new();
    let mut live_rows = Vec::new();
    for snapshot in live_snapshots {
        let Ok(identity) = bookmark_service::resolve_document_identity(&snapshot.path) else {
            continue;
        };
        live_document_ids.insert(identity.sidecar_id);
        live_rows.extend(snapshot.bookmarks.into_iter().map(|bookmark| {
            bookmark_service::WorkspaceBookmark {
                path: snapshot.path.clone(),
                bookmark_id: bookmark.id,
                line: bookmark.line,
                label: bookmark.label,
            }
        }));
    }

    if live_document_ids.is_empty() {
        return persisted;
    }

    let mut live_path_cache = HashMap::new();
    let mut merged: Vec<_> = persisted
        .into_iter()
        .filter(|bookmark| {
            let is_live_document =
                *live_path_cache
                    .entry(bookmark.path.clone())
                    .or_insert_with(|| {
                        bookmark_service::resolve_document_identity(&bookmark.path)
                            .is_ok_and(|identity| live_document_ids.contains(&identity.sidecar_id))
                    });
            !is_live_document
        })
        .collect();
    merged.extend(live_rows);
    merged.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.bookmark_id.0.cmp(&right.bookmark_id.0))
    });
    merged
}

/// Keep non-folder note rows in their familiar title/subtitle order.
fn sort_note_entries_by_label(entries: &mut [PaletteNoteEntry]) {
    entries.sort_by(|left, right| {
        left.title
            .cmp(&right.title)
            .then_with(|| left.subtitle.cmp(&right.subtitle))
    });
}

/// Find the first configured workspace that owns one saved path.
fn workspace_for_path<'a>(
    workspaces: &'a [WorkspaceConfig],
    path: &Path,
) -> Option<&'a WorkspaceConfig> {
    workspaces
        .iter()
        .find(|workspace| workspace_folder_for_path(workspace, path).is_some())
}

/// Find the first configured folder in one workspace that owns a path.
fn workspace_folder_for_path(workspace: &WorkspaceConfig, path: &Path) -> Option<PathBuf> {
    workspace
        .folders
        .iter()
        .find(|folder| path.starts_with(folder.path()))
        .map(|folder| folder.path.clone())
}

/// Display one zero-based bookmark line in the 1-based form users expect.
#[must_use]
pub fn format_line_label(line: u32) -> String {
    format!("Line {}", line.saturating_add(1))
}

/// Return the bookmark's explicit label or its stable line fallback.
#[must_use]
pub fn bookmark_display_label(label: Option<&str>, line: u32) -> String {
    label
        .filter(|label| !label.trim().is_empty())
        .map_or_else(|| format_line_label(line), ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::bookmark::BookmarkRecord;
    use crate::model::workspace::{WorkspaceFolder, WorkspaceId, WorkspaceScope, WorkspacesFile};
    use crate::services::filesystem::fixture;
    use tempfile::TempDir;

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fixture::create_dir_all(parent);
        }
        fixture::write_text(path, contents);
    }

    fn workspace(id: &str, name: &str, folders: Vec<PathBuf>) -> WorkspaceConfig {
        WorkspaceConfig::with_folders(
            WorkspaceId::new(id),
            name,
            folders.into_iter().map(WorkspaceFolder::new).collect(),
        )
    }

    fn categories(entries: &[PaletteNoteEntry]) -> Vec<PaletteNoteCategory> {
        entries.iter().map(|entry| entry.category).collect()
    }

    fn test_note_entry(category: PaletteNoteCategory, title: &str, body: &str) -> PaletteNoteEntry {
        PaletteNoteEntry {
            category,
            title: title.to_string(),
            subtitle: "Core · /workspace".to_string(),
            detail: None,
            note_text: Some(body.to_string()),
            target: PaletteNoteTarget::FolderNote {
                workspace_name: "Core".to_string(),
                folder: PathBuf::from("/workspace"),
            },
        }
    }

    #[test]
    fn notes_browser_no_match_query_scans_the_complete_admitted_source() {
        let entries = (0..1_000)
            .map(|index| {
                test_note_entry(
                    PaletteNoteCategory::FolderNotes,
                    &format!("entry-{index:04}"),
                    "ordinary body",
                )
            })
            .collect::<Vec<_>>();
        let outcome = query_notes_browser_source(
            &entries,
            &NotesBrowserQueryRequest {
                query: "missing needle".to_string(),
                mode: NotesBrowserMode::AllNotes,
            },
            500,
            &PaletteSearchCancellation::default(),
        );

        let PaletteSearchOutcome::Complete { value, metrics } = outcome else {
            panic!("fresh query should complete");
        };
        assert!(value.matching_indices.is_empty());
        assert!(!value.truncated);
        assert_eq!(metrics.candidates_examined, entries.len());
        assert_eq!(metrics.peak_retained_per_source, 0);
    }

    #[test]
    fn notes_browser_query_retains_only_the_ordered_render_cap() {
        let entries = (0..=500)
            .map(|index| {
                test_note_entry(
                    PaletteNoteCategory::DocumentNotes,
                    &format!("matching-{index:04}"),
                    "matching body",
                )
            })
            .collect::<Vec<_>>();
        let outcome = query_notes_browser_source(
            &entries,
            &NotesBrowserQueryRequest {
                query: "matching".to_string(),
                mode: NotesBrowserMode::AllNotes,
            },
            500,
            &PaletteSearchCancellation::default(),
        );

        let PaletteSearchOutcome::Complete { value, metrics } = outcome else {
            panic!("fresh query should complete");
        };
        assert_eq!(value.matching_indices, (0..500).collect::<Vec<_>>());
        assert!(value.truncated);
        assert_eq!(metrics.peak_retained_per_source, 500);
    }

    #[test]
    fn notes_browser_bookmark_mode_matches_only_bookmark_targets() {
        let bookmark = PaletteNoteEntry {
            category: PaletteNoteCategory::Bookmarks,
            title: "Scoped bookmark".to_string(),
            subtitle: "Core".to_string(),
            detail: None,
            note_text: None,
            target: PaletteNoteTarget::Bookmark {
                path: PathBuf::from("/workspace/scoped.rs"),
                line: 2,
                workspace_folders: vec![PathBuf::from("/workspace")],
            },
        };
        let document = test_note_entry(PaletteNoteCategory::DocumentNotes, "Document note", "body");
        let open_tab_bookmark = PaletteNoteEntry {
            category: PaletteNoteCategory::OpenTabs,
            title: "Live bookmark".to_string(),
            subtitle: "Open tab".to_string(),
            detail: None,
            note_text: None,
            target: PaletteNoteTarget::Bookmark {
                path: PathBuf::from("/outside/live.rs"),
                line: 4,
                workspace_folders: Vec::new(),
            },
        };
        let entries = vec![bookmark, document, open_tab_bookmark];

        let outcome = query_notes_browser_source(
            &entries,
            &NotesBrowserQueryRequest {
                query: String::new(),
                mode: NotesBrowserMode::Bookmarks,
            },
            500,
            &PaletteSearchCancellation::default(),
        );

        let PaletteSearchOutcome::Complete { value, metrics } = outcome else {
            panic!("fresh bookmark query should complete");
        };
        assert_eq!(value.matching_indices, vec![0, 2]);
        assert!(!value.truncated);
        assert_eq!(metrics.candidates_examined, entries.len());
        assert_eq!(metrics.matching_candidates, 2);
    }

    #[test]
    fn bounded_bookmark_source_skips_non_bookmark_sidecars() {
        let data = TempDir::new().expect("bookmark-mode data tempdir");
        let workspace_folder = data.path().join("workspace");
        fixture::create_dir(&workspace_folder);
        let path = workspace_folder.join("source.rs");
        write_file(&path, "one\ntwo\n");
        bookmark_service::save_for_path(
            data.path(),
            &path,
            &[BookmarkRecord::new(1, Some("bounded bookmark".to_string()))],
        )
        .expect("save bookmark sidecar");
        document_note_service::save_for_path(
            data.path(),
            &path,
            &RichNoteBody::new("document body must not enter bookmark mode"),
        )
        .expect("save document note sidecar");
        let scope = WorkspacesFile {
            current_scope: WorkspaceScope::All,
            workspaces: vec![workspace("core", "Core", vec![workspace_folder])],
        }
        .current_scope_snapshot();

        let outcome = load_note_entries_bounded_for_scope(
            data.path(),
            &scope,
            &[],
            false,
            NotesBrowserMode::Bookmarks,
            PALETTE_NOTE_SOURCE_LIMITS,
            &PaletteSearchCancellation::default(),
        )
        .expect("load bounded bookmark source");

        let PaletteNoteSourceOutcome::Complete { load, metrics } = outcome else {
            panic!("fresh bookmark source should complete");
        };
        assert_eq!(load.entries.len(), 1);
        assert!(matches!(
            load.entries[0].target,
            PaletteNoteTarget::Bookmark { line: 1, .. }
        ));
        assert_eq!(metrics.retained_entries, 1);
        assert_eq!(metrics.loaded_sidecars, 1);
    }

    #[test]
    fn build_note_entries_returns_empty_for_empty_sources() {
        let dir = TempDir::new().expect("tempdir");

        let entries = build_note_entries(
            &[],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            dir.path(),
        );

        assert!(entries.is_empty());
    }

    #[test]
    fn bounded_note_admission_stops_at_the_exact_entry_limit() {
        let mut admission = NoteSourceAdmission::with_test_limits(2, usize::MAX, 4);

        assert!(admission.admit(test_note_entry(
            PaletteNoteCategory::Bookmarks,
            "First",
            "one",
        )));
        assert!(admission.admit(test_note_entry(
            PaletteNoteCategory::DocumentNotes,
            "Second",
            "two",
        )));
        assert!(!admission.admit(test_note_entry(
            PaletteNoteCategory::FolderNotes,
            "Rejected",
            "three",
        )));

        let PaletteNoteSourceOutcome::Complete { load, metrics } = admission.complete() else {
            unreachable!("completed admission must publish a bounded source");
        };
        assert_eq!(load.entries.len(), 2);
        assert_eq!(metrics.retained_entries, 2);
        assert_eq!(metrics.peak_retained_entries, 2);
        assert_eq!(
            load.truncation_reasons,
            vec![NoteSourceTruncationReason::EntryLimit]
        );
    }

    #[test]
    fn bounded_note_admission_accepts_the_exact_byte_boundary_only() {
        let first = test_note_entry(PaletteNoteCategory::FolderNotes, "Boundary", "body");
        let exact_bytes = palette_note_searchable_bytes(&first);
        let mut admission = NoteSourceAdmission::with_test_limits(4, exact_bytes, 4);

        assert!(admission.admit(first));
        assert!(!admission.admit(test_note_entry(
            PaletteNoteCategory::FolderNotes,
            "Later",
            "x",
        )));

        let PaletteNoteSourceOutcome::Complete { load, metrics } = admission.complete() else {
            unreachable!("completed admission must publish a bounded source");
        };
        assert_eq!(load.entries.len(), 1);
        assert_eq!(metrics.retained_searchable_bytes, exact_bytes);
        assert_eq!(
            load.truncation_reasons,
            vec![NoteSourceTruncationReason::TextByteLimit]
        );
    }

    #[test]
    fn sidecar_path_accounting_accepts_exact_bytes_and_rejects_one_over() {
        let mut admission = NoteSourceAdmission::with_limits(NoteSourceLimits {
            entries: 1,
            searchable_text_bytes: usize::MAX,
            retained_bytes: u64::MAX,
            sidecar_entries: 1,
            sidecar_path_bytes: 10,
            construction_bytes: 20,
            diagnostics: 1,
        });

        assert!(admission.try_charge_sidecar_paths(10));
        assert_eq!(admission.metrics.current_sidecar_path_bytes, 10);
        assert_eq!(admission.metrics.peak_sidecar_path_bytes, 10);
        assert_eq!(admission.metrics.current_construction_bytes, 10);
        admission.release_sidecar_paths(10);
        assert_eq!(admission.metrics.current_sidecar_path_bytes, 0);
        assert_eq!(admission.metrics.current_construction_bytes, 0);

        assert!(!admission.try_charge_sidecar_paths(11));
        assert_eq!(admission.metrics.current_sidecar_path_bytes, 0);
        assert!(
            admission
                .metrics
                .truncation_reasons
                .contains(&NoteSourceTruncationReason::SidecarPathByteLimit)
        );
    }

    #[test]
    fn construction_charge_scope_releases_on_admit_filter_break_and_exhaustion() {
        let mut admission = NoteSourceAdmission::with_limits(NoteSourceLimits {
            entries: 8,
            searchable_text_bytes: usize::MAX,
            retained_bytes: u64::MAX,
            sidecar_entries: 8,
            sidecar_path_bytes: 1_024,
            construction_bytes: 100,
            diagnostics: 1,
        });

        // Admit path: the body runs with the charge visible, then the scope
        // releases exactly once.
        let outcome = admission.with_construction_charge(40, |admission| {
            assert_eq!(admission.metrics.current_construction_bytes, 40);
            ControlFlow::<(), ()>::Continue(())
        });
        assert_eq!(outcome, ChargeOutcome::Ran);
        assert_eq!(admission.metrics.current_construction_bytes, 0);
        assert_eq!(admission.metrics.peak_construction_bytes, 40);

        // Filter-out path: an early `Continue` (the loops' `continue`) still
        // releases by scope with no manual call.
        let outcome =
            admission.with_construction_charge(25, |_| ControlFlow::<(), ()>::Continue(()));
        assert_eq!(outcome, ChargeOutcome::Ran);
        assert_eq!(admission.metrics.current_construction_bytes, 0);

        // Break path: an early loop exit releases before the value surfaces,
        // so a new early exit cannot leak the charge.
        let outcome = admission.with_construction_charge(30, |_| ControlFlow::Break("stop"));
        assert_eq!(outcome, ChargeOutcome::Broke("stop"));
        assert_eq!(admission.metrics.current_construction_bytes, 0);

        // Budget-exhausted path: the body never runs and nothing is charged.
        assert!(admission.try_charge_construction(90));
        let outcome = admission.with_construction_charge(20, |_| {
            unreachable!("an exhausted budget must not run the charged body");
            #[expect(unreachable_code, reason = "type anchor for the never-run body")]
            ControlFlow::<(), ()>::Continue(())
        });
        assert_eq!(outcome, ChargeOutcome::BudgetExhausted);
        assert_eq!(admission.metrics.current_construction_bytes, 90);
        assert!(
            admission
                .metrics
                .truncation_reasons
                .contains(&NoteSourceTruncationReason::ConstructionByteLimit)
        );
        admission.release_construction(90);
        assert_eq!(admission.metrics.current_construction_bytes, 0);
        assert_eq!(admission.metrics.peak_construction_bytes, 90);
    }

    #[test]
    fn sidecar_scan_peak_is_bounded_by_remaining_aggregate_construction() {
        let dir = TempDir::new().expect("unicode sidecar scan fixture");
        let sidecars = dir.path().join("données-équipe-東京-🙂");
        fixture::create_dir_all(&sidecars);
        for index in 0..8 {
            write_file(
                &sidecars.join(format!("note-équipe-東京-🙂-{index:02}.json")),
                "{}",
            );
        }
        let limits = NoteSourceLimits {
            entries: 16,
            searchable_text_bytes: usize::MAX,
            retained_bytes: u64::MAX,
            sidecar_entries: 16,
            sidecar_path_bytes: 32 * 1024,
            construction_bytes: 32 * 1024,
            diagnostics: 1,
        };
        let mut admission = NoteSourceAdmission::with_limits(limits);
        assert!(admission.try_charge_construction(1_024));

        let BoundedSidecarEntries::Admitted {
            entries,
            retained_bytes,
        } = bounded_sidecar_entries(
            &sidecars,
            limits.sidecar_entries,
            &PaletteSearchCancellation::default(),
            &mut admission,
        )
        .expect("bounded sidecar scan")
        else {
            panic!("unicode fixture must fit the declared scan envelope");
        };

        assert_eq!(entries.len(), 8);
        assert!(retained_bytes > 0);
        assert!(admission.metrics.peak_sidecar_path_bytes > 0);
        assert!(admission.metrics.peak_construction_bytes <= limits.construction_bytes);
        admission.release_sidecar_paths(retained_bytes);
        admission.release_construction(1_024);
    }

    #[test]
    fn sidecar_parse_reserves_raw_plus_model_peak_before_loading() {
        let dir = TempDir::new().expect("sidecar parse reservation fixture");
        let path = dir.path().join("note.json");
        write_file(&path, &"x".repeat(4_096));
        let raw_bytes = fs_metadata::file_facts(&path)
            .expect("sidecar facts")
            .byte_size;
        let required = raw_bytes
            .saturating_mul(NOTE_SIDECAR_MODEL_EXPANSION_MULTIPLIER.saturating_add(1))
            .saturating_add(NOTE_SIDECAR_DIAGNOSTIC_RESERVATION_BYTES);
        let limits = |construction_bytes| NoteSourceLimits {
            entries: 1,
            searchable_text_bytes: usize::MAX,
            retained_bytes: u64::MAX,
            sidecar_entries: 1,
            sidecar_path_bytes: u64::MAX,
            construction_bytes,
            diagnostics: 1,
        };

        let mut exact = NoteSourceAdmission::with_limits(limits(required.saturating_add(17)));
        assert!(exact.try_charge_construction(17));
        let reservation = reserve_sidecar_parse(std::slice::from_ref(&path), &mut exact)
            .expect("preflight")
            .expect("exact parse envelope");
        assert_eq!(reservation.charged_bytes, required);
        assert_eq!(exact.metrics.peak_construction_bytes, required + 17);
        exact.release_construction(reservation.charged_bytes);

        let mut one_under = NoteSourceAdmission::with_limits(limits(required.saturating_add(16)));
        assert!(one_under.try_charge_construction(17));
        assert!(
            reserve_sidecar_parse(&[path], &mut one_under)
                .expect("preflight")
                .is_none()
        );
        assert!(
            one_under
                .metrics
                .truncation_reasons
                .contains(&NoteSourceTruncationReason::ConstructionByteLimit)
        );
    }

    #[test]
    fn unreadable_sidecar_preflight_preserves_existing_rows_and_diagnostic() {
        let dir = TempDir::new().expect("unreadable sidecar fixture");
        let not_a_directory = dir.path().join("not-a-directory");
        write_file(&not_a_directory, "not a directory");
        let sidecar_path = not_a_directory.join("bookmark.json");
        let mut admission = NoteSourceAdmission::with_limits(NoteSourceLimits {
            entries: 2,
            searchable_text_bytes: usize::MAX,
            retained_bytes: u64::MAX,
            sidecar_entries: 1,
            sidecar_path_bytes: u64::MAX,
            construction_bytes: u64::MAX,
            diagnostics: 1,
        });
        assert!(admission.admit(test_note_entry(
            PaletteNoteCategory::OpenTabs,
            "Valid open note",
            "body",
        )));

        let reservation =
            reserve_sidecar_parse(std::slice::from_ref(&sidecar_path), &mut admission)
                .expect("metadata failure stays in the recovery flow")
                .expect("diagnostic reservation");
        let load = note_storage::load_json_file_recovering_with_max_bytes::<BookmarkDocument>(
            dir.path(),
            &sidecar_path,
            RecoveryMetadataClass::BookmarkSidecar,
            reservation.max_read_bytes,
        );
        assert!(matches!(
            load.diagnostics.as_slice(),
            [diagnostic]
                if matches!(
                    diagnostic.problem,
                    crate::services::recovery_metadata::RecoveryProblem::Unreadable { .. }
                )
        ));
        assert!(admit_parsed_sidecar(
            &reservation,
            0,
            &load.diagnostics,
            &mut admission,
        ));

        let PaletteNoteSourceOutcome::Complete { load, .. } = admission.complete() else {
            unreachable!("recovery-aware source completes");
        };
        assert_eq!(load.entries.len(), 1);
        assert_eq!(load.diagnostics.len(), 1);
    }

    #[test]
    fn bookmark_model_expansion_stays_within_parse_reservation_factor() {
        let identity = crate::model::sidecar_identity::DocumentSidecarIdentity::from_paths(
            PathBuf::from("/workspace/main.rs"),
            PathBuf::from("/workspace/main.rs"),
        );
        let document = BookmarkDocument {
            identity,
            bookmarks: (0..1_000)
                .map(|index| BookmarkRecord::new(index, None))
                .collect(),
        };
        let compact_json = serde_json::to_vec(&document).expect("serialize bookmark fixture");
        assert!(
            document.retained_heap_byte_weight()
                <= u64::try_from(compact_json.len())
                    .unwrap_or(u64::MAX)
                    .saturating_mul(NOTE_SIDECAR_MODEL_EXPANSION_MULTIPLIER)
        );
    }

    #[test]
    fn construction_accounting_is_saturating_and_preserves_peak_after_release() {
        let mut admission = NoteSourceAdmission::with_limits(NoteSourceLimits {
            entries: 1,
            searchable_text_bytes: usize::MAX,
            retained_bytes: u64::MAX,
            sidecar_entries: 1,
            sidecar_path_bytes: u64::MAX,
            construction_bytes: 10,
            diagnostics: 1,
        });

        assert!(admission.try_charge_construction(10));
        admission.release_construction(10);
        assert_eq!(admission.metrics.current_construction_bytes, 0);
        assert_eq!(admission.metrics.peak_construction_bytes, 10);
        assert!(!admission.try_charge_construction(u64::MAX));
        assert_eq!(admission.metrics.current_construction_bytes, 0);
        assert!(
            admission
                .metrics
                .truncation_reasons
                .contains(&NoteSourceTruncationReason::ConstructionByteLimit)
        );
    }

    #[test]
    fn unicode_path_and_diagnostic_heap_weights_have_exact_construction_boundaries() {
        let mut unicode_path = PathBuf::from("/workspace/équipe/東京/🙂.json");
        unicode_path.reserve(256);
        let paths = vec![unicode_path];
        let path_bytes = path_slice_retained_byte_weight(&paths);
        assert!(path_bytes > u64::try_from(paths[0].as_os_str().len()).unwrap_or(u64::MAX));

        let diagnostic = RecoveryDiagnostic::repair_skipped(
            RecoveryMetadataClass::BookmarkSidecar,
            paths[0].clone(),
            "diagnostic detail".repeat(32),
        );
        let diagnostic_bytes = u64::try_from(std::mem::size_of::<RecoveryDiagnostic>())
            .unwrap_or(u64::MAX)
            .saturating_add(diagnostic.retained_heap_byte_weight());
        let limits = |construction_bytes| NoteSourceLimits {
            entries: 1,
            searchable_text_bytes: usize::MAX,
            retained_bytes: u64::MAX,
            sidecar_entries: 1,
            sidecar_path_bytes: u64::MAX,
            construction_bytes,
            diagnostics: 1,
        };

        let mut exact = NoteSourceAdmission::with_limits(limits(diagnostic_bytes));
        assert!(exact.loaded_sidecar(std::slice::from_ref(&diagnostic)));
        assert_eq!(exact.metrics.peak_construction_bytes, diagnostic_bytes);

        let mut one_under =
            NoteSourceAdmission::with_limits(limits(diagnostic_bytes.saturating_sub(1)));
        assert!(!one_under.loaded_sidecar(&[diagnostic]));
        assert!(
            one_under
                .metrics
                .truncation_reasons
                .contains(&NoteSourceTruncationReason::ConstructionByteLimit)
        );
    }

    #[test]
    fn final_row_and_category_overlap_stop_before_one_over_construction() {
        let entry = test_note_entry(PaletteNoteCategory::DocumentNotes, "Boundary", "body");
        let retained = u64::try_from(std::mem::size_of::<PaletteNoteEntry>())
            .unwrap_or(u64::MAX)
            .saturating_add(entry.retained_heap_byte_weight());
        let vector_overlap = u64::try_from(std::mem::size_of::<PaletteNoteEntry>())
            .unwrap_or(u64::MAX)
            .saturating_mul(2);
        let exact_bytes = retained.saturating_add(vector_overlap);
        let limits = |construction_bytes| NoteSourceLimits {
            entries: 1,
            searchable_text_bytes: usize::MAX,
            retained_bytes: u64::MAX,
            sidecar_entries: 1,
            sidecar_path_bytes: u64::MAX,
            construction_bytes,
            diagnostics: 1,
        };

        let mut exact = NoteSourceAdmission::with_limits(limits(exact_bytes));
        assert!(exact.admit(entry.clone()));
        let PaletteNoteSourceOutcome::Complete { load, metrics } = exact.complete() else {
            unreachable!("exact construction boundary completes");
        };
        assert_eq!(load.entries.len(), 1);
        assert_eq!(metrics.peak_construction_bytes, exact_bytes);

        let mut one_under = NoteSourceAdmission::with_limits(limits(exact_bytes.saturating_sub(1)));
        assert!(!one_under.admit(entry));
        assert!(
            one_under
                .metrics
                .truncation_reasons
                .contains(&NoteSourceTruncationReason::ConstructionByteLimit)
        );
    }

    #[test]
    fn bounded_note_admission_charges_activation_target_paths_and_vector_capacity() {
        let mut path = PathBuf::from("/workspace/document.md");
        path.reserve(8 * 1024);
        let mut folder = PathBuf::from("/workspace");
        folder.reserve(16 * 1024);
        let mut workspace_folders = Vec::with_capacity(32);
        workspace_folders.push(folder);
        let entry = PaletteNoteEntry {
            category: PaletteNoteCategory::DocumentNotes,
            title: "Document".to_string(),
            subtitle: "Workspace".to_string(),
            detail: None,
            note_text: Some("body".to_string()),
            target: PaletteNoteTarget::DocumentNote {
                path,
                workspace_folders,
            },
        };
        let target_bytes = entry.target.retained_heap_byte_weight();
        let complete_bytes = u64::try_from(std::mem::size_of::<PaletteNoteEntry>())
            .unwrap_or(u64::MAX)
            .saturating_add(entry.retained_heap_byte_weight());
        assert!(target_bytes > 0);
        let mut admission = NoteSourceAdmission::with_limits(NoteSourceLimits {
            entries: 4,
            searchable_text_bytes: usize::MAX,
            retained_bytes: complete_bytes.saturating_sub(target_bytes),
            sidecar_entries: 4,
            sidecar_path_bytes: u64::MAX,
            construction_bytes: u64::MAX,
            diagnostics: 4,
        });

        assert!(!admission.admit(entry));
        let PaletteNoteSourceOutcome::Complete { load, metrics } = admission.complete() else {
            unreachable!("completed admission must publish a bounded source");
        };
        assert!(load.entries.is_empty());
        assert_eq!(metrics.retained_bytes, 0);
        assert_eq!(
            load.truncation_reasons,
            vec![NoteSourceTruncationReason::RetainedByteLimit]
        );
    }

    #[test]
    fn cancelled_note_source_never_publishes_partial_rows() {
        let dir = TempDir::new().expect("tempdir");
        let scope = WorkspacesFile {
            current_scope: WorkspaceScope::All,
            workspaces: Vec::new(),
        }
        .current_scope_snapshot();
        let cancellation = PaletteSearchCancellation::default();
        assert!(cancellation.cancel());

        let outcome = load_palette_note_entries_for_scope(dir.path(), &scope, &[], &cancellation)
            .expect("cancelled load");

        assert!(matches!(
            outcome,
            PaletteNoteSourceOutcome::Cancelled {
                metrics: NoteSourceMetrics {
                    retained_entries: 0,
                    ..
                }
            }
        ));
    }

    #[test]
    fn note_refresh_coordinator_keeps_only_the_latest_pending_request() {
        let scope = WorkspacesFile {
            current_scope: WorkspaceScope::All,
            workspaces: Vec::new(),
        }
        .current_scope_snapshot();
        let request = |name: &str, mode| NoteSourceRefreshRequest {
            data_dir: PathBuf::from(name),
            scope_snapshot: scope.clone(),
            open_editor_snapshots: Arc::from([]),
            open_editor_snapshots_truncated: false,
            mode,
            limits: PALETTE_NOTE_SOURCE_LIMITS,
        };
        let mut coordinator = NoteSourceRefreshCoordinator::default();

        let first = coordinator
            .submit(request("first", NotesBrowserMode::AllNotes))
            .expect("first request starts");
        assert!(
            coordinator
                .submit(request("second", NotesBrowserMode::AllNotes))
                .is_none()
        );
        assert!(
            coordinator
                .submit(request("latest", NotesBrowserMode::Bookmarks))
                .is_none()
        );
        assert!(first.cancellation.is_cancelled());
        assert_eq!(
            coordinator.snapshot(),
            NoteSourceRefreshCoordinatorSnapshot {
                active: 1,
                pending: 1,
                started: 1,
                cancellation_requests: 1,
            }
        );

        let latest = coordinator
            .finish(first.generation)
            .expect("latest pending request starts");
        assert_eq!(latest.request.data_dir, PathBuf::from("latest"));
        assert_eq!(latest.request.mode, NotesBrowserMode::Bookmarks);
        assert!(coordinator.is_current(latest.generation));
        assert_eq!(coordinator.snapshot().started, 2);
    }

    #[test]
    fn build_note_entries_preserves_note_category_order() {
        let dir = TempDir::new().expect("tempdir");
        let root = dir.path().join("workspace");
        let file = root.join("src/main.rs");
        write_file(&file, "fn main() {}\n");
        fixture::create_dir_all(&root);
        let workspaces = vec![workspace("ws", "Core", vec![root.clone()])];

        let entries = build_note_entries(
            &workspaces,
            vec![bookmark_service::WorkspaceBookmark {
                path: file.clone(),
                bookmark_id: crate::model::bookmark::BookmarkId("bookmark-a".to_string()),
                line: 6,
                label: Some("Important bookmark".to_string()),
            }],
            vec![folder_note_service::ListedFolderNote {
                workspace_name: "Core".to_string(),
                folder: root,
                note: RichNoteBody::new("Folder mission"),
            }],
            vec![document_note_service::WorkspaceDocumentNote {
                path: file,
                note: RichNoteBody::new("Document rationale"),
            }],
            Vec::new(),
            dir.path(),
        );

        assert_eq!(
            categories(&entries),
            vec![
                PaletteNoteCategory::Bookmarks,
                PaletteNoteCategory::FolderNotes,
                PaletteNoteCategory::DocumentNotes,
            ]
        );
        assert_eq!(entries[0].title, "Bookmark · Important bookmark");
        assert_eq!(entries[1].detail.as_deref(), Some("Folder mission"));
        assert_eq!(entries[2].detail.as_deref(), Some("Document rationale"));
    }

    #[test]
    fn search_note_entries_matches_metadata_and_note_bodies() {
        let dir = TempDir::new().expect("tempdir");
        let root = dir.path().join("workspace");
        let folder = root.join("docs");
        let file = folder.join("guide.md");
        write_file(&file, "visible source text\n");
        let workspaces = vec![workspace("ws", "Product Docs", vec![root])];
        let entries = build_note_entries(
            &workspaces,
            vec![bookmark_service::WorkspaceBookmark {
                path: file.clone(),
                bookmark_id: crate::model::bookmark::BookmarkId("bookmark-a".to_string()),
                line: 12,
                label: Some("Launch checklist".to_string()),
            }],
            vec![folder_note_service::ListedFolderNote {
                workspace_name: "Product Docs".to_string(),
                folder,
                note: RichNoteBody::new("Folder body has migration plan"),
            }],
            vec![document_note_service::WorkspaceDocumentNote {
                path: file,
                note: RichNoteBody::new("Document body has launch narrative"),
            }],
            Vec::new(),
            dir.path(),
        );

        assert_eq!(
            search_note_entries(&entries, "launch checklist", 10).len(),
            1
        );
        assert_eq!(search_note_entries(&entries, "Line 13", 10).len(), 1);
        assert_eq!(search_note_entries(&entries, "Product Docs", 10).len(), 3);
        assert_eq!(search_note_entries(&entries, "docs/guide.md", 10).len(), 2);
        assert_eq!(search_note_entries(&entries, "migration plan", 10).len(), 1);
        assert_eq!(
            search_note_entries(&entries, "launch narrative", 10).len(),
            1
        );
    }

    #[test]
    fn palette_note_text_query_matches_trimmed_query_and_preserves_prefix_table() {
        let query = PaletteNoteTextQuery::new("  Launch Plan  ").expect("query");
        let overlap_query = PaletteNoteTextQuery::new("ababaca").expect("overlap query");

        assert!(query.matches("the launch plan is ready"));
        assert!(overlap_query.matches("prefix abababaca suffix"));
        assert!(!overlap_query.matches("prefix ababaxyca suffix"));
        assert_eq!(
            PaletteNoteTextQuery::prefix_table(&"ababaca".chars().collect::<Vec<_>>()),
            vec![0, 0, 1, 2, 3, 0, 1]
        );
        assert_eq!(
            PaletteNoteTextQuery::prefix_table(&"ababb".chars().collect::<Vec<_>>()),
            vec![0, 0, 1, 2, 0]
        );
    }

    #[test]
    fn large_note_body_remains_eligible_without_unbounded_fuzzy_scoring() {
        let body = format!(
            "{} bounded-tail-needle",
            "x".repeat(MAX_NOTE_FUZZY_SCORE_BYTES + 1)
        );
        let entries = vec![test_note_entry(
            PaletteNoteCategory::DocumentNotes,
            "Unrelated title",
            &body,
        )];
        let cancellation = PaletteSearchCancellation::default();

        let outcome = search_note_entries_cancellable(
            &entries,
            None,
            "bounded-tail-needle",
            10,
            &cancellation,
        );
        let PaletteSearchOutcome::Complete { value, metrics } = outcome else {
            panic!("fresh search should complete");
        };

        assert_eq!(value.len(), 1);
        assert_eq!(value[0].score, 0);
        assert_eq!(metrics.candidates_examined, 1);
        assert_eq!(metrics.candidates_scored, 1);
        assert_eq!(metrics.note_bodies_examined, 1);
        assert_eq!(metrics.note_bodies_safely_pruned, 0);
    }

    #[test]
    fn metadata_match_prunes_only_a_body_with_no_possible_score_improvement() {
        let body = format!(
            "{} launch checklist appears again",
            "x".repeat(MAX_NOTE_FUZZY_SCORE_BYTES + 1)
        );
        let entries = vec![test_note_entry(
            PaletteNoteCategory::DocumentNotes,
            "Launch checklist",
            &body,
        )];
        let evidence =
            note_scoring_equivalence_for_property_test(&entries, None, "launch checklist", 10);

        assert_eq!(
            evidence.optimized, evidence.unpruned_reference,
            "pruning must preserve the selected row and score"
        );
        assert_eq!(evidence.optimized.len(), 1);
        assert_eq!(evidence.optimized_metrics.candidates_scored, 1);
        assert_eq!(evidence.optimized_metrics.note_bodies_examined, 0);
        assert_eq!(evidence.optimized_metrics.note_bodies_safely_pruned, 1);
        assert_eq!(evidence.reference_bodies_examined, 1);
    }

    #[test]
    fn score_bounds_and_empty_query_preserve_body_and_source_ordinal_contracts() {
        assert_eq!(note_field_score_upper_bound("small"), u32::from(u16::MAX));
        assert_eq!(
            note_field_score_upper_bound(&"x".repeat(MAX_NOTE_FUZZY_SCORE_BYTES + 1)),
            0
        );

        let entries = vec![
            test_note_entry(
                PaletteNoteCategory::FolderNotes,
                "Café 東京",
                &format!("{} café 東京", "x".repeat(MAX_NOTE_FUZZY_SCORE_BYTES + 1)),
            ),
            test_note_entry(
                PaletteNoteCategory::FolderNotes,
                "Café 東京",
                "body-only needle",
            ),
            test_note_entry(
                PaletteNoteCategory::DocumentNotes,
                "Different",
                "body-only needle",
            ),
        ];
        let unicode = note_scoring_equivalence_for_property_test(&entries, None, "CAFÉ 東京", 2);
        assert_eq!(unicode.optimized, unicode.unpruned_reference);
        assert_eq!(unicode.optimized[0].source_ordinal, 0);

        let empty = note_scoring_equivalence_for_property_test(&entries, None, "  ", 2);
        assert_eq!(empty.optimized, empty.unpruned_reference);
        assert_eq!(
            empty
                .optimized
                .iter()
                .map(|rank| rank.source_ordinal)
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert_eq!(empty.optimized_metrics.candidates_scored, 0);
        assert_eq!(empty.optimized_metrics.note_bodies_examined, 0);
        assert_eq!(empty.optimized_metrics.note_bodies_safely_pruned, 0);
    }

    #[test]
    fn note_text_query_observes_preexisting_cancellation() {
        let query = PaletteNoteTextQuery::new("needle").expect("query");
        let cancellation = PaletteSearchCancellation::default();
        let _ = cancellation.cancel();

        assert_eq!(
            query.matches_cancellable(&"x".repeat(8_192), &cancellation),
            None
        );
    }

    #[test]
    fn search_note_entries_in_category_limits_matches_to_that_category() {
        let entries = vec![
            test_note_entry(
                PaletteNoteCategory::FolderNotes,
                "Folder launch note",
                "shared body",
            ),
            test_note_entry(
                PaletteNoteCategory::DocumentNotes,
                "Document launch note",
                "shared body",
            ),
        ];

        let document_hits = search_note_entries_in_category(
            &entries,
            PaletteNoteCategory::DocumentNotes,
            "shared body",
            10,
        );
        assert_eq!(document_hits.len(), 1);
        assert_eq!(document_hits[0].title, "Document launch note");

        let default_folder_hits =
            search_note_entries_in_category(&entries, PaletteNoteCategory::FolderNotes, "", 10);
        assert_eq!(default_folder_hits.len(), 1);
        assert_eq!(default_folder_hits[0].title, "Folder launch note");

        let whitespace_folder_hits = search_note_entries_in_category(
            &entries,
            PaletteNoteCategory::FolderNotes,
            "   \t ",
            10,
        );
        assert_eq!(whitespace_folder_hits.len(), 1);
        assert_eq!(whitespace_folder_hits[0].title, "Folder launch note");

        let padded_document_hits = search_note_entries_in_category(
            &entries,
            PaletteNoteCategory::DocumentNotes,
            "  shared body  ",
            10,
        );
        assert_eq!(padded_document_hits.len(), 1);
        assert_eq!(padded_document_hits[0].title, "Document launch note");
    }

    #[test]
    fn search_note_entries_does_not_match_bookmark_source_excerpt_text() {
        let dir = TempDir::new().expect("tempdir");
        let root = dir.path().join("workspace");
        let file = root.join("source.md");
        write_file(&file, "needle-from-source-only\n");
        let workspaces = vec![workspace("ws", "Core", vec![root])];
        let entries = build_note_entries(
            &workspaces,
            vec![bookmark_service::WorkspaceBookmark {
                path: file,
                bookmark_id: crate::model::bookmark::BookmarkId("bookmark-a".to_string()),
                line: 0,
                label: Some("Bookmark label".to_string()),
            }],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            dir.path(),
        );

        assert!(search_note_entries(&entries, "needle-from-source-only", 10).is_empty());
    }

    #[test]
    fn note_path_scope_and_open_tab_source_labels_are_exact() {
        let folder = PathBuf::from("/workspace/root");
        assert!(path_is_in_folders(
            Path::new("/workspace/root/docs/file.md"),
            std::slice::from_ref(&folder)
        ));
        assert!(!path_is_in_folders(
            Path::new("/workspace/rootish/docs/file.md"),
            std::slice::from_ref(&folder)
        ));

        let in_workspace = PaletteOpenTabSource {
            workspace_name: Some("Docs".to_string()),
            workspace_folder: Some(folder.clone()),
        };
        assert_eq!(
            in_workspace.row_label(),
            format!("Open tab · Docs · {}", folder.display())
        );
        assert_eq!(
            PaletteOpenTabSource {
                workspace_name: Some("Scratch".to_string()),
                workspace_folder: None,
            }
            .row_label(),
            "Open tab · Scratch"
        );
        assert_eq!(
            PaletteOpenTabSource {
                workspace_name: None,
                workspace_folder: None,
            }
            .row_label(),
            "Open tab · Outside workspace"
        );
    }

    #[test]
    fn document_identity_dedupe_and_label_sorting_helpers_preserve_note_rows() {
        let dir = TempDir::new().expect("tempdir");
        let file = dir.path().join("doc.md");
        write_file(&file, "# doc\n");
        let mut document_ids = HashSet::new();

        remember_document_identity(&mut document_ids, &file);
        remember_document_identity(&mut document_ids, &dir.path().join("missing.md"));

        assert_eq!(document_ids.len(), 1);

        let mut entries = vec![
            test_note_entry(PaletteNoteCategory::DocumentNotes, "Zeta", "body"),
            test_note_entry(PaletteNoteCategory::DocumentNotes, "Alpha", "body"),
            PaletteNoteEntry {
                subtitle: "A subtitle".to_string(),
                ..test_note_entry(PaletteNoteCategory::DocumentNotes, "Alpha", "body")
            },
        ];

        sort_note_entries_by_label(&mut entries);

        assert_eq!(
            entries
                .iter()
                .map(|entry| (entry.title.as_str(), entry.subtitle.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("Alpha", "A subtitle"),
                ("Alpha", "Core · /workspace"),
                ("Zeta", "Core · /workspace"),
            ]
        );
    }

    #[test]
    fn overlapping_workspace_folders_keep_user_order_for_note_context() {
        let dir = TempDir::new().expect("tempdir");
        let root = dir.path().join("workspace");
        let nested = root.join("src");
        let file = nested.join("main.rs");
        write_file(&file, "fn main() {}\n");

        let parent_first = build_note_entries(
            &[workspace("ws", "Core", vec![root.clone(), nested.clone()])],
            Vec::new(),
            Vec::new(),
            vec![document_note_service::WorkspaceDocumentNote {
                path: file.clone(),
                note: RichNoteBody::new("Parent context"),
            }],
            Vec::new(),
            dir.path(),
        );
        let nested_first = build_note_entries(
            &[workspace("ws", "Core", vec![nested.clone(), root.clone()])],
            Vec::new(),
            Vec::new(),
            vec![document_note_service::WorkspaceDocumentNote {
                path: file,
                note: RichNoteBody::new("Nested context"),
            }],
            Vec::new(),
            dir.path(),
        );

        assert!(matches!(
            &parent_first[0].target,
            PaletteNoteTarget::DocumentNote { workspace_folders, .. }
                if workspace_folders.as_slice() == std::slice::from_ref(&root)
        ));
        assert!(matches!(
            &nested_first[0].target,
            PaletteNoteTarget::DocumentNote { workspace_folders, .. }
                if workspace_folders == &vec![nested]
        ));
    }

    #[test]
    fn open_tab_notes_are_supplemental_and_deduplicated_from_scoped_documents() {
        let dir = TempDir::new().expect("tempdir");
        let scoped_root = dir.path().join("scoped");
        let outside_root = dir.path().join("outside");
        let scoped_file = scoped_root.join("main.rs");
        let outside_file = outside_root.join("outside.md");
        write_file(&scoped_file, "scoped\n");
        write_file(&outside_file, "outside\n");
        let visible = vec![workspace("scoped", "Scoped", vec![scoped_root])];
        let all = vec![
            visible[0].clone(),
            workspace("outside", "Outside", vec![outside_root]),
        ];
        document_note_service::save_for_path(
            dir.path(),
            &outside_file,
            &RichNoteBody::new("Open tab document note"),
        )
        .expect("save outside note");

        let entries = build_note_entries(
            &visible,
            vec![bookmark_service::WorkspaceBookmark {
                path: scoped_file.clone(),
                bookmark_id: crate::model::bookmark::BookmarkId("bookmark-scoped".to_string()),
                line: 0,
                label: Some("Scoped bookmark".to_string()),
            }],
            Vec::new(),
            Vec::new(),
            vec![
                PaletteOpenEditorNoteSnapshot {
                    path: scoped_file,
                    bookmarks: vec![BookmarkRecord::new(1, Some("Live scoped".to_string()))],
                    open_tab_source: None,
                },
                PaletteOpenEditorNoteSnapshot {
                    path: outside_file.clone(),
                    bookmarks: vec![BookmarkRecord::new(3, Some("Outside tab".to_string()))],
                    open_tab_source: Some(open_tab_source_for_path(&all, &outside_file)),
                },
            ],
            dir.path(),
        );

        assert_eq!(
            categories(&entries),
            vec![
                PaletteNoteCategory::Bookmarks,
                PaletteNoteCategory::OpenTabs,
                PaletteNoteCategory::OpenTabs,
            ]
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.title == "Bookmark · Scoped bookmark")
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.title == "Bookmark · Outside tab")
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.title == "Document Note · outside.md")
        );
    }

    #[test]
    fn load_note_entries_overlays_live_bookmarks_for_open_scoped_documents() {
        let dir = TempDir::new().expect("tempdir");
        let root = dir.path().join("workspace");
        let file = root.join("main.rs");
        write_file(&file, "fn main() {}\n");
        bookmark_service::save_for_path(
            dir.path(),
            &file,
            &[BookmarkRecord::new(
                0,
                Some("Persisted bookmark".to_string()),
            )],
        )
        .expect("save bookmark sidecar");
        let workspaces_file = WorkspacesFile {
            current_scope: WorkspaceScope::All,
            workspaces: vec![workspace("ws", "Core", vec![root])],
        };
        let scope_snapshot = workspaces_file.current_scope_snapshot();

        let load = load_note_entries_for_scope(
            dir.path(),
            &scope_snapshot,
            vec![PaletteOpenEditorNoteSnapshot {
                path: file,
                bookmarks: vec![BookmarkRecord::new(3, Some("Live bookmark".to_string()))],
                open_tab_source: None,
            }],
        )
        .expect("load notes");

        assert!(
            load.entries
                .iter()
                .any(|entry| entry.title == "Bookmark · Live bookmark")
        );
        assert!(
            !load
                .entries
                .iter()
                .any(|entry| entry.title == "Bookmark · Persisted bookmark")
        );
    }
}
