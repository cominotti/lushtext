// SPDX-License-Identifier: GPL-3.0-or-later

use crate::model::workspace::WorkspacesFile;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{self, glib, CompositeTemplate};
use std::cell::{Cell, RefCell};
use std::path::Path;

use super::workspace_section::LushtextWorkspaceSection;

type FileCallback = Box<dyn Fn(&Path)>;
type RenameCallback = Box<dyn Fn(&Path, &Path)>;

#[derive(Default, CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/sidebar.ui")]
pub struct LushtextSidebar {
    #[template_child]
    pub sections_box: TemplateChild<gtk4::Box>,
    #[template_child]
    pub new_workspace_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub new_workspace_label: TemplateChild<gtk4::Label>,

    // Workspace state
    pub workspaces_file: RefCell<WorkspacesFile>,
    pub sections: RefCell<Vec<LushtextWorkspaceSection>>,

    // Callbacks forwarded from sections to window
    pub file_activated_callback: RefCell<Option<FileCallback>>,
    pub rename_callback: RefCell<Option<RenameCallback>>,
    pub delete_callback: RefCell<Option<FileCallback>>,
    pub create_callback: RefCell<Option<FileCallback>>,
    pub workspace_changed_callback: RefCell<Option<Box<dyn Fn()>>>,
    pub persist_generation: Cell<u32>,
    pub persist_inflight: Cell<bool>,
    pub persist_dirty: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextSidebar {
    const NAME: &'static str = "LushtextSidebar";
    type Type = super::LushtextSidebar;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        LushtextWorkspaceSection::ensure_type();
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextSidebar {
    fn constructed(&self) {
        self.parent_constructed();

        // Wire the "New Workspace" footer button
        let sidebar_weak = self.obj().downgrade();
        self.new_workspace_button.connect_clicked(move |_| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.create_new_workspace();
            }
        });
    }
}

impl WidgetImpl for LushtextSidebar {}
impl BoxImpl for LushtextSidebar {}
