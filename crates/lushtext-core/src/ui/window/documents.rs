// SPDX-License-Identifier: GPL-3.0-or-later

//! Document lifecycle and window chrome helpers for the main window.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
#[cfg(feature = "test-utils")]
use std::sync::atomic::{AtomicU64, Ordering};

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::gio;
use gtk4::prelude::*;

use crate::config::keys;
use crate::services::editorconfig;
use crate::services::filesystem::metadata as fs_metadata;
use crate::services::notifications::InlineActionNotification;
use crate::ui::accessibility::AnnouncementLane;
use crate::ui::editor_page::{EditorLoadState, LushtextEditorPage};
use crate::ui::sidebar::SidebarFileRowStateSnapshot;
use crate::ui::status_bar::MessageKind;

use super::LushtextWindow;

#[cfg(feature = "test-utils")]
static CANONICAL_REFRESH_DELAY_MS: AtomicU64 = AtomicU64::new(0);

/// Configure an artificial canonical-refresh delay for window identity tests.
#[cfg(feature = "test-utils")]
pub fn set_canonical_refresh_delay_for_test(delay_ms: u64) {
    CANONICAL_REFRESH_DELAY_MS.store(delay_ms, Ordering::Release);
}

/// Return the key used for duplicate-tab bookkeeping.
///
/// The GTK thread uses the path spelling it already has; canonical filesystem
/// identity is reconciled later from the background load result.
pub(super) fn open_path_key(path: &Path) -> PathBuf {
    path.to_path_buf()
}

/// Source of a document-open request.
#[derive(Clone, Copy, Eq, PartialEq)]
enum OpenDocumentIntent {
    /// In-app navigation such as sidebar rows, command palette results, and
    /// recent-document activation.
    InApp,
    /// Desktop, CLI, or file-manager activation explicitly requesting a file.
    ExternalActivation,
    /// Startup session restore should reopen tabs without reshuffling recents.
    SessionRestore,
}

impl OpenDocumentIntent {
    fn records_recent(self) -> bool {
        !matches!(self, Self::SessionRestore)
    }

    fn bypasses_failed_placeholder(self) -> bool {
        matches!(self, Self::ExternalActivation)
    }
}

impl LushtextWindow {
    /// Speak a bounded workflow milestone through the shared status-bar target.
    pub(super) fn announce_workflow_update(
        &self,
        lane: AnnouncementLane,
        key: &str,
        message: &str,
    ) -> bool {
        self.imp()
            .status_bar
            .announce_workflow_update(lane, key, message)
    }

    /// Apply sidebar rename effects across tabs, sidecars, search indexes, and status.
    pub(super) fn handle_sidebar_file_renamed(&self, old_path: &Path, new_path: &Path) {
        self.update_tab_path(old_path, new_path);
        self.migrate_note_sidecars_after_rename(old_path, new_path);
        self.migrate_local_history_after_rename(old_path, new_path);
        self.imp()
            .command_palette
            .update_index_file_renamed(old_path, new_path);
        let name = new_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.publish_status_message(&format!("Renamed to {name}"), MessageKind::Info);
    }

    /// Apply sidebar deletion effects across open tabs, palette index, and status.
    pub(super) fn handle_sidebar_file_deleted(&self, path: &Path) {
        self.close_tab_for_path(path);
        self.imp().command_palette.update_index_file_deleted(path);
        self.publish_status_message("Deleted", MessageKind::Info);
    }

    /// Apply sidebar creation effects by opening the new file and indexing it.
    pub(super) fn handle_sidebar_file_created(&self, path: &Path) {
        self.open_document(path);
        self.imp().command_palette.update_index_file_created(path);
    }

    /// Report an activation input that GTK could not expose as a local path.
    pub fn report_unsupported_open_file(&self, file: &gio::File) {
        let uri = file.uri();
        self.publish_status_message(
            &format!("Could not open {uri}: only local files are supported"),
            MessageKind::Error,
        );
        self.refresh_status_bar();
    }

    /// Open a file in a new tab, or focus the existing tab if already open.
    ///
    /// This remains the single authority for real document opening, so sidebar
    /// double-click, `Enter`, Save As handoff, and file-peek promotion all
    /// reuse the same duplicate-tab and editor-focus behavior.
    pub fn open_document(&self, path: &Path) {
        self.open_document_with_intent(path, OpenDocumentIntent::InApp);
    }

