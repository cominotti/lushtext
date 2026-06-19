// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the workspace section widget.
//!
//! Each section manages one workspace's file tree (GtkListView + TreeListModel),
//! context menus for files and the workspace header, and callback forwarding
//! to the parent sidebar.

use super::super::file_tree_item::FileTreeItem;
use super::icon_presentation;
use crate::model::workspace::{
    FolderTreeEntry, WorkspaceFolderId, WorkspaceFolderMoveDirection, WorkspaceId,
};
use crate::services::file_peek::PeekRequestToken;
use crate::services::notifications::NotificationSeverity;
use crate::services::workspace_watch::WorkspaceWatcher;
use crate::ui::accessibility::{self, RowAccessibility};
use crate::ui::sidebar::SidebarFileRowStateSnapshot;
use gtk_lush_settle::Debounce;
use gtk_lush_signals::SignalBag;
use gtk4::gio;
use gtk4::gio::prelude::ListModelExt;
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
const ROW_EXPANDED_ACCESSIBILITY_HOOK: &str = "workspace-row-expanded-accessibility-hook";

/// Cached position of a file tree item for O(1) model removal.
/// Stores the parent directory (or `None` for configured top-level rows) and
/// the index within the parent's `ListStore`.
#[derive(Debug, Clone)]
pub struct ItemLocation {
    pub parent_dir: Option<PathBuf>,
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
    /// Paths accumulated since the last refresh run.
    pub pending_paths: RefCell<HashSet<PathBuf>>,
    /// Whether the next refresh must rebuild the whole current section view.
    pub pending_full_reload: Cell<bool>,
    /// Whether the current scan burst should announce manual-refresh completion.
    pub manual_refresh_announcing: Cell<bool>,
    /// Last scan failure shown to the user so repeated auto-refresh attempts do
    /// not spam the status bar while a folder remains unreadable.
    pub last_reported_error: RefCell<Option<String>>,
}

/// Live filesystem-watch wiring for one workspace section.
#[derive(Default)]
pub struct WatchRuntimeState {
    /// Backend watcher for the current materialized folder scopes, if active.
    pub watcher: RefCell<Option<WorkspaceWatcher>>,
    /// Generation counter dropping stale deferred watcher startups after folders
    /// or drill-down scope change again before the scheduled start runs.
    pub start_generation: Cell<u32>,
    /// Pending deferred startup source so startup/dispose can cancel it.
    pub start_source_id: RefCell<Option<glib::SourceId>>,
    /// GTK main-loop source that polls the watcher receiver without blocking.
    pub poll_source_id: RefCell<Option<glib::SourceId>>,
    /// Last watcher error shown to the user so repeated backend failures do not
    /// spam the status bar every poll tick.
    pub last_reported_error: RefCell<Option<String>>,
}

