// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination role `execution`, stage-order-qualified: bounded note-source
//! construction.
//!
//! One coordination job with **two consumers**. The notes browser dialog and the
//! command palette both need the same bounded note-source build — the same
//! sidecar scan, the same admission ledger, the same disposal reservation, and
//! the same one-active/one-latest ownership — so they share this module rather
//! than each growing a copy. The stage-order qualifier is `source`; the sibling
//! `query_execution` owns the second stage order over the published source.
//!
//! # Inversions
//!
//! 1. **Disposal admission may refuse.** A compact request parks in
//!    `source_admission` and resumes in the source capacity wakeup, which
//!    re-submits it through [`retry_notes_browser_source_admission`] or
//!    `retry_command_palette_note_admission`.
//! 2. **The scan runs on a worker.** Control returns once
//!    `spawn_blocking_then` is dispatched and resumes in
//!    [`finish_notes_browser_source_load`] (browser) or
//!    `finish_command_palette_note_refresh` (palette), each validating a
//!    `seams::NotesBrowserTicket` against live facts before publishing.
//! 3. **The palette's refresh is debounced.** A live note or bookmark burst
//!    resumes once, after the quiet window, in the debounce callback.
//! 4. **The browser's open-editor capture is sliced.** It runs in turns from a
//!    `G_PRIORITY_DEFAULT_IDLE` source owned by the browser state, each turn
//!    taking units while `model::list_budget::turn_continues` holds, and
//!    resumes in [`run_open_editor_capture_turn`] until every editor is
//!    captured; only then is the worker request submitted.
//!
//! Worker-side guarding lives in [`guard_note_source_on_worker`], which reserves
//! the retained-byte weight the load actually produced and shrinks the
//! reservation to it, so a rejected or stale result is retired off the GTK thread
//! rather than freed inside a completion turn.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;

use crate::model::bookmark::BookmarkRecord;
use crate::model::list_budget::{Admission, RetainedBudget, turn_continues};
use crate::model::palette::{
    PaletteNoteEntry, PaletteOpenEditorNoteSnapshot, PaletteOpenTabSource,
};
use crate::model::workspace::{WorkspaceConfig, WorkspaceScopeSnapshot};
use crate::services::palette::{
    NoteSourceRefreshRequest, NoteSourceRefreshStart, PaletteNoteSourceOutcome,
};
use crate::services::recovery_metadata::RecoveryDiagnostic;
use crate::services::{json_store, palette as palette_service};
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::status_bar::MessageKind;

use super::browser::{NotesBrowserEntry, NotesBrowserState, OpenEditorNoteSnapshots};
use super::policy;
use super::policy::{
    CaptureSliceDecision, NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT, NotesBrowserListedCount,
    NotesBrowserModeExt as _, capture_slice_decision, capture_slice_rows,
};
use super::query_execution::{submit_notes_browser_query, update_notes_browser_limit_label};
use super::seams::{NotesBrowserFacts, NotesBrowserTicket, SourceFlight};
use super::{LushtextWindow, notes_browser_source_limits};

enum GuardedNoteSourceOutcome {
    Complete {
        load: palette_service::PaletteNoteSourceLoad,
        entries: crate::ui::plain_disposal::DisposalOwned<Box<[PaletteNoteEntry]>>,
        had_recovery_diagnostics: bool,
    },
    Cancelled,
    Failed(anyhow::Error),
}

