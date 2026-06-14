// SPDX-License-Identifier: GPL-3.0-or-later

//! Per-workspace section widget: header, tree, and context-menu callbacks.
//!
//! Folder-tree loading and drill-down flows live in `folders.rs`, file operations
//! live in `actions.rs`, and index/cache helpers live in their dedicated files.

mod actions;
mod dnd;
mod folders;
mod icon_presentation;
mod imp;
mod peek;
mod refresh;
mod tree_index;
mod tree_loading;
mod watch;

#[cfg(feature = "test-utils")]
use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;

use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use super::SidebarFileRowStateSnapshot;
use super::file_tree_item::FileTreeItem;
use crate::model::workspace::{WorkspaceFolderId, WorkspaceFolderMoveDirection, WorkspaceId};
use crate::services::notifications::NotificationSeverity;

// glib::wrapper! generates the public GObject wrapper around the private
// imp.rs subclass; the extends/implements list declares the GTK interfaces the
// wrapper can be used as by parents and templates.
glib::wrapper! {
    /// Public GObject wrapper for one workspace section in the sidebar.
    ///
    /// The private implementation stores template children and row state; this
    /// facade exposes callback wiring, workflow methods, and test hooks.
    pub struct LushtextWorkspaceSection(ObjectSubclass<imp::LushtextWorkspaceSection>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

/// Test-only summary of how a shield-routed reorder hover is handled.
#[cfg(feature = "test-utils")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceFolderReorderHoverDecision {
    /// Whether the row-level shield should consume hover before disclosure widgets.
    pub owns_hover: bool,
    /// Whether this hover should paint the single insertion line.
    pub shows_indicator: bool,
    /// Whether dropping at this target would be handled as a folder reorder drop.
    pub accepts_drop: bool,
}

#[cfg(feature = "test-utils")]
/// Test-only summary of the realized open/active decoration on one file row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileRowStateForTest {
    /// Whether the realized file row carries the open-tab marker.
    pub open: bool,
    /// Whether the realized file row carries the active-tab marker.
    pub active: bool,
    /// Whether the row keeps the fixed-width indicator gutter allocated.
    pub indicator: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FileRowVisualState {
    Ordinary,
    Open,
    Active,
}

/// CSS class applied to file rows whose path is open in any tab.
const FILE_ROW_OPEN_CLASS: &str = "workspace-file-open";
/// CSS class applied to the file row for the currently selected tab.
const FILE_ROW_ACTIVE_CLASS: &str = "workspace-file-active";
#[cfg(feature = "test-utils")]
/// CSS class for the fixed-width row indicator gutter used by widget tests.
const FILE_ROW_INDICATOR_CLASS: &str = "workspace-file-open-indicator";

impl LushtextWorkspaceSection {
    /// Construct a workspace section with the provided stable workspace id.
    #[must_use]
    pub fn new(workspace_id: WorkspaceId) -> Self {
        let obj: Self = Object::builder().build();
        *obj.imp().workspace_id.borrow_mut() = workspace_id;
        obj
    }

    /// Update the visible workspace header label.
    pub fn set_workspace_name(&self, name: &str) {
        self.imp().header_label.set_label(name);
    }

    /// Return the current workspace header label text.
    #[must_use]
    pub fn workspace_name(&self) -> String {
        self.imp().header_label.label().to_string()
    }

    /// Return this section's stable workspace id.
    #[must_use]
    pub fn workspace_id(&self) -> WorkspaceId {
        self.imp().workspace_id.borrow().clone()
    }

    /// Replace the open/active file-row projection and resync realized rows.
    pub(crate) fn set_file_row_state_snapshot(&self, snapshot: Rc<SidebarFileRowStateSnapshot>) {
        *self.imp().file_row_state_snapshot.borrow_mut() = snapshot;
        if self.property::<bool>("visible") {
            self.sync_file_row_states();
        }
    }

    /// Reapply open/active file-row styling to currently realized ListView rows.
    pub(crate) fn sync_file_row_states(&self) {
        for_each_realized_file_row_overlay(self, |overlay| {
            sync_file_row_state_for_overlay(self, &overlay);
        });
    }

    /// Test helper for applying an open/active file projection without a window.
    #[cfg(feature = "test-utils")]
    pub fn set_file_row_state_for_test(&self, open_paths: &[&Path], active_paths: &[&Path]) {
        let open_identities = open_paths
            .iter()
            .map(|path| path.to_path_buf())
            .collect::<HashSet<_>>();
        let active_identities = active_paths
            .iter()
            .map(|path| path.to_path_buf())
            .collect::<HashSet<_>>();
        self.set_file_row_state_snapshot(Rc::new(SidebarFileRowStateSnapshot::from_identities(
            open_identities,
            active_identities,
        )));
    }

    /// Store the parent-sidebar callback invoked when a file row is activated.
    pub fn connect_file_activated<F: Fn(&Path) + 'static>(&self, f: F) {
        self.imp()
            .file_tree_view
            .connect_activate(move |list_view, position| {
                activate_file_at(list_view, position, &f);
            });
    }

    /// Store the callback invoked after inline rename changes a path.
    pub fn connect_file_renamed<F: Fn(&Path, &Path) + 'static>(&self, f: F) {
        *self.imp().rename_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Store the callback invoked after a file or directory row is deleted.
    pub fn connect_file_deleted<F: Fn(&Path) + 'static>(&self, f: F) {
        *self.imp().delete_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Store the callback invoked after a new file row is created.
    pub fn connect_file_created<F: Fn(&Path) + 'static>(&self, f: F) {
        *self.imp().create_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Store the callback invoked when local history should open for one file row.
    pub fn connect_local_history_requested<F: Fn(&Path) + 'static>(&self, f: F) {
        *self.imp().local_history_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Store the callback invoked when a file row should open its document note.
    pub fn connect_document_note_requested<F: Fn(&Path) + 'static>(&self, f: F) {
        *self.imp().document_note_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Store the callback used for lightweight window-owned status messages.
    pub fn connect_message<F: Fn(&str, NotificationSeverity) + 'static>(&self, f: F) {
        *self.imp().message_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Store the callback used when peek promotion should open a real tab.
    pub fn connect_peek_promoted<F: Fn(&Path) + 'static>(&self, f: F) {
        *self.imp().peek_promote_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Store the callback invoked when the workspace header requests rename.
    pub fn connect_rename_workspace_requested<F: Fn(&WorkspaceId) + 'static>(&self, f: F) {
        *self.imp().rename_workspace_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Store the callback invoked when the section asks to add another folder.
    pub fn connect_add_folder_requested<F: Fn(&WorkspaceId) + 'static>(&self, f: F) {
        *self.imp().add_folder_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Store the callback invoked when a folder membership should be removed.
    pub fn connect_remove_folder_requested<
        F: Fn(&WorkspaceId, &WorkspaceFolderId, &Path) + 'static,
    >(
        &self,
        f: F,
    ) {
        *self.imp().remove_folder_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Store the callback invoked when a configured folder row requests its note.
    pub fn connect_folder_note_for_folder_requested<F: Fn(&WorkspaceId, &Path) + 'static>(
        &self,
        f: F,
    ) {
        *self.imp().folder_note_for_folder_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Store the callback invoked when a folder should move up or down.
    pub fn connect_reorder_folder_requested<
        F: Fn(&WorkspaceId, &WorkspaceFolderId, WorkspaceFolderMoveDirection) + 'static,
    >(
        &self,
        f: F,
    ) {
        *self.imp().reorder_folder_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Store the callback used by drag-and-drop reorder with an absolute index.
    pub fn connect_reorder_folder_to_index_requested<
        F: Fn(&WorkspaceId, &WorkspaceFolderId, usize) + 'static,
    >(
        &self,
        f: F,
    ) {
        *self.imp().reorder_folder_to_index_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Store the callback invoked when the workspace should be unlisted.
    pub fn connect_unlist_workspace_requested<F: Fn(&WorkspaceId) + 'static>(&self, f: F) {
        *self.imp().unlist_workspace_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Store the callback invoked after drill-down focuses this section.
    pub fn connect_folder_focused<F: Fn(&WorkspaceId) + 'static>(&self, f: F) {
        *self.imp().folder_focused_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Store the callback invoked when the workspace-level folder note opens.
    pub fn connect_folder_note_requested<F: Fn(&WorkspaceId) + 'static>(&self, f: F) {
        *self.imp().folder_note_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Emit the stored workspace-rename callback with this section's workspace id.
    pub fn notify_rename_workspace_requested(&self) {
        let workspace_id = self.workspace_id();
        if let Some(ref callback) = *self.imp().rename_workspace_callback.borrow() {
            callback(&workspace_id);
        }
    }

    /// Emit the stored add-folder callback with this section's workspace id.
    pub fn notify_add_folder_requested(&self) {
        let workspace_id = self.workspace_id();
        if let Some(ref callback) = *self.imp().add_folder_callback.borrow() {
            callback(&workspace_id);
        }
    }

    /// Emit the stored remove-folder callback for one configured folder.
    pub fn notify_remove_folder_requested(&self, folder_id: &WorkspaceFolderId, path: &Path) {
        let workspace_id = self.workspace_id();
        if let Some(ref callback) = *self.imp().remove_folder_callback.borrow() {
            callback(&workspace_id, folder_id, path);
        }
    }

    /// Emit the stored folder-note callback for a configured folder path.
    pub fn notify_folder_note_for_folder_requested(&self, path: &Path) {
        let workspace_id = self.workspace_id();
        if let Some(ref callback) = *self.imp().folder_note_for_folder_callback.borrow() {
            callback(&workspace_id, path);
        }
    }

    /// Emit the stored directional folder-reorder callback.
    pub fn notify_reorder_folder_requested(
        &self,
        folder_id: &WorkspaceFolderId,
        direction: WorkspaceFolderMoveDirection,
    ) {
        let workspace_id = self.workspace_id();
        if let Some(ref callback) = *self.imp().reorder_folder_callback.borrow() {
            callback(&workspace_id, folder_id, direction);
        }
    }

    /// Notify the parent sidebar that one folder should move to a new index.
    pub fn notify_reorder_folder_to_index_requested(
        &self,
        folder_id: &WorkspaceFolderId,
        new_index: usize,
    ) {
        let workspace_id = self.workspace_id();
        if let Some(ref callback) = *self.imp().reorder_folder_to_index_callback.borrow() {
            callback(&workspace_id, folder_id, new_index);
        }
    }

    /// Emit the stored workspace-unlist callback with this section's workspace id.
    pub fn notify_unlist_workspace_requested(&self) {
        let workspace_id = self.workspace_id();
        if let Some(ref callback) = *self.imp().unlist_workspace_callback.borrow() {
            callback(&workspace_id);
        }
    }

    /// Emit the stored drill-down focus callback with this section's workspace id.
    pub fn notify_folder_focused(&self) {
        let workspace_id = self.workspace_id();
        if let Some(ref callback) = *self.imp().folder_focused_callback.borrow() {
            callback(&workspace_id);
        }
    }

    /// Emit the stored workspace-level folder-note callback.
    pub fn notify_folder_note_requested(&self) {
        let workspace_id = self.workspace_id();
        if let Some(ref callback) = *self.imp().folder_note_callback.borrow() {
            callback(&workspace_id);
        }
    }

    /// Forward the peeked file path to the stored promotion callback.
    pub(super) fn notify_peek_promoted(&self, path: &Path) {
        if let Some(ref callback) = *self.imp().peek_promote_callback.borrow() {
            callback(path);
        }
    }

    /// Forward the selected file path to the stored local-history callback.
    pub(super) fn notify_local_history_requested(&self, path: &Path) {
        if let Some(ref callback) = *self.imp().local_history_callback.borrow() {
            callback(path);
        }
    }

    /// Forward the selected file path to the stored document-note callback.
    pub(super) fn notify_document_note_requested(&self, path: &Path) {
        if let Some(ref callback) = *self.imp().document_note_callback.borrow() {
            callback(path);
        }
    }

    fn emit_message(&self, text: &str, severity: NotificationSeverity) {
        if let Some(ref callback) = *self.imp().message_callback.borrow() {
            callback(text, severity);
        }
    }

    /// Test helper for exercising the same drop-index logic as row DnD.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn drop_workspace_folder_before_for_test(
        &self,
        dragged_folder_id: &WorkspaceFolderId,
        target_folder_id: &WorkspaceFolderId,
    ) -> bool {
        self.request_workspace_folder_drop(
            dragged_folder_id,
            target_folder_id,
            dnd::DropPosition::Before,
        )
    }

    /// Test helper for exercising after-row drops without synthesizing pointer input.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn drop_workspace_folder_after_for_test(
        &self,
        dragged_folder_id: &WorkspaceFolderId,
        target_folder_id: &WorkspaceFolderId,
    ) -> bool {
        self.request_workspace_folder_drop(
            dragged_folder_id,
            target_folder_id,
            dnd::DropPosition::After,
        )
    }

    /// Test helper for the same before-row validity check that drives the DnD indicator.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn drop_workspace_folder_before_would_show_indicator_for_test(
        &self,
        dragged_folder_id: &WorkspaceFolderId,
        target_folder_id: &WorkspaceFolderId,
    ) -> bool {
        self.drop_indicator_would_show_for_test(
            dragged_folder_id,
            target_folder_id,
            dnd::DropPosition::Before,
        )
    }

    /// Test helper for the same after-row validity check that drives the DnD indicator.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn drop_workspace_folder_after_would_show_indicator_for_test(
        &self,
        dragged_folder_id: &WorkspaceFolderId,
        target_folder_id: &WorkspaceFolderId,
    ) -> bool {
        self.drop_indicator_would_show_for_test(
            dragged_folder_id,
            target_folder_id,
            dnd::DropPosition::After,
        )
    }

    /// Test helper for simulating an active reorder drag without synthesizing pointer input.
    #[cfg(feature = "test-utils")]
    pub fn with_active_workspace_folder_reorder_drag_for_test<R>(
        &self,
        folder_id: &WorkspaceFolderId,
        f: impl FnOnce() -> R,
    ) -> R {
        self.with_active_folder_reorder_drag_for_test(folder_id, f)
    }

    /// Test helper for the row-surface DnD accept path that suppresses hover expansion.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn workspace_folder_reorder_drag_owns_row_hover_for_test() -> bool {
        dnd::folder_reorder_drag_owns_row_hover_for_test()
    }

    /// Test helper for simulating a shield hover at the row's before edge.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn simulate_workspace_folder_reorder_hover_before_for_test(
        &self,
        target_path: &Path,
    ) -> WorkspaceFolderReorderHoverDecision {
        self.simulate_reorder_hover_before_for_test(target_path)
    }

    /// Test helper for simulating a shield hover at the row's after edge.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn simulate_workspace_folder_reorder_hover_after_for_test(
        &self,
        target_path: &Path,
    ) -> WorkspaceFolderReorderHoverDecision {
        self.simulate_reorder_hover_after_for_test(target_path)
    }

    /// Test helper for starting a fresh fallback-counter observation window.
    #[cfg(feature = "test-utils")]
    pub fn reset_workspace_folder_reorder_drag_hover_fallback_count_for_test(&self) {
        tree_loading::reset_drag_hover_child_model_count_for_test();
    }

    /// Test helper for reading the defensive drag-hover child-model fallback counter.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn workspace_folder_reorder_drag_hover_fallback_count_for_test(&self) -> usize {
        tree_loading::drag_hover_child_model_count_for_test()
    }

    /// Test helper exposing the realized CSS state for a file-tree row.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn file_row_state_for_test(&self, path: &Path) -> Option<FileRowStateForTest> {
        let overlay = realized_file_row_overlay_for_path(self, path)?;
        Some(FileRowStateForTest {
            open: overlay.has_css_class(FILE_ROW_OPEN_CLASS),
            active: overlay.has_css_class(FILE_ROW_ACTIVE_CLASS),
            indicator: file_row_state_indicator(&overlay).is_some(),
        })
    }
}