    /// Open a file requested by desktop, CLI, or file-manager activation.
    ///
    /// Unlike ordinary in-app opens, explicit activation bypasses failed
    /// placeholders for the same path so the requested file can load in a fresh
    /// selected tab while any recoverable failed buffer remains visible.
    pub fn open_document_from_activation(&self, path: &Path) {
        if self.queue_activation_open_if_startup_pending(path) {
            return;
        }
        self.open_document_with_intent(path, OpenDocumentIntent::ExternalActivation);
    }

    /// Open a file while restoring startup session state without updating recent history.
    pub(super) fn open_document_from_session_restore(&self, path: &Path) {
        self.open_document_with_intent(path, OpenDocumentIntent::SessionRestore);
    }

    fn open_document_with_intent(&self, path: &Path, intent: OpenDocumentIntent) {
        let tab_view = &self.imp().tab_view;
        let key = open_path_key(path);
        if let Some(page) = self.find_open_document_page(&key, intent) {
            tab_view.set_selected_page(&page);
            if intent.records_recent()
                && let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>()
            {
                self.record_recent_open_for_editor(editor, path);
            }
            return;
        }

        self.imp().open_paths.borrow_mut().insert(key);
        let editor_page = LushtextEditorPage::new();
        self.wire_info_bar(&editor_page);
        self.wire_note_callbacks(&editor_page);
        editor_page.set_file_path_for_pending_load(path);
        self.resolve_editorconfig_for_editor(&editor_page, path);
        self.assign_draft_id(&editor_page);

        let page = tab_view.append(&editor_page);
        page.set_title(&editor_page.title());
        self.wire_modified_indicator(&page, &editor_page);
        self.configure_tab_page(&page);
        self.track_editor_memory(&editor_page);

        let window_weak = self.downgrade();
        let path_for_draft = path.to_path_buf();
        let path_for_recent = path.to_path_buf();
        let record_recent = intent.records_recent();
        let editor_weak = editor_page.downgrade();
        *editor_page.imp().load.load_completed_callback.borrow_mut() = Some(Box::new(move || {
            if let Some(window) = window_weak.upgrade()
                && let Some(editor) = editor_weak.upgrade()
            {
                if record_recent {
                    window.record_recent_open_for_editor(&editor, &path_for_recent);
                }
                if window.close_loaded_canonical_duplicate(&editor) {
                    window.refresh_sidebar_file_row_states();
                    window.refresh_open_popover_rows();
                    return;
                }
                editor.start_file_monitor();
                window.check_draft_on_open(&editor, &path_for_draft);
                window.refresh_sidebar_file_row_states();
                window.refresh_open_popover_rows();
                window.refresh_status_bar();
                if record_recent {
                    let title = editor.title();
                    window.announce_workflow_update(
                        AnnouncementLane::StatusUpdate,
                        &format!("document-load:{title}"),
                        &format!("Loaded {title}"),
                    );
                }
            }
        }));

        let window_weak = self.downgrade();
        let editor_weak = editor_page.downgrade();
        let page_weak = page.downgrade();
        let path_for_failure = path.to_path_buf();
        *editor_page.imp().load.load_failed_callback.borrow_mut() = Some(Box::new(move |error| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            window.publish_status_message(
                &format!("Could not open {}: {error}", path_for_failure.display()),
                MessageKind::Error,
            );
            let Some(editor) = editor_weak.upgrade() else {
                window.reconcile_open_paths_from_tabs();
                window.refresh_sidebar_file_row_states();
                window.refresh_open_popover_rows();
                return;
            };
            let first_open_failed = editor.load_state() == EditorLoadState::Failed;
            if !first_open_failed {
                window.refresh_header_bar();
                window.refresh_status_bar();
                window.refresh_sidebar_file_row_states();
                window.refresh_open_popover_rows();
                return;
            }
            window.reconcile_open_paths_from_tabs();
            window.apply_preloaded_draft_for_path(&editor, &path_for_failure);
            // A failed load can arrive after the user typed into the tab; keep
            // that buffer instead of demoting the page and rewriting its draft identity.
            if editor.is_modified() {
                window.refresh_header_bar();
                window.refresh_status_bar();
                window.refresh_sidebar_file_row_states();
                window.refresh_open_popover_rows();
                return;
            }
            editor.clear_file_path_after_failed_load();
            window.assign_draft_id(&editor);
            if let Some(page) = page_weak.upgrade() {
                page.set_title(&editor.title());
            }
            window.refresh_sidebar_file_row_states();
            window.refresh_open_popover_rows();
            window.refresh_header_bar();
            window.refresh_status_bar();
        }));

