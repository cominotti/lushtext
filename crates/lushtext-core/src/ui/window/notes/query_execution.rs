// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination role `execution`, stage-order-qualified: the notes-browser query.
//!
//! The second of this workflow's two browser stage orders. It runs over the
//! source `source_execution` published, so it owns a **different** coordinator
//! and therefore a different generation counter — which is why
//! `seams::NotesBrowserTicket` is phantom-typed by flight and a query generation
//! cannot be validated against source facts.
//!
//! # Inversions
//!
//! 1. **Typing is debounced.** A keystroke burst resumes once, after the quiet
//!    window, in the `search_debounce` callback; an emptied query invalidates the
//!    debounce and submits immediately so clearing the field feels instant.
//! 2. **Matching and grouping run on a worker.** Control returns once the worker
//!    is dispatched and resumes in [`finish_notes_browser_query`], which
//!    validates the ticket and either publishes or retires the result, then
//!    starts the one retained latest request. Publishing swaps the lazy row
//!    model's projection in a removal and an addition, whatever the row count.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;

use crate::services::palette as palette_service;
use crate::ui::accessibility;

use super::browser::NotesBrowserState;
use super::policy::{
    GroupedNoteRows, NOTES_BROWSER_SEARCH_DEBOUNCE_MS, group_note_rows, notes_browser_empty_label,
    notes_browser_limit_message, should_announce_limit,
};
use super::seams::{NotesBrowserFacts, NotesBrowserTicket, QueryFlight};

/// Debounce browser search so large note sets do not rebuild on every keystroke.
pub(super) fn schedule_notes_browser_search(state: &Rc<NotesBrowserState>, query: String) {
    if !state.source_ready.get() || state.disposed.get() {
        return;
    }
    if query.is_empty() {
        let _ = state.search_debounce.invalidate();
        submit_notes_browser_query(state, query);
        return;
    }
    let state_weak = Rc::downgrade(state);
    state.search_debounce.schedule(
        &state.search_entry,
        Duration::from_millis(NOTES_BROWSER_SEARCH_DEBOUNCE_MS),
        move |_, _| {
            let Some(state) = state_weak.upgrade() else {
                return;
            };
            submit_notes_browser_query(&state, query);
        },
    );
}

pub(super) fn submit_notes_browser_query(state: &Rc<NotesBrowserState>, query: String) {
    let request = palette_service::NotesBrowserQueryRequest {
        query,
        mode: state.mode.get(),
    };
    let start = state.query_runtime.borrow_mut().submit(request);
    if let Some(start) = start {
        start_notes_browser_query(state, start);
    }
}

fn start_notes_browser_query(
    state: &Rc<NotesBrowserState>,
    start: palette_service::PaletteSearchStart<palette_service::NotesBrowserQueryRequest>,
) {
    let palette_service::PaletteSearchStart {
        generation,
        request,
        cancellation,
    } = start;
    let mode = request.mode;
    let source = Arc::clone(&state.all_entries.borrow());
    let state_weak = Rc::downgrade(state);
    spawn_blocking_then(
        (),
        move || {
            let outcome =
                palette_service::query_notes_browser_source(&source, &request, &cancellation);
            // Grouping is pure policy and runs here, off GTK: GTK receives the
            // rows in visual order with their section bounds.
            outcome.map(|result| {
                group_note_rows(&result.matching_indices, |index| {
                    usize::try_from(index)
                        .ok()
                        .and_then(|index| source.get(index))
                        .map(|entry| entry.category)
                })
            })
        },
        move |(), outcome| {
            let Some(state) = state_weak.upgrade() else {
                retire_notes_browser_query_result(outcome);
                return;
            };
            finish_notes_browser_query(&state, generation, mode, outcome);
        },
    );
}