fn guard_note_source_on_worker(
    result: anyhow::Result<PaletteNoteSourceOutcome>,
    reservation: crate::ui::plain_disposal::DisposalReservation,
) -> GuardedNoteSourceOutcome {
    match result {
        Ok(PaletteNoteSourceOutcome::Complete { mut load, metrics }) => {
            let diagnostics = std::mem::take(&mut load.diagnostics);
            let had_recovery_diagnostics = !diagnostics.is_empty();
            LushtextWindow::trace_browse_recovery_diagnostics(&diagnostics);
            drop(diagnostics);
            let entries = std::mem::take(&mut load.entries).into_boxed_slice();
            debug_assert_eq!(
                metrics.retained_bytes,
                crate::model::palette::palette_note_entries_retained_byte_weight(&entries)
            );
            debug_assert!(
                metrics.retained_bytes <= palette_service::MAX_PALETTE_NOTE_RETAINED_BYTES
            );
            GuardedNoteSourceOutcome::Complete {
                load,
                entries: reservation.shrink_to_and_own(metrics.retained_bytes, entries),
                had_recovery_diagnostics,
            }
        }
        Ok(PaletteNoteSourceOutcome::Cancelled { .. }) => GuardedNoteSourceOutcome::Cancelled,
        Err(error) => GuardedNoteSourceOutcome::Failed(error),
    }
}

