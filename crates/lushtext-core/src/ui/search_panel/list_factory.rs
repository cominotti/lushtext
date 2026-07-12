// SPDX-License-Identifier: GPL-3.0-or-later

//! Results-list factory and row rendering for the search panel.
//!
//! This stays in the driving-adapter layer because it owns GTK row creation,
//! result-item binding, and preview-mode checkbox wiring. It is split out of
//! `imp.rs` so the private implementation file can focus on template wiring
//! and high-level widget setup instead of row-by-row rendering details.

use std::path::PathBuf;

use glib::subclass::prelude::ObjectSubclassExt;
use gtk_lush_signals::BindingBag;
use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;
use gtk4::{self, gio, glib};

use super::imp;
use super::item::SearchResultItem;
use crate::ui::accessibility::{self, RowAccessibility};

impl imp::LushtextSearchPanel {
    /// Set up the `GtkTreeListModel` and `ListView` factory for grouped results.
    pub(super) fn setup_results_list(&self) {
        let root_store = self.runtime.root_store.clone();
        // Use WeakRef to the panel so the callback sees live file groups
        // instead of any state captured at construction time.
        let panel_weak = self.obj().downgrade();

        // TreeListModel: root items are file headers, children are match rows.
        let tree_model = gtk4::TreeListModel::new(
            root_store,
            false, // passthrough = false (we need TreeListRow wrappers)
            false, // autoexpand = false (NEVER true per project rules)
            move |item| -> Option<gio::ListModel> {
                let panel = panel_weak.upgrade()?;
                let result_item = item.downcast_ref::<SearchResultItem>()?;
                if result_item.is_file_item() {
                    let path = PathBuf::from(result_item.file_path());
                    panel
                        .imp()
                        .runtime
                        .file_groups
                        .borrow()
                        .get(&path)
                        .map(|group| group.child_store.clone().upcast())
                } else {
                    None
                }
            },
        );

        let selection = gtk4::SingleSelection::new(Some(tree_model));
        self.results_list.set_model(Some(&selection));

        let factory = gtk4::SignalListItemFactory::new();
        let setup_panel_weak = self.obj().downgrade();
        factory.connect_setup(move |_, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("ListItem");

            let expander = gtk4::TreeExpander::new();

            // margin-end keeps the overlay scrollbar from obscuring the count badge.
            let content_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            content_box.set_margin_start(4);
            content_box.set_margin_end(24);
            content_box.set_margin_top(2);
            content_box.set_margin_bottom(2);

            let preview_checkbox = gtk4::CheckButton::new();
            preview_checkbox.add_css_class("preview-check");
            preview_checkbox.set_visible(false);
            accessibility::set_hidden(&preview_checkbox, true);

            let panel_weak = setup_panel_weak.clone();
            preview_checkbox.connect_toggled(move |checkbox| {
                accessibility::set_pressed(checkbox, checkbox.is_active());
                let Some(panel) = panel_weak.upgrade() else {
                    return;
                };
                let Some(match_id) = preview_match_id_for_checkbox(&panel, checkbox) else {
                    return;
                };
                let imp = panel.imp();
                let mut checked_ids = imp.preview.checked_match_ids.borrow_mut();
                if checkbox.is_active() {
                    checked_ids.insert(match_id);
                } else {
                    checked_ids.remove(&match_id);
                }
                let has_checked = !checked_ids.is_empty();
                drop(checked_ids);
                panel.refresh_preview_summary();
                imp.replace_all_button.set_sensitive(has_checked);
            });

            let file_label = gtk4::Label::new(None);
            file_label.set_hexpand(true);
            file_label.set_xalign(0.0);
            file_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
            file_label.add_css_class("heading");

            let count_badge = gtk4::Label::new(None);
            count_badge.add_css_class("caption");

            let line_num_label = gtk4::Label::new(None);
            line_num_label.add_css_class("caption");
            line_num_label.add_css_class("dim-label");
            line_num_label.add_css_class("monospace");
            line_num_label.set_width_chars(5);
            line_num_label.set_xalign(1.0);

            let line_content_label = gtk4::Label::new(None);
            line_content_label.set_hexpand(true);
            line_content_label.set_xalign(0.0);
            line_content_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            line_content_label.add_css_class("monospace");

            content_box.append(&preview_checkbox);
            content_box.append(&file_label);
            content_box.append(&count_badge);
            content_box.append(&line_num_label);
            content_box.append(&line_content_label);

            expander.set_child(Some(&content_box));
            list_item.set_child(Some(&expander));
        });

