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
use super::policy::{self, GroupedRowKind};
use crate::ui::accessibility::{self, RowAccessibility};
use crate::ui::row_text::LushtextRowText;
use lushtext_model::workspace::palette::{PaletteCategory, PaletteSearchRow, SearchMode};

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

            // Row text measured without laying the text out at the allocated
            // width (`lighten-notes-browser-rows` D2, ledger A108). The name
            // and the subtitle share the row by their natural widths, so both
            // report their shaped width (width parity with `GtkLabel`).
            let name_label = LushtextRowText::new();
            name_label.set_natural_width_from_text(true);
            name_label.set_halign(gtk4::Align::Start);
            name_label.set_hexpand(true);
            name_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);

            let section_separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
            section_separator.set_hexpand(true);
            section_separator.set_visible(false);

            let subtitle_label = LushtextRowText::new();
            subtitle_label.set_natural_width_from_text(true);
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
            .and_downcast::<LushtextRowText>()
            .expect("palette row should start with the title label");
        let section_separator = name_label
            .next_sibling()
            .and_downcast::<gtk4::Separator>()
            .expect("palette row title should be followed by the section separator");
        let subtitle_label = section_separator
            .next_sibling()
            .and_downcast::<LushtextRowText>()
            .expect("palette row separator should be followed by the subtitle label");

        list_item.set_activatable(item.is_activatable());
        list_item.set_selectable(item.is_activatable());
        name_label.set_text(&item.display_name());
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
            subtitle_label.set_text(&item.subtitle());
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
                self.category_model.full_len(),
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

/// Convert a background search row DTO into its main-thread `PaletteItem`.
fn palette_row_into_item(
    row: PaletteSearchRow,
    mode: SearchMode,
    workspace_group_label: &str,
) -> PaletteItem {
    match row {
        PaletteSearchRow::More { category, hidden } => PaletteItem::new_more(
            category,
            hidden,
            category.label(mode, workspace_group_label),
        ),
        PaletteSearchRow::Header { label, .. } => PaletteItem::new_header_raw(label),
        PaletteSearchRow::File {
            display_name,
            subtitle,
            file_path,
        } => PaletteItem::new_file_raw(display_name, subtitle, file_path),
        PaletteSearchRow::Command {
            display_name,
            subtitle,
            action_id,
        } => PaletteItem::new_command_raw(display_name, subtitle, action_id),
        PaletteSearchRow::Note {
            display_name,
            subtitle,
            target,
        } => PaletteItem::new_note_raw(display_name, subtitle, target),
    }
}

/// The category of each grouped row: a header's own category, carried down
/// to the rows that follow it (review finding R3: matching header labels
/// confused a workspace named like an absent group). Rows before any header
/// (Commands mode) are commands.
fn grouped_row_categories(
    rows: &[PaletteSearchRow],
    mode: SearchMode,
) -> Vec<Option<PaletteCategory>> {
    let mut current = (mode == SearchMode::Commands).then_some(PaletteCategory::Commands);
    rows.iter()
        .map(|row| match row {
            PaletteSearchRow::Header { category, .. } => {
                current = Some(*category);
                current
            }
            PaletteSearchRow::More { category, .. } => Some(*category),
            _ => current,
        })
        .collect()
}

/// The kind of each grouped row, for `policy::leave_selection`.
fn grouped_row_kinds(items: &[PaletteItem]) -> Vec<GroupedRowKind> {
    items
        .iter()
        .map(
            |item| match (item.is_header(), item.is_more(), item.category()) {
                (false, true, Some(category)) => GroupedRowKind::More(category),
                (false, false, Some(category)) => GroupedRowKind::Result(category),
                _ => GroupedRowKind::Header,
            },
        )
        .collect()
}

impl LushtextCommandPalette {
    /// Give the palette's compact controls durable accessible names for screen
    /// readers and AT-SPI smoke assertions.
    pub(super) fn apply_accessibility_metadata(&self) {
        accessibility::set_labelled_description(
            &*self.search_entry,
            "Command palette query",
            "Search across open tabs, workspace files, notes, and commands",
        );
        accessibility::set_labelled_description(
            &*self.mode_dropdown,
            "Command palette mode",
            "Choose which result category to search",
        );
        accessibility::set_value_text(&*self.mode_dropdown, self.mode.get().label());
        accessibility::set_role(&*self.results_view, gtk4::AccessibleRole::List);
        accessibility::set_labelled_description(
            &*self.results_view,
            "Command palette results",
            "Matching files, notes, and commands",
        );
        accessibility::set_role(&*self.category_view, gtk4::AccessibleRole::List);
        accessibility::set_labelled_description(
            &*self.category_back_button,
            "Back to all results",
            "Return to the grouped command palette results",
        );
        accessibility::set_role(&*self.no_results_label, gtk4::AccessibleRole::Status);
        accessibility::set_labelled_description(
            &*self.no_results_label,
            "Command palette no results",
            "No matching files, notes, or commands",
        );
        self.refresh_accessibility_state();
    }