impl LushtextWindow {
    pub(super) fn trace_browse_recovery_diagnostics(diagnostics: &[RecoveryDiagnostic]) {
        for diagnostic in diagnostics {
            tracing::warn!("{}", diagnostic.summary());
        }
    }
    /// Snapshot open saved-editor note state without touching the filesystem.
    ///
    /// This runs on the GTK main thread because `bookmark_records()` reads the
    /// live `GtkSourceMark` projection. Sidecar loading and identity
    /// deduplication stay in the existing background browse task.
    pub(super) fn open_editor_note_snapshots_bounded(
        &self,
        scope_folders: &[PathBuf],
        all_workspaces: &[WorkspaceConfig],
        max_snapshots_and_bookmarks: usize,
        max_retained_bytes: u64,
    ) -> OpenEditorNoteSnapshots {
        let tab_view = &self.imp().tab_view;
        let snapshot_size = std::mem::size_of::<PaletteOpenEditorNoteSnapshot>();
        let page_count = usize::try_from(tab_view.n_pages()).unwrap_or(usize::MAX);
        let capacity = policy::open_editor_snapshot_capacity(
            max_snapshots_and_bookmarks,
            page_count,
            max_retained_bytes,
            snapshot_size,
        );
        let mut snapshots = Vec::with_capacity(capacity);
        let mut retained_bytes =
            policy::open_editor_snapshot_reserved_bytes(capacity, snapshot_size);
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
            let snapshot_heap_bytes = policy::open_editor_snapshot_heap_bytes(
                path.capacity(),
                open_tab_source
                    .as_ref()
                    .and_then(|source| source.workspace_name.as_ref().map(String::capacity)),
                open_tab_source.as_ref().and_then(|source| {
                    source
                        .workspace_folder
                        .as_ref()
                        .map(std::path::PathBuf::capacity)
                }),
            );
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
            rows_not_retained: 0,
        }
    }
    /// Start the browser's source for `mode`: present the loading label, then
    /// capture the open editors in sliced turns; the worker request follows the
    /// last turn (`bound-notes-browser-by-budget` D4).
    pub(super) fn submit_notes_browser_source(
        &self,
        state: &Rc<NotesBrowserState>,
        mode: palette_service::NotesBrowserMode,
    ) {
        let workspaces_file = self.imp().sidebar.workspaces_file();
        let scope_snapshot = workspaces_file.current_scope_snapshot();
        let all_workspaces = workspaces_file.workspaces;
        let tab_view = &self.imp().tab_view;
        let editors = (0..tab_view.n_pages())
            .filter_map(|index| {
                tab_view
                    .nth_page(index)
                    .child()
                    .downcast::<LushtextEditorPage>()
                    .ok()
            })
            .map(|editor| editor.downgrade())
            .collect();
        state.limit_label.set_label(mode.loading_label());
        state.limit_label.set_visible(true);
        cancel_open_editor_capture(state);
        state
            .snapshot_capture
            .replace(Some(OpenEditorSnapshotCapture {
                mode,
                scope_snapshot,
                all_workspaces,
                editors,
                next_editor: 0,
                current: None,
                entries: Vec::new(),
                budget: RetainedBudget::new(NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT),
                counting_only: false,
                rows_not_retained: 0,
            }));
        let state_weak = Rc::downgrade(state);
        let source = glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
            let Some(state) = state_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            run_open_editor_capture_turn(&state)
        });
        state.snapshot_source.replace(Some(source));
    }

    /// Coalesce cached note-row refreshes after bursty note and bookmark edits.
    pub(in crate::ui::window) fn refresh_command_palette_note_source_debounced(&self) {
        if !self.imp().palette_revealer.reveals_child() {
            self.invalidate_command_palette_note_source();
            return;
        }

        self.imp().command_palette_notes_refresh_debounce.schedule(
            self,
            Duration::from_millis(super::policy::COMMAND_PALETTE_NOTES_REFRESH_DEBOUNCE_MS),
            |window, _| {
                window.refresh_command_palette_note_source();
            },
        );
    }

    /// Refresh cached command-palette note rows from the current workspace scope.
    ///
    /// The GTK thread only snapshots open-editor bookmark metadata here. Sidecar
    /// listing and document identity work stay in the background task, and the
    /// generation guard prevents stale completions from replacing newer rows.
    pub(in crate::ui::window) fn refresh_command_palette_note_source(&self) {
        if !self.imp().palette_revealer.reveals_child() {
            self.invalidate_command_palette_note_source();
            return;
        }

        let workspaces_file = self.imp().sidebar.workspaces_file();
        let scope_snapshot = workspaces_file.current_scope_snapshot();
        let all_workspaces = workspaces_file.workspaces;
        let open_editor_snapshots = self.open_editor_note_snapshots_bounded(
            scope_snapshot.folder_paths(),
            &all_workspaces,
            palette_service::MAX_PALETTE_NOTE_ENTRIES,
            NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT,
        );
        debug_assert!(
            open_editor_snapshots.retained_bytes <= NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT
        );
        let request = NoteSourceRefreshRequest {
            data_dir: json_store::data_dir(),
            scope_snapshot,
            open_editor_snapshots: Arc::from(open_editor_snapshots.entries),
            open_editor_snapshots_truncated: open_editor_snapshots.truncated,
            open_editor_rows_not_retained: open_editor_snapshots.rows_not_retained,
            mode: palette_service::NotesBrowserMode::AllNotes,
            limits: palette_service::PALETTE_NOTE_SOURCE_LIMITS,
        };
        let start = self
            .imp()
            .command_palette_note_refreshes
            .borrow_mut()
            .submit(request);
        if let Some(start) = start {
            self.start_command_palette_note_refresh(start);
        } else {
            self.finish_cancelled_command_palette_note_admission();
        }
    }

    fn start_command_palette_note_refresh(&self, start: NoteSourceRefreshStart) {
        if start.cancellation.is_cancelled() {
            self.finish_command_palette_note_refresh(
                start.generation,
                GuardedNoteSourceOutcome::Cancelled,
            );
            return;
        }
        let observed_epoch = crate::ui::plain_disposal::disposal_capacity_epoch();
        let weight = palette_service::MAX_PALETTE_NOTE_RETAINED_BYTES;
        let reservation = self
            .imp()
            .command_palette
            .note_source_reservation_weight()
            .map_or_else(
                || crate::ui::plain_disposal::try_reserve_for_gtk(weight),
                |current_weight| {
                    crate::ui::plain_disposal::try_reserve_replacement_for_gtk(
                        weight,
                        current_weight,
                    )
                },
            );
        let Some(reservation) = reservation else {
            debug_assert!(self.imp().command_palette_note_admission.borrow().is_none());
            self.imp()
                .command_palette_note_admission
                .replace(Some(start));
            let window_weak = self.downgrade();
            self.imp()
                .command_palette_note_capacity_wakeup
                .arm(observed_epoch, move || {
                    if let Some(window) = window_weak.upgrade() {
                        window.retry_command_palette_note_admission();
                    }
                });
            if self.imp().palette_revealer.reveals_child() {
                self.publish_status_message(
                    "Command palette note update deferred by memory pressure",
                    MessageKind::Warning,
                );
            }
            return;
        };

        let NoteSourceRefreshStart {
            generation,
            request,
            cancellation,
        } = start;
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                guard_note_source_on_worker(
                    palette_service::load_note_entries_bounded_for_scope(
                        &request.data_dir,
                        &request.scope_snapshot,
                        &request.open_editor_snapshots,
                        request.open_editor_snapshots_truncated,
                        request.open_editor_rows_not_retained,
                        request.mode,
                        request.limits,
                        &cancellation,
                    ),
                    reservation,
                )
            },
            move |(), result| {
                let Some(window) = window_weak.upgrade() else {
                    retire_note_source_result(result);
                    return;
                };
                window.finish_command_palette_note_refresh(generation, result);
            },
        );
    }

    fn retry_command_palette_note_admission(&self) {
        let Some(start) = self
            .imp()
            .command_palette_note_admission
            .borrow_mut()
            .take()
        else {
            return;
        };
        self.start_command_palette_note_refresh(start);
    }

    fn finish_cancelled_command_palette_note_admission(&self) {
        let cancelled = self
            .imp()
            .command_palette_note_admission
            .borrow()
            .as_ref()
            .is_some_and(|start| start.cancellation.is_cancelled());
        if !cancelled {
            return;
        }
        self.imp().command_palette_note_capacity_wakeup.cancel();
        let start = self
            .imp()
            .command_palette_note_admission
            .borrow_mut()
            .take();
        if let Some(start) = start {
            self.finish_command_palette_note_refresh(
                start.generation,
                GuardedNoteSourceOutcome::Cancelled,
            );
        }
    }

    fn finish_command_palette_note_refresh(
        &self,
        generation: u64,
        result: GuardedNoteSourceOutcome,
    ) {
        let (accepted, next) = {
            let mut refreshes = self.imp().command_palette_note_refreshes.borrow_mut();
            let accepted = refreshes.is_current(generation);
            let next = refreshes.finish(generation);
            (accepted, next)
        };

        if accepted {
            match result {
                GuardedNoteSourceOutcome::Complete {
                    load,
                    entries,
                    had_recovery_diagnostics,
                } => {
                    let was_truncated = !load.truncation_reasons.is_empty();
                    self.imp().command_palette.set_guarded_note_entries(entries);
                    if was_truncated && self.imp().palette_revealer.reveals_child() {
                        self.publish_status_message(
                            "Command palette note source was limited to stay responsive",
                            MessageKind::Warning,
                        );
                    } else if had_recovery_diagnostics
                        && self.imp().palette_revealer.reveals_child()
                    {
                        self.publish_status_message(
                            "Some note data could not be loaded for the palette",
                            MessageKind::Warning,
                        );
                    }
                }
                GuardedNoteSourceOutcome::Cancelled => {}
                GuardedNoteSourceOutcome::Failed(error) => {
                    tracing::warn!("Failed to refresh command-palette notes: {error}");
                    if self.imp().palette_revealer.reveals_child() {
                        self.publish_status_message(
                            "Notes could not be loaded for the palette",
                            MessageKind::Warning,
                        );
                    }
                }
            }
        } else {
            retire_note_source_result(result);
        }

        if let Some(next) = next {
            self.start_command_palette_note_refresh(next);
        }
    }

    fn invalidate_command_palette_note_source(&self) {
        self.imp()
            .command_palette_note_refreshes
            .borrow_mut()
            .invalidate();
        self.imp()
            .command_palette_note_admission
            .borrow_mut()
            .take();
        self.imp().command_palette_note_capacity_wakeup.cancel();
        self.imp().command_palette.clear_note_entries();
    }
}

