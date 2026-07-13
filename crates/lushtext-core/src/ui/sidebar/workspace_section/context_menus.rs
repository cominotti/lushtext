// SPDX-License-Identifier: GPL-3.0-or-later

//! Section-owned file-tree and workspace-header context menus.

use super::super::file_tree_item::FileTreeItem;
use super::imp;
use crate::model::workspace::{WorkspaceFolderId, WorkspaceFolderMoveDirection};
use crate::ui::accessibility;
use glib::subclass::prelude::{ObjectSubclassExt, ObjectSubclassIsExt};
use gtk4::gio;
use gtk4::prelude::*;
use gtk4::{self, glib};
use std::path::PathBuf;

/// Row currently targeted by the file-tree context menu.
#[derive(Clone)]
pub(super) struct FileContextTarget {
    /// Path selected when the context menu was opened.
    pub path: PathBuf,
    /// Whether the selected row represents a directory.
    pub is_dir: bool,
    /// Stable id when the selected row is a configured workspace folder.
    pub workspace_folder_id: Option<WorkspaceFolderId>,
    /// Row expander used to swap the visible label for inline rename.
    pub expander: gtk4::TreeExpander,
}

impl FileContextTarget {
    /// Capture the context target from a bound tree row and its model item.
    #[must_use]
    pub(super) fn from_item(
        expander: &gtk4::TreeExpander,
        file_item: &FileTreeItem,
    ) -> Option<Self> {
        Some(Self {
            path: file_item.path()?,
            is_dir: file_item.is_dir(),
            workspace_folder_id: file_item.workspace_folder_id(),
            expander: expander.clone(),
        })
    }
}

/// Return true only for header-background clicks, leaving child buttons to own their actions.
fn click_target_is_header_background(gesture: &gtk4::GestureClick, x: f64, y: f64) -> bool {
    let Some(widget) = gesture.widget() else {
        return false;
    };
    let Some(target) = widget.pick(x, y, gtk4::PickFlags::DEFAULT) else {
        return true;
    };
    target.ancestor(gtk4::Button::static_type()).is_none()
}

/// Walk up the widget tree to find a `TreeExpander` ancestor.
fn find_ancestor_expander(widget: &gtk4::Widget) -> Option<gtk4::TreeExpander> {
    let mut current: Option<gtk4::Widget> = Some(widget.clone());
    while let Some(ref w) = current {
        if let Some(expander) = w.downcast_ref::<gtk4::TreeExpander>() {
            return Some(expander.clone());
        }
        current = w.parent();
    }
    None
}

#[derive(Clone)]
pub(super) struct FileContextMenuWiring {
    /// Reveals a deeply nested directory as the focused tree root.
    focus_folder_action: gio::SimpleAction,
    /// Opens the local-history workflow for the targeted file.
    local_history_action: gio::SimpleAction,
    /// Opens the document-note workflow for the targeted file.
    document_note_action: gio::SimpleAction,
    /// Opens the folder-note workflow for the targeted folder.
    folder_note_action: gio::SimpleAction,
    /// Moves a configured top-level folder one position earlier.
    move_folder_up_action: gio::SimpleAction,
    /// Moves a configured top-level folder one position later.
    move_folder_down_action: gio::SimpleAction,
    /// Removes a configured top-level folder from its workspace.
    remove_folder_action: gio::SimpleAction,
}

/// Static description of one item in the file-navigation context menu.
#[derive(Clone, Copy)]
struct PopoverMenuActionSpec {
    /// Stable item id used for accessible naming and section lookup.
    id: &'static str,
    /// Visible menu item label.
    label: &'static str,
    /// Detailed action name activated by the menu item.
    action: &'static str,
    /// Accessible description for assistive technologies.
    description: &'static str,
}

