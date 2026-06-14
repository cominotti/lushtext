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
    pub context_menu: RefCell<Option<gtk4::PopoverMenu>>,
    /// Context-menu model for ordinary file and descendant directory rows.
    pub context_file_menu_model: RefCell<Option<gio::Menu>>,
    /// Context-menu model for configured top-level workspace folder rows.
    pub context_folder_menu_model: RefCell<Option<gio::Menu>>,
    /// Popover for the right-click context menu on the workspace header.
    pub header_context_menu: RefCell<Option<gtk4::PopoverMenu>>,
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
        self.setup_factory();
        self.obj().register_folder_reorder_section();
        self.setup_file_context_menu();
        self.setup_header_context_menu();
        self.setup_header_double_click();
        self.obj().setup_peek();

        self.add_folder_button.update_property(&[
            gtk4::accessible::Property::Label("Add folder"),
            gtk4::accessible::Property::Description("Add a folder to this workspace"),
        ]);
        self.collapse_button.update_property(&[
            gtk4::accessible::Property::Label("Collapse Workspace"),
            gtk4::accessible::Property::Description("Hide this workspace's folder list"),
        ]);

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

            // GTK4 trees use TreeListModel for hierarchy, ListView for row
            // recycling, and TreeExpander for indentation/disclosure; each bind
            // reattaches the expander to the currently recycled TreeListRow.
            let expander = gtk4::TreeExpander::new();
            let content_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            content_box.set_halign(gtk4::Align::Start);

            let drag_handle = gtk4::Button::from_icon_name("list-drag-handle-symbolic");
            drag_handle.set_valign(gtk4::Align::Center);
            drag_handle.set_tooltip_text(Some("Reorder Folder"));
            drag_handle.set_visible(false);
            drag_handle.add_css_class("flat");
            drag_handle.add_css_class("circular");
            drag_handle.add_css_class("workspace-folder-drag-handle");
            drag_handle.update_property(&[
                gtk4::accessible::Property::Label("Reorder Folder"),
                gtk4::accessible::Property::Description("Drag to reorder this workspace folder"),
            ]);

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
            content_box.append(&icon);
            content_box.append(&label);
            expander.set_child(Some(&content_box));

            let drop_target = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            drop_target.add_css_class("workspace-folder-drop-target");
            drop_target.set_can_target(false);
            drop_target.set_halign(gtk4::Align::Fill);
            drop_target.set_valign(gtk4::Align::Start);
            drop_target.set_height_request(2);
            drop_target.set_visible(false);

            let drop_shield = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            drop_shield.add_css_class("workspace-folder-dnd-shield");
            drop_shield.set_can_target(false);
            drop_shield.set_focusable(false);
            drop_shield.set_halign(gtk4::Align::Fill);
            drop_shield.set_valign(gtk4::Align::Fill);
            drop_shield.set_hexpand(true);
            drop_shield.set_vexpand(true);

            let drop_indicator = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            drop_indicator.add_css_class("workspace-folder-drop-indicator");
            drop_indicator.set_can_target(false);
            drop_indicator.set_halign(gtk4::Align::Fill);
            drop_indicator.set_valign(gtk4::Align::Center);
            drop_indicator.set_hexpand(true);
            drop_indicator.set_height_request(2);
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

            let mut focus_btn_opt = None;
            let mut current = overlay.first_child();
            while let Some(child) = current {
                if child.downcast_ref::<gtk4::Button>().is_some() {
                    focus_btn_opt = child.downcast::<gtk4::Button>().ok();
                    break;
                }
                current = child.next_sibling();
            }
            let focus_btn = focus_btn_opt.expect("focus_btn missing");

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
                    .and_downcast::<gtk4::Image>()
                    .expect("second child is Image");

                let label = icon
                    .next_sibling()
                    .and_downcast::<gtk4::Label>()
                    .expect("third child is Label");

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
                    && let Some(icon) = drag_handle.next_sibling().and_downcast::<gtk4::Image>()
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
                    content_box.set_margin_end(0);
                }

                super::dnd::reset_reorder_row_for_unbind(&overlay);

                if let Some(section) = section_weak.upgrade()
                    && section
                        .imp()
                        .context_expander
                        .borrow()
                        .as_ref()
                        .is_some_and(|context_expander| context_expander == &expander)
                {
                    *section.imp().context_expander.borrow_mut() = None;
                    *section.imp().context_path.borrow_mut() = None;
                    *section.imp().context_workspace_folder_id.borrow_mut() = None;
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

        let file_menu = gio::Menu::new();

        let nav_section = gio::Menu::new();
        nav_section.append(Some("Focus Folder"), Some("section.focus-folder"));
        nav_section.append(Some("Local History…"), Some("section.local-history"));
        nav_section.append(Some("Open Document Note…"), Some("section.document-note"));
        file_menu.append_section(None, &nav_section);

        let create_section = gio::Menu::new();
        create_section.append(Some("New File"), Some("section.new-file"));
        create_section.append(Some("New Folder"), Some("section.new-dir"));
        file_menu.append_section(None, &create_section);

        let edit_section = gio::Menu::new();
        edit_section.append(Some("Rename"), Some("section.rename"));
        edit_section.append(Some("Delete"), Some("section.delete"));
        file_menu.append_section(None, &edit_section);

        let folder_menu = gio::Menu::new();
        let folder_note_section = gio::Menu::new();
        folder_note_section.append(Some("Open Folder Note…"), Some("section.folder-note"));
        folder_menu.append_section(None, &folder_note_section);

        let membership_section = gio::Menu::new();
        membership_section.append(Some("Move Up"), Some("section.move-folder-up"));
        membership_section.append(Some("Move Down"), Some("section.move-folder-down"));
        membership_section.append(Some("Remove from Workspace"), Some("section.remove-folder"));
        folder_menu.append_section(None, &membership_section);

        let folder_create_section = gio::Menu::new();
        folder_create_section.append(Some("New File"), Some("section.new-file"));
        folder_create_section.append(Some("New Folder"), Some("section.new-dir"));
        folder_menu.append_section(None, &folder_create_section);

        let popover = gtk4::PopoverMenu::from_model(Some(&file_menu));
        popover.set_parent(&*self.file_tree_view);
        popover.set_has_arrow(false);
        popover.set_halign(gtk4::Align::Start);
        *self.context_file_menu_model.borrow_mut() = Some(file_menu.clone());
        *self.context_folder_menu_model.borrow_mut() = Some(folder_menu.clone());
        *self.context_menu.borrow_mut() = Some(popover);

        // Register actions under the "section" prefix. Context menu items
        // reference them as "section.new-file", "section.rename", etc.
        // GTK resolves these by walking up the widget tree for the prefix.
        let action_group = gio::SimpleActionGroup::new();

        let focus_folder_action = gio::SimpleAction::new("focus-folder", None);
        let section_weak = obj.downgrade();
        focus_folder_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade()
                && let Some(path) = section.imp().context_path.borrow().clone()
                && section.imp().context_is_dir.get()
            {
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
                section.show_remove_folder_confirmation();
            }
        });
        action_group.add_action(&remove_folder_action);

        let new_file_action = gio::SimpleAction::new("new-file", None);
        let section_weak = obj.downgrade();
        new_file_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                section.create_new_item(false);
            }
        });
        action_group.add_action(&new_file_action);

        let new_dir_action = gio::SimpleAction::new("new-dir", None);
        let section_weak = obj.downgrade();
        new_dir_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                section.create_new_item(true);
            }
        });
        action_group.add_action(&new_dir_action);

        let rename_action = gio::SimpleAction::new("rename", None);
        let section_weak = obj.downgrade();
        rename_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                section.begin_rename();
            }
        });
        action_group.add_action(&rename_action);

        let delete_action = gio::SimpleAction::new("delete", None);
        let section_weak = obj.downgrade();
        delete_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                section.show_delete_confirmation();
            }
        });
        action_group.add_action(&delete_action);

        obj.insert_action_group("section", Some(&action_group));

        // Attach the gesture to the stable list view; press-time picking
        // resolves the current recycled row before opening the menu.
        let gesture = gtk4::GestureClick::new();
        gesture.set_button(3);

        let section_weak = obj.downgrade();
        let focus_folder_action_clone = focus_folder_action;
        let local_history_action_clone = local_history_action;
        let document_note_action_clone = document_note_action;
        let folder_note_action_clone = folder_note_action;
        let move_folder_up_action_clone = move_folder_up_action;
        let move_folder_down_action_clone = move_folder_down_action;
        let remove_folder_action_clone = remove_folder_action;
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
            let Some(path) = file_item.path() else {
                return;
            };

            let imp = section.imp();
            let workspace_folder_id = file_item.workspace_folder_id();
            *imp.context_path.borrow_mut() = Some(path);
            imp.context_is_dir.set(file_item.is_dir());
            *imp.context_workspace_folder_id.borrow_mut() = workspace_folder_id.clone();
            *imp.context_expander.borrow_mut() = Some(expander);

            let is_workspace_folder = workspace_folder_id.is_some();
            focus_folder_action_clone.set_enabled(
                file_item.is_dir() && !file_item.is_placeholder() && tree_row.depth() > 0,
            );
            // Avoid filesystem metadata checks on the right-click path; the
            // window-level local-history workflow validates file size on
            // activation and reports a warning if the file is too large.
            let local_history_enabled = !file_item.is_dir() && !file_item.is_placeholder();
            local_history_action_clone.set_enabled(local_history_enabled);
            document_note_action_clone
                .set_enabled(!file_item.is_dir() && !file_item.is_placeholder());
            folder_note_action_clone.set_enabled(is_workspace_folder);
            remove_folder_action_clone.set_enabled(is_workspace_folder);
            let (can_move_up, can_move_down) = workspace_folder_id
                .as_ref()
                .map_or((false, false), |folder_id| {
                    section.workspace_folder_move_availability(folder_id)
                });
            move_folder_up_action_clone.set_enabled(can_move_up);
            move_folder_down_action_clone.set_enabled(can_move_down);

            let popover = imp.context_menu.borrow().clone();
            if let Some(popover) = popover {
                popover.set_menu_model(Some(if is_workspace_folder {
                    &folder_menu
                } else {
                    &file_menu
                }));
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "Pointer event coordinates are already bounded by GTK widget geometry before converting to i32"
                )]
                popover.set_pointing_to(Some(&gdk4::Rectangle::new(x as i32, y as i32, 1, 1)));
                popover.popup();
            }
        });

        self.file_tree_view.add_controller(gesture);
    }

    /// Build right-click context menu for the workspace header.
    fn setup_header_context_menu(&self) {
        let obj = self.obj();

        let menu = gio::Menu::new();
        menu.append(Some("Add Folder…"), Some("ws-header.add-folder"));
        menu.append(
            Some("Open Folder Note…"),
            Some("ws-header.open-folder-note"),
        );
        menu.append(Some("Rename Workspace"), Some("ws-header.rename"));
        menu.append(Some("Remove Workspace"), Some("ws-header.unlist"));

        let popover = gtk4::PopoverMenu::from_model(Some(&menu));
        popover.set_parent(&*self.header_box);
        popover.set_has_arrow(false);
        popover.set_halign(gtk4::Align::Start);
        *self.header_context_menu.borrow_mut() = Some(popover.clone());

        let action_group = gio::SimpleActionGroup::new();

        let folder_note_action = gio::SimpleAction::new("open-folder-note", None);
        let section_weak = obj.downgrade();
        folder_note_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                section.notify_folder_note_requested();
            }
        });
        action_group.add_action(&folder_note_action);

        let add_folder_action = gio::SimpleAction::new("add-folder", None);
        let section_weak = obj.downgrade();
        add_folder_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                section.notify_add_folder_requested();
            }
        });
        action_group.add_action(&add_folder_action);

        let rename_action = gio::SimpleAction::new("rename", None);
        let section_weak = obj.downgrade();
        rename_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                section.notify_rename_workspace_requested();
            }
        });
        action_group.add_action(&rename_action);

        let unlist_action = gio::SimpleAction::new("unlist", None);
        let section_weak = obj.downgrade();
        unlist_action.connect_activate(move |_, _| {
            if let Some(section) = section_weak.upgrade() {
                section.notify_unlist_workspace_requested();
            }
        });
        action_group.add_action(&unlist_action);

        obj.insert_action_group("ws-header", Some(&action_group));

        // Right-click gesture on the header box
        let gesture = gtk4::GestureClick::new();
        gesture.set_button(3);

        let popover_ref = popover;
        gesture.connect_pressed(move |_gesture, _n_press, x, y| {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "Pointer event coordinates are already bounded by GTK widget geometry before converting to i32"
            )]
            popover_ref.set_pointing_to(Some(&gdk4::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover_ref.popup();
        });

        self.header_box.add_controller(gesture);
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