// CompositeTemplate loads this widget's XML from the compiled GResource.
// Each TemplateChild is bound by init_template() before constructed() runs.
// GObject methods always take &self; Cell/RefCell provide interior mutability.
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
    /// Path of the item under the right-click context menu. Set on gesture
    /// press, read by action handlers (rename, delete, new file).
    pub context_path: RefCell<Option<PathBuf>>,
    /// Whether the context-menu target is a directory.
    pub context_is_dir: Cell<bool>,
    /// Stable folder id when the context target is a persisted top-level folder row.
    pub context_workspace_folder_id: RefCell<Option<WorkspaceFolderId>>,
    /// The TreeExpander widget for the context-menu target row. Needed to
    /// swap the label for an inline rename entry.
    pub context_expander: RefCell<Option<gtk4::TreeExpander>>,
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
        self.setup_factory();
        self.obj().register_folder_reorder_section();
        self.setup_file_context_menu();
        self.setup_header_context_menu();
        self.setup_header_double_click();
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

        // Wire drilldown back button
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
    /// Set up the list item factory for rendering file tree rows.
    ///
    /// `SignalListItemFactory` is GTK4's way of creating and recycling row widgets:
    /// - `connect_setup`: creates the row's widget hierarchy (reused across items)
    /// - `connect_bind`: updates row widgets to reflect the current data item
    /// - `connect_unbind`: cleans up item-specific state for row recycling
    fn setup_factory(&self) {
        let factory = gtk4::SignalListItemFactory::new();

        let section_weak_for_setup = self.obj().downgrade();
        factory.connect_setup(move |_factory, list_item| {
            // Factory callbacks receive generic GObjects from GTK, so
            // downcast_ref checks the runtime type before using ListItem APIs.
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("item is ListItem");

            let overlay = gtk4::Overlay::new();
            overlay.add_css_class("workspace-folder-dnd-surface");
            overlay.add_css_class("workspace-file-row-state-surface");

            // GTK4 trees use TreeListModel for hierarchy, ListView for row
            // recycling, and TreeExpander for indentation/disclosure; each bind
            // reattaches the expander to the currently recycled TreeListRow.
            let expander = gtk4::TreeExpander::new();
            let content_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            content_box.set_halign(gtk4::Align::Start);

            let drag_handle = gtk4::Button::from_icon_name("list-drag-handle-symbolic");
            drag_handle.set_valign(gtk4::Align::Center);
            drag_handle.set_focusable(false);
            drag_handle.set_tooltip_text(Some("Reorder Folder"));
            drag_handle.set_visible(false);
            drag_handle.add_css_class("flat");
            drag_handle.add_css_class("circular");
            drag_handle.add_css_class("workspace-folder-drag-handle");
            accessibility::set_labelled_description(
                &drag_handle,
                "Reorder Folder",
                "Drag or use the folder context menu to reorder this workspace folder",
            );
            accessibility::set_hidden(&drag_handle, true);
            accessibility::set_disabled(&drag_handle, true);

            let open_indicator = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            open_indicator.add_css_class("workspace-file-open-indicator");
            open_indicator.set_valign(gtk4::Align::Center);
            open_indicator.set_can_target(false);
            open_indicator.set_focusable(false);

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
            accessibility::set_labelled_description(
                &focus_btn,
                "Focus Folder",
                "Temporarily show this folder as the root of the workspace tree",
            );

            let list_item_weak = list_item.downgrade();
            let overlay_weak = overlay.downgrade();
            focus_btn.connect_clicked(move |_| {
                if super::dnd::folder_reorder_drag_is_active() {
                    return;
                }
                if let Some(list_item) = list_item_weak.upgrade()
                    && let Some(overlay) = overlay_weak.upgrade()
                    && let Some(tree_row) = list_item.item().and_downcast::<gtk4::TreeListRow>()
                    && let Some(file_item) = tree_row.item().and_downcast::<FileTreeItem>()
                    && let Some(path) = file_item.path()
                {
                    // Factory setup only has recycled row widgets, so resolve
                    // the owning section at click time from the live widget tree.
                    let mut current: Option<gtk4::Widget> = Some(overlay.upcast::<gtk4::Widget>());
                    while let Some(w) = current {
                        if let Some(section) = w.downcast_ref::<super::LushtextWorkspaceSection>() {
                            section.focus_folder(&path);
                            break;
                        }
                        current = w.parent();
                    }
                }
            });

            content_box.append(&drag_handle);
            content_box.append(&open_indicator);
            content_box.append(&icon);
            content_box.append(&label);
            expander.set_child(Some(&content_box));

            let drop_target = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            drop_target.add_css_class("workspace-folder-drop-target");
            drop_target.set_can_target(false);
            drop_target.set_focusable(false);
            drop_target.set_halign(gtk4::Align::Fill);
            drop_target.set_valign(gtk4::Align::Start);
            drop_target.set_height_request(2);
            drop_target.set_visible(false);
            accessibility::set_hidden(&drop_target, true);
            accessibility::set_disabled(&drop_target, true);

            let drop_shield = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            drop_shield.add_css_class("workspace-folder-dnd-shield");
            drop_shield.set_can_target(false);
            drop_shield.set_focusable(false);
            drop_shield.set_halign(gtk4::Align::Fill);
            drop_shield.set_valign(gtk4::Align::Fill);
            drop_shield.set_hexpand(true);
            drop_shield.set_vexpand(true);
            accessibility::set_hidden(&drop_shield, true);
            accessibility::set_disabled(&drop_shield, true);

            let drop_indicator = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            drop_indicator.add_css_class("workspace-folder-drop-indicator");
            drop_indicator.set_can_target(false);
            drop_indicator.set_focusable(false);
            drop_indicator.set_halign(gtk4::Align::Fill);
            drop_indicator.set_valign(gtk4::Align::Center);
            drop_indicator.set_hexpand(true);
            drop_indicator.set_height_request(2);
            accessibility::set_hidden(&drop_indicator, true);
            accessibility::set_disabled(&drop_indicator, true);
            drop_target.append(&drop_indicator);

            overlay.set_child(Some(&expander));
            // Reorder DnD hover belongs to the transparent full-row shield;
            // the separate 2px indicator surface only paints the insertion line.
            overlay.add_overlay(&drop_shield);
            overlay.set_measure_overlay(&drop_shield, false);
            overlay.add_overlay(&drop_target);
            overlay.set_measure_overlay(&drop_target, false);
            overlay.add_overlay(&focus_btn);
            overlay.set_measure_overlay(&focus_btn, false);

            let motion = gtk4::EventControllerMotion::new();
            let btn_enter = focus_btn.clone();
            motion.connect_enter(move |_, _, _| {
                if super::dnd::folder_reorder_drag_is_active() {
                    btn_enter.set_visible(false);
                    return;
                }
                if btn_enter.has_css_class("can-focus") {
                    btn_enter.set_visible(true);
                }
            });
            let btn_leave = focus_btn;
            motion.connect_leave(move |_| {
                btn_leave.set_visible(false);
            });
            overlay.add_controller(motion);

            if let Some(section) = section_weak_for_setup.upgrade() {
                section.install_folder_reorder_dnd(
                    list_item,
                    &drag_handle,
                    &overlay,
                    &drop_shield,
                    &drop_target,
                );
            }

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
            super::dnd::reset_reorder_row_for_bind(&overlay);
            clear_expanded_accessibility_hook(&overlay);

            let focus_btn = focus_button_for_overlay(&overlay).expect("focus_btn missing");

            if let Some(file_item) = tree_row.item().and_downcast::<FileTreeItem>() {
                let content_box = expander
                    .child()
                    .and_downcast::<gtk4::Box>()
                    .expect("expander child is Box");

                let drag_handle = content_box
                    .first_child()
                    .and_downcast::<gtk4::Button>()
                    .expect("first child is drag handle");

                let icon = drag_handle
                    .next_sibling()
                    .and_downcast::<gtk4::Widget>()
                    .expect("second child is open indicator")
                    .next_sibling()
                    .and_downcast::<gtk4::Image>()
                    .expect("third child is Image");

                let label = icon
                    .next_sibling()
                    .and_downcast::<gtk4::Label>()
                    .expect("fourth child is Label");

                icon_presentation::icon_for_file_item(&file_item).apply_to(&icon);
                let display_name = file_item.name();

                if file_item.is_empty() == Some(true) {
                    label.set_markup(&format!(
                        "{} <span alpha=\"60%\"><i>(Empty)</i></span>",
                        glib::markup_escape_text(&display_name)
                    ));
                } else {
                    label.set_use_markup(false);
                    label.set_label(&display_name);
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

                let show_reorder_handle = section_weak.upgrade().is_some_and(|section| {
                    super::dnd::workspace_folder_reorder_handle_should_show(&section, &tree_row)
                });
                drag_handle.set_visible(show_reorder_handle);
                drag_handle.set_sensitive(show_reorder_handle);

                if file_item.is_dir()
                    && !file_item.is_placeholder()
                    && let Some(section) = section_weak.upgrade()
                    && let Some(path) = file_item.path()
                {
                    // Tree rows persist beyond one ListItem binding, but the
                    // signal is still tied to the visible binding and
                    // disconnected on unbind. That keeps auto-refresh scoped to
                    // directories the user has actually expanded.
                    let has_expanded_hook =
                        // SAFETY: the private key stores only this row's
                        // expansion handler id and is cleared in unbind.
                        unsafe {
                            tree_row
                                .data::<SignalBag>("workspace-watch-expanded-hook")
                        }
                            .is_some();
                    if !has_expanded_hook {
                        let section_weak = section.downgrade();
                        let handler_id =
                            tree_row.connect_notify_local(Some("expanded"), move |row, _| {
                                if super::dnd::expanded_watch_should_be_suppressed(row) {
                                    return;
                                }
                                let section_weak = section_weak.clone();
                                glib::idle_add_local_once(move || {
                                    if let Some(section) = section_weak.upgrade() {
                                        section.restart_workspace_watch();
                                    }
                                });
                            });
                        let signals = SignalBag::new();
                        signals.track(&tree_row, handler_id);
                        // SAFETY: this private signal bag is cleared and
                        // cleared in unbind; no external code reads it.
                        unsafe {
                            tree_row.set_data("workspace-watch-expanded-hook", signals);
                        }
                    }
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

                // New file/folder rows carry a one-shot flag so rename starts
                // only after GTK has bound the recycled row widget.
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

                if let Some(section) = section_weak.upgrade() {
                    apply_file_tree_row_accessibility(FileTreeRowAccessibilityTarget {
                        overlay: &overlay,
                        drag_handle: &drag_handle,
                        focus_btn: &focus_btn,
                        file_item: &file_item,
                        tree_row: &tree_row,
                        section: &section,
                        position: list_item.position(),
                        show_reorder_handle,
                        show_focus,
                    });
                    install_expanded_accessibility_hook(&overlay, &tree_row, &file_item, &section);
                    super::sync_file_row_state_for_overlay(&section, &overlay);
                } else {
                    accessibility::clear_row_accessibility(&overlay);
                    accessibility::set_expanded(&overlay, None);
                    super::reset_file_row_state_for_overlay(&overlay);
                }
            } else {
                clear_expanded_accessibility_hook(&overlay);
                accessibility::clear_row_accessibility(&overlay);
                accessibility::set_expanded(&overlay, None);
                super::reset_file_row_state_for_overlay(&overlay);
            }
        });

        let section_weak = self.obj().downgrade();
        factory.connect_unbind(move |_factory, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("item is ListItem");

            let tree_row = list_item.item().and_downcast::<gtk4::TreeListRow>();

            if let Some(overlay) = list_item.child().and_downcast::<gtk4::Overlay>()
                && let Some(expander) = overlay.child().and_downcast::<gtk4::TreeExpander>()
            {
                expander.set_list_row(None::<&gtk4::TreeListRow>);
                if let Some(content_box) = expander.child().and_downcast::<gtk4::Box>()
                    && let Some(drag_handle) =
                        content_box.first_child().and_downcast::<gtk4::Button>()
                    && let Some(open_indicator) =
                        drag_handle.next_sibling().and_downcast::<gtk4::Widget>()
                    && let Some(icon) = open_indicator.next_sibling().and_downcast::<gtk4::Image>()
                    && let Some(label) = icon.next_sibling().and_downcast::<gtk4::Label>()
                {
                    // Recycled ListItem widgets must leave no row-local editing
                    // controls or markup mode behind for the next bound item.
                    let mut child = label.next_sibling();
                    while let Some(sibling) = child {
                        child = sibling.next_sibling();
                        if sibling.downcast_ref::<gtk4::Entry>().is_some() {
                            content_box.remove(&sibling);
                        }
                    }
                    label.set_visible(true);
                    label.set_use_markup(false);
                    drag_handle.set_visible(false);
                    drag_handle.set_sensitive(false);
                    accessibility::set_hidden(&drag_handle, true);
                    accessibility::set_disabled(&drag_handle, true);
                    content_box.set_margin_end(0);
                }

                super::reset_file_row_state_for_overlay(&overlay);
                super::dnd::reset_reorder_row_for_unbind(&overlay);
                clear_expanded_accessibility_hook(&overlay);
                accessibility::clear_row_accessibility(&overlay);
                accessibility::set_expanded(&overlay, None);
                accessibility::set_disabled(&overlay, false);

                if let Some(focus_btn) = focus_button_for_overlay(&overlay) {
                    accessibility::set_labelled_description(
                        &focus_btn,
                        "Focus Folder",
                        "Temporarily show this folder as the root of the workspace tree",
                    );
                    accessibility::set_hidden(&focus_btn, true);
                }

                if let Some(section) = section_weak.upgrade() {
                    let context_matches = section
                        .imp()
                        .context_expander
                        .borrow()
                        .as_ref()
                        .is_some_and(|context_expander| context_expander == &expander);
                    if context_matches {
                        section.imp().context_expander.borrow_mut().take();
                        section.imp().context_path.borrow_mut().take();
                        section
                            .imp()
                            .context_workspace_folder_id
                            .borrow_mut()
                            .take();
                    }
                }
            }

            let Some(tree_row) = tree_row else { return };
            let Some(file_item) = tree_row.item().and_downcast::<FileTreeItem>() else {
                return;
            };

            // SAFETY: mirrors set_data("workspace-watch-expanded-hook") in
            // connect_bind above. Clearing on unbind keeps recycled rows from
            // retaining section callbacks beyond their visible binding.
            unsafe {
                if let Some(signals) =
                    tree_row.steal_data::<SignalBag>("workspace-watch-expanded-hook")
                {
                    signals.clear();
                }
            }

            if file_item.is_dir()
                && let Some(section) = section_weak.upgrade()
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

        let popover = gtk4::Popover::new();
        popover.set_parent(&*self.file_tree_view);
        popover.set_has_arrow(false);
        popover.set_halign(gtk4::Align::Start);
        let menu_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        menu_box.add_css_class("context-menu");
        popover.set_child(Some(&menu_box));
        accessibility::set_role(&popover, gtk4::AccessibleRole::Menu);
        accessibility::set_labelled_description(
            &popover,
            "File tree context menu",
            "Actions for the selected file or folder row",
        );
        *self.context_menu_box.borrow_mut() = Some(menu_box);
        *self.context_menu.borrow_mut() = Some(popover);

        // Register actions under the "section" prefix. Context menu items
        // reference them as "section.new-file", "section.rename", etc.
        // GTK resolves these by walking up the widget tree for the prefix.
        let action_group = gio::SimpleActionGroup::new();

        let focus_folder_action = gio::SimpleAction::new("focus-folder", None);
        let section_weak = obj.downgrade();
        focus_folder_action.connect_activate(move |_, _| {
            let Some(section) = section_weak.upgrade() else {
                return;
            };
            let path = section.imp().context_path.borrow().clone();
            let is_dir = section.imp().context_is_dir.get();
            if let Some(path) = path
                && is_dir
            {
                popdown_context_popovers(&section);
                section.imp().context_expander.borrow_mut().take();
                section.imp().context_path.borrow_mut().take();
                section
                    .imp()
                    .context_workspace_folder_id
                    .borrow_mut()
                    .take();
                section.focus_folder(&path);
            }
        });
        action_group.add_action(&focus_folder_action);

        let local_history_action = gio::SimpleAction::new("local-history", None);
        let section_weak = obj.downgrade();
        local_history_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade()
                && let Some(path) = section.imp().context_path.borrow().clone()
                && !section.imp().context_is_dir.get()
            {
                popdown_context_popovers(&section);
                section.notify_local_history_requested(&path);
            }
        });
        action_group.add_action(&local_history_action);

        let document_note_action = gio::SimpleAction::new("document-note", None);
        let section_weak = obj.downgrade();
        document_note_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade()
                && let Some(path) = section.imp().context_path.borrow().clone()
                && !section.imp().context_is_dir.get()
            {
                popdown_context_popovers(&section);
                section.notify_document_note_requested(&path);
            }
        });
        action_group.add_action(&document_note_action);

        let folder_note_action = gio::SimpleAction::new("folder-note", None);
        let section_weak = obj.downgrade();
        folder_note_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade()
                && let Some(path) = section.imp().context_path.borrow().clone()
                && section.imp().context_workspace_folder_id.borrow().is_some()
            {
                popdown_context_popovers(&section);
                section.notify_folder_note_for_folder_requested(&path);
            }
        });
        action_group.add_action(&folder_note_action);

        let move_folder_up_action = gio::SimpleAction::new("move-folder-up", None);
        let section_weak = obj.downgrade();
        move_folder_up_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade()
                && let Some(folder_id) = section.imp().context_workspace_folder_id.borrow().clone()
            {
                popdown_context_popovers(&section);
                section
                    .notify_reorder_folder_requested(&folder_id, WorkspaceFolderMoveDirection::Up);
            }
        });
        action_group.add_action(&move_folder_up_action);

        let move_folder_down_action = gio::SimpleAction::new("move-folder-down", None);
        let section_weak = obj.downgrade();
        move_folder_down_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade()
                && let Some(folder_id) = section.imp().context_workspace_folder_id.borrow().clone()
            {
                popdown_context_popovers(&section);
                section.notify_reorder_folder_requested(
                    &folder_id,
                    WorkspaceFolderMoveDirection::Down,
                );
            }
        });
        action_group.add_action(&move_folder_down_action);

        let remove_folder_action = gio::SimpleAction::new("remove-folder", None);
        let section_weak = obj.downgrade();
        remove_folder_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                popdown_context_popovers(&section);
                section.show_remove_folder_confirmation();
            }
        });
        action_group.add_action(&remove_folder_action);

        let new_file_action = gio::SimpleAction::new("new-file", None);
        let section_weak = obj.downgrade();
        new_file_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                popdown_context_popovers(&section);
                section.create_new_item(false);
            }
        });
        action_group.add_action(&new_file_action);

        let new_dir_action = gio::SimpleAction::new("new-dir", None);
        let section_weak = obj.downgrade();
        new_dir_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                popdown_context_popovers(&section);
                section.create_new_item(true);
            }
        });
        action_group.add_action(&new_dir_action);

        let rename_action = gio::SimpleAction::new("rename", None);
        let section_weak = obj.downgrade();
        rename_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                popdown_context_popovers(&section);
                section.begin_rename();
            }
        });
        action_group.add_action(&rename_action);

        let delete_action = gio::SimpleAction::new("delete", None);
        let section_weak = obj.downgrade();
        delete_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                popdown_context_popovers(&section);
                section.show_delete_confirmation();
            }
        });
        action_group.add_action(&delete_action);

        obj.insert_action_group("section", Some(&action_group));

        let context_menu_wiring = FileContextMenuWiring {
            focus_folder_action,
            local_history_action,
            document_note_action,
            folder_note_action,
            move_folder_up_action,
            move_folder_down_action,
            remove_folder_action,
        };
        *self.context_menu_wiring.borrow_mut() = Some(context_menu_wiring.clone());

        // Attach the gesture to the stable list view; press-time picking
        // resolves the current recycled row before opening the menu.
        let gesture = gtk4::GestureClick::new();
        gesture.set_button(3);

        let section_weak = obj.downgrade();
        let pointer_wiring = context_menu_wiring.clone();
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
            #[expect(
                clippy::cast_possible_truncation,
                reason = "Pointer event coordinates are already bounded by GTK widget geometry before converting to i32"
            )]
            let pointing_to = gdk4::Rectangle::new(x as i32, y as i32, 1, 1);
            show_file_context_menu_for_row(
                &section,
                &expander,
                &tree_row,
                &file_item,
                &pointer_wiring,
                pointing_to,
            );
        });

        self.file_tree_view.add_controller(gesture);

        let key_controller = gtk4::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let section_weak = obj.downgrade();
        let keyboard_wiring = context_menu_wiring;
        key_controller.connect_key_pressed(move |_, key, _, state| {
            if !file_tree_context_menu_key(key, state) {
                return glib::Propagation::Proceed;
            }
            let Some(section) = section_weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if show_file_context_menu_for_selection(&section, &keyboard_wiring) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.file_tree_view.add_controller(key_controller);
    }

    /// Build right-click context menu for the workspace header.
    fn setup_header_context_menu(&self) {
        let obj = self.obj();

        let popover = gtk4::Popover::new();
        popover.set_parent(&*self.header_box);
        popover.set_has_arrow(false);
        popover.set_halign(gtk4::Align::Start);
        let menu_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        menu_box.add_css_class("context-menu");
        rebuild_popover_action_menu(&popover, &menu_box, &[HEADER_CONTEXT_MENU_SPECS]);
        popover.set_child(Some(&menu_box));
        accessibility::set_role(&popover, gtk4::AccessibleRole::Menu);
        accessibility::set_labelled_description(
            &popover,
            "Workspace context menu",
            "Actions for this workspace section",
        );
        *self.header_context_menu_box.borrow_mut() = Some(menu_box);
        *self.header_context_menu.borrow_mut() = Some(popover.clone());

        let action_group = gio::SimpleActionGroup::new();

        let folder_note_action = gio::SimpleAction::new("open-folder-note", None);
        let section_weak = obj.downgrade();
        folder_note_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                popdown_context_popovers(&section);
                section.notify_folder_note_requested();
            }
        });
        action_group.add_action(&folder_note_action);

        let add_folder_action = gio::SimpleAction::new("add-folder", None);
        let section_weak = obj.downgrade();
        add_folder_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                popdown_context_popovers(&section);
                section.notify_add_folder_requested();
            }
        });
        action_group.add_action(&add_folder_action);

        let rename_action = gio::SimpleAction::new("rename", None);
        let section_weak = obj.downgrade();
        rename_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                popdown_context_popovers(&section);
                section.notify_rename_workspace_requested();
            }
        });
        action_group.add_action(&rename_action);

        let unlist_action = gio::SimpleAction::new("unlist", None);
        let section_weak = obj.downgrade();
        unlist_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                popdown_context_popovers(&section);
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
            #[expect(
                clippy::cast_possible_truncation,
                reason = "Pointer event coordinates are already bounded by GTK widget geometry before converting to i32"
            )]
            popover_ref.set_pointing_to(Some(&gdk4::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover_ref.popup();
        });

        self.header_box.add_controller(gesture);

        let key_controller = gtk4::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let popover_ref = popover;
        key_controller.connect_key_pressed(move |controller, key, _, state| {
            if !file_tree_context_menu_key(key, state) {
                return glib::Propagation::Proceed;
            }
            let Some(header_box) = controller.widget() else {
                return glib::Propagation::Proceed;
            };
            popover_ref.set_pointing_to(Some(&gdk4::Rectangle::new(
                0,
                0,
                header_box.width().max(1),
                header_box.height().max(1),
            )));
            popover_ref.popup();
            glib::Propagation::Stop
        });
        self.header_box.add_controller(key_controller);
    }

    /// Set up double-click gesture on the workspace header to collapse/expand the section body.
    fn setup_header_double_click(&self) {
        let obj = self.obj();
        let gesture = gtk4::GestureClick::new();

        let section_weak = obj.downgrade();
        gesture.connect_pressed(move |gesture, n_press, x, y| {
            if n_press == 2
                && click_target_is_header_background(gesture, x, y)
                && let Some(section) = section_weak.upgrade()
            {
                section.toggle_section_body_collapsed();
            }
        });

        self.header_box.add_controller(gesture);
    }
}

