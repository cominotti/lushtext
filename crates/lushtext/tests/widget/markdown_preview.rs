// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextMarkdownPreview widget.

use crate::common::{ensure_gtk_init, present_window, test_application, wait_until};
use gio::prelude::ListModelExt;
use glib::prelude::{Cast, IsA};
use gtk4::prelude::*;
use lushtext_core::config::{self, keys};
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
fn test_preview_background_opacity_tracks_setting() {
    ensure_gtk_init();
    let settings = gio::Settings::new(config::APP_ID);
    settings
        .set_double(keys::TAB_CONTENT_OPACITY, 0.7)
        .expect("set tab-content-opacity");

    let preview = LushtextMarkdownPreview::new();
    assert!((preview.background_opacity() - 0.7).abs() < f64::EPSILON);

    settings
        .set_double(keys::TAB_CONTENT_OPACITY, 0.55)
        .expect("update tab-content-opacity");
    while glib::MainContext::default().iteration(false) {}

    assert!((preview.background_opacity() - 0.55).abs() < f64::EPSILON);
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
fn test_render_tight_unordered_list_has_no_blank_rows() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("- alpha\n- beta\n- gamma");

    assert_eq!(
        preview.buffer_text(),
        "\u{2022} alpha\n\u{2022} beta\n\u{2022} gamma\n",
        "Expected tight unordered lists to render one row per item"
    );
}

#[test]
fn test_render_tight_ordered_list_has_no_blank_rows() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("1. first\n2. second\n3. third");

    assert_eq!(
        preview.buffer_text(),
        "1. first\n2. second\n3. third\n",
        "Expected tight ordered lists to render one row per item"
    );
}

#[test]
fn test_render_nested_list_after_parent_prose_starts_on_child_row() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("- parent text\n  - child text\n- next parent");
    let text = preview.buffer_text();

    assert_eq!(
        text,
        "\u{2022} parent text\n\u{2022} child text\n\u{2022} next parent\n",
        "Expected nested child markers to start on their own rendered row"
    );
    assert!(
        !text.contains("parent text\u{2022}"),
        "Expected parent prose and child marker to never share one rendered row"
    );
}

#[test]
fn test_render_loose_list_preserves_item_paragraph_break_only() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("- first paragraph\n\n  second paragraph\n- next item");
    let text = preview.buffer_text();

    assert_eq!(
        text,
        "\u{2022} first paragraph\n\nsecond paragraph\n\u{2022} next item\n",
        "Expected loose-list paragraph spacing without an extra blank row before the next item"
    );
    assert!(
        !text.contains("\n\n\n"),
        "Expected loose list rendering to avoid duplicated empty rows"
    );
}

#[test]
fn test_render_task_list_uses_one_row_per_item() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("- [x] done\n- [ ] todo");

    assert_eq!(
        preview.buffer_text(),
        "\u{2611} done\n\u{2610} todo\n",
        "Expected task-list marker replacement to follow ordinary list row flow"
    );
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
fn test_ordered_list_markers_share_rendered_row_with_item_text() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("1. Alpha row\n2. Beta row\n3. Gamma row");
    let _window = present_preview_with_size(&preview, 420, 300);

    assert_same_rendered_row(&preview, "2.", "Beta");
    assert_same_rendered_row(&preview, "3.", "Gamma");

    let offset_preview = LushtextMarkdownPreview::new();
    offset_preview.render_markdown("57. Offset row\n58. After offset row");
    let _offset_window = present_preview_with_size(&offset_preview, 420, 300);

    assert_same_rendered_row(&offset_preview, "57.", "Offset");
    assert_same_rendered_row(&offset_preview, "58.", "After");
}

#[test]
fn test_ordered_list_wrapped_lines_align_under_item_text() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown(
        "1. Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda markerend",
    );
    let _window = present_preview_with_size(&preview, 220, 240);

    assert_same_rendered_row(&preview, "1.", "Alpha");
    assert_wrapped_under_item_text(&preview, "1.", "Alpha", "markerend");
}

