// SPDX-License-Identifier: GPL-3.0-or-later

//! Drag-and-drop reorder support for top-level workspace folder rows.
//!
//! GTK recycles `GtkListView` row widgets, so this module treats row widgets as
//! event surfaces only. Every drag/drop operation re-reads the currently bound
//! `TreeListRow` and moves folders by stable `WorkspaceFolderId`.

use glib::prelude::{StaticType, ToValue};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gdk;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::model::workspace::{WorkspaceFolderId, WorkspaceId};
use crate::ui::sidebar::file_tree_item::FileTreeItem;

use super::LushtextWorkspaceSection;

/// Drop edge relative to the target folder row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DropPosition {
    /// Insert the dragged folder before the target row.
    Before,
    /// Insert the dragged folder after the target row.
    After,
}

/// Stable folder identity carried through GTK DnD independent of recycled row widgets.
#[derive(Clone)]
struct FolderDragPayload {
    workspace_id: WorkspaceId,
    folder_id: WorkspaceFolderId,
}

// GTK drag state lives on the main thread, so thread-local storage keeps the
// active payload and weak section registry close to the widget event loop
// without global locks or strong references to disposed sections.
thread_local! {
    /// Active reorder payload for the single GTK drag currently in flight.
    static ACTIVE_FOLDER_DRAG: RefCell<Option<FolderDragPayload>> = const { RefCell::new(None) };
    /// Weak registry of realized sections whose row shields must turn on together.
    static REGISTERED_FOLDER_REORDER_SECTIONS: RefCell<Vec<glib::WeakRef<LushtextWorkspaceSection>>> = const { RefCell::new(Vec::new()) };
}

/// Row-local data key used for one-shot expansion-watch suppression after defensive DnD collapse.
const SUPPRESS_EXPANDED_WATCH_KEY: &str = "workspace-reorder-suppress-expanded-watch";

/// Internal verdict for whether the row shield owns hover, paints a line, and accepts drop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FolderReorderHoverDecision {
    owns_hover: bool,
    shows_indicator: bool,
    accepts_drop: bool,
}