const FILE_NAV_CONTEXT_MENU_SPECS: &[PopoverMenuActionSpec] = &[
    PopoverMenuActionSpec {
        id: "file-focus-folder",
        label: "Focus Folder",
        action: "section.focus-folder",
        description: "Temporarily show this folder as the root of the workspace tree",
    },
    PopoverMenuActionSpec {
        id: "file-local-history",
        label: "Local History…",
        action: "section.local-history",
        description: "Open local history for this file",
    },
    PopoverMenuActionSpec {
        id: "file-document-note",
        label: "Open Document Note…",
        action: "section.document-note",
        description: "Open the note attached to this document",
    },
];
const FILE_CREATE_CONTEXT_MENU_SPECS: &[PopoverMenuActionSpec] = &[
    PopoverMenuActionSpec {
        id: "file-new-file",
        label: "New File",
        action: "section.new-file",
        description: "Create a new file in this folder",
    },
    PopoverMenuActionSpec {
        id: "file-new-folder",
        label: "New Folder",
        action: "section.new-dir",
        description: "Create a new folder in this folder",
    },
];
const FILE_EDIT_CONTEXT_MENU_SPECS: &[PopoverMenuActionSpec] = &[
    PopoverMenuActionSpec {
        id: "file-rename",
        label: "Rename",
        action: "section.rename",
        description: "Rename the selected file or folder",
    },
    PopoverMenuActionSpec {
        id: "file-delete",
        label: "Delete",
        action: "section.delete",
        description: "Delete the selected file or folder after confirmation",
    },
];
const FOLDER_NOTE_CONTEXT_MENU_SPECS: &[PopoverMenuActionSpec] = &[PopoverMenuActionSpec {
    id: "folder-open-note",
    label: "Open Folder Note…",
    action: "section.folder-note",
    description: "Open the note attached to this workspace folder",
}];
const FOLDER_MEMBERSHIP_CONTEXT_MENU_SPECS: &[PopoverMenuActionSpec] = &[
    PopoverMenuActionSpec {
        id: "folder-move-up",
        label: "Move Up",
        action: "section.move-folder-up",
        description: "Move this folder earlier in the workspace",
    },
    PopoverMenuActionSpec {
        id: "folder-move-down",
        label: "Move Down",
        action: "section.move-folder-down",
        description: "Move this folder later in the workspace",
    },
    PopoverMenuActionSpec {
        id: "folder-remove",
        label: "Remove from Workspace",
        action: "section.remove-folder",
        description: "Remove this folder from the workspace without deleting it from disk",
    },
];
const HEADER_CONTEXT_MENU_SPECS: &[PopoverMenuActionSpec] = &[
    PopoverMenuActionSpec {
        id: "header-add-folder",
        label: "Add Folder…",
        action: "ws-header.add-folder",
        description: "Add a folder to this workspace",
    },
    PopoverMenuActionSpec {
        id: "header-open-folder-note",
        label: "Open Folder Note…",
        action: "ws-header.open-folder-note",
        description: "Open the note attached to this workspace",
    },
    PopoverMenuActionSpec {
        id: "header-rename",
        label: "Rename Workspace",
        action: "ws-header.rename",
        description: "Rename this workspace",
    },
    PopoverMenuActionSpec {
        id: "header-remove",
        label: "Remove Workspace",
        action: "ws-header.unlist",
        description: "Remove this workspace after confirmation",
    },
];

fn rebuild_popover_action_menu(
    popover: &gtk4::Popover,
    menu_box: &gtk4::Box,
    groups: &[&[PopoverMenuActionSpec]],
) {
    while let Some(child) = menu_box.first_child() {
        menu_box.remove(&child);
    }

    for (index, specs) in groups.iter().enumerate() {
        if index > 0 {
            let separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
            separator.set_margin_top(4);
            separator.set_margin_bottom(4);
            menu_box.append(&separator);
        }

        for spec in *specs {
            menu_box.append(&popover_action_button(popover, spec));
        }
    }
}

