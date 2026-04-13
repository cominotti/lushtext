// SPDX-License-Identifier: GPL-3.0-or-later

//! Main application window.

mod dialogs;
// Private implementation module. In GTK's GObject system, every widget has
// two halves: a private struct (imp.rs) holding data and trait impls, and
// a public wrapper type (this file) providing the API.
mod imp;
// Markdown preview pane: side-by-side and preview-only toggle modes.
mod preview;
// Print action: GtkSourceView PrintCompositor + native print dialog.
mod print;
// Workspace-wide search panel: toggle, pre-fill, result activation, focus.
mod search;
// Session persistence and draft management.
mod session;
// Zoom controls: hamburger menu widget and window actions.
mod zoom;

use crate::config::keys;
use crate::model::draft::DraftEntry;
use crate::services::async_task;
use crate::services::editorconfig;
use crate::services::notifications::{
    InlineActionNotification, NotificationOwner, NotificationPayload, NotificationSeverity,
    NotificationSurface,
};
use crate::services::palette::FileIndex;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::status_bar::MessageKind;
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;
use libadwaita::prelude::AnimationExt;
use std::path::Path;
use std::time::Duration;

/// Maximum total estimated buffer memory across all tabs before evicting
/// unmodified background tabs. ~256MB is comfortable on 8GB machines.
const BUFFER_MEMORY_BUDGET: u64 = 256_000_000;

/// Map a GSettings `color-scheme` string to its `libadwaita::ColorScheme` variant.
/// Unknown values fall back to `Default` (follow system).
#[must_use]
pub fn parse_color_scheme(value: &str) -> libadwaita::ColorScheme {
    match value {
        "force-light" => libadwaita::ColorScheme::ForceLight,
        "force-dark" => libadwaita::ColorScheme::ForceDark,
        _ => libadwaita::ColorScheme::Default,
    }
}

// glib::wrapper! generates the public wrapper type for this widget.
// @extends declares the GTK class hierarchy; @implements lists interfaces.
glib::wrapper! {
    pub struct LushtextWindow(ObjectSubclass<imp::LushtextWindow>)
        @extends libadwaita::ApplicationWindow, gtk4::ApplicationWindow, gtk4::Window, gtk4::Widget,
        @implements gio::ActionMap, gio::ActionGroup, gtk4::Accessible, gtk4::Buildable,
                    gtk4::ConstraintTarget, gtk4::Native, gtk4::Root, gtk4::ShortcutManager;
}

impl LushtextWindow {
    #[must_use]
    pub fn new(app: &libadwaita::Application) -> Self {
        let window: Self = Object::builder().property("application", app).build();
        window.setup_actions();
        window.setup_fullscreen();
        window.setup_theme_selector();
        preview::setup_preview_actions(&window);
        print::setup_print_action(&window);
        zoom::setup_zoom_actions(&window);
        zoom::setup_zoom_controls(&window);
        search::setup_search_panel(&window);
        window.start_notification_sweep_timer();
        window.setup_shortcuts();
        window.update_content_stack();
        window.refresh_status_bar();
        window.render_notifications();
        window
    }

