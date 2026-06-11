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
        factory.connect_setup(|_, list_item| {
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

            remove_preview_checkbox(&content_box);

            let file_label = content_box
                .first_child()
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
                        let file_path = result_item.file_path();
                        let line_number = result_item.line_number();
                        let original_match_start = result_item.original_match_start() as usize;
                        let replacements = imp.preview.preview_replacements.borrow();
                        let match_idx = replacements.iter().position(|r| {
                            r.path.display().to_string() == file_path
                                && r.line_number == u64::from(line_number)
                                && r.match_range.start == original_match_start
                        });

                        if let Some(idx) = match_idx {
                            let r = &replacements[idx];
                            let original = &r.original_line;
                            let replaced = &r.replaced_line;
                            let start = r.match_range.start.min(original.len());
                            let end = r.match_range.end.min(original.len());

                            let markup = render_preview_markup(original, replaced, start, end);
                            let is_checked = imp.preview.checked_indices.borrow().contains(&idx);
                            drop(replacements);

                            if let Some(ref label) = line_content_label {
                                label.set_markup(&markup);
                                label.set_visible(true);
                            }

                            let checkbox = gtk4::CheckButton::new();
                            checkbox.set_active(is_checked);
                            checkbox.add_css_class("preview-check");
                            content_box.prepend(&checkbox);

                            let panel_weak = panel.downgrade();
                            checkbox.connect_toggled(move |cb| {
                                let Some(panel) = panel_weak.upgrade() else {
                                    return;
                                };
                                let imp = panel.imp();
                                let mut indices = imp.preview.checked_indices.borrow_mut();
                                if cb.is_active() {
                                    indices.insert(idx);
                                } else {
                                    indices.remove(&idx);
                                }
                                let checked = indices.len();
                                let total = imp.preview.preview_replacements.borrow().len();
                                drop(indices);
                                imp.replace_all_button
                                    .set_label(&format!("Replace {checked} of {total}"));
                                imp.replace_all_button.set_sensitive(checked > 0);
                            });
                        } else {
                            drop(replacements);
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

/// Remove any dynamically added preview checkbox from a content box.
fn remove_preview_checkbox(content_box: &gtk4::Box) {
    if let Some(first) = content_box.first_child()
        && first.downcast_ref::<gtk4::CheckButton>().is_some()
    {
        content_box.remove(&first);
    }
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