        let bind_panel_weak = self.obj().downgrade();
        factory.connect_bind(move |_, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("ListItem");
            let Some(expander) = list_item
                .child()
                .and_then(|w| w.downcast::<gtk4::TreeExpander>().ok())
            else {
                return;
            };

            let Some(row) = list_item
                .item()
                .and_then(|item| item.downcast::<gtk4::TreeListRow>().ok())
            else {
                return;
            };
            expander.set_list_row(Some(&row));

            let Some(result_item) = row.item().and_downcast::<SearchResultItem>() else {
                return;
            };

            let Some(content_box) = expander
                .child()
                .and_then(|w| w.downcast::<gtk4::Box>().ok())
            else {
                return;
            };

            let preview_checkbox = content_box
                .first_child()
                .and_then(|w| w.downcast::<gtk4::CheckButton>().ok());
            if let Some(ref checkbox) = preview_checkbox {
                checkbox.set_visible(false);
                accessibility::set_hidden(checkbox, true);
            }

            let file_label = preview_checkbox
                .as_ref()
                .and_then(gtk4::prelude::WidgetExt::next_sibling)
                .and_then(|w| w.downcast::<gtk4::Label>().ok());
            let count_badge = file_label
                .as_ref()
                .and_then(gtk4::prelude::WidgetExt::next_sibling)
                .and_then(|w| w.downcast::<gtk4::Label>().ok());
            let line_num_label = count_badge
                .as_ref()
                .and_then(gtk4::prelude::WidgetExt::next_sibling)
                .and_then(|w| w.downcast::<gtk4::Label>().ok());
            let line_content_label = line_num_label
                .as_ref()
                .and_then(gtk4::prelude::WidgetExt::next_sibling)
                .and_then(|w| w.downcast::<gtk4::Label>().ok());

            if result_item.is_file_item() {
                apply_result_row_accessibility(&expander, &result_item, Some(row.is_expanded()));
                if let Some(ref label) = file_label {
                    label.set_text(&result_item.display_path());
                    label.set_visible(true);
                }
                if let Some(ref badge) = count_badge {
                    badge.set_visible(true);
                    let binding = result_item
                        .bind_property("match-count", badge, "label")
                        .transform_to(|_: &glib::Binding, value: &glib::Value| {
                            let count: u32 = value.get().ok()?;
                            Some(format!("{count}").to_value())
                        })
                        .sync_create()
                        .build();
                    let bindings = BindingBag::new();
                    bindings.track(binding);
                    // SAFETY: the binding bag is removed in connect_unbind
                    // below using the same storage key on the recycled ListItem.
                    unsafe {
                        list_item.set_data("count-bindings", bindings);
                    }
                }
                if let Some(ref label) = line_num_label {
                    label.set_visible(false);
                }
                if let Some(ref label) = line_content_label {
                    label.set_visible(false);
                }
            } else {
                apply_result_row_accessibility(&expander, &result_item, None);
                let in_preview = bind_panel_weak
                    .upgrade()
                    .is_some_and(|p| p.imp().preview.preview_mode.get());

                if let Some(ref label) = file_label {
                    label.set_visible(false);
                }
                if let Some(ref badge) = count_badge {
                    badge.set_visible(false);
                }
                if let Some(ref label) = line_num_label {
                    label.set_text(&format!("{}", result_item.line_number()));
                    label.set_visible(true);
                }

                if in_preview {
                    if let Some(panel) = bind_panel_weak.upgrade() {
                        let imp = panel.imp();
                        let match_id = result_item.match_id();
                        let outcome = imp.preview.preview_outcome.borrow();
                        let match_idx = outcome
                            .as_ref()
                            .and_then(|outcome| outcome.preview_index(match_id));

                        if let Some(idx) = match_idx {
                            let r = &outcome.as_ref().expect("preview outcome").replacements[idx];
                            let replacement_line_number = r.line_number;
                            let original = &r.original_line;
                            let replaced = &r.replaced_line;
                            let start = r.match_range.start.min(original.len());
                            let end = r.match_range.end.min(original.len());

                            let markup = render_preview_markup(original, replaced, start, end);
                            let is_checked =
                                imp.preview.checked_match_ids.borrow().contains(&match_id);
                            drop(outcome);

                            if let Some(ref label) = line_content_label {
                                label.set_markup(&markup);
                                label.set_visible(true);
                            }

                            if let Some(ref checkbox) = preview_checkbox {
                                accessibility::set_labelled_description(
                                    checkbox,
                                    &format!(
                                        "Include replacement at line {replacement_line_number}"
                                    ),
                                    "Toggle whether this replacement is applied",
                                );
                                accessibility::set_pressed(checkbox, is_checked);
                                accessibility::set_hidden(checkbox, false);
                                checkbox.set_active(is_checked);
                                checkbox.set_visible(true);
                            }
                        } else {
                            drop(outcome);
                            if let Some(ref label) = line_content_label {
                                let content = result_item.line_content();
                                let markup = render_match_markup(
                                    &content,
                                    result_item.match_start() as usize,
                                    result_item.match_end() as usize,
                                );
                                label.set_markup(&markup);
                                label.set_visible(true);
                            }
                        }
                    }
                } else if let Some(ref label) = line_content_label {
                    let content = result_item.line_content();
                    let markup = render_match_markup(
                        &content,
                        result_item.match_start() as usize,
                        result_item.match_end() as usize,
                    );
                    label.set_markup(&markup);
                    label.set_visible(true);
                }
            }

            // Disable expander gesture for match rows, mirroring the sidebar tree fix.
            for controller in expander.observe_controllers().into_iter().flatten() {
                if let Ok(gesture) = controller.downcast::<gtk4::GestureClick>() {
                    if result_item.is_match_item() {
                        gesture.set_propagation_phase(gtk4::PropagationPhase::None);
                    } else {
                        gesture.set_propagation_phase(gtk4::PropagationPhase::Bubble);
                    }
                }
            }
        });

        factory.connect_unbind(|_, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("ListItem");
            // SAFETY: mirrors set_data("count-bindings") in connect_bind above.
            unsafe {
                if let Some(bindings) = list_item.steal_data::<BindingBag>("count-bindings") {
                    bindings.clear();
                }
            }
            if let Some(expander) = list_item
                .child()
                .and_then(|w| w.downcast::<gtk4::TreeExpander>().ok())
            {
                accessibility::clear_row_accessibility(&expander);
                accessibility::set_expanded(&expander, None);
                if let Some(content_box) = expander
                    .child()
                    .and_then(|w| w.downcast::<gtk4::Box>().ok())
                    && let Some(checkbox) = content_box
                        .first_child()
                        .and_then(|w| w.downcast::<gtk4::CheckButton>().ok())
                {
                    checkbox.set_visible(false);
                    accessibility::set_hidden(&checkbox, true);
                }
            }
        });

        self.results_list.set_factory(Some(&factory));

        let panel_weak = self.obj().downgrade();
        self.results_list.connect_activate(move |list_view, pos| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            let Some(model) = list_view.model() else {
                return;
            };
            let Some(item) = model.item(pos) else {
                return;
            };

            if let Some(row) = item.downcast_ref::<gtk4::TreeListRow>()
                && let Some(result_item) = row.item().and_downcast::<SearchResultItem>()
            {
                if result_item.is_match_item() {
                    let path = PathBuf::from(result_item.file_path());
                    let line = result_item.line_number();
                    if let Some(ref cb) = *panel.imp().callbacks.open_file_callback.borrow() {
                        cb(&path, line);
                    }
                } else {
                    row.set_expanded(!row.is_expanded());
                }
            }
        });
    }
}

