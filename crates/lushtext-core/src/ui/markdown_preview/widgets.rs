// SPDX-License-Identifier: GPL-3.0-or-later

//! **Called presentation surface** — not a role.
//!
//! The preview's widget half: the text view and its scroller geometry, the
//! readable-column margins, the placeholder and failure states, and the
//! embedded-widget insertion point. It owns no ordered stage and no coordination
//! state; the facade and the coordination roles call it to make something
//! visible.
//!
//! Follows the minimap precedent, where four widget accessors became a
//! `widgets.rs` called presentation surface rather than a role. Two contracts it
//! must keep, both from `.agents/rules/ui.md`:
//!
//! * **TextView child anchors** do not fill the text column by themselves, so
//!   anchored block widths are computed from the text view's allocation minus its
//!   margins and refreshed after render, on allocation, after readable-column
//!   changes, and on map.
//! * **The placeholder is a template child**, so `placeholder_description` reads
//!   it through `try_get()` and answers honestly on a disposed widget. That
//!   accessor is reachable from the workflow's evidence surface, so a panic here
//!   would be a panic at every observation point.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::services::markdown_render::MarkdownRenderState;
use crate::ui::accessibility;
use crate::ui::editor_page::{approximate_char_width, readable_column_margin};

use super::LushtextMarkdownPreview;
use super::seams::{EmbeddedBlockLayout, RenderedEmbed};

impl LushtextMarkdownPreview {
    /// Return the internal `GtkTextView` used for rendered preview content.
    ///
    /// Tests use this to inspect controllers, coordinates, and anchored child
    /// widgets through normal GTK APIs.
    #[must_use]
    pub fn text_view(&self) -> gtk4::TextView {
        self.imp().text_view.get()
    }

    /// Pin the preview scroller to one natural size for compact embedded uses.
    pub(crate) fn set_scroller_content_size(&self, width: i32, height: i32) {
        let scroller = self.imp().scrolled_window.get();
        scroller.set_min_content_width(width);
        scroller.set_max_content_width(width);
        scroller.set_min_content_height(height);
        scroller.set_max_content_height(height);
        scroller.set_propagate_natural_width(false);
        scroller.set_propagate_natural_height(false);
    }

    /// Apply or clear Focus Mode readable-column margins for rendered Markdown.
    pub(crate) fn set_focus_mode_readable_column(&self, active: bool, target_columns: u32) {
        let text_view = self.text_view();
        if active {
            let margin = readable_column_margin(
                text_view.width(),
                approximate_char_width(text_view.upcast_ref::<gtk4::Widget>()),
                target_columns,
            );
            text_view.set_left_margin(margin);
            text_view.set_right_margin(margin);
        } else {
            text_view.set_left_margin(16);
            text_view.set_right_margin(16);
        }
        self.queue_code_block_width_refresh();
    }

    #[must_use]
    pub fn content_margins(&self) -> (i32, i32) {
        let text_view = self.text_view();
        (text_view.left_margin(), text_view.right_margin())
    }

    /// Return the current document-surface opacity used by the preview.
    ///
    /// Widget tests use this to verify that preview mode tracks the same
    /// transparency preference as the editor surface.
    #[must_use]
    pub fn background_opacity(&self) -> f64 {
        gtk4::gio::Settings::new(crate::config::APP_ID)
            .double(crate::config::keys::TAB_CONTENT_OPACITY)
            .clamp(0.0, 1.0)
    }

    /// Clear the rendered content and show the placeholder for non-Markdown files.
    pub fn show_placeholder(&self, description: &str) {
        self.cancel_render_session();
        let imp = self.imp();
        imp.placeholder.set_description(Some(description));
        imp.scrolled_window.set_visible(false);
        imp.placeholder.set_visible(true);
        accessibility::set_description(&*imp.placeholder, description);
        accessibility::set_hidden(&*imp.scrolled_window, true);
        accessibility::set_hidden(&*imp.text_view, true);
        accessibility::set_hidden(&*imp.placeholder, false);
        self.clear_rendered_state(true);
        imp.showing_content.set(false);
    }

    /// Show placeholder copy inside the rendered text surface.
    ///
    /// Note editors use this while their Render page is hidden inside a
    /// `GtkStack`: the final scrolled text surface must be part of the first
    /// measurement pass, otherwise a later placeholder-to-content swap can make
    /// the surrounding dialog resize by a pixel when Render is clicked.
    pub(crate) fn show_content_placeholder(&self, description: &str) {
        self.cancel_render_session();
        self.show_content_view();
        self.clear_rendered_state(true);
        self.imp().text_view.buffer().set_text(description);
    }

    /// Clear the rendered content without showing the placeholder.
    pub fn clear(&self) {
        self.cancel_render_session();
        self.clear_rendered_state(true);
    }