fn retire_note_source_result(result: GuardedNoteSourceOutcome) {
    drop(result);
}

pub(super) fn start_notes_browser_source_load(
    state: &Rc<NotesBrowserState>,
    start: NoteSourceRefreshStart,
) {
    if start.cancellation.is_cancelled() {
        finish_notes_browser_source_load(
            state,
            start.generation,
            start.request.mode,
            GuardedNoteSourceOutcome::Cancelled,
        );
        return;
    }
    let observed_epoch = crate::ui::plain_disposal::progress_disposal_capacity_epoch();
    let weight = palette_service::MAX_PALETTE_NOTE_RETAINED_BYTES;
    let reservation = state.all_entries.borrow().reservation_weight().map_or_else(
        || crate::ui::plain_disposal::try_reserve_progress_for_gtk(weight),
        |current_weight| {
            crate::ui::plain_disposal::try_reserve_progress_replacement_for_gtk(
                weight,
                current_weight,
            )
        },
    );
    let Some(reservation) = reservation else {
        let mode = start.request.mode;
        debug_assert!(state.source_admission.borrow().is_none());
        state.source_admission.replace(Some(start));
        let state_weak = Rc::downgrade(state);
        state.source_capacity_wakeup.arm(observed_epoch, move || {
            if let Some(state) = state_weak.upgrade() {
                retry_notes_browser_source_admission(&state);
            }
        });
        state.limit_label.set_label(mode.deferred_label());
        state.limit_label.set_visible(true);
        state
            .window
            .publish_status_message(mode.deferred_label(), MessageKind::Warning);
        return;
    };

    let NoteSourceRefreshStart {
        generation,
        request,
        cancellation,
    } = start;
    let mode = request.mode;
    let state_weak = Rc::downgrade(state);
    spawn_blocking_then(
        (),
        move || {
            guard_note_source_on_worker(
                palette_service::load_note_entries_bounded_for_scope(
                    &request.data_dir,
                    &request.scope_snapshot,
                    &request.open_editor_snapshots,
                    request.open_editor_snapshots_truncated,
                    request.open_editor_rows_not_retained,
                    request.mode,
                    request.limits,
                    &cancellation,
                ),
                reservation,
            )
        },
        move |(), result| {
            let Some(state) = state_weak.upgrade() else {
                retire_note_source_result(result);
                return;
            };
            finish_notes_browser_source_load(&state, generation, mode, result);
        },
    );
}