#[test]
fn test_nested_unordered_wrapped_lines_align_under_child_item_text() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown(
        "- Parent row\n  - Child alpha beta gamma delta epsilon zeta eta theta iota markerend",
    );
    let _window = present_preview_with_size(&preview, 240, 260);

    assert_same_rendered_row(&preview, "\u{2022}", "Parent");
    assert_same_rendered_row_nth(&preview, "\u{2022}", 1, "Child", 0);
    assert_wrapped_under_item_text_nth(&preview, "\u{2022}", 1, "Child", 0, "markerend", 0);
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
fn test_render_blockquote_inserts_rail_without_raw_marker() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("> quoted text");
    let text = preview.buffer_text();

    assert_rendered_text_order(&text, &["\u{2502}", "quoted text"]);
    assert!(
        text.contains("\u{2502} quoted text"),
        "Expected visible quote rail before blockquote text"
    );
    assert!(
        !text.contains("> quoted"),
        "Expected rendered blockquote to hide raw source marker"
    );
    let tags = tags_for_rendered_text(&preview, "quoted text");
    assert!(
        tags.iter().any(|name| name == "blockquote-depth-1"),
        "Expected top-level blockquote text to carry depth tag, got {tags:?}"
    );
}

#[test]
fn test_render_nested_blockquotes_from_adjacent_markers() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown(
        "> parent quote\n>> child quote\n>>> grandchild quote",
    );
    let text = preview.buffer_text();

    assert_rendered_text_order(&text, &["parent quote", "child quote", "grandchild quote"]);
    assert!(
        text.contains("\u{2502} parent quote"),
        "Expected first quote depth to use one rail"
    );
    assert!(
        text.contains("\u{2502} \u{2502} child quote"),
        "Expected second quote depth to use two rails"
    );
    assert!(
        text.contains("\u{2502} \u{2502} \u{2502} grandchild quote"),
        "Expected third quote depth to use three rails"
    );

    for (quoted_text, expected_tag) in [
        ("parent quote", "blockquote-depth-1"),
        ("child quote", "blockquote-depth-2"),
        ("grandchild quote", "blockquote-depth-3"),
    ] {
        let tags = tags_for_rendered_text(&preview, quoted_text);
        assert!(
            tags.iter().any(|name| name == expected_tag),
            "Expected '{quoted_text}' to carry {expected_tag}, got {tags:?}"
        );
    }
}

#[test]
fn test_render_nested_blockquotes_from_spaced_markers() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown(
        "> parent quote\n> > child quote\n> > > grandchild quote",
    );
    let text = preview.buffer_text();

    assert_rendered_text_order(&text, &["parent quote", "child quote", "grandchild quote"]);
    assert!(
        text.contains("\u{2502} \u{2502} \u{2502} grandchild quote"),
        "Expected spaced nested markers to render with the same three-level rail hierarchy"
    );
    let tags = tags_for_rendered_text(&preview, "grandchild quote");
    assert!(
        tags.iter().any(|name| name == "blockquote-depth-3"),
        "Expected spaced nested blockquote to carry depth-3 tag, got {tags:?}"
    );
}

#[test]
fn test_render_blockquote_preserves_inline_formatting() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("> *em* **strong** `code` [link](https://example.com)");
    let text = preview.buffer_text();

    assert!(
        text.contains("\u{2502} em strong code link"),
        "Expected inline blockquote content to remain in one quoted line"
    );
    for (rendered_text, expected_tag) in [
        ("em", "italic"),
        ("strong", "bold"),
        ("code", "code"),
        ("link", "link"),
    ] {
        let tags = tags_for_rendered_text(&preview, rendered_text);
        assert!(
            tags.iter().any(|name| name == expected_tag),
            "Expected '{rendered_text}' to carry {expected_tag}, got {tags:?}"
        );
    }
}

