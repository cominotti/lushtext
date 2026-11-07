// SPDX-License-Identifier: GPL-3.0-or-later

//! The palette's row factory, shared by its two lists: the grouped results
//! and the category view (`drill-down-palette-categories` D2). One row
//! widget and one accessible projection; a row the category model retargets
//! in place (`content-changed`) is projected again without a rebind, and a
//! row's set size is its own list's length (the category model's length in
//! the category view, never a walk).
//!
//! # Role
//!
//! This module carries **no role**. It is a **called presentation surface** of
//! `WFR-COMMAND-PALETTE` (list-factory row projection), extending `imp`.
//! Named in that workflow's matrix row.

use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

use super::imp::LushtextCommandPalette;
use super::item::PaletteItem;
use super::policy;
use crate::ui::accessibility::{self, RowAccessibility};

/// Which of the palette's two lists a row factory projects.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PaletteView {
    Grouped,
    Category,
}

impl LushtextCommandPalette {
    pub(super) fn setup_factory(&self, view: PaletteView) {
        let factory = gtk4::SignalListItemFactory::new();

        factory.connect_setup(|_, list_item| {
            // SignalListItemFactory uses GObject signals to set up and bind
            // recycled rows. GTK passes rows as generic Objects, so handlers
            // downcast before attaching or updating row widgets.
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("ListItem");

            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            row.set_margin_start(8);
            row.set_margin_end(8);
            row.set_margin_top(4);
            row.set_margin_bottom(4);

            let name_label = gtk4::Label::new(None);
            name_label.set_halign(gtk4::Align::Start);
            name_label.set_hexpand(true);
            name_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);

            let section_separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
            section_separator.set_hexpand(true);
            section_separator.set_visible(false);

            let subtitle_label = gtk4::Label::new(None);
            subtitle_label.set_halign(gtk4::Align::End);
            subtitle_label.add_css_class("dim-label");
            subtitle_label.add_css_class("caption");
            subtitle_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);

            row.append(&name_label);
            row.append(&section_separator);
            row.append(&subtitle_label);

            list_item.set_child(Some(&row));
        });

        let bind_obj_weak = self.obj().downgrade();
        factory.connect_bind(move |_, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("ListItem");
            let Some(obj) = bind_obj_weak.upgrade() else {
                return;
            };
            let imp = obj.imp();
            imp.project_list_item(list_item, view);
            let item = list_item
                .item()
                .and_downcast::<PaletteItem>()
                .expect("PaletteItem");
            // A category row retargeted by a publish projects again in place.
            let item_weak = list_item.downgrade();
            let obj_weak = obj.downgrade();
            let handler = item.connect_content_changed(move |_| {
                if let (Some(list_item), Some(obj)) = (item_weak.upgrade(), obj_weak.upgrade()) {
                    obj.imp().project_list_item(&list_item, view);
                }
            });
            if let Some((previous, handler)) = imp
                .bound_rows
                .borrow_mut()
                .insert(list_item.clone(), (item, handler))
            {
                previous.disconnect(handler);
            }
        });

        let unbind_obj_weak = self.obj().downgrade();
        factory.connect_unbind(move |_, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("ListItem");
            if let Some(obj) = unbind_obj_weak.upgrade()
                && let Some((item, handler)) = obj.imp().bound_rows.borrow_mut().remove(list_item)
            {
                item.disconnect(handler);
            }
            if let Some(row) = list_item.child().and_downcast::<gtk4::Box>() {
                accessibility::clear_row_accessibility(&row);
            }
        });

        match view {
            PaletteView::Grouped => self.results_view.set_factory(Some(&factory)),
            PaletteView::Category => self.category_view.set_factory(Some(&factory)),
        }
    }

    /// Project one bound row: its labels, its style, and its accessible
    /// metadata, with the set size of its own list (the category model's
    /// length in the category view, never a walk).
    fn project_list_item(&self, list_item: &gtk4::ListItem, view: PaletteView) {
        let Some(item) = list_item.item().and_downcast::<PaletteItem>() else {
            return;
        };
        let Some(row) = list_item.child().and_downcast::<gtk4::Box>() else {
            return;
        };
        let name_label = row
            .first_child()
            .and_downcast::<gtk4::Label>()
            .expect("palette row should start with the title label");
        let section_separator = name_label
            .next_sibling()
            .and_downcast::<gtk4::Separator>()
            .expect("palette row title should be followed by the section separator");
        let subtitle_label = section_separator
            .next_sibling()
            .and_downcast::<gtk4::Label>()
            .expect("palette row separator should be followed by the subtitle label");

        list_item.set_activatable(item.is_activatable());
        list_item.set_selectable(item.is_activatable());
        name_label.set_label(&item.display_name());
        if item.is_header() {
            row.add_css_class("command-palette-section-row");
            name_label.add_css_class("command-palette-section-header");
            name_label.set_hexpand(false);
            name_label.set_ellipsize(gtk4::pango::EllipsizeMode::None);
            section_separator.set_visible(true);
            subtitle_label.set_visible(false);
        } else {
            row.remove_css_class("command-palette-section-row");
            name_label.remove_css_class("command-palette-section-header");
            name_label.set_hexpand(true);
            name_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
            section_separator.set_visible(false);
            subtitle_label.set_visible(!item.is_more());
            subtitle_label.set_label(&item.subtitle());
        }
        if item.is_more() {
            row.add_css_class("command-palette-more-row");
        } else {
            row.remove_css_class("command-palette-more-row");
        }

        let position = i32::try_from(list_item.position()).unwrap_or(i32::MAX - 1) + 1;
        let (set_size, selected) = match view {
            PaletteView::Grouped => (
                self.results_store.n_items(),
                self.selection_model()
                    .is_some_and(|selection| selection.selected() == list_item.position()),
            ),
            PaletteView::Category => (
                self.category_model.len(),
                self.category_selection()
                    .is_some_and(|selection| selection.selected() == list_item.position()),
            ),
        };
        let set_size = i32::try_from(set_size).unwrap_or(i32::MAX);
        apply_palette_row_accessibility(&row, &item, selected, position, set_size);
    }
}

/// Project one row's accessible name, description, selection and position
/// (the "More" row's name states its count and category).
pub(super) fn apply_palette_row_accessibility(
    row: &gtk4::Box,
    item: &PaletteItem,
    selected: bool,
    position: i32,
    set_size: i32,
) {
    if item.is_more() {
        let label = item.category_label();
        accessibility::apply_row_accessibility(
            row,
            RowAccessibility::new(&policy::more_row_accessible_name(
                item.hidden_count(),
                &label,
            ))
            .description(&policy::more_row_description(&label))
            .selected(selected)
            .position(position, set_size),
        );
        return;
    }
    let label = if item.is_header() {
        format!("{} section", item.display_name())
    } else if item.is_file() {
        format!("Open file {}", item.display_name())
    } else if item.is_note() {
        format!("Open note {}", item.display_name())
    } else {
        format!("Run command {}", item.display_name())
    };
    let description = if item.is_header() {
        "Command palette result group".to_string()
    } else if let Some(path) = item.file_path() {
        format!("{}; {}", item.subtitle(), path.display())
    } else {
        item.subtitle()
    };
    accessibility::apply_row_accessibility(
        row,
        RowAccessibility::new(&label)
            .description(&description)
            .selected(selected)
            .position(position, set_size),
    );
}
