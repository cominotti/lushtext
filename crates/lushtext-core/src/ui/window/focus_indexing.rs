// SPDX-License-Identifier: GPL-3.0-or-later

//! Focus restoration, editor-memory tracking, and command-palette indexing helpers.

use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;

use crate::model::palette::{PaletteFileEntry, SearchMode};
use crate::services::async_task;
use crate::services::palette::FileIndex;
use crate::ui::editor_page::LushtextEditorPage;

use super::{BUFFER_MEMORY_BUDGET, LushtextWindow};

/// Delay between focus retries after tab selection or adaptive layout changes.
/// Thirty milliseconds keeps retries below perceptible interaction latency while
/// giving GTK a frame to settle newly mapped or reparented editor widgets.
const EDITOR_FOCUS_RETRY_INTERVAL: Duration = Duration::from_millis(30);
/// Maximum retry count for editor focus handoffs before giving control back to
/// GTK's normal focus model. Six attempts covers roughly 180ms of settling.
const EDITOR_FOCUS_MAX_ATTEMPTS: u8 = 6;

impl LushtextWindow {
    pub(super) fn track_editor_memory(&self, editor: &LushtextEditorPage) {
        let key = editor.as_ptr() as usize;
        let window_weak = self.downgrade();
        editor.connect_estimated_memory_changed(move |bytes| {
            if let Some(window) = window_weak.upgrade() {
                window.update_editor_memory_estimate(key, bytes);
            }
        });
        self.update_editor_memory_estimate(key, editor.estimated_buffer_bytes());
    }

    fn update_editor_memory_estimate(&self, key: usize, bytes: u64) {
        let imp = self.imp();
        let previous = imp
            .editor_memory
            .by_editor
            .borrow_mut()
            .insert(key, bytes)
            .unwrap_or(0);
        let total = imp
            .editor_memory
            .total
            .get()
            .saturating_sub(previous)
            .saturating_add(bytes);
        imp.editor_memory.total.set(total);
    }

    pub(super) fn untrack_editor_memory(&self, editor: &LushtextEditorPage) {
        let key = editor.as_ptr() as usize;
        if let Some(previous) = self.imp().editor_memory.by_editor.borrow_mut().remove(&key) {
            self.imp().editor_memory.total.set(
                self.imp()
                    .editor_memory
                    .total
                    .get()
                    .saturating_sub(previous),
            );
        }
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

    /// Evict unmodified background tabs when total buffer memory exceeds the budget.
    pub(super) fn maybe_evict_background_tabs(&self) {
        if self.imp().editor_memory.total.get() <= BUFFER_MEMORY_BUDGET {
            return;
        }

        let tab_view = &self.imp().tab_view;
        let selected = tab_view.selected_page();
        let mut total = self.imp().editor_memory.total.get();
        let mut evict_candidates = Vec::new();

        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                if editor.is_evicted() {
                    continue;
                }
                if selected.as_ref() != Some(&page)
                    && !editor.is_modified()
                    && editor.file_path().is_some()
                {
                    evict_candidates.push(page.downgrade());
                }
            }
        }

        for page_weak in evict_candidates {
            let Some(page) = page_weak.upgrade() else {
                continue;
            };
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                let evicted_size = editor.estimated_buffer_bytes();
                tracing::info!("Evicting tab to free memory: {}", editor.title());
                editor.evict();
                total = total.saturating_sub(evicted_size);
                if total <= BUFFER_MEMORY_BUDGET {
                    break;
                }
            }
        }
    }

    /// Build the file index from all workspace folders on a background thread.
    pub fn rebuild_file_index(&self) {
        let generation = self.imp().index_rebuild_generation.get().wrapping_add(1);
        self.imp().index_rebuild_generation.set(generation);

        let window_weak = self.downgrade();
        glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            if window.imp().index_rebuild_generation.get() != generation {
                return;
            }
            let prev_count = window.imp().command_palette.file_index_len();
            let folders = window.current_workspace_folder_paths();
            let window_weak = window.downgrade();
            async_task::spawn_blocking_then(
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
                        if window.imp().index_rebuild_generation.get() != generation {
                            return;
                        }
                        window.imp().command_palette.set_file_index(index);
                    }
                },
            );
        });
    }

    /// Refresh command-palette source metadata owned by the window shell.
    pub(super) fn refresh_command_palette_sources(&self) {
        let open_tabs = self.open_file_palette_entries();
        let workspace_group_label = self.command_palette_workspace_group_label();
        self.imp()
            .command_palette
            .set_sources(open_tabs, workspace_group_label);
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
