// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the workspace section widget.
//!
//! Each section manages one workspace's file tree (GtkListView + TreeListModel),
//! context menus for files and the workspace header, and callback forwarding
//! to the parent sidebar.

use super::context_menus::FileContextMenuWiring;
pub(super) use super::context_menus::FileContextTarget;
use super::watch_targets::{
    MaterializedWatchTargets, WatchLifetimeGeneration, WatchTargetGeneration,
};
use crate::model::workspace::{
    FolderTreeEntry, WorkspaceFolderId, WorkspaceFolderMoveDirection, WorkspaceId,
};
use crate::services::file_peek::PeekRequestToken;
use crate::services::file_tree::DirectoryRowState;
use crate::services::notifications::NotificationSeverity;
use crate::services::workspace_watch::WorkspaceWatcher;
use crate::ui::accessibility;
use crate::ui::sidebar::SidebarFileRowStateSnapshot;
use gtk_lush_settle::Debounce;
use gtk4::gio;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, glib};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

type FileCallback = Box<dyn Fn(&Path)>;
type FolderMembershipCallback = Box<dyn Fn(&WorkspaceId, &WorkspaceFolderId, &Path)>;
type FolderNotePathCallback = Box<dyn Fn(&WorkspaceId, &Path)>;
type FolderReorderCallback =
    Box<dyn Fn(&WorkspaceId, &WorkspaceFolderId, WorkspaceFolderMoveDirection)>;
type FolderReorderToIndexCallback = Box<dyn Fn(&WorkspaceId, &WorkspaceFolderId, usize)>;
type MessageCallback = Box<dyn Fn(&str, NotificationSeverity)>;
type RenameCallback = Box<dyn Fn(&Path, &Path)>;
type WorkspaceCallback = Box<dyn Fn(&WorkspaceId)>;

/// Cached position of a file tree item for O(1) model removal.
/// Stores the parent directory (or `None` for configured top-level rows) and
/// the index within the parent's `ListStore`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ItemLocation {
    /// Parent directory store containing the item; `None` means top-level folder row.
    pub parent_dir: Option<PathBuf>,
    /// Current index within that parent `ListStore` for direct removal.
    pub index: usize,
}

/// Runtime widget references for the section-owned peek popover.
#[derive(Default)]
pub struct PeekWidgets {
    /// Floating preview card anchored beside the selected file row.
    pub popover: RefCell<Option<gtk4::Popover>>,
    /// Preview title showing the file name.
    pub title_label: RefCell<Option<gtk4::Label>>,
    /// Subtitle showing the absolute file path.
    pub path_label: RefCell<Option<gtk4::Label>>,
    /// Secondary metadata line with file size and modified time.
    pub meta_label: RefCell<Option<gtk4::Label>>,
    /// Stack switching between loading, text, and fallback states.
    pub body_stack: RefCell<Option<gtk4::Stack>>,
    /// Read-only buffer for the bounded text sample.
    pub text_buffer: RefCell<Option<gtk4::TextBuffer>>,
    /// Read-only text view for the bounded text sample.
    pub text_view: RefCell<Option<gtk4::TextView>>,
    /// Fallback headline for unsupported and error states.
    pub fallback_title_label: RefCell<Option<gtk4::Label>>,
    /// Explanatory fallback body copy.
    pub fallback_body_label: RefCell<Option<gtk4::Label>>,
    /// Footer button that promotes the previewed file into a real tab.
    pub open_button: RefCell<Option<gtk4::Button>>,
}

/// Visible session state for the section-owned peek flow.
#[derive(Default)]
pub struct PeekSessionState {
    /// Path currently bound to the popover, if any.
    pub active_path: RefCell<Option<PathBuf>>,
    /// Latest request token used to drop stale async completions.
    pub active_generation: Cell<PeekRequestToken>,
    /// Whether closing the popover should restore focus to the list view.
    pub restore_focus_on_close: Cell<bool>,
    /// Whether the current preview state allows normal open promotion.
    pub open_allowed: Cell<bool>,
}

