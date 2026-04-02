// SPDX-License-Identifier: GPL-3.0-or-later

//! Main application window.

mod dialogs;
mod imp;

pub use imp::clamp_sidebar_position;

use crate::services::async_task;
use crate::services::palette::FileIndex;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::status_bar::MessageKind;
use glib::subclass::prelude::ObjectSubclassIsExt;
use glib::Object;
use gtk4::gio;
use gtk4::prelude::*;
use std::path::Path;

/// Maximum total estimated buffer memory across all tabs before evicting
/// unmodified background tabs. ~256MB is comfortable on 8GB machines.
const BUFFER_MEMORY_BUDGET: u64 = 256_000_000;

glib::wrapper! {
    pub struct LushtextWindow(ObjectSubclass<imp::LushtextWindow>)
        @extends libadwaita::ApplicationWindow, gtk4::ApplicationWindow, gtk4::Window, gtk4::Widget,
        @implements gio::ActionMap, gio::ActionGroup, gtk4::Accessible, gtk4::Buildable,
                    gtk4::ConstraintTarget, gtk4::Native, gtk4::Root, gtk4::ShortcutManager;
}

impl LushtextWindow {
    pub fn new(app: &libadwaita::Application) -> Self {
        let window: Self = Object::builder().property("application", app).build();
        window.setup_actions();
        window.setup_shortcuts();
        window.update_content_stack();
        window.refresh_status_bar();
        window
    }