/// Return true only for header-background clicks, leaving child buttons to own their actions.
fn click_target_is_header_background(gesture: &gtk4::GestureClick, x: f64, y: f64) -> bool {
    let Some(widget) = gesture.widget() else {
        return false;
    };
    let Some(target) = widget.pick(x, y, gtk4::PickFlags::DEFAULT) else {
        return true;
    };
    target.ancestor(gtk4::Button::static_type()).is_none()
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

#[derive(Clone)]
pub(super) struct FileContextMenuWiring {
    focus_folder_action: gio::SimpleAction,
    local_history_action: gio::SimpleAction,
    document_note_action: gio::SimpleAction,
    folder_note_action: gio::SimpleAction,
    move_folder_up_action: gio::SimpleAction,
    move_folder_down_action: gio::SimpleAction,
    remove_folder_action: gio::SimpleAction,
}

#[derive(Clone, Copy)]
struct PopoverMenuActionSpec {
    id: &'static str,
    label: &'static str,
    action: &'static str,
    description: &'static str,
}

const FILE_NAV_CONTEXT_MENU_SPECS: &[PopoverMenuActionSpec] = &[
    PopoverMenuActionSpec {
        id: "file-focus-folder",
        label: "Focus Folder",
        action: "section.focus-folder",
        description: "Temporarily show this folder as the root of the workspace tree",
    },
    PopoverMenuActionSpec {
        id: "file-local-history",
        label: "Local History…",
        action: "section.local-history",
        description: "Open local history for this file",
    },
    PopoverMenuActionSpec {
        id: "file-document-note",
        label: "Open Document Note…",
        action: "section.document-note",
        description: "Open the note attached to this document",
    },
];
const FILE_CREATE_CONTEXT_MENU_SPECS: &[PopoverMenuActionSpec] = &[
    PopoverMenuActionSpec {
        id: "file-new-file",
        label: "New File",
        action: "section.new-file",
        description: "Create a new file in this folder",
    },
    PopoverMenuActionSpec {
        id: "file-new-folder",
        label: "New Folder",
        action: "section.new-dir",
        description: "Create a new folder in this folder",
    },
];
const FILE_EDIT_CONTEXT_MENU_SPECS: &[PopoverMenuActionSpec] = &[
    PopoverMenuActionSpec {
        id: "file-rename",
        label: "Rename",
        action: "section.rename",
        description: "Rename the selected file or folder",
    },
    PopoverMenuActionSpec {
        id: "file-delete",
        label: "Delete",
        action: "section.delete",
        description: "Delete the selected file or folder after confirmation",
    },
];
const FOLDER_NOTE_CONTEXT_MENU_SPECS: &[PopoverMenuActionSpec] = &[PopoverMenuActionSpec {
    id: "folder-open-note",
    label: "Open Folder Note…",
    action: "section.folder-note",
    description: "Open the note attached to this workspace folder",
}];
const FOLDER_MEMBERSHIP_CONTEXT_MENU_SPECS: &[PopoverMenuActionSpec] = &[
    PopoverMenuActionSpec {
        id: "folder-move-up",
        label: "Move Up",
        action: "section.move-folder-up",
        description: "Move this folder earlier in the workspace",
    },
    PopoverMenuActionSpec {
        id: "folder-move-down",
        label: "Move Down",
        action: "section.move-folder-down",
        description: "Move this folder later in the workspace",
    },
    PopoverMenuActionSpec {
        id: "folder-remove",
        label: "Remove from Workspace",
        action: "section.remove-folder",
        description: "Remove this folder from the workspace without deleting it from disk",
    },
];
const HEADER_CONTEXT_MENU_SPECS: &[PopoverMenuActionSpec] = &[
    PopoverMenuActionSpec {
        id: "header-add-folder",
        label: "Add Folder…",
        action: "ws-header.add-folder",
        description: "Add a folder to this workspace",
    },
    PopoverMenuActionSpec {
        id: "header-open-folder-note",
        label: "Open Folder Note…",
        action: "ws-header.open-folder-note",
        description: "Open the note attached to this workspace",
    },
    PopoverMenuActionSpec {
        id: "header-rename",
        label: "Rename Workspace",
        action: "ws-header.rename",
        description: "Rename this workspace",
    },
    PopoverMenuActionSpec {
        id: "header-remove",
        label: "Remove Workspace",
        action: "ws-header.unlist",
        description: "Remove this workspace after confirmation",
    },
];

fn rebuild_popover_action_menu(
    popover: &gtk4::Popover,
    menu_box: &gtk4::Box,
    groups: &[&[PopoverMenuActionSpec]],
) {
    while let Some(child) = menu_box.first_child() {
        menu_box.remove(&child);
    }

    for (index, specs) in groups.iter().enumerate() {
        if index > 0 {
            let separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
            separator.set_margin_top(4);
            separator.set_margin_bottom(4);
            menu_box.append(&separator);
        }

        for spec in *specs {
            menu_box.append(&popover_action_button(popover, spec));
        }
    }
}

fn popover_action_button(popover: &gtk4::Popover, spec: &PopoverMenuActionSpec) -> gtk4::Button {
    let button = gtk4::Button::with_label(spec.label);
    button.add_css_class("flat");
    button.add_css_class("model");
    button.set_action_name(Some(spec.action));
    button.set_halign(gtk4::Align::Fill);
    button.set_hexpand(true);
    button.set_widget_name(spec.id);
    accessibility::set_role(&button, gtk4::AccessibleRole::MenuItem);
    accessibility::set_labelled_description(&button, spec.label, spec.description);

    let popover_weak = popover.downgrade();
    button.connect_clicked(move |_| {
        let popover_weak = popover_weak.clone();
        glib::idle_add_local_once(move || {
            if let Some(popover) = popover_weak.upgrade() {
                popover.popdown();
            }
        });
    });

    button
}

fn popdown_context_popovers(section: &super::LushtextWorkspaceSection) {
    if let Some(popover) = section.imp().context_menu.borrow().as_ref() {
        popover.popdown();
    }
    if let Some(popover) = section.imp().header_context_menu.borrow().as_ref() {
        popover.popdown();
    }
}

fn file_tree_context_menu_key(key: gtk4::gdk::Key, state: gtk4::gdk::ModifierType) -> bool {
    key == gtk4::gdk::Key::Menu
        || (key == gtk4::gdk::Key::F10 && state.contains(gtk4::gdk::ModifierType::SHIFT_MASK))
}

fn show_file_context_menu_for_selection(
    section: &super::LushtextWorkspaceSection,
    wiring: &FileContextMenuWiring,
) -> bool {
    let Some(selection) = section
        .imp()
        .file_tree_view
        .model()
        .and_downcast::<gtk4::SingleSelection>()
    else {
        return false;
    };
    if selection.selected() == gtk4::INVALID_LIST_POSITION {
        return false;
    }
    let Some(tree_row) = selection
        .selected_item()
        .and_downcast::<gtk4::TreeListRow>()
    else {
        return false;
    };
    let Some(file_item) = tree_row.item().and_downcast::<FileTreeItem>() else {
        return false;
    };
    let Some((expander, pointing_to)) =
        realized_expander_and_bounds_for_tree_row(section, &tree_row)
    else {
        section.imp().file_tree_view.scroll_to(
            selection.selected(),
            gtk4::ListScrollFlags::FOCUS,
            None,
        );
        return false;
    };

    show_file_context_menu_for_row(
        section,
        &expander,
        &tree_row,
        &file_item,
        wiring,
        pointing_to,
    )
}

impl super::LushtextWorkspaceSection {
    /// Open the file-tree context menu for the current selection.
    ///
    /// This reuses the same menu wiring as pointer and keyboard handlers so
    /// automation-opened menus stay behaviorally identical to user-opened ones.
    pub(crate) fn show_selected_file_context_menu(&self) -> bool {
        let Some(wiring) = self.imp().context_menu_wiring.borrow().clone() else {
            return false;
        };
        show_file_context_menu_for_selection(self, &wiring)
    }

    /// Open the workspace-header context menu at the header bounds.
    ///
    /// The header can be focused through a child button, but automation also
    /// needs a direct menu-open path when synthetic key delivery is unavailable.
    pub(crate) fn show_header_context_menu(&self) -> bool {
        let imp = self.imp();
        let Some(popover) = imp.header_context_menu.borrow().clone() else {
            return false;
        };
        popover.set_pointing_to(Some(&gdk4::Rectangle::new(
            0,
            0,
            imp.header_box.width().max(1),
            imp.header_box.height().max(1),
        )));
        popover.popup();
        true
    }
}

fn realized_expander_and_bounds_for_tree_row(
    section: &super::LushtextWorkspaceSection,
    target_row: &gtk4::TreeListRow,
) -> Option<(gtk4::TreeExpander, gdk4::Rectangle)> {
    let list_view = section.imp().file_tree_view.clone();
    let mut child = list_view.first_child();
    while let Some(row_widget) = child {
        let next = row_widget.next_sibling();
        if let Some(overlay) = row_widget.first_child().and_downcast::<gtk4::Overlay>()
            && let Some(expander) = overlay.child().and_downcast::<gtk4::TreeExpander>()
            && expander.list_row().as_ref() == Some(target_row)
            && let Some(bounds) = row_widget.compute_bounds(&list_view)
        {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "Popover anchor geometry comes from GTK allocation data that already lives in i32 widget coordinates"
            )]
            let pointing_to = gdk4::Rectangle::new(
                bounds.x().round() as i32,
                bounds.y().round() as i32,
                bounds.width().max(1.0).round() as i32,
                bounds.height().max(1.0).round() as i32,
            );
            return Some((expander, pointing_to));
        }
        child = next;
    }
    None
}