#[test]
fn test_render_gfm_callout_stays_distinct_from_generic_blockquote() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("> [!NOTE]\n> Pay attention.");
    let text = preview.buffer_text();

    assert!(text.contains("Note"), "Expected callout title in preview text");
    assert!(
        !text.contains('\u{2502}'),
        "Expected typed alert callout to avoid generic blockquote rail rendering"
    );
    let tags = tags_for_rendered_text(&preview, "Note");
    assert!(
        tags.iter().any(|name| name == "alert-title-note"),
        "Expected callout title to keep typed alert styling, got {tags:?}"
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
fn test_render_atx_heading_levels_apply_matching_tags_and_hide_markers() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown(
        "# Alpha\n## Beta\n### Gamma\n#### Delta\n##### Epsilon\n###### Zeta",
    );

    for (text, tag) in [
        ("Alpha", "heading1"),
        ("Beta", "heading2"),
        ("Gamma", "heading3"),
        ("Delta", "heading4"),
        ("Epsilon", "heading5"),
        ("Zeta", "heading6"),
    ] {
        let tags = tags_for_rendered_text(&preview, text);
        assert!(
            tags.iter().any(|name| name == tag),
            "Expected '{text}' to carry the {tag} text tag, got {tags:?}"
        );
    }

    let rendered = preview.buffer_text();
    for marker in ["# Alpha", "## Beta", "### Gamma", "#### Delta", "##### Epsilon", "###### Zeta"]
    {
        assert!(
            !rendered.contains(marker),
            "Expected rendered ATX heading to hide raw marker '{marker}'"
        );
    }
}

#[test]
fn test_render_setext_heading_levels_apply_matching_tags_and_hide_underlines() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("Setext Alpha\n============\n\nSetext Beta\n-----------");

    for (text, tag) in [("Setext Alpha", "heading1"), ("Setext Beta", "heading2")] {
        let tags = tags_for_rendered_text(&preview, text);
        assert!(
            tags.iter().any(|name| name == tag),
            "Expected '{text}' to carry the {tag} text tag, got {tags:?}"
        );
    }

    let rendered = preview.buffer_text();
    assert!(
        !rendered.contains("============"),
        "Expected Setext H1 underline to be omitted from rendered text"
    );
    assert!(
        !rendered.contains("-----------"),
        "Expected Setext H2 underline to be omitted from rendered text"
    );
}

