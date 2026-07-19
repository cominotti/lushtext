// SPDX-License-Identifier: GPL-3.0-or-later

//! Window-layer focus restoration, editor-memory orchestration, and palette indexing.
//!
//! GTK-owned focus and eviction revalidation stay on the main thread, scalar
//! memory decisions live in `model::editor_memory`, and filesystem indexing
//! crosses to background work through the task adapter.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::gio;
use gtk4::prelude::*;

use crate::model::editor_memory::{
    EDITOR_MEMORY_UPPER_BUDGET_BYTES, EditorMemoryBudgetOutcome, EditorResidency,
    evaluate_editor_memory_budget,
};
use crate::model::palette::{
    PaletteFileEntry, PaletteFileIdentity, PaletteFileIdentityFailure, SearchMode,
};
use crate::services::palette::{
    FileIndex, FileIndexBuildMetrics, FileIndexBuildOutcome, FileIndexBuildRequest,
    FileIndexBuildStart,
};
use crate::ui::accessibility::AnnouncementLane;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::status_bar::MessageKind;

use super::LushtextWindow;

/// Delay between focus retries after tab selection or adaptive layout changes.
/// Thirty milliseconds keeps retries below perceptible interaction latency while
/// giving GTK a frame to settle newly mapped or reparented editor widgets.
const EDITOR_FOCUS_RETRY_INTERVAL: Duration = Duration::from_millis(30);
/// Maximum retry count for editor focus handoffs before giving control back to
/// GTK's normal focus model. Six attempts covers roughly 180ms of settling.
const EDITOR_FOCUS_MAX_ATTEMPTS: u8 = 6;

enum GuardedFileIndexBuildOutcome {
    Complete {
        index: crate::ui::plain_disposal::DisposalOwned<FileIndex>,
        metrics: FileIndexBuildMetrics,
    },
    Cancelled,
}

impl LushtextWindow {
    /// Wire one editor's residency transitions into the window memory policy.
    ///
    /// GTK-main-thread callbacks use weak references so tabs and the window are
    /// not retained. Attaching the page installs one scalar ledger record.
    pub(super) fn track_editor_memory(&self, editor: &LushtextEditorPage) {
        let window_weak = self.downgrade();
        let editor_weak = editor.downgrade();
        editor.connect_memory_policy_changed(move || {
            if let (Some(window), Some(editor)) = (window_weak.upgrade(), editor_weak.upgrade()) {
                window.update_editor_memory_record(&editor);
            }
        });

        let window_weak = self.downgrade();
        let editor_weak = editor.downgrade();
        // A completed load is newly resident, including during out-of-order
        // restore, so it receives a fresh window-wide LRU generation.
        editor.connect_file_loaded(move || {
            if let (Some(window), Some(editor)) = (window_weak.upgrade(), editor_weak.upgrade()) {
                window.mark_editor_memory_accessed(&editor);
            }
        });

        self.update_editor_memory_record(editor);
    }

    /// Remove one detached editor's scalar record without walking remaining tabs.
    pub(super) fn untrack_editor_memory(&self, editor: &LushtextEditorPage) {
        let editor_id = editor.as_ptr() as usize;
        let state = &self.imp().editor_memory;
        let update = state.ledger.borrow_mut().remove(editor_id);
        if state
            .active_editor
            .borrow()
            .as_ref()
            .and_then(glib::WeakRef::upgrade)
            .is_some_and(|active| active.as_ptr() == editor.as_ptr())
        {
            state.active_editor.borrow_mut().take();
        }
        if state.evaluation_running.get() {
            if !state.applying_eviction.get() {
                state.evaluation_armed.set(true);
            }
        } else if update.is_some_and(|update| update.total_bytes > EDITOR_MEMORY_UPPER_BUDGET_BYTES)
            || state.accounting_uncertain.get()
        {
            self.schedule_editor_memory_evaluation();
        } else {
            state
                .last_outcome
                .set(EditorMemoryBudgetOutcome::WithinBudget);
        }
    }