pub(super) fn sync_file_row_state_for_overlay(
    section: &LushtextWorkspaceSection,
    overlay: &gtk4::Overlay,
) {
    let state = file_row_visual_state_for_overlay(section, overlay);
    apply_file_row_visual_state(overlay, state);
}

pub(super) fn reset_file_row_state_for_overlay(overlay: &gtk4::Overlay) {
    apply_file_row_visual_state(overlay, FileRowVisualState::Ordinary);
}

fn file_row_visual_state_for_overlay(
    section: &LushtextWorkspaceSection,
    overlay: &gtk4::Overlay,
) -> FileRowVisualState {
    let Some(tree_row) = overlay
        .child()
        .and_downcast::<gtk4::TreeExpander>()
        .and_then(|expander| expander.list_row())
    else {
        return FileRowVisualState::Ordinary;
    };
    let Some(item) = tree_row.item().and_downcast::<FileTreeItem>() else {
        return FileRowVisualState::Ordinary;
    };
    if item.is_dir() || item.is_placeholder() {
        return FileRowVisualState::Ordinary;
    }
    let Some(path) = item.path() else {
        return FileRowVisualState::Ordinary;
    };

    let snapshot = section.imp().file_row_state_snapshot.borrow();
    if snapshot.is_active(&path) {
        FileRowVisualState::Active
    } else if snapshot.is_open(&path) {
        FileRowVisualState::Open
    } else {
        FileRowVisualState::Ordinary
    }
}

