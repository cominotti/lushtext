// SPDX-License-Identifier: GPL-3.0-or-later

//! Search panel wiring: toggle action, keyboard shortcut, pre-fill,
//! result activation, workspace root forwarding, and focus management.
//!
//! Extracted from `window/mod.rs` to keep the main window responsibilities
//! split into smaller modules.
//! All methods are `impl LushtextWindow` called from `new()` and `constructed()`.

use crate::config::keys;
use crate::services::notifications::{
    NotificationOwner, NotificationSeverity, NotificationSurface,
};
use crate::services::{async_task, content_search, json_store, saved_searches, search_history};
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::status_bar::MessageKind;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{self, glib};
use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::LushtextWindow;

pub(super) const SEARCH_PANEL_TRANSITION_DELAY_MS: u64 = 260;

fn format_search_progress_message(files_searched: usize) -> String {
    format!("Searching {files_searched} files\u{2026}")
}

/// Set up the search panel action, callbacks, and workspace root forwarding.
pub fn setup_search_panel(window: &LushtextWindow) {
    let imp = window.imp();

    // --- Workspace roots: forward current roots and future changes ---
    let initial_roots = imp.sidebar.workspace_roots();
    imp.search_panel.set_workspace_roots(initial_roots);

    let window_weak = window.downgrade();
    imp.sidebar.connect_workspace_changed(move || {
        if let Some(window) = window_weak.upgrade() {
            let roots = window.imp().sidebar.workspace_roots();
            window.imp().search_panel.set_workspace_roots(roots);
            // Also rebuild the command palette file index — the sidebar uses a
            // single-slot callback, so both operations must be in the same closure.
            window.rebuild_file_index();
        }
    });

    // --- Result activation: open file at line ---
    let window_weak = window.downgrade();
    imp.search_panel.connect_open_file(move |path, line| {
        if let Some(window) = window_weak.upgrade() {
            open_file_at_line(&window, path, line);
        }
    });

    // --- F4/Shift+F4 navigation: open file at line (same as activation) ---
    let window_weak = window.downgrade();
    imp.search_panel
        .connect_navigate_to_match(move |path, line| {
            if let Some(window) = window_weak.upgrade() {
                open_file_at_line(&window, path, line);
            }
        });

    // --- Search progress: notification store + 500ms delay + navigation action update ---
    let window_weak = window.downgrade();
    imp.search_panel
        .search_entry()
        .connect_search_changed(move |_| {
            if let Some(window) = window_weak.upgrade() {
                window.prepare_search_progress_tracking();
            }
        });

    {
        let window_weak = window.downgrade();
        imp.search_panel
            .connect_search_progress(move |files_searched, is_done| {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };

                if is_done {
                    window.finish_search_progress_tracking();
                    window.update_search_navigation_actions();
                    return;
                }

                // Only show progress after the 500ms delay has elapsed
                // and while the search panel is still visible.
                if !window.imp().search_progress_visible.get()
                    || !window.imp().search_panel_revealer.reveals_child()
                {
                    return;
                }

                let message = format_search_progress_message(files_searched);
                window.update_search_progress_message(&message);
            });
    }

    // --- Close request: hide panel + restore focus ---
    let window_weak = window.downgrade();
    imp.search_panel.connect_close_requested(move || {
        if let Some(window) = window_weak.upgrade() {
            window.close_search_panel();
        }
    });

    // --- Status messages from search panel (e.g., "Search saved as '...'" ) ---
    let window_weak = window.downgrade();
    imp.search_panel.connect_message(move |text| {
        if let Some(window) = window_weak.upgrade() {
            window.publish_status_message(text, MessageKind::Info);
        }
    });

    // --- Replace All: skip modified tabs, execute, status bar message ---
    let window_weak = window.downgrade();
    imp.search_panel.connect_replace_all(move |replacements| {
        let Some(window) = window_weak.upgrade() else {
            return;
        };
        let imp = window.imp();

        // Build skip_paths: files open with unsaved modifications.
        let mut skip_paths = HashSet::new();
        let tab_view = &imp.tab_view;
        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>()
                && let Some(path) = editor.file_path()
                && editor.is_modified()
            {
                skip_paths.insert(path);
            }
        }

        // Count skipped replacements for the status message.
        let total_replacements = replacements.len();
        let filtered: Vec<_> = replacements
            .into_iter()
            .filter(|r| !skip_paths.contains(&r.path))
            .collect();

        let skipped_count = total_replacements - filtered.len();

        if filtered.is_empty() {
            window.publish_status_message(
                "No replacements to apply (all files have unsaved changes)",
                MessageKind::Warning,
            );
            return;
        }

        // Collect paths that will be affected (for tab reload after replace).
        let affected_paths: HashSet<std::path::PathBuf> =
            filtered.iter().map(|r| r.path.clone()).collect();

        let cancel = AtomicBool::new(false);
        async_task::spawn_blocking_then(
            window.clone(),
            move || content_search::apply_replacements(&filtered, &HashSet::new(), &cancel),
            move |window, result| {
                let imp = window.imp();
                match result {
                    Ok((replace_result, backup)) => {
                        let mut msg = format!(
                            "Replaced {} of {} matches in {} files",
                            replace_result.replaced_count,
                            total_replacements,
                            replace_result.files_affected,
                        );
                        if skipped_count > 0 || !replace_result.skipped_paths.is_empty() {
                            let skip_total = skipped_count + replace_result.skipped_paths.len();
                            msg.push_str(&format!(" ({skip_total} files skipped)"));
                        }
                        if !replace_result.errors.is_empty() {
                            msg.push_str(&format!(" ({} errors)", replace_result.errors.len()));
                        }
                        let kind = if replace_result.errors.is_empty() {
                            MessageKind::Info
                        } else {
                            MessageKind::Warning
                        };
                        window.publish_status_message(&msg, kind);

                        // Store backup and show undo button.
                        imp.search_panel.set_undo_backup(backup);
                        imp.search_panel.show_undo_button();

                        // Reload affected open tabs to show updated content.
                        reload_affected_tabs(&window, &affected_paths);
                    }
                    Err(e) => {
                        window.publish_status_message(
                            &format!("Replace failed: {e}"),
                            MessageKind::Error,
                        );
                    }
                }
            },
        );
    });

    // --- Undo All: restore files, status bar message ---
    let window_weak = window.downgrade();
    imp.search_panel.connect_undo_all(move |backup| {
        let Some(window) = window_weak.upgrade() else {
            return;
        };

        let affected_paths: HashSet<std::path::PathBuf> = backup.keys().cloned().collect();

        async_task::spawn_blocking_then(
            window.clone(),
            move || content_search::undo_replacements(&backup),
            move |window, result| match result {
                Ok(count) => {
                    window.publish_status_message(
                        &format!("Reverted {count} files"),
                        MessageKind::Info,
                    );
                    reload_affected_tabs(&window, &affected_paths);
                    window.imp().search_panel.clear_undo_backup();
                }
                Err(e) => {
                    window.publish_status_message(&format!("Undo failed: {e}"), MessageKind::Error);
                    window.imp().search_panel.show_undo_button();
                }
            },
        );
    });

    // --- Restore panel visibility from GSettings ---
    let panel_visible = imp.settings.boolean(keys::SEARCH_PANEL_VISIBLE);
    if panel_visible {
        imp.search_panel_revealer.set_reveal_child(true);
        // Don't grab focus on startup — let session restore handle focus.
    }

    // --- Load search history from disk (AC #7, #8) ---
    let data_dir = json_store::data_dir();
    let data_dir_saved = data_dir.clone();
    async_task::spawn_blocking_then(
        window.clone(),
        move || search_history::load(&data_dir),
        |window, entries| {
            window.imp().search_panel.set_search_history(entries);
        },
    );

    // --- Load saved searches from disk (parallel to history) ---
    async_task::spawn_blocking_then(
        window.clone(),
        move || saved_searches::load(&data_dir_saved),
        |window, entries| {
            window.imp().search_panel.set_saved_searches(entries);
        },
    );
}

