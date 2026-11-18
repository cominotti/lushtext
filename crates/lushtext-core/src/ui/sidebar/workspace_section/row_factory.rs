// SPDX-License-Identifier: GPL-3.0-or-later

//! Recycled file-tree row setup, projection, and cleanup.
//!
//! # Role: called presentation surface — **not** one of the five roles
//!
//! List-factory row projection: setup, bind, and unbind for recycled file-tree rows.
//!
//! **Deliberately not a role, so that no role move touches its two hardest
//! contracts**: the expander arrow's capture-phase release gesture that hands
//! every arrow click to the collapse and expand entry points while the row's
//! press-side behaviour (focus, selection, double-press activation) stays
//! GTK's (ledger A119), and the rename-entry cleanup that must run on both
//! bind and unbind.
//!
//! It owns no `policy.rs` and no `evidence.rs`, and it keeps every behavior obligation
//! stated below and in the workflow's matrix row.

use super::super::file_tree_item::FileTreeItem;
use super::context_menus::FileContextTarget;
use super::row_accessibility::{
    FileTreeRowAccessibilityTarget, apply_file_tree_row_accessibility,
    clear_expanded_accessibility_hook, install_expanded_accessibility_hook,
};
use super::{icon_presentation, imp};
use crate::ui::accessibility;
use glib::subclass::prelude::{ObjectSubclassExt, ObjectSubclassIsExt};
use gtk4::prelude::*;
use gtk4::{self, glib};