/// Debounced refresh state for one workspace section.
#[derive(Default)]
pub struct RefreshRuntimeState {
    /// Debounce used to drop stale refresh callbacks.
    pub debounce: Debounce,
    /// Paths accumulated since the last refresh run, capped by watcher policy.
    pub pending_paths: RefCell<HashSet<PathBuf>>,
    /// Whether the next refresh must rebuild the whole current section view.
    pub pending_full_reload: Cell<bool>,
    /// Whether the current scan burst should announce manual-refresh completion.
    pub manual_refresh_announcing: Cell<bool>,
    /// Last scan failure shown to the user so repeated auto-refresh attempts do
    /// not spam the status bar while a folder remains unreadable.
    pub last_reported_error: RefCell<Option<String>>,
    /// Accepted GTK reconciliation batches in the latest refresh lifecycle.
    pub reconcile_batch_count: Cell<u64>,
    /// Largest changed-row batch accepted in the latest refresh lifecycle.
    pub reconcile_max_batch_rows: Cell<usize>,
    /// Exact current reconciliation terminals published by this section.
    pub reconcile_terminal_count: Cell<u64>,
    /// Active batch sources retired because a newer scan superseded them.
    pub reconcile_superseded_count: Cell<u64>,
    /// Rows in the most recently accepted terminal child-cache replacement.
    pub cache_rebuild_input_rows: Cell<usize>,
    /// Plain map/row operations in that terminal child-cache replacement.
    pub cache_rebuild_operations: Cell<usize>,
    #[cfg(feature = "test-utils")]
    /// Section-local delay between GTK reconciliation batches in lifecycle tests.
    pub test_reconcile_batch_delay: Cell<std::time::Duration>,
}

/// Live filesystem-watch wiring for one workspace section.
#[derive(Default)]
pub struct WatchRuntimeState {
    /// Backend watcher for the current materialized folder scopes, if active.
    pub watcher: RefCell<Option<WorkspaceWatcher>>,
    /// Incremental mirror of flattened rows and their effective target set.
    pub(super) targets: RefCell<MaterializedWatchTargets>,
    /// Coalesces rapid model signals before a worker owns watcher replacement.
    pub restart_debounce: Debounce,
    /// Invalidates every completion created before this section is disposed.
    pub(super) lifetime_generation: Cell<WatchLifetimeGeneration>,
    /// Generation owned by the installed watcher, or none during replacement.
    pub(super) installed_generation: Cell<Option<WatchTargetGeneration>>,
    /// Current target generation whose watcher reached a terminal unavailable state.
    pub(super) unavailable_generation: Cell<Option<WatchTargetGeneration>>,
    /// Whether this section already owns one watcher lifecycle worker.
    pub(super) worker_inflight: Cell<bool>,
    /// GTK main-loop source that takes one mailbox notice without blocking.
    pub poll_source_id: RefCell<Option<glib::SourceId>>,
    /// Last watcher error shown to the user so repeated backend failures do not
    /// spam the status bar every poll tick.
    pub last_reported_error: RefCell<Option<String>>,
    #[cfg(feature = "test-utils")]
    /// Section-local worker delay before backend creation in responsiveness tests.
    pub(super) test_start_delay: Cell<std::time::Duration>,
    #[cfg(feature = "test-utils")]
    /// Section-local worker delay before old-handle teardown in responsiveness tests.
    pub(super) test_drop_delay: Cell<std::time::Duration>,
    #[cfg(feature = "test-utils")]
    /// Worker starts observed by this section for latest-only handoff tests.
    pub(super) test_worker_starts: Cell<usize>,
    #[cfg(feature = "test-utils")]
    /// Notices consumed by the most recent GTK poll callback (always zero or one).
    pub(super) test_last_poll_notices: Cell<usize>,
    #[cfg(feature = "test-utils")]
    /// Permanently suppresses watcher restarts for deterministic manual-refresh tests.
    pub(super) test_disabled: Cell<bool>,
}

