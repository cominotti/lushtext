// SPDX-License-Identifier: GPL-3.0-or-later

//! Accessibility projection and expanded-state hooks for recycled tree rows.
//!
//! # Role: called presentation surface — **not** one of the five roles
//!
//! Row accessibility projection: applies per-item accessible metadata on bind and
//! clears it on unbind so a recycled row cannot keep a previous item's name,
//! description, selection, or set-position.
//!
//! It owns no `policy.rs` and no `evidence.rs`, and it keeps every behavior obligation
//! stated below and in the workflow's matrix row.

use super::super::file_tree_item::FileTreeItem;
use crate::ui::accessibility::{self, RowAccessibility};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_signals::SignalBag;
use gtk4::gio::prelude::ListModelExt;
use gtk4::prelude::*;
use gtk4::{self, glib};

/// Private object-data key for the current row's expanded-state signal bag.
const ROW_EXPANDED_ACCESSIBILITY_HOOK: &str = "workspace-row-expanded-accessibility-hook";

#[derive(Clone, Copy)]
pub(super) struct FileTreeRowAccessibilityTarget<'a> {
    pub(super) overlay: &'a gtk4::Overlay,
    pub(super) drag_handle: &'a gtk4::Button,
    pub(super) focus_btn: &'a gtk4::Button,
    pub(super) file_item: &'a FileTreeItem,
    pub(super) tree_row: &'a gtk4::TreeListRow,
    pub(super) section: &'a super::LushtextWorkspaceSection,
    pub(super) position: u32,
    pub(super) show_reorder_handle: bool,
    pub(super) show_focus: bool,
}

pub(super) fn apply_file_tree_row_accessibility(target: FileTreeRowAccessibilityTarget<'_>) {
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

pub(super) fn install_expanded_accessibility_hook(
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

pub(super) fn clear_expanded_accessibility_hook(overlay: &gtk4::Overlay) {
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