#[cfg(test)]
mod tests {
    use super::format_search_progress_message;

    #[test]
    fn search_progress_message_does_not_use_palette_index_total() {
        assert_eq!(
            format_search_progress_message(14_100),
            "Searching 14100 files\u{2026}"
        );
    }
}

impl LushtextWindow {
    /// Toggle the search panel visibility. Handles open, re-invocation, and pre-fill.
    /// If the in-editor Find bar is open, closes it first with animation, then
    /// opens the search panel after the animation completes (260ms delay).
    pub fn toggle_search_panel(&self) {
        let imp = self.imp();
        let revealer = &imp.search_panel_revealer;

        if revealer.reveals_child() {
            self.close_search_panel();
            return;
        }

        // If the in-editor Find bar is visible, close it first with animation.
        if let Some(editor) = self.active_editor()
            && editor.is_search_visible()
        {
            editor.hide_search();
            self.after_search_panel_transition(|window| {
                window.open_search_panel();
            });
            return;
        }

        self.open_search_panel();
    }

    /// Internal helper: open the search panel with pre-fill and focus.
    fn open_search_panel(&self) {
        let imp = self.imp();

        // Save focus before the panel steals it.
        let weak = glib::WeakRef::new();
        if let Some(focused) = gtk4::prelude::GtkWindowExt::focus(self) {
            weak.set(Some(&focused));
        }
        imp.search_saved_focus.replace(Some(weak));

        // Pre-fill: if active editor has selected text, use it.
        if let Some(editor) = self.active_editor() {
            let buffer = editor.buffer();
            if let Some((start, end)) = buffer.selection_bounds() {
                let selected = buffer.text(&start, &end, false);
                if !selected.is_empty() {
                    imp.search_panel.set_query(&selected);
                }
            }
        }

        imp.search_panel_revealer.set_reveal_child(true);
        imp.search_panel.open();
        let _ = imp.settings.set_boolean(keys::SEARCH_PANEL_VISIBLE, true);
        self.update_search_navigation_actions();
    }