fn popover_action_button(popover: &gtk4::Popover, spec: &PopoverMenuActionSpec) -> gtk4::Button {
    let button = gtk4::Button::with_label(spec.label);
    button.add_css_class("flat");
    button.add_css_class("model");
    button.set_action_name(Some(spec.action));
    button.set_halign(gtk4::Align::Fill);
    button.set_hexpand(true);
    button.set_widget_name(spec.id);
    accessibility::set_role(&button, gtk4::AccessibleRole::MenuItem);
    accessibility::set_labelled_description(&button, spec.label, spec.description);

    let popover_weak = popover.downgrade();
    button.connect_clicked(move |_| {
        let popover_weak = popover_weak.clone();
        glib::idle_add_local_once(move || {
            if let Some(popover) = popover_weak.upgrade() {
                popover.popdown();
            }
        });
    });

    button
}

fn popdown_context_popovers(section: &super::LushtextWorkspaceSection) {
    if let Some(popover) = section.imp().context_menu.borrow().as_ref() {
        popover.popdown();
    }
    if let Some(popover) = section.imp().header_context_menu.borrow().as_ref() {
        popover.popdown();
    }
}

fn file_tree_context_menu_key(key: gtk4::gdk::Key, state: gtk4::gdk::ModifierType) -> bool {
    key == gtk4::gdk::Key::Menu
        || (key == gtk4::gdk::Key::F10 && state.contains(gtk4::gdk::ModifierType::SHIFT_MASK))
}

fn show_file_context_menu_for_selection(
    section: &super::LushtextWorkspaceSection,
    wiring: &FileContextMenuWiring,
) -> bool {
    let Some(selection) = section
        .imp()
        .file_tree_view
        .model()
        .and_downcast::<gtk4::SingleSelection>()
    else {
        return false;
    };
    if selection.selected() == gtk4::INVALID_LIST_POSITION {
        return false;
    }
    let Some(tree_row) = selection
        .selected_item()
        .and_downcast::<gtk4::TreeListRow>()
    else {
        return false;
    };
    let Some(file_item) = tree_row.item().and_downcast::<FileTreeItem>() else {
        return false;
    };
    let Some((expander, pointing_to)) =
        realized_expander_and_bounds_for_tree_row(section, &tree_row)
    else {
        section.imp().file_tree_view.scroll_to(
            selection.selected(),
            gtk4::ListScrollFlags::FOCUS,
            None,
        );
        return false;
    };

    show_file_context_menu_for_row(
        section,
        &expander,
        &tree_row,
        &file_item,
        wiring,
        pointing_to,
    )
}

impl super::LushtextWorkspaceSection {
    /// Open the file-tree context menu for the current selection.
    ///
    /// This reuses the same menu wiring as pointer and keyboard handlers so
    /// automation-opened menus stay behaviorally identical to user-opened ones.
    pub(in crate::ui::sidebar) fn show_selected_file_context_menu(&self) -> bool {
        let Some(wiring) = self.imp().context_menu_wiring.borrow().clone() else {
            return false;
        };
        show_file_context_menu_for_selection(self, &wiring)
    }

    /// Open the workspace-header context menu at the header bounds.
    ///
    /// The header can be focused through a child button, but automation also
    /// needs a direct menu-open path when synthetic key delivery is unavailable.
    pub(in crate::ui::sidebar) fn show_header_context_menu(&self) -> bool {
        let imp = self.imp();
        let Some(popover) = imp.header_context_menu.borrow().clone() else {
            return false;
        };
        popover.set_pointing_to(Some(&gdk4::Rectangle::new(
            0,
            0,
            imp.header_box.width().max(1),
            imp.header_box.height().max(1),
        )));
        popover.popup();
        true
    }
}

