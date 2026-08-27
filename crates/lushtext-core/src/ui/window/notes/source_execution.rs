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

use crate::model::palette::{PaletteNoteEntry, PaletteOpenEditorNoteSnapshot};
use crate::model::workspace::WorkspaceConfig;
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
    NOTES_BROWSER_OPEN_EDITOR_SNAPSHOT_LIMIT, NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT,
    NotesBrowserModeExt as _,
};
use super::query_execution::submit_notes_browser_query;
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
        }
    }
    pub(super) fn submit_notes_browser_source(
        &self,
        state: &Rc<NotesBrowserState>,
        mode: palette_service::NotesBrowserMode,
    ) {
        let workspaces_file = self.imp().sidebar.workspaces_file();
        let scope_snapshot = workspaces_file.current_scope_snapshot();
        let all_workspaces = workspaces_file.workspaces;
        let open_editor_snapshots = self.open_editor_note_snapshots_bounded(
            scope_snapshot.folder_paths(),
            &all_workspaces,
            NOTES_BROWSER_OPEN_EDITOR_SNAPSHOT_LIMIT,
            NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT,
        );
        debug_assert!(
            open_editor_snapshots.retained_bytes <= NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT
        );
        state.limit_label.set_label(mode.loading_label());
        state.limit_label.set_visible(true);
        let request = NoteSourceRefreshRequest {
            data_dir: json_store::data_dir(),
            scope_snapshot,
            open_editor_snapshots: Arc::from(open_editor_snapshots.entries),
            open_editor_snapshots_truncated: open_editor_snapshots.truncated,
            mode,
            limits: notes_browser_source_limits(),
        };
        let start = state.source_refreshes.borrow_mut().submit(request);
        if let Some(start) = start {
            start_notes_browser_source_load(state, start);
        }
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
                let previous = state
                    .all_entries
                    .replace(Arc::new(entries.into_retained_current()));
                drop(previous);
                *state.source_truncation.borrow_mut() = source_truncation;
                state.source_ready.set(true);
                if !state.source_truncation.borrow().is_empty() {
                    state.window.publish_status_message(
                        ticket.mode().source_limit_status_message(),
                        MessageKind::Warning,
                    );
                } else if had_recovery_diagnostics {
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
        self.sidebar.remove_all();
        self.filtered_indices.borrow_mut().clear();
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
        self.filtered_indices.borrow_mut().clear();
        let source = self.all_entries.replace(Arc::new(
            crate::ui::plain_disposal::DisposalOwned::small_unreserved(
                Vec::<NotesBrowserEntry>::new().into_boxed_slice(),
            ),
        ));
        drop(source);
    }
}