pub(super) fn retry_notes_browser_source_admission(state: &Rc<NotesBrowserState>) {
    let Some(start) = state.source_admission.borrow_mut().take() else {
        return;
    };
    start_notes_browser_source_load(state, start);
}

fn finish_notes_browser_source_load(
    state: &Rc<NotesBrowserState>,
    generation: u64,
    mode: palette_service::NotesBrowserMode,
    result: GuardedNoteSourceOutcome,
) {
    let ticket = NotesBrowserTicket::<SourceFlight>::new(generation, mode);
    let (accepted, next) = {
        let mut refreshes = state.source_refreshes.borrow_mut();
        let accepted = ticket.may_publish(&NotesBrowserFacts::new(
            refreshes.is_current(ticket.generation()),
            state.mode.get(),
            state.disposed.get(),
        ));
        let next = refreshes.finish(ticket.generation());
        (accepted, next)
    };
    if accepted {
        match result {
            GuardedNoteSourceOutcome::Complete {
                load,
                entries,
                had_recovery_diagnostics,
            } => {
                let source_truncation = load.truncation_reasons;
                let listed = entries.len();
                let previous = state
                    .all_entries
                    .replace(Arc::new(entries.into_retained_current()));
                drop(previous);
                *state.source_truncation.borrow_mut() = source_truncation;
                state
                    .source_generation
                    .set(state.source_generation.get().wrapping_add(1));
                state.listed_count.set(NotesBrowserListedCount {
                    listed,
                    found: load.rows_found.max(listed),
                    found_is_lower_bound: load.found_is_lower_bound,
                });
                state.source_ready.set(true);
                // The limit notice and its one announcement follow the query
                // publish; a byte cut is no longer a status-bar warning.
                if had_recovery_diagnostics {
                    state.window.publish_status_message(
                        ticket.mode().source_recovery_status_message(),
                        MessageKind::Warning,
                    );
                }
                submit_notes_browser_query(state, state.search_entry.text().to_string());
            }
            GuardedNoteSourceOutcome::Cancelled => {}
            GuardedNoteSourceOutcome::Failed(error) => {
                let mode = ticket.mode();
                tracing::error!("Failed to list {}: {error}", mode.title().to_lowercase());
                state.limit_label.set_label(mode.source_failure_message());
                state.limit_label.set_visible(true);
                state
                    .window
                    .publish_status_message(mode.source_failure_message(), MessageKind::Error);
            }
        }
    } else {
        retire_note_source_result(result);
    }
    if let Some(next) = next {
        start_notes_browser_source_load(state, next);
    }
}