impl LushtextWorkspaceSection {
    /// Install drag/drop controllers on one recycled list item widget.
    pub(super) fn install_folder_reorder_dnd(
        &self,
        list_item: &gtk4::ListItem,
        drag_handle: &gtk4::Button,
        overlay: &gtk4::Overlay,
        drop_shield: &gtk4::Box,
        drop_target_surface: &gtk4::Box,
    ) {
        // DragSource prepare runs after the handle has started a drag; resolve
        // the payload from the currently bound row so recycled widgets never
        // cache stale folder identity.
        let drag_source = gtk4::DragSource::new();
        drag_source.set_actions(gdk::DragAction::MOVE);

        let section_weak = self.downgrade();
        let list_item_weak = list_item.downgrade();
        drag_source.connect_prepare(move |_, _, _| {
            let section = section_weak.upgrade()?;
            let list_item = list_item_weak.upgrade()?;
            let payload = drag_payload_for_list_item(&section, &list_item)?;
            begin_folder_reorder_drag(payload.clone());
            Some(gdk::ContentProvider::for_value(
                &encode_drag_payload(&payload).to_value(),
            ))
        });
        drag_source.connect_drag_end(move |_, _, _| {
            end_folder_reorder_drag();
        });
        drag_source.connect_drag_cancel(move |_, _, _| {
            end_folder_reorder_drag();
            false
        });
        drag_handle.add_controller(drag_source);

        // DropTarget lives on the transparent row shield. Capture-phase hover
        // is owned before GtkTreeExpander can treat the drag as disclosure hover.
        let drop_target = gtk4::DropTarget::new(String::static_type(), gdk::DragAction::MOVE);
        drop_target.set_propagation_phase(gtk4::PropagationPhase::Capture);
        drop_target.connect_accept(move |_, _| folder_reorder_drag_should_own_row_hover());

        let section_weak = self.downgrade();
        let list_item_weak = list_item.downgrade();
        let overlay_weak = overlay.downgrade();
        let drop_surface_weak = drop_target_surface.downgrade();
        let shown_position = Rc::new(Cell::new(None));
        let shown_position_for_motion = Rc::clone(&shown_position);
        drop_target.connect_motion(move |_, _, y| {
            let Some(section) = section_weak.upgrade() else {
                return gdk::DragAction::empty();
            };
            let Some(list_item) = list_item_weak.upgrade() else {
                return gdk::DragAction::empty();
            };
            let Some(overlay) = overlay_weak.upgrade() else {
                return gdk::DragAction::empty();
            };
            let Some(drop_surface) = drop_surface_weak.upgrade() else {
                return gdk::DragAction::empty();
            };
            let position = drop_position_for_y(overlay.height(), y);
            let decision = active_drag_hover_decision_for_list_item(&section, &list_item, position);
            if decision.shows_indicator {
                show_drop_indicator(&drop_surface, &shown_position_for_motion, position);
            } else {
                hide_drop_indicator(&drop_surface, &shown_position_for_motion);
            }
            if decision.owns_hover {
                gdk::DragAction::MOVE
            } else {
                gdk::DragAction::empty()
            }
        });

        let drop_surface_weak = drop_target_surface.downgrade();
        let shown_position_for_leave = Rc::clone(&shown_position);
        drop_target.connect_leave(move |_| {
            if let Some(drop_surface) = drop_surface_weak.upgrade() {
                hide_drop_indicator(&drop_surface, &shown_position_for_leave);
            }
        });

        let section_weak = self.downgrade();
        let list_item_weak = list_item.downgrade();
        let overlay_weak = overlay.downgrade();
        let drop_surface_weak = drop_target_surface.downgrade();
        let shown_position_for_drop = shown_position;
        drop_target.connect_drop(move |_, value, _, y| {
            if let Some(drop_surface) = drop_surface_weak.upgrade() {
                hide_drop_indicator(&drop_surface, &shown_position_for_drop);
            }
            let Ok(payload_text) = value.get::<String>() else {
                return false;
            };
            let Some(payload) = decode_drag_payload(&payload_text) else {
                return false;
            };
            let Some(section) = section_weak.upgrade() else {
                return false;
            };
            let Some(list_item) = list_item_weak.upgrade() else {
                return false;
            };
            let Some(overlay) = overlay_weak.upgrade() else {
                return false;
            };
            let position = drop_position_for_y(overlay.height(), y);
            section.request_workspace_folder_drop_from_payload(&payload, &list_item, position)
        });
        drop_shield.add_controller(drop_target);
    }

    /// Register this section for drag-lifetime shield synchronization.
    pub(super) fn register_folder_reorder_section(&self) {
        let section_ptr = self.as_ptr();
        REGISTERED_FOLDER_REORDER_SECTIONS.with(|registered| {
            let mut registered = registered.borrow_mut();
            registered.retain(|weak| weak.upgrade().is_some());
            if registered.iter().any(|weak| {
                weak.upgrade()
                    .is_some_and(|section| section.as_ptr() == section_ptr)
            }) {
                return;
            }
            registered.push(self.downgrade());
        });
        self.sync_folder_reorder_shields_for_active_drag();
    }

    /// Remove this section from the drag shield registry during disposal.
    pub(super) fn unregister_folder_reorder_section(&self) {
        let section_ptr = self.as_ptr();
        REGISTERED_FOLDER_REORDER_SECTIONS.with(|registered| {
            registered.borrow_mut().retain(|weak| {
                weak.upgrade()
                    .is_some_and(|section| section.as_ptr() != section_ptr)
            });
        });
    }

    /// Match realized row shields to the current global reorder-drag state.
    pub(super) fn sync_folder_reorder_shields_for_active_drag(&self) {
        let active = folder_reorder_drag_is_active();
        for_each_realized_row_overlay(self, |overlay| {
            set_reorder_shield_targetable(&overlay, active);
            if !active {
                hide_reorder_indicator(&overlay);
            }
            if active {
                hide_focus_folder_button(&overlay);
            }
        });
    }