    /// Open a file in a new tab, or focus existing tab if already open.
    /// The tab appears immediately; file content loads asynchronously.
    pub fn open_document(&self, path: &Path) {
        let tab_view = &self.imp().tab_view;
        // O(1) duplicate check; only iterates tabs when a duplicate is found
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
        editor_page.load_file_async(path);
        editor_page.start_file_monitor();
        self.resolve_editorconfig_for_editor(&editor_page, path);
        self.assign_draft_id(&editor_page);
        // Register this file path in the draft manifest so autosave knows
        // the original_path for new entries.
        if let Some(draft_id) = editor_page.draft_id() {
            let mut manifest = self.imp().draft_manifest.borrow_mut();
            if manifest.find_by_id(&draft_id).is_none() {
                manifest.upsert(DraftEntry {
                    draft_id,
                    original_path: Some(path.to_path_buf()),
                    // Mtime populated later by autosave (background thread) or
                    // load_file_async — avoids blocking the main thread with
                    // stat() during session restore.
                    original_mtime_secs: None,
                    saved_at_secs: 0,
                });
            }
        }
        // Defer draft recovery to run AFTER file content loads. Without
        // this, check_draft_on_open races with load_file_async — the file
        // load typically finishes last and overwrites draft content.
        let window_weak = self.downgrade();
        let path_for_draft = path.to_path_buf();
        let editor_weak = editor_page.downgrade();
        *editor_page.imp().load_completed_callback.borrow_mut() = Some(Box::new(move || {
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
        self.track_editor_memory(&editor_page);

        tab_view.set_selected_page(&page);
        self.update_content_stack();
        self.refresh_status_bar();
    }

    /// Save the active tab's file. If untitled, shows Save As dialog.
    fn save_current(&self) {
        let Some(editor) = self.active_editor() else {
            return;
        };
        if editor.file_path().is_none() {
            self.show_save_as_dialog();
            return;
        }
        let window = self.clone();
        let save_path = editor.file_path();
        editor.save_file_async(move |result| match result {
            Ok(()) => {
                // Delete draft and dismiss info bar after a successful save.
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
            Err(e) => {
                tracing::error!("Failed to save: {}", e);
                window.publish_status_message(&format!("Save failed: {e}"), MessageKind::Error);
            }
        });
    }

    /// Discard unsaved changes and reload the file from disk.
    /// Shows a confirmation dialog before proceeding.
    fn discard_changes(&self) {
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

    pub fn render_notifications(&self) {
        let imp = self.imp();
        let status_view = imp.notification_bus.status_bar_view();
        imp.status_bar.render_message(status_view.as_ref());

        let tab_view = &imp.tab_view;
        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                let editor_view = imp
                    .notification_bus
                    .editor_info_bar_view(editor.notification_owner_id());
                editor.info_bar().render_notification(editor_view.as_ref());
            }
        }
    }

    pub fn publish_status_message(&self, text: &str, severity: NotificationSeverity) {
        if self.imp().notification_bus.publish(
            NotificationOwner::Window,
            NotificationSurface::StatusBar,
            NotificationPayload::Transient(crate::services::notifications::StatusMessage {
                text: text.to_string(),
                severity,
            }),
        ) {
            self.render_notifications();
        }
    }

    pub fn publish_editor_inline_notification(
        &self,
        editor: &LushtextEditorPage,
        notification: InlineActionNotification,
    ) {
        let owner = NotificationOwner::Editor(editor.notification_owner_id());
        let surface = NotificationSurface::EditorInfoBar(editor.notification_owner_id());
        if self.imp().notification_bus.publish(
            owner,
            surface,
            NotificationPayload::InlineAction(notification),
        ) {
            self.render_notifications();
        }
    }

    pub fn dismiss_editor_notifications(&self, editor: &LushtextEditorPage) {
        if self
            .imp()
            .notification_bus
            .dismiss_owner(NotificationOwner::Editor(editor.notification_owner_id()))
        {
            self.render_notifications();
        }
    }

    fn start_notification_sweep_timer(&self) {
        let window_weak = self.downgrade();
        let source_id = glib::timeout_add_local(Duration::from_secs(1), move || {
            let Some(window) = window_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if window.imp().notification_bus.sweep_expired() {
                window.render_notifications();
            }
            glib::ControlFlow::Continue
        });
        self.imp()
            .notification_sweep_source_id
            .replace(Some(source_id));
    }

    /// Update the enabled state of the discard-changes action based on the
    /// active tab's modified state and whether it has a backing file.
    fn update_discard_action(&self) {
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
        self.track_editor_memory(&editor_page);
        self.imp().tab_view.set_selected_page(&page);
        self.update_content_stack();
        self.refresh_status_bar();
    }

    /// Connect a buffer's modified-changed signal to update the tab title
    /// and header bar. Prepends "● " to the tab title when the buffer has
    /// unsaved changes, placing the dot immediately before the filename.
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
                    // Mark the buffer as needing a draft save on the next
                    // autosave tick.
                    editor.set_draft_dirty(true);
                } else {
                    page.set_title(&name);
                }
            }
            // Only refresh header bar and discard action if this is the active tab
            if let (Some(window), Some(page)) = (window_weak.upgrade(), page_weak.upgrade())
                && window.imp().tab_view.selected_page().as_ref() == Some(&page)
            {
                window.refresh_header_bar();
                window.update_discard_action();
            }
        });
        editor.imp().modified_handler_id.replace(Some(handler_id));

        // Wire buffer text changes to re-arm the draft dirty flag and refresh
        // the preview pane (debounced). `connect_changed` fires on every text
        // mutation — unlike `connect_modified_changed` which only fires on
        // is_modified() state transitions. This ensures `draft_dirty` is re-set
        // after the autosave timer clears it, even when the buffer stays
        // continuously modified (e.g., after draft restore).
        // Handler ID is stored and disconnected in EditorPage::dispose() to
        // prevent accumulation across tab open/close cycles.
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
    /// Connects retry (reload file), save, and discard/reload buttons.
    fn wire_info_bar(&self, editor: &LushtextEditorPage) {
        let window_weak = self.downgrade();
        let editor_weak = editor.downgrade();
        editor.connect_inline_notification(move |notification| {
            if let Some(window) = window_weak.upgrade()
                && let Some(editor) = editor_weak.upgrade()
            {
                window.publish_editor_inline_notification(&editor, notification);
            }
        });

        // Retry: re-attempt loading the file (for access errors)
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

        // Discard: reload original file content, delete draft if applicable.
        // Used for both "Discard draft" and "Discard Changes and Reload".
        let window_weak = self.downgrade();
        let editor_weak = editor.downgrade();
        editor.info_bar().connect_discard(move || {
            if let Some(editor) = editor_weak.upgrade() {
                // Delete the draft file if this was a draft-restored scenario.
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

        // Save: save current buffer content (used for draft save)
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
    fn update_content_stack(&self) {
        let imp = self.imp();
        let has_tabs = imp.tab_view.n_pages() > 0;
        let stack = &imp.content_stack;
        if has_tabs {
            stack.set_visible_child_name("tabs");
        } else {
            stack.set_visible_child_name("empty");
            // Reset preview state when all tabs are closed so the "tabs" page
            // is in a clean state when the next tab opens.
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
            "save",
            "save-as",
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

        // Search navigation actions have additional conditions beyond has_tabs.
        self.update_search_navigation_actions();
    }

    /// Enable or disable the F4/Shift+F4 search navigation actions.
    /// Disabled when: no tabs open, search panel not visible, or no results.
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
    /// Single `active_editor()` lookup shared by both updates.
    fn refresh_status_bar(&self) {
        let imp = self.imp();
        let editor = self.active_editor();
        imp.properties_panel.set_active_editor(editor.as_ref());
        // Status bar
        match &editor {
            Some(e) => {
                imp.status_bar.set_metadata_visible(true);
                imp.status_bar.set_file_size(e.file_size());
                let ec_active = !e.formatting_overrides().is_empty()
                    && imp.settings.boolean(keys::USE_EDITORCONFIG);
                imp.status_bar.set_editorconfig_active(ec_active);
            }
            None => {
                imp.status_bar.set_metadata_visible(false);
            }
        }
        // Header bar title/subtitle + modified dot
        self.refresh_header_bar_with(editor.as_ref());
        // Discard-changes action depends on per-tab modified + file-backed state
        self.update_discard_action();
    }

    /// Update the header bar title/subtitle to reflect the given editor.
    /// Prepends "● " to the title when the buffer has unsaved changes.
    /// Reverts to "LushText" with no subtitle when no editor is active.
    fn refresh_header_bar(&self) {
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

    // --- EditorConfig resolution ---

    /// Resolve EditorConfig overrides for a file on a background thread
    /// and apply them to the editor page when done.
    fn resolve_editorconfig_for_editor(&self, editor: &LushtextEditorPage, path: &Path) {
        if !self.imp().settings.boolean(keys::USE_EDITORCONFIG) {
            return;
        }
        let path = path.to_path_buf();
        async_task::spawn_blocking_then(
            editor.clone(),
            move || editorconfig::resolve_for_path(&path),
            |editor, overrides| {
                editor.apply_editorconfig_overrides(overrides);
            },
        );
    }

    /// Handle the `use-editorconfig` GSettings toggle changing. Re-resolves
    /// or clears EditorConfig overrides on all open tabs.
    fn on_use_editorconfig_changed(&self, enabled: bool) {
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
    fn active_editor(&self) -> Option<LushtextEditorPage> {
        self.imp()
            .tab_view
            .selected_page()
            .and_then(|page| page.child().downcast::<LushtextEditorPage>().ok())
    }

    fn track_editor_memory(&self, editor: &LushtextEditorPage) {
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
            .buffer_memory_by_editor
            .borrow_mut()
            .insert(key, bytes)
            .unwrap_or(0);
        let total = imp
            .buffer_memory_total
            .get()
            .saturating_sub(previous)
            .saturating_add(bytes);
        imp.buffer_memory_total.set(total);
    }

    fn untrack_editor_memory(&self, editor: &LushtextEditorPage) {
        let key = editor.as_ptr() as usize;
        if let Some(previous) = self.imp().buffer_memory_by_editor.borrow_mut().remove(&key) {
            self.imp().buffer_memory_total.set(
                self.imp()
                    .buffer_memory_total
                    .get()
                    .saturating_sub(previous),
            );
        }
    }

    fn setup_actions(&self) {
        self.add_action_entries([
            gio::ActionEntry::builder("new-tab")
                .activate(|window: &Self, _, _| window.new_tab())
                .build(),
            gio::ActionEntry::builder("open-file")
                .activate(|window: &Self, _, _| window.show_open_file_dialog())
                .build(),
            gio::ActionEntry::builder("open-folder")
                .activate(|window: &Self, _, _| {
                    window.imp().sidebar.create_new_workspace();
                })
                .build(),
            gio::ActionEntry::builder("save")
                .activate(|window: &Self, _, _| window.save_current())
                .build(),
            gio::ActionEntry::builder("save-as")
                .activate(|window: &Self, _, _| window.show_save_as_dialog())
                .build(),
            gio::ActionEntry::builder("begin-search")
                .activate(|window: &Self, _, _| {
                    if window.imp().search_panel_revealer.reveals_child() {
                        window.close_search_panel();
                        window.after_search_panel_transition(|window| {
                            if let Some(editor) = window.active_editor() {
                                editor.show_search();
                            }
                        });
                    } else if let Some(editor) = window.active_editor() {
                        editor.show_search();
                    }
                })
                .build(),
            gio::ActionEntry::builder("begin-replace")
                .activate(|window: &Self, _, _| {
                    if window.imp().search_panel_revealer.reveals_child() {
                        window.close_search_panel();
                        window.after_search_panel_transition(|window| {
                            if let Some(editor) = window.active_editor() {
                                editor.show_replace();
                            }
                        });
                    } else if let Some(editor) = window.active_editor() {
                        editor.show_replace();
                    }
                })
                .build(),
            gio::ActionEntry::builder("next-match")
                .activate(|window: &Self, _, _| {
                    if let Some(editor) = window.active_editor()
                        && editor.is_search_visible()
                    {
                        editor.search_bar().move_next();
                    }
                })
                .build(),
            gio::ActionEntry::builder("prev-match")
                .activate(|window: &Self, _, _| {
                    if let Some(editor) = window.active_editor()
                        && editor.is_search_visible()
                    {
                        editor.search_bar().move_prev();
                    }
                })
                .build(),
            gio::ActionEntry::builder("close-tab")
                .activate(|window: &Self, _, _| {
                    // Delegate to AdwTabView::close_page which fires connect_close_page.
                    // The close_page handler shows the save-changes dialog if needed,
                    // and page_detached handles cleanup (open_paths, monitor, memory).
                    let tab_view = &window.imp().tab_view;
                    if let Some(page) = tab_view.selected_page() {
                        tab_view.close_page(&page);
                    }
                })
                .build(),
            gio::ActionEntry::builder("toggle-command-palette")
                .activate(|window: &Self, _, _| window.toggle_command_palette())
                .build(),
            gio::ActionEntry::builder("toggle-search-panel")
                .activate(|window: &Self, _, _| window.toggle_search_panel())
                .build(),
            gio::ActionEntry::builder("search-next-match")
                .activate(|window: &Self, _, _| {
                    window.imp().search_panel.navigate_next_match();
                })
                .build(),
            gio::ActionEntry::builder("search-prev-match")
                .activate(|window: &Self, _, _| {
                    window.imp().search_panel.navigate_prev_match();
                })
                .build(),
        ]);

        // Discard Changes: reload file from disk after user confirmation.
        // Uses SimpleAction (not ActionEntry) for fine-grained enabled state —
        // must be disabled when the buffer is unmodified or untitled.
        let discard_action = gio::SimpleAction::new("discard-changes", None);
        discard_action.set_enabled(false);
        {
            let window_weak = self.downgrade();
            discard_action.connect_activate(move |_, _| {
                if let Some(window) = window_weak.upgrade() {
                    window.discard_changes();
                }
            });
        }
        self.add_action(&discard_action);

        self.register_split_view_toggle_action(
            "toggle-sidebar",
            &self.imp().workspace_split_view,
            keys::WORKSPACE_SIDEBAR_VISIBLE,
            |imp, visible| {
                imp.sidebar_visible.set(visible);
            },
        );
        self.register_split_view_toggle_action(
            "toggle-properties",
            &self.imp().properties_split_view,
            keys::PROPERTIES_SIDEBAR_VISIBLE,
            |imp, visible| {
                imp.properties_sidebar_visible.set(visible);
            },
        );
    }

    fn register_split_view_toggle_action(
        &self,
        action_name: &'static str,
        split_view: &libadwaita::OverlaySplitView,
        settings_key: &'static str,
        cache_visible: fn(&imp::LushtextWindow, bool),
    ) {
        let action = gio::SimpleAction::new_stateful(
            action_name,
            None,
            &split_view.shows_sidebar().to_variant(),
        );
        {
            let split_view = split_view.clone();
            action.connect_change_state(move |_action, state| {
                let Some(state) = state else { return };
                let Some(new_visible) = state.get::<bool>() else {
                    tracing::error!("{action_name}: expected bool state");
                    return;
                };
                split_view.set_show_sidebar(new_visible);
            });
        }
        self.add_action(&action);

        let window_weak = self.downgrade();
        split_view.connect_show_sidebar_notify(move |split| {
            let visible = split.shows_sidebar();
            action.set_state(&visible.to_variant());
            if let Some(window) = window_weak.upgrade() {
                cache_visible(window.imp(), visible);
                let _ = window.imp().settings.set_boolean(settings_key, visible);
                if !visible {
                    window.restore_focus_after_secondary_pane_close();
                }
            }
        });
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
        // Iterate in reverse so removing pages doesn't shift indices
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

    fn toggle_command_palette(&self) {
        let imp = self.imp();
        if imp.palette_revealer.reveals_child() {
            self.close_command_palette();
        } else {
            // Save the currently focused widget before the palette steals focus
            let weak = glib::WeakRef::new();
            if let Some(focused) = gtk4::prelude::GtkWindowExt::focus(self) {
                weak.set(Some(&focused));
            }
            imp.saved_focus.replace(Some(weak));

            imp.palette_revealer.set_reveal_child(true);
            imp.command_palette.open();
        }
    }

    fn close_command_palette(&self) {
        let imp = self.imp();
        imp.command_palette.close();
        imp.palette_revealer.set_reveal_child(false);
        self.restore_saved_focus();
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
        let attempts_clone = attempts.clone();

        glib::timeout_add_local(Duration::from_millis(30), move || {
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

            if focused || next_attempt >= 6 {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    /// Restore focus to the widget saved before an overlay was opened.
    /// Falls back to the active editor's source view if the saved widget
    /// is gone (e.g., tab closed while palette was open).
    /// If no editor is active (empty state), clears window focus.
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
    fn reload_if_evicted(&self) {
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
    /// Single pass collects total + eviction candidates; second pass only visits candidates.
    fn maybe_evict_background_tabs(&self) {
        if self.imp().buffer_memory_total.get() <= BUFFER_MEMORY_BUDGET {
            return;
        }

        let tab_view = &self.imp().tab_view;
        let selected = tab_view.selected_page();
        let mut total = self.imp().buffer_memory_total.get();
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

    /// Build the file index from all workspace roots on a background thread.
    /// Debounced at 300ms to coalesce rapid workspace mutations (e.g., adding
    /// multiple folders fires `connect_workspace_changed` for each).
    pub fn rebuild_file_index(&self) {
        let generation = self.imp().index_rebuild_generation.get().wrapping_add(1);
        self.imp().index_rebuild_generation.set(generation);

        let window_weak = self.downgrade();
        glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            if window.imp().index_rebuild_generation.get() != generation {
                return; // superseded by a newer rebuild request
            }
            let prev_count = window.imp().command_palette.file_index_len();
            let roots = window.imp().sidebar.workspace_roots();
            let window_weak = window.downgrade();
            async_task::spawn_blocking_then(
                (),
                move || {
                    if prev_count == 0 {
                        FileIndex::rebuild(&roots)
                    } else {
                        FileIndex::rebuild_with_hint(&roots, prev_count)
                    }
                },
                move |(), index| {
                    if let Some(window) = window_weak.upgrade() {
                        window.imp().command_palette.set_file_index(index);
                    }
                },
            );
        });
    }

    fn setup_shortcuts(&self) {
        let controller = gtk4::ShortcutController::new();
        controller.set_scope(gtk4::ShortcutScope::Managed);

        let shortcuts = [
            ("win.new-tab", "<Control>t"),
            ("win.open-file", "<Control>o"),
            ("win.save", "<Control>s"),
            ("win.save-as", "<Control><Shift>s"),
            ("win.begin-search", "<Control>f"),
            ("win.begin-replace", "<Control>h"),
            ("win.next-match", "<Control>g"),
            ("win.prev-match", "<Control><Shift>g"),
            ("win.close-tab", "<Control>w"),
            ("win.print", "<Control>p"),
            ("win.toggle-command-palette", "<Control><Shift>p"),
            ("win.toggle-search-panel", "<Control><Shift>f"),
            ("win.search-next-match", "F4"),
            ("win.search-prev-match", "<Shift>F4"),
            ("win.toggle-sidebar", "F9"),
            ("win.toggle-preview-mode", "<Alt>p"),
            ("win.toggle-fullscreen", "F11"),
            (
                "win.zoom-in",
                "<Control>equal|<Control>plus|<Control>KP_Add",
            ),
            ("win.zoom-out", "<Control>minus|<Control>KP_Subtract"),
            ("win.zoom-reset", "<Control>0|<Control>KP_0"),
        ];

        for (action, accel) in shortcuts {
            controller.add_shortcut(gtk4::Shortcut::new(
                gtk4::ShortcutTrigger::parse_string(accel),
                Some(gtk4::NamedAction::new(action)),
            ));
        }

        self.add_controller(controller);
    }

    /// Register fullscreen/unfullscreen/toggle-fullscreen actions and wire
    /// the `fullscreened` property to toggle which menu item is visible.
    fn setup_fullscreen(&self) {
        // Menu items use hidden-when: action-disabled, so only the correct
        // one is visible at any time.
        let fullscreen_action = gio::SimpleAction::new("fullscreen", None);
        let unfullscreen_action = gio::SimpleAction::new("unfullscreen", None);
        unfullscreen_action.set_enabled(false);

        {
            let window_weak = self.downgrade();
            fullscreen_action.connect_activate(move |_, _| {
                if let Some(window) = window_weak.upgrade() {
                    window.fullscreen();
                }
            });
        }
        {
            let window_weak = self.downgrade();
            unfullscreen_action.connect_activate(move |_, _| {
                if let Some(window) = window_weak.upgrade() {
                    window.unfullscreen();
                }
            });
        }

        self.add_action(&fullscreen_action);
        self.add_action(&unfullscreen_action);

        // F11 toggle — always enabled, decides direction based on current state.
        self.add_action_entries([gio::ActionEntry::builder("toggle-fullscreen")
            .activate(|window: &Self, _, _| {
                if window.is_fullscreen() {
                    window.unfullscreen();
                } else {
                    window.fullscreen();
                }
            })
            .build()]);

        // Sync action enabled states when fullscreen state changes.
        let fs_action = fullscreen_action;
        let unfs_action = unfullscreen_action;
        self.connect_notify_local(Some("fullscreened"), move |window, _| {
            let is_fs = window.is_fullscreen();
            fs_action.set_enabled(!is_fs);
            unfs_action.set_enabled(is_fs);
        });
    }

    /// Create the theme selector widget (follow-system/light/dark circles)
    /// matching GNOME Text Editor's visual pattern, and insert it into
    /// the hamburger menu's popover as a custom child.
    fn setup_theme_selector(&self) {
        let settings = &self.imp().settings;
        let style_manager = libadwaita::StyleManager::default();

        // Container with the CSS class that targets the custom styling.
        let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        container.add_css_class("theme-selector");
        container.set_hexpand(true);
        container.set_halign(gtk4::Align::Center);

        // GtkCheckButton radio group — CSS hides the radio indicator
        // and styles each button as a 44px colored circle.
        // Order matches GNOME Text Editor: follow (system), light, dark.
        let follow_btn = gtk4::CheckButton::builder()
            .tooltip_text("Follow System Style")
            .halign(gtk4::Align::Center)
            .hexpand(true)
            .focus_on_click(false)
            .build();
        follow_btn.add_css_class("follow");

        let light_btn = gtk4::CheckButton::builder()
            .tooltip_text("Light Style")
            .halign(gtk4::Align::Center)
            .hexpand(true)
            .focus_on_click(false)
            .group(&follow_btn)
            .build();
        light_btn.add_css_class("light");

        let dark_btn = gtk4::CheckButton::builder()
            .tooltip_text("Dark Style")
            .halign(gtk4::Align::Center)
            .hexpand(true)
            .focus_on_click(false)
            .group(&follow_btn)
            .build();
        dark_btn.add_css_class("dark");

        container.append(&follow_btn);
        container.append(&light_btn);
        container.append(&dark_btn);

        // Restore persisted state.
        let scheme = settings.string(keys::COLOR_SCHEME);
        match parse_color_scheme(scheme.as_str()) {
            libadwaita::ColorScheme::ForceLight => light_btn.set_active(true),
            libadwaita::ColorScheme::ForceDark => dark_btn.set_active(true),
            _ => follow_btn.set_active(true),
        }

        // Wire each button to set the color scheme and persist.
        {
            let sm = style_manager.clone();
            let s = settings.clone();
            light_btn.connect_toggled(move |btn| {
                if btn.is_active() {
                    sm.set_color_scheme(libadwaita::ColorScheme::ForceLight);
                    let _ = s.set_string(keys::COLOR_SCHEME, "force-light");
                }
            });
        }
        {
            let sm = style_manager.clone();
            let s = settings.clone();
            follow_btn.connect_toggled(move |btn| {
                if btn.is_active() {
                    sm.set_color_scheme(libadwaita::ColorScheme::Default);
                    let _ = s.set_string(keys::COLOR_SCHEME, "default");
                }
            });
        }
        {
            let sm = style_manager;
            let s = settings.clone();
            dark_btn.connect_toggled(move |btn| {
                if btn.is_active() {
                    sm.set_color_scheme(libadwaita::ColorScheme::ForceDark);
                    let _ = s.set_string(keys::COLOR_SCHEME, "force-dark");
                }
            });
        }

        // Insert the widget into the hamburger menu's popover at the
        // <attribute name="custom">theme</attribute> slot.
        let menu_button = &self.imp().primary_menu_button;
        let Some(popover) = menu_button.popover() else {
            tracing::error!("setup_theme_selector: primary_menu_button has no popover");
            return;
        };
        let Ok(popover_menu) = popover.downcast::<gtk4::PopoverMenu>() else {
            tracing::error!("setup_theme_selector: popover is not a PopoverMenu");
            return;
        };
        if !popover_menu.add_child(&container, "theme") {
            tracing::error!(
                "setup_theme_selector: failed to add theme widget \
                 (missing 'theme' custom slot in menu XML?)"
            );
        }
    }
}
