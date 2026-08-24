// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the multi-workspace sidebar.
//!
//! Manages workspace sections, the fixed workspace selector row, and debounced
//! persistence of workspace state to disk.

use crate::model::workspace::{WorkspaceId, WorkspaceScope, WorkspacesFile};
use crate::model::workspace_persistence::WorkspacePersistenceState;
use crate::services::notifications::NotificationSeverity;
use crate::ui::accessibility;
use crate::ui::sidebar::SidebarFileRowStateSnapshot;
use gtk_lush_settle::{Debounce, SupersedingTimer};
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, glib};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::workspace_section::LushtextWorkspaceSection;

type FileCallback = Box<dyn Fn(&Path)>;
type MessageCallback = Box<dyn Fn(&str, NotificationSeverity)>;
type RenameCallback = Box<dyn Fn(&Path, &Path)>;
type WorkspaceCallback = Box<dyn Fn(WorkspaceId)>;
type FolderNotePathCallback = Box<dyn Fn(WorkspaceId, PathBuf)>;
type WorkspaceScopeCallback = Box<dyn Fn(WorkspaceScope)>;
type WorkspacePersistenceFlushCallback =
    Box<dyn FnOnce(Result<(), super::WorkspacePersistenceFlushError>)>;

/// Private template implementation for the multi-workspace sidebar.
///
/// Owns the fixed selector row, scrollable workspace-section list, persistence
/// state, and callback glue that forwards row actions to the window.
#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/sidebar.ui")]
pub struct LushtextSidebar {
    /// Outer scroller containing the workspace section list.
    #[template_child]
    pub outer_scrolled_window: TemplateChild<gtk4::ScrolledWindow>,
    /// Vertical box that hosts live workspace section widgets.
    #[template_child]
    pub sections_box: TemplateChild<gtk4::Box>,
    /// Fixed top row containing workspace scope and add-workspace controls.
    #[template_child]
    pub new_workspace_box: TemplateChild<gtk4::Box>,
    /// Dropdown selecting all workspaces or one concrete workspace.
    #[template_child]
    pub workspace_filter_dropdown: TemplateChild<gtk4::DropDown>,
    /// Revealer used for workspace-list filter transitions.
    #[template_child]
    pub workspace_list_revealer: TemplateChild<gtk4::Revealer>,
    /// Button that creates a new workspace from a selected folder.
    #[template_child]
    pub new_workspace_button: TemplateChild<gtk4::Button>,

    /// Current in-memory workspace configuration. Cloned out of `RefCell`
    /// for background save operations.
    pub workspaces_file: RefCell<WorkspacesFile>,
    /// Live workspace section widgets in display order.
    pub sections: RefCell<Vec<LushtextWorkspaceSection>>,
    /// Window-owned tab projection forwarded into every workspace section.
    pub(super) file_row_state_snapshot: RefCell<Rc<SidebarFileRowStateSnapshot>>,
    /// Current workspace scope mirrored into the selector row and section visibility.
    pub current_scope: RefCell<WorkspaceScope>,
    /// Maps dropdown positions to concrete scope values.
    pub workspace_filter_options: RefCell<Vec<WorkspaceScope>>,
    /// Guard to suppress selector callbacks while the dropdown is being rebuilt.
    pub syncing_workspace_filter: Cell<bool>,
    /// Scope currently applied to the visible workspace list.
    pub applied_workspace_filter: RefCell<WorkspaceScope>,
    /// Guard tracking the fade-out/fade-in sequence for selector changes.
    pub workspace_filter_animation_active: Cell<bool>,
    /// Safety-net timer that reapplies the workspace filter if the revealer's
    /// child-revealed notification does not fire (test/headless frame clocks).
    /// Superseding semantics remove a stale arm when the animation re-arms.
    pub workspace_filter_settle_timer: SupersedingTimer,