fn show_file_context_menu_for_row(
    section: &super::LushtextWorkspaceSection,
    expander: &gtk4::TreeExpander,
    tree_row: &gtk4::TreeListRow,
    file_item: &FileTreeItem,
    wiring: &FileContextMenuWiring,
    pointing_to: gdk4::Rectangle,
) -> bool {
    let Some(path) = file_item.path() else {
        return false;
    };

    let imp = section.imp();
    let workspace_folder_id = file_item.workspace_folder_id();
    *imp.context_path.borrow_mut() = Some(path);
    imp.context_is_dir.set(file_item.is_dir());
    *imp.context_workspace_folder_id.borrow_mut() = workspace_folder_id.clone();
    *imp.context_expander.borrow_mut() = Some(expander.clone());

    let is_workspace_folder = workspace_folder_id.is_some();
    wiring
        .focus_folder_action
        .set_enabled(file_item.is_dir() && !file_item.is_placeholder() && tree_row.depth() > 0);
    // Avoid filesystem metadata checks on the context-menu path; the
    // window-level local-history workflow validates file size on activation
    // and reports a warning if the file is too large.
    let local_history_enabled = !file_item.is_dir() && !file_item.is_placeholder();
    wiring
        .local_history_action
        .set_enabled(local_history_enabled);
    wiring
        .document_note_action
        .set_enabled(!file_item.is_dir() && !file_item.is_placeholder());
    wiring.folder_note_action.set_enabled(is_workspace_folder);
    wiring.remove_folder_action.set_enabled(is_workspace_folder);
    let (can_move_up, can_move_down) = workspace_folder_id
        .as_ref()
        .map_or((false, false), |folder_id| {
            section.workspace_folder_move_availability(folder_id)
        });
    wiring.move_folder_up_action.set_enabled(can_move_up);
    wiring.move_folder_down_action.set_enabled(can_move_down);

    let popover = imp.context_menu.borrow().clone();
    let menu_box = imp.context_menu_box.borrow().clone();
    if let (Some(popover), Some(menu_box)) = (popover, menu_box) {
        let item_kind = if is_workspace_folder {
            "Workspace folder"
        } else if file_item.is_dir() {
            "Folder"
        } else {
            "File"
        };
        let display_name = file_item.name();
        accessibility::set_labelled_description(
            &popover,
            &format!("{item_kind} actions for {display_name}"),
            "Context actions for the selected workspace file-tree row",
        );
        if is_workspace_folder {
            rebuild_popover_action_menu(
                &popover,
                &menu_box,
                &[
                    FOLDER_NOTE_CONTEXT_MENU_SPECS,
                    FOLDER_MEMBERSHIP_CONTEXT_MENU_SPECS,
                    FILE_CREATE_CONTEXT_MENU_SPECS,
                ],
            );
        } else {
            rebuild_popover_action_menu(
                &popover,
                &menu_box,
                &[
                    FILE_NAV_CONTEXT_MENU_SPECS,
                    FILE_CREATE_CONTEXT_MENU_SPECS,
                    FILE_EDIT_CONTEXT_MENU_SPECS,
                ],
            );
        }
        popover.set_pointing_to(Some(&pointing_to));
        popover.popup();
        return true;
    }
    false
}

