// SPDX-License-Identifier: GPL-3.0-or-later

//! `execution` role for the workspace tree workflow's **file peek** stage order:
//! `Space` on a selected row, bounded read, transient preview popover, dismissal.
//!
//! # Role
//!
//! Coordination, `execution`, qualified by the stage order it serves, nested under the
//! workflow's canonical role home in `ui/sidebar/`. Renamed from `peek.rs` for symmetry
//! with its siblings; the topic was already right, the role was simply unstated.
//!
//! The workspace section already owns selection, row recycling, and anchor
//! invalidation, so the temporary preview stays here as one local popover plus a small
//! async request state machine.
//!
//! # Inversion to be aware of
//!
//! The peek body is read off the GTK thread, so control resumes in the worker completion.
//! A completion whose request generation is stale, or whose row has been recycled or
//! deselected, must not present its popover.

use std::path::PathBuf;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gdk;
use gtk4::prelude::*;
use gtk4::{self, glib};

use crate::services::file_peek::{self, PeekPreviewState, PeekRequestToken, PeekSnapshot};
use crate::ui::accessibility;

use super::super::file_tree_item::FileTreeItem;
use super::LushtextWorkspaceSection;

/// Default width for the floating peek card.
const PEEK_CARD_WIDTH: i32 = 520;
/// Default height for the floating peek card.
const PEEK_CARD_HEIGHT: i32 = 420;
/// Horizontal gap between the sidebar edge and the floating peek card.
const PEEK_CARD_X_OFFSET: i32 = 15;

/// Minimal metadata fallback when a stat does not provide modified time.
const UNKNOWN_MODIFIED_LABEL: &str = "Modified time unavailable";

/// Selection-derived preview target used only inside the widget adapter.
struct PeekTarget {
    absolute_path: PathBuf,
    display_path: String,
}

impl LushtextWorkspaceSection {
    /// Build the popover widgets and connect the keyboard/visibility wiring.
    pub(super) fn setup_peek(&self) {
        self.setup_peek_popover();
        self.setup_peek_key_controller();
        self.setup_peek_visibility_watcher();
    }

    /// Rebind the current `SingleSelection` model to the peek workflow.
    pub(super) fn install_peek_selection_model(&self, selection: &gtk4::SingleSelection) {
        let section_weak = self.downgrade();
        selection.connect_selected_notify(move |_| {
            if let Some(section) = section_weak.upgrade() {
                section.refresh_peek_for_selection();
            }
        });
    }

    /// Dismiss any visible peek because the section's rows were rebuilt.
    pub(super) fn dismiss_peek_for_rebuild(&self) {
        self.dismiss_peek(false);
    }

    /// Return whether the floating peek card is currently visible.
    #[must_use]
    pub fn peek_visible(&self) -> bool {
        self.peek_popover()
            .as_ref()
            .is_some_and(gtk4::prelude::WidgetExt::is_visible)
    }

    /// Return the path currently shown in the peek card, if any.
    #[must_use]
    pub fn peeked_path(&self) -> Option<PathBuf> {
        self.imp().peek_session.active_path.borrow().clone()
    }

    /// Open or close peek for the current selection.
    #[must_use]
    pub fn toggle_peek_for_selection(&self) -> bool {
        let Some(target) = self.selected_peek_target() else {
            return false;
        };

        if self.peek_visible()
            && self
                .peeked_path()
                .as_deref()
                .is_some_and(|current| current == target.absolute_path.as_path())
        {
            self.dismiss_peek(true);
            return true;
        }

        self.start_peek_request(target);
        true
    }

    /// Promote the current preview into the normal open-document flow.
    #[must_use]
    pub fn promote_peeked_file(&self) -> bool {
        if !self.imp().peek_session.open_allowed.get() {
            return false;
        }

        let Some(path) = self.peeked_path() else {
            return false;
        };

        self.dismiss_peek(false);
        self.notify_peek_promoted(&path);
        true
    }