    /// Render an accessible terminal when a caller cannot produce a plan.
    pub fn show_render_failure(&self, description: &str) {
        self.cancel_render_session();
        self.show_content_view();
        self.clear_rendered_state(true);
        self.imp().text_view.buffer().set_text(description);
        accessibility::set_description(&*self.imp().text_view, description);
        let mut session = self.imp().render_session.borrow_mut();
        let generation = session.generation();
        session.transition(generation, MarkdownRenderState::Failed);
    }

    /// Whether the widget is currently showing rendered Markdown content.
    #[must_use]
    pub fn is_showing_content(&self) -> bool {
        self.imp().showing_content.get()
    }

    /// The placeholder currently shown instead of content, if any.
    ///
    /// Reached through `try_get()` because `placeholder` is a template child GTK
    /// clears in `dispose()`. A disposed widget shows no placeholder, which is
    /// the honest answer; dereferencing the child here would turn a teardown
    /// observation into a panic, and this accessor is reachable from the
    /// workflow's evidence surface, so every observation point shares that risk.
    #[must_use]
    pub fn placeholder_description(&self) -> Option<String> {
        self.imp()
            .placeholder
            .try_get()
            .and_then(|placeholder| placeholder.description())
            .map(|description| description.to_string())
    }

    /// Get the rendered text content from the internal buffer.
    ///
    /// GTK child anchors are not plain text, so embedded table and image
    /// widgets do not appear in this string. Tests use it for surrounding text
    /// flow only.
    #[must_use]
    pub fn buffer_text(&self) -> String {
        let buffer = self.imp().text_view.buffer();
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .to_string()
    }

    /// Whether the text view is editable (should always be false).
    #[must_use]
    pub fn is_editable(&self) -> bool {
        self.imp().text_view.is_editable()
    }

    /// Whether the cursor is visible in the text view (should always be false).
    #[must_use]
    pub fn is_cursor_visible(&self) -> bool {
        self.imp().text_view.is_cursor_visible()
    }

    /// Look up a tag by name in the internal buffer's tag table.
    #[must_use]
    pub fn has_tag(&self, name: &str) -> bool {
        self.imp()
            .text_view
            .buffer()
            .tag_table()
            .lookup(name)
            .is_some()
    }

    /// Switch to content mode: text view visible, placeholder hidden.
    pub(super) fn show_content_view(&self) {
        let imp = self.imp();
        if !imp.showing_content.get() {
            imp.scrolled_window.set_visible(true);
            imp.placeholder.set_visible(false);
            accessibility::set_hidden(&*imp.scrolled_window, false);
            accessibility::set_hidden(&*imp.text_view, false);
            accessibility::set_hidden(&*imp.placeholder, true);
            imp.showing_content.set(true);
        }
    }

    /// Insert one already-built GTK widget into the preview text flow.
    pub(super) fn insert_embedded_widget(
        &self,
        buffer: &gtk4::TextBuffer,
        iter: &mut gtk4::TextIter,
        widget: &gtk4::Widget,
        layout: EmbeddedBlockLayout,
    ) {
        let anchor = buffer.create_child_anchor(iter);
        self.imp().text_view.add_child_at_anchor(widget, &anchor);
        self.imp()
            .rendered_embeds
            .borrow_mut()
            .push(RenderedEmbed::new(widget.clone(), layout));
        self.advance_rendered_embed_generation();
    }

    pub(super) fn advance_rendered_embed_generation(&self) {
        let imp = self.imp();
        imp.rendered_embed_generation
            .set(imp.rendered_embed_generation.get().wrapping_add(1));
    }
}

/// Build one compact fallback for Markdown structures that exceed preview budgets.
pub(super) fn build_preview_limit_fallback_widget(
    title: &str,
    body: &str,
    css_class: &str,
) -> gtk4::Widget {
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    container.set_margin_top(8);
    container.set_margin_bottom(8);
    container.set_margin_start(10);
    container.set_margin_end(10);
    container.set_halign(gtk4::Align::Start);
    container.set_width_request(280);
    container.add_css_class("card");
    container.add_css_class("markdown-preview-limit-fallback");
    container.add_css_class(css_class);
    accessibility::set_role(&container, gtk4::AccessibleRole::Group);
    accessibility::set_labelled_description(&container, title, body);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);

    let title_label = gtk4::Label::new(Some(title));
    title_label.set_xalign(0.0);
    title_label.set_wrap(true);
    title_label.add_css_class("heading");

    let body_label = gtk4::Label::new(Some(body));
    body_label.set_xalign(0.0);
    body_label.set_wrap(true);
    body_label.set_selectable(false);
    body_label.add_css_class("dim-label");

    content.append(&title_label);
    content.append(&body_label);
    container.append(&content);
    container.upcast()
}
