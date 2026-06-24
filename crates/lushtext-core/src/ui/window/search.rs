// SPDX-License-Identifier: GPL-3.0-or-later

//! Search panel wiring: toggle action, keyboard shortcut, pre-fill,
//! result activation, workspace folder forwarding, and focus management.
//!
//! Extracted from `window/mod.rs` to keep the main window responsibilities
//! split into smaller modules.
//! All methods are `impl LushtextWindow` called from `new()` and `constructed()`.

use crate::config::keys;
use crate::services::notifications::{
    NotificationOwner, NotificationSeverity, NotificationSurface, StatusMessage,
};
use crate::services::{
    content_search, filesystem::metadata as fs_metadata, json_store, saved_searches, search_history,
};
use crate::ui::accessibility::AnnouncementLane;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::search_panel::SearchProgressUpdate;
use crate::ui::status_bar::MessageKind;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;
use gtk4::{self, gio, glib};
use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::LushtextWindow;

/// Delay that lets the in-editor Find revealer finish closing before workspace search opens.
///
/// The 260 ms value tracks the panel transition budget; too short can hand
/// focus over mid-animation, while much longer makes Ctrl+Shift+F feel laggy.
pub(super) const SEARCH_PANEL_TRANSITION_DELAY_MS: u64 = 260;

/// Maximum editor-selection size copied into the workspace search entry.
///
/// Users sometimes hit Ctrl+Shift+F with a whole document selected. The search
/// panel prefill path runs on the GTK main thread, so keep it query-sized and
/// skip very large selections instead of allocating a huge temporary string.
const SEARCH_PANEL_PREFILL_CHAR_LIMIT: i32 = 1024;

fn format_search_progress_message(files_searched: usize) -> String {
    format!("Searching {files_searched} files\u{2026}")
}

fn selection_within_search_prefill_limit(start_offset: i32, end_offset: i32) -> bool {
    start_offset.abs_diff(end_offset) <= SEARCH_PANEL_PREFILL_CHAR_LIMIT as u32
}