fn finish_notes_browser_query(
    state: &Rc<NotesBrowserState>,
    generation: u64,
    mode: palette_service::NotesBrowserMode,
    outcome: palette_service::PaletteSearchOutcome<GroupedNoteRows>,
) {
    let ticket = NotesBrowserTicket::<QueryFlight>::new(generation, mode);
    let (accepted, next) = {
        let mut runtime = state.query_runtime.borrow_mut();
        let accepted = ticket.may_publish(&NotesBrowserFacts::new(
            runtime.is_current(ticket.generation()),
            state.mode.get(),
            state.disposed.get(),
        ));
        let next = runtime.finish(ticket.generation());
        (accepted, next)
    };
    if accepted {
        if let palette_service::PaletteSearchOutcome::Complete { value, .. } = outcome {
            publish_notes_browser_query(state, value);
        }
    } else {
        retire_notes_browser_query_result(outcome);
    }
    if let Some(next) = next {
        start_notes_browser_query(state, next);
    }
}

fn retire_notes_browser_query_result(
    outcome: palette_service::PaletteSearchOutcome<GroupedNoteRows>,
) {
    let palette_service::PaletteSearchOutcome::Complete { value, .. } = outcome else {
        return;
    };
    // Query ownership is `u32` indexes, at most one per source row of a
    // byte-bounded source; the document-sized source is guarded separately.
    drop(value);
}

/// Publish one current background match while preserving the selection.
///
/// O(1) in rows on GTK except the selection-restore scan, a linear pass over
/// `u32`s (`bound-notes-browser-by-budget` D2, measured in task 1.1).
fn publish_notes_browser_query(state: &Rc<NotesBrowserState>, grouped: GroupedNoteRows) {
    let previously_selected = state.selected_source_index();
    let empty_message = notes_browser_empty_label(
        state.mode.get(),
        state.all_entries.borrow().is_empty(),
        !state.search_entry.text().is_empty(),
        state.listed_count.get().is_limited(),
    );
    state.publishing.set(true);
    state.rows.publish(grouped, state.source_generation.get());
    let selected = if state.rows.is_empty() {
        None
    } else {
        Some(
            previously_selected
                .and_then(|previous| state.rows.position_of(previous))
                .unwrap_or(0),
        )
    };
    if let Some(position) = selected {
        state.selection.set_selected(position);
    }
    state.publishing.set(false);
    state.show_results(empty_message);
    update_notes_browser_limit_label(state);

    // A row that is still selected keeps its preview: re-rendering it would
    // flash the preview on every keystroke.
    let unchanged =
        previously_selected.is_some() && previously_selected == state.selected_source_index();
    if !unchanged {
        NotesBrowserState::refresh_preview(state, selected, false);
    }
}

/// Project the source's "showing X of Y" onto the limit label, and raise the
/// one polite announcement the first time a limited source generation shows.
pub(super) fn update_notes_browser_limit_label(state: &NotesBrowserState) {
    let count = state.listed_count.get();
    let searching = !state.search_entry.text().trim().is_empty();
    let Some(message) = notes_browser_limit_message(state.mode.get(), count, searching) else {
        state.limit_label.set_visible(false);
        state.limit_label.set_label("");
        accessibility::set_label(&state.limit_label, "Notes result limit");
        return;
    };
    state.limit_label.set_label(&message);
    accessibility::set_label(&state.limit_label, &message);
    state.limit_label.set_visible(true);
    let generation = state.source_generation.get();
    if should_announce_limit(
        state.limit_announced_generation.get(),
        generation,
        count.is_limited(),
    ) {
        state.limit_announced_generation.set(Some(generation));
        state
            .limit_announcements
            .set(state.limit_announcements.get().saturating_add(1));
        // The announcement is the "showing" sentence, not the query form.
        let announcement =
            notes_browser_limit_message(state.mode.get(), count, false).unwrap_or(message);
        accessibility::announce_with_lane(
            &state.limit_label,
            &announcement,
            accessibility::AnnouncementLane::StatusUpdate,
        );
    }
}