impl NotesBrowserState {
    pub(super) fn begin_mode(&self, mode: palette_service::NotesBrowserMode) {
        let _ = self.search_debounce.invalidate();
        self.query_runtime.borrow_mut().invalidate();
        self.source_ready.set(false);
        self.source_truncation.borrow_mut().clear();
        self.listed_count.set(NotesBrowserListedCount::default());
        self.rows.clear();
        update_notes_browser_limit_label(self);
        self.split_view.set_show_content(false);
        self.preview_loads.borrow_mut().invalidate();
        self.configure_mode(mode);
    }

    /// Cancel source/query publication and release retained browser payloads.
    pub(super) fn dispose_runtime(&self) {
        if self.disposed.replace(true) {
            return;
        }
        let _ = self.search_debounce.invalidate();
        self.source_refreshes.borrow_mut().invalidate();
        self.source_admission.borrow_mut().take();
        self.source_capacity_wakeup.cancel();
        self.query_runtime.borrow_mut().invalidate();
        self.preview_loads.borrow_mut().invalidate();
        self.source_ready.set(false);
        stop_open_editor_capture(self);
        self.rows.clear();
        self.bound_rows.borrow_mut().clear();
        let source = self.all_entries.replace(Arc::new(
            crate::ui::plain_disposal::DisposalOwned::small_unreserved(
                Vec::<NotesBrowserEntry>::new().into_boxed_slice(),
            ),
        ));
        drop(source);
    }
}

/// The browser's sliced open-editor capture (`bound-notes-browser-by-budget`
/// D4): GTK-side work that grows with bookmarks, so it runs in turns and
/// admits bytes through `RetainedBudget` at the unchanged 4 MiB, with no count.
pub(super) struct OpenEditorSnapshotCapture {
    mode: palette_service::NotesBrowserMode,
    scope_snapshot: WorkspaceScopeSnapshot,
    all_workspaces: Vec<WorkspaceConfig>,
    editors: Vec<glib::WeakRef<LushtextEditorPage>>,
    next_editor: usize,
    current: Option<EditorCapture>,
    entries: Vec<PaletteOpenEditorNoteSnapshot>,
    budget: RetainedBudget,
    /// The byte budget latched: later rows are counted, never copied.
    counting_only: bool,
    rows_not_retained: u64,
}

/// One editor's capture in progress.
struct EditorCapture {
    editor: LushtextEditorPage,
    path: PathBuf,
    open_tab_source: Option<PaletteOpenTabSource>,
    /// `bookmark_change_generation()` at the editor's first slice.
    generation: u64,
    next: usize,
    bookmarks: Vec<BookmarkRecord>,
    /// Whether the editor's path snapshot fit the budget (else its rows are
    /// only counted; an in-scope editor whose path did not fit is skipped,
    /// since the worker counts its bookmark sidecar instead).
    path_retained: bool,
    /// Budget state once the path was admitted, restored on a restart.
    charged_before: u64,
    counting_only_before: bool,
    counted: u64,
}

/// Stop a running capture and remove its turn source (dispose, mode switch).
pub(super) fn stop_open_editor_capture(state: &NotesBrowserState) {
    if let Some(source) = state.snapshot_source.borrow_mut().take() {
        source.remove();
    }
    state.snapshot_capture.borrow_mut().take();
}