/// Set up the list item factory for rendering file tree rows.
///
/// `SignalListItemFactory` is GTK4's way of creating and recycling row widgets:
/// - `connect_setup`: creates the row's widget hierarchy (reused across items)
/// - `connect_bind`: updates row widgets to reflect the current data item
/// - `connect_unbind`: cleans up item-specific state for row recycling
pub(super) fn setup(imp: &imp::LushtextWorkspaceSection) {
    let factory = gtk4::SignalListItemFactory::new();

    factory.connect_setup(move |_factory, list_item| {
        // Factory callbacks receive generic GObjects from GTK, so
        // downcast_ref checks the runtime type before using ListItem APIs.
        let list_item = list_item
            .downcast_ref::<gtk4::ListItem>()
            .expect("item is ListItem");

        // Every recycled row carries only what every row shows (design D17):
        // the overlay, the expander, the content box, the open indicator, the
        // icon and the label. The reorder drag handle, the drop surfaces and the
        // Focus Folder button, with their controllers, are created on demand by
        // `bind` for the rows that use them (`ensure_row_decorations`).
        let overlay = gtk4::Overlay::new();
        overlay.add_css_class("workspace-folder-dnd-surface");
        overlay.add_css_class("workspace-file-row-state-surface");

        // GTK4 trees use TreeListModel for hierarchy, ListView for row
        // recycling, and TreeExpander for indentation/disclosure; each bind
        // reattaches the expander to the currently recycled TreeListRow.
        let expander = gtk4::TreeExpander::new();
        let content_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        content_box.set_halign(gtk4::Align::Start);

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

        content_box.append(&open_indicator);
        content_box.append(&icon);
        content_box.append(&label);
        expander.set_child(Some(&content_box));

        overlay.set_child(Some(&expander));
        install_arrow_gesture(&expander);

        list_item.set_child(Some(&overlay));
    });

    let section_weak = imp.obj().downgrade();
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
        let section = section_weak.upgrade();
        super::reorder_execution::reset_reorder_row_for_bind(section.as_ref(), &overlay);
        clear_expanded_accessibility_hook(&overlay);

        if let Some(file_item) = tree_row.item().and_downcast::<FileTreeItem>() {
            let content_box = expander
                .child()
                .and_downcast::<gtk4::Box>()
                .expect("expander child is Box");
            let (icon, label) = row_icon_and_label(&content_box);
            let icon = icon.expect("row content contains Image");
            let label = label.expect("row content contains Label");

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
            let focus_btn = if show_focus {
                let focus_btn = ensure_focus_button(&overlay);
                focus_btn.add_css_class("can-focus");
                content_box.set_margin_end(36);
                Some(focus_btn)
            } else {
                let focus_btn = focus_button_for_overlay(&overlay);
                if let Some(focus_btn) = &focus_btn {
                    focus_btn.remove_css_class("can-focus");
                    focus_btn.set_visible(false);
                }
                content_box.set_margin_end(0);
                focus_btn
            };

            if let Some(section) = &section
                && super::reorder_execution::row_is_workspace_folder_root(&tree_row)
            {
                section.ensure_folder_reorder_row_surfaces(&overlay);
            }
            let show_reorder_handle = section.as_ref().is_some_and(|section| {
                super::reorder_execution::workspace_folder_reorder_handle_should_show(
                    section, &tree_row,
                )
            });
            let drag_handle = super::reorder_execution::content_box_drag_handle(&content_box);
            if let Some(drag_handle) = &drag_handle {
                drag_handle.set_visible(show_reorder_handle);
                drag_handle.set_sensitive(show_reorder_handle);
            }

            if file_item.is_dir()
                && !file_item.is_placeholder()
                && let Some(section) = &section
                && let Some(path) = file_item.path()
            {
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
                if let Some(section) = &section {
                    let imp = section.imp();
                    *imp.context_target.borrow_mut() =
                        FileContextTarget::from_item(&expander, &file_item);
                    let sw = section.downgrade();
                    glib::idle_add_local_once(move || {
                        if let Some(s) = sw.upgrade() {
                            s.begin_rename();
                        }
                    });
                }
            }

            if let Some(section) = &section {
                apply_file_tree_row_accessibility(FileTreeRowAccessibilityTarget {
                    overlay: &overlay,
                    drag_handle: drag_handle.as_ref(),
                    focus_btn: focus_btn.as_ref(),
                    file_item: &file_item,
                    tree_row: &tree_row,
                    section,
                    position: list_item.position(),
                    show_reorder_handle,
                    show_focus,
                });
                install_expanded_accessibility_hook(&overlay, &tree_row, &file_item, section);
                section.install_expanded_watch_hook(&tree_row);
                section.apply_collapse_projection(&overlay, &expander, &tree_row);
                super::sync_file_row_state_for_overlay(section, &overlay);
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

    let section_weak = imp.obj().downgrade();
    factory.connect_unbind(move |_factory, list_item| {
        let list_item = list_item
            .downcast_ref::<gtk4::ListItem>()
            .expect("item is ListItem");

        let tree_row = list_item.item().and_downcast::<gtk4::TreeListRow>();

        if let Some(overlay) = list_item.child().and_downcast::<gtk4::Overlay>()
            && let Some(expander) = overlay.child().and_downcast::<gtk4::TreeExpander>()
        {
            expander.set_list_row(None::<&gtk4::TreeListRow>);
            expander.remove_css_class(super::collapse_execution::COLLAPSING_CLASS);
            if let Some(content_box) = expander.child().and_downcast::<gtk4::Box>()
                && let (_, Some(label)) = row_icon_and_label(&content_box)
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
                if let Some(drag_handle) =
                    super::reorder_execution::content_box_drag_handle(&content_box)
                {
                    drag_handle.set_visible(false);
                    drag_handle.set_sensitive(false);
                    accessibility::set_hidden(&drag_handle, true);
                    accessibility::set_disabled(&drag_handle, true);
                }
                content_box.set_margin_end(0);
            }

            super::reset_file_row_state_for_overlay(&overlay);
            super::reorder_execution::reset_reorder_row_for_unbind(&overlay);
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
                    .context_target
                    .borrow()
                    .as_ref()
                    .is_some_and(|target| target.expander == expander);
                if context_matches {
                    section.imp().context_target.borrow_mut().take();
                }
            }
        }

        let Some(tree_row) = tree_row else { return };
        let Some(file_item) = tree_row.item().and_downcast::<FileTreeItem>() else {
            return;
        };

        if file_item.is_dir()
            && let Some(section) = section_weak.upgrade()
            && let Some(ref path) = file_item.path()
        {
            section.imp().dir_rows.borrow_mut().remove(path.as_path());
        }
    });

    imp.file_tree_view.set_factory(Some(&factory));
}