    /// Close the search panel and restore focus.
    /// No-ops if the command palette is open — Escape should close the topmost
    /// overlay first (command palette), not the search panel underneath it.
    pub fn close_search_panel(&self) {
        let imp = self.imp();
        if imp.palette_revealer.reveals_child() {
            return; // Command palette is the topmost overlay — let it handle Escape.
        }
        imp.search_panel.close();
        imp.search_panel_revealer.set_reveal_child(false);
        self.finish_search_progress_tracking();
        let _ = imp.settings.set_boolean(keys::SEARCH_PANEL_VISIBLE, false);
        self.update_search_navigation_actions();
        self.restore_search_saved_focus();
    }

    /// Restore focus saved before the search panel was opened.
    fn restore_search_saved_focus(&self) {
        let saved = self.imp().search_saved_focus.take();
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

    pub(super) fn after_search_panel_transition<F: FnOnce(&LushtextWindow) + 'static>(
        &self,
        callback: F,
    ) {
        let window_weak = self.downgrade();
        let callback = std::cell::RefCell::new(Some(callback));
        glib::timeout_add_local_once(
            Duration::from_millis(SEARCH_PANEL_TRANSITION_DELAY_MS),
            move || {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                if let Some(callback) = callback.borrow_mut().take() {
                    callback(&window);
                }
            },
        );
    }