fn cancel_open_editor_capture(state: &NotesBrowserState) {
    stop_open_editor_capture(state);
}

/// What one unit of the capture did.
enum CaptureUnit {
    /// More units remain.
    Continue,
    /// Every editor is captured.
    Done,
}

#[cfg(feature = "test-utils")]
fn capture_turn_capped_for_test(units: usize) -> bool {
    super::test_policy::capture_turn_capped(units)
}

#[cfg(not(feature = "test-utils"))]
const fn capture_turn_capped_for_test(_units: usize) -> bool {
    false
}

/// Run one capture turn; at its end, submit the source request.
fn run_open_editor_capture_turn(state: &Rc<NotesBrowserState>) -> glib::ControlFlow {
    if state.disposed.get() {
        state.snapshot_source.borrow_mut().take();
        return glib::ControlFlow::Break;
    }
    let start = std::time::Instant::now();
    let mut units = 0usize;
    let mut done = false;
    {
        let mut slot = state.snapshot_capture.borrow_mut();
        let Some(capture) = slot.as_mut() else {
            drop(slot);
            state.snapshot_source.borrow_mut().take();
            return glib::ControlFlow::Break;
        };
        let mut evidence = state.snapshot_evidence.get();
        while turn_continues(
            u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX),
            units,
        ) && !capture_turn_capped_for_test(units)
        {
            units = units.saturating_add(1);
            if matches!(capture_unit(capture, &mut evidence), CaptureUnit::Done) {
                done = true;
                break;
            }
        }
        evidence.turns = evidence.turns.saturating_add(1);
        evidence.worst_turn_us = evidence
            .worst_turn_us
            .max(u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX));
        evidence.rows_counted_not_retained = capture.rows_not_retained;
        state.snapshot_evidence.set(evidence);
    }
    if !done {
        return glib::ControlFlow::Continue;
    }
    state.snapshot_source.borrow_mut().take();
    let Some(capture) = state.snapshot_capture.borrow_mut().take() else {
        return glib::ControlFlow::Break;
    };
    submit_captured_notes_browser_source(state, capture);
    glib::ControlFlow::Break
}

/// One unit: start the next editor, or capture or count one slice of the
/// current editor's bookmarks.
fn capture_unit(
    capture: &mut OpenEditorSnapshotCapture,
    evidence: &mut super::browser::OpenEditorSnapshotEvidence,
) -> CaptureUnit {
    let Some(current) = capture.current.as_mut() else {
        return start_next_editor(capture);
    };
    let generation = current.editor.bookmark_change_generation();
    if capture_slice_decision(Some(current.generation), generation)
        == CaptureSliceDecision::RestartEditor
    {
        // Drop what this editor gave, return its charge, and start it again.
        capture.budget = RetainedBudget::new(capture.budget.limit());
        let _ = capture.budget.admit(current.charged_before);
        capture.counting_only = current.counting_only_before;
        capture.rows_not_retained = capture.rows_not_retained.saturating_sub(current.counted);
        current.counted = 0;
        current.bookmarks.clear();
        current.next = 0;
        current.generation = generation;
        evidence.restarts = evidence.restarts.saturating_add(1);
        return CaptureUnit::Continue;
    }
    let remaining = current
        .editor
        .bookmark_entry_count()
        .saturating_sub(current.next);
    let rows = capture_slice_rows(remaining);
    let budget = &mut capture.budget;
    let progress = if capture.counting_only {
        current
            .editor
            .bookmark_record_slice(current.next, rows, |_| true, None)
    } else {
        current.editor.bookmark_record_slice(
            current.next,
            rows,
            |charge| budget.admit(charge) == Admission::Retained,
            Some(&mut current.bookmarks),
        )
    };
    current.next = progress.next;
    if capture.counting_only {
        let visited = u64::try_from(progress.visited).unwrap_or(u64::MAX);
        current.counted = current.counted.saturating_add(visited);
        capture.rows_not_retained = capture.rows_not_retained.saturating_add(visited);
    }
    if progress.refused {
        // The byte budget latched: count this editor's rest and every later
        // editor's rows without copying them.
        capture.counting_only = true;
        return CaptureUnit::Continue;
    }
    if progress.done || rows == 0 {
        finish_current_editor(capture);
    }
    CaptureUnit::Continue
}

