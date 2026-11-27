// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the workspace section widget.
//!
//! Each section manages one workspace's file tree (GtkListView + TreeListModel),
//! context menus for files and the workspace header, and callback forwarding
//! to the parent sidebar.
//!
//! # Role: called presentation surface — **not** one of the five roles
//!
//! The section subclass's private state, template children, construction, and
//! disposal, including the runtime state structs the coordination roles read and
//! write. GTK subclass plumbing, not coordination.
//!
//! It owns no `policy.rs` and no `evidence.rs`, and it keeps every behavior obligation
//! stated below and in the workflow's matrix row.

use super::context_menus::FileContextMenuWiring;
pub(super) use super::context_menus::FileContextTarget;
use super::folder_execution::{
    ActiveFolderEmptyProbe, FolderEmptyProbeKey, PendingFolderEmptyProbe,
};
use super::peek_execution::{PeekSession, PeekWidgets};
use super::refresh_execution::RefreshRuntimeState;
use super::scan_execution::{ActiveChildScan, PendingChildScan, RetiringListing};
use super::watch::WatchRuntimeState;
use crate::ui::sidebar::policy::WorkspaceScanFlight;
use crate::ui::sidebar::seams::SidebarFileRowStateSnapshot;
use gtk4::gio;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, glib};
use lushtext_model::values::MirrorGeneration;
use lushtext_model::workspace::{FolderTreeEntry, WorkspaceFolderId, WorkspaceId};
use lushtext_services::workspace::file_tree::DirectoryRowState;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

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
    /// Viewport slice that keeps the list view virtualized inside the sidebar's
    /// outer scroller while its width follows the sidebar, not the deepest row.
    #[template_child]
    pub file_tree_slice: TemplateChild<gtk_lush_widgets::ViewportSliceBin>,
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
    pub expanded_paths: RefCell<HashSet<PathBuf>>,
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
    pub(crate) file_row_state_snapshot: RefCell<Rc<SidebarFileRowStateSnapshot>>,
    /// Scalar one-active plus one-latest ownership policy per materialized store.
    pub(super) child_scan_flights: RefCell<HashMap<usize, WorkspaceScanFlight>>,
    /// Admitted scan payload per store; strong GTK ownership stays in the worker handoff.
    pub(super) child_active_scans: RefCell<HashMap<usize, ActiveChildScan>>,
    /// Sole weak-store compact request waiting behind each active store scan.
    pub(super) child_pending_scans: RefCell<HashMap<usize, PendingChildScan>>,
    /// Current weak child-scan requests waiting for aggregate worker admission.
    pub(super) child_admission_scans: RefCell<HashMap<usize, PendingChildScan>>,
    /// Section lifetime invalidating every queued or admitted child scan.
    pub(super) child_scan_lifetime: Cell<u64>,
    /// Queued child stores returned by `build_children_model` awaiting the
    /// `Priority::HIGH` materialization drain.
    pub(super) pending_materializations:
        RefCell<VecDeque<super::materialization_execution::PendingMaterialization>>,
    /// Whether the `Priority::HIGH` materialization drain is currently armed.
    pub(super) materialization_drain_armed: Cell<bool>,
    /// Armed `Priority::HIGH` materialization drain idle source, removed on rebuild/dispose.
    pub(super) materialization_drain_source: RefCell<Option<glib::SourceId>>,
    /// Whether `dispose()` has run on this section widget.
    pub(super) disposed: Cell<bool>,
    /// One-active plus one-latest generation policy per configured-folder probe.
    pub(super) folder_empty_flights: RefCell<HashMap<FolderEmptyProbeKey, WorkspaceScanFlight>>,
    /// Admitted top-level emptiness probe per stable folder identity.
    pub(super) folder_empty_active: RefCell<HashMap<FolderEmptyProbeKey, ActiveFolderEmptyProbe>>,
    /// Sole weak-store/item emptiness request waiting behind each active probe.
    pub(super) folder_empty_pending: RefCell<HashMap<FolderEmptyProbeKey, PendingFolderEmptyProbe>>,
    /// Current weak emptiness probes waiting for aggregate worker admission.
    pub(super) folder_empty_admission:
        RefCell<HashMap<FolderEmptyProbeKey, PendingFolderEmptyProbe>>,
    /// Sole section wakeup that retries compact requests against aggregate admission.
    pub(super) workspace_scan_admission_source: RefCell<Option<glib::SourceId>>,
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
    pub child_row_mirror_generations: RefCell<HashMap<usize, MirrorGeneration>>,
    /// Scheduled large-reconciliation source owned with its scan token.
    pub child_reconcile_sources: RefCell<HashMap<usize, (Arc<AtomicBool>, glib::SourceId)>>,
    /// Ordered folder paths in the folder ListStore.
    pub folder_paths: RefCell<Vec<PathBuf>>,
    /// Ordered child paths per parent directory.
    pub child_paths: RefCell<HashMap<PathBuf, Vec<PathBuf>>>,
    /// The child store whose rows each parent's path caches project. A
    /// directory can have more than one store at once (GTK's expander probes
    /// `is_expandable()` with a store it discards, A22), so per-batch cache
    /// maintenance follows only the live, owning store
    /// (`bound-lists-by-budget-not-count` D6).
    pub child_cache_owners: RefCell<HashMap<PathBuf, usize>>,
    /// The child store each expanded directory row shows, held strongly so
    /// that collapsing the row only unlinks it. Without this hold GTK's
    /// `set_expanded(false)` drops the store's last reference and finalizes
    /// every listed item inside the collapse (ledger A91;
    /// `bound-lists-by-budget-not-count`, stage 4 review). Released to
    /// `retiring_child_stores` when the row collapses or is replaced.
    pub(super) retained_child_stores: RefCell<HashMap<PathBuf, gio::ListStore>>,
    /// Collapsed listings no one else holds, emptied in time-sliced turns
    /// and then released.
    pub(super) retiring_child_stores: RefCell<VecDeque<RetiringListing>>,
    /// The timeout source draining `retiring_child_stores`, while one is armed.
    pub(super) child_store_retirement_source: RefCell<Option<glib::SourceId>>,
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
    pub(super) peek_session: PeekSession,
    /// Debounced refresh request state for manual and automatic reloads.
    pub refresh_runtime: RefreshRuntimeState,
    /// Materialized-scope watcher plus GTK-side poll source for automatic refresh.
    pub watch_runtime: WatchRuntimeState,
}

