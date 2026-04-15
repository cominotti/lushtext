// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the multi-workspace sidebar.
//!
//! Manages workspace sections, the fixed workspace selector row, and debounced
//! persistence of workspace state to disk.

use crate::model::workspace::{WorkspaceId, WorkspacesFile};
use crate::services::notifications::NotificationSeverity;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, glib};
use std::cell::{Cell, RefCell};
use std::path::Path;

use super::workspace_section::LushtextWorkspaceSection;

type FileCallback = Box<dyn Fn(&Path)>;
type MessageCallback = Box<dyn Fn(&str, NotificationSeverity)>;
type RenameCallback = Box<dyn Fn(&Path, &Path)>;

// CompositeTemplate loads the UI layout from a compiled XML file.
// GObject methods always take &self; RefCell/Cell provide interior mutability.
#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/sidebar.ui")]
pub struct LushtextSidebar {
    #[template_child]
    pub outer_scrolled_window: TemplateChild<gtk4::ScrolledWindow>,
    #[template_child]
    pub sections_box: TemplateChild<gtk4::Box>,
    #[template_child]
    pub new_workspace_box: TemplateChild<gtk4::Box>,
    #[template_child]
    pub workspace_filter_dropdown: TemplateChild<gtk4::DropDown>,
    #[template_child]
    pub workspace_list_revealer: TemplateChild<gtk4::Revealer>,
    #[template_child]
    pub new_workspace_button: TemplateChild<gtk4::Button>,

    /// Current in-memory workspace configuration. Cloned out of `RefCell`
    /// for background save operations.
    pub workspaces_file: RefCell<WorkspacesFile>,
    /// Live workspace section widgets in display order.
    pub sections: RefCell<Vec<LushtextWorkspaceSection>>,
    /// Visible filter selection in the top selector row. `None` means all workspaces.
    pub selected_workspace_filter: RefCell<Option<WorkspaceId>>,
    /// Maps dropdown positions to workspace IDs. Position 0 is always `None`.
    pub workspace_filter_options: RefCell<Vec<Option<WorkspaceId>>>,
    /// Guard to suppress selector callbacks while the dropdown is being rebuilt.
    pub syncing_workspace_filter: Cell<bool>,
    /// Filter currently applied to the visible workspace list.
    pub applied_workspace_filter: RefCell<Option<WorkspaceId>>,
    /// Guard tracking the fade-out/fade-in sequence for selector changes.
    pub workspace_filter_animation_active: Cell<bool>,

    /// Callback for file double-click activation, forwarded to the window.
    pub file_activated_callback: RefCell<Option<FileCallback>>,
    pub rename_callback: RefCell<Option<RenameCallback>>,
    pub delete_callback: RefCell<Option<FileCallback>>,
    pub create_callback: RefCell<Option<FileCallback>>,
    /// Callback forwarding workspace-section status messages to the window.
    pub message_callback: RefCell<Option<MessageCallback>>,
    /// Callback notifying the window that workspace structure changed.
    pub workspace_changed_callback: RefCell<Option<Box<dyn Fn()>>>,
    /// Generation counter for debouncing workspace persistence (150ms).
    pub persist_generation: Cell<u32>,
    /// Guard preventing overlapping persistence writes to disk.
    pub persist_inflight: Cell<bool>,
    /// Dirty flag set when a mutation occurs while persistence is in-flight.
    pub persist_dirty: Cell<bool>,
    /// Application settings for feature toggles.
    pub settings: gio::Settings,
}

impl Default for LushtextSidebar {
    fn default() -> Self {
        Self {
            outer_scrolled_window: TemplateChild::default(),
            sections_box: TemplateChild::default(),
            new_workspace_box: TemplateChild::default(),
            workspace_filter_dropdown: TemplateChild::default(),
            workspace_list_revealer: TemplateChild::default(),
            new_workspace_button: TemplateChild::default(),
            workspaces_file: RefCell::default(),
            sections: RefCell::default(),
            selected_workspace_filter: RefCell::default(),
            workspace_filter_options: RefCell::default(),
            syncing_workspace_filter: Cell::default(),
            applied_workspace_filter: RefCell::default(),
            workspace_filter_animation_active: Cell::default(),
            file_activated_callback: RefCell::default(),
            rename_callback: RefCell::default(),
            delete_callback: RefCell::default(),
            create_callback: RefCell::default(),
            message_callback: RefCell::default(),
            workspace_changed_callback: RefCell::default(),
            persist_generation: Cell::default(),
            persist_inflight: Cell::default(),
            persist_dirty: Cell::default(),
            settings: gio::Settings::new(crate::config::APP_ID),
        }
    }
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

        // Wire the fixed workspace selector row at the top of the sidebar.
        let sidebar_weak = self.obj().downgrade();
        self.workspace_filter_dropdown
            .connect_selected_notify(move |dropdown| {
                let Some(sidebar) = sidebar_weak.upgrade() else {
                    return;
                };
                if sidebar.imp().syncing_workspace_filter.get() {
                    return;
                }
                let index = dropdown.selected() as usize;
                let filter = sidebar
                    .imp()
                    .workspace_filter_options
                    .borrow()
                    .get(index)
                    .cloned()
                    .unwrap_or(None);
                let current_filter = sidebar.imp().selected_workspace_filter.borrow().clone();
                if current_filter == filter {
                    return;
                }
                *sidebar.imp().selected_workspace_filter.borrow_mut() = filter;
                sidebar.animate_workspace_filter_change();
            });

        let sidebar_weak = self.obj().downgrade();
        self.workspace_list_revealer
            .connect_child_revealed_notify(move |revealer| {
                let Some(sidebar) = sidebar_weak.upgrade() else {
                    return;
                };
                if !sidebar.imp().workspace_filter_animation_active.get() {
                    return;
                }

                if !revealer.reveals_child() && !revealer.is_child_revealed() {
                    sidebar.apply_workspace_filter_visibility();
                    revealer.set_reveal_child(true);
                    return;
                }

                if revealer.reveals_child() && revealer.is_child_revealed() {
                    sidebar.imp().workspace_filter_animation_active.set(false);
                    if sidebar.imp().applied_workspace_filter.borrow().clone()
                        != sidebar.imp().selected_workspace_filter.borrow().clone()
                    {
                        sidebar.animate_workspace_filter_change();
                    }
                }
            });

        // Wire the fixed "New Workspace" button at the top of the sidebar.
        let sidebar_weak = self.obj().downgrade();
        self.new_workspace_button.connect_clicked(move |_| {
            if let Some(sidebar) = sidebar_weak.upgrade() {
                sidebar.create_new_workspace();
            }
        });

        self.obj().refresh_workspace_filter_dropdown();
    }
}

impl WidgetImpl for LushtextSidebar {}
impl BoxImpl for LushtextSidebar {}