/// Private template implementation for one workspace section.
///
/// Owns the section header, drill-down chrome, virtualized file tree, context
/// menu state, and callbacks for mutating one workspace's folder set.
#[derive(Default, CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/workspace-section.ui")]
pub struct LushtextWorkspaceSection {
    /// Header row bound from the template and reused for workspace gestures.
    #[template_child]
    pub header_box: TemplateChild<gtk4::Box>,
    /// Disclosure button that hides or reveals this workspace section's folder body.
    #[template_child]
    pub collapse_button: TemplateChild<gtk4::Button>,
    /// Workspace title label; `TemplateChild` delays access until template init.
    #[template_child]
    pub header_label: TemplateChild<gtk4::Label>,
    /// Button that adds another folder membership to this workspace.
    #[template_child]
    pub add_folder_button: TemplateChild<gtk4::Button>,
    /// Manual refresh button shown at the right edge of the section header.
    #[template_child]
    pub refresh_button: TemplateChild<gtk4::Button>,
    /// Drill-down navigation header revealed while the section is focused.
    #[template_child]
    pub drilldown_header_box: TemplateChild<gtk4::Box>,
    /// Back button for leaving the current drill-down folder.
    #[template_child]
    pub drilldown_back_button: TemplateChild<gtk4::Button>,
    /// Label showing the focused folder path in drill-down mode.
    #[template_child]
    pub drilldown_path_label: TemplateChild<gtk4::Label>,
    /// Inner scroller that lets ListView rows yield to the sidebar width.
    #[template_child]
    pub inner_scrolled_window: TemplateChild<gtk4::ScrolledWindow>,
    /// Virtualized tree view rendering the current workspace folder model.
    #[template_child]
    pub file_tree_view: TemplateChild<gtk4::ListView>,
    /// Empty-folder-set message shown when a workspace has no configured folders.
    #[template_child]
    pub empty_folder_set_label: TemplateChild<gtk4::Label>,

    /// Unique ID for this workspace (matches `WorkspaceConfig.id`).
    pub workspace_id: RefCell<WorkspaceId>,

    /// Stack of paths for deep-nesting drill-down navigation.
    pub drilldown_stack: RefCell<Vec<PathBuf>>,
    /// Original top-level folder tree seeds to restore when exiting drill-down.
    pub original_folders: RefCell<Vec<FolderTreeEntry>>,
    /// Stable ids for configured top-level workspace folders, keyed by path.
    pub workspace_folder_ids: RefCell<HashMap<PathBuf, WorkspaceFolderId>>,
    /// Remember expanded paths across drill-downs to restore tree state.
    pub expanded_paths: RefCell<std::collections::HashSet<PathBuf>>,
    /// Path to select and scroll to once it loads (used after navigating back).
    pub pending_selection: RefCell<Option<PathBuf>>,
    /// Whether this section's folder body is hidden by the workspace header disclosure.
    pub section_body_collapsed: Cell<bool>,

    /// Popover for the right-click context menu on file rows.
    pub context_menu: RefCell<Option<gtk4::Popover>>,
    /// Vertical action list inside the file-tree context popover.
    pub context_menu_box: RefCell<Option<gtk4::Box>>,
    /// Action/menu handles reused by pointer, keyboard, and automation-opened
    /// file-tree context menus.
    pub(super) context_menu_wiring: RefCell<Option<FileContextMenuWiring>>,
    /// Popover for the right-click context menu on the workspace header.
    pub header_context_menu: RefCell<Option<gtk4::Popover>>,
    /// Vertical action list inside the workspace-header context popover.
    pub header_context_menu_box: RefCell<Option<gtk4::Box>>,
    /// Row targeted by the file context menu and later action handlers.
    pub(super) context_target: RefCell<Option<FileContextTarget>>,
    /// True while a New File/Folder flow is active. Distinguishes
    /// rename-after-create from user-initiated rename.
    pub is_new_item: Cell<bool>,

