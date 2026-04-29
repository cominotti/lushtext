// SPDX-License-Identifier: GPL-3.0-or-later

//! Document lifecycle and window chrome helpers for the main window.

use std::path::Path;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;
use libadwaita::prelude::AnimationExt;

use crate::config::keys;
use crate::services::async_task;
use crate::services::editorconfig;
use crate::services::notifications::InlineActionNotification;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::status_bar::MessageKind;

use super::LushtextWindow;

impl LushtextWindow {
    /// Open a file in a new tab, or focus the existing tab if already open.
    ///
    /// This remains the single authority for real document opening, so sidebar
    /// double-click, `Enter`, Save As handoff, and file-peek promotion all
    /// reuse the same duplicate-tab and editor-focus behavior.
    pub fn open_document(&self, path: &Path) {
        let tab_view = &self.imp().tab_view;
        if self.imp().open_paths.borrow().contains(path) {
            for i in 0..tab_view.n_pages() {
                let page = tab_view.nth_page(i);
                if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>()
                    && editor.file_path().as_deref() == Some(path)
                {
                    tab_view.set_selected_page(&page);
                    return;
                }
            }
        }

        self.imp()
            .open_paths
            .borrow_mut()
            .insert(path.to_path_buf());
        let editor_page = LushtextEditorPage::new();
        self.wire_info_bar(&editor_page);
        self.wire_note_callbacks(&editor_page);
        editor_page.load_file_async(path);
        editor_page.start_file_monitor();
        self.resolve_editorconfig_for_editor(&editor_page, path);
        self.assign_draft_id(&editor_page);

        let window_weak = self.downgrade();
        let path_for_draft = path.to_path_buf();
        let editor_weak = editor_page.downgrade();
        *editor_page.imp().load.load_completed_callback.borrow_mut() = Some(Box::new(move || {
            if let Some(window) = window_weak.upgrade()
                && let Some(editor) = editor_weak.upgrade()
            {
                window.check_draft_on_open(&editor, &path_for_draft);
                window.refresh_status_bar();
            }
        }));

        let page = tab_view.append(&editor_page);
        page.set_title(&editor_page.title());
        self.wire_modified_indicator(&page, &editor_page);
        self.configure_tab_page(&page);
        self.track_editor_memory(&editor_page);

        tab_view.set_selected_page(&page);
        self.update_content_stack();
        self.refresh_status_bar();
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
                }
                if let Some(editor) = window.active_editor() {
                    editor.set_draft_restored(false);
                    window.dismiss_editor_notifications(&editor);
                }
                window.publish_status_message("File saved", MessageKind::Info);
                window.refresh_status_bar();
            }
            Err(crate::ui::editor_page::SaveError::LossyEncoding { preview, .. }) => {
                let window_for_retry = window.clone();
                window.confirm_lossy_save(&editor_for_retry, &preview, move || {
                    window_for_retry.save_current();
                });
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
        self.update_content_stack();
        self.refresh_status_bar();
    }

    /// Connect a buffer's modified-changed signal to update the tab title
    /// and header bar.
    fn wire_modified_indicator(&self, page: &libadwaita::TabPage, editor: &LushtextEditorPage) {
        let buffer = editor.buffer();
        if let Some(previous) = editor.imp().modified_handler_id.borrow_mut().take() {
            buffer.disconnect(previous);
        }
        let page_weak = page.downgrade();
        let window_weak = self.downgrade();
        let handler_id = buffer.connect_modified_changed(move |buf| {
            if let Some(page) = page_weak.upgrade()
                && let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>()
            {
                let name = editor.title();
                if buf.is_modified() {
                    page.set_title(&format!("• {name}"));
                    editor.set_draft_dirty(true);
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
        editor.imp().modified_handler_id.replace(Some(handler_id));

        let window_weak = self.downgrade();
        let page_weak = page.downgrade();
        let changed_handler_id = buffer.connect_changed(move |_| {
            if let Some(page) = page_weak.upgrade()
                && let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>()
            {
                editor.set_draft_dirty(true);
            }
            if let (Some(window), Some(page)) = (window_weak.upgrade(), page_weak.upgrade())
                && window.imp().tab_view.selected_page().as_ref() == Some(&page)
            {
                window.refresh_preview_debounced();
            }
        });
        editor
            .imp()
            .buffer_changed_handler_id
            .replace(Some(changed_handler_id));
    }

    /// Wire info bar button callbacks for a newly created editor page.
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

    /// Switch the content stack between "tabs" and "empty" states,
    /// and enable/disable actions that require an active tab.
    pub(super) fn update_content_stack(&self) {
        let imp = self.imp();
        let has_tabs = imp.tab_view.n_pages() > 0;
        if has_tabs {
            imp.content_stack.set_visible_child_name("tabs");
        } else {
            imp.content_stack.set_visible_child_name("empty");
            if imp.preview_mode.get() {
                imp.preview_mode.set(false);
                imp.editor_box.set_visible(true);
                imp.markdown_preview.set_visible(false);
                if let Some(anim) = imp.preview_animation.take() {
                    anim.pause();
                }
                imp.preview_paned.set_shrink_start_child(false);
            }
        }

        for name in [
            "begin-search",
            "begin-replace",
            "next-match",
            "prev-match",
            "toggle-bookmark",
            "edit-bookmark-label",
            "next-bookmark",
            "prev-bookmark",
            "add-annotation",
            "edit-annotation",
            "save",
            "save-as",
            "show-local-history",
            "close-tab",
            "discard-changes",
            "print",
            "toggle-preview-pane",
            "toggle-preview-mode",
        ] {
            if let Some(action) = self.lookup_action(name)
                && let Some(simple) = action.downcast_ref::<gio::SimpleAction>()
            {
                simple.set_enabled(has_tabs);
            }
        }

        self.update_search_navigation_actions();
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
        async_task::spawn_blocking_then(
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
                paths.insert(updated.clone());
                drop(paths);
                editor.set_file_path(&updated);
                page.set_title(&editor.title());
            }
        }
        self.refresh_header_bar();
        self.refresh_status_bar();
    }

    /// Close any tab whose file path matches `path` or is inside it (for directories).
    pub fn close_tab_for_path(&self, path: &Path) {
        let tab_view = &self.imp().tab_view;
        for i in (0..tab_view.n_pages()).rev() {
            let page = tab_view.nth_page(i);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                let Some(ep) = editor.file_path() else {
                    continue;
                };
                if ep.as_path() == path || ep.starts_with(path) {
                    self.imp().open_paths.borrow_mut().remove(ep.as_path());
                    editor.cancel_load();
                    editor.stop_file_monitor();
                    self.untrack_editor_memory(editor);
                    tab_view.close_page(&page);
                }
            }
        }
        self.update_content_stack();
        self.refresh_status_bar();
    }
}