    /// Assign the next window-wide recency generation to one live editor.
    pub(super) fn mark_editor_memory_accessed(&self, editor: &LushtextEditorPage) {
        let state = &self.imp().editor_memory;
        let previous = state
            .active_editor
            .borrow()
            .as_ref()
            .and_then(glib::WeakRef::upgrade);
        if let Some(previous) = previous
            && previous.as_ptr() != editor.as_ptr()
        {
            self.update_editor_memory_record(&previous);
        }
        let active_weak = editor.downgrade();
        state.active_editor.replace(Some(active_weak));
        let generation = state.next_access_generation.get().wrapping_add(1);
        state.next_access_generation.set(generation);
        editor.mark_memory_accessed(generation);
    }

    /// Refresh one current scalar record and enforce only when the aggregate needs it.
    fn update_editor_memory_record(&self, editor: &LushtextEditorPage) {
        let state = &self.imp().editor_memory;
        let editor_id = editor.as_ptr() as usize;
        if !editor.is_ancestor(&*self.imp().tab_view) {
            // A delayed callback from a detached page must not resurrect its
            // record after the trusted detach delta removed it.
            state.ledger.borrow_mut().remove(editor_id);
            return;
        }
        let active = self.is_selected_editor(editor);
        let update = state.ledger.borrow_mut().upsert(EditorResidency {
            editor_id,
            estimated_bytes: editor.estimated_live_buffer_bytes(),
            access_generation: editor.memory_access_generation(),
            policy_generation: editor.memory_policy_generation(),
            eligible_for_eviction: editor.eligible_for_memory_eviction(active),
        });

        if state.evaluation_running.get() {
            if !state.applying_eviction.get() {
                state.evaluation_armed.set(true);
            }
            return;
        }
        if update.total_bytes > EDITOR_MEMORY_UPPER_BUDGET_BYTES || state.accounting_uncertain.get()
        {
            self.schedule_editor_memory_evaluation();
        } else {
            state
                .last_outcome
                .set(EditorMemoryBudgetOutcome::WithinBudget);
        }
    }