pub(super) fn apply_result_row_accessibility(
    row_widget: &gtk4::TreeExpander,
    result_item: &SearchResultItem,
    expanded: Option<bool>,
) {
    if result_item.is_file_item() {
        let label = format!("Search results in {}", result_item.display_path());
        let matches = result_item.match_count();
        let description = match matches {
            0 => "No matches loaded yet".to_string(),
            1 => "1 match in this file".to_string(),
            _ => format!("{matches} matches in this file"),
        };
        accessibility::apply_row_accessibility(
            row_widget,
            RowAccessibility::new(&label).description(&description),
        );
        accessibility::set_expanded(row_widget, expanded);
        return;
    }

    let line_content = result_item.line_content();
    let bounded_line = accessibility::bounded_announcement_text(&line_content, 120);
    let label = format!("Line {} search match", result_item.line_number());
    let description = format!("{}: {}", result_item.file_path(), bounded_line);
    accessibility::apply_row_accessibility(
        row_widget,
        RowAccessibility::new(&label).description(&description),
    );
    accessibility::set_expanded(row_widget, None);
}

/// Build Pango markup highlighting the matched substring with bold.
/// Falls back to plain escaped text when the range is invalid.
fn render_match_markup(content: &str, start: usize, end: usize) -> String {
    let start = content.floor_char_boundary(start.min(content.len()));
    let end = content.ceil_char_boundary(end.min(content.len()));
    if start >= end {
        return glib::markup_escape_text(content).to_string();
    }
    format!(
        "{}<b>{}</b>{}",
        glib::markup_escape_text(&content[..start]),
        glib::markup_escape_text(&content[start..end]),
        glib::markup_escape_text(&content[end..]),
    )
}

