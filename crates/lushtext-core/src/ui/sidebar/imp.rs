// SPDX-License-Identifier: GPL-3.0-or-later

use super::file_tree_item::FileTreeItem;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{self, glib, CompositeTemplate};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/sidebar.ui")]
pub struct LushtextSidebar {
    #[template_child]
    pub workspace_label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub file_tree_view: TemplateChild<gtk4::ListView>,
    #[template_child]
    pub add_folder_button: TemplateChild<gtk4::Button>,
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

            content_box.append(&icon);
            content_box.append(&label);
            expander.set_child(Some(&content_box));

            list_item.set_child(Some(&expander));
        });

        factory.connect_bind(|_factory, list_item| {
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
            }
        });

        self.file_tree_view.set_factory(Some(&factory));
    }
}