    /// Resolve a drop onto a live row and emit the absolute-index reorder request.
    fn request_workspace_folder_drop_from_payload(
        &self,
        payload: &FolderDragPayload,
        target_list_item: &gtk4::ListItem,
        position: DropPosition,
    ) -> bool {
        if payload.workspace_id != self.workspace_id() {
            return false;
        }
        let Some(target_folder_id) = workspace_folder_id_for_list_item(self, target_list_item)
        else {
            return false;
        };
        self.request_workspace_folder_drop(&payload.folder_id, &target_folder_id, position)
    }

    /// Emit the absolute-index reorder callback for a non-noop stable-id drop.
    pub(super) fn request_workspace_folder_drop(
        &self,
        dragged_folder_id: &WorkspaceFolderId,
        target_folder_id: &WorkspaceFolderId,
        position: DropPosition,
    ) -> bool {
        let Some((source_index, new_index)) =
            drop_source_and_new_index(self, dragged_folder_id, target_folder_id, position)
        else {
            return false;
        };
        if source_index == new_index {
            return true;
        }

        self.notify_reorder_folder_to_index_requested(dragged_folder_id, new_index);
        true
    }

    fn workspace_folder_index(&self, folder_id: &WorkspaceFolderId) -> Option<usize> {
        let folders = self.imp().original_folders.borrow();
        let folder_ids = self.imp().workspace_folder_ids.borrow();
        folders.iter().position(|entry| {
            folder_ids
                .get(entry.path())
                .is_some_and(|candidate_id| candidate_id == folder_id)
        })
    }

    #[cfg(feature = "test-utils")]
    /// Return whether this test drop would move a folder and show the insertion line.
    pub(super) fn drop_indicator_would_show_for_test(
        &self,
        dragged_folder_id: &WorkspaceFolderId,
        target_folder_id: &WorkspaceFolderId,
        position: DropPosition,
    ) -> bool {
        let Some((source_index, new_index)) =
            drop_source_and_new_index(self, dragged_folder_id, target_folder_id, position)
        else {
            return false;
        };
        source_index != new_index
    }

    #[cfg(feature = "test-utils")]
    /// Install a temporary active drag payload for tests and restore the previous one.
    pub(super) fn with_active_folder_reorder_drag_for_test<R>(
        &self,
        folder_id: &WorkspaceFolderId,
        f: impl FnOnce() -> R,
    ) -> R {
        let _restore = ActiveFolderDragRestore {
            previous: active_drag_payload(),
        };
        begin_folder_reorder_drag(FolderDragPayload {
            workspace_id: self.workspace_id(),
            folder_id: folder_id.clone(),
        });
        f()
    }

    #[cfg(feature = "test-utils")]
    /// Simulate the shield's before-edge hover path without brittle pointer synthesis.
    pub(super) fn simulate_reorder_hover_before_for_test(
        &self,
        target_path: &std::path::Path,
    ) -> super::WorkspaceFolderReorderHoverDecision {
        self.simulate_reorder_hover_for_test(target_path, DropPosition::Before)
    }

    #[cfg(feature = "test-utils")]
    /// Simulate the shield's after-edge hover path without brittle pointer synthesis.
    pub(super) fn simulate_reorder_hover_after_for_test(
        &self,
        target_path: &std::path::Path,
    ) -> super::WorkspaceFolderReorderHoverDecision {
        self.simulate_reorder_hover_for_test(target_path, DropPosition::After)
    }

    #[cfg(feature = "test-utils")]
    fn simulate_reorder_hover_for_test(
        &self,
        target_path: &std::path::Path,
        position: DropPosition,
    ) -> super::WorkspaceFolderReorderHoverDecision {
        hide_all_reorder_indicators(self);
        let decision = active_drag_hover_decision_for_path(self, target_path, position);
        if let Some(overlay) = realized_overlay_for_path(self, target_path) {
            set_reorder_shield_targetable(&overlay, folder_reorder_drag_is_active());
            if let Some(drop_surface) = reorder_indicator_surface(&overlay) {
                let shown_position = Cell::new(None);
                if decision.shows_indicator {
                    show_drop_indicator(&drop_surface, &shown_position, position);
                } else {
                    hide_drop_indicator(&drop_surface, &shown_position);
                }
            }
        }
        super::WorkspaceFolderReorderHoverDecision {
            owns_hover: decision.owns_hover,
            shows_indicator: decision.shows_indicator,
            accepts_drop: decision.accepts_drop,
        }
    }
}

