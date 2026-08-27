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
//! 2. **Matching runs on a worker.** Control returns once the worker is
//!    dispatched and resumes in [`finish_notes_browser_query`], which validates
//!    the ticket and either publishes or retires the result, then starts the one
//!    retained latest request.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;

use crate::services::palette as palette_service;
use crate::ui::accessibility;

use super::browser::NotesBrowserState;
use super::browser::append_notes_sidebar_sections;
use super::chrome::empty_browser_label;
use super::policy::{
    NOTES_BROWSER_RENDER_LIMIT, NOTES_BROWSER_SEARCH_DEBOUNCE_MS, NotesBrowserModeExt as _,
    notes_browser_limit_messages,
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
            palette_service::query_notes_browser_source(
                &source,
                &request,
                NOTES_BROWSER_RENDER_LIMIT,
                &cancellation,
            )
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
    outcome: palette_service::PaletteSearchOutcome<palette_service::NotesBrowserQueryResult>,
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
            publish_notes_browser_query(state, &value);
        }
    } else {
        retire_notes_browser_query_result(outcome);
    }
    if let Some(next) = next {
        start_notes_browser_query(state, next);
    }
}

fn retire_notes_browser_query_result(
    outcome: palette_service::PaletteSearchOutcome<palette_service::NotesBrowserQueryResult>,
) {
    let palette_service::PaletteSearchOutcome::Complete { value, .. } = outcome else {
        return;
    };
    // Query ownership is capped at 500 scalar indexes; the document-sized
    // immutable source remains guarded separately.
    drop(value);
}

/// Publish one current background match while preserving grouped selection.
fn publish_notes_browser_query(
    state: &Rc<NotesBrowserState>,
    result: &palette_service::NotesBrowserQueryResult,
) {
    let previously_selected = state.selected_entry_index();
    state.sidebar.remove_all();
    let source = state.all_entries.borrow();
    let empty_message = if source.is_empty() && state.search_entry.text().is_empty() {
        state.mode.get().empty_source_label()
    } else {
        state.mode.get().no_matches_label()
    };
    state
        .sidebar
        .set_placeholder(Some(&empty_browser_label(empty_message)));
    let grouped_indices = append_notes_sidebar_sections(state, &source, &result.matching_indices);
    update_notes_browser_limit_label(state, result.truncated);

    if grouped_indices.is_empty() {
        *state.filtered_indices.borrow_mut() = Vec::new();
        NotesBrowserState::refresh_preview(state, None, false);
        return;
    }
    let selected = previously_selected
        .and_then(|previous| grouped_indices.iter().position(|index| *index == previous))
        .unwrap_or(0);
    *state.filtered_indices.borrow_mut() = grouped_indices;
    state
        .sidebar
        .set_selected(u32::try_from(selected).unwrap_or(0));
    NotesBrowserState::refresh_preview(state, Some(selected), false);
}

pub(super) fn update_notes_browser_limit_label(state: &NotesBrowserState, render_truncated: bool) {
    let source_truncated = !state.source_truncation.borrow().is_empty();
    let messages = notes_browser_limit_messages(
        state.mode.get(),
        source_truncated,
        render_truncated,
        NOTES_BROWSER_RENDER_LIMIT,
    );
    let message = messages.join(" ");
    state.limit_label.set_label(&message);
    accessibility::set_label(&state.limit_label, &message);
    state.limit_label.set_visible(!messages.is_empty());
}
