// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the multi-workspace sidebar.
//!
//! Manages workspace sections, the fixed "New Workspace" affordance, and
//! debounced persistence of workspace state to disk.

use crate::model::workspace::WorkspacesFile;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, glib};
use std::cell::{Cell, RefCell};
use std::path::Path;

use super::workspace_section::LushtextWorkspaceSection;

type FileCallback = Box<dyn Fn(&Path)>;
type RenameCallback = Box<dyn Fn(&Path, &Path)>;

// CompositeTemplate loads the UI layout from a compiled XML file.
// GObject methods always take &self; RefCell/Cell provide interior mutability.
#[derive(Default, CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/sidebar.ui")]
pub struct LushtextSidebar {
    #[template_child]
    pub outer_scrolled_window: TemplateChild<gtk4::ScrolledWindow>,
    #[template_child]
    pub sections_box: TemplateChild<gtk4::Box>,
    #[template_child]
    pub new_workspace_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub new_workspace_label: TemplateChild<gtk4::Label>,

    /// Current in-memory workspace configuration. Cloned out of `RefCell`
    /// for background save operations.
    pub workspaces_file: RefCell<WorkspacesFile>,
    /// Live workspace section widgets in display order.
    pub sections: RefCell<Vec<LushtextWorkspaceSection>>,

    /// Callback for file double-click activation, forwarded to the window.
    pub file_activated_callback: RefCell<Option<FileCallback>>,
    pub rename_callback: RefCell<Option<RenameCallback>>,
    pub delete_callback: RefCell<Option<FileCallback>>,
    pub create_callback: RefCell<Option<FileCallback>>,
    /// Callback notifying the window that workspace structure changed.
    pub workspace_changed_callback: RefCell<Option<Box<dyn Fn()>>>,
    /// Generation counter for debouncing workspace persistence (150ms).
    pub persist_generation: Cell<u32>,
    /// Guard preventing overlapping persistence writes to disk.
    pub persist_inflight: Cell<bool>,
    /// Dirty flag set when a mutation occurs while persistence is in-flight.
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

        // Wire the fixed "New Workspace" button at the top of the sidebar.
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