#[cfg(feature = "test-utils")]
struct ActiveFolderDragRestore {
    previous: Option<FolderDragPayload>,
}

#[cfg(feature = "test-utils")]
impl Drop for ActiveFolderDragRestore {
    fn drop(&mut self) {
        set_active_drag(self.previous.take());
    }
}

fn set_active_drag(payload: Option<FolderDragPayload>) {
    ACTIVE_FOLDER_DRAG.with(|active| {
        *active.borrow_mut() = payload;
    });
    sync_registered_folder_reorder_shields();
}

fn begin_folder_reorder_drag(payload: FolderDragPayload) {
    set_active_drag(Some(payload));
}

fn end_folder_reorder_drag() {
    set_active_drag(None);
}

fn active_drag_payload() -> Option<FolderDragPayload> {
    ACTIVE_FOLDER_DRAG.with(|active| active.borrow().clone())
}

pub(super) fn folder_reorder_drag_is_active() -> bool {
    ACTIVE_FOLDER_DRAG.with(|active| active.borrow().is_some())
}

/// Reset one recycled row so no targetability or insertion line leaks.
pub(super) fn reset_reorder_row_for_unbind(overlay: &gtk4::Overlay) {
    set_reorder_shield_targetable(overlay, false);
    hide_reorder_indicator(overlay);
}

/// Prepare one newly-bound row for the current drag state.
pub(super) fn reset_reorder_row_for_bind(overlay: &gtk4::Overlay) {
    set_reorder_shield_targetable(overlay, folder_reorder_drag_is_active());
    hide_reorder_indicator(overlay);
    if folder_reorder_drag_is_active() {
        hide_focus_folder_button(overlay);
    }
}

pub(super) fn suppress_next_expanded_watch_for_drag(row: &gtk4::TreeListRow) {
    // SAFETY: the private key stores a single row-local marker consumed by the
    // same widget factory's notify::expanded handler. No external code reads it.
    unsafe {
        row.set_data(SUPPRESS_EXPANDED_WATCH_KEY, true);
    }
}

pub(super) fn expanded_watch_should_be_suppressed(row: &gtk4::TreeListRow) -> bool {
    // SAFETY: mirrors set_data(SUPPRESS_EXPANDED_WATCH_KEY) above. Taking the
    // marker makes the suppression one-shot, so later user expansions still restart watching.
    let marker_present = unsafe {
        row.steal_data::<bool>(SUPPRESS_EXPANDED_WATCH_KEY)
            .is_some()
    };
    marker_present || folder_reorder_drag_is_active()
}

/// Return whether the active drag should be consumed by row shields.
fn folder_reorder_drag_should_own_row_hover() -> bool {
    folder_reorder_drag_is_active()
}

#[cfg(feature = "test-utils")]
/// Test seam for the DropTarget accept decision.
pub(super) fn folder_reorder_drag_owns_row_hover_for_test() -> bool {
    folder_reorder_drag_should_own_row_hover()
}

fn active_drag_hover_decision_for_list_item(
    section: &LushtextWorkspaceSection,
    list_item: &gtk4::ListItem,
    position: DropPosition,
) -> FolderReorderHoverDecision {
    let target_folder_id = workspace_folder_id_for_list_item(section, list_item);
    active_drag_hover_decision_for_target(section, target_folder_id.as_ref(), position)
}

