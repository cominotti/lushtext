// SPDX-License-Identifier: GPL-3.0-or-later

//! Main application window.

mod imp;

use crate::services;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::sidebar::file_tree_item::FileTreeItem;
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
        let window: Self = Object::builder()
            .property("application", app)
            .build();
        window.setup_actions();
        window.setup_shortcuts();
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
        let title = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());

        let page = tab_view.append(&editor_page);
        page.set_title(&title);

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

        editor_page.load_file_async(path);
        tab_view.set_selected_page(&page);
        self.update_content_stack();
    }

    /// Create a new untitled tab.
    pub fn new_tab(&self) {
        let editor_page = LushtextEditorPage::new();
        let page = self.imp().tab_view.append(&editor_page);
        page.set_title("Untitled");
        self.imp().tab_view.set_selected_page(&page);
        self.update_content_stack();
    }

    /// Load a directory tree into the sidebar.
    pub fn load_directory(&self, path: &Path) {
        let root_model = services::file_tree::build_root_model(&[path.to_path_buf()]);
        let tree_model = gtk4::TreeListModel::new(root_model, false, false, |item| {
            item.downcast_ref::<FileTreeItem>()
                .filter(|fi| fi.is_dir())
                .map(|fi| services::file_tree::build_children_model(&fi.path()))
                .map(|m| m.upcast::<gio::ListModel>())
        });
        self.imp().sidebar.set_model(&tree_model);
    }

    /// Switch the content stack between "tabs" and "empty" states.
    fn update_content_stack(&self) {
        let stack = &self.imp().content_stack;
        if self.imp().tab_view.n_pages() > 0 {
            stack.set_visible_child_name("tabs");
        } else {
            stack.set_visible_child_name("empty");
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
        // New tab
        let action_new_tab = gio::ActionEntry::builder("new-tab")
            .activate(|window: &Self, _, _| window.new_tab())
            .build();

        // Open file
        let action_open = gio::ActionEntry::builder("open-file")
            .activate(|window: &Self, _, _| window.show_open_file_dialog())
            .build();

        // Open folder
        let action_open_folder = gio::ActionEntry::builder("open-folder")
            .activate(|window: &Self, _, _| window.show_open_folder_dialog())
            .build();

        // Save
        let action_save = gio::ActionEntry::builder("save")
            .activate(|window: &Self, _, _| {
                if let Some(editor) = window.active_editor() {
                    if let Err(e) = editor.save_file() {
                        tracing::error!("Failed to save: {}", e);
                    }
                }
            })
            .build();

        // Toggle search
        let action_search = gio::ActionEntry::builder("toggle-search")
            .activate(|window: &Self, _, _| {
                if let Some(editor) = window.active_editor() {
                    editor.toggle_search();
                }
            })
            .build();

        // Close tab
        let action_close_tab = gio::ActionEntry::builder("close-tab")
            .activate(|window: &Self, _, _| {
                let tab_view = &window.imp().tab_view;
                if let Some(page) = tab_view.selected_page() {
                    tab_view.close_page(&page);
                }
                window.update_content_stack();
            })
            .build();

        self.add_action_entries([
            action_new_tab,
            action_open,
            action_open_folder,
            action_save,
            action_search,
            action_close_tab,
        ]);
    }

    fn setup_shortcuts(&self) {
        let controller = gtk4::ShortcutController::new();
        controller.set_scope(gtk4::ShortcutScope::Managed);

        let shortcuts = [
            ("win.new-tab", "<Control>t"),
            ("win.open-file", "<Control>o"),
            ("win.save", "<Control>s"),
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

    fn show_open_file_dialog(&self) {
        let dialog = gtk4::FileDialog::builder()
            .title("Open File")
            .modal(true)
            .build();

        let window = self.clone();
        dialog.open(Some(self), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    window.open_document(&path);
                }
            }
        });
    }

    fn show_open_folder_dialog(&self) {
        let dialog = gtk4::FileDialog::builder()
            .title("Open Folder")
            .modal(true)
            .build();

        let window = self.clone();
        dialog.select_folder(Some(self), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    window.load_directory(&path);
                    window.imp().sidebar.set_workspace_name(
                        path.file_name()
                            .map(|n| n.to_string_lossy())
                            .as_deref()
                            .unwrap_or("workspace"),
                    );
                }
            }
        });
    }
}
