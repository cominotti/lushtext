// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the workspace section widget.
//!
//! Each section manages one workspace's file tree (GtkListView + TreeListModel),
//! context menus for files and the workspace header, and callback forwarding
//! to the parent sidebar.

use super::super::file_tree_item::FileTreeItem;
use crate::model::workspace::{WorkspaceEntry, WorkspaceId};
use gtk4::gio;
use gtk4::gio::prelude::ListModelExt;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, glib};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

type FileCallback = Box<dyn Fn(&Path)>;
type RenameCallback = Box<dyn Fn(&Path, &Path)>;
type WorkspaceCallback = Box<dyn Fn(&WorkspaceId)>;

/// Cached position of a file tree item for O(1) model removal.
/// Stores the parent directory (or `None` for root items) and the
/// index within the parent's `ListStore`.
#[derive(Debug, Clone)]
pub struct ItemLocation {
    pub parent_dir: Option<PathBuf>,
    pub index: usize,
}

// CompositeTemplate loads the UI layout from a compiled XML file.
// GObject methods always take &self; Cell/RefCell provide interior mutability.
#[derive(Default, CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/workspace-section.ui")]
pub struct LushtextWorkspaceSection {
    #[template_child]
    pub header_box: TemplateChild<gtk4::Box>,
    #[template_child]
    pub header_label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub add_folder_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub drilldown_header_box: TemplateChild<gtk4::Box>,
    #[template_child]
    pub drilldown_back_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub drilldown_path_label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub inner_scrolled_window: TemplateChild<gtk4::ScrolledWindow>,
    #[template_child]
    pub file_tree_view: TemplateChild<gtk4::ListView>,

    /// Unique ID for this workspace (matches `WorkspaceConfig.id`).
    pub workspace_id: RefCell<WorkspaceId>,

    /// Stack of paths for deep-nesting drill-down navigation.
    pub drilldown_stack: RefCell<Vec<PathBuf>>,
    /// Original workspace entries to restore when exiting drill-down.
    pub original_roots: RefCell<Vec<WorkspaceEntry>>,
    /// Remember expanded paths across drill-downs to restore tree state.
    pub expanded_paths: RefCell<std::collections::HashSet<PathBuf>>,
    /// Path to select and scroll to once it loads (used after navigating back).
    pub pending_selection: RefCell<Option<PathBuf>>,

    /// Popover for the right-click context menu on file rows.
    pub context_menu: RefCell<Option<gtk4::PopoverMenu>>,
    /// Popover for the right-click context menu on the workspace header.
    pub header_context_menu: RefCell<Option<gtk4::PopoverMenu>>,
    /// Path of the item under the right-click context menu. Set on gesture
    /// press, read by action handlers (rename, delete, new file).
    pub context_path: RefCell<Option<PathBuf>>,
    /// Whether the context-menu target is a directory.
    pub context_is_dir: Cell<bool>,
    /// The TreeExpander widget for the context-menu target row. Needed to
    /// swap the label for an inline rename entry.
    pub context_expander: RefCell<Option<gtk4::TreeExpander>>,
    /// True while a New File/Folder flow is active. Distinguishes
    /// rename-after-create from user-initiated rename.
    pub is_new_item: Cell<bool>,

    /// The flattened tree model that GtkListView renders.
    pub tree_model: RefCell<Option<gtk4::TreeListModel>>,
    /// Root-level ListStore backing the TreeListModel.
    pub root_store: RefCell<Option<gio::ListStore>>,
    /// Weak refs to expanded directory rows for fast lookup by path.
    pub dir_rows: RefCell<HashMap<PathBuf, glib::WeakRef<gtk4::TreeListRow>>>,
    /// Weak refs to child ListStores for direct model manipulation.
    pub dir_stores: RefCell<HashMap<PathBuf, glib::WeakRef<gio::ListStore>>>,
    /// Cancellation tokens for background directory scans, keyed by path.
    pub child_scan_tokens: RefCell<HashMap<PathBuf, Arc<AtomicBool>>>,
    /// Ordered root paths in the root ListStore.
    pub root_paths: RefCell<Vec<PathBuf>>,
    /// Ordered child paths per parent directory.
    pub child_paths: RefCell<HashMap<PathBuf, Vec<PathBuf>>>,
    /// O(1) path → (parent, index) lookup for fast model removal.
    pub item_locations: RefCell<HashMap<PathBuf, ItemLocation>>,

