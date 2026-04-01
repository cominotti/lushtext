// SPDX-License-Identifier: GPL-3.0-or-later

use super::file_tree_item::FileTreeItem;
use gtk4::gio;
use gtk4::gio::prelude::ListModelExt;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{self, glib, CompositeTemplate};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};

type RenameCallback = Box<dyn Fn(&Path, &Path)>;
type DeleteCallback = Box<dyn Fn(&Path)>;
type CreateCallback = Box<dyn Fn(&Path)>;

#[derive(Default, CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/sidebar.ui")]
pub struct LushtextSidebar {
    #[template_child]
    pub workspace_label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub file_tree_view: TemplateChild<gtk4::ListView>,
    #[template_child]
    pub add_folder_button: TemplateChild<gtk4::Button>,

    // Context menu state
    pub context_menu: RefCell<Option<gtk4::PopoverMenu>>,
    pub context_path: RefCell<Option<PathBuf>>,
    pub context_is_dir: Cell<bool>,
    pub context_expander: RefCell<Option<gtk4::TreeExpander>>,
    pub is_new_item: Cell<bool>,

    // Model references for tree manipulation
    pub tree_model: RefCell<Option<gtk4::TreeListModel>>,
    pub root_store: RefCell<Option<gio::ListStore>>,

    // Callbacks for window integration
    pub rename_callback: RefCell<Option<RenameCallback>>,
    pub delete_callback: RefCell<Option<DeleteCallback>>,
    pub create_callback: RefCell<Option<CreateCallback>>,
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextSidebar {
    const NAME: &'static str = "LushtextSidebar";
    type Type = super::LushtextSidebar;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextSidebar {
    fn constructed(&self) {
        self.parent_constructed();
        self.setup_factory();
        self.setup_context_menu();
    }
}

impl WidgetImpl for LushtextSidebar {}
impl BoxImpl for LushtextSidebar {}

impl LushtextSidebar {
    /// Set up the list item factory for rendering file tree rows.
    fn setup_factory(&self) {
        let factory = gtk4::SignalListItemFactory::new();

        factory.connect_setup(|_factory, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("item is ListItem");

            let expander = gtk4::TreeExpander::new();
            let content_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);

            let icon = gtk4::Image::new();
            icon.set_icon_size(gtk4::IconSize::Normal);

            let label = gtk4::Label::new(None);
            label.set_xalign(0.0);
            label.set_ellipsize(pango::EllipsizeMode::End);
            label.add_css_class("monospace");

            content_box.append(&icon);
            content_box.append(&label);
            expander.set_child(Some(&content_box));

            list_item.set_child(Some(&expander));
        });

        let sidebar_weak = self.obj().downgrade();
        factory.connect_bind(move |_factory, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("item is ListItem");

            let tree_row = list_item
                .item()
                .and_downcast::<gtk4::TreeListRow>()
                .expect("item is TreeListRow");

            let expander = list_item
                .child()
                .and_downcast::<gtk4::TreeExpander>()
                .expect("child is TreeExpander");

            expander.set_list_row(Some(&tree_row));

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

                let icon_name = if file_item.is_dir() {
                    "folder-symbolic"
                } else {
                    "text-x-generic-symbolic"
                };
                icon.set_icon_name(Some(icon_name));
                label.set_label(&file_item.name());

                // Clean up any rename entry left from row recycling
                if let Some(sibling) = label.next_sibling() {
                    if sibling.downcast_ref::<gtk4::Entry>().is_some() {
                        content_box.remove(&sibling);
                    }
                }
                label.set_visible(true);

                // If this item was just created (New File/Folder), show inline entry
                if file_item.is_pending_rename() {
                    file_item.set_pending_rename(false);
                    if let Some(sidebar) = sidebar_weak.upgrade() {
                        let imp = sidebar.imp();
                        *imp.context_path.borrow_mut() = Some(file_item.path());
                        imp.context_is_dir.set(file_item.is_dir());
                        *imp.context_expander.borrow_mut() = Some(expander.clone());
                        // Schedule rename on next idle (row needs to be fully realized)
                        let sw = sidebar.downgrade();
                        glib::idle_add_local_once(move || {
                            if let Some(s) = sw.upgrade() {
                                s.begin_rename();
                            }
                        });
                    }
                }

                // Disable the TreeExpander's internal GestureClick for file rows.
                let phase = if file_item.is_dir() {
                    gtk4::PropagationPhase::Bubble
                } else {
                    gtk4::PropagationPhase::None
                };
                let controllers = expander.observe_controllers();
                for i in 0..controllers.n_items() {
                    if let Some(obj) = controllers.item(i) {
                        if let Ok(gesture) = obj.downcast::<gtk4::GestureClick>() {
                            gesture.set_propagation_phase(phase);
                        }
                    }
                }
            }
        });

        self.file_tree_view.set_factory(Some(&factory));
    }

    /// Build the right-click context menu with New, Rename, and Delete actions.
    fn setup_context_menu(&self) {
        let obj = self.obj();

        // Menu model with two sections: create and edit
        let menu = gio::Menu::new();

        let create_section = gio::Menu::new();
        create_section.append(Some("New File"), Some("sidebar.new-file"));
        create_section.append(Some("New Folder"), Some("sidebar.new-dir"));
        menu.append_section(None, &create_section);

        let edit_section = gio::Menu::new();
        edit_section.append(Some("Rename"), Some("sidebar.rename"));
        edit_section.append(Some("Delete"), Some("sidebar.delete"));
        menu.append_section(None, &edit_section);

        // Popover attached to the ListView
        let popover = gtk4::PopoverMenu::from_model(Some(&menu));
        popover.set_parent(&*self.file_tree_view);
        popover.set_has_arrow(false);
        popover.set_halign(gtk4::Align::Start);
        *self.context_menu.borrow_mut() = Some(popover.clone());

        // Actions
        let action_group = gio::SimpleActionGroup::new();

        let new_file_action = gio::SimpleAction::new("new-file", None);
        let sidebar_weak = obj.downgrade();
        new_file_action.connect_activate(move |_, _| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.create_new_item(false);
            }
        });
        action_group.add_action(&new_file_action);

        let new_dir_action = gio::SimpleAction::new("new-dir", None);
        let sidebar_weak = obj.downgrade();
        new_dir_action.connect_activate(move |_, _| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.create_new_item(true);
            }
        });
        action_group.add_action(&new_dir_action);

        let rename_action = gio::SimpleAction::new("rename", None);
        let sidebar_weak = obj.downgrade();
        rename_action.connect_activate(move |_, _| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.begin_rename();
            }
        });
        action_group.add_action(&rename_action);

        let delete_action = gio::SimpleAction::new("delete", None);
        let sidebar_weak = obj.downgrade();
        delete_action.connect_activate(move |_, _| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.show_delete_confirmation();
            }
        });
        action_group.add_action(&delete_action);

        obj.insert_action_group("sidebar", Some(&action_group));

        // Right-click gesture on the ListView
        let gesture = gtk4::GestureClick::new();
        gesture.set_button(3); // Secondary (right) click

        let sidebar_weak = obj.downgrade();
        gesture.connect_pressed(move |gesture, _n_press, x, y| {
            let Some(sidebar) = sidebar_weak.upgrade() else {
                return;
            };
            let Some(list_view) = gesture.widget() else {
                return;
            };

            // Find the TreeExpander at the click position
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

            // Store context for the action handlers
            let imp = sidebar.imp();
            *imp.context_path.borrow_mut() = Some(file_item.path());
            imp.context_is_dir.set(file_item.is_dir());
            *imp.context_expander.borrow_mut() = Some(expander);

            // Position and show the popover
            let popover = imp.context_menu.borrow().clone();
            if let Some(popover) = popover {
                popover.set_pointing_to(Some(&gdk4::Rectangle::new(x as i32, y as i32, 1, 1)));
                popover.popup();
            }
        });

        self.file_tree_view.add_controller(gesture);
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