fn realized_expander_and_bounds_for_tree_row(
    section: &super::LushtextWorkspaceSection,
    target_row: &gtk4::TreeListRow,
) -> Option<(gtk4::TreeExpander, gdk4::Rectangle)> {
    let list_view = section.imp().file_tree_view.clone();
    let mut child = list_view.first_child();
    while let Some(row_widget) = child {
        let next = row_widget.next_sibling();
        if let Some(overlay) = row_widget.first_child().and_downcast::<gtk4::Overlay>()
            && let Some(expander) = overlay.child().and_downcast::<gtk4::TreeExpander>()
            && expander.list_row().as_ref() == Some(target_row)
            && let Some(bounds) = row_widget.compute_bounds(&list_view)
        {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "Popover anchor geometry comes from GTK allocation data that already lives in i32 widget coordinates"
            )]
            let pointing_to = gdk4::Rectangle::new(
                bounds.x().round() as i32,
                bounds.y().round() as i32,
                bounds.width().max(1.0).round() as i32,
                bounds.height().max(1.0).round() as i32,
            );
            return Some((expander, pointing_to));
        }
        child = next;
    }
    None
}

fn show_file_context_menu_for_row(
    section: &super::LushtextWorkspaceSection,
    expander: &gtk4::TreeExpander,
    tree_row: &gtk4::TreeListRow,
    file_item: &FileTreeItem,
    wiring: &FileContextMenuWiring,
    pointing_to: gdk4::Rectangle,
) -> bool {
    let Some(path) = file_item.path() else {
        return false;
    };

    let imp = section.imp();
    let workspace_folder_id = file_item.workspace_folder_id();
    *imp.context_target.borrow_mut() = Some(FileContextTarget {
        path,
        is_dir: file_item.is_dir(),
        workspace_folder_id: workspace_folder_id.clone(),
        expander: expander.clone(),
    });

    let is_workspace_folder = workspace_folder_id.is_some();
    wiring
        .focus_folder_action
        .set_enabled(file_item.is_dir() && !file_item.is_placeholder() && tree_row.depth() > 0);
    // Avoid filesystem metadata checks on the context-menu path; the
    // window-level local-history workflow validates file size on activation
    // and reports a warning if the file is too large.
    let local_history_enabled = !file_item.is_dir() && !file_item.is_placeholder();
    wiring
        .local_history_action
        .set_enabled(local_history_enabled);
    wiring
        .document_note_action
        .set_enabled(!file_item.is_dir() && !file_item.is_placeholder());
    wiring.folder_note_action.set_enabled(is_workspace_folder);
    wiring.remove_folder_action.set_enabled(is_workspace_folder);
    let (can_move_up, can_move_down) = workspace_folder_id
        .as_ref()
        .map_or((false, false), |folder_id| {
            section.workspace_folder_move_availability(folder_id)
        });
    wiring.move_folder_up_action.set_enabled(can_move_up);
    wiring.move_folder_down_action.set_enabled(can_move_down);

    let popover = imp.context_menu.borrow().clone();
    let menu_box = imp.context_menu_box.borrow().clone();
    if let (Some(popover), Some(menu_box)) = (popover, menu_box) {
        let item_kind = if is_workspace_folder {
            "Workspace folder"
        } else if file_item.is_dir() {
            "Folder"
        } else {
            "File"
        };
        let display_name = file_item.name();
        accessibility::set_labelled_description(
            &popover,
            &format!("{item_kind} actions for {display_name}"),
            "Context actions for the selected workspace file-tree row",
        );
        if is_workspace_folder {
            rebuild_popover_action_menu(
                &popover,
                &menu_box,
                &[
                    FOLDER_NOTE_CONTEXT_MENU_SPECS,
                    FOLDER_MEMBERSHIP_CONTEXT_MENU_SPECS,
                    FILE_CREATE_CONTEXT_MENU_SPECS,
                ],
            );
        } else {
            rebuild_popover_action_menu(
                &popover,
                &menu_box,
                &[
                    FILE_NAV_CONTEXT_MENU_SPECS,
                    FILE_CREATE_CONTEXT_MENU_SPECS,
                    FILE_EDIT_CONTEXT_MENU_SPECS,
                ],
            );
        }
        popover.set_pointing_to(Some(&pointing_to));
        popover.popup();
        return true;
    }
    false
}