    /// Callback for file double-click activation, forwarded to the window.
    pub file_activated_callback: RefCell<Option<FileCallback>>,
    /// Callback for file-row local-history requests, forwarded to the window.
    pub local_history_callback: RefCell<Option<FileCallback>>,
    /// Callback for file-row document-note requests, forwarded to the window.
    pub document_note_callback: RefCell<Option<FileCallback>>,
    /// Callback for sidebar file renames so the window can update open tabs.
    pub rename_callback: RefCell<Option<RenameCallback>>,
    /// Callback for sidebar file deletion so the window can close affected tabs.
    pub delete_callback: RefCell<Option<FileCallback>>,
    /// Callback for sidebar-created files so the window can open the new tab.
    pub create_callback: RefCell<Option<FileCallback>>,
    /// Callback forwarding workspace-section status messages to the window.
    pub message_callback: RefCell<Option<MessageCallback>>,
    /// Callback for workspace-header note requests, forwarded to the window.
    pub folder_note_callback: RefCell<Option<WorkspaceCallback>>,
    /// Callback for exact top-level folder-row note requests.
    pub folder_note_for_folder_callback: RefCell<Option<FolderNotePathCallback>>,
    /// Callback notifying the window that workspace structure changed.
    pub workspace_structure_changed_callback: RefCell<Option<Box<dyn Fn()>>>,
    /// Callback notifying the window that the shared workspace scope changed.
    pub workspace_scope_changed_callback: RefCell<Option<WorkspaceScopeCallback>>,
    /// Debounce for workspace persistence (150ms).
    pub persist_debounce: Debounce,
    /// Requested, in-flight, durable, and failed workspace-save generations.
    pub persistence: RefCell<WorkspacePersistenceState>,
    /// Close-flush completions waiting for the newest requested durable terminal.
    pub(super) persistence_flush_waiters: RefCell<Vec<WorkspacePersistenceFlushCallback>>,
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
            file_row_state_snapshot: RefCell::default(),
            current_scope: RefCell::new(WorkspaceScope::All),
            workspace_filter_options: RefCell::default(),
            syncing_workspace_filter: Cell::default(),
            applied_workspace_filter: RefCell::new(WorkspaceScope::All),
            workspace_filter_animation_active: Cell::default(),
            workspace_filter_settle_timer: SupersedingTimer::default(),
            file_activated_callback: RefCell::default(),
            local_history_callback: RefCell::default(),
            document_note_callback: RefCell::default(),
            rename_callback: RefCell::default(),
            delete_callback: RefCell::default(),
            create_callback: RefCell::default(),
            message_callback: RefCell::default(),
            folder_note_callback: RefCell::default(),
            folder_note_for_folder_callback: RefCell::default(),
            workspace_structure_changed_callback: RefCell::default(),
            workspace_scope_changed_callback: RefCell::default(),
            persist_debounce: Debounce::default(),
            persistence: RefCell::default(),
            persistence_flush_waiters: RefCell::default(),
            settings: gio::Settings::new(crate::config::APP_ID),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextSidebar {
    const NAME: &str = "LushtextSidebar";
    type Type = super::LushtextSidebar;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        // The sidebar template embeds workspace-section custom widgets, so the
        // type must be registered before GTK binds the XML template.
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

        accessibility::set_labelled_description(
            &*self.workspace_filter_dropdown,
            "Workspace scope",
            "Choose whether the sidebar shows all workspaces or one workspace",
        );
        accessibility::set_labelled_description(
            &*self.new_workspace_button,
            "New Workspace",
            "Create a named workspace",
        );

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
                let scope = sidebar
                    .imp()
                    .workspace_filter_options
                    .borrow()
                    .get(index)
                    .cloned()
                    .unwrap_or(WorkspaceScope::All);
                let current_scope = sidebar.imp().current_scope.borrow().clone();
                if current_scope == scope {
                    return;
                }
                sidebar.change_scope_from_selector(scope);
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
                        != sidebar.imp().current_scope.borrow().clone()
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
