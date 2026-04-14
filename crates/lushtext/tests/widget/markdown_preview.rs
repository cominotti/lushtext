// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextMarkdownPreview widget.

use crate::common::{ensure_gtk_init, present_window, test_application, wait_until};
use glib::prelude::{Cast, IsA};
use gtk4::prelude::{GtkWindowExt, WidgetExt};
use lushtext_core::ui::markdown_preview::LushtextMarkdownPreview;
use std::time::Duration;

#[test]
fn test_new() {
    ensure_gtk_init();
    let _preview = LushtextMarkdownPreview::new();
}

#[test]
fn test_starts_not_showing_content() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    assert!(!preview.is_showing_content());
}

#[test]
fn test_render_markdown_switches_to_content_mode() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("# Hello");
    assert!(preview.is_showing_content());
}

#[test]
fn test_show_placeholder_switches_to_placeholder_mode() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("# Hello");
    assert!(preview.is_showing_content());
    preview.show_placeholder("Not a Markdown file");
    assert!(!preview.is_showing_content());
}

#[test]
fn test_render_heading_inserts_text() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("# Hello World");
    assert!(
        preview.buffer_text().contains("Hello World"),
        "Expected heading text in buffer"
    );
}

#[test]
fn test_render_bold_inserts_text() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("**bold text**");
    assert!(
        preview.buffer_text().contains("bold text"),
        "Expected bold text in buffer"
    );
}

#[test]
fn test_render_italic_inserts_text() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("*italic text*");
    assert!(
        preview.buffer_text().contains("italic text"),
        "Expected italic text in buffer"
    );
}

#[test]
fn test_render_code_block_inserts_text() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("```\nlet x = 42;\n```");
    assert!(
        preview.buffer_text().contains("let x = 42;"),
        "Expected code block text in buffer"
    );
}

#[test]
fn test_render_inline_code_inserts_text() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("Use `cargo build` to compile.");
    assert!(
        preview.buffer_text().contains("cargo build"),
        "Expected inline code in buffer"
    );
}

#[test]
fn test_render_link_inserts_text() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("[click here](https://example.com)");
    assert!(
        preview.buffer_text().contains("click here"),
        "Expected link text in buffer"
    );
}

#[test]
fn test_render_unordered_list_inserts_bullets() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("- item one\n- item two");
    let text = preview.buffer_text();
    assert!(text.contains("item one"), "Expected list item text");
    assert!(text.contains('\u{2022}'), "Expected bullet character");
}

#[test]
fn test_render_ordered_list_inserts_numbers() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("1. first\n2. second");
    let text = preview.buffer_text();
    assert!(text.contains("1."), "Expected ordered list number");
    assert!(text.contains("first"), "Expected list item text");
}

#[test]
fn test_render_horizontal_rule() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("above\n\n---\n\nbelow");
    assert!(
        preview.buffer_text().contains('\u{2500}'),
        "Expected horizontal rule character"
    );
}

#[test]
fn test_render_blockquote_inserts_text() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("> quoted text");
    assert!(
        preview.buffer_text().contains("quoted text"),
        "Expected blockquote text in buffer"
    );
}

#[test]
fn test_heading_tag_exists_after_render() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("# Title");
    assert!(
        preview.has_tag("heading1"),
        "Expected heading1 tag in tag table"
    );
}

#[test]
fn test_clear_removes_content() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("# Hello");
    preview.clear();
    assert!(
        preview.buffer_text().is_empty(),
        "Expected empty buffer after clear"
    );
}

#[test]
fn test_re_render_replaces_previous_content() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("# First");
    preview.render_markdown("# Second");
    let text = preview.buffer_text();
    assert!(
        !text.contains("First"),
        "Previous content should be replaced"
    );
    assert!(text.contains("Second"), "New content should be present");
}

#[test]
fn test_text_view_is_not_editable() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    assert!(
        !preview.is_editable(),
        "Preview text view must be read-only"
    );
}

#[test]
fn test_text_view_cursor_not_visible() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    assert!(
        !preview.is_cursor_visible(),
        "Cursor should be hidden in preview"
    );
}

#[test]
fn test_render_table_adds_anchored_grid_and_preserves_surrounding_text() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview(&preview);

    preview.render_markdown("above\n\n| Name | Value |\n| --- | --- |\n| one | two |\n\nbelow");
    wait_until(Duration::from_secs(2), || {
        widgets_with_css_class::<gtk4::Grid>(&preview, "markdown-table").len() == 1
    });

    let text = preview.buffer_text();
    assert!(text.contains("above"), "Expected text before the table");
    assert!(text.contains("below"), "Expected text after the table");
    assert_eq!(
        widgets_with_css_class::<gtk4::Grid>(&preview, "markdown-table").len(),
        1,
        "Expected one anchored table grid"
    );
}