    // File operation callbacks (forwarded to sidebar → window)
    pub rename_callback: RefCell<Option<RenameCallback>>,
    pub delete_callback: RefCell<Option<FileCallback>>,
    pub create_callback: RefCell<Option<FileCallback>>,

    // Workspace-level callbacks (handled by sidebar)
    pub add_folder_callback: RefCell<Option<WorkspaceCallback>>,
    pub rename_workspace_callback: RefCell<Option<WorkspaceCallback>>,
    pub unlist_workspace_callback: RefCell<Option<WorkspaceCallback>>,
    pub folder_focused_callback: RefCell<Option<WorkspaceCallback>>,
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextWorkspaceSection {
    const NAME: &'static str = "LushtextWorkspaceSection";
    type Type = super::LushtextWorkspaceSection;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextWorkspaceSection {
    fn constructed(&self) {
        self.parent_constructed();
        self.setup_factory();
        self.setup_file_context_menu();
        self.setup_header_context_menu();
        self.setup_header_double_click();

        // Wire add-folder button
        let obj_weak = self.obj().downgrade();
        self.add_folder_button.connect_clicked(move |_| {
            if let Some(section) = obj_weak.upgrade() {
                section.notify_add_folder_requested();
            }
        });

        // Wire drilldown back button
        let obj_weak = self.obj().downgrade();
        self.drilldown_back_button.connect_clicked(move |_| {
            if let Some(section) = obj_weak.upgrade() {
                section.navigate_back();
            }
        });
    }

    fn dispose(&self) {
        if let Some(popover) = self.context_menu.borrow_mut().take() {
            popover.unparent();
        }
        if let Some(popover) = self.header_context_menu.borrow_mut().take() {
            popover.unparent();
        }
    }
}

impl WidgetImpl for LushtextWorkspaceSection {}
impl BoxImpl for LushtextWorkspaceSection {}

impl LushtextWorkspaceSection {
    /// Set up the list item factory for rendering file tree rows.
    ///
    /// `SignalListItemFactory` is GTK4's way of creating and recycling row widgets:
    /// - `connect_setup`: creates the row's widget hierarchy (reused across items)
    /// - `connect_bind`: updates row widgets to reflect the current data item
    /// - `connect_unbind`: cleans up item-specific state for row recycling
    fn setup_factory(&self) {
        let factory = gtk4::SignalListItemFactory::new();

        factory.connect_setup(|_factory, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("item is ListItem");

            let overlay = gtk4::Overlay::new();

            let expander = gtk4::TreeExpander::new();
            let content_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            content_box.set_halign(gtk4::Align::Start);

            let icon = gtk4::Image::new();
            icon.set_icon_size(gtk4::IconSize::Normal);

            let label = gtk4::Label::new(None);
            label.set_xalign(0.0);
            label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            label.set_wrap(false);
            label.set_hexpand(true);

            let focus_btn = gtk4::Button::from_icon_name("go-next-symbolic");
            focus_btn.set_valign(gtk4::Align::Center);
            focus_btn.set_halign(gtk4::Align::End);
            focus_btn.add_css_class("flat");
            focus_btn.add_css_class("circular");
            focus_btn.set_tooltip_text(Some("Focus Folder"));
            focus_btn.set_margin_end(6);
            focus_btn.set_visible(false);

            let list_item_clone = list_item.clone();
            let overlay_clone = overlay.clone();
            focus_btn.connect_clicked(move |_| {
                if let Some(tree_row) = list_item_clone.item().and_downcast::<gtk4::TreeListRow>()
                    && let Some(file_item) = tree_row.item().and_downcast::<FileTreeItem>()
                    && let Some(path) = file_item.path()
                {
                    // Find the WorkspaceSection by walking up the widget tree
                    let mut current: Option<gtk4::Widget> =
                        Some(overlay_clone.clone().upcast::<gtk4::Widget>());
                    while let Some(w) = current {
                        if let Some(section) = w.downcast_ref::<super::LushtextWorkspaceSection>() {
                            section.focus_folder(&path);
                            break;
                        }
                        current = w.parent();
                    }
                }
            });

            content_box.append(&icon);
            content_box.append(&label);
            expander.set_child(Some(&content_box));

            overlay.set_child(Some(&expander));
            overlay.add_overlay(&focus_btn);

            let motion = gtk4::EventControllerMotion::new();
            let btn_enter = focus_btn.clone();
            motion.connect_enter(move |_, _, _| {
                if btn_enter.has_css_class("can-focus") {
                    btn_enter.set_visible(true);
                }
            });
            let btn_leave = focus_btn.clone();
            motion.connect_leave(move |_| {
                btn_leave.set_visible(false);
            });
            overlay.add_controller(motion);

            list_item.set_child(Some(&overlay));
        });

        let section_weak = self.obj().downgrade();
        factory.connect_bind(move |_factory, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("item is ListItem");

            let tree_row = list_item
                .item()
                .and_downcast::<gtk4::TreeListRow>()
                .expect("item is TreeListRow");

            let overlay = list_item
                .child()
                .and_downcast::<gtk4::Overlay>()
                .expect("child is Overlay");

            let expander = overlay
                .child()
                .and_downcast::<gtk4::TreeExpander>()
                .expect("overlay child is TreeExpander");

            expander.set_list_row(Some(&tree_row));

            let mut focus_btn_opt = None;
            let mut current = overlay.first_child();
            while let Some(child) = current {
                if child.downcast_ref::<gtk4::Button>().is_some() {
                    focus_btn_opt = child.downcast::<gtk4::Button>().ok();
                    break;
                }
                current = child.next_sibling();
            }
            let focus_btn = focus_btn_opt.expect("focus_btn missing");

            if let Some(file_item) = tree_row.item().and_downcast::<FileTreeItem>() {
                let content_box = expander
                    .child()
                    .and_downcast::<gtk4::Box>()
                    .expect("expander child is Box");

                let icon = content_box
                    .first_child()
                    .and_downcast::<gtk4::Image>()
                    .expect("first child is Image");

                let label = icon
                    .next_sibling()
                    .and_downcast::<gtk4::Label>()
                    .expect("second child is Label");

                let icon_name = if file_item.is_placeholder() {
                    "dialog-information-symbolic"
                } else if file_item.is_dir() {
                    "folder-symbolic"
                } else {
                    "text-x-generic-symbolic"
                };
                icon.set_icon_name(Some(icon_name));

                if file_item.is_empty() == Some(true) {
                    label.set_markup(&format!(
                        "{} <span alpha=\"60%\"><i>(Empty)</i></span>",
                        glib::markup_escape_text(&file_item.name())
                    ));
                } else {
                    label.set_label(&file_item.name());
                }

                if let Some(path) = file_item.path() {
                    expander.set_tooltip_text(Some(&path.to_string_lossy()));
                } else {
                    expander.set_tooltip_text(None);
                }

                let show_focus = file_item.is_dir()
                    && !file_item.is_placeholder()
                    && file_item.is_empty() != Some(true)
                    && tree_row.depth() > 0;
                if show_focus {
                    focus_btn.add_css_class("can-focus");
                    content_box.set_margin_end(36);
                } else {
                    focus_btn.remove_css_class("can-focus");
                    content_box.set_margin_end(0);
                    focus_btn.set_visible(false);
                }

                if file_item.is_dir()
                    && !file_item.is_placeholder()
                    && let Some(section) = section_weak.upgrade()
                    && let Some(path) = file_item.path()
                {
                    section
                        .imp()
                        .dir_rows
                        .borrow_mut()
                        .insert(path, tree_row.downgrade());
                }

                // GTK recycles ListItem widgets: a row previously used for
                // inline rename may still have a GtkEntry appended.
                let mut child = label.next_sibling();
                while let Some(sibling) = child {
                    child = sibling.next_sibling();
                    if sibling.downcast_ref::<gtk4::Entry>().is_some() {
                        content_box.remove(&sibling);
                    }
                }
                label.set_visible(true);

                // If this item was just created (New File/Folder), show inline entry
                if file_item.is_pending_rename() {
                    file_item.set_pending_rename(false);
                    if let Some(section) = section_weak.upgrade() {
                        let imp = section.imp();
                        *imp.context_path.borrow_mut() = file_item.path();
                        imp.context_is_dir.set(file_item.is_dir());
                        *imp.context_expander.borrow_mut() = Some(expander.clone());
                        let sw = section.downgrade();
                        glib::idle_add_local_once(move || {
                            if let Some(s) = sw.upgrade() {
                                s.begin_rename();
                            }
                        });
                    }
                }

                // Disable the TreeExpander's internal GestureClick for file rows.
                // GtkTreeExpander installs a BUBBLE-phase gesture that intercepts
                // clicks for ALL rows — even non-expandable files — preventing
                // GtkListView's built-in double-click activation from firing.
                // Setting phase=None disables it for files while preserving
                // expand/collapse for directories. Must run on every bind
                // (row recycling resets state).
                let phase = if file_item.is_dir() && !file_item.is_placeholder() {
                    gtk4::PropagationPhase::Bubble
                } else {
                    gtk4::PropagationPhase::None
                };
                let controllers = expander.observe_controllers();
                for i in 0..controllers.n_items() {
                    if let Some(obj) = controllers.item(i)
                        && let Ok(gesture) = obj.downcast::<gtk4::GestureClick>()
                    {
                        gesture.set_propagation_phase(phase);
                    }
                }
            }
        });

        let section_weak = self.obj().downgrade();
        factory.connect_unbind(move |_factory, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("item is ListItem");

            let Some(tree_row) = list_item.item().and_downcast::<gtk4::TreeListRow>() else {
                return;
            };
            let Some(file_item) = tree_row.item().and_downcast::<FileTreeItem>() else {
                return;
            };
            if !file_item.is_dir() {
                return;
            }

            if let Some(section) = section_weak.upgrade()
                && let Some(ref path) = file_item.path()
            {
                section.imp().dir_rows.borrow_mut().remove(path.as_path());
            }
        });