// ObjectSubclass registers this Rust struct as LushtextWorkspaceSection in
// GLib's runtime type system; ParentType makes it behave as a GtkBox.
#[glib::object_subclass]
impl ObjectSubclass for LushtextWorkspaceSection {
    const NAME: &str = "LushtextWorkspaceSection";
    type Type = super::LushtextWorkspaceSection;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        gtk_lush_widgets::ViewportSliceBin::ensure_type();
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextWorkspaceSection {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: std::sync::OnceLock<Vec<glib::subclass::Signal>> =
            std::sync::OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("section-event")
                    .param_types([super::BoxedWorkspaceSectionEvent::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        self.file_tree_view.add_css_class("workspace-file-tree");
        super::row_factory::setup(self);
        self.obj().register_folder_reorder_section();
        super::context_menus::setup_file_context_menu(self);
        super::context_menus::setup_header_context_menu(self);
        super::context_menus::setup_header_double_click(self);
        self.obj().setup_peek();
        super::row_accessibility::apply_section_accessibility(self);

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
        self.disposed.set(true);
        self.obj().unregister_folder_reorder_section();
        self.obj().stop_workspace_watch();
        super::scan_execution::retire_all_dir_listings(&self.obj());
        // A disposed section drains nothing later: release what is retiring.
        if let Some(source) = self.child_store_retirement_source.take() {
            source.remove();
        }
        self.retiring_child_stores.borrow_mut().clear();

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

impl LushtextWorkspaceSection {
    /// The context-menu target's path and workspace-folder id, for the evidence surface.
    ///
    /// Returns owned values so `FileContextTarget` stays private to this directory —
    /// the canonical role home needs the two facts, not the type.
    pub(crate) fn context_target_evidence(&self) -> (Option<PathBuf>, Option<WorkspaceFolderId>) {
        let target = self.context_target.borrow();
        (
            target.as_ref().map(|target| target.path.clone()),
            target
                .as_ref()
                .and_then(|target| target.workspace_folder_id.clone()),
        )
    }
}
