// SPDX-License-Identifier: GPL-3.0-or-later

//! Adoption-lab page for GTK Lush background tasks and freshness tokens.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gtk_lush_tasks::{FreshnessToken, active_worker_count, spawn_blocking_then};
use gtk4::prelude::*;

use crate::shared_ui::{append_body, append_control_row, scroll_page, status_label, workflow_box};

#[derive(Clone)]
/// Keeps task counters and pending state alive for the background-work demo.
pub(crate) struct TaskOwners {
    generation: Rc<Cell<u64>>,
    fresh_results: Rc<Cell<u32>>,
    stale_results: Rc<Cell<u32>>,
    pending_work: Rc<Cell<bool>>,
}

impl TaskOwners {
    /// Return the current task generation while touching counters for lifecycle coverage.
    pub(crate) fn generation(&self) -> u64 {
        let _fresh = self.fresh_results.get();
        let _stale = self.stale_results.get();
        let _pending = self.pending_work.get();
        self.generation.get()
    }
}

/// Build the page that demonstrates bounded workers and freshness-token rejection.
pub(crate) fn build_tasks_page() -> (gtk4::Widget, TaskOwners) {
    let content = workflow_box("Bounded Background Work And Freshness");
    append_body(
        &content,
        "Start a worker, supersede its generation before completion, and run a \
         panic-safe job whose error is reported on the GTK main loop.",
    );

    let generation = Rc::new(Cell::new(0u64));
    let fresh_results = Rc::new(Cell::new(0u32));
    let stale_results = Rc::new(Cell::new(0u32));
    let pending_work = Rc::new(Cell::new(false));
    let task_status = status_label("No background work has completed.");
    let counter_status = status_label("fresh=0 stale=0 active-workers=0");

    let start_button = gtk4::Button::with_label("Start Worker");
    start_button.connect_clicked({
        let generation = Rc::clone(&generation);
        let fresh_results = Rc::clone(&fresh_results);
        let stale_results = Rc::clone(&stale_results);
        let pending_work = Rc::clone(&pending_work);
        let task_status = task_status.clone();
        let counter_status = counter_status.clone();
        move |_| {
            if pending_work.get() {
                task_status.set_text("Worker already pending; request collapsed.");
                return;
            }
            let next_generation = generation.get().saturating_add(1);
            generation.set(next_generation);
            pending_work.set(true);
            let token = FreshnessToken::new(next_generation);
            task_status.set_text(&format!(
                "Worker requested for generation {next_generation}."
            ));
            let state = TaskCompletionState {
                generation: Rc::clone(&generation),
                fresh_results: Rc::clone(&fresh_results),
                stale_results: Rc::clone(&stale_results),
                pending_work: Rc::clone(&pending_work),
                task_status: task_status.clone(),
                counter_status: counter_status.clone(),
            };
            spawn_blocking_then(
                token,
                move || {
                    std::thread::sleep(Duration::from_millis(90));
                    Ok(format!("loaded payload for generation {next_generation}"))
                },
                move |token, result| apply_task_result(&state, token, result),
            );
        }
    });

    let supersede_button = gtk4::Button::with_label("Supersede Generation");
    supersede_button.connect_clicked({
        let generation = Rc::clone(&generation);
        let task_status = task_status.clone();
        move |_| {
            let next_generation = generation.get().saturating_add(1);
            generation.set(next_generation);
            task_status.set_text(&format!("Generation advanced to {next_generation}."));
        }
    });

    let panic_button = gtk4::Button::with_label("Panic-Safe Worker");
    panic_button.connect_clicked({
        let generation = Rc::clone(&generation);
        let fresh_results = Rc::clone(&fresh_results);
        let stale_results = Rc::clone(&stale_results);
        let pending_work = Rc::clone(&pending_work);
        let task_status = task_status.clone();
        let counter_status = counter_status.clone();
        move |_| {
            if pending_work.get() {
                task_status.set_text("Worker already pending; panic-safe request collapsed.");
                return;
            }
            let next_generation = generation.get().saturating_add(1);
            generation.set(next_generation);
            pending_work.set(true);
            let token = FreshnessToken::new(next_generation);
            let state = TaskCompletionState {
                generation: Rc::clone(&generation),
                fresh_results: Rc::clone(&fresh_results),
                stale_results: Rc::clone(&stale_results),
                pending_work: Rc::clone(&pending_work),
                task_status: task_status.clone(),
                counter_status: counter_status.clone(),
            };
            spawn_blocking_then(
                token,
                move || {
                    let result = std::panic::catch_unwind(|| {
                        panic!("simulated adoption-lab worker panic");
                    });
                    match result {
                        Ok(()) => Ok("unexpected clean worker".to_string()),
                        Err(_) => Err("caught simulated worker panic".to_string()),
                    }
                },
                move |token, result| apply_task_result(&state, token, result),
            );
        }
    });

    let command_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    command_row.append(&start_button);
    command_row.append(&supersede_button);
    command_row.append(&panic_button);
    append_control_row(&content, "gtk-lush-tasks", &command_row, &task_status);
    content.append(&counter_status);

    (
        scroll_page(&content),
        TaskOwners {
            generation,
            fresh_results,
            stale_results,
            pending_work,
        },
    )
}

struct TaskCompletionState {
    generation: Rc<Cell<u64>>,
    fresh_results: Rc<Cell<u32>>,
    stale_results: Rc<Cell<u32>>,
    pending_work: Rc<Cell<bool>>,
    task_status: gtk4::Label,
    counter_status: gtk4::Label,
}

fn apply_task_result(
    state: &TaskCompletionState,
    token: FreshnessToken,
    result: Result<String, String>,
) {
    let current = FreshnessToken::new(state.generation.get());
    state.pending_work.set(false);
    match token.accept(current, result) {
        Ok(fresh) => {
            let accepted = fresh.into_inner();
            state
                .fresh_results
                .set(state.fresh_results.get().saturating_add(1));
            match accepted {
                Ok(text) => state.task_status.set_text(&format!("Fresh result: {text}")),
                Err(error) => state
                    .task_status
                    .set_text(&format!("Fresh worker error: {error}")),
            }
        }
        Err(stale) => {
            state
                .stale_results
                .set(state.stale_results.get().saturating_add(1));
            state.task_status.set_text(&format!(
                "Stale result rejected: requested {} current {}",
                stale.requested().generation(),
                stale.current().generation()
            ));
        }
    }
    let fresh = state.fresh_results.get();
    let stale = state.stale_results.get();
    let active = active_worker_count();
    state.counter_status.set_text(&format!(
        "fresh={fresh} stale={stale} active-workers={active}"
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_freshness_rejects_stale_result() {
        let requested = FreshnessToken::new(4);
        let current = FreshnessToken::new(5);
        let result = requested.accept(current, "payload");

        assert!(result.is_err());
    }
}