fn focus_button_for_overlay(overlay: &gtk4::Overlay) -> Option<gtk4::Button> {
    let mut current = overlay.first_child();
    while let Some(child) = current {
        if let Ok(button) = child.clone().downcast::<gtk4::Button>() {
            return Some(button);
        }
        current = child.next_sibling();
    }
    None
}

#[derive(Clone, Copy)]
struct FileTreeRowAccessibilityTarget<'a> {
    overlay: &'a gtk4::Overlay,
    drag_handle: &'a gtk4::Button,
    focus_btn: &'a gtk4::Button,
    file_item: &'a FileTreeItem,
    tree_row: &'a gtk4::TreeListRow,
    section: &'a super::LushtextWorkspaceSection,
    position: u32,
    show_reorder_handle: bool,
    show_focus: bool,
}

fn apply_file_tree_row_accessibility(target: FileTreeRowAccessibilityTarget<'_>) {
    let FileTreeRowAccessibilityTarget {
        overlay,
        drag_handle,
        focus_btn,
        file_item,
        tree_row,
        section,
        position,
        show_reorder_handle,
        show_focus,
    } = target;
    accessibility::set_role(overlay, gtk4::AccessibleRole::ListItem);

    let display_name = file_item.name();
    let label = if file_item.is_placeholder() {
        display_name.clone()
    } else if file_item.is_dir() {
        format!("Folder {display_name}")
    } else {
        format!("File {display_name}")
    };
    let description = file_tree_row_description(file_item, tree_row, section);
    let selected = section
        .imp()
        .file_tree_view
        .model()
        .and_downcast::<gtk4::SingleSelection>()
        .is_some_and(|selection| selection.selected() == position);

    let set_size = section
        .imp()
        .tree_model
        .borrow()
        .as_ref()
        .map_or(0, ListModelExt::n_items);
    let row_accessibility = if set_size > 0 && position != gtk4::INVALID_LIST_POSITION {
        RowAccessibility::new(&label)
            .description(&description)
            .selected(selected)
            .position((position + 1) as i32, set_size as i32)
    } else {
        RowAccessibility::new(&label)
            .description(&description)
            .selected(selected)
    };
    accessibility::apply_row_accessibility(overlay, row_accessibility);

    let expanded = if file_item.is_dir() && !file_item.is_placeholder() {
        Some(tree_row.is_expanded())
    } else {
        None
    };
    accessibility::set_expanded(overlay, expanded);
    accessibility::set_disabled(overlay, file_item.is_placeholder());

    let reorder_label = format!("Reorder workspace folder {display_name}");
    accessibility::set_labelled_description(
        drag_handle,
        &reorder_label,
        "Drag or use the folder context menu to reorder this workspace folder",
    );
    accessibility::set_hidden(drag_handle, !show_reorder_handle);
    accessibility::set_disabled(drag_handle, !show_reorder_handle);

    let focus_label = format!("Focus folder {display_name}");
    accessibility::set_labelled_description(
        focus_btn,
        &focus_label,
        "Temporarily show this folder as the root of the workspace tree",
    );
    accessibility::set_hidden(focus_btn, !show_focus);
    accessibility::set_disabled(focus_btn, !show_focus);
}