    /// Open a file in a new tab, or focus existing tab if already open.
    /// The tab appears immediately; file content loads asynchronously.
    pub fn open_document(&self, path: &Path) {
        let tab_view = &self.imp().tab_view;
        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                if editor.file_path().as_deref() == Some(path) {
                    tab_view.set_selected_page(&page);
                    return;
                }
            }
        }

        let editor_page = LushtextEditorPage::new();
        editor_page.load_file_async(path);

        let page = tab_view.append(&editor_page);
        page.set_title(&editor_page.title());
        self.wire_modified_indicator(&page, &editor_page);

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
        editor.save_file_async(move |result| match result {
            Ok(()) => {
                window
                    .imp()
                    .status_bar
                    .push_message("File saved", MessageKind::Info);
                window.refresh_status_bar();
            }
            Err(e) => {
                tracing::error!("Failed to save: {}", e);
                window
                    .imp()
                    .status_bar
                    .push_message(&format!("Save failed: {e}"), MessageKind::Error);
            }
        });
    }

    /// Create a new untitled tab.
    pub fn new_tab(&self) {
        let editor_page = LushtextEditorPage::new();
        let page = self.imp().tab_view.append(&editor_page);
        page.set_title("Untitled");
        self.wire_modified_indicator(&page, &editor_page);
        self.imp().tab_view.set_selected_page(&page);
        self.update_content_stack();
        self.refresh_status_bar();
    }

    /// Connect a buffer's modified-changed signal to update the tab title
    /// and header bar. Prepends "● " to the tab title when the buffer has
    /// unsaved changes, placing the dot immediately before the filename.
    fn wire_modified_indicator(&self, page: &libadwaita::TabPage, editor: &LushtextEditorPage) {
        let page_weak = page.downgrade();
        let window_weak = self.downgrade();
        editor.buffer().connect_modified_changed(move |buf| {
            if let Some(page) = page_weak.upgrade() {
                if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                    let name = editor.title();
                    if buf.is_modified() {
                        page.set_title(&format!("• {name}"));
                    } else {
                        page.set_title(&name);
                    }
                }
            }
            // Only refresh header bar if this is the active tab
            if let (Some(window), Some(page)) = (window_weak.upgrade(), page_weak.upgrade()) {
                if window.imp().tab_view.selected_page().as_ref() == Some(&page) {
                    window.refresh_header_bar();
                }
            }
        });
    }

    /// Switch the content stack between "tabs" and "empty" states,
    /// and enable/disable actions that require an active tab.
    fn update_content_stack(&self) {
        let has_tabs = self.imp().tab_view.n_pages() > 0;
        let stack = &self.imp().content_stack;
        if has_tabs {
            stack.set_visible_child_name("tabs");
        } else {
            stack.set_visible_child_name("empty");
        }

        for name in ["toggle-search", "save", "save-as", "close-tab"] {
            if let Some(action) = self.lookup_action(name) {
                if let Some(simple) = action.downcast_ref::<gio::SimpleAction>() {
                    simple.set_enabled(has_tabs);
                }
            }
        }
    }

    /// Refresh the status bar and header bar for the active tab.
    /// Single `active_editor()` lookup shared by both updates.
    fn refresh_status_bar(&self) {
        let imp = self.imp();
        let editor = self.active_editor();
        // Status bar
        match &editor {
            Some(e) => {
                imp.status_bar.set_metadata_visible(true);
                imp.status_bar.set_file_size(e.file_size());
            }
            None => {
                imp.status_bar.set_metadata_visible(false);
            }
        }
        // Header bar title/subtitle + modified dot
        self.refresh_header_bar_with(editor.as_ref());
    }

    /// Update the header bar title/subtitle to reflect the given editor.
    /// Prepends "● " to the title when the buffer has unsaved changes.
    /// Reverts to "LushText" with no subtitle when no editor is active.
    fn refresh_header_bar(&self) {
        self.refresh_header_bar_with(self.active_editor().as_ref());
    }

    fn refresh_header_bar_with(&self, editor: Option<&LushtextEditorPage>) {
        let title_widget = &self.imp().title_widget;
        match editor {
            Some(editor) => {
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
            }
            None => {
                title_widget.set_title("LushText");
                title_widget.set_subtitle("");
            }
        }
    }

    /// Get the currently active editor page, if any.
    fn active_editor(&self) -> Option<LushtextEditorPage> {
        self.imp()
            .tab_view
            .selected_page()
            .and_then(|page| page.child().downcast::<LushtextEditorPage>().ok())
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
            gio::ActionEntry::builder("toggle-search")
                .activate(|window: &Self, _, _| {
                    if let Some(editor) = window.active_editor() {
                        editor.toggle_search();
                    }
                })
                .build(),
            gio::ActionEntry::builder("close-tab")
                .activate(|window: &Self, _, _| {
                    let tab_view = &window.imp().tab_view;
                    if let Some(page) = tab_view.selected_page() {
                        if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                            editor.cancel_load();
                        }
                        tab_view.close_page(&page);
                    }
                    window.update_content_stack();
                    window.refresh_status_bar();
                })
                .build(),
            gio::ActionEntry::builder("toggle-command-palette")
                .activate(|window: &Self, _, _| window.toggle_command_palette())
                .build(),
        ]);
    }

    /// Update the file path and title for any tab matching `old_path`.
    /// For directory renames, rewrites paths of all files inside the directory.
    pub fn update_tab_path(&self, old_path: &Path, new_path: &Path) {
        let tab_view = &self.imp().tab_view;
        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                let Some(ref ep) = editor.file_path() else {
                    continue;
                };
                if ep.as_path() == old_path {
                    editor.set_file_path(new_path);
                    page.set_title(&editor.title());
                } else if let Ok(suffix) = ep.strip_prefix(old_path) {
                    let updated = new_path.join(suffix);
                    editor.set_file_path(&updated);
                    page.set_title(&editor.title());
                }
            }
        }
        self.refresh_header_bar();
    }

    /// Close any tab whose file path matches `path` or is inside it (for directories).
    pub fn close_tab_for_path(&self, path: &Path) {
        let tab_view = &self.imp().tab_view;
        // Iterate in reverse so removing pages doesn't shift indices
        for i in (0..tab_view.n_pages()).rev() {
            let page = tab_view.nth_page(i);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                let matches = editor
                    .file_path()
                    .as_ref()
                    .is_some_and(|p| p.as_path() == path || p.starts_with(path));
                if matches {
                    editor.cancel_load();
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
            imp.palette_revealer.set_reveal_child(true);
            imp.command_palette.open();
        }
    }

    fn close_command_palette(&self) {
        let imp = self.imp();
        imp.command_palette.close();
        imp.palette_revealer.set_reveal_child(false);
    }

    /// If the active tab was evicted, reload its content from disk.
    fn reload_if_evicted(&self) {
        let Some(editor) = self.active_editor() else {
            return;
        };
        if !editor.is_evicted() {
            return;
        }
        let Some(path) = editor.file_path() else {
            return;
        };
        editor.load_file_async(&path);
    }

    /// Evict unmodified background tabs when total buffer memory exceeds the budget.
    /// Computes a running total inline to avoid O(n²) rescanning after each eviction.
    fn maybe_evict_background_tabs(&self) {
        let tab_view = &self.imp().tab_view;
        let selected = tab_view.selected_page();

        let mut total = 0u64;
        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                if !editor.is_evicted() {
                    if let Some(size) = editor.file_size() {
                        total = total.saturating_add(size);
                    }
                }
            }
        }

        if total <= BUFFER_MEMORY_BUDGET {
            return;
        }

        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            if selected.as_ref() == Some(&page) {
                continue;
            }
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                if !editor.is_modified() && !editor.is_evicted() && editor.file_path().is_some() {
                    let evicted_size = editor.file_size().unwrap_or(0);
                    tracing::info!("Evicting tab to free memory: {}", editor.title());
                    editor.evict();
                    total = total.saturating_sub(evicted_size);
                    if total <= BUFFER_MEMORY_BUDGET {
                        break;
                    }
                }
            }
        }
    }

    /// Build the file index from all workspace roots on a background thread.
    /// Debounced at 300ms to coalesce rapid workspace mutations (e.g., adding
    /// multiple folders fires `connect_workspace_changed` for each).
    pub fn rebuild_file_index(&self) {
        let gen = self.imp().index_rebuild_generation.get().wrapping_add(1);
        self.imp().index_rebuild_generation.set(gen);

        let window_weak = self.downgrade();
        glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            if window.imp().index_rebuild_generation.get() != gen {
                return; // superseded by a newer rebuild request
            }
            let roots = window.imp().sidebar.workspace_roots();
            let window_weak = window.downgrade();
            async_task::spawn_blocking_then(
                (),
                move || FileIndex::rebuild(&roots),
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
            ("win.toggle-search", "<Control>f"),
            ("win.close-tab", "<Control>w"),
            ("win.toggle-command-palette", "<Control>p"),
        ];

        for (action, accel) in shortcuts {
            controller.add_shortcut(gtk4::Shortcut::new(
                gtk4::ShortcutTrigger::parse_string(accel),
                Some(gtk4::NamedAction::new(action)),
            ));
        }

        self.add_controller(controller);
    }
}