/// Admit the next editor's path snapshot, or finish when none remain.
fn start_next_editor(capture: &mut OpenEditorSnapshotCapture) -> CaptureUnit {
    loop {
        let Some(weak) = capture.editors.get(capture.next_editor) else {
            return CaptureUnit::Done;
        };
        capture.next_editor = capture.next_editor.saturating_add(1);
        let Some(editor) = weak.upgrade() else {
            continue;
        };
        let Some(path) = editor.file_path() else {
            continue;
        };
        let scope_folders = capture.scope_snapshot.folder_paths();
        let open_tab_source = (!palette_service::path_is_in_folders(&path, scope_folders))
            .then(|| palette_service::open_tab_source_for_path(&capture.all_workspaces, &path));
        let charge = u64::try_from(std::mem::size_of::<PaletteOpenEditorNoteSnapshot>())
            .unwrap_or(u64::MAX)
            .saturating_add(policy::open_editor_snapshot_heap_bytes(
                path.capacity(),
                open_tab_source
                    .as_ref()
                    .and_then(|source| source.workspace_name.as_ref().map(String::capacity)),
                open_tab_source
                    .as_ref()
                    .and_then(|source| source.workspace_folder.as_ref().map(PathBuf::capacity)),
            ));
        if !capture.counting_only && capture.budget.admit(charge) == Admission::Limited {
            capture.counting_only = true;
        }
        let path_retained = !capture.counting_only;
        if !path_retained && open_tab_source.is_none() {
            // In scope and not in the snapshot: the worker reads this file's
            // bookmark sidecar in its place and counts those rows itself, so
            // counting the live rows here too would count them twice.
            continue;
        }
        let charged_before = capture.budget.charged();
        let counting_only_before = capture.counting_only;
        let generation = editor.bookmark_change_generation();
        capture.current = Some(EditorCapture {
            editor,
            path,
            open_tab_source,
            generation,
            next: 0,
            bookmarks: Vec::new(),
            path_retained,
            charged_before,
            counting_only_before,
            counted: 0,
        });
        return CaptureUnit::Continue;
    }
}

/// Keep the finished editor's snapshot if its path was admitted.
fn finish_current_editor(capture: &mut OpenEditorSnapshotCapture) {
    let Some(current) = capture.current.take() else {
        return;
    };
    // An editor whose path snapshot did not fit contributes counted rows only.
    if !current.path_retained {
        return;
    }
    let mut bookmarks = current.bookmarks;
    bookmarks.shrink_to_fit();
    capture.entries.push(PaletteOpenEditorNoteSnapshot {
        path: current.path,
        bookmarks,
        open_tab_source: current.open_tab_source,
    });
}

/// Hand the captured, byte-bounded snapshots to the source coordinator.
fn submit_captured_notes_browser_source(
    state: &Rc<NotesBrowserState>,
    capture: OpenEditorSnapshotCapture,
) {
    debug_assert!(capture.budget.charged() <= NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT);
    let request = NoteSourceRefreshRequest {
        data_dir: json_store::data_dir(),
        scope_snapshot: capture.scope_snapshot,
        open_editor_snapshots: Arc::from(capture.entries),
        open_editor_snapshots_truncated: capture.counting_only || capture.budget.is_limited(),
        open_editor_rows_not_retained: capture.rows_not_retained,
        mode: capture.mode,
        limits: notes_browser_source_limits(),
    };
    let start = state.source_refreshes.borrow_mut().submit(request);
    if let Some(start) = start {
        start_notes_browser_source_load(state, start);
    }
}