fn install_expanded_accessibility_hook(
    overlay: &gtk4::Overlay,
    tree_row: &gtk4::TreeListRow,
    file_item: &FileTreeItem,
    section: &super::LushtextWorkspaceSection,
) {
    clear_expanded_accessibility_hook(overlay);

    if !file_item.is_dir() || file_item.is_placeholder() {
        return;
    }

    let overlay_weak = overlay.downgrade();
    let section_weak = section.downgrade();
    let file_item = file_item.clone();
    let handler_id = tree_row.connect_notify_local(Some("expanded"), move |row, _| {
        let Some(overlay) = overlay_weak.upgrade() else {
            return;
        };

        accessibility::set_expanded(&overlay, Some(row.is_expanded()));
        if let Some(section) = section_weak.upgrade() {
            let description = file_tree_row_description(&file_item, row, &section);
            accessibility::set_description(&overlay, &description);
        }
    });

    let signals = SignalBag::new();
    signals.track(tree_row, handler_id);
    // SAFETY: the key is private to this row factory. The bag is stolen and
    // cleared on both bind and unbind before the recycled overlay is reused.
    unsafe {
        overlay.set_data(ROW_EXPANDED_ACCESSIBILITY_HOOK, signals);
    }
}

fn clear_expanded_accessibility_hook(overlay: &gtk4::Overlay) {
    // SAFETY: mirrors set_data(ROW_EXPANDED_ACCESSIBILITY_HOOK) above; no
    // external code reads this private row-local signal bag.
    unsafe {
        if let Some(signals) = overlay.steal_data::<SignalBag>(ROW_EXPANDED_ACCESSIBILITY_HOOK) {
            signals.clear();
        }
    }
}

fn file_tree_row_description(
    file_item: &FileTreeItem,
    tree_row: &gtk4::TreeListRow,
    section: &super::LushtextWorkspaceSection,
) -> String {
    if file_item.is_placeholder() {
        return "Additional children are hidden by the sidebar scan limit".to_string();
    }

    let mut parts = Vec::new();
    if file_item.is_dir() {
        parts.push("Directory".to_string());
    } else {
        parts.push("File".to_string());
    }

    if let Some(path) = file_item.path() {
        parts.push(path.display().to_string());
    }

    if file_item.workspace_folder_id().is_some() {
        parts.push("Top-level workspace folder".to_string());
    }

    if tree_row.depth() > 0 {
        parts.push(format!(
            "Nested level {}",
            tree_row.depth().saturating_add(1)
        ));
    }

    if !section.imp().drilldown_stack.borrow().is_empty() {
        parts.push("Focused folder view".to_string());
    }

    if file_item.is_empty() == Some(true) {
        parts.push("Empty folder".to_string());
    } else if file_item.is_dir() {
        parts.push(
            if tree_row.is_expanded() {
                "Expanded"
            } else {
                "Collapsed"
            }
            .to_string(),
        );
    }

    parts.join(". ")
}