    /// The flattened tree model that GtkListView renders.
    pub tree_model: RefCell<Option<gtk4::TreeListModel>>,
    /// Top-level ListStore backing the TreeListModel's configured folder rows.
    pub top_level_store: RefCell<Option<gio::ListStore>>,
    /// Weak refs to expanded directory rows for fast lookup by path.
    pub dir_rows: RefCell<HashMap<PathBuf, glib::WeakRef<gtk4::TreeListRow>>>,
    /// Weak refs to child ListStores for direct model manipulation.
    pub dir_stores: RefCell<HashMap<PathBuf, glib::WeakRef<gio::ListStore>>>,
    /// Latest window-owned open/active tab projection for file-row decoration.
    pub(super) file_row_state_snapshot: RefCell<Rc<SidebarFileRowStateSnapshot>>,
    /// Cancellation tokens for background directory scans, grouped by path.
    ///
    /// Overlapping workspace folders can materialize the same directory in
    /// multiple visible tree rows. Each expanded row owns a scan token so one
    /// duplicate row cannot cancel another row's child-store population.
    pub child_scan_tokens: RefCell<HashMap<PathBuf, Vec<Arc<AtomicBool>>>>,
    /// Direct store-to-token ownership used by batch freshness checks.
    pub child_store_tokens: RefCell<HashMap<usize, Arc<AtomicBool>>>,
    /// Weak identity guard for raw child-store pointer keys.
    ///
    /// GLib may reuse an address after GTK releases a collapsed child model, so
    /// every keyed mirror must still match the weakly held store object.
    pub child_store_refs: RefCell<HashMap<usize, glib::WeakRef<gio::ListStore>>>,
    /// Plain row projection for each child store, keyed by guarded live identity.
    pub child_row_mirrors: RefCell<HashMap<usize, Vec<DirectoryRowState>>>,
    /// Parent directory identity for each retained child-store mirror.
    pub child_store_paths: RefCell<HashMap<usize, PathBuf>>,
    /// Scalar generation advanced with every accepted mirror splice.
    pub child_row_mirror_generations: RefCell<HashMap<usize, u64>>,
    /// Scheduled large-reconciliation source owned with its scan token.
    pub child_reconcile_sources: RefCell<HashMap<usize, (Arc<AtomicBool>, glib::SourceId)>>,
    /// Ordered folder paths in the folder ListStore.
    pub folder_paths: RefCell<Vec<PathBuf>>,
    /// Ordered child paths per parent directory.
    pub child_paths: RefCell<HashMap<PathBuf, Vec<PathBuf>>>,
    /// Visible path occurrence counts across top-level and child rows.
    ///
    /// Overlapping workspace folders can show the same path in more than one
    /// row. Counts let the tree cache keep O(1) locations only for unique
    /// visible paths and fall back to duplicate-aware scans only when needed.
    pub visible_path_counts: RefCell<HashMap<PathBuf, usize>>,
    /// O(1) path → (parent, index) lookup for fast model removal.
    pub item_locations: RefCell<HashMap<PathBuf, ItemLocation>>,
    /// Widgets backing the section-owned file peek popover.
    pub peek_widgets: PeekWidgets,
    /// Active peek target, generation, and focus-return contract.
    pub peek_session: PeekSessionState,
    /// Debounced refresh request state for manual and automatic reloads.
    pub refresh_runtime: RefreshRuntimeState,
    /// Materialized-scope watcher plus GTK-side poll source for automatic refresh.
    pub watch_runtime: WatchRuntimeState,

    /// Rename callback installed by the sidebar after construction.
    pub rename_callback: RefCell<Option<RenameCallback>>,
    /// Delete callback installed by the sidebar after construction.
    pub delete_callback: RefCell<Option<FileCallback>>,
    /// Create callback installed by the sidebar after construction.
    pub create_callback: RefCell<Option<FileCallback>>,
    /// Callback used when a file row should open the local-history browser.
    pub local_history_callback: RefCell<Option<FileCallback>>,
    /// Callback used when a file row should open its document note.
    pub document_note_callback: RefCell<Option<FileCallback>>,
    /// Callback used when a peek should be promoted into the normal open flow.
    pub peek_promote_callback: RefCell<Option<FileCallback>>,
    /// Callback used for lightweight status-bar messages owned by the window.
    pub message_callback: RefCell<Option<MessageCallback>>,

    /// Workspace rename callback installed by the sidebar after construction.
    pub rename_workspace_callback: RefCell<Option<WorkspaceCallback>>,
    /// Add-folder callback installed by the sidebar after construction.
    pub add_folder_callback: RefCell<Option<WorkspaceCallback>>,
    /// Remove-folder callback installed by the sidebar after construction.
    pub remove_folder_callback: RefCell<Option<FolderMembershipCallback>>,
    /// Folder-row note callback installed by the sidebar after construction.
    pub folder_note_for_folder_callback: RefCell<Option<FolderNotePathCallback>>,
    /// Folder-row reorder callback installed by the sidebar after construction.
    pub reorder_folder_callback: RefCell<Option<FolderReorderCallback>>,
    /// Drag-and-drop reorder callback using an absolute post-drop folder index.
    pub reorder_folder_to_index_callback: RefCell<Option<FolderReorderToIndexCallback>>,
    /// Workspace removal callback installed by the sidebar after construction.
    pub unlist_workspace_callback: RefCell<Option<WorkspaceCallback>>,
    /// Drill-down focus callback used to synchronize the parent sidebar.
    pub folder_focused_callback: RefCell<Option<WorkspaceCallback>>,
    /// Folder note callback installed by the sidebar after construction.
    pub folder_note_callback: RefCell<Option<WorkspaceCallback>>,
}

