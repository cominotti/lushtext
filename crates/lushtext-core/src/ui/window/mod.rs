// SPDX-License-Identifier: GPL-3.0-or-later

//! Main application window.

mod dialogs;
mod imp;

pub use imp::clamp_sidebar_position;

use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::status_bar::MessageKind;
use glib::subclass::prelude::ObjectSubclassIsExt;
use glib::Object;
use gtk4::gio;
use gtk4::prelude::*;
use std::path::Path;

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

        let page_weak = page.downgrade();
        editor_page.buffer().connect_modified_changed(move |buf| {
            if let Some(page) = page_weak.upgrade() {
                if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                    let base_title = editor.title();
                    if buf.is_modified() {
                        page.set_title(&format!("{}*", base_title));
                    } else {
                        page.set_title(&base_title);
                    }
                }
            }
        });

        tab_view.set_selected_page(&page);
        self.update_content_stack();
        self.refresh_status_bar();
    }

    /// Create a new untitled tab.
    pub fn new_tab(&self) {
        let editor_page = LushtextEditorPage::new();
        let page = self.imp().tab_view.append(&editor_page);
        page.set_title("Untitled");
        self.imp().tab_view.set_selected_page(&page);
        self.update_content_stack();
        self.refresh_status_bar();
    }

    /// Load a directory tree into the sidebar.
    pub fn load_directory(&self, path: &Path) {
        self.imp().sidebar.load_roots(&[path.to_path_buf()]);
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

    /// Refresh the status bar metadata (encoding, file size) for the active tab.
    fn refresh_status_bar(&self) {
        let status_bar = &self.imp().status_bar;
        match self.active_editor() {
            Some(editor) => {
                status_bar.set_metadata_visible(true);
                status_bar.set_file_size(editor.file_size());
            }
            None => {
                status_bar.set_metadata_visible(false);
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
                .activate(|window: &Self, _, _| window.show_open_folder_dialog())
                .build(),
            gio::ActionEntry::builder("save")
                .activate(|window: &Self, _, _| {
                    if let Some(editor) = window.active_editor() {
                        if editor.file_path().is_none() {
                            window.show_save_as_dialog();
                            return;
                        }
                        match editor.save_file() {
                            Ok(()) => {
                                window
                                    .imp()
                                    .status_bar
                                    .push_message("File saved", MessageKind::Info);
                                window.refresh_status_bar();
                            }
                            Err(e) => {
                                tracing::error!("Failed to save: {}", e);
                                window.imp().status_bar.push_message(
                                    &format!("Save failed: {}", e),
                                    MessageKind::Error,
                                );
                            }
                        }
                    }
                })
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
                        tab_view.close_page(&page);
                    }
                    window.update_content_stack();
                    window.refresh_status_bar();
                })
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
                    // Directory rename: rewrite child paths
                    let updated = new_path.join(suffix);
                    editor.set_file_path(&updated);
                    page.set_title(&editor.title());
                }
            }
        }
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
                    tab_view.close_page(&page);
                }
            }
        }
        self.update_content_stack();
        self.refresh_status_bar();
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