/// Resolve the replacement preview index for the row that owns a stable checkbox slot.
fn preview_match_id_for_checkbox(
    panel: &super::LushtextSearchPanel,
    checkbox: &gtk4::CheckButton,
) -> Option<crate::model::content_search::SearchMatchId> {
    let expander = checkbox
        .parent()
        .and_then(|w| w.parent())
        .and_then(|w| w.downcast::<gtk4::TreeExpander>().ok())?;
    let row = expander.list_row()?;
    let result_item = row.item().and_downcast::<SearchResultItem>()?;
    if !result_item.is_match_item() {
        return None;
    }

    let imp = panel.imp();
    if !imp.preview.preview_mode.get() {
        return None;
    }

    let match_id = result_item.match_id();
    imp.preview
        .preview_outcome
        .borrow()
        .as_ref()
        .and_then(|outcome| outcome.preview_index(match_id))
        .map(|_| match_id)
}

/// Build markup for a preview row: original line dimmed/struck through and the
/// replacement line with the new text emphasized.
fn render_preview_markup(
    original: &str,
    replaced: &str,
    match_start: usize,
    match_end: usize,
) -> String {
    let start = original.floor_char_boundary(match_start.min(original.len()));
    let end = original.ceil_char_boundary(match_end.min(original.len()));

    let line1 = if start < end {
        format!(
            "{}<span strikethrough=\"true\" alpha=\"50%\">{}</span>{}",
            glib::markup_escape_text(&original[..start]),
            glib::markup_escape_text(&original[start..end]),
            glib::markup_escape_text(&original[end..]),
        )
    } else {
        glib::markup_escape_text(original).to_string()
    };

    #[expect(
        clippy::cast_possible_wrap,
        reason = "Rendered line fragments stay far below isize::MAX before they become GTK text buffer offsets"
    )]
    let new_len =
        replaced.len() as isize - original.len() as isize + (end as isize - start as isize);
    #[expect(
        clippy::cast_sign_loss,
        reason = "The highlight end offset is clamped to a non-negative value before converting to usize"
    )]
    let new_end = (start as isize + new_len).max(start as isize) as usize;
    let new_end = replaced.ceil_char_boundary(new_end.min(replaced.len()));
    let new_start = replaced.floor_char_boundary(start.min(replaced.len()));

    let line2 = if new_start < new_end {
        format!(
            "{}<b>{}</b>{}",
            glib::markup_escape_text(&replaced[..new_start]),
            glib::markup_escape_text(&replaced[new_start..new_end]),
            glib::markup_escape_text(&replaced[new_end..]),
        )
    } else {
        glib::markup_escape_text(replaced).to_string()
    };

    format!("{line1}\n{line2}")
}
