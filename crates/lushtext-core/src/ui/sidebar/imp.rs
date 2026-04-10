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

use super::{WorkspaceSidebarWidthPreset, workspace_section::LushtextWorkspaceSection};

type FileCallback = Box<dyn Fn(&Path)>;
type RenameCallback = Box<dyn Fn(&Path, &Path)>;
type WidthPresetCallback = Box<dyn Fn(WorkspaceSidebarWidthPreset)>;

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
    pub new_workspace_box: TemplateChild<gtk4::Box>,
    #[template_child]
    pub new_workspace_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub new_workspace_label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub workspace_size_box: TemplateChild<gtk4::Box>,
    #[template_child]
    pub small_width_button: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub comfy_width_button: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub large_width_button: TemplateChild<gtk4::ToggleButton>,

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
    /// Callback notifying the window that the width preset changed.
    pub width_preset_callback: RefCell<Option<WidthPresetCallback>>,
    /// Guard to suppress re-entrant button updates while syncing selection.
    pub syncing_width_preset: Cell<bool>,
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

        for (button, preset) in [
            (&self.small_width_button, WorkspaceSidebarWidthPreset::Small),
            (&self.comfy_width_button, WorkspaceSidebarWidthPreset::Comfy),
            (&self.large_width_button, WorkspaceSidebarWidthPreset::Large),
        ] {
            let sidebar_weak = self.obj().downgrade();
            button.connect_clicked(move |_| {
                if let Some(sidebar) = sidebar_weak.upgrade() {
                    sidebar.select_width_preset(preset);
                }
            });
        }

        self.obj()
            .set_width_preset(WorkspaceSidebarWidthPreset::DEFAULT);
    }
}

impl WidgetImpl for LushtextSidebar {}
impl BoxImpl for LushtextSidebar {}