fn apply_file_row_visual_state(overlay: &gtk4::Overlay, state: FileRowVisualState) {
    let wants_open = matches!(state, FileRowVisualState::Open | FileRowVisualState::Active);
    let wants_active = matches!(state, FileRowVisualState::Active);

    if overlay.has_css_class(FILE_ROW_OPEN_CLASS) != wants_open {
        if wants_open {
            overlay.add_css_class(FILE_ROW_OPEN_CLASS);
        } else {
            overlay.remove_css_class(FILE_ROW_OPEN_CLASS);
        }
    }

    if overlay.has_css_class(FILE_ROW_ACTIVE_CLASS) != wants_active {
        if wants_active {
            overlay.add_css_class(FILE_ROW_ACTIVE_CLASS);
        } else {
            overlay.remove_css_class(FILE_ROW_ACTIVE_CLASS);
        }
    }
}

fn for_each_realized_file_row_overlay(
    section: &LushtextWorkspaceSection,
    mut visit: impl FnMut(gtk4::Overlay),
) {
    let mut row_widget = section.imp().file_tree_view.first_child();
    while let Some(row) = row_widget {
        if let Some(overlay) = row.first_child().and_downcast::<gtk4::Overlay>() {
            visit(overlay);
        }
        row_widget = row.next_sibling();
    }
}