#[test]
fn test_render_table_exposes_header_and_body_cells() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview(&preview);

    preview.render_markdown("| Name | Value |\n| --- | --- |\n| one | two |");
    wait_until(Duration::from_secs(2), || {
        widgets_with_css_class::<gtk4::Label>(&preview, "markdown-table-header-cell").len() == 2
            && widgets_with_css_class::<gtk4::Label>(&preview, "markdown-table-cell").len() >= 4
    });

    let header_cells =
        widgets_with_css_class::<gtk4::Label>(&preview, "markdown-table-header-cell");
    let body_cells = widgets_with_css_class::<gtk4::Label>(&preview, "markdown-table-cell")
        .into_iter()
        .filter(|label| !label.has_css_class("markdown-table-header-cell"))
        .collect::<Vec<_>>();

    assert_eq!(header_cells.len(), 2, "Expected two header cells");
    assert_eq!(body_cells.len(), 2, "Expected two body cells");
    assert!(
        descendants(&preview)
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk4::Separator>().ok())
            .any(|separator| separator.has_css_class("markdown-table-header-separator")),
        "Expected a separator between header and body rows"
    );
}

#[test]
fn test_render_table_maps_alignment_to_label_xalign() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview(&preview);

    preview.render_markdown(
        "| Left | Center | Right |\n| :--- | :----: | ----: |\n| left-body | center-body | right-body |",
    );
    wait_until(Duration::from_secs(2), || {
        find_label_with_text(&preview, "left-body").is_some()
            && find_label_with_text(&preview, "center-body").is_some()
            && find_label_with_text(&preview, "right-body").is_some()
    });

    assert_eq!(
        find_label_with_text(&preview, "left-body")
            .expect("left label")
            .xalign(),
        0.0
    );
    assert_eq!(
        find_label_with_text(&preview, "center-body")
            .expect("center label")
            .xalign(),
        0.5
    );
    assert_eq!(
        find_label_with_text(&preview, "right-body")
            .expect("right label")
            .xalign(),
        1.0
    );
    assert!(
        !find_label_with_text(&preview, "left-body")
            .expect("left label")
            .wraps(),
        "Expected table cells to keep their natural width instead of auto-wrapping"
    );
}

#[test]
fn test_render_table_cell_markup_subset_uses_label_markup() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview(&preview);

    preview.render_markdown(
        "| Bold | Italic | Strike | Code |\n| --- | --- | --- | --- |\n| **bold** | *italic* | ~~strike~~ | `code` |",
    );
    wait_until(Duration::from_secs(2), || {
        find_label_with_text(&preview, "bold").is_some()
            && find_label_with_text(&preview, "italic").is_some()
            && find_label_with_text(&preview, "strike").is_some()
            && find_label_with_text(&preview, "code").is_some()
    });

    let bold = find_label_with_text(&preview, "bold").expect("bold label");
    let italic = find_label_with_text(&preview, "italic").expect("italic label");
    let strike = find_label_with_text(&preview, "strike").expect("strike label");
    let code = find_label_with_text(&preview, "code").expect("code label");

    assert!(bold.uses_markup(), "Expected table cells to use label markup");
    assert_eq!(bold.label(), "<b>bold</b>");
    assert_eq!(italic.label(), "<i>italic</i>");
    assert_eq!(strike.label(), "<s>strike</s>");
    assert_eq!(code.label(), "<tt>code</tt>");
}

#[test]
fn test_render_markdown_cleans_up_table_widgets_on_rerender() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview(&preview);

    preview.render_markdown("| Name |\n| --- |\n| one |");
    wait_until(Duration::from_secs(2), || {
        widgets_with_css_class::<gtk4::Grid>(&preview, "markdown-table").len() == 1
    });

    preview.render_markdown("# No Table");
    wait_until(Duration::from_secs(2), || {
        widgets_with_css_class::<gtk4::Grid>(&preview, "markdown-table").is_empty()
    });

    assert!(
        widgets_with_css_class::<gtk4::Grid>(&preview, "markdown-table").is_empty(),
        "Expected rerender without tables to remove the old anchored grid"
    );
}

fn present_preview(preview: &LushtextMarkdownPreview) -> gtk4::ApplicationWindow {
    let window = gtk4::ApplicationWindow::new(&test_application());
    window.set_child(Some(preview));
    present_window(&window);
    window
}

fn descendants(root: &impl IsA<gtk4::Widget>) -> Vec<gtk4::Widget> {
    let mut widgets = Vec::new();
    let mut stack = vec![root.as_ref().clone()];

    while let Some(widget) = stack.pop() {
        let mut child = widget.first_child();
        while let Some(current) = child {
            stack.push(current.clone());
            child = current.next_sibling();
        }
        widgets.push(widget);
    }

    widgets
}

fn widgets_with_css_class<T>(root: &impl IsA<gtk4::Widget>, css_class: &str) -> Vec<T>
where
    T: IsA<gtk4::Widget> + glib::object::ObjectType + Clone + 'static,
{
    descendants(root)
        .into_iter()
        .filter(|widget| widget.has_css_class(css_class))
        .filter_map(|widget| widget.downcast::<T>().ok())
        .collect()
}

fn find_label_with_text(root: &impl IsA<gtk4::Widget>, text: &str) -> Option<gtk4::Label> {
    descendants(root)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk4::Label>().ok())
        .find(|label| label.text() == text || label.label().contains(text))
}