    /// Query stage 4's widget mutation: replace the whole visible model with one splice.
    ///
    /// `query_execution::settle_query_flight` owns stage 4 and calls this once it
    /// has proved the completion is still current.
    pub(super) fn publish_search_rows(&self, rows: Vec<PaletteSearchRow>, query: &str) {
        let mode = self.mode.get();
        let workspace_group_label = self.workspace_group_label.borrow().clone();
        let categories = grouped_row_categories(&rows, mode);
        let items: Vec<PaletteItem> = rows
            .into_iter()
            .zip(categories)
            .map(|(row, category)| {
                let item = palette_row_into_item(row, mode, &workspace_group_label);
                item.set_category(category);
                item
            })
            .collect();

        // One splice keeps GTK projection work independent of the match count.
        let old_count = self.results_store.n_items();
        self.results_store.splice(0, old_count, &items);

        let has_results = items.iter().any(PaletteItem::is_result);
        self.no_results_label
            .set_visible(policy::no_results_visible(
                has_results,
                policy::QueryTextState::from_is_empty(query.is_empty()),
            ));

        let selected = match self.leaving_category.take() {
            Some(category) => policy::leave_selection(&grouped_row_kinds(&items), category),
            None => self.first_activatable_position(),
        };
        if let Some(position) = selected
            && let Some(selection) = self.selection_model()
        {
            selection.set_selected(position);
            self.results_view
                .scroll_to(position, gtk4::ListScrollFlags::NONE, None);
        }
        self.refresh_accessibility_state();
    }

    /// Move the selection one row (`delta` ±1) or one page in the visible
    /// view: headers skipped in the grouped view, an O(1) clamp in the
    /// category view.
    pub(super) fn move_selection(&self, delta: i32) {
        if self.category.get().is_some() {
            let Some(selection) = self.category_selection() else {
                return;
            };
            if !self.category_rows_current() {
                return;
            }
            // A key lands where it would on the full ranking: a staged
            // first publish still growing lists the rest first (D5).
            self.finish_category_growth();
            let current = Some(selection.selected()).filter(|p| *p != gtk4::INVALID_LIST_POSITION);
            let Some(position) =
                policy::next_category_row(current, self.category_model.len(), delta)
            else {
                return;
            };
            selection.set_selected(position);
            self.category_view
                .scroll_to(position, gtk4::ListScrollFlags::NONE, None);
            self.refresh_accessibility_state();
            return;
        }
        let Some(selection) = self.selection_model() else {
            return;
        };
        let n = self.results_store.n_items();
        if n == 0 {
            return;
        }
        let current = selection.selected();
        let flags = self.activatable_flags();
        let new_pos = if delta.unsigned_abs() > 1 {
            policy::page_activatable(&flags, current, delta)
        } else {
            policy::next_activatable(&flags, current, delta)
        };
        let Some(new_pos) = new_pos else {
            return;
        };
        selection.set_selected(new_pos);
        self.results_view
            .scroll_to(new_pos, gtk4::ListScrollFlags::NONE, None);
        self.refresh_accessibility_state();
    }

    /// Activate the currently selected actionable row of the visible view.
    ///
    /// Presentation-only source headers are ignored before forwarding the row
    /// to the registered activation callback.
    pub fn activate_selected(&self) {
        if self.category.get().is_some() {
            if let Some(selection) = self.category_selection() {
                self.activate_category_at(selection.selected());
            }
            return;
        }
        let Some(selection) = self.selection_model() else {
            return;
        };
        self.activate_at(selection.selected());
    }

    pub(super) fn activate_at(&self, position: u32) {
        let Some(item) = self.results_store.item(position) else {
            return;
        };
        let Some(palette_item) = item.downcast_ref::<PaletteItem>() else {
            return;
        };
        self.activate_item(palette_item);
    }

    pub(super) fn activate_category_at(&self, position: u32) {
        // Rows left by a cancelled retirement belong to the category shown
        // before; they are hidden and never activated.
        if !self.category_rows_current() || position >= self.category_model.len() {
            return;
        }
        let Some(item) = self
            .category_model
            .item(position)
            .and_downcast::<PaletteItem>()
        else {
            return;
        };
        self.activate_item(&item);
    }

    /// A "More" row opens its category view inside the palette and never
    /// reaches the window; any other activatable row goes to the callback.
    fn activate_item(&self, item: &PaletteItem) {
        if item.is_more() {
            if let Some(category) = item.category() {
                let _ = self.enter_category(category);
            }
            return;
        }
        if !item.is_activatable() {
            return;
        }
        if let Some(ref cb) = *self.activate_callback.borrow() {
            cb(item);
        }
    }

