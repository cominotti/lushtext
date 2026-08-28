// SPDX-License-Identifier: GPL-3.0-or-later

//! Per-workspace section widget, and the nested coordination role modules of the
//! workspace tree workflow.
//!
//! # Role of this module
//!
//! This module itself is a **called presentation surface**, not a role: it is the
//! section GObject's public wrapper. The workflow's canonical role home is the
//! parent directory `ui/sidebar/`, which holds the narrative facade, the single
//! `policy.rs`, the single `evidence.rs`, and `seams.rs`. This directory holds the
//! workflow's **nested** coordination role modules, which is the arrangement
//! `gtk-adapter-module-boundaries` permits for one workflow spanning a directory and
//! a widget subdirectory of it.
//!
//! # Coordination roles here
//!
//! | Module | Role | Stage order it serves |
//! | --- | --- | --- |
//! | `scan_admission` | `admission` | the per-child-store directory-scan flight |
//! | `scan_execution` | `execution` | the child scan worker, reconciliation, and child-store materialization |
//! | `refresh_execution` | `execution` | targeted in-place and full refresh coalescing |
//! | `folder_execution` | `execution` | top-level folder rows, the empty probe, focused-folder drilldown |
//! | `file_execution` | `execution` | file create, inline rename, delete |
//! | `peek_execution` | `execution` | `Space` file peek |
//! | `reorder_execution` | `execution` | workspace-folder reorder drag and drop |
//! | `watch` | `watch` | watcher install, mailbox reconcile, and target mirroring |
//!
//! `watch.rs` **keeps its name**: it already carries a correct bounded role name, and
//! the convention forbids renaming a stable correct module for symmetry with newly
//! named siblings. Read the asymmetry as deliberate, not as an oversight.
//!
//! # Called presentation surfaces here
//!
//! `mod.rs` (this file), `imp.rs`, `row_factory.rs`, `context_menus.rs`,
//! `row_accessibility.rs`, and `icon_presentation.rs` are called presentation
//! surfaces. They carry no role, own no `policy.rs` or `evidence.rs`, and keep every
//! behavior obligation stated in their own module docs.
//!
//! # Neither a role nor a presentation surface
//!
//! `watch_targets.rs` is a **plain data structure owned by the `watch` role** — an
//! incremental mirror of the flattened model's watch contributions, with no GTK
//! import, no widget, and no stage of its own. Naming it a coordination role would
//! give `watch` two role modules for one job; calling it a presentation surface would
//! be false, because it projects nothing onto widgets. It is classified here so a
//! reader does not have to conclude it was overlooked. It is not `policy.rs`
//! because it is stateful bookkeeping rather than a pure decision, and the workflow
//! already owns exactly one `policy.rs` at its canonical role home.

mod context_menus;
mod file_execution;
mod folder_execution;
mod icon_presentation;
// gtk-rs custom widgets keep their public wrapper in `mod.rs` and the private
// ObjectSubclass/template state in `imp.rs`, matching GLib's split between the
// reference-counted object API and per-instance implementation data.
mod imp;
mod peek_execution;
mod refresh_execution;
mod reorder_execution;
mod row_accessibility;
mod row_factory;
mod scan_admission;
mod scan_execution;
mod watch;
mod watch_targets;

#[cfg(feature = "test-utils")]
use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;

use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use super::file_tree_item::FileTreeItem;
use crate::model::workspace::{WorkspaceFolderId, WorkspaceFolderMoveDirection, WorkspaceId};
use crate::services::notifications::NotificationSeverity;
use crate::ui::accessibility;
use crate::ui::sidebar::seams::SidebarFileRowStateSnapshot;

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

