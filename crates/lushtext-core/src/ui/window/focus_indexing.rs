// SPDX-License-Identifier: GPL-3.0-or-later

//! Window-layer focus restoration, editor-memory orchestration, and palette indexing.
//!
//! GTK-owned focus and eviction revalidation stay on the main thread, scalar
//! memory decisions live in `model::editor_memory`, and filesystem indexing
//! crosses to background work through the task adapter.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::gio;
use gtk4::prelude::*;

use crate::model::editor_memory::{
    EditorMemoryBudgetOutcome, EditorResidency, evaluate_editor_memory_budget,
};
use crate::model::palette::{PaletteFileEntry, SearchMode};
use crate::services::palette::FileIndex;
use crate::ui::accessibility::AnnouncementLane;
use crate::ui::editor_page::LushtextEditorPage;

use super::LushtextWindow;

/// Delay between focus retries after tab selection or adaptive layout changes.
/// Thirty milliseconds keeps retries below perceptible interaction latency while
/// giving GTK a frame to settle newly mapped or reparented editor widgets.
const EDITOR_FOCUS_RETRY_INTERVAL: Duration = Duration::from_millis(30);
/// Maximum retry count for editor focus handoffs before giving control back to
/// GTK's normal focus model. Six attempts covers roughly 180ms of settling.
const EDITOR_FOCUS_MAX_ATTEMPTS: u8 = 6;

impl LushtextWindow {
    /// Wire one editor's residency transitions into the window memory policy.
    ///
    /// GTK-main-thread callbacks use weak references so tabs and the window are
    /// not retained, and an initial aggregate evaluation is scheduled.
    pub(super) fn track_editor_memory(&self, editor: &LushtextEditorPage) {
        let window_weak = self.downgrade();
        editor.connect_memory_policy_changed(move || {
            if let Some(window) = window_weak.upgrade() {
                window.schedule_editor_memory_evaluation();
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

        self.schedule_editor_memory_evaluation();
    }

    /// Schedule a fresh aggregate snapshot after an editor leaves the tab view.
    ///
    /// The next GTK idle pass reads only remaining live pages, so there is no
    /// parallel per-editor accounting entry to remove.
    pub(super) fn untrack_editor_memory(&self, _editor: &LushtextEditorPage) {
        self.schedule_editor_memory_evaluation();
    }

    /// Assign the next window-wide recency generation to one live editor.
    pub(super) fn mark_editor_memory_accessed(&self, editor: &LushtextEditorPage) {
        let state = &self.imp().editor_memory;
        let generation = state.next_access_generation.get().wrapping_add(1);
        state.next_access_generation.set(generation);
        editor.mark_memory_accessed(generation);
    }

    /// Coalesce any number of residency transitions into one next-idle pass.
    fn schedule_editor_memory_evaluation(&self) {
        let state = &self.imp().editor_memory;
        // One armed callback absorbs a burst; signals caused by the active
        // eviction pass are ignored so no-progress state cannot spin.
        if state.evaluation_running.get() || state.evaluation_armed.replace(true) {
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
        self.schedule_editor_memory_evaluation();
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

        let tab_view = &self.imp().tab_view;
        let selected = tab_view.selected_page();
        let mut snapshot = Vec::with_capacity(usize::try_from(tab_view.n_pages()).unwrap_or(0));
        let mut pages_by_editor = HashMap::with_capacity(snapshot.capacity());
        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                let editor_id = editor.as_ptr() as usize;
                let active = selected.as_ref() == Some(&page);
                // Every uncertain or non-recoverable state stays protected.
                let eligible_for_eviction = !active
                    && !editor.is_evicted()
                    && !editor.is_modified()
                    && !editor.is_saving()
                    && editor.load_state() == crate::ui::editor_page::EditorLoadState::Loaded
                    && !editor.latest_load_failed()
                    && editor.file_path().is_some();
                snapshot.push(EditorResidency {
                    editor_id,
                    estimated_bytes: editor.estimated_live_buffer_bytes(),
                    access_generation: editor.memory_access_generation(),
                    policy_generation: editor.memory_policy_generation(),
                    eligible_for_eviction,
                });
                pages_by_editor.insert(editor_id, (page.downgrade(), editor.downgrade()));
            }
        }

        let decision = evaluate_editor_memory_budget(&snapshot);
        memory.last_outcome.set(decision.outcome);
        #[cfg(feature = "test-utils")]
        if let Some(hook) = memory.before_eviction_hook.borrow_mut().take() {
            hook();
        }
        // The test race hook can detach pages after planning. Snapshot attached
        // identities once so candidate checks stay O(1); AdwTabView::page_position
        // performs its own linear scan and would make a many-candidate pass O(n²).
        let attached_pages = (0..tab_view.n_pages())
            .map(|index| tab_view.nth_page(index).as_ptr() as usize)
            .collect::<HashSet<_>>();
        let mut applied_bytes = 0u64;
        for candidate in decision.candidates {
            // Weak O(1) lookup keeps application linear after policy sorting;
            // upgrades plus the attached identity set reject later detachments.
            let Some((page_weak, editor_weak)) = pages_by_editor.get(&candidate.editor_id) else {
                continue;
            };
            let (Some(page), Some(editor)) = (page_weak.upgrade(), editor_weak.upgrade()) else {
                continue;
            };
            if !attached_pages.contains(&(page.as_ptr() as usize))
                || page.child().as_ptr() != editor.upcast_ref::<gtk4::Widget>().as_ptr()
            {
                continue;
            }

            let still_active = tab_view.selected_page().as_ref() == Some(&page);
            let still_current = editor.memory_access_generation() == candidate.access_generation
                && editor.memory_policy_generation() == candidate.policy_generation;
            let still_reloadable = !still_active
                && !editor.is_evicted()
                && !editor.is_modified()
                && !editor.is_saving()
                && editor.load_state() == crate::ui::editor_page::EditorLoadState::Loaded
                && !editor.latest_load_failed()
                && editor.file_path().is_some();
            if still_current && still_reloadable {
                tracing::info!("Evicting tab to free memory: {}", editor.title());
                editor.evict();
                applied_bytes = applied_bytes.saturating_add(candidate.reclaimable_bytes);
            }
        }

        if decision.outcome != EditorMemoryBudgetOutcome::WithinBudget {
            // Candidates can fail freshness checks, so convergence is based on
            // bytes actually evicted rather than the immutable plan.
            let actual_projected = decision.total_bytes.saturating_sub(applied_bytes);
            memory.last_outcome.set(
                if actual_projected <= crate::model::editor_memory::EDITOR_MEMORY_LOWER_WATER_BYTES
                {
                    EditorMemoryBudgetOutcome::Converged
                } else {
                    EditorMemoryBudgetOutcome::NoProgress
                },
            );
        }
        memory.evaluation_running.set(false);
    }

    /// Number of completed aggregate passes for burst-coalescing tests.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn editor_memory_evaluation_count_for_test(&self) -> u64 {
        self.imp().editor_memory.evaluation_count.get()
    }

    /// Stable result of the latest pass for protected-budget assertions.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn editor_memory_outcome_for_test(&self) -> EditorMemoryBudgetOutcome {
        self.imp().editor_memory.last_outcome.get()
    }