#[cfg(feature = "test-utils")]
fn file_row_state_indicator(overlay: &gtk4::Overlay) -> Option<gtk4::Widget> {
    let expander = overlay.child().and_downcast::<gtk4::TreeExpander>()?;
    let content_box = expander.child().and_downcast::<gtk4::Box>()?;
    let mut child = content_box.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if widget.has_css_class(FILE_ROW_INDICATOR_CLASS) {
            return Some(widget);
        }
    }
    None
}

#[cfg(feature = "test-utils")]
fn realized_file_row_overlay_for_path(
    section: &LushtextWorkspaceSection,
    target_path: &Path,
) -> Option<gtk4::Overlay> {
    let mut row_widget = section.imp().file_tree_view.first_child();
    while let Some(row) = row_widget {
        if let Some(overlay) = row.first_child().and_downcast::<gtk4::Overlay>()
            && let Some(expander) = overlay.child().and_downcast::<gtk4::TreeExpander>()
            && let Some(tree_row) = expander.list_row()
            && let Some(item) = tree_row.item().and_downcast::<FileTreeItem>()
            && item.path().as_deref() == Some(target_path)
        {
            return Some(overlay);
        }
        row_widget = row.next_sibling();
    }
    None
}

/// Activate a row: open files, toggle directories, and ignore reorder drags.
fn activate_file_at(list_view: &gtk4::ListView, position: u32, callback: &dyn Fn(&Path)) {
    if dnd::folder_reorder_drag_is_active() {
        return;
    }

    let Some(model) = list_view.model() else {
        return;
    };
    if let Some(item) = model.item(position)
        && let Some(tree_row) = item.downcast_ref::<gtk4::TreeListRow>()
        && let Some(file_item) = tree_row
            .item()
            .and_then(|item| item.downcast::<FileTreeItem>().ok())
    {
        if file_item.is_dir() && !file_item.is_placeholder() && file_item.is_empty() != Some(true) {
            tree_row.set_expanded(!tree_row.is_expanded());
            if let Some(section) = list_view
                .ancestor(LushtextWorkspaceSection::static_type())
                .and_downcast::<LushtextWorkspaceSection>()
            {
                section.restart_workspace_watch();
            }
        } else if !file_item.is_dir()
            && let Some(ref path) = file_item.path()
        {
            callback(path);
        }
    }
}

impl Default for LushtextWorkspaceSection {
    fn default() -> Self {
        Self::new(WorkspaceId::default())
    }
}