        self.file_tree_view.set_factory(Some(&factory));
    }

    /// Build the right-click context menu for file/directory items.
    fn setup_file_context_menu(&self) {
        let obj = self.obj();

        let menu = gio::Menu::new();

        let nav_section = gio::Menu::new();
        nav_section.append(Some("Focus Folder"), Some("section.focus-folder"));
        menu.append_section(None, &nav_section);

        let create_section = gio::Menu::new();
        create_section.append(Some("New File"), Some("section.new-file"));
        create_section.append(Some("New Folder"), Some("section.new-dir"));
        menu.append_section(None, &create_section);

        let edit_section = gio::Menu::new();
        edit_section.append(Some("Rename"), Some("section.rename"));
        edit_section.append(Some("Delete"), Some("section.delete"));
        menu.append_section(None, &edit_section);

        let popover = gtk4::PopoverMenu::from_model(Some(&menu));
        popover.set_parent(&*self.file_tree_view);
        popover.set_has_arrow(false);
        popover.set_halign(gtk4::Align::Start);
        *self.context_menu.borrow_mut() = Some(popover);

        // Register actions under the "section" prefix. Context menu items
        // reference them as "section.new-file", "section.rename", etc.
        // GTK resolves these by walking up the widget tree for the prefix.
        let action_group = gio::SimpleActionGroup::new();

        let focus_folder_action = gio::SimpleAction::new("focus-folder", None);
        let section_weak = obj.downgrade();
        focus_folder_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade()
                && let Some(path) = section.imp().context_path.borrow().clone()
                && section.imp().context_is_dir.get()
            {
                section.focus_folder(&path);
            }
        });
        action_group.add_action(&focus_folder_action);

        let new_file_action = gio::SimpleAction::new("new-file", None);
        let section_weak = obj.downgrade();
        new_file_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                section.create_new_item(false);
            }
        });
        action_group.add_action(&new_file_action);

        let new_dir_action = gio::SimpleAction::new("new-dir", None);
        let section_weak = obj.downgrade();
        new_dir_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                section.create_new_item(true);
            }
        });
        action_group.add_action(&new_dir_action);

        let rename_action = gio::SimpleAction::new("rename", None);
        let section_weak = obj.downgrade();
        rename_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                section.begin_rename();
            }
        });
        action_group.add_action(&rename_action);

        let delete_action = gio::SimpleAction::new("delete", None);
        let section_weak = obj.downgrade();
        delete_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                section.show_delete_confirmation();
            }
        });
        action_group.add_action(&delete_action);

        obj.insert_action_group("section", Some(&action_group));

        // Right-click gesture on the ListView
        let gesture = gtk4::GestureClick::new();
        gesture.set_button(3);

        let section_weak = obj.downgrade();
        let focus_folder_action_clone = focus_folder_action.clone();
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
            let Some(path) = file_item.path() else {
                return;
            };

            let imp = section.imp();
            *imp.context_path.borrow_mut() = Some(path);
            imp.context_is_dir.set(file_item.is_dir());
            *imp.context_expander.borrow_mut() = Some(expander);

            focus_folder_action_clone.set_enabled(
                file_item.is_dir() && !file_item.is_placeholder() && tree_row.depth() > 0,
            );

            let popover = imp.context_menu.borrow().clone();
            if let Some(popover) = popover {
                #[expect(clippy::cast_possible_truncation)] // click coords fit in i32
                popover.set_pointing_to(Some(&gdk4::Rectangle::new(x as i32, y as i32, 1, 1)));
                popover.popup();
            }
        });

        self.file_tree_view.add_controller(gesture);
    }

    /// Build right-click context menu for the workspace header (Rename / Unlist).
    fn setup_header_context_menu(&self) {
        let obj = self.obj();

        let menu = gio::Menu::new();
        menu.append(Some("Rename Workspace"), Some("ws-header.rename"));
        menu.append(Some("Unlist Workspace"), Some("ws-header.unlist"));

        let popover = gtk4::PopoverMenu::from_model(Some(&menu));
        popover.set_parent(&*self.header_box);
        popover.set_has_arrow(false);
        popover.set_halign(gtk4::Align::Start);
        *self.header_context_menu.borrow_mut() = Some(popover.clone());

        let action_group = gio::SimpleActionGroup::new();

        let rename_action = gio::SimpleAction::new("rename", None);
        let section_weak = obj.downgrade();
        rename_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                section.notify_rename_workspace_requested();
            }
        });
        action_group.add_action(&rename_action);

        let unlist_action = gio::SimpleAction::new("unlist", None);
        let section_weak = obj.downgrade();
        unlist_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
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
            #[expect(clippy::cast_possible_truncation)] // click coords fit in i32
            popover_ref.set_pointing_to(Some(&gdk4::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover_ref.popup();
        });

        self.header_box.add_controller(gesture);
    }

    /// Set up double-click gesture on the workspace header to expand/collapse roots.
    fn setup_header_double_click(&self) {
        let obj = self.obj();
        let gesture = gtk4::GestureClick::new();

        let section_weak = obj.downgrade();
        gesture.connect_pressed(move |_, n_press, _, _| {
            if n_press == 2
                && let Some(section) = section_weak.upgrade()
            {
                section.toggle_roots();
            }
        });

        self.header_box.add_controller(gesture);
    }
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