    /// Inject one transition between candidate selection and safety rechecks.
    #[cfg(feature = "test-utils")]
    pub fn set_before_editor_memory_eviction_hook_for_test<F: FnOnce() + 'static>(&self, hook: F) {
        self.imp()
            .editor_memory
            .before_eviction_hook
            .replace(Some(Box::new(hook)));
    }

    /// Build the file index from all workspace folders on a background thread.
    pub fn rebuild_file_index(&self) {
        self.imp().index_rebuild_debounce.schedule(
            self,
            std::time::Duration::from_millis(300),
            move |window, token| {
                let prev_count = window.imp().command_palette.file_index_len();
                let folders = window.current_workspace_folder_paths();
                let window_weak = window.downgrade();
                spawn_blocking_then(
                    (),
                    move || {
                        if prev_count == 0 {
                            FileIndex::rebuild(&folders)
                        } else {
                            FileIndex::rebuild_with_hint(&folders, prev_count)
                        }
                    },
                    move |(), index| {
                        if let Some(window) = window_weak.upgrade() {
                            if !window.imp().index_rebuild_debounce.is_current(token) {
                                return;
                            }
                            let indexed_files = index.len();
                            window.imp().command_palette.set_file_index(index);
                            window.announce_workflow_update(
                                AnnouncementLane::ProgressMilestone,
                                "workspace-file-index-updated",
                                &format!("Workspace file index updated with {indexed_files} files"),
                            );
                        }
                    },
                );
            },
        );
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