    pub(crate) fn prepare_search_progress_tracking(&self) {
        self.finish_search_progress_tracking();
        let imp = self.imp();
        let generation = imp.search_progress_generation.get().wrapping_add(1);
        imp.search_progress_generation.set(generation);
        imp.search_progress_visible.set(false);
        self.start_search_progress_heartbeat();

        let window_weak = self.downgrade();
        glib::timeout_add_local_once(Duration::from_millis(500), move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let imp = window.imp();
            if imp.search_progress_generation.get() != generation
                || !imp.search_panel.imp().searching.get()
                || !imp.search_panel_revealer.reveals_child()
            {
                return;
            }
            imp.search_progress_visible.set(true);
        });
    }

    pub(crate) fn update_search_progress_message(&self, message: &str) {
        if self.imp().notification_bus.update_progress(
            NotificationOwner::Search,
            NotificationSurface::StatusBar,
            message,
            NotificationSeverity::Info,
        ) {
            self.render_notifications();
        }
    }

    pub(crate) fn finish_search_progress_tracking(&self) {
        self.imp().search_progress_visible.set(false);
        self.stop_search_progress_heartbeat();
        if self
            .imp()
            .notification_bus
            .resolve(NotificationOwner::Search, NotificationSurface::StatusBar)
        {
            self.render_notifications();
        }
    }

    fn start_search_progress_heartbeat(&self) {
        self.stop_search_progress_heartbeat();
        let window_weak = self.downgrade();
        let source_id = glib::timeout_add_local(Duration::from_secs(1), move || {
            let Some(window) = window_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let imp = window.imp();
            if !imp.search_panel.imp().searching.get() {
                window.finish_search_progress_tracking();
                return glib::ControlFlow::Break;
            }

            if imp.search_panel_revealer.reveals_child()
                && imp.search_progress_visible.get()
                && imp
                    .notification_bus
                    .heartbeat(NotificationOwner::Search, NotificationSurface::StatusBar)
            {
                window.render_notifications();
            }
            glib::ControlFlow::Continue
        });
        self.imp()
            .search_progress_heartbeat_source_id
            .replace(Some(source_id));
    }

    fn stop_search_progress_heartbeat(&self) {
        if let Some(source_id) = self.imp().search_progress_heartbeat_source_id.take() {
            source_id.remove();
        }
    }
}

/// Open a file at a specific line number. Shared by result activation (double-click/Enter)
/// and F4/Shift+F4 navigation. Handles evicted tabs, already-loaded buffers, and
/// newly-opened tabs where the async load is still in progress.
fn open_file_at_line(window: &LushtextWindow, path: &std::path::Path, line: u32) {
    window.open_document(path);

    let tab_view = &window.imp().tab_view;
    if let Some(page) = tab_view.selected_page()
        && let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>()
    {
        if editor.is_evicted() {
            let line_0 = line.saturating_sub(1);
            editor.set_restore_position(line_0, 0, line_0.saturating_sub(3));
            window.reload_if_evicted();
        } else if editor.buffer().char_count() > 0 {
            scroll_editor_to_line(editor, line);
        } else {
            let line_0 = line.saturating_sub(1);
            editor.set_restore_position(line_0, 0, line_0.saturating_sub(3));
        }
        // Explicitly move focus to the editor (AC#4: "focus moves to the editor").
        // Without this, GTK4's focus traversal may leave focus on the search panel
        // or sidebar after tab switch.
        editor.source_view().grab_focus();
    }
}

/// Reload open tabs whose file was affected by replace/undo.
/// Updates `last_known_mtime` to suppress the file monitor's "File Has Changed" bar,
/// then reloads the file content via `load_file_async`.
fn reload_affected_tabs(window: &LushtextWindow, affected_paths: &HashSet<std::path::PathBuf>) {
    let tab_view = &window.imp().tab_view;
    for i in 0..tab_view.n_pages() {
        let page = tab_view.nth_page(i);
        if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>()
            && let Some(path) = editor.file_path()
            && affected_paths.contains(&path)
            && !editor.is_modified()
        {
            // Update mtime to suppress file monitor "changed" detection for our own write.
            if let Ok(metadata) = std::fs::metadata(&path) {
                use std::time::SystemTime;
                let mtime = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs());
                editor.imp().last_known_mtime.set(mtime);
            }
            editor.load_file_async(&path);
        }
    }
}

/// Scroll an editor to a specific line number (1-based).
/// If the editor is still loading, this is a best-effort attempt — the content
/// may not be available yet, in which case the cursor ends up at line 0.
fn scroll_editor_to_line(editor: &LushtextEditorPage, line: u32) {
    let buffer = editor.buffer();
    let line_0 = line.saturating_sub(1);
    let line_index = i32::try_from(line_0).unwrap_or(i32::MAX);
    let iter = buffer
        .iter_at_line(line_index)
        .unwrap_or_else(|| buffer.end_iter());
    buffer.place_cursor(&iter);
    let mut scroll_iter = iter;
    editor
        .source_view()
        .scroll_to_iter(&mut scroll_iter, 0.0, true, 0.0, 0.3);
}