    /// Coalesce any number of residency transitions into one next-idle pass.
    fn schedule_editor_memory_evaluation(&self) {
        let state = &self.imp().editor_memory;
        // Remember transitions that happen between bounded eviction turns. A
        // signal emitted synchronously by `evict()` itself is already included
        // in the running pass and does not require a redundant scan.
        if state.evaluation_running.get() {
            if !state.applying_eviction.get() {
                state.evaluation_armed.set(true);
            }
            return;
        }
        if state.evaluation_armed.replace(true) {
            return;
        }

        let window_weak = self.downgrade();
        // Queue on GTK's main loop so a burst of buffer and tab signals becomes
        // one pass. The `_local` callback stays where widget access is safe.
        glib::idle_add_local_once(move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            window.imp().editor_memory.evaluation_armed.set(false);
            window.evaluate_editor_memory_budget();
        });
    }

    pub(super) fn toggle_command_palette(&self) {
        let imp = self.imp();
        if imp.palette_revealer.reveals_child() {
            self.close_command_palette();
        } else {
            let weak = glib::WeakRef::new();
            if let Some(focused) = gtk4::prelude::GtkWindowExt::focus(self) {
                weak.set(Some(&focused));
            }
            imp.saved_focus.replace(Some(weak));

            self.refresh_command_palette_sources();
            imp.palette_revealer.set_reveal_child(true);
            imp.command_palette.open();
            self.set_command_palette_actions_enabled(true);
            self.refresh_command_palette_note_source();
        }
    }

    pub(super) fn close_command_palette(&self) {
        let imp = self.imp();
        imp.command_palette.close();
        imp.palette_revealer.set_reveal_child(false);
        self.set_command_palette_actions_enabled(false);
        self.restore_saved_focus();
    }

    /// Enable actions that require the visible command-palette overlay.
    pub(super) fn set_command_palette_actions_enabled(&self, enabled: bool) {
        for name in ["set-command-palette-query", "set-command-palette-mode"] {
            if let Some(action) = self.lookup_action(name)
                && let Some(simple) = action.downcast_ref::<gio::SimpleAction>()
            {
                simple.set_enabled(enabled);
            }
        }
    }

    /// Set command-palette text through the visible search entry.
    pub(super) fn set_command_palette_query(&self, query: &str) {
        if !self.imp().palette_revealer.reveals_child() {
            return;
        }
        self.imp().command_palette.set_query(query);
    }

    /// Set the command-palette mode using the same stable names as snapshots.
    pub(super) fn set_command_palette_mode(&self, mode_name: &str) {
        if !self.imp().palette_revealer.reveals_child() {
            return;
        }
        let Some(mode) = SearchMode::from_stable_name(mode_name) else {
            tracing::error!(
                "set-command-palette-mode: expected one of all, files, notes, commands"
            );
            return;
        };
        self.imp().command_palette.set_search_mode(mode);
    }

    /// Move keyboard focus to the editor that is selected when an action runs.
    ///
    /// Command-palette activation restores its saved focus after running the
    /// action, so this schedules the editor handoff for a later main-loop tick
    /// and retries briefly while GTK finishes selecting or mapping the tab.
    pub(super) fn focus_selected_editor_after_action(&self) {
        let Some(page) = self.imp().tab_view.selected_page() else {
            return;
        };
        let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>().cloned() else {
            return;
        };

        let window_weak = self.downgrade();
        let page_weak = page.downgrade();
        let editor_weak = editor.downgrade();
        let attempts = std::rc::Rc::new(std::cell::Cell::new(0u8));

        glib::timeout_add_local(EDITOR_FOCUS_RETRY_INTERVAL, move || {
            let Some(window) = window_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let Some(page) = page_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let Some(editor) = editor_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if window.imp().tab_view.selected_page().as_ref() != Some(&page) {
                return glib::ControlFlow::Break;
            }

            let source_view = editor.source_view();
            let source_ptr = source_view.upcast_ref::<gtk4::Widget>().as_ptr();
            gtk4::prelude::GtkWindowExt::set_focus(
                &window,
                Some(source_view.upcast_ref::<gtk4::Widget>()),
            );
            source_view.grab_focus();

            let focused = gtk4::prelude::GtkWindowExt::focus(&window).map(|widget| widget.as_ptr())
                == Some(source_ptr);
            let next_attempt = attempts.get().saturating_add(1);
            attempts.set(next_attempt);

            if focused || next_attempt >= EDITOR_FOCUS_MAX_ATTEMPTS {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    /// Return focus to the active editor after a split-view pane closes.
    pub(super) fn restore_focus_after_secondary_pane_close(&self) {
        let window_weak = self.downgrade();
        glib::idle_add_local_once(move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            if let Some(editor) = window.active_editor() {
                gtk4::prelude::GtkWindowExt::set_focus(
                    &window,
                    Some(editor.source_view().upcast_ref::<gtk4::Widget>()),
                );
                editor.source_view().grab_focus();
            } else {
                gtk4::prelude::GtkWindowExt::set_focus(&window, gtk4::Widget::NONE);
            }
        });
    }

    /// Breakpoint-driven split-view collapse can clear focus more than once as
    /// GTK settles the new adaptive layout, so retry a few short ticks until
    /// the active editor successfully owns focus again.
    pub(super) fn restore_focus_after_breakpoint_collapse(&self) {
        let window_weak = self.downgrade();
        let attempts = std::rc::Rc::new(std::cell::Cell::new(0u8));
        let attempts_clone = attempts;

        glib::timeout_add_local(EDITOR_FOCUS_RETRY_INTERVAL, move || {
            let Some(window) = window_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };

            let Some(editor) = window.active_editor() else {
                gtk4::prelude::GtkWindowExt::set_focus(&window, gtk4::Widget::NONE);
                return glib::ControlFlow::Break;
            };

            let source_view = editor.source_view();
            let source_ptr = source_view.upcast_ref::<gtk4::Widget>().as_ptr();
            gtk4::prelude::GtkWindowExt::set_focus(
                &window,
                Some(source_view.upcast_ref::<gtk4::Widget>()),
            );
            source_view.grab_focus();

            let focused = gtk4::prelude::GtkWindowExt::focus(&window).map(|widget| widget.as_ptr())
                == Some(source_ptr);
            let next_attempt = attempts_clone.get().saturating_add(1);
            attempts_clone.set(next_attempt);

            if focused || next_attempt >= EDITOR_FOCUS_MAX_ATTEMPTS {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    /// Restore focus to the widget saved before an overlay was opened.
    fn restore_saved_focus(&self) {
        let saved = self.imp().saved_focus.take();
        let target = saved.as_ref().and_then(glib::WeakRef::upgrade).or_else(|| {
            self.active_editor()
                .map(|e| e.source_view().clone().upcast::<gtk4::Widget>())
        });

        match target {
            Some(widget) => {
                widget.grab_focus();
            }
            None => {
                gtk4::prelude::GtkWindowExt::set_focus(self, gtk4::Widget::NONE);
            }
        }
    }

    /// If the active tab was evicted, reload its content from disk.
    pub(super) fn reload_if_evicted(&self) {
        let Some(editor) = self.active_editor() else {
            return;
        };
        if !editor.is_evicted() {
            return;
        }
        if let Some(ref path) = editor.file_path() {
            editor.load_file_async(path);
        }
    }

    /// Schedule the same coalesced evaluation used by live editor callbacks.
    pub(super) fn maybe_evict_background_tabs(&self) {
        let memory = &self.imp().editor_memory;
        if memory.ledger.borrow().total_bytes() > EDITOR_MEMORY_UPPER_BUDGET_BYTES
            || memory.accounting_uncertain.get()
        {
            self.schedule_editor_memory_evaluation();
        }
    }

    /// Run one GTK-main-thread aggregate memory-policy pass.
    ///
    /// Scalar facts feed the GTK-free LRU policy, then every candidate is
    /// re-found and revalidated before its buffer is cleared. Protected pages
    /// remain resident, and races that prevent convergence record no progress.
    fn evaluate_editor_memory_budget(&self) {
        let memory = &self.imp().editor_memory;
        if memory.evaluation_running.replace(true) {
            return;
        }
        memory
            .evaluation_count
            .set(memory.evaluation_count.get().wrapping_add(1));
        memory
            .full_scan_count
            .set(memory.full_scan_count.get().wrapping_add(1));

        let tab_view = &self.imp().tab_view;
        let selected = tab_view.selected_page();
        let mut snapshot = Vec::with_capacity(usize::try_from(tab_view.n_pages()).unwrap_or(0));
        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                let editor_id = editor.as_ptr() as usize;
                let active = selected.as_ref() == Some(&page);
                // Every uncertain or non-recoverable state stays protected.
                let eligible_for_eviction = editor.eligible_for_memory_eviction(active);
                snapshot.push(EditorResidency {
                    editor_id,
                    estimated_bytes: editor.estimated_live_buffer_bytes(),
                    access_generation: editor.memory_access_generation(),
                    policy_generation: editor.memory_policy_generation(),
                    eligible_for_eviction,
                });
            }
        }
        memory
            .ledger
            .borrow_mut()
            .reconcile(snapshot.iter().copied());
        memory.accounting_uncertain.set(false);

        let decision = evaluate_editor_memory_budget(&snapshot);
        memory.last_outcome.set(decision.outcome);
        if decision.candidates.is_empty() {
            memory.evaluation_running.set(false);
            return;
        }

        #[cfg(feature = "test-utils")]
        if let Some(hook) = memory.before_eviction_hook.borrow_mut().take() {
            hook();
        }

        // Build widget lookups only for an over-budget decision. The ordinary
        // edit path therefore pays for one scalar tab walk and no hash tables.
        let candidate_ids = decision
            .candidates
            .iter()
            .map(|candidate| candidate.editor_id)
            .collect::<HashSet<_>>();
        let mut pages_by_editor = HashMap::with_capacity(candidate_ids.len());
        for index in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(index);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                let editor_id = editor.as_ptr() as usize;
                if candidate_ids.contains(&editor_id) {
                    pages_by_editor.insert(editor_id, (page.downgrade(), editor.downgrade()));
                }
            }
        }

        let mut candidates = VecDeque::from(decision.candidates);
        let mut actual_projected = decision.total_bytes;
        let window_weak = self.downgrade();
        // Clearing a GtkTextBuffer and its projections is main-thread work. One
        // candidate per idle dispatch gives GTK a chance to serve input and
        // render between large clean-tab evictions.
        glib::idle_add_local(move || {
            let Some(window) = window_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let memory = &window.imp().editor_memory;
            #[cfg(feature = "test-utils")]
            memory
                .eviction_dispatch_count
                .set(memory.eviction_dispatch_count.get().wrapping_add(1));

            // Any transition outside `evict()` invalidates aggregate totals and
            // candidate ordering. Resnapshot instead of applying a stale plan.
            if memory.evaluation_armed.replace(false) {
                memory
                    .last_outcome
                    .set(EditorMemoryBudgetOutcome::NoProgress);
                memory.evaluation_running.set(false);
                window.schedule_editor_memory_evaluation();
                return glib::ControlFlow::Break;
            }

            let Some(candidate) = candidates.pop_front() else {
                memory
                    .last_outcome
                    .set(EditorMemoryBudgetOutcome::NoProgress);
                memory.evaluation_running.set(false);
                return glib::ControlFlow::Break;
            };
            if let Some((page_weak, editor_weak)) = pages_by_editor.get(&candidate.editor_id)
                && let (Some(page), Some(editor)) = (page_weak.upgrade(), editor_weak.upgrade())
            {
                // Widget ancestry proves current membership without scanning
                // every open tab on each bounded continuation callback.
                let still_attached = editor.is_ancestor(&*window.imp().tab_view);
                let same_child =
                    page.child().as_ptr() == editor.upcast_ref::<gtk4::Widget>().as_ptr();
                let still_active = window.imp().tab_view.selected_page().as_ref() == Some(&page);
                let still_current = editor.memory_access_generation()
                    == candidate.access_generation
                    && editor.memory_policy_generation() == candidate.policy_generation;
                let still_reloadable = editor.eligible_for_memory_eviction(still_active);
                if still_attached && same_child && still_current && still_reloadable {
                    tracing::info!("Evicting tab to free memory: {}", editor.title());
                    let before = editor.estimated_live_buffer_bytes();
                    memory.applying_eviction.set(true);
                    editor.evict();
                    memory.applying_eviction.set(false);
                    if editor.buffer_replacement_in_progress() {
                        // A document-sized clear owns later GTK slices. Stop
                        // this pass until its terminal memory notification
                        // resnapshots residency and eligibility.
                        memory
                            .last_outcome
                            .set(EditorMemoryBudgetOutcome::NoProgress);
                        memory.evaluation_running.set(false);
                        return glib::ControlFlow::Break;
                    }
                    #[cfg(feature = "test-utils")]
                    if let Some(hook) = memory.after_eviction_hook.borrow_mut().take() {
                        hook();
                    }
                    let reclaimed = before.saturating_sub(editor.estimated_live_buffer_bytes());
                    actual_projected = actual_projected.saturating_sub(reclaimed);
                }
            }

            if actual_projected <= crate::model::editor_memory::EDITOR_MEMORY_LOWER_WATER_BYTES {
                memory
                    .last_outcome
                    .set(EditorMemoryBudgetOutcome::Converged);
                memory.evaluation_running.set(false);
                if memory.evaluation_armed.replace(false) {
                    window.schedule_editor_memory_evaluation();
                }
                glib::ControlFlow::Break
            } else if candidates.is_empty() {
                memory
                    .last_outcome
                    .set(EditorMemoryBudgetOutcome::NoProgress);
                memory.evaluation_running.set(false);
                if memory.evaluation_armed.replace(false) {
                    window.schedule_editor_memory_evaluation();
                }
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    /// Number of completed aggregate passes for burst-coalescing tests.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn editor_memory_evaluation_count_for_test(&self) -> u64 {
        self.imp().editor_memory.evaluation_count.get()
    }

    /// Number of full tab walks used for enforcement or explicit reconciliation.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn editor_memory_full_scan_count_for_test(&self) -> u64 {
        self.imp().editor_memory.full_scan_count.get()
    }

    /// Current saturating total maintained by constant-work editor deltas.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn editor_memory_incremental_total_for_test(&self) -> u64 {
        self.imp().editor_memory.ledger.borrow().total_bytes()
    }

    /// Compare the incremental ledger with one explicit current GTK snapshot.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn editor_memory_reconciles_for_test(&self) -> bool {
        let tab_view = &self.imp().tab_view;
        let selected = tab_view.selected_page();
        let mut current = Vec::with_capacity(usize::try_from(tab_view.n_pages()).unwrap_or(0));
        for index in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(index);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                current.push(EditorResidency {
                    editor_id: editor.as_ptr() as usize,
                    estimated_bytes: editor.estimated_live_buffer_bytes(),
                    access_generation: editor.memory_access_generation(),
                    policy_generation: editor.memory_policy_generation(),
                    eligible_for_eviction: editor
                        .eligible_for_memory_eviction(selected.as_ref() == Some(&page)),
                });
            }
        }
        current.sort_unstable_by_key(|record| record.editor_id);
        self.imp().editor_memory.ledger.borrow().snapshot() == current
    }

    /// Stable result of the latest pass for protected-budget assertions.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn editor_memory_outcome_for_test(&self) -> EditorMemoryBudgetOutcome {
        self.imp().editor_memory.last_outcome.get()
    }

    /// Number of bounded idle callbacks used to apply eviction candidates.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn editor_memory_eviction_dispatch_count_for_test(&self) -> u64 {
        self.imp().editor_memory.eviction_dispatch_count.get()
    }

    /// Inject one transition between candidate selection and safety rechecks.
    #[cfg(feature = "test-utils")]
    pub fn set_before_editor_memory_eviction_hook_for_test<F: FnOnce() + 'static>(&self, hook: F) {
        self.imp()
            .editor_memory
            .before_eviction_hook
            .replace(Some(Box::new(hook)));
    }

    /// Inject one transition after the first applied candidate and before the next idle turn.
    #[cfg(feature = "test-utils")]
    pub fn set_after_editor_memory_eviction_hook_for_test<F: FnOnce() + 'static>(&self, hook: F) {
        self.imp()
            .editor_memory
            .after_eviction_hook
            .replace(Some(Box::new(hook)));
    }

    /// Build the file index from all workspace folders on a background thread.
    pub fn rebuild_file_index(&self) {
        self.imp().index_rebuild_debounce.schedule(
            self,
            std::time::Duration::from_millis(300),
            move |window, _| {
                let request = FileIndexBuildRequest {
                    workspace_folders: Arc::from(window.current_workspace_folder_paths()),
                    capacity_hint: window.imp().command_palette.file_index_len().max(64),
                };
                let start = window.imp().file_index_builds.borrow_mut().submit(request);
                if let Some(start) = start {
                    window.start_file_index_build(start);
                } else {
                    window.finish_cancelled_file_index_admission();
                }
            },
        );
    }

    fn start_file_index_build(&self, start: FileIndexBuildStart) {
        if start.cancellation.is_cancelled() {
            self.finish_file_index_build(start.generation, GuardedFileIndexBuildOutcome::Cancelled);
            return;
        }
        let observed_epoch = crate::ui::plain_disposal::disposal_capacity_epoch();
        let weight = crate::services::palette::MAX_FILE_INDEX_RETAINED_BYTES;
        let reservation = self
            .imp()
            .command_palette
            .file_index_reservation_weight()
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
            debug_assert!(self.imp().file_index_admission.borrow().is_none());
            self.imp().file_index_admission.replace(Some(start));
            let window_weak = self.downgrade();
            self.imp()
                .file_index_capacity_wakeup
                .arm(observed_epoch, move || {
                    if let Some(window) = window_weak.upgrade() {
                        window.retry_file_index_admission();
                    }
                });
            self.publish_status_message(
                "Workspace file index update deferred by memory pressure",
                MessageKind::Warning,
            );
            return;
        };

        let FileIndexBuildStart {
            generation,
            request,
            cancellation,
        } = start;
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                let outcome = FileIndex::rebuild_cancellable_with_hint(
                    &request.workspace_folders,
                    request.capacity_hint,
                    &cancellation,
                );
                match outcome {
                    FileIndexBuildOutcome::Complete { index, metrics } => {
                        let retained_bytes = index.retained_byte_weight();
                        debug_assert!(
                            retained_bytes
                                <= crate::services::palette::MAX_FILE_INDEX_RETAINED_BYTES
                        );
                        let mut reservation = reservation;
                        reservation.shrink_to(retained_bytes);
                        GuardedFileIndexBuildOutcome::Complete {
                            index: reservation.own(index),
                            metrics,
                        }
                    }
                    FileIndexBuildOutcome::Cancelled { .. } => {
                        GuardedFileIndexBuildOutcome::Cancelled
                    }
                }
            },
            move |(), outcome| {
                let Some(window) = window_weak.upgrade() else {
                    retire_file_index_outcome(outcome);
                    return;
                };
                window.finish_file_index_build(generation, outcome);
            },
        );
    }

    fn retry_file_index_admission(&self) {
        let Some(start) = self.imp().file_index_admission.borrow_mut().take() else {
            return;
        };
        self.start_file_index_build(start);
    }

    fn finish_cancelled_file_index_admission(&self) {
        let cancelled = self
            .imp()
            .file_index_admission
            .borrow()
            .as_ref()
            .is_some_and(|start| start.cancellation.is_cancelled());
        if !cancelled {
            return;
        }
        self.imp().file_index_capacity_wakeup.cancel();
        let start = self.imp().file_index_admission.borrow_mut().take();
        if let Some(start) = start {
            self.finish_file_index_build(start.generation, GuardedFileIndexBuildOutcome::Cancelled);
        }
    }

    fn finish_file_index_build(&self, generation: u64, outcome: GuardedFileIndexBuildOutcome) {
        let (accepted, next) = {
            let mut builds = self.imp().file_index_builds.borrow_mut();
            let accepted = builds.is_current(generation);
            let next = builds.finish(generation);
            (accepted, next)
        };

        if accepted {
            match outcome {
                GuardedFileIndexBuildOutcome::Complete { index, metrics } => {
                    let indexed_files = index.len();
                    self.imp().command_palette.set_guarded_file_index(index);
                    self.announce_workflow_update(
                        AnnouncementLane::ProgressMilestone,
                        "workspace-file-index-updated",
                        &format!("Workspace file index updated with {indexed_files} files"),
                    );
                    if metrics.truncation.is_some() {
                        self.publish_status_message(
                            &format!("Workspace file index limited to {indexed_files} entries"),
                            MessageKind::Warning,
                        );
                    }
                }
                GuardedFileIndexBuildOutcome::Cancelled => {}
            }
        } else {
            retire_file_index_outcome(outcome);
        }

        if let Some(next) = next {
            self.start_file_index_build(next);
        }
    }

    /// Refresh command-palette source metadata owned by the window shell.
    pub(super) fn refresh_command_palette_sources(&self) {
        let open_tabs = self.open_file_palette_entries();
        let workspace_group_label = self.command_palette_workspace_group_label();
        self.imp()
            .command_palette
            .set_sources(open_tabs, workspace_group_label);
        self.refresh_command_palette_note_source();
    }

    /// Snapshot file-backed tabs so the palette can search active documents.
    fn open_file_palette_entries(&self) -> Vec<PaletteFileEntry> {
        let tab_view = &self.imp().tab_view;
        let mut entries =
            Vec::with_capacity(usize::try_from(tab_view.n_pages()).unwrap_or_default());

        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>()
                && let Some(path) = editor.file_path()
            {
                entries.push(PaletteFileEntry::new(
                    editor.title(),
                    path.display().to_string(),
                    path,
                    editor.canonical_file_path().map_or(
                        PaletteFileIdentity::Unavailable(PaletteFileIdentityFailure::NotResolved),
                        PaletteFileIdentity::canonical,
                    ),
                ));
            }
        }

        entries
    }

    /// Name the workspace file group according to the sidebar's current scope.
    fn command_palette_workspace_group_label(&self) -> &'static str {
        if self.current_workspace_scope().is_all() {
            "All Workspaces"
        } else {
            "Selected Workspace"
        }
    }
}

fn retire_file_index_outcome(outcome: GuardedFileIndexBuildOutcome) {
    if let GuardedFileIndexBuildOutcome::Complete { index, .. } = outcome {
        drop(index);
    }
}