/// Validate workspace identity, target identity, and no-op moves for live hover feedback.
fn active_drag_hover_decision_for_target(
    section: &LushtextWorkspaceSection,
    target_folder_id: Option<&WorkspaceFolderId>,
    position: DropPosition,
) -> FolderReorderHoverDecision {
    let owns_hover = folder_reorder_drag_should_own_row_hover();
    let Some(payload) = active_drag_payload().filter(|_| owns_hover) else {
        return FolderReorderHoverDecision {
            owns_hover,
            shows_indicator: false,
            accepts_drop: false,
        };
    };
    if payload.workspace_id != section.workspace_id() {
        return FolderReorderHoverDecision {
            owns_hover,
            shows_indicator: false,
            accepts_drop: false,
        };
    }
    let Some(target_folder_id) = target_folder_id else {
        return FolderReorderHoverDecision {
            owns_hover,
            shows_indicator: false,
            accepts_drop: false,
        };
    };
    let Some((source_index, new_index)) =
        drop_source_and_new_index(section, &payload.folder_id, target_folder_id, position)
    else {
        return FolderReorderHoverDecision {
            owns_hover,
            shows_indicator: false,
            accepts_drop: false,
        };
    };
    FolderReorderHoverDecision {
        owns_hover,
        shows_indicator: source_index != new_index,
        accepts_drop: true,
    }
}

#[cfg(feature = "test-utils")]
fn active_drag_hover_decision_for_path(
    section: &LushtextWorkspaceSection,
    target_path: &std::path::Path,
    position: DropPosition,
) -> FolderReorderHoverDecision {
    let target_folder_id = workspace_folder_id_for_path(section, target_path);
    active_drag_hover_decision_for_target(section, target_folder_id.as_ref(), position)
}

fn show_drop_indicator(
    drop_target_surface: &gtk4::Box,
    shown_position: &Cell<Option<DropPosition>>,
    position: DropPosition,
) {
    if shown_position.get() != Some(position) {
        drop_target_surface.set_valign(match position {
            DropPosition::Before => gtk4::Align::Start,
            DropPosition::After => gtk4::Align::End,
        });
        shown_position.set(Some(position));
    }
    if !drop_target_surface.is_visible() {
        drop_target_surface.set_visible(true);
    }
}

fn hide_drop_indicator(
    drop_target_surface: &gtk4::Box,
    shown_position: &Cell<Option<DropPosition>>,
) {
    shown_position.set(None);
    drop_target_surface.set_visible(false);
}

fn drop_source_and_new_index(
    section: &LushtextWorkspaceSection,
    dragged_folder_id: &WorkspaceFolderId,
    target_folder_id: &WorkspaceFolderId,
    position: DropPosition,
) -> Option<(usize, usize)> {
    let source_index = section.workspace_folder_index(dragged_folder_id)?;
    let target_index = section.workspace_folder_index(target_folder_id)?;
    let target_insertion_index = match position {
        DropPosition::Before => target_index,
        DropPosition::After => target_index + 1,
    };
    let new_index = if source_index < target_insertion_index {
        target_insertion_index - 1
    } else {
        target_insertion_index
    };
    Some((source_index, new_index))
}

/// Resolve the DnD payload from the currently bound recycled row.
fn drag_payload_for_list_item(
    section: &LushtextWorkspaceSection,
    list_item: &gtk4::ListItem,
) -> Option<FolderDragPayload> {
    let folder_id = workspace_folder_id_for_list_item(section, list_item)?;
    Some(FolderDragPayload {
        workspace_id: section.workspace_id(),
        folder_id,
    })
}

fn workspace_folder_id_for_list_item(
    section: &LushtextWorkspaceSection,
    list_item: &gtk4::ListItem,
) -> Option<WorkspaceFolderId> {
    if !section.imp().drilldown_stack.borrow().is_empty() {
        return None;
    }
    let tree_row = list_item.item().and_downcast::<gtk4::TreeListRow>()?;
    workspace_folder_id_for_tree_row(&tree_row)
}

/// Resolve a persisted top-level folder id; drill-down mode disables reordering.
fn workspace_folder_id_for_tree_row(tree_row: &gtk4::TreeListRow) -> Option<WorkspaceFolderId> {
    if tree_row.depth() != 0 {
        return None;
    }
    let file_item = tree_row.item().and_downcast::<FileTreeItem>()?;
    file_item.workspace_folder_id()
}