/// Own the expander arrow's click (`slice-large-directory-collapse` D5).
///
/// GTK's click gesture lives on the expander's `expander` icon child, which
/// GTK re-creates whenever the row's expandability changes, and its release
/// handler toggles the row through `listitem.toggle-expand` (ledger A119).
/// This capture-phase gesture on the expander itself lets the press through,
/// so focus on click, selection and double-press activation stay GTK's, and
/// claims the sequence on a release inside the current icon, before the
/// icon's gesture sees it, so the collapse and expand entry points own the
/// toggle whatever icon GTK has created since.
fn install_arrow_gesture(expander: &gtk4::TreeExpander) {
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_PRIMARY);
    gesture.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let expander_weak = expander.downgrade();
    gesture.connect_released(move |gesture, _presses, x, y| {
        let Some(expander) = expander_weak.upgrade() else {
            return;
        };
        let Some(icon) = arrow_icon(&expander) else {
            return;
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Graphene points use f32 coordinates while GTK pointer events report f64 coordinates."
        )]
        let press = gtk4::graphene::Point::new(x as f32, y as f32);
        let inside = expander
            .compute_point(&icon, &press)
            .is_some_and(|point| icon.contains(f64::from(point.x()), f64::from(point.y())));
        if !inside {
            return;
        }
        let Some(row) = expander.list_row() else {
            return;
        };
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let mut current: Option<gtk4::Widget> = expander.parent();
        while let Some(widget) = current {
            if let Some(section) = widget.downcast_ref::<super::LushtextWorkspaceSection>() {
                if section.row_inside_collapsing_subtree(&row) {
                    // On `main` this row is already gone (design R6.5).
                } else if section.row_shows_expanded(&row) {
                    section.collapse_directory(&row, super::CollapseOrigin::Arrow);
                } else if section.expand_directory(&row) {
                    let _ = expander.activate_action("listitem.scroll-to", None);
                }
                return;
            }
            current = widget.parent();
        }
    });
    expander.add_controller(gesture);
}

/// The expander's current disclosure icon (CSS name `expander`), if the row
/// is expandable.
fn arrow_icon(expander: &gtk4::TreeExpander) -> Option<gtk4::Widget> {
    let mut child = expander.first_child();
    while let Some(widget) = child {
        if widget.css_name() == "expander" {
            return Some(widget);
        }
        child = widget.next_sibling();
    }
    None
}

/// The row's icon and label: the content box's first `GtkImage` and first
/// `GtkLabel` children. Read by type, not position, because the drag handle
/// (design D17) and an inline-rename entry come and go around them.
pub(super) fn row_icon_and_label(
    content_box: &gtk4::Box,
) -> (Option<gtk4::Image>, Option<gtk4::Label>) {
    let mut icon = None;
    let mut label = None;
    let mut child = content_box.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if icon.is_none()
            && let Some(image) = widget.downcast_ref::<gtk4::Image>()
        {
            icon = Some(image.clone());
        } else if label.is_none()
            && let Some(text) = widget.downcast_ref::<gtk4::Label>()
        {
            label = Some(text.clone());
        }
        if icon.is_some() && label.is_some() {
            break;
        }
    }
    (icon, label)
}

/// Give one row the Focus Folder button, on demand (design D17), and return it.
///
/// Only a non-root, non-empty directory row can be focused, so the button,
/// its image, its click handler and the overlay's hover controller that shows
/// it are created when such a row is first bound, never in the factory's
/// setup. A recycled row keeps them, hidden and without `can-focus`, while it
/// shows anything else.
fn ensure_focus_button(overlay: &gtk4::Overlay) -> gtk4::Button {
    if let Some(focus_btn) = focus_button_for_overlay(overlay) {
        return focus_btn;
    }
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

    let overlay_weak = overlay.downgrade();
    focus_btn.connect_clicked(move |_| {
        if super::reorder_execution::folder_reorder_drag_is_active() {
            return;
        }
        if let Some(overlay) = overlay_weak.upgrade()
            && let Some(path) = super::file_row_overlay_path(&overlay)
        {
            // Recycled row widgets do not know their section, so resolve the
            // owning section at click time from the live widget tree.
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
    overlay.add_overlay(&focus_btn);
    overlay.set_measure_overlay(&focus_btn, false);

    let motion = gtk4::EventControllerMotion::new();
    let btn_enter = focus_btn.downgrade();
    motion.connect_enter(move |_, _, _| {
        let Some(btn_enter) = btn_enter.upgrade() else {
            return;
        };
        if super::reorder_execution::folder_reorder_drag_is_active() {
            btn_enter.set_visible(false);
            return;
        }
        if btn_enter.has_css_class("can-focus") {
            btn_enter.set_visible(true);
        }
    });
    let btn_leave = focus_btn.downgrade();
    motion.connect_leave(move |_| {
        if let Some(btn_leave) = btn_leave.upgrade() {
            btn_leave.set_visible(false);
        }
    });
    overlay.add_controller(motion);
    focus_btn
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