    /// Close the peek card and optionally restore list focus afterward.
    pub fn dismiss_peek(&self, restore_focus: bool) {
        self.clear_peek_state();
        self.invalidate_peek_requests();
        self.imp()
            .peek_session
            .restore_focus_on_close
            .set(restore_focus);

        if let Some(popover) = self.peek_popover() {
            popover.popdown();
        } else if restore_focus {
            self.restore_peek_focus();
        }
    }

    /// Refresh or dismiss the current peek based on the latest selection.
    fn refresh_peek_for_selection(&self) {
        if !self.peek_visible() {
            return;
        }

        let Some(target) = self.selected_peek_target() else {
            self.dismiss_peek(true);
            return;
        };

        let current = self.peeked_path();
        if current.as_deref() == Some(target.absolute_path.as_path()) {
            self.reanchor_peek_to_selection();
            return;
        }

        self.start_peek_request(target);
    }

    /// Create the popover widgets once and keep weak references in the imp state.
    fn setup_peek_popover(&self) {
        let popover = gtk4::Popover::new();
        popover.set_parent(&*self.imp().file_tree_view);
        popover.set_position(gtk4::PositionType::Right);
        popover.set_has_arrow(false);
        popover.set_autohide(true);
        popover.set_offset(PEEK_CARD_X_OFFSET, 0);
        popover.set_width_request(PEEK_CARD_WIDTH);
        popover.set_height_request(PEEK_CARD_HEIGHT);
        popover.add_css_class("card");
        accessibility::set_labelled_description(
            &popover,
            "File peek",
            "Read-only preview for the selected file",
        );

        let card = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        card.set_margin_top(12);
        card.set_margin_bottom(12);
        card.set_margin_start(12);
        card.set_margin_end(12);

        let title_label = gtk4::Label::new(None);
        title_label.set_xalign(0.0);
        title_label.set_wrap(true);
        title_label.add_css_class("heading");
        accessibility::set_labelled_description(
            &title_label,
            "Peek file name",
            "Name of the file being previewed",
        );

        let path_label = gtk4::Label::new(None);
        path_label.set_xalign(0.0);
        path_label.set_wrap(true);
        path_label.add_css_class("dim-label");
        path_label.add_css_class("monospace");
        accessibility::set_labelled_description(
            &path_label,
            "Peek file path",
            "Full path of the file being previewed",
        );

        let meta_label = gtk4::Label::new(None);
        meta_label.set_xalign(0.0);
        meta_label.set_wrap(true);
        meta_label.add_css_class("dim-label");
        accessibility::set_labelled_description(
            &meta_label,
            "Peek file metadata",
            "Size and modification time for the file being previewed",
        );

        let header = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        header.append(&title_label);
        header.append(&path_label);
        header.append(&meta_label);

        let body_stack = gtk4::Stack::new();
        body_stack.set_vexpand(true);
        accessibility::set_labelled_description(
            &body_stack,
            "Peek preview body",
            "Preview content or an explanation when preview is unavailable",
        );

        let loading_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        loading_box.set_valign(gtk4::Align::Center);
        let spinner = gtk4::Spinner::new();
        spinner.start();
        let loading_label = gtk4::Label::new(Some("Loading preview…"));
        loading_label.add_css_class("dim-label");
        loading_box.append(&spinner);
        loading_box.append(&loading_label);
        body_stack.add_named(&loading_box, Some("loading"));

        let text_buffer = gtk4::TextBuffer::new(None);
        let text_view = gtk4::TextView::with_buffer(&text_buffer);
        text_view.set_editable(false);
        text_view.set_cursor_visible(false);
        text_view.set_monospace(true);
        text_view.set_wrap_mode(gtk4::WrapMode::WordChar);
        text_view.add_css_class("monospace");
        accessibility::set_labelled_description(
            &text_view,
            "Peek text preview",
            "Read-only text sample for the selected file",
        );
        accessibility::set_read_only(&text_view, true);
        accessibility::set_multi_line(&text_view, true);

        let text_scroller = gtk4::ScrolledWindow::new();
        text_scroller.set_child(Some(&text_view));
        text_scroller.set_vexpand(true);
        text_scroller.set_hexpand(true);
        body_stack.add_named(&text_scroller, Some("text"));

        let fallback_box = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        fallback_box.set_valign(gtk4::Align::Center);
        let fallback_title_label = gtk4::Label::new(None);
        fallback_title_label.set_xalign(0.0);
        fallback_title_label.set_wrap(true);
        fallback_title_label.add_css_class("heading");
        accessibility::set_labelled_description(
            &fallback_title_label,
            "Peek fallback",
            "Reason an inline preview is unavailable",
        );
        let fallback_body_label = gtk4::Label::new(None);
        fallback_body_label.set_xalign(0.0);
        fallback_body_label.set_wrap(true);
        accessibility::set_labelled_description(
            &fallback_body_label,
            "Peek fallback detail",
            "Explanation of the current preview limitation",
        );
        fallback_box.append(&fallback_title_label);
        fallback_box.append(&fallback_body_label);
        body_stack.add_named(&fallback_box, Some("fallback"));

        let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        footer.set_halign(gtk4::Align::End);
        let open_button = gtk4::Button::with_label("Open");
        open_button.add_css_class("suggested-action");
        accessibility::set_labelled_description(
            &open_button,
            "Open previewed file",
            "Open the previewed file in an editor tab",
        );
        accessibility::set_value_text(&open_button, "Open unavailable");
        accessibility::set_disabled(&open_button, true);
        footer.append(&open_button);

        card.append(&header);
        card.append(&body_stack);
        card.append(&footer);
        popover.set_child(Some(&card));

        let section_weak = self.downgrade();
        open_button.connect_clicked(move |_| {
            if let Some(section) = section_weak.upgrade() {
                let _ = section.promote_peeked_file();
            }
        });

        let section_weak = self.downgrade();
        popover.connect_closed(move |_| {
            let Some(section) = section_weak.upgrade() else {
                return;
            };
            if section.peeked_path().is_some() {
                section.clear_peek_state();
                section.invalidate_peek_requests();
            }
            if section.imp().peek_session.restore_focus_on_close.get() {
                section.restore_peek_focus();
            }
            section.imp().peek_session.restore_focus_on_close.set(false);
        });

        *self.imp().peek_widgets.popover.borrow_mut() = Some(popover);
        *self.imp().peek_widgets.title_label.borrow_mut() = Some(title_label);
        *self.imp().peek_widgets.path_label.borrow_mut() = Some(path_label);
        *self.imp().peek_widgets.meta_label.borrow_mut() = Some(meta_label);
        *self.imp().peek_widgets.body_stack.borrow_mut() = Some(body_stack);
        *self.imp().peek_widgets.text_buffer.borrow_mut() = Some(text_buffer);
        *self.imp().peek_widgets.text_view.borrow_mut() = Some(text_view);
        *self.imp().peek_widgets.fallback_title_label.borrow_mut() = Some(fallback_title_label);
        *self.imp().peek_widgets.fallback_body_label.borrow_mut() = Some(fallback_body_label);
        *self.imp().peek_widgets.open_button.borrow_mut() = Some(open_button);
    }