/// Build the right-click context menu for file/directory items.
pub(super) fn setup_file_context_menu(imp: &imp::LushtextWorkspaceSection) {
    let obj = imp.obj();

    let popover = gtk4::Popover::new();
    popover.set_parent(&*imp.file_tree_view);
    popover.set_has_arrow(false);
    popover.set_halign(gtk4::Align::Start);
    let menu_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    menu_box.add_css_class("context-menu");
    popover.set_child(Some(&menu_box));
    accessibility::set_role(&popover, gtk4::AccessibleRole::Menu);
    accessibility::set_labelled_description(
        &popover,
        "File tree context menu",
        "Actions for the selected file or folder row",
    );
    *imp.context_menu_box.borrow_mut() = Some(menu_box);
    *imp.context_menu.borrow_mut() = Some(popover);

    // Register actions under the "section" prefix. Context menu items
    // reference them as "section.new-file", "section.rename", etc.
    // GTK resolves these by walking up the widget tree for the prefix.
    let action_group = gio::SimpleActionGroup::new();

    let focus_folder_action = gio::SimpleAction::new("focus-folder", None);
    let section_weak = obj.downgrade();
    focus_folder_action.connect_activate(move |_, _| {
        let Some(section) = section_weak.upgrade() else {
            return;
        };
        let target = section.imp().context_target.borrow().clone();
        if let Some(target) = target
            && target.is_dir
        {
            popdown_context_popovers(&section);
            section.imp().context_target.borrow_mut().take();
            section.focus_folder(&target.path);
        }
    });
    action_group.add_action(&focus_folder_action);

    let local_history_action = gio::SimpleAction::new("local-history", None);
    let section_weak = obj.downgrade();
    local_history_action.connect_activate(move |_, _| {
        if let Some(section) = section_weak.upgrade()
            && let Some(target) = section.imp().context_target.borrow().clone()
            && !target.is_dir
        {
            popdown_context_popovers(&section);
            section.notify_local_history_requested(&target.path);
        }
    });
    action_group.add_action(&local_history_action);

    let document_note_action = gio::SimpleAction::new("document-note", None);
    let section_weak = obj.downgrade();
    document_note_action.connect_activate(move |_, _| {
        if let Some(section) = section_weak.upgrade()
            && let Some(target) = section.imp().context_target.borrow().clone()
            && !target.is_dir
        {
            popdown_context_popovers(&section);
            section.notify_document_note_requested(&target.path);
        }
    });
    action_group.add_action(&document_note_action);

    let folder_note_action = gio::SimpleAction::new("folder-note", None);
    let section_weak = obj.downgrade();
    folder_note_action.connect_activate(move |_, _| {
        if let Some(section) = section_weak.upgrade()
            && let Some(target) = section.imp().context_target.borrow().clone()
            && target.workspace_folder_id.is_some()
        {
            popdown_context_popovers(&section);
            section.notify_folder_note_for_folder_requested(&target.path);
        }
    });
    action_group.add_action(&folder_note_action);

    let move_folder_up_action = gio::SimpleAction::new("move-folder-up", None);
    let section_weak = obj.downgrade();
    move_folder_up_action.connect_activate(move |_, _| {
        if let Some(section) = section_weak.upgrade()
            && let Some(target) = section.imp().context_target.borrow().clone()
            && let Some(folder_id) = target.workspace_folder_id
        {
            popdown_context_popovers(&section);
            section.notify_reorder_folder_requested(&folder_id, WorkspaceFolderMoveDirection::Up);
        }
    });
    action_group.add_action(&move_folder_up_action);

    let move_folder_down_action = gio::SimpleAction::new("move-folder-down", None);
    let section_weak = obj.downgrade();
    move_folder_down_action.connect_activate(move |_, _| {
        if let Some(section) = section_weak.upgrade()
            && let Some(target) = section.imp().context_target.borrow().clone()
            && let Some(folder_id) = target.workspace_folder_id
        {
            popdown_context_popovers(&section);
            section.notify_reorder_folder_requested(&folder_id, WorkspaceFolderMoveDirection::Down);
        }
    });
    action_group.add_action(&move_folder_down_action);

    let remove_folder_action = gio::SimpleAction::new("remove-folder", None);
    let section_weak = obj.downgrade();
    remove_folder_action.connect_activate(move |_, _| {
        if let Some(section) = section_weak.upgrade() {
            popdown_context_popovers(&section);
            section.show_remove_folder_confirmation();
        }
    });
    action_group.add_action(&remove_folder_action);

    let new_file_action = gio::SimpleAction::new("new-file", None);
    let section_weak = obj.downgrade();
    new_file_action.connect_activate(move |_, _| {
        if let Some(section) = section_weak.upgrade() {
            popdown_context_popovers(&section);
            section.create_new_item(false);
        }
    });
    action_group.add_action(&new_file_action);

    let new_dir_action = gio::SimpleAction::new("new-dir", None);
    let section_weak = obj.downgrade();
    new_dir_action.connect_activate(move |_, _| {
        if let Some(section) = section_weak.upgrade() {
            popdown_context_popovers(&section);
            section.create_new_item(true);
        }
    });
    action_group.add_action(&new_dir_action);

    let rename_action = gio::SimpleAction::new("rename", None);
    let section_weak = obj.downgrade();
    rename_action.connect_activate(move |_, _| {
        if let Some(section) = section_weak.upgrade() {
            popdown_context_popovers(&section);
            section.begin_rename();
        }
    });
    action_group.add_action(&rename_action);

    let delete_action = gio::SimpleAction::new("delete", None);
    let section_weak = obj.downgrade();
    delete_action.connect_activate(move |_, _| {
        if let Some(section) = section_weak.upgrade() {
            popdown_context_popovers(&section);
            section.show_delete_confirmation();
        }
    });
    action_group.add_action(&delete_action);

    obj.insert_action_group("section", Some(&action_group));

    let context_menu_wiring = FileContextMenuWiring {
        focus_folder_action,
        local_history_action,
        document_note_action,
        folder_note_action,
        move_folder_up_action,
        move_folder_down_action,
        remove_folder_action,
    };
    *imp.context_menu_wiring.borrow_mut() = Some(context_menu_wiring.clone());

    // Attach the gesture to the stable list view; press-time picking
    // resolves the current recycled row before opening the menu.
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(3);

    let section_weak = obj.downgrade();
    let pointer_wiring = context_menu_wiring.clone();
    gesture.connect_pressed(move |gesture, _n_press, x, y| {
        let Some(section) = section_weak.upgrade() else {
            return;
        };
        let Some(list_view) = gesture.widget() else {
            return;
        };

        let Some(picked) = list_view.pick(x, y, gtk4::PickFlags::DEFAULT) else {
            return;
        };
        let Some(expander) = find_ancestor_expander(&picked) else {
            return;
        };
        let Some(tree_row) = expander.list_row() else {
            return;
        };
        let Some(file_item) = tree_row.item().and_downcast::<FileTreeItem>() else {
            return;
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Pointer event coordinates are already bounded by GTK widget geometry before converting to i32"
        )]
        let pointing_to = gdk4::Rectangle::new(x as i32, y as i32, 1, 1);
        show_file_context_menu_for_row(
            &section,
            &expander,
            &tree_row,
            &file_item,
            &pointer_wiring,
            pointing_to,
        );
    });

    imp.file_tree_view.add_controller(gesture);

    let key_controller = gtk4::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let section_weak = obj.downgrade();
    let keyboard_wiring = context_menu_wiring;
    key_controller.connect_key_pressed(move |_, key, _, state| {
        if !file_tree_context_menu_key(key, state) {
            return glib::Propagation::Proceed;
        }
        let Some(section) = section_weak.upgrade() else {
            return glib::Propagation::Proceed;
        };
        if show_file_context_menu_for_selection(&section, &keyboard_wiring) {
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    imp.file_tree_view.add_controller(key_controller);
}

/// Build right-click context menu for the workspace header.
pub(super) fn setup_header_context_menu(imp: &imp::LushtextWorkspaceSection) {
    let obj = imp.obj();

    let popover = gtk4::Popover::new();
    popover.set_parent(&*imp.header_box);
    popover.set_has_arrow(false);
    popover.set_halign(gtk4::Align::Start);
    let menu_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    menu_box.add_css_class("context-menu");
    rebuild_popover_action_menu(&popover, &menu_box, &[HEADER_CONTEXT_MENU_SPECS]);
    popover.set_child(Some(&menu_box));
    accessibility::set_role(&popover, gtk4::AccessibleRole::Menu);
    accessibility::set_labelled_description(
        &popover,
        "Workspace context menu",
        "Actions for this workspace section",
    );
    *imp.header_context_menu_box.borrow_mut() = Some(menu_box);
    *imp.header_context_menu.borrow_mut() = Some(popover.clone());

    let action_group = gio::SimpleActionGroup::new();

    let folder_note_action = gio::SimpleAction::new("open-folder-note", None);
    let section_weak = obj.downgrade();
    folder_note_action.connect_activate(move |_, _| {
        if let Some(section) = section_weak.upgrade() {
            popdown_context_popovers(&section);
            section.notify_folder_note_requested();
        }
    });
    action_group.add_action(&folder_note_action);

    let add_folder_action = gio::SimpleAction::new("add-folder", None);
    let section_weak = obj.downgrade();
    add_folder_action.connect_activate(move |_, _| {
        if let Some(section) = section_weak.upgrade() {
            popdown_context_popovers(&section);
            section.notify_add_folder_requested();
        }
    });
    action_group.add_action(&add_folder_action);

    let rename_action = gio::SimpleAction::new("rename", None);
    let section_weak = obj.downgrade();
    rename_action.connect_activate(move |_, _| {
        if let Some(section) = section_weak.upgrade() {
            popdown_context_popovers(&section);
            section.notify_rename_workspace_requested();
        }
    });
    action_group.add_action(&rename_action);

    let unlist_action = gio::SimpleAction::new("unlist", None);
    let section_weak = obj.downgrade();
    unlist_action.connect_activate(move |_, _| {
        if let Some(section) = section_weak.upgrade() {
            popdown_context_popovers(&section);
            section.notify_unlist_workspace_requested();
        }
    });
    action_group.add_action(&unlist_action);

    obj.insert_action_group("ws-header", Some(&action_group));

    // Right-click gesture on the header box
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(3);

    let popover_ref = popover.clone();
    gesture.connect_pressed(move |_gesture, _n_press, x, y| {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Pointer event coordinates are already bounded by GTK widget geometry before converting to i32"
        )]
        popover_ref.set_pointing_to(Some(&gdk4::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover_ref.popup();
    });

    imp.header_box.add_controller(gesture);

    let key_controller = gtk4::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let popover_ref = popover;
    key_controller.connect_key_pressed(move |controller, key, _, state| {
        if !file_tree_context_menu_key(key, state) {
            return glib::Propagation::Proceed;
        }
        let Some(header_box) = controller.widget() else {
            return glib::Propagation::Proceed;
        };
        popover_ref.set_pointing_to(Some(&gdk4::Rectangle::new(
            0,
            0,
            header_box.width().max(1),
            header_box.height().max(1),
        )));
        popover_ref.popup();
        glib::Propagation::Stop
    });
    imp.header_box.add_controller(key_controller);
}

/// Set up double-click gesture on the workspace header to collapse/expand the section body.
pub(super) fn setup_header_double_click(imp: &imp::LushtextWorkspaceSection) {
    let obj = imp.obj();
    let gesture = gtk4::GestureClick::new();

    let section_weak = obj.downgrade();
    gesture.connect_pressed(move |gesture, n_press, x, y| {
        if n_press == 2
            && click_target_is_header_background(gesture, x, y)
            && let Some(section) = section_weak.upgrade()
        {
            section.toggle_section_body_collapsed();
        }
    });

    imp.header_box.add_controller(gesture);
}