#[cfg(feature = "test-utils")]
fn workspace_folder_id_for_path(
    section: &LushtextWorkspaceSection,
    target_path: &std::path::Path,
) -> Option<WorkspaceFolderId> {
    if !section.imp().drilldown_stack.borrow().is_empty() {
        return None;
    }
    let tree_model = section.imp().tree_model.borrow().as_ref()?.clone();
    for index in 0..tree_model.n_items() {
        let Some(tree_row) = tree_model.item(index).and_downcast::<gtk4::TreeListRow>() else {
            continue;
        };
        let Some(file_item) = tree_row.item().and_downcast::<FileTreeItem>() else {
            continue;
        };
        if file_item.path().as_deref() == Some(target_path) {
            return workspace_folder_id_for_tree_row(&tree_row);
        }
    }
    None
}

fn encode_drag_payload(payload: &FolderDragPayload) -> String {
    format!(
        "{}\n{}",
        payload.workspace_id.as_str(),
        payload.folder_id.as_str()
    )
}

fn decode_drag_payload(text: &str) -> Option<FolderDragPayload> {
    let mut lines = text.splitn(3, '\n');
    let workspace_id = lines.next()?;
    let folder_id = lines.next()?;
    if lines.next().is_some() || workspace_id.is_empty() || folder_id.is_empty() {
        return None;
    }
    Some(FolderDragPayload {
        workspace_id: WorkspaceId::new(workspace_id),
        folder_id: WorkspaceFolderId::new(folder_id),
    })
}

fn drop_position_for_y(row_height: i32, y: f64) -> DropPosition {
    if row_height > 0 && y > f64::from(row_height) / 2.0 {
        DropPosition::After
    } else {
        DropPosition::Before
    }
}

/// Synchronize every live section's realized row shields with the active drag state.
fn sync_registered_folder_reorder_shields() {
    REGISTERED_FOLDER_REORDER_SECTIONS.with(|registered| {
        registered.borrow_mut().retain(|weak| {
            let Some(section) = weak.upgrade() else {
                return false;
            };
            section.sync_folder_reorder_shields_for_active_drag();
            true
        });
    });
}

fn for_each_realized_row_overlay(
    section: &LushtextWorkspaceSection,
    mut visit: impl FnMut(gtk4::Overlay),
) {
    // GtkListView exposes only realized/recycled row widgets; shield sync is
    // intentionally limited to visible rows because unrealized rows cannot
    // receive pointer hover until GTK binds them.
    let mut row_widget = section.imp().file_tree_view.first_child();
    while let Some(row) = row_widget {
        if let Some(overlay) = row.first_child().and_downcast::<gtk4::Overlay>() {
            visit(overlay);
        }
        row_widget = row.next_sibling();
    }
}

fn set_reorder_shield_targetable(overlay: &gtk4::Overlay, targetable: bool) {
    if let Some(shield) = reorder_shield(overlay) {
        shield.set_can_target(targetable);
    }
}

fn hide_focus_folder_button(overlay: &gtk4::Overlay) {
    let mut child = overlay.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Some(button) = widget.downcast_ref::<gtk4::Button>()
            && button.has_css_class("can-focus")
        {
            button.set_visible(false);
        }
    }
}

fn hide_reorder_indicator(overlay: &gtk4::Overlay) {
    if let Some(drop_surface) = reorder_indicator_surface(overlay) {
        let shown_position = Cell::new(None);
        hide_drop_indicator(&drop_surface, &shown_position);
    }
}

#[cfg(feature = "test-utils")]
fn hide_all_reorder_indicators(section: &LushtextWorkspaceSection) {
    for_each_realized_row_overlay(section, |overlay| {
        hide_reorder_indicator(&overlay);
    });
}

fn reorder_shield(overlay: &gtk4::Overlay) -> Option<gtk4::Widget> {
    let mut child = overlay.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if widget.has_css_class("workspace-folder-dnd-shield") {
            return Some(widget);
        }
    }
    None
}

fn reorder_indicator_surface(overlay: &gtk4::Overlay) -> Option<gtk4::Box> {
    let mut child = overlay.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if widget.has_css_class("workspace-folder-drop-target") {
            return widget.downcast::<gtk4::Box>().ok();
        }
    }
    None
}

#[cfg(feature = "test-utils")]
fn realized_overlay_for_path(
    section: &LushtextWorkspaceSection,
    target_path: &std::path::Path,
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