        tab_view.set_selected_page(&page);
        self.update_content_stack();
        self.refresh_command_palette_sources();
        self.refresh_status_bar();
        editor_page.load_file_async(path);
    }

    fn find_open_document_page(
        &self,
        key: &Path,
        intent: OpenDocumentIntent,
    ) -> Option<libadwaita::TabPage> {
        if !self.imp().open_paths.borrow().contains(key) {
            return None;
        }

        let tab_view = &self.imp().tab_view;
        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            let child = page.child();
            let Some(editor) = child.downcast_ref::<LushtextEditorPage>() else {
                continue;
            };
            let matches_key = editor
                .file_path()
                .is_some_and(|editor_path| open_path_key(&editor_path) == key);
            if !matches_key {
                continue;
            }
            if intent.bypasses_failed_placeholder()
                && editor.load_state() == EditorLoadState::Failed
            {
                continue;
            }
            return Some(page);
        }
        None
    }

    /// Close a just-loaded tab when another open tab already owns the same canonical file.
    ///
    /// Canonical path resolution happens in the background load service. This
    /// keeps desktop activation responsive on slow filesystems while preserving
    /// the "do not keep two tabs for the same real file" contract after load.
    fn close_loaded_canonical_duplicate(&self, editor: &LushtextEditorPage) -> bool {
        let Some(canonical_path) = editor.canonical_file_path() else {
            return false;
        };

        let tab_view = &self.imp().tab_view;
        let mut current_page = None;
        let mut existing_page = None;

        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            let child = page.child();
            let Some(candidate) = child.downcast_ref::<LushtextEditorPage>() else {
                continue;
            };
            if candidate.as_ptr() == editor.as_ptr() {
                current_page = Some(page);
                continue;
            }
            if candidate
                .canonical_file_path()
                .is_some_and(|candidate_path| candidate_path == canonical_path)
            {
                existing_page = Some(page);
            }
        }

        let (Some(current_page), Some(existing_page)) = (current_page, existing_page) else {
            self.imp().open_paths.borrow_mut().insert(canonical_path);
            self.refresh_sidebar_file_row_states();
            self.refresh_open_popover_rows();
            return false;
        };

        self.imp().open_paths.borrow_mut().insert(canonical_path);
        editor.imp().canonical_file_path.borrow_mut().take();
        tab_view.set_selected_page(&existing_page);
        tab_view.close_page(&current_page);
        self.refresh_sidebar_file_row_states();
        self.refresh_open_popover_rows();
        true
    }

    /// Save the active tab's file. If untitled, shows Save As dialog.
    pub(super) fn save_current(&self) {
        let Some(editor) = self.active_editor() else {
            return;
        };
        if editor.file_path().is_none() {
            self.show_save_as_dialog();
            return;
        }
        let window = self.clone();
        let editor_for_retry = editor.clone();
        let save_path = editor.file_path();
        editor.save_file_async(move |result| match result {
            Ok(()) => {
                if let Some(ref path) = save_path {
                    window.delete_draft_for_path(path);
                    let mut open_paths = window.imp().open_paths.borrow_mut();
                    open_paths.insert(open_path_key(path));
                    if let Some(canonical_path) = editor_for_retry.canonical_file_path() {
                        open_paths.insert(canonical_path);
                    }
                }
                window.reconcile_open_paths_from_tabs();
                window.refresh_sidebar_file_row_states();
                window.refresh_open_popover_rows();
                if let Some(editor) = window.active_editor() {
                    editor.set_draft_restored(false);
                    window.dismiss_editor_notifications(&editor);
                }
                window.publish_status_message("File saved", MessageKind::Info);
                window.announce_workflow_update(
                    AnnouncementLane::StatusUpdate,
                    "document-save",
                    "File saved",
                );
                window.refresh_status_bar();
            }
            Err(crate::ui::editor_page::EditorSaveError::LossyEncoding { preview, .. }) => {
                let window_for_retry = window.clone();
                window.confirm_lossy_save(&editor_for_retry, &preview, move || {
                    window_for_retry.save_current();
                });
            }
            Err(crate::ui::editor_page::EditorSaveError::DurabilityUnconfirmed {
                path,
                source,
            }) => {
                // The bytes are already at the destination, but the directory
                // fsync that proves the rename durable failed. Keep the document
                // modified (the Err path does this) and tell the user the change
                // is on disk yet unconfirmed, rather than implying it was lost.
                tracing::warn!(
                    "Saved {}, but durability sync failed: {source}",
                    path.display()
                );
                window.publish_status_message(
                    "Saved, but the change is not yet confirmed on disk — save again to retry",
                    MessageKind::Warning,
                );
                window.refresh_status_bar();
            }
            Err(e) => {
                tracing::error!("Failed to save: {}", e);
                window.publish_status_message(&format!("Save failed: {e}"), MessageKind::Error);
            }
        });
    }

    /// Discard unsaved changes and reload the file from disk.
    /// Shows a confirmation dialog before proceeding.
    pub(super) fn discard_changes(&self) {
        let Some(editor) = self.active_editor() else {
            return;
        };
        let Some(path) = editor.file_path() else {
            return;
        };
        if !editor.is_modified() {
            return;
        }
        let window_weak = self.downgrade();
        let editor_weak = editor.downgrade();
        self.show_discard_changes_dialog(&editor.title(), move |confirmed| {
            if !confirmed {
                return;
            }
            let Some(editor) = editor_weak.upgrade() else {
                return;
            };
            if let Some(window) = window_weak.upgrade() {
                window.delete_draft_for_path(&path);
            }
            editor.set_draft_restored(false);
            if let Some(window) = window_weak.upgrade() {
                window.dismiss_editor_notifications(&editor);
            }
            editor.load_file_async(&path);
        });
    }

    /// Update the enabled state of the discard-changes action based on the
    /// active tab's modified state and whether it has a backing file.
    pub(super) fn update_discard_action(&self) {
        if let Some(action) = self.lookup_action("discard-changes")
            && let Some(simple) = action.downcast_ref::<gio::SimpleAction>()
        {
            let enabled = self
                .active_editor()
                .is_some_and(|e| e.is_modified() && e.file_path().is_some());
            simple.set_enabled(enabled);
        }
    }

    /// Create a new untitled tab.
    pub fn new_tab(&self) {
        let editor_page = LushtextEditorPage::new();
        self.assign_draft_id(&editor_page);
        let page = self.imp().tab_view.append(&editor_page);
        page.set_title("Untitled");
        self.wire_modified_indicator(&page, &editor_page);
        self.wire_info_bar(&editor_page);
        self.configure_tab_page(&page);
        self.track_editor_memory(&editor_page);
        self.imp().tab_view.set_selected_page(&page);
        self.exit_preview_only_mode_now();
        self.update_content_stack();
        self.refresh_sidebar_file_row_states();
        self.refresh_open_popover_rows();
        self.refresh_status_bar();
    }

    /// Connect a buffer's modified-changed signal to update the tab title
    /// and header bar.
    fn wire_modified_indicator(&self, page: &libadwaita::TabPage, editor: &LushtextEditorPage) {
        let buffer = editor.buffer();
        // Buffer signals stay connected until the editor is rewired or dropped.
        // Clearing the group disconnects stale closures before attaching new
        // tab-title and draft-dirty listeners to the same buffer.
        editor.imp().document_buffer_signals.clear();
        let page_weak = page.downgrade();
        let window_weak = self.downgrade();
        let handler_id = buffer.connect_modified_changed(move |buf| {
            if let Some(page) = page_weak.upgrade()
                && let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>()
            {
                let name = editor.title();
                if buf.is_modified() {
                    let was_draft_dirty = editor.draft_dirty();
                    page.set_title(&format!("• {name}"));
                    editor.set_draft_dirty(true);
                    if !was_draft_dirty && let Some(window) = window_weak.upgrade() {
                        window.schedule_first_dirty_draft_autosave();
                    }
                } else {
                    page.set_title(&name);
                }
            }
            if let (Some(window), Some(page)) = (window_weak.upgrade(), page_weak.upgrade())
                && window.imp().tab_view.selected_page().as_ref() == Some(&page)
            {
                window.refresh_header_bar();
                window.update_discard_action();
            }
        });
        editor
            .imp()
            .document_buffer_signals
            .track(&buffer, handler_id);

        let window_weak = self.downgrade();
        let page_weak = page.downgrade();
        let changed_handler_id = buffer.connect_changed(move |_| {
            let mut became_draft_dirty = false;
            if let Some(page) = page_weak.upgrade()
                && let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>()
            {
                became_draft_dirty = !editor.draft_dirty();
                editor.set_draft_dirty(true);
            }
            if let Some(window) = window_weak.upgrade() {
                if became_draft_dirty {
                    window.schedule_first_dirty_draft_autosave();
                } else {
                    window.mark_draft_autosave_pending_if_inflight();
                }
                if let Some(page) = page_weak.upgrade()
                    && window.imp().tab_view.selected_page().as_ref() == Some(&page)
                {
                    window.refresh_preview_debounced();
                }
            }
        });
        editor
            .imp()
            .document_buffer_signals
            .track(&buffer, changed_handler_id);
    }

    /// Wire inline alert button callbacks for a newly created editor page.
    fn wire_info_bar(&self, editor: &LushtextEditorPage) {
        let window_weak = self.downgrade();
        let editor_weak = editor.downgrade();
        editor.connect_inline_notification(move |notification: InlineActionNotification| {
            if let Some(window) = window_weak.upgrade()
                && let Some(editor) = editor_weak.upgrade()
            {
                window.publish_editor_inline_notification(&editor, notification);
            }
        });

        let editor_weak = editor.downgrade();
        let window_weak = self.downgrade();
        editor.info_bar().connect_retry(move || {
            if let Some(editor) = editor_weak.upgrade() {
                if let Some(window) = window_weak.upgrade() {
                    window.dismiss_editor_notifications(&editor);
                }
                if let Some(ref path) = editor.file_path() {
                    editor.load_file_async(path);
                }
            }
        });

        let window_weak = self.downgrade();
        let editor_weak = editor.downgrade();
        editor.info_bar().connect_discard(move || {
            if let Some(editor) = editor_weak.upgrade() {
                match editor.take_pending_warning_action() {
                    Some(crate::ui::editor_page::PendingWarningAction::NormalizeLineEndings) => {
                        if let Some(window) = window_weak.upgrade() {
                            window.show_line_ending_controls_dialog();
                        }
                        return;
                    }
                    Some(crate::ui::editor_page::PendingWarningAction::UndoLocalHistoryRestore) => {
                        if let Some(window) = window_weak.upgrade() {
                            window.undo_local_history_restore(&editor);
                        }
                        return;
                    }
                    None => {}
                }
                if editor.is_draft_restored() {
                    if let Some(window) = window_weak.upgrade()
                        && let Some(ref path) = editor.file_path()
                    {
                        window.delete_draft_for_path(path);
                    }
                    editor.set_draft_restored(false);
                }
                if let Some(window) = window_weak.upgrade() {
                    window.dismiss_editor_notifications(&editor);
                }
                if let Some(ref path) = editor.file_path() {
                    editor.load_file_async(path);
                }
            }
        });

        let window_weak = self.downgrade();
        editor.info_bar().connect_save(move || {
            if let Some(window) = window_weak.upgrade() {
                window.save_current();
            }
        });

        let window_weak = self.downgrade();
        let editor_weak = editor.downgrade();
        editor.info_bar().connect_dismissed(move || {
            if let Some(window) = window_weak.upgrade()
                && let Some(editor) = editor_weak.upgrade()
            {
                window.dismiss_editor_notifications(&editor);
            }
        });
    }

    /// Switch the content stack and tab-strip chrome between tabbed and empty states.
    ///
    /// This also enables/disables actions that require an active tab.
    pub(super) fn update_content_stack(&self) {
        let imp = self.imp();
        let has_tabs = imp.tab_view.n_pages() > 0;
        self.sync_tab_bar_visibility();
        if has_tabs {
            imp.content_stack.set_visible_child_name("tabs");
        } else {
            imp.content_stack.set_visible_child_name("empty");
            if imp.preview_mode.get() {
                self.exit_preview_only_mode_now();
            }
        }

        for name in [
            "begin-search",
            "set-search-query",
            "begin-replace",
            "next-match",
            "prev-match",
            "toggle-bookmark",
            "edit-bookmark-label",
            "next-bookmark",
            "prev-bookmark",
            "save",
            "save-as",
            "show-local-history",
            "close-tab",
            "select-tab",
            "discard-changes",
            "print",
            "toggle-preview-pane",
            "set-preview-pane-visible",
            "toggle-preview-mode",
            "set-preview-mode",
        ] {
            if let Some(action) = self.lookup_action(name)
                && let Some(simple) = action.downcast_ref::<gio::SimpleAction>()
            {
                simple.set_enabled(has_tabs);
            }
        }

        self.update_search_navigation_actions();
    }

    /// Keep the tab strip visible only for normal-mode windows with real tab targets.
    pub(super) fn sync_tab_bar_visibility(&self) {
        let imp = self.imp();
        imp.tab_bar
            .set_visible(imp.tab_view.n_pages() > 0 && !self.is_focus_mode_active());
    }

    /// Enable or disable the F4/Shift+F4 search navigation actions.
    pub fn update_search_navigation_actions(&self) {
        let imp = self.imp();
        let enabled = imp.tab_view.n_pages() > 0
            && imp.search_panel_revealer.reveals_child()
            && imp.search_panel.has_results();

        for name in ["search-next-match", "search-prev-match"] {
            if let Some(action) = self.lookup_action(name)
                && let Some(simple) = action.downcast_ref::<gio::SimpleAction>()
            {
                simple.set_enabled(enabled);
            }
        }
    }

    /// Refresh the status bar and header bar for the active tab.
    pub(super) fn refresh_status_bar(&self) {
        let imp = self.imp();
        let editor = self.active_editor();
        imp.properties_panel.set_active_editor(editor.as_ref());
        if let Some(e) = &editor {
            imp.status_bar.set_metadata_visible(true);
            let ec_active = !e.formatting_overrides().is_empty()
                && imp.settings.boolean(keys::USE_EDITORCONFIG);
            imp.status_bar.set_editorconfig_active(ec_active);
            imp.status_bar
                .set_encoding_label(e.opened_encoding().label());
            let line_ending_label =
                if e.detected_line_ending() == crate::model::encoding::LineEnding::Mixed {
                    "Mixed"
                } else {
                    e.save_line_ending().label()
                };
            imp.status_bar.set_line_ending_label(line_ending_label);
        } else {
            imp.status_bar.set_metadata_visible(false);
            imp.status_bar.set_editorconfig_active(false);
        }
        self.refresh_header_bar_with(editor.as_ref());
        self.update_discard_action();
        self.update_local_history_action();
        self.refresh_notes_menu_state();
    }

    /// Update the header bar title/subtitle to reflect the given editor.
    pub(super) fn refresh_header_bar(&self) {
        self.refresh_header_bar_with(self.active_editor().as_ref());
    }

    fn refresh_header_bar_with(&self, editor: Option<&LushtextEditorPage>) {
        let title_widget = &self.imp().title_widget;
        if let Some(editor) = editor {
            let name = editor.title();
            let title = if editor.is_modified() {
                format!("• {name}")
            } else {
                name
            };
            title_widget.set_title(&title);
            let subtitle = editor
                .file_path()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            title_widget.set_subtitle(&subtitle);
        } else {
            title_widget.set_title("LushText");
            title_widget.set_subtitle("");
        }
    }

    /// Resolve EditorConfig overrides for a file on a background thread
    /// and apply them to the editor page when done.
    pub(super) fn resolve_editorconfig_for_editor(&self, editor: &LushtextEditorPage, path: &Path) {
        if !self.imp().settings.boolean(keys::USE_EDITORCONFIG) {
            return;
        }
        let path = path.to_path_buf();
        let window_weak = self.downgrade();
        spawn_blocking_then(
            editor.clone(),
            move || editorconfig::resolve_for_path(&path),
            move |editor, overrides| {
                editor.apply_editorconfig_overrides(overrides);
                if let Some(window) = window_weak.upgrade()
                    && window
                        .active_editor()
                        .as_ref()
                        .is_some_and(|active| active.as_ptr() == editor.as_ptr())
                {
                    window.refresh_status_bar();
                }
            },
        );
    }

    /// Handle the `use-editorconfig` GSettings toggle changing.
    pub(super) fn on_use_editorconfig_changed(&self, enabled: bool) {
        let tab_view = &self.imp().tab_view;
        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                if enabled {
                    if let Some(path) = editor.file_path() {
                        self.resolve_editorconfig_for_editor(editor, &path);
                    }
                } else {
                    editor.clear_editorconfig_overrides();
                }
            }
        }
        self.refresh_status_bar();
    }

    /// Get the currently active editor page, if any.
    pub(crate) fn active_editor(&self) -> Option<LushtextEditorPage> {
        self.imp()
            .tab_view
            .selected_page()
            .and_then(|page| page.child().downcast::<LushtextEditorPage>().ok())
    }

    /// Whether `editor` is the tab currently selected in the window shell.
    #[must_use]
    pub(crate) fn is_active_editor(&self, editor: &LushtextEditorPage) -> bool {
        self.active_editor()
            .as_ref()
            .is_some_and(|active| active.as_ptr() == editor.as_ptr())
    }

    /// Whether `editor` is still mounted as an open tab in this window.
    pub(crate) fn contains_editor(&self, editor: &LushtextEditorPage) -> bool {
        let tab_view = &self.imp().tab_view;
        (0..tab_view.n_pages()).any(|index| {
            tab_view
                .nth_page(index)
                .child()
                .downcast_ref::<LushtextEditorPage>()
                .is_some_and(|candidate| candidate.as_ptr() == editor.as_ptr())
        })
    }

    /// Return file identities owned by mounted, non-failed editor tabs.
    ///
    /// This is the source of truth for visible UI state. The `open_paths` cache
    /// can be ahead or behind during close/detach and async path refreshes, but
    /// mounted tabs tell the Open popover which files are truly open now.
    pub(super) fn current_open_document_identities(&self) -> HashSet<PathBuf> {
        let tab_view = &self.imp().tab_view;
        let mut identities = HashSet::new();
        for index in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(index);
            let child = page.child();
            let Some(editor) = child.downcast_ref::<LushtextEditorPage>() else {
                continue;
            };
            collect_open_document_identities(&mut identities, editor);
        }
        identities
    }

    /// Rebuild duplicate-detection bookkeeping from the mounted tab model.
    ///
    /// The cache keeps `open_document()` fast, but tab close, Save As, failed
    /// loads, and delayed canonical probes can leave stale identities behind.
    /// Scrubbing from live tabs keeps future duplicate checks healthy without
    /// making the cache the Open popover's visibility source of truth.
    pub(super) fn reconcile_open_paths_from_tabs(&self) {
        let identities = self.current_open_document_identities();
        self.imp().open_paths.replace(identities);
    }

    /// Temporarily coalesce tab-model projection refreshes during tab storms.
    pub(super) fn begin_tab_projection_refresh_batch(&self) {
        let depth = self.imp().tab_projection_refresh_defer_depth.get();
        self.imp()
            .tab_projection_refresh_defer_depth
            .set(depth.saturating_add(1));
    }

    /// End a coalesced tab-model refresh and rebuild derived state once.
    pub(super) fn end_tab_projection_refresh_batch(&self) {
        let depth = self.imp().tab_projection_refresh_defer_depth.get();
        debug_assert!(depth > 0, "tab projection refresh batch underflow");
        if depth <= 1 {
            self.imp().tab_projection_refresh_defer_depth.set(0);
            self.refresh_tab_model_projections();
        } else {
            self.imp().tab_projection_refresh_defer_depth.set(depth - 1);
        }
    }

    /// Whether tab-count notifications should leave derived projections deferred.
    pub(super) fn tab_projection_refresh_deferred(&self) -> bool {
        self.imp().tab_projection_refresh_defer_depth.get() > 0
    }

    /// Rebuild all window projections that derive from the mounted tab model.
    pub(super) fn refresh_tab_model_projections(&self) {
        self.reconcile_open_paths_from_tabs();
        self.update_content_stack();
        self.refresh_command_palette_sources();
        self.refresh_sidebar_file_row_states();
        self.refresh_open_popover_rows();
        self.refresh_status_bar();
    }

    /// Refresh sidebar file-row markers from the current file-backed tabs.
    pub(super) fn refresh_sidebar_file_row_states(&self) {
        let tab_view = &self.imp().tab_view;
        let selected_page = tab_view.selected_page();
        let mut open_identities = HashSet::new();
        let mut active_identities = HashSet::new();

        for index in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(index);
            let child = page.child();
            let Some(editor) = child.downcast_ref::<LushtextEditorPage>() else {
                continue;
            };
            if editor.load_state() == EditorLoadState::Failed {
                continue;
            }

            collect_open_document_identities(&mut open_identities, editor);
            if selected_page.as_ref() == Some(&page) {
                collect_open_document_identities(&mut active_identities, editor);
            }
        }

        self.imp().sidebar.set_file_row_state_snapshot(
            SidebarFileRowStateSnapshot::from_identities(open_identities, active_identities),
        );
    }

    /// Update the file path and title for any tab matching `old_path`.
    /// For directory renames, rewrites paths of all files inside the directory.
    pub fn update_tab_path(&self, old_path: &Path, new_path: &Path) {
        let tab_view = &self.imp().tab_view;
        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                let Some(ep) = editor.file_path() else {
                    continue;
                };
                let updated = if ep.as_path() == old_path {
                    new_path.to_path_buf()
                } else if let Ok(suffix) = ep.strip_prefix(old_path) {
                    new_path.join(suffix)
                } else {
                    continue;
                };

                let mut paths = self.imp().open_paths.borrow_mut();
                paths.remove(ep.as_path());
                paths.remove(&open_path_key(&ep));
                if let Some(canonical_path) = editor.canonical_file_path() {
                    paths.remove(&canonical_path);
                }
                paths.insert(open_path_key(&updated));
                drop(paths);
                editor.set_file_path(&updated);
                self.refresh_canonical_path_after_rename(editor, &updated);
                page.set_title(&editor.title());
            }
        }
        self.reconcile_open_paths_from_tabs();
        self.refresh_sidebar_file_row_states();
        self.refresh_open_popover_rows();
        self.refresh_header_bar();
        self.refresh_command_palette_sources();
        self.refresh_status_bar();
    }

    pub(super) fn refresh_canonical_path_after_rename(
        &self,
        editor: &LushtextEditorPage,
        updated: &Path,
    ) {
        let updated_for_probe = updated.to_path_buf();
        let updated_for_apply = updated.to_path_buf();
        let editor_weak = editor.downgrade();
        spawn_blocking_then(
            self.clone(),
            move || {
                delay_canonical_refresh_for_test();
                fs_metadata::canonical_path(&updated_for_probe).ok()
            },
            move |window, canonical| {
                let Some(canonical) = canonical else {
                    return;
                };
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                if !window.contains_editor(&editor)
                    || editor.file_path().as_deref() != Some(updated_for_apply.as_path())
                {
                    return;
                }
                window
                    .imp()
                    .open_paths
                    .borrow_mut()
                    .insert(canonical.clone());
                editor.set_file_path_with_canonical(&updated_for_apply, Some(canonical));
                window.reconcile_open_paths_from_tabs();
                window.refresh_sidebar_file_row_states();
                window.refresh_open_popover_rows();
            },
        );
    }

    /// Close any tab whose file path matches `path` or is inside it (for directories).
    pub fn close_tab_for_path(&self, path: &Path) {
        let tab_view = &self.imp().tab_view;
        self.begin_tab_projection_refresh_batch();
        // Closing pages from the end preserves earlier page indexes while
        // directory deletes may remove many matching tabs in one pass.
        for i in (0..tab_view.n_pages()).rev() {
            let page = tab_view.nth_page(i);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                let Some(ep) = editor.file_path() else {
                    continue;
                };
                if ep.as_path() == path || ep.starts_with(path) {
                    let mut paths = self.imp().open_paths.borrow_mut();
                    paths.remove(ep.as_path());
                    paths.remove(&open_path_key(&ep));
                    if let Some(canonical_path) = editor.canonical_file_path() {
                        paths.remove(&canonical_path);
                    }
                    drop(paths);
                    editor.cancel_load();
                    editor.stop_file_monitor();
                    self.untrack_editor_memory(editor);
                    tab_view.close_page(&page);
                }
            }
        }
        self.end_tab_projection_refresh_batch();
    }
}

fn collect_open_document_identities(
    identities: &mut HashSet<PathBuf>,
    editor: &LushtextEditorPage,
) {
    if editor.load_state() == EditorLoadState::Failed {
        return;
    }
    if let Some(path) = editor.file_path() {
        identities.insert(open_path_key(&path));
    }
    if let Some(canonical_path) = editor.canonical_file_path() {
        identities.insert(canonical_path);
    }
}

fn delay_canonical_refresh_for_test() {
    #[cfg(feature = "test-utils")]
    {
        let delay_ms = CANONICAL_REFRESH_DELAY_MS.load(Ordering::Acquire);
        if delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
    }
}