#[test]
fn test_render_heading_flow_preserves_source_order() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("Before\n\n# First Heading\n\nBetween\n\n## Second Heading\n\nAfter");

    assert_rendered_text_order(
        &preview.buffer_text(),
        &["Before", "First Heading", "Between", "Second Heading", "After"],
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

fn present_preview_with_size(
    preview: &LushtextMarkdownPreview,
    width: i32,
    height: i32,
) -> gtk4::ApplicationWindow {
    let window = gtk4::ApplicationWindow::new(&test_application());
    window.set_default_size(width, height);
    window.set_child(Some(preview));
    present_window(&window);
    wait_until(Duration::from_secs(2), || {
        preview.text_view().width() > 0 && preview.text_view().height() > 0
    });
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

fn assert_rendered_text_order(rendered: &str, expected: &[&str]) {
    let mut previous = 0usize;
    for text in expected {
        let offset = rendered[previous..].find(text).map_or_else(
            || panic!("expected rendered text to contain '{text}' after byte {previous}"),
            |relative| previous + relative,
        );
        previous = offset + text.len();
    }
}

fn emit_preview_click_for_text(preview: &LushtextMarkdownPreview, text: &str) {
    let text_view = preview.text_view();
    let offset = rendered_text_char_offset(preview, text);
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
    let offset = rendered_text_char_offset(preview, text);
    text_view
        .buffer()
        .iter_at_offset(offset)
        .tags()
        .into_iter()
        .filter_map(|tag| tag.name().map(|name| name.to_string()))
        .collect()
}

fn assert_same_rendered_row(preview: &LushtextMarkdownPreview, marker: &str, text: &str) {
    assert_same_rendered_row_nth(preview, marker, 0, text, 0);
}

fn assert_same_rendered_row_nth(
    preview: &LushtextMarkdownPreview,
    marker: &str,
    marker_occurrence: usize,
    text: &str,
    text_occurrence: usize,
) {
    let marker_rect = rendered_text_location_nth(preview, marker, marker_occurrence);
    let text_rect = rendered_text_location_nth(preview, text, text_occurrence);
    assert_eq!(
        marker_rect.y(),
        text_rect.y(),
        "Expected marker '{marker}' occurrence {marker_occurrence} and text '{text}' occurrence {text_occurrence} to share a rendered row"
    );
    assert!(
        marker_rect.x() < text_rect.x(),
        "Expected marker '{marker}' to remain visually before item text '{text}'"
    );
}

fn assert_wrapped_under_item_text(
    preview: &LushtextMarkdownPreview,
    marker: &str,
    first_text: &str,
    wrapped_text: &str,
) {
    assert_wrapped_under_item_text_nth(preview, marker, 0, first_text, 0, wrapped_text, 0);
}

fn assert_wrapped_under_item_text_nth(
    preview: &LushtextMarkdownPreview,
    marker: &str,
    marker_occurrence: usize,
    first_text: &str,
    first_text_occurrence: usize,
    wrapped_text: &str,
    wrapped_text_occurrence: usize,
) {
    let marker_rect = rendered_text_location_nth(preview, marker, marker_occurrence);
    let first_text_rect = rendered_text_location_nth(preview, first_text, first_text_occurrence);
    let wrapped_rect = rendered_text_location_nth(preview, wrapped_text, wrapped_text_occurrence);

    assert!(
        wrapped_rect.y() > first_text_rect.y(),
        "Expected '{wrapped_text}' to wrap onto a later rendered row"
    );
    assert!(
        wrapped_rect.x() >= first_text_rect.x() - 2,
        "Expected wrapped text x={} to align under item text x={} rather than marker x={}",
        wrapped_rect.x(),
        first_text_rect.x(),
        marker_rect.x()
    );
    assert!(
        wrapped_rect.x() > marker_rect.x() + 8,
        "Expected wrapped text to stay out of the marker column"
    );
}

fn rendered_text_location_nth(
    preview: &LushtextMarkdownPreview,
    text: &str,
    occurrence: usize,
) -> gtk4::gdk::Rectangle {
    let text_view = preview.text_view();
    let offset = rendered_text_char_offset_nth(preview, text, occurrence);
    let iter = text_view.buffer().iter_at_offset(offset);
    text_view.iter_location(&iter)
}

/// Return a GTK text offset for rendered text found through Rust string APIs.
///
/// `str::find` reports byte offsets, while `TextBuffer::iter_at_offset` expects
/// character offsets. Rendered Markdown can contain Unicode rails and bullets,
/// so tests must convert before probing tags or click locations.
fn rendered_text_char_offset(preview: &LushtextMarkdownPreview, text: &str) -> i32 {
    rendered_text_char_offset_nth(preview, text, 0)
}

/// Return the character offset for one occurrence of rendered text.
fn rendered_text_char_offset_nth(
    preview: &LushtextMarkdownPreview,
    text: &str,
    occurrence: usize,
) -> i32 {
    let rendered = preview.buffer_text();
    let mut search_start = 0usize;
    let mut byte_offset = None;

    for _ in 0..=occurrence {
        let relative = rendered[search_start..]
            .find(text)
            .unwrap_or_else(|| panic!("rendered text should contain occurrence {occurrence} of '{text}'"));
        byte_offset = Some(search_start + relative);
        search_start += relative + text.len();
    }

    let byte_offset = byte_offset.expect("occurrence loop should run at least once");
    i32::try_from(rendered[..byte_offset].chars().count())
        .expect("rendered text offset should fit in i32")
}