// ObjectSubclass registers this Rust struct as LushtextWorkspaceSection in
// GLib's runtime type system; ParentType makes it behave as a GtkBox.
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
        self.file_tree_view.add_css_class("workspace-file-tree");
        super::row_factory::setup(self);
        self.obj().register_folder_reorder_section();
        super::context_menus::setup_file_context_menu(self);
        super::context_menus::setup_header_context_menu(self);
        super::context_menus::setup_header_double_click(self);
        self.obj().setup_peek();

        accessibility::set_role(&*self.header_box, gtk4::AccessibleRole::Group);
        accessibility::set_labelled_description(
            &*self.header_box,
            "Workspace",
            "Workspace header with folder actions and collapse control",
        );
        accessibility::set_has_popup(&*self.header_box, true);
        accessibility::set_key_shortcuts(&*self.header_box, "Menu, Shift+F10");
        accessibility::set_labelled_description(
            &*self.add_folder_button,
            "Add folder",
            "Add a folder to this workspace",
        );
        accessibility::set_labelled_description(
            &*self.collapse_button,
            "Collapse Workspace",
            "Hide this workspace's folder list",
        );
        accessibility::set_expanded(&*self.collapse_button, Some(true));
        accessibility::set_labelled_description(
            &*self.refresh_button,
            "Refresh Workspace Folders",
            "Reload the files and folders in this workspace section",
        );
        accessibility::set_labelled_description(
            &*self.drilldown_back_button,
            "Back to workspace folders",
            "Return from the focused folder view to the workspace folder list",
        );
        accessibility::set_labelled_description(
            &*self.file_tree_view,
            "Workspace file tree",
            "Files and folders in this workspace",
        );
        accessibility::set_has_popup(&*self.file_tree_view, true);
        accessibility::set_key_shortcuts(&*self.file_tree_view, "Menu, Shift+F10");
        accessibility::set_role(&*self.empty_folder_set_label, gtk4::AccessibleRole::Status);
        accessibility::set_labelled_description(
            &*self.empty_folder_set_label,
            "No folders in this workspace",
            "Add a folder to show files in this workspace",
        );

        // Signal closures keep weak section refs so GTK handlers do not keep a
        // disposed sidebar section alive through a strong reference cycle.
        let obj_weak = self.obj().downgrade();
        self.collapse_button.connect_clicked(move |_| {
            if let Some(section) = obj_weak.upgrade() {
                section.toggle_section_body_collapsed();
            }
        });

        let obj_weak = self.obj().downgrade();
        self.add_folder_button.connect_clicked(move |_| {
            if let Some(section) = obj_weak.upgrade() {
                section.notify_add_folder_requested();
            }
        });

        // Manual refresh uses the same refresh controller as the filesystem
        // watcher so manual and automatic updates stay in sync.
        let obj_weak = self.obj().downgrade();
        self.refresh_button.connect_clicked(move |_| {
            if let Some(section) = obj_weak.upgrade() {
                section.request_manual_refresh();
            }
        });

        let obj_weak = self.obj().downgrade();
        self.drilldown_back_button.connect_clicked(move |_| {
            if let Some(section) = obj_weak.upgrade() {
                section.navigate_back();
            }
        });
    }

    fn dispose(&self) {
        self.obj().unregister_folder_reorder_section();
        self.obj().stop_workspace_watch();
        super::tree_loading::clear_all_dir_state(&self.obj());

        if let Some(popover) = self.context_menu.borrow_mut().take() {
            popover.unparent();
        }
        if let Some(popover) = self.header_context_menu.borrow_mut().take() {
            popover.unparent();
        }
        if let Some(popover) = self.peek_widgets.popover.borrow_mut().take() {
            popover.unparent();
        }
    }
}

impl WidgetImpl for LushtextWorkspaceSection {}
impl BoxImpl for LushtextWorkspaceSection {}
