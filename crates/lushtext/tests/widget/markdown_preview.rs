// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextMarkdownPreview widget.

use crate::common::{ensure_gtk_init, present_window, test_application, wait_until};
use gio::prelude::ListModelExt;
use glib::prelude::{Cast, IsA};
use gtk4::prelude::*;
use lushtext_core::ui::markdown_preview::{
    LushtextMarkdownPreview, MarkdownPreviewRenderContext,
};
use std::cell::Cell;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
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
fn test_clickable_preview_link_activates_external_target() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview(&preview);
    let launched = Rc::new(RefCell::new(Vec::<String>::new()));
    let launched_clone = launched.clone();
    preview.connect_link_activated(move |uri| launched_clone.borrow_mut().push(uri.to_string()));

    preview.render_markdown("[click here](https://example.com)");
    emit_preview_click_for_text(&preview, "click here");

    assert_eq!(
        launched.borrow().as_slice(),
        ["https://example.com"],
        "Expected preview click to activate the rendered link target"
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
fn test_render_nested_list_uses_deeper_margin_tag() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("- parent\n  - child");

    let child_tags = tags_for_rendered_text(&preview, "child");
    assert!(
        child_tags.iter().any(|name| name == "list-item-depth-2"),
        "Expected nested list items to carry a deeper indentation tag"
    );
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
fn test_render_task_list_uses_checkbox_markers() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("- [x] done\n- [ ] todo");
    let text = preview.buffer_text();
    assert!(text.contains("done"), "Expected checked task text");
    assert!(text.contains("todo"), "Expected unchecked task text");
    assert!(
        text.contains('\u{2611}'),
        "Expected checked task list marker in preview text"
    );
    assert!(
        text.contains('\u{2610}'),
        "Expected unchecked task list marker in preview text"
    );
    assert!(
        !text.contains("[x]") && !text.contains("[ ]"),
        "Expected task list preview to render task markers instead of raw source syntax"
    );
}

#[test]
fn test_render_gfm_callout_inserts_title_without_raw_marker() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("> [!NOTE]\n> Pay attention to `this`.");
    let text = preview.buffer_text();
    assert!(text.contains("Note"), "Expected callout title in preview text");
    assert!(text.contains("Pay attention"), "Expected callout body text");
    assert!(text.contains("this"), "Expected inline code text inside callout");
    assert!(
        !text.contains("[!NOTE]"),
        "Expected callout preview to hide the raw alert marker"
    );
}

#[test]
fn test_render_footnote_reference_and_definition() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("hello[^1]\n\n[^1]: note");
    let text = preview.buffer_text();
    assert!(
        text.contains("hello[1]"),
        "Expected inline footnote reference marker in preview text"
    );
    assert!(
        text.contains("[1] note"),
        "Expected rendered footnote definition in preview text"
    );
    assert!(
        !text.contains("[^1]"),
        "Expected raw footnote syntax to be replaced in preview text"
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
fn test_render_table_cell_links_use_markup_and_activation() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview(&preview);
    let launched = Rc::new(RefCell::new(Vec::<String>::new()));
    let launched_clone = launched.clone();
    preview.connect_link_activated(move |uri| launched_clone.borrow_mut().push(uri.to_string()));

    preview.render_markdown("| Docs |\n| --- |\n| [Open Guide](https://example.com/guide) |");
    wait_until(Duration::from_secs(2), || {
        find_label_with_text(&preview, "Open Guide").is_some()
    });

    let label = find_label_with_text(&preview, "Open Guide").expect("table link label");
    assert!(label.uses_markup(), "Expected table links to use label markup");
    assert!(
        label
            .label()
            .contains("<a href=\"https://example.com/guide\">Open Guide</a>"),
        "Expected table cell markup to preserve a launchable link"
    );
    let handled: bool = label.emit_by_name("activate-link", &[&"https://example.com/guide"]);
    assert!(handled, "Expected table label activation to stop further handling");
    assert_eq!(
        launched.borrow().as_slice(),
        ["https://example.com/guide"],
        "Expected table-cell link activation to use the shared preview launcher"
    );
}

#[test]
fn test_render_markdown_renders_local_image_block() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview(&preview);
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root");
    let context = MarkdownPreviewRenderContext::new(
        Some(repo_root.join("samples/markdown-test.md")),
        Vec::new(),
    );
    let inserted_paintable = Rc::new(Cell::new(false));
    let inserted_paintable_clone = inserted_paintable.clone();
    preview
        .text_view()
        .buffer()
        .connect_insert_paintable(move |_, _, _| {
            inserted_paintable_clone.set(true);
        });

    preview.render_markdown_with_context(
        "![File-relative preview card sample](assets/preview-secondary.svg)",
        &context,
    );
    wait_until(Duration::from_secs(2), || {
        inserted_paintable.get()
    });

    assert!(
        inserted_paintable.get(),
        "Expected the preview buffer to insert a paintable for a resolved local image"
    );
    assert!(
        widgets_with_css_class::<gtk4::Box>(&preview, "markdown-preview-image-fallback")
            .is_empty(),
        "Expected the tracked SVG sample asset to render instead of falling back"
    );
}

#[test]
fn test_render_markdown_shows_image_fallback_states() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview(&preview);

    preview.render_markdown(
        "![Missing image](missing.png)\n\n![Remote image](https://example.com/remote.png)",
    );
    wait_until(Duration::from_secs(2), || {
        widgets_with_css_class::<gtk4::Box>(&preview, "markdown-preview-image-fallback").len() == 2
    });

    assert!(
        find_label_with_text(&preview, "Image file not found").is_some(),
        "Expected a missing local image fallback"
    );
    assert!(
        find_label_with_text(&preview, "Remote images are not supported").is_some(),
        "Expected a remote-image fallback title"
    );
    let fallback_cards =
        widgets_with_css_class::<gtk4::Box>(&preview, "markdown-preview-image-fallback");
    assert!(
        fallback_cards.iter().all(|card| card.width_request() >= 240),
        "Expected fallback cards to reserve enough width for readable path text"
    );
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

fn emit_preview_click_for_text(preview: &LushtextMarkdownPreview, text: &str) {
    let text_view = preview.text_view();
    let offset = i32::try_from(
        preview
        .buffer_text()
        .find(text)
        .expect("rendered text should exist"),
    )
    .expect("rendered text offset should fit in i32");
    let iter = text_view.buffer().iter_at_offset(offset);
    let rect = text_view.iter_location(&iter);
    let (x, y) = text_view.buffer_to_window_coords(
        gtk4::TextWindowType::Widget,
        rect.x() + 1,
        rect.y() + 1,
    );

    let controllers = text_view.observe_controllers();
    let gesture = (0..controllers.n_items())
        .find_map(|index| {
            controllers
                .item(index)
                .and_then(|object| object.downcast::<gtk4::GestureClick>().ok())
        })
        .expect("text view should have a click controller");
    gesture.emit_by_name::<()>("pressed", &[&1i32, &f64::from(x), &f64::from(y)]);
}

fn tags_for_rendered_text(preview: &LushtextMarkdownPreview, text: &str) -> Vec<String> {
    let text_view = preview.text_view();
    let offset = i32::try_from(
        preview
        .buffer_text()
        .find(text)
        .expect("rendered text should exist"),
    )
    .expect("rendered text offset should fit in i32");
    text_view
        .buffer()
        .iter_at_offset(offset)
        .tags()
        .into_iter()
        .filter_map(|tag| tag.name().map(|name| name.to_string()))
        .collect()
}