    /// Keep peek keyboard interactions local to the sidebar list.
    fn setup_peek_key_controller(&self) {
        let controller = gtk4::EventControllerKey::new();
        // Real keyboard focus lands on realized row widgets inside GtkListView,
        // not on the ListView wrapper itself. Capture-phase delivery lets the
        // section observe Space/Escape/Enter before row-local widgets consume
        // them, while still preserving default behavior for inline rename
        // entries and other focused controls that should own their keys.
        controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let section_weak = self.downgrade();
        controller.connect_key_pressed(move |_, key, _, _| {
            let Some(section) = section_weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if !section.focus_allows_peek_shortcuts() {
                return glib::Propagation::Proceed;
            }

            match key {
                gdk::Key::space => {
                    if section.toggle_peek_for_selection() {
                        glib::Propagation::Stop
                    } else {
                        glib::Propagation::Proceed
                    }
                }
                gdk::Key::Escape if section.peek_visible() => {
                    section.dismiss_peek(true);
                    glib::Propagation::Stop
                }
                gdk::Key::Return | gdk::Key::KP_Enter if section.peek_visible() => {
                    let _ = section.promote_peeked_file();
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        self.imp().file_tree_view.add_controller(controller);
    }

    /// Hide peek whenever the whole section is hidden by the workspace filter.
    fn setup_peek_visibility_watcher(&self) {
        let section_weak = self.downgrade();
        self.connect_visible_notify(move |section| {
            if !section.is_visible()
                && let Some(section) = section_weak.upgrade()
                && section.peek_visible()
            {
                section.dismiss_peek(false);
            }
        });
    }

    /// Start an async snapshot load for the current selection.
    fn start_peek_request(&self, target: PeekTarget) {
        let loading = PeekSnapshot::loading(&target.absolute_path, target.display_path.clone());
        let token = self.invalidate_peek_requests();
        self.imp()
            .peek_session
            .active_path
            .replace(Some(target.absolute_path.clone()));
        self.render_peek_snapshot(&loading);
        self.reanchor_peek_to_selection();

        let path = target.absolute_path.clone();
        let display_path = target.display_path;
        gtk_lush_tasks::spawn_blocking_then(
            self.clone(),
            move || file_peek::load_snapshot(&path, display_path),
            move |section, snapshot| {
                if !token.matches(section.imp().peek_session.active_generation.get()) {
                    return;
                }
                if section.peeked_path().as_deref() != Some(snapshot.absolute_path.as_path()) {
                    return;
                }
                section.render_peek_snapshot(&snapshot);
                section.reanchor_peek_to_selection();
            },
        );
    }

    /// Update the popover UI to match the given snapshot.
    fn render_peek_snapshot(&self, snapshot: &PeekSnapshot) {
        if let Some(label) = self.peek_title_label() {
            label.set_label(&snapshot.display_name);
        }
        if let Some(label) = self.peek_path_label() {
            label.set_label(&snapshot.display_path);
        }
        if let Some(label) = self.peek_meta_label() {
            label.set_label(&format_metadata(snapshot));
        }
        if let Some(button) = self.peek_open_button() {
            button.set_sensitive(snapshot.open_allowed);
            accessibility::set_disabled(&button, !snapshot.open_allowed);
            accessibility::set_value_text(
                &button,
                if snapshot.open_allowed {
                    "Open available"
                } else {
                    "Open unavailable"
                },
            );
        }
        self.imp()
            .peek_session
            .open_allowed
            .set(snapshot.open_allowed);

        let loading = matches!(snapshot.preview_state, PeekPreviewState::Loading);
        let invalid = matches!(snapshot.preview_state, PeekPreviewState::Unreadable);
        if let Some(popover) = self.peek_popover() {
            accessibility::set_description(
                &popover,
                &format!("Read-only preview for {}", snapshot.display_path),
            );
            accessibility::set_busy(&popover, loading);
            accessibility::set_invalid(&popover, invalid);
        }
        if let Some(stack) = self.peek_body_stack() {
            accessibility::set_busy(&stack, loading);
            accessibility::set_invalid(&stack, invalid);
        }

        match snapshot.preview_state {
            PeekPreviewState::Loading => {
                if let Some(stack) = self.peek_body_stack() {
                    stack.set_visible_child_name("loading");
                }
            }
            PeekPreviewState::Text => {
                if let Some(buffer) = self.peek_text_buffer() {
                    let body = snapshot.sample_text.as_deref().unwrap_or_default();
                    buffer.set_text(body);
                }
                if let Some(stack) = self.peek_body_stack() {
                    stack.set_visible_child_name("text");
                }
            }
            PeekPreviewState::BinaryOrUnsupported => {
                self.render_fallback_state(
                    "Inline preview unavailable",
                    "This file does not look like UTF-8 text, so LushText keeps peek read-only and does not offer a normal open action.",
                );
            }
            PeekPreviewState::Unreadable => {
                self.render_fallback_state(
                    "Could not read this file",
                    "LushText could not read the selected path. It may have moved, been removed, or no longer be readable.",
                );
            }
            PeekPreviewState::TooLargeToOpen => {
                self.render_fallback_state(
                    "Too large to open",
                    "This file exceeds LushText's existing open-size limit, so peek does not try to load it inline and the normal open action stays disabled.",
                );
            }
        }

        if let Some(popover) = self.peek_popover() {
            popover.popup();
        }
    }

    /// Render the fallback body inside the shared popover.
    fn render_fallback_state(&self, title: &str, body: &str) {
        if let Some(label) = self.peek_fallback_title_label() {
            label.set_label(title);
        }
        if let Some(label) = self.peek_fallback_body_label() {
            label.set_label(body);
        }
        if let Some(stack) = self.peek_body_stack() {
            stack.set_visible_child_name("fallback");
        }
        if let Some(buffer) = self.peek_text_buffer() {
            buffer.set_text("");
        }
    }

    /// Re-anchor the popover beside the currently selected row, dismissing it
    /// if the row is no longer realized.
    fn reanchor_peek_to_selection(&self) {
        let Some(popover) = self.peek_popover() else {
            return;
        };
        let Some(rect) = self.selected_row_bounds() else {
            self.dismiss_peek(false);
            return;
        };
        popover.set_pointing_to(Some(&rect));
    }

    /// Return the current selection as a previewable file target.
    fn selected_peek_target(&self) -> Option<PeekTarget> {
        let row = self.selected_tree_row()?;
        let item = row.item()?.downcast::<FileTreeItem>().ok()?;
        if item.is_dir() || item.is_placeholder() {
            return None;
        }
        let absolute_path = item.path()?;
        Some(PeekTarget {
            display_path: absolute_path.display().to_string(),
            absolute_path,
        })
    }

    /// Return the selected flattened tree row from the current `SingleSelection`.
    fn selected_tree_row(&self) -> Option<gtk4::TreeListRow> {
        let selection = self
            .imp()
            .file_tree_view
            .model()
            .and_downcast::<gtk4::SingleSelection>()?;
        selection
            .selected_item()?
            .downcast::<gtk4::TreeListRow>()
            .ok()
    }

    /// Return the selected row's bounds relative to the `GtkListView`.
    fn selected_row_bounds(&self) -> Option<gdk::Rectangle> {
        let target_row = self.selected_tree_row()?;
        let list_view = self.imp().file_tree_view.clone();
        let mut child = list_view.first_child();
        while let Some(row_widget) = child {
            let next = row_widget.next_sibling();
            if let Some(overlay) = row_widget.first_child().and_downcast::<gtk4::Overlay>()
                && let Some(expander) = overlay.child().and_downcast::<gtk4::TreeExpander>()
                && expander.list_row().as_ref() == Some(&target_row)
                && let Some(bounds) = row_widget.compute_bounds(&list_view)
            {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "Popover anchor geometry comes from GTK allocation data that already lives in i32 widget coordinates"
                )]
                return Some(gdk::Rectangle::new(
                    bounds.x().round() as i32,
                    bounds.y().round() as i32,
                    bounds.width().max(1.0).round() as i32,
                    bounds.height().max(1.0).round() as i32,
                ));
            }
            child = next;
        }
        None
    }

    /// Advance the request token and return the new active generation.
    fn invalidate_peek_requests(&self) -> PeekRequestToken {
        let next = self.imp().peek_session.active_generation.get().next();
        self.imp().peek_session.active_generation.set(next);
        next
    }

    /// Clear the transient popover state without changing focus yet.
    fn clear_peek_state(&self) {
        self.imp().peek_session.active_path.borrow_mut().take();
        self.imp().peek_session.open_allowed.set(false);
        if let Some(button) = self.peek_open_button() {
            button.set_sensitive(false);
            accessibility::set_disabled(&button, true);
            accessibility::set_value_text(&button, "Open unavailable");
        }
        if let Some(buffer) = self.peek_text_buffer() {
            buffer.set_text("");
        }
        if let Some(popover) = self.peek_popover() {
            accessibility::set_busy(&popover, false);
            accessibility::set_invalid(&popover, false);
        }
        if let Some(stack) = self.peek_body_stack() {
            accessibility::set_busy(&stack, false);
            accessibility::set_invalid(&stack, false);
        }
    }

    /// Restore focus to the list view so keyboard scanning can continue.
    fn restore_peek_focus(&self) {
        self.imp().file_tree_view.grab_focus();
    }

    /// Return whether the currently focused widget should participate in the
    /// sidebar peek shortcut flow.
    fn focus_allows_peek_shortcuts(&self) -> bool {
        let Some(root) = self.root() else {
            return true;
        };
        let Some(window) = root.downcast_ref::<gtk4::Window>() else {
            return true;
        };
        let Some(focus) = gtk4::prelude::GtkWindowExt::focus(window) else {
            return true;
        };

        if focus.is::<gtk4::Entry>() || focus.is::<gtk4::Button>() {
            return false;
        }

        widget_is_within(
            &focus,
            self.imp().file_tree_view.upcast_ref::<gtk4::Widget>(),
        )
    }

    fn peek_popover(&self) -> Option<gtk4::Popover> {
        self.imp().peek_widgets.popover.borrow().clone()
    }

    fn peek_title_label(&self) -> Option<gtk4::Label> {
        self.imp().peek_widgets.title_label.borrow().clone()
    }

    fn peek_path_label(&self) -> Option<gtk4::Label> {
        self.imp().peek_widgets.path_label.borrow().clone()
    }

    fn peek_meta_label(&self) -> Option<gtk4::Label> {
        self.imp().peek_widgets.meta_label.borrow().clone()
    }

    fn peek_body_stack(&self) -> Option<gtk4::Stack> {
        self.imp().peek_widgets.body_stack.borrow().clone()
    }

    fn peek_text_buffer(&self) -> Option<gtk4::TextBuffer> {
        self.imp().peek_widgets.text_buffer.borrow().clone()
    }

    fn peek_fallback_title_label(&self) -> Option<gtk4::Label> {
        self.imp()
            .peek_widgets
            .fallback_title_label
            .borrow()
            .clone()
    }

    fn peek_fallback_body_label(&self) -> Option<gtk4::Label> {
        self.imp().peek_widgets.fallback_body_label.borrow().clone()
    }

    fn peek_open_button(&self) -> Option<gtk4::Button> {
        self.imp().peek_widgets.open_button.borrow().clone()
    }
}

/// Format the metadata line shared by text and fallback states.
fn format_metadata(snapshot: &PeekSnapshot) -> String {
    let size = format_file_size(snapshot.byte_size);
    let modified = snapshot
        .modified_at_secs
        .and_then(format_modified_time)
        .unwrap_or_else(|| UNKNOWN_MODIFIED_LABEL.to_string());

    match snapshot.preview_state {
        PeekPreviewState::Text if snapshot.truncated => {
            format!(
                "{size} • {modified} • Showing first {} lines",
                snapshot.sample_line_count
            )
        }
        _ => format!("{size} • {modified}"),
    }
}

/// Format bytes using the same SI-style units as the status bar.
fn format_file_size(bytes: u64) -> String {
    const KB: f64 = 1_000.0;
    const MB: f64 = 1_000_000.0;
    let bytes_f = bytes as f64;
    if bytes_f >= MB {
        format!("{:.1} MB", bytes_f / MB)
    } else if bytes_f >= KB {
        let kb = bytes_f / KB;
        if kb >= 999.95 {
            format!("{:.1} MB", bytes_f / MB)
        } else {
            format!("{kb:.1} KB")
        }
    } else {
        format!("{bytes} B")
    }
}

/// Render a stable local timestamp for the preview metadata line.
fn format_modified_time(modified_at_secs: u64) -> Option<String> {
    glib::DateTime::from_unix_local(modified_at_secs as i64)
        .ok()
        .map(|datetime| {
            datetime.format("%Y-%m-%d %H:%M").map_or_else(
                |_| UNKNOWN_MODIFIED_LABEL.to_string(),
                |formatted| formatted.to_string(),
            )
        })
}

/// Return whether `widget` is `ancestor` itself or one of its descendants.
fn widget_is_within(widget: &gtk4::Widget, ancestor: &gtk4::Widget) -> bool {
    let mut current = Some(widget.clone());
    while let Some(candidate) = current {
        if candidate.as_ptr() == ancestor.as_ptr() {
            return true;
        }
        current = candidate.parent();
    }
    false
}