/// Set up the search panel action, callbacks, and workspace folder forwarding.
pub fn setup_search_panel(window: &LushtextWindow) {
    let imp = window.imp();

    // --- Workspace folders: forward current folders and future changes ---
    let initial_folders = window.current_workspace_folder_paths();
    imp.search_panel.set_workspace_folders(initial_folders);

    // --- Result activation: open file at line ---
    // GTK signal closures may outlive the current window instance; weak refs
    // let callbacks no-op instead of keeping closed windows alive.
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
        imp.search_panel.connect_search_progress(move |update| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };

            match update {
                SearchProgressUpdate::Started => {
                    window.prepare_search_progress_tracking();
                    window.announce_workflow_update(
                        AnnouncementLane::DebouncedResults,
                        "workspace-search-started",
                        "Workspace search started",
                    );
                }
                SearchProgressUpdate::Cancelled { files_searched } => {
                    let was_visible = window.imp().search_progress.visible.get();
                    window.finish_search_progress_tracking();
                    if was_visible {
                        window.announce_workflow_update(
                            AnnouncementLane::StatusUpdate,
                            "workspace-search-cancelled",
                            &format!("Workspace search cancelled after {files_searched} files"),
                        );
                    }
                    window.update_search_navigation_actions();
                }
                SearchProgressUpdate::Done { files_searched } => {
                    window.finish_search_progress_tracking();
                    let matches = window.imp().search_panel.navigation_match_count();
                    let files = window.imp().search_panel.result_file_count();
                    let message = if matches == 0 {
                        format!("Workspace search complete: no results in {files_searched} files")
                    } else {
                        format!("Workspace search complete: {matches} results in {files} files")
                    };
                    window.announce_workflow_update(
                        AnnouncementLane::StatusUpdate,
                        "workspace-search-complete",
                        &message,
                    );
                    window.update_search_navigation_actions();
                }
                SearchProgressUpdate::Progress { files_searched } => {
                    // Only show progress after the 500ms delay has elapsed
                    // and while the search panel is still visible.
                    if !window.imp().search_progress.visible.get()
                        || !window.imp().search_panel_revealer.reveals_child()
                    {
                        return;
                    }

                    let message = format_search_progress_message(files_searched);
                    window.update_search_progress_message(&message);
                }
            }
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

        // Build skip_paths: files open with unsaved edits or an in-flight save.
        // The replacement service also takes the same per-path advisory lock
        // as editor saves, so a save that starts after this snapshot cannot
        // race the final replacement rename for that file.
        let mut skip_paths = HashSet::new();
        let tab_view = &imp.tab_view;
        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            // Tab pages store generic GTK widgets, so the cast gives access to
            // editor-specific save and path state only for editor tabs.
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>()
                && let Some(path) = editor.file_path()
                && (editor.is_modified() || editor.is_saving())
            {
                skip_paths.insert(path);
            }
        }

        let total_replacements = replacements.len();
        let affected_paths: HashSet<std::path::PathBuf> = replacements
            .iter()
            .filter(|r| !skip_paths.contains(&r.path))
            .map(|r| r.path.clone())
            .collect();

        if affected_paths.is_empty() {
            window.publish_status_message(
                "No replacements to apply (all files have unsaved changes or active saves)",
                MessageKind::Warning,
            );
            return;
        }

        let cancel = AtomicBool::new(false);
        let data_dir = json_store::data_dir();
        spawn_blocking_then(
            window.clone(),
            move || {
                content_search::apply_replacements(
                    &replacements,
                    &skip_paths,
                    &cancel,
                    Some(&data_dir),
                )
            },
            move |window, result| {
                let imp = window.imp();
                match result {
                    Ok(outcome) => {
                        let (replace_result, backup) = outcome.into_parts();
                        let mut msg = format!(
                            "Replaced {} of {} matches in {} files",
                            replace_result.replaced_count,
                            total_replacements,
                            replace_result.files_affected,
                        );
                        if !replace_result.skipped_paths.is_empty() {
                            msg.push_str(&format!(
                                " ({} files skipped)",
                                replace_result.skipped_paths.len()
                            ));
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
                        if matches!(kind, MessageKind::Info) {
                            window.announce_workflow_update(
                                AnnouncementLane::StatusUpdate,
                                "replace-all-complete",
                                &msg,
                            );
                        }

                        if backup.is_empty() {
                            imp.search_panel.clear_undo_backup();
                        } else {
                            imp.search_panel.set_persisted_undo_backup(&backup);
                            imp.search_panel.show_undo_button();
                            window.announce_workflow_update(
                                AnnouncementLane::StatusUpdate,
                                "replace-all-undo-available",
                                "Undo is available for the last Replace All",
                            );
                        }

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

        spawn_blocking_then(
            window,
            move || content_search::undo_replacements(&backup),
            move |window, outcome| {
                let restored_paths: HashSet<std::path::PathBuf> =
                    outcome.restored_paths.iter().cloned().collect();
                if !restored_paths.is_empty() {
                    reload_affected_tabs(&window, &restored_paths);
                }

                if outcome.remaining_backup.is_empty() {
                    let message = format!("Reverted {} files", outcome.restored_count());
                    window.publish_status_message(&message, MessageKind::Info);
                    window.announce_workflow_update(
                        AnnouncementLane::StatusUpdate,
                        "replace-all-undo-complete",
                        &message,
                    );
                    window.imp().search_panel.clear_undo_backup();
                } else {
                    let remaining = outcome.remaining_count();
                    let skipped = outcome.skipped_paths.len();
                    let failed = outcome.failed_paths.len();
                    let message = if outcome.restored_count() > 0 {
                        format!(
                            "Reverted {} files; {remaining} files still need attention",
                            outcome.restored_count()
                        )
                    } else if skipped > 0 && failed == 0 {
                        format!(
                            "Undo skipped {skipped} files changed after Replace All; backup kept"
                        )
                    } else {
                        format!("Undo could not restore {remaining} files; backup kept")
                    };
                    window.publish_status_message(&message, MessageKind::Warning);
                    window
                        .imp()
                        .search_panel
                        .set_undo_backup(&outcome.remaining_backup);
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

    // --- Load search history from disk ---
    let data_dir = json_store::data_dir();
    let data_dir_saved = data_dir.clone();
    spawn_blocking_then(
        window.clone(),
        move || search_history::load_recovering(&data_dir),
        |window, load| {
            for diagnostic in &load.diagnostics {
                tracing::warn!("{}", diagnostic.summary());
            }
            window.imp().search_panel.set_search_history(load.value);
        },
    );

    // --- Load saved searches from disk (parallel to history) ---
    spawn_blocking_then(
        window.clone(),
        move || saved_searches::load_recovering(&data_dir_saved),
        |window, load| {
            for diagnostic in &load.diagnostics {
                tracing::warn!("{}", diagnostic.summary());
            }
            if !load.diagnostics.is_empty() {
                window.publish_status_message(
                    "Saved searches needed recovery; unsupported metadata was preserved",
                    MessageKind::Warning,
                );
            }
            window.imp().search_panel.set_saved_searches(load.value);
        },
    );
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
            if let Some((start, end)) = buffer.selection_bounds()
                && selection_within_search_prefill_limit(start.offset(), end.offset())
            {
                let selected = buffer.text(&start, &end, false);
                if !selected.is_empty() {
                    imp.search_panel.set_query(&selected);
                }
            }
        }

        imp.search_panel_revealer.set_reveal_child(true);
        imp.search_panel.open();
        self.set_search_panel_actions_enabled(true);
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
        self.set_search_panel_actions_enabled(false);
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

    /// Enable actions that require the visible workspace-search panel.
    pub(super) fn set_search_panel_actions_enabled(&self, enabled: bool) {
        for action_name in [
            "set-search-panel-query",
            "set-search-panel-replace-query",
            "preview-search-panel-replacements",
            "confirm-search-panel-replacements",
            "undo-search-panel-replacements",
        ] {
            if let Some(action) = self.lookup_action(action_name)
                && let Some(simple) = action.downcast_ref::<gio::SimpleAction>()
            {
                simple.set_enabled(enabled);
            }
        }
    }

    /// Set workspace-search text through the visible search panel.
    pub(super) fn set_search_panel_query(&self, query: &str) {
        if !self.imp().search_panel_revealer.reveals_child() {
            return;
        }
        self.imp().search_panel.set_query(query);
    }

    /// Set workspace-search replacement text through the visible search panel.
    pub(super) fn set_search_panel_replace_query(&self, text: &str) {
        if !self.imp().search_panel_revealer.reveals_child() {
            return;
        }
        self.imp().search_panel.set_replace_query(text);
    }

    /// Build the Replace All preview through the visible search panel workflow.
    pub(super) fn preview_search_panel_replacements(&self) {
        if !self.imp().search_panel_revealer.reveals_child() {
            return;
        }
        self.imp().search_panel.activate_replace_preview();
    }

    /// Confirm checked Replace All preview rows through the visible panel workflow.
    pub(super) fn confirm_search_panel_replacements(&self) {
        if !self.imp().search_panel_revealer.reveals_child() {
            return;
        }
        self.imp().search_panel.activate_confirm_replacements();
    }

    /// Undo the last Replace All through the visible panel workflow.
    pub(super) fn undo_search_panel_replacements(&self) {
        if !self.imp().search_panel_revealer.reveals_child() {
            return;
        }
        self.imp().search_panel.activate_undo_replacements();
    }

    pub(super) fn after_search_panel_transition<F: FnOnce(&LushtextWindow) + 'static>(
        &self,
        callback: F,
    ) {
        let window_weak = self.downgrade();
        let callback = std::cell::RefCell::new(Some(callback));
        // Schedule on GTK's main loop so focus changes happen after the panel
        // animation frame, without blocking input while the delay elapses.
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

    /// Start delayed status-bar progress tracking for a new workspace search.
    ///
    /// Clears stale progress, arms the 500 ms visibility delay, and starts the
    /// heartbeat timer that keeps active progress notifications alive.
    pub(crate) fn prepare_search_progress_tracking(&self) {
        self.finish_search_progress_tracking();
        let imp = self.imp();
        imp.search_progress.visible.set(false);
        self.start_search_progress_heartbeat();

        imp.search_progress.visibility_timer.arm(
            self,
            Duration::from_millis(500),
            move |window, _| {
                let imp = window.imp();
                if !imp.search_panel.imp().runtime.searching.get()
                    || !imp.search_panel_revealer.reveals_child()
                {
                    return;
                }
                imp.search_progress.visible.set(true);
            },
        );
    }

    /// Publish an informational search-progress update through the notification bus.
    pub(crate) fn update_search_progress_message(&self, message: &str) {
        self.update_search_progress_status_message(message, NotificationSeverity::Info);
    }

    /// Route progress updates through the visible-status pulse gate.
    ///
    /// The expected `StatusMessage` lets rendering pulse only when this progress
    /// update actually occupies the status bar instead of sitting below a transient.
    fn update_search_progress_status_message(&self, message: &str, severity: NotificationSeverity) {
        let status_message = StatusMessage {
            text: message.to_string(),
            severity,
        };
        if self.imp().notification_bus.update_progress(
            NotificationOwner::Search,
            NotificationSurface::StatusBar,
            status_message.text.clone(),
            status_message.severity,
        ) {
            self.render_notifications_for_status_update(&status_message);
        }
    }

    /// Publish a search-progress status message through the production routing path.
    ///
    /// Widget tests use this to exercise visible and hidden progress updates
    /// without starting a real workspace search.
    #[cfg(feature = "test-utils")]
    pub fn update_search_progress_message_for_test(
        &self,
        message: &str,
        severity: NotificationSeverity,
    ) {
        self.update_search_progress_status_message(message, severity);
    }

    pub(crate) fn finish_search_progress_tracking(&self) {
        self.imp().search_progress.visible.set(false);
        let _ = self.imp().search_progress.visibility_timer.invalidate();
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
            if !imp.search_panel.imp().runtime.searching.get() {
                window.finish_search_progress_tracking();
                return glib::ControlFlow::Break;
            }

            if imp.search_panel_revealer.reveals_child()
                && imp.search_progress.visible.get()
                && imp
                    .notification_bus
                    .heartbeat(NotificationOwner::Search, NotificationSurface::StatusBar)
            {
                window.render_notifications();
            }
            glib::ControlFlow::Continue
        });
        self.imp()
            .search_progress
            .heartbeat_source_id
            .replace(Some(source_id));
    }

    fn stop_search_progress_heartbeat(&self) {
        if let Some(source_id) = self.imp().search_progress.heartbeat_source_id.take() {
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
            && !editor.is_saving()
        {
            let editor_weak = editor.downgrade();
            let path_for_facts = path.clone();
            spawn_blocking_then(
                path,
                move || {
                    fs_metadata::file_facts(&path_for_facts)
                        .ok()
                        .and_then(|facts| facts.modified_at_secs)
                },
                move |path, modified_at_secs| {
                    let Some(editor) = editor_weak.upgrade() else {
                        return;
                    };
                    if editor.file_path().as_deref() != Some(path.as_path()) {
                        return;
                    }
                    // Update mtime before reload to suppress the file monitor's
                    // "changed" warning for writes made by Replace All/Undo.
                    editor.imp().monitor.last_known_mtime.set(modified_at_secs);
                    editor.load_file_async(&path);
                },
            );
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

#[cfg(test)]
mod tests {
    use super::{
        SEARCH_PANEL_PREFILL_CHAR_LIMIT, format_search_progress_message,
        selection_within_search_prefill_limit,
    };

    #[test]
    fn search_progress_message_does_not_use_palette_index_total() {
        assert_eq!(
            format_search_progress_message(14_100),
            "Searching 14100 files\u{2026}"
        );
    }

    #[test]
    fn search_panel_prefill_skips_large_selection_ranges() {
        assert!(selection_within_search_prefill_limit(
            0,
            SEARCH_PANEL_PREFILL_CHAR_LIMIT
        ));
        assert!(!selection_within_search_prefill_limit(
            0,
            SEARCH_PANEL_PREFILL_CHAR_LIMIT + 1
        ));
        assert!(selection_within_search_prefill_limit(42, 1));
    }
}