    /// Project the visible model into the activatable-flag sequence policy reads.
    fn activatable_flags(&self) -> Vec<bool> {
        (0..self.results_store.n_items())
            .map(|position| {
                self.results_store
                    .item(position)
                    .and_downcast_ref::<PaletteItem>()
                    .is_some_and(PaletteItem::is_activatable)
            })
            .collect()
    }

    /// Find the first row that can actually be opened or executed.
    fn first_activatable_position(&self) -> Option<u32> {
        policy::first_activatable(&self.activatable_flags())
    }

    pub(super) fn selection_model(&self) -> Option<gtk4::SingleSelection> {
        self.results_view
            .model()
            .and_then(|m| m.downcast::<gtk4::SingleSelection>().ok())
    }

    /// The selected position of the visible view, when one is selected.
    /// Read through `try_get`, so the evidence surface stays honest on a
    /// disposed widget.
    pub(super) fn visible_selected_position(&self) -> Option<u32> {
        if self.category.get().is_some() && !self.category_rows_current() {
            return None;
        }
        let view = if self.category.get().is_some() {
            self.category_view.try_get()
        } else {
            self.results_view.try_get()
        };
        view.and_then(|view| view.model())
            .and_then(|model| model.downcast::<gtk4::SingleSelection>().ok())
            .map(|selection| selection.selected())
            .filter(|position| *position != gtk4::INVALID_LIST_POSITION)
    }

    pub(super) fn category_selection(&self) -> Option<gtk4::SingleSelection> {
        self.category_view
            .model()
            .and_then(|m| m.downcast::<gtk4::SingleSelection>().ok())
    }

    /// Project live palette search state into accessible states and values.
    pub(super) fn refresh_accessibility_state(&self) {
        let searching = self.searching.get();
        let in_category = self.category.get().is_some();
        let (result_count, selected_name) = if in_category {
            // The model's length and the selection's own item: no walk.
            // Before the view's first publish it lists no rows of its own.
            let current = self.category_rows_current();
            let selected = self
                .category_selection()
                .filter(|_| current)
                .and_then(|selection| selection.selected_item())
                .and_downcast::<PaletteItem>()
                .map(|item| item.display_name());
            (
                if current {
                    usize::try_from(self.category_model.full_len()).unwrap_or(usize::MAX)
                } else {
                    0
                },
                selected,
            )
        } else {
            let results = (0..self.results_store.n_items())
                .filter(|position| {
                    self.results_store
                        .item(*position)
                        .and_downcast_ref::<PaletteItem>()
                        .is_some_and(PaletteItem::is_result)
                })
                .count();
            let selected = self
                .selection_model()
                .and_then(|selection| self.results_store.item(selection.selected()))
                .and_downcast::<PaletteItem>()
                .filter(PaletteItem::is_activatable)
                .map(|item| item.display_name());
            (results, selected)
        };
        // The label's own visibility already encodes the non-empty-query half of
        // the no-results decision, set by the publish; this only adds the live
        // "and nothing is listed, and we are not still searching" half that
        // AT-SPI hiding needs.
        let status_visible = self.no_results_label.is_visible() && result_count == 0 && !searching;
        let visible_list: &gtk4::ListView = if in_category {
            &self.category_view
        } else {
            &self.results_view
        };

        accessibility::set_busy(&*self.search_entry, searching);
        accessibility::set_busy(visible_list, searching);
        accessibility::set_hidden(&*self.no_results_label, !status_visible);
        accessibility::set_hidden(visible_list, status_visible);

        let value_text =
            policy::accessible_value_text(selected_name.as_deref(), searching, result_count);
        accessibility::set_value_text(visible_list, &value_text);
        accessibility::set_value_text(&*self.no_results_label, &policy::result_count_text(0));
    }
}

#[cfg(test)]
mod tests {
    use super::grouped_row_categories;
    use lushtext_model::workspace::palette::{PaletteCategory, PaletteSearchRow, SearchMode};
    use std::path::PathBuf;

    fn file(name: &str) -> PaletteSearchRow {
        PaletteSearchRow::File {
            display_name: name.to_string(),
            subtitle: String::new(),
            file_path: PathBuf::from(name),
        }
    }

    /// Review finding R3: a workspace whose label equals another group's
    /// ("Open Tabs") is not mistaken for that group when the other group is
    /// absent; the grouped search knows each header's category.
    #[test]
    fn a_workspace_labelled_like_an_absent_group_keeps_its_category() {
        let rows = vec![
            PaletteSearchRow::Header {
                label: "Open Tabs".to_string(),
                category: PaletteCategory::WorkspaceFiles,
            },
            file("a.rs"),
            PaletteSearchRow::More {
                category: PaletteCategory::WorkspaceFiles,
                hidden: 3,
            },
        ];
        assert_eq!(
            grouped_row_categories(&rows, SearchMode::Files),
            vec![Some(PaletteCategory::WorkspaceFiles); 3]
        );
    }
}