/// Direct ownership evidence for materialized directory scans.
///
/// The three `process_*` fields are **process-global**, shared by every workspace
/// section in every window, exactly as `scan_admission`'s module doc requires any
/// projection of those counters to say. They were once named `aggregate_*`, which
/// read as "summed over this section's stores" — the reading that module exists to
/// prevent, because a window with no workspaces would then appear to report scans
/// belonging elsewhere. Every other field here is per-section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceScanPressureEvidence {
    pub active_scans: usize,
    pub pending_scans: usize,
    pub admission_waiting_scans: usize,
    /// Directory-scan tasks currently admitted **process-wide**.
    pub process_active_scan_tasks: usize,
    /// The **process-wide** admitted-scan ceiling.
    pub process_scan_task_limit: usize,
    /// High-water mark of admitted scan tasks **process-wide**.
    pub process_scan_task_high_water: usize,
    pub dispatch_queue: usize,
    pub dispatch_queue_high_water: usize,
    pub dispatch_batch_high_water: usize,
    pub active_per_store_high_water: usize,
    pub pending_per_store_high_water: usize,
    pub weak_pending_high_water: usize,
    pub mirror_captures: u64,
    pub cancellation_requests: u64,
    pub cancelled_terminals: u64,
    pub stale_completions: u64,
    pub terminal_publications: u64,
    pub active_empty_probes: usize,
    pub pending_empty_probes: usize,
    pub empty_probe_stale_rejections: u64,
    pub empty_probe_terminal_publications: u64,
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
        let label = format!("Workspace {name}");
        accessibility::set_labelled_description(
            &*self.imp().header_box,
            &label,
            "Workspace header with folder actions and collapse control",
        );
    }

    /// Return the current workspace header label text.
    #[must_use]
    pub fn workspace_name(&self) -> String {
        self.imp().header_label.label().to_string()
    }

    /// Move keyboard focus to this section's file tree.
    ///
    /// Runs on the GTK main thread and exists for keyboard/automation paths
    /// that need to open the selected row's context menu without pointer input.
    pub(super) fn focus_file_tree(&self) -> bool {
        self.imp().file_tree_view.grab_focus()
    }

    /// Move keyboard focus to the workspace header's collapse button.
    ///
    /// The header itself owns the context-menu key controller, and GTK bubbles
    /// key events from this focusable child through that header controller.
    pub(super) fn focus_header_controls(&self) -> bool {
        self.imp().collapse_button.grab_focus()
    }

    /// Return this section's stable workspace id.
    #[must_use]
    pub fn workspace_id(&self) -> WorkspaceId {
        self.imp().workspace_id.borrow().clone()
    }

    /// Replace the open/active file-row projection and resync realized rows.
    pub(super) fn set_file_row_state_snapshot(&self, snapshot: Rc<SidebarFileRowStateSnapshot>) {
        *self.imp().file_row_state_snapshot.borrow_mut() = snapshot;
        if self.property::<bool>("visible") {
            self.sync_file_row_states();
        }
    }

    /// Reapply open/active file-row styling to currently realized ListView rows.
    pub(super) fn sync_file_row_states(&self) {
        for_each_realized_file_row_overlay(self, |overlay| {
            sync_file_row_state_for_overlay(self, &overlay);
        });
    }

    /// Mirror refresh and watcher failures into the file tree's accessible state.
    pub(super) fn sync_file_tree_error_state(&self) {
        let has_refresh_error = self
            .imp()
            .refresh_runtime
            .last_reported_error
            .borrow()
            .is_some();
        let has_watch_error = self
            .imp()
            .watch_runtime
            .last_reported_error
            .borrow()
            .is_some();
        accessibility::set_invalid(
            &*self.imp().file_tree_view,
            has_refresh_error || has_watch_error,
        );
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

    /// Test helper for action tests that do not need a realized row expander.
    #[cfg(feature = "test-utils")]
    pub fn set_context_target_for_test(
        &self,
        path: &Path,
        is_dir: bool,
        workspace_folder_id: Option<WorkspaceFolderId>,
    ) {
        *self.imp().context_target.borrow_mut() = Some(imp::FileContextTarget {
            path: path.to_path_buf(),
            is_dir,
            workspace_folder_id,
            expander: gtk4::TreeExpander::new(),
        });
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

    pub(super) fn emit_message(&self, text: &str, severity: NotificationSeverity) {
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
            reorder_execution::DropPosition::Before,
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
            reorder_execution::DropPosition::After,
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
            reorder_execution::DropPosition::Before,
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
            reorder_execution::DropPosition::After,
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
        reorder_execution::folder_reorder_drag_owns_row_hover_for_test()
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
        reorder_execution::reset_drag_hover_child_model_count_for_test();
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

/// Exercise the production bulk child-cache reducer without constructing GTK.
#[doc(hidden)]
#[must_use]
pub fn child_cache_rebuild_operation_evidence_for_benchmark(row_count: usize) -> (usize, usize) {
    scan_execution::child_cache_rebuild_operation_evidence(row_count)
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

/// Visit every realized file-row overlay in this section's list view.
///
/// `GtkListView` exposes only realized/recycled row widgets, so every caller is
/// intentionally limited to visible rows: an unrealized row cannot carry a shield,
/// a marker, or pointer hover until GTK binds it.
///
/// One walk, not one per caller. `reorder_execution` kept a byte-identical private
/// copy under a second name until this became shared, which is how a fix to one of
/// them silently missed the other.
pub(super) fn for_each_realized_file_row_overlay(
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

/// Find the realized overlay whose row currently renders `target_path`.
///
/// Shared with `reorder_execution` for the same reason as the walk above: it had a
/// byte-identical private copy under a second name.
#[cfg(feature = "test-utils")]
pub(super) fn realized_file_row_overlay_for_path(
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
    if reorder_execution::folder_reorder_drag_is_active() {
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

/// Directory-scan tasks currently admitted **process-wide**, for the evidence surface.
///
/// Process-global rather than per-section, and named so at every layer. Reading an
/// `AtomicUsize` cannot materialize toolkit state.
pub(crate) fn process_active_scan_tasks() -> usize {
    scan_admission::active_scan_tasks()
}

/// High-water mark of admitted scan tasks **process-wide**.
pub(crate) fn process_scan_task_high_water() -> usize {
    scan_admission::scan_task_high_water()
}

/// The **process-wide** admitted-scan ceiling.
pub(crate) const fn process_scan_task_limit() -> usize {
    scan_admission::WORKSPACE_SCAN_TASK_LIMIT
}

/// Times the reorder drag fell back to the inert row shield, process-wide.
pub(crate) fn drag_hover_child_model_count() -> usize {
    reorder_execution::drag_hover_child_model_count()
}
