// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextMarkdownPreview widget.

use crate::common::{
    ensure_gtk_init, fixture, flush_after_delay, flush_events, fs_metadata, present_window,
    test_application, wait_until,
};
use gio::prelude::ListModelExt;
use glib::prelude::{Cast, IsA};
use gtk4::prelude::*;
use lushtext_core::config::{self, keys};
use lushtext_core::services::markdown_render::{
    MARKDOWN_EVENTS_PER_PROJECTION_SLICE, MAX_MARKDOWN_PLACEHOLDER_WIDGETS,
    MAX_MARKDOWN_SOURCE_BYTES,
};
use lushtext_core::ui::accessibility::test_audit::AccessibleAudit;
use lushtext_core::ui::markdown_preview::{
    LushtextMarkdownPreview, MarkdownPreviewRenderContext, MarkdownRenderState,
};
use lushtext_core::ui::plain_disposal::{
    hold_disposal_capacity_for_test, lane_snapshot_for_test,
};
use sourceview5::prelude::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};
use tempfile::TempDir;

struct ImageWorkDelayReset;

impl Drop for ImageWorkDelayReset {
    fn drop(&mut self) {
        LushtextMarkdownPreview::set_image_work_delay_for_test(0);
        LushtextMarkdownPreview::set_image_post_decode_delay_for_test(0);
    }
}

struct MarkdownPlanDelayReset;

impl Drop for MarkdownPlanDelayReset {
    fn drop(&mut self) {
        LushtextMarkdownPreview::set_markdown_plan_delay_for_test(0);
    }
}

#[test]
fn test_new() {
    ensure_gtk_init();
    let _preview = LushtextMarkdownPreview::new();
}

/// Proof 1 of 3 for `MarkdownPreviewEvidence` — **reentrancy**.
///
/// One accessor reads the whole surface through shared borrows, so no field may
/// be read from inside a mutable borrow of the state it reads. This drives the
/// workflow through each operation that takes such a borrow, reads the surface
/// *after* each one, and asserts repeated reads of unchanged state are identical.
/// It deliberately never reads the surface *while* a borrow is held — that is the
/// panic the constraint prevents, not a demonstration of it.
///
/// The hazard here is concrete rather than theoretical: `render_pending()`
/// re-borrows four of the same `RefCell`s the accessor already read
/// (`render_session`, `queued_plan`, `deferred_work`, `retirement`), and
/// `placeholder_description()` reaches a template child. Every operation below
/// mutates at least one of those.
#[test]
fn test_preview_evidence_reads_stay_side_effect_free_across_mutation() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();

    // No content yet: the surface must answer without a render having happened.
    // The template ships a default placeholder description, so the initial value
    // is that string rather than `None` — measured, not assumed. (`None` is
    // reserved for the disposed case, which proof 2 covers.)
    let initial = preview.evidence();
    assert_eq!(preview.evidence(), initial, "repeated reads must be identical");
    assert_eq!(
        initial.placeholder_description.as_deref(),
        Some("Open a Markdown file to see a rendered preview"),
        "the template's default placeholder description"
    );

    // `show_placeholder` mutates the placeholder template child and the content
    // mode flag.
    preview.show_placeholder("Not a Markdown file");
    let placeholder = preview.evidence();
    assert_eq!(
        placeholder.placeholder_description.as_deref(),
        Some("Not a Markdown file")
    );
    assert_eq!(preview.evidence(), placeholder);

    // `render_markdown` takes mutable borrows of the render session and the
    // queued plan, and clears the placeholder.
    preview.render_markdown("# Heading\n\nBody text.\n");
    let rendered = preview.evidence();
    assert!(preview.is_showing_content());
    assert_eq!(preview.evidence(), rendered);

    // A second render supersedes the first, which is the path that writes
    // `queued_plan` and advances the render generation.
    preview.render_markdown("# Second\n\nDifferent body.\n");
    let superseded = preview.evidence();
    assert_eq!(preview.evidence(), superseded);

    // `clear` detaches the buffer and arms retirement, mutating `retirement`.
    preview.clear();
    let cleared = preview.evidence();
    assert_eq!(preview.evidence(), cleared, "repeated reads after clear");

    // Drain whatever retirement armed, then read again: the drain mutates the
    // same `retirement` cell the surface reports from.
    flush_events();
    let drained = preview.evidence();
    assert_eq!(preview.evidence(), drained);
}

/// Proof 2 of 3 for `MarkdownPreviewEvidence` — **disposal honesty**.
///
/// GTK4 clears template children in `dispose()`, before Rust's `Drop`. The
/// surface's `placeholder_description` field is derived from an `AdwStatusPage`
/// template child, so it must go through `try_get()` and answer honestly once
/// that child is gone. Reading the surface at teardown must not panic, because
/// one accessor now reaches every field from every observation point.
#[test]
fn test_preview_evidence_answers_honestly_after_disposal() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.show_placeholder("Not a Markdown file");
    assert_eq!(
        preview.evidence().placeholder_description.as_deref(),
        Some("Not a Markdown file"),
        "sanity: the placeholder is readable while the widget is alive"
    );

    // SAFETY: this standalone test preview is disposed exactly once, and
    // everything after this point only reads the evidence surface, which must
    // answer honestly on a disposed widget rather than panicking.
    unsafe { preview.run_dispose() };

    let disposed = preview.evidence();
    assert!(
        disposed.placeholder_description.is_none(),
        "a disposed widget must report no placeholder rather than panicking"
    );
    assert_eq!(
        preview.evidence(),
        disposed,
        "repeated reads of a disposed widget must stay identical"
    );
}

/// Proof 3 of 3 for `MarkdownPreviewEvidence` — **non-materialization**.
///
/// The surface must not make the toolkit do work, and must not advance a metric
/// it reports — an observer that changes what it observes is not an observation.
/// This row has real counters to check that against: projection dispatches,
/// planning source copies, retirement high-water marks, image admission
/// high-water marks, and the code-block traversal count are all surface fields
/// *and* live counters, so a read that touched any of them would be visible here.
#[test]
fn test_preview_evidence_reads_materialize_no_state() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("# Heading\n\n| a | b |\n| - | - |\n| 1 | 2 |\n");
    flush_events();

    let before = preview.evidence();
    for _ in 0..8 {
        let _ = preview.evidence();
    }
    let after = preview.evidence();

    assert_eq!(
        after, before,
        "eight reads must not change any field the surface reports"
    );
    // Spelled out for the counters most likely to drift, so a future field that
    // does advance on read fails with a specific message rather than a whole
        // struct diff.
    assert_eq!(after.projection.dispatch_count, before.projection.dispatch_count);
    assert_eq!(after.planning.source_copies, before.planning.source_copies);
    assert_eq!(
        after.retirement.generations_high_water,
        before.retirement.generations_high_water
    );
    assert_eq!(
        after.images.high_water_count,
        before.images.high_water_count
    );
    assert_eq!(
        after.code_blocks.width_traversal_count,
        before.code_blocks.width_traversal_count
    );
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
fn test_preview_surface_exposes_read_only_accessibility_metadata() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();

    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Document)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&preview);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::TextBox)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ReadOnly,
            gtk4::AccessibleProperty::MultiLine,
        ])
        .assert_on(&preview.text_view());
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
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Status)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(
            &descendants(&preview)
                .into_iter()
                .filter_map(|widget| widget.downcast::<libadwaita::StatusPage>().ok())
                .next()
                .expect("placeholder status page"),
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
fn test_render_inline_code_inserts_text() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("Use `cargo build` to compile.");
    assert!(
        preview.buffer_text().contains("cargo build"),
        "Expected inline code in buffer"
    );
    assert!(
        widgets_with_css_class::<sourceview5::View>(&preview, "markdown-code-block-view")
            .is_empty(),
        "Expected inline code to stay in the text buffer instead of creating a block widget"
    );
    let tags = tags_for_rendered_text(&preview, "cargo build");
    assert!(
        tags.iter().any(|name| name == "code"),
        "Expected inline code text to keep the inline code tag, got {tags:?}"
    );
}

#[test]
fn test_render_code_block_exposes_read_only_accessibility_metadata() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview(&preview);

    preview.render_markdown("```rust\nfn main() {}\n```");
    wait_until(Duration::from_secs(2), || {
        !code_block_containers(&preview).is_empty() && !source_views(&preview).is_empty()
    });

    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Group)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(
            &code_block_containers(&preview)
                .into_iter()
                .next()
                .expect("code block container"),
        );
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::TextBox)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ReadOnly,
            gtk4::AccessibleProperty::MultiLine,
        ])
        .assert_on(
            &source_views(&preview)
                .into_iter()
                .next()
                .expect("code block source view"),
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
    flush_after_delay(Duration::from_millis(20));
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
fn test_render_simple_definition_list_hides_colon_marker() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("Term\n: Definition");

    assert_eq!(
        preview.buffer_text(),
        "Term\nDefinition\n",
        "Expected pulldown-cmark definition lists to render term and definition rows"
    );
    assert!(
        preview.has_tag("definition-term") && preview.has_tag("definition-definition"),
        "Expected definition-list tags to be registered"
    );
    assert!(
        tags_for_rendered_text(&preview, "Term")
            .iter()
            .any(|name| name == "definition-term"),
        "Expected term text to carry definition-term styling"
    );
    assert!(
        tags_for_rendered_text(&preview, "Definition")
            .iter()
            .any(|name| name == "definition-definition"),
        "Expected definition text to carry definition-definition styling"
    );
}

#[test]
fn test_render_definition_list_preserves_multiple_definitions_order() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("Term\n: First definition\n: Second definition\n\nNext\n: Another");
    let text = preview.buffer_text();

    assert_rendered_text_order(
        &text,
        &[
            "Term",
            "First definition",
            "Second definition",
            "Next",
            "Another",
        ],
    );
    assert!(
        !text.contains(": First") && !text.contains(": Second") && !text.contains(": Another"),
        "Expected raw definition markers to stay out of rendered text, got {text:?}"
    );
}

#[test]
fn test_render_definition_list_preserves_inline_formatting() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("*Inline Term*\n: Definition with **strong** and `code`");

    for (rendered_text, expected_tags) in [
        ("Inline Term", ["definition-term", "italic"]),
        ("strong", ["definition-definition", "bold"]),
        ("code", ["definition-definition", "code"]),
    ] {
        let tags = tags_for_rendered_text(&preview, rendered_text);
        for expected_tag in expected_tags {
            assert!(
                tags.iter().any(|name| name == expected_tag),
                "Expected '{rendered_text}' to carry {expected_tag}, got {tags:?}"
            );
        }
    }
}

#[test]
fn test_render_definition_list_preserves_nested_paragraphs() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("Term\n\n:   First paragraph\n\n    Second paragraph");
    let text = preview.buffer_text();

    assert_rendered_text_order(&text, &["Term", "First paragraph", "Second paragraph"]);
    assert!(
        text.contains("First paragraph\n\nSecond paragraph"),
        "Expected definition paragraphs to keep readable separation, got {text:?}"
    );
    assert!(
        !text.contains("\n\n\n"),
        "Expected definition paragraph rendering to avoid duplicated blank rows, got {text:?}"
    );
}

#[test]
fn test_render_definition_list_preserves_nested_lists() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("Term\n\n:   Intro\n\n    - child one\n    - child two");
    let text = preview.buffer_text();

    assert_rendered_text_order(&text, &["Term", "Intro", "child one", "child two"]);
    assert!(
        text.contains("\u{2022} child one\n\u{2022} child two"),
        "Expected nested ordinary lists to keep their list markers inside definitions, got {text:?}"
    );
    let tags = tags_for_rendered_text(&preview, "child one");
    assert!(
        tags.iter().any(|name| name == "definition-definition")
            && tags.iter().any(|name| name == "list-item-depth-1"),
        "Expected nested list text to keep both definition and list tags, got {tags:?}"
    );
}

#[test]
fn test_render_definition_list_preserves_nested_blockquote() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("Term\n\n:   Intro\n\n    > quoted definition");
    let text = preview.buffer_text();

    assert_rendered_text_order(&text, &["Term", "Intro", "\u{2502}", "quoted definition"]);
    let tags = tags_for_rendered_text(&preview, "quoted definition");
    assert!(
        tags.iter().any(|name| name == "definition-definition")
            && tags.iter().any(|name| name == "blockquote-depth-1"),
        "Expected nested blockquote text to keep definition and blockquote tags, got {tags:?}"
    );
}

#[test]
fn test_render_definition_list_code_block_without_false_horizontal_overflow() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview_with_size(&preview, 640, 300);

    preview.render_markdown(
        "Term\n\n:   Definition\n\n        const readable = true;\n\n    Third paragraph",
    );
    wait_for_code_block_layout(&preview);

    let source_view = source_views(&preview).pop().expect("source view");
    assert_eq!(
        source_view_buffer_text(&source_view),
        "const readable = true;\n",
        "Expected definition-list code block text to live in one embedded source buffer"
    );
    assert_nested_code_block_geometry(&preview);
    let scroller = code_block_scrollers(&preview).pop().expect("code scroller");
    let overflow = horizontal_overflow(&scroller);
    assert!(
        overflow <= 1.0,
        "Expected nested definition-list code to fit without false horizontal overflow, got {overflow}"
    );
    assert!(
        preview.buffer_text().contains("Third paragraph"),
        "Expected prose after the nested code block to keep rendering"
    );
}

#[test]
fn test_render_definition_list_screenshot_code_block_without_false_horizontal_overflow() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview_with_size(&preview, 720, 360);

    preview.render_markdown(concat!(
        "Term 2 with *inline markup*\n",
        "\n",
        ":   Definition 2\n",
        "\n",
        "        { some code, part of Definition 2 }\n",
        "\n",
        "    Third paragraph of definition 2.",
    ));
    wait_for_code_block_layout(&preview);

    let source_view = source_views(&preview).pop().expect("source view");
    assert_eq!(
        source_view_buffer_text(&source_view),
        "{ some code, part of Definition 2 }\n",
        "Expected screenshot-style definition-list code to stay in one source buffer"
    );
    assert_nested_code_block_geometry(&preview);
    let scroller = code_block_scrollers(&preview).pop().expect("code scroller");
    let overflow = horizontal_overflow(&scroller);
    assert!(
        overflow <= 1.0,
        "Expected screenshot-style definition-list code to fit without horizontal overflow, got {overflow}"
    );
}

#[test]
fn test_render_definition_list_code_block_width_updates_after_late_allocation() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();

    preview.render_markdown("Term\n\n:   Definition\n\n        allocated later\n");
    let _window = present_preview_with_size(&preview, 620, 280);
    wait_for_code_block_layout(&preview);

    assert_nested_code_block_geometry(&preview);
}

#[test]
fn test_render_definition_list_code_block_allows_horizontal_overflow_for_long_line() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview_with_size(&preview, 280, 300);
    let long_line = "{ ".to_string() + &"definition_code_segment ".repeat(16) + "}";

    preview.render_markdown(&format!("Term\n\n:   Definition\n\n        {long_line}\n"));
    wait_for_code_block_layout(&preview);

    assert_nested_code_block_geometry(&preview);
    let scroller = code_block_scrollers(&preview).pop().expect("code scroller");
    let overflow = horizontal_overflow(&scroller);
    assert!(
        overflow > 1.0,
        "Expected genuinely long nested definition-list code to expose horizontal overflow"
    );
}

#[test]
fn test_render_tilde_definition_marker_syntax_stays_plain_text() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("Term ~ Definition");
    let text = preview.buffer_text();

    assert_eq!(text, "Term ~ Definition\n");
    let tags = tags_for_rendered_text(&preview, "Term");
    assert!(
        tags.iter()
            .all(|name| name != "definition-term" && name != "definition-definition"),
        "Expected markdown-it tilde syntax to stay outside definition-list styling, got {tags:?}"
    );
}

#[test]
fn test_render_ordinary_colon_prose_stays_plain_text() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("Status: Ready");
    let text = preview.buffer_text();

    assert_eq!(text, "Status: Ready\n");
    let tags = tags_for_rendered_text(&preview, "Status");
    assert!(
        tags.iter()
            .all(|name| name != "definition-term" && name != "definition-definition"),
        "Expected ordinary colon prose to stay outside definition-list styling, got {tags:?}"
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
fn test_render_inline_footnote_reference_and_definition() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("hello^[inline note].");
    let text = preview.buffer_text();
    assert!(
        text.contains("hello[1]."),
        "Expected inline footnote marker in preview text"
    );
    assert!(
        text.contains("[1] inline note"),
        "Expected generated inline footnote definition in preview text"
    );
    assert!(
        !text.contains("^[inline note]"),
        "Expected raw inline footnote syntax to be replaced in preview text"
    );
}

#[test]
fn test_render_inline_footnote_preserves_inline_formatting_in_definition() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("hello^[inline **bold** and `code`].");
    let text = preview.buffer_text();
    assert!(
        text.contains("[1] inline bold and code"),
        "Expected generated definition to contain inline footnote body text"
    );

    let bold_tags = tags_for_rendered_text(&preview, "bold");
    assert!(
        bold_tags.iter().any(|name| name == "bold"),
        "Expected inline footnote definition bold text to keep bold tag, got {bold_tags:?}"
    );
    let code_tags = tags_for_rendered_text(&preview, "code");
    assert!(
        code_tags.iter().any(|name| name == "code"),
        "Expected inline footnote definition code text to keep code tag, got {code_tags:?}"
    );
}

#[test]
fn test_render_mixed_inline_and_reference_style_footnotes() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("First^[Inline note].\n\nSecond[^ref].\n\n[^ref]: Reference note");
    let text = preview.buffer_text();

    assert!(
        text.contains("First[1]."),
        "Expected inline footnote to receive first rendered marker"
    );
    assert!(
        text.contains("Second[2]."),
        "Expected reference-style footnote marker to keep matching numbering"
    );
    assert!(
        text.contains("[1] Inline note"),
        "Expected inline footnote definition to match marker number"
    );
    assert!(
        text.contains("[2] Reference note"),
        "Expected reference-style definition to match marker number"
    );
    assert!(
        !text.contains("__lush_inline_footnote_") && !text.contains("^[Inline note]"),
        "Expected generated labels and raw inline footnote syntax to stay hidden"
    );
    assert!(
        !text.contains("[^ref]"),
        "Expected existing reference-style source marker to remain rendered, not raw"
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
fn test_dense_markdown_projects_over_bounded_main_loop_turns() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let markdown = (0..400)
        .map(|index| format!("paragraph {index}\n\n"))
        .collect::<String>();

    preview.render_markdown(&markdown);
    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Projecting);
    wait_until(Duration::from_secs(5), || !preview.render_pending());

    let projection = preview.evidence().projection;
    let (dispatches, high_water_events) =
        (projection.dispatch_count, projection.high_water_events);
    assert!(dispatches > 1, "dense Markdown should yield between batches");
    assert!(high_water_events <= MARKDOWN_EVENTS_PER_PROJECTION_SLICE);
    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Complete);
    assert!(preview.buffer_text().contains("paragraph 399"));
}

#[test]
fn test_rapid_large_markdown_renders_keep_one_planner_and_latest_request() {
    ensure_gtk_init();
    let _delay_reset = MarkdownPlanDelayReset;
    LushtextMarkdownPreview::set_markdown_plan_delay_for_test(250);
    let preview = LushtextMarkdownPreview::new();
    let source = |label: &str| {
        (0..4_000)
            .map(|index| format!("{label} paragraph {index}\n\n"))
            .collect::<String>()
    };

    preview.render_markdown(&source("first"));
    preview.render_markdown(&source("second"));
    preview.render_markdown(&source("latest"));

    let planning = preview.evidence().planning;
    assert!(planning.worker_running && planning.queued);
    wait_until(Duration::from_secs(10), || !preview.render_pending());
    let planning = preview.evidence().planning;
    assert!(!planning.worker_running && !planning.queued);
    let text = preview.buffer_text();
    assert!(text.contains("latest paragraph 3999"));
    assert!(!text.contains("first paragraph"));
    assert!(!text.contains("second paragraph"));
    let retirement = preview.evidence().retirement;
    let (_, _, _, _, plain_jobs, pending_plain_jobs, plain_high_water) = (
        retirement.detached_generations,
        retirement.generations_high_water,
        usize::from(retirement.deferred_work_pending),
        retirement.max_generations,
        retirement.plain_jobs,
        retirement.plain_pending,
        retirement.plain_pending_high_water,
    );
    assert_eq!(
        plain_jobs, 0,
        "superseded queued sources should coalesce in the retained allocation"
    );
    assert_eq!(pending_plain_jobs, 0);
    assert_eq!(plain_high_water, 0);
}

#[test]
fn test_markdown_capacity_pressure_copies_only_after_admission() {
    ensure_gtk_init();
    wait_until(Duration::from_secs(5), || {
        let snapshot = lane_snapshot_for_test();
        snapshot.running_jobs == 0 && snapshot.queued_jobs == 0
    });
    let capacity_hold = hold_disposal_capacity_for_test();
    let preview = LushtextMarkdownPreview::new();
    let source = format!(
        "# Admitted preview\n\n{}",
        "bounded source text\n\n".repeat(4_000)
    );
    let copies_before = preview.evidence().planning.source_copies;

    preview.render_markdown(&source);
    preview.render_markdown(&source);
    preview.render_markdown(&source);
    wait_until(Duration::from_secs(5), || !preview.render_pending());
    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Failed);
    assert!(preview.buffer_text().contains("memory pressure"));
    assert_eq!(
        preview.evidence().planning.source_copies,
        copies_before,
        "capacity rejection must retain no unguarded Markdown source"
    );

    drop(capacity_hold);
    preview.render_markdown(&source);
    wait_until(Duration::from_secs(10), || !preview.render_pending());

    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Complete);
    assert!(preview.buffer_text().contains("Admitted preview"));
    assert!(preview.buffer_text().contains("bounded source text"));
    assert_eq!(
        preview.evidence().planning.source_copies,
        copies_before + 1
    );
}

#[test]
fn test_snapshot_markdown_capacity_and_source_limit_publish_compact_terminals() {
    ensure_gtk_init();
    wait_until(Duration::from_secs(5), || {
        let snapshot = lane_snapshot_for_test();
        snapshot.running_jobs == 0 && snapshot.queued_jobs == 0
    });
    let capacity_hold = hold_disposal_capacity_for_test();
    let preview = LushtextMarkdownPreview::new();

    preview.render_snapshot_for_test("guarded snapshot source".repeat(8_000));
    wait_until(Duration::from_secs(10), || !preview.render_pending());
    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Failed);
    assert!(preview.buffer_text().contains("memory pressure"));

    preview.render_snapshot_for_test("x".repeat(MAX_MARKDOWN_SOURCE_BYTES + 1));
    wait_until(Duration::from_secs(10), || !preview.render_pending());
    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Limited);
    assert!(
        preview
            .buffer_text()
            .contains("source exceeds 4 MiB")
    );

    drop(capacity_hold);
}

#[test]
fn test_rapid_rerenders_cap_detached_generations_and_keep_latest_work() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();

    preview.render_markdown("first generation");
    preview.render_markdown("second generation");
    preview.render_markdown("third generation");
    preview.render_markdown("latest generation");

    let retirement = preview.evidence().retirement;
    let (detached, high_water, deferred, limit, _, _, _) = (
        retirement.detached_generations,
        retirement.generations_high_water,
        usize::from(retirement.deferred_work_pending),
        retirement.max_generations,
        retirement.plain_jobs,
        retirement.plain_pending,
        retirement.plain_pending_high_water,
    );
    assert_eq!(detached, limit);
    assert_eq!(high_water, limit);
    assert_eq!(limit, 2);
    assert_eq!(deferred, 1);
    assert_eq!(preview.buffer_text().trim(), "third generation");

    wait_until(Duration::from_secs(10), || !preview.render_pending());

    assert_eq!(preview.buffer_text().trim(), "latest generation");
    let retirement = preview.evidence().retirement;
    let (detached, high_water, deferred, limit, _, pending_plain_jobs, _) = (
        retirement.detached_generations,
        retirement.generations_high_water,
        usize::from(retirement.deferred_work_pending),
        retirement.max_generations,
        retirement.plain_jobs,
        retirement.plain_pending,
        retirement.plain_pending_high_water,
    );
    assert_eq!(detached, 0);
    assert!(high_water <= limit);
    assert_eq!(deferred, 0);
    assert_eq!(pending_plain_jobs, 0);
    eprintln!(
        "markdown-retirement-bound-evidence detached_generations={detached} detached_high_water={high_water} ordinary_limit={limit} deferred_latest={deferred} pending_plain_jobs={pending_plain_jobs}"
    );
}

#[test]
fn test_deferred_large_source_moves_into_background_planner_after_retirement() {
    ensure_gtk_init();
    let _delay_reset = MarkdownPlanDelayReset;
    LushtextMarkdownPreview::set_markdown_plan_delay_for_test(250);
    let preview = LushtextMarkdownPreview::new();

    preview.render_markdown("first generation");
    preview.render_markdown("second generation");
    preview.render_markdown("third generation");
    let latest = (0..5_000)
        .map(|index| format!("owned deferred paragraph {index}\n\n"))
        .collect::<String>();
    preview.render_markdown(&latest);

    assert_eq!(usize::from(preview.evidence().retirement.deferred_work_pending), 1);
    wait_until(Duration::from_secs(10), || {
        (preview.evidence().planning.worker_running, preview.evidence().planning.queued)
            == (true, false)
    });
    wait_until(Duration::from_secs(10), || !preview.render_pending());
    assert!(preview.buffer_text().contains("owned deferred paragraph 4999"));
    let planning = preview.evidence().planning;
    assert!(!planning.worker_running && !planning.queued);
}

#[test]
fn test_placeholder_close_remains_terminal_under_retirement_pressure() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("first generation");
    preview.render_markdown("second generation");
    preview.render_markdown("third generation");
    assert_eq!(preview.evidence().retirement.detached_generations, 2);

    preview.show_placeholder("Preview closed under pressure");

    assert!(!preview.is_showing_content());
    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Cancelled);
    wait_until(Duration::from_secs(10), || !preview.render_pending());
    assert_eq!(
        preview.placeholder_description().as_deref(),
        Some("Preview closed under pressure")
    );
    let retirement = preview.evidence().retirement;
    let (detached, high_water, deferred, limit, _, pending_plain_jobs, _) = (
        retirement.detached_generations,
        retirement.generations_high_water,
        usize::from(retirement.deferred_work_pending),
        retirement.max_generations,
        retirement.plain_jobs,
        retirement.plain_pending,
        retirement.plain_pending_high_water,
    );
    assert_eq!(detached, 0);
    assert_eq!(high_water, limit + 1);
    assert_eq!(deferred, 0);
    assert_eq!(pending_plain_jobs, 0);
}

#[test]
fn test_repeated_terminal_updates_reuse_the_single_escape_generation() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("first generation");
    preview.render_markdown("second generation");
    preview.render_markdown("third generation");
    assert_eq!(preview.evidence().retirement.detached_generations, 2);

    preview.show_render_failure("terminal one");
    for index in 2..=8 {
        preview.show_render_failure(&format!("terminal {index}"));
    }

    let retirement = preview.evidence().retirement;
    let (detached, high_water, _, limit, _, _, _) = (
        retirement.detached_generations,
        retirement.generations_high_water,
        usize::from(retirement.deferred_work_pending),
        retirement.max_generations,
        retirement.plain_jobs,
        retirement.plain_pending,
        retirement.plain_pending_high_water,
    );
    assert_eq!(detached, limit + 1);
    assert_eq!(high_water, limit + 1);
    assert_eq!(preview.buffer_text(), "terminal 8");
    wait_until(Duration::from_secs(10), || !preview.render_pending());
}

#[test]
fn test_large_render_teardown_is_detached_and_retired_in_bounded_turns() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let source = (0..4_000)
        .map(|index| format!("retired paragraph {index} with enough retained text\n\n"))
        .collect::<String>();

    preview.render_markdown(&source);
    wait_until(Duration::from_secs(10), || !preview.render_pending());
    assert!(preview.buffer_text().contains("retired paragraph 3999"));

    preview.render_markdown("current generation");
    assert_eq!(preview.buffer_text().trim(), "current generation");
    assert!(preview.render_pending());
    wait_until(Duration::from_secs(10), || !preview.render_pending());

    let retirement = preview.evidence().retirement;
    let (retired_chars, retired_items) =
        (retirement.chars_high_water, retirement.items_high_water);
    assert!(retired_chars <= 64 * 1024);
    assert!(retired_items <= 64);
    assert_eq!(preview.buffer_text().trim(), "current generation");
}

#[test]
fn test_dense_single_block_uses_accessible_simplified_terminal() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let markdown = (0..300).map(|_| "**x** ").collect::<String>();

    preview.render_markdown(&markdown);

    assert!(!preview.render_pending());
    assert_eq!(
        preview.evidence().render_state,
        MarkdownRenderState::Simplified,
        "a document planned to its end with one omission is complete-with-omissions, not stopped"
    );
    let text = preview.buffer_text();
    assert!(
        text.contains("Markdown preview complete; 1 block was too complex to render"),
        "expected the completion terminal copy, got: {text}"
    );
    assert!(
        !text.contains("exceeds a projection slice"),
        "the retired stopped-preview copy must not come back: {text}"
    );

    // The omitted top-level block is replaced in place by exactly one fallback.
    let fallbacks = widgets_with_css_class::<gtk4::Box>(&preview, "markdown-omission-fallback");
    assert_eq!(fallbacks.len(), 1);
    assert!(
        find_label_with_text(&preview, "Markdown preview omitted one block that exceeds 256 render events")
            .is_some(),
        "the marker must name the crossed budget"
    );
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Group)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&fallbacks[0]);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::TextBox)
        .properties(&[gtk4::AccessibleProperty::Description])
        .assert_on(&preview.text_view());
}

/// A document whose two footnote references land in different projection
/// batches, so batch-local numbering restarts at the second reference.
fn multi_batch_footnote_fixture() -> String {
    let mut markdown = String::from("alpha-ref[^a]\n\n");
    for index in 0..120 {
        markdown.push_str(&format!("filler paragraph {index}\n\n"));
    }
    markdown.push_str("beta-ref[^b]\n\n[^a]: alpha definition\n\n[^b]: beta definition\n");
    markdown
}

/// Footnote numbering is owned by the render generation, not by one batch.
///
/// This is the inverted form of the section 1 characterization test: numbering
/// used to restart at every projection batch, so a reference and its definition
/// in different GTK turns disagreed.
#[test]
fn test_footnote_numbering_continues_across_projection_batches() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();

    preview.render_markdown(&multi_batch_footnote_fixture());
    wait_until(Duration::from_secs(10), || !preview.render_pending());

    let text = preview.buffer_text();
    assert!(
        preview.evidence().projection.dispatch_count > 1,
        "the fixture must span several projection turns"
    );
    assert!(
        text.contains("alpha-ref[1]"),
        "first reference should be numbered 1: {text}"
    );
    assert!(
        text.contains("beta-ref[2]"),
        "the second reference must continue the generation numbering: {text}"
    );
    assert!(
        !text.contains("beta-ref[1]"),
        "numbering must not restart per batch: {text}"
    );
    // Each definition keeps the number its reference showed.
    assert_rendered_text_order(&text, &["alpha-ref[1]", "beta-ref[2]", "[1] alpha definition", "[2] beta definition"]);
}

/// Rows and columns of the oversized-but-renderable values table.
///
/// 505 cells stays inside the preview's 1,000-cell widget budget while ~1,700
/// parser events are far past one projection slice, so the table can only render
/// completely if its rows accumulate into one continuous widget across turns.
const OVERSIZED_TABLE_ROWS: usize = 100;
const OVERSIZED_TABLE_COLUMNS: usize = 5;

fn oversized_table_fixture() -> String {
    let mut markdown = String::from("# Values\n\n| c0 | c1 | c2 | c3 | c4 |\n");
    markdown.push_str("| --- | --- | --- | --- | --- |\n");
    for row in 0..OVERSIZED_TABLE_ROWS {
        for column in 0..OVERSIZED_TABLE_COLUMNS {
            markdown.push_str(&format!("| r{row}c{column} "));
        }
        markdown.push_str("|\n");
    }
    markdown.push_str("\nTAIL-AFTER-TABLE\n");
    markdown
}

fn oversized_ordered_list_fixture() -> String {
    let mut markdown = String::from("# Ordered\n\n");
    for index in 1..=100 {
        markdown.push_str(&format!("{index}. item-{index}\n"));
    }
    markdown.push_str("\nTAIL-AFTER-ORDERED-LIST\n");
    markdown
}

fn oversized_blockquote_fixture() -> String {
    let mut markdown = String::from("# Quote\n\n");
    for index in 0..90 {
        markdown.push_str(&format!("> quoted-{index}\n>\n"));
    }
    markdown.push_str("> > nested-quoted\n\nTAIL-AFTER-QUOTE\n");
    markdown
}

fn oversized_definition_list_fixture() -> String {
    let mut markdown = String::from("# Definitions\n\n");
    for index in 0..60 {
        markdown.push_str(&format!("term-{index}\n: definition-{index}\n\n"));
    }
    markdown.push_str("\nTAIL-AFTER-DEFINITIONS\n");
    markdown
}

/// Lines in the indented code fixture, which is over one projection slice in
/// events but far inside the code-block byte budget.
const INDENTED_CODE_LINES: usize = 400;

fn indented_code_block_fixture() -> String {
    let mut markdown = String::from("# Indented\n\n");
    for index in 0..INDENTED_CODE_LINES {
        markdown.push_str(&format!("    indented-line-{index}\n"));
    }
    markdown.push_str("\nTAIL-AFTER-INDENTED-CODE\n");
    markdown
}

#[test]
fn test_oversized_table_renders_one_widget_with_every_row() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();

    preview.render_markdown(&oversized_table_fixture());
    wait_until(Duration::from_secs(15), || !preview.render_pending());

    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Complete);
    let grids = widgets_with_css_class::<gtk4::Grid>(&preview, "markdown-table");
    assert_eq!(
        grids.len(),
        1,
        "an oversized table must stay one continuous table widget"
    );
    assert!(
        widgets_with_css_class::<gtk4::Box>(&preview, "markdown-preview-limit-fallback").is_empty(),
        "a table inside the cell budget must not degrade"
    );
    // Check every cell, not three sampled rows: a sub-slicing bug that dropped
    // one interior batch would sit between the samples and pass unnoticed. The
    // label set is collected once rather than walking the tree per cell.
    let cell_labels: std::collections::HashSet<String> =
        widgets_with_css_class::<gtk4::Label>(&preview, "markdown-table-cell")
            .iter()
            .map(|label| label.text().to_string())
            .collect();
    for row in 0..OVERSIZED_TABLE_ROWS {
        for column in 0..OVERSIZED_TABLE_COLUMNS {
            let cell = format!("r{row}c{column}");
            assert!(
                cell_labels.contains(&cell),
                "row {row} column {column} missing from the rendered table"
            );
        }
    }
    assert!(preview.buffer_text().contains("TAIL-AFTER-TABLE"));
    let projection = preview.evidence().projection;
    let (dispatches, high_water) =
        (projection.dispatch_count, projection.high_water_events);
    assert!(dispatches > 1, "the table must be projected over several turns");
    assert!(high_water <= MARKDOWN_EVENTS_PER_PROJECTION_SLICE);
}

#[test]
fn test_oversized_ordered_list_keeps_continuous_numbering() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();

    preview.render_markdown(&oversized_ordered_list_fixture());
    wait_until(Duration::from_secs(15), || !preview.render_pending());

    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Complete);
    let text = preview.buffer_text();
    assert!(preview.evidence().projection.dispatch_count > 1);
    for index in 1..=100 {
        assert!(
            text.contains(&format!("{index}. item-{index}")),
            "ordered item {index} lost its numbering: {text}"
        );
    }
    assert!(text.contains("TAIL-AFTER-ORDERED-LIST"));
}

#[test]
fn test_oversized_blockquote_keeps_rail_depth_across_turns() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();

    preview.render_markdown(&oversized_blockquote_fixture());
    wait_until(Duration::from_secs(15), || !preview.render_pending());

    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Complete);
    assert!(preview.evidence().projection.dispatch_count > 1);
    let text = preview.buffer_text();
    assert_rendered_text_order(&text, &["quoted-0", "quoted-89", "nested-quoted", "TAIL-AFTER-QUOTE"]);
    assert!(
        !text.contains("> quoted-89"),
        "the raw quote marker must stay hidden after a projection boundary: {text}"
    );
    assert!(
        preview.has_tag("blockquote-depth-2"),
        "nested rail depth must survive the carried blockquote continuation"
    );
}

#[test]
fn test_oversized_definition_list_renders_every_entry() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();

    preview.render_markdown(&oversized_definition_list_fixture());
    wait_until(Duration::from_secs(15), || !preview.render_pending());

    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Complete);
    assert!(preview.evidence().projection.dispatch_count > 1);
    let text = preview.buffer_text();
    for index in 0..60 {
        assert!(
            text.contains(&format!("term-{index}")),
            "definition term {index} missing"
        );
        assert!(
            text.contains(&format!("definition-{index}")),
            "definition body {index} missing"
        );
    }
    assert!(text.contains("TAIL-AFTER-DEFINITIONS"));
}

#[test]
fn test_fenced_code_block_within_budget_renders_whole_in_one_slice() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let mut body = String::new();
    for index in 0..400 {
        body.push_str(&format!("echo fenced-line-{index}\n"));
    }
    assert!(body.len() < 64 * 1024, "fixture must stay inside the widget budget");
    let markdown = format!("# Script\n\n```sh\n{body}```\n\nTAIL-AFTER-FENCE\n");

    preview.render_markdown(&markdown);
    wait_until(Duration::from_secs(15), || !preview.render_pending());

    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Complete);
    // A fenced body is one coalesced `Text` event, so the whole block fits one
    // slice: this is a single-turn render, not a sub-sliced one.
    assert_eq!(
        preview.evidence().projection.dispatch_count,
        1,
        "a fenced block is three events regardless of line count"
    );
    let views = source_views(&preview);
    assert_eq!(views.len(), 1, "one code block renders as one surface");
    let buffer = views[0].buffer();
    let rendered = buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string();
    assert!(rendered.contains("echo fenced-line-0"));
    assert!(rendered.contains("echo fenced-line-399"));
    assert!(preview.buffer_text().contains("TAIL-AFTER-FENCE"));
}

#[test]
fn test_indented_code_block_over_one_slice_renders_one_continuous_surface() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();

    preview.render_markdown(&indented_code_block_fixture());
    wait_until(Duration::from_secs(15), || !preview.render_pending());

    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Complete);
    let projection = preview.evidence().projection;
    let (dispatches, high_water) =
        (projection.dispatch_count, projection.high_water_events);
    assert!(
        dispatches > 1,
        "an indented block emits one text event per line, so it is sub-sliced"
    );
    assert!(high_water <= MARKDOWN_EVENTS_PER_PROJECTION_SLICE);

    let views = source_views(&preview);
    assert_eq!(
        views.len(),
        1,
        "a sub-sliced code block must not split into several surfaces"
    );
    assert!(
        widgets_with_css_class::<gtk4::Box>(&preview, "markdown-preview-limit-fallback").is_empty()
    );
    let buffer = views[0].buffer();
    let rendered = buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string();
    let lines: Vec<&str> = rendered.lines().collect();
    assert_eq!(lines.len(), INDENTED_CODE_LINES, "every line must be present once");
    for index in [0usize, INDENTED_CODE_LINES / 2, INDENTED_CODE_LINES - 1] {
        assert_eq!(lines[index], format!("indented-line-{index}"));
    }
    assert!(preview.buffer_text().contains("TAIL-AFTER-INDENTED-CODE"));
}

#[test]
fn test_code_block_past_the_widget_budget_keeps_its_single_fallback_and_completes() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let body_bytes = 96 * 1024;
    let markdown = format!(
        "# Big\n\n```text\n{}\n```\n\nTAIL-AFTER-BIG-CODE\n",
        "c".repeat(body_bytes)
    );

    preview.render_markdown(&markdown);
    wait_until(Duration::from_secs(20), || !preview.render_pending());

    assert_eq!(
        preview.evidence().render_state,
        MarkdownRenderState::Complete,
        "a carried-embed crossing is a charge carrier, not a user-visible omission"
    );
    assert!(
        !preview.buffer_text().contains("too complex to render"),
        "the block explains itself through its own fallback"
    );
    assert!(
        widgets_with_css_class::<gtk4::Box>(&preview, "markdown-omission-fallback").is_empty(),
        "no omission marker may accompany the in-place fallback"
    );
    let fallbacks =
        widgets_with_css_class::<gtk4::Box>(&preview, "markdown-code-block-fallback");
    assert_eq!(fallbacks.len(), 1, "exactly today's single fallback widget");
    assert!(source_views(&preview).is_empty(), "no partial code surface");
    assert!(
        find_label_with_text(&preview, &format!("This code block is {} bytes", body_bytes + 1))
            .is_some(),
        "the fallback must report the block's true source size"
    );
    assert!(preview.buffer_text().contains("TAIL-AFTER-BIG-CODE"));
}

/// Columns and rows of the table that crosses the 1,000-cell widget budget.
const PAST_BUDGET_COLUMNS: usize = 4;
const PAST_BUDGET_ROWS: usize = 250;

fn cell_table(columns: usize, rows: usize) -> String {
    let mut markdown = String::new();
    for _ in 0..columns {
        markdown.push_str("| h ");
    }
    markdown.push_str("|\n");
    for _ in 0..columns {
        markdown.push_str("| --- ");
    }
    markdown.push_str("|\n");
    for _ in 0..rows {
        for _ in 0..columns {
            markdown.push_str("| c ");
        }
        markdown.push_str("|\n");
    }
    markdown
}

#[test]
fn test_table_past_the_cell_budget_keeps_its_single_fallback_and_completes() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let markdown = format!(
        "# Past budget\n\n{}\nTAIL-AFTER-PAST-BUDGET\n",
        cell_table(PAST_BUDGET_COLUMNS, PAST_BUDGET_ROWS)
    );

    preview.render_markdown(&markdown);
    wait_until(Duration::from_secs(20), || !preview.render_pending());

    assert_eq!(
        preview.evidence().render_state,
        MarkdownRenderState::Complete,
        "a cell-ceiling crossing must not turn into a user-visible omission"
    );
    assert!(!preview.buffer_text().contains("too complex to render"));
    assert!(
        widgets_with_css_class::<gtk4::Box>(&preview, "markdown-omission-fallback").is_empty()
    );
    let fallbacks = widgets_with_css_class::<gtk4::Box>(&preview, "markdown-table-fallback");
    assert_eq!(fallbacks.len(), 1);
    assert!(
        widgets_with_css_class::<gtk4::Grid>(&preview, "markdown-table").is_empty(),
        "no partially rendered grid may accompany the fallback"
    );
    let total_cells = PAST_BUDGET_COLUMNS * (PAST_BUDGET_ROWS + 1);
    assert!(
        find_label_with_text(&preview, &format!("This table has {total_cells} cells")).is_some(),
        "the fallback must report the table's true cell count"
    );
    assert!(preview.buffer_text().contains("TAIL-AFTER-PAST-BUDGET"));
}

#[test]
fn test_table_past_the_cell_budget_inside_a_footnote_charges_an_empty_builder() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    // A footnote definition forbids cuts, so the planner has no checkpoint
    // inside it and withdraws the whole table body: the projector receives
    // `Start(Table)` immediately followed by `End(Table)` and must still charge
    // the unretained cells onto the empty builder for the fallback to fire.
    let mut markdown = String::from("See[^t].\n\n[^t]: footnote intro prose\n\n");
    for line in cell_table(PAST_BUDGET_COLUMNS, PAST_BUDGET_ROWS).lines() {
        markdown.push_str("    ");
        markdown.push_str(line);
        markdown.push('\n');
    }
    markdown.push_str("\nTAIL-AFTER-FOOTNOTE-TABLE\n");

    preview.render_markdown(&markdown);
    wait_until(Duration::from_secs(20), || !preview.render_pending());

    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Complete);
    let fallbacks = widgets_with_css_class::<gtk4::Box>(&preview, "markdown-table-fallback");
    assert_eq!(fallbacks.len(), 1, "the empty builder must still degrade");
    let total_cells = PAST_BUDGET_COLUMNS * (PAST_BUDGET_ROWS + 1);
    assert!(
        find_label_with_text(&preview, &format!("This table has {total_cells} cells")).is_some()
    );
    let text = preview.buffer_text();
    assert!(text.contains("footnote intro prose"), "{text}");
    assert!(text.contains("TAIL-AFTER-FOOTNOTE-TABLE"));
}

#[test]
fn test_large_byte_table_within_the_cell_budget_renders_every_row() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    // 603 cells of ~100 bytes: far past the 64 KiB byte ceiling that bounds
    // carried code text, and well inside the 1,000-cell table budget.
    let filler = "z".repeat(100);
    let mut markdown = String::from("# Wide bytes\n\n| a | b | c |\n| --- | --- | --- |\n");
    for row in 0..200 {
        markdown.push_str(&format!("| r{row}-{filler} | {filler} | {filler} |\n"));
    }
    markdown.push_str("\nTAIL-AFTER-WIDE-BYTES\n");

    preview.render_markdown(&markdown);
    wait_until(Duration::from_secs(20), || !preview.render_pending());

    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Complete);
    assert_eq!(
        widgets_with_css_class::<gtk4::Grid>(&preview, "markdown-table").len(),
        1
    );
    assert!(
        widgets_with_css_class::<gtk4::Box>(&preview, "markdown-preview-limit-fallback").is_empty(),
        "no retention ceiling may truncate a large-byte table"
    );
    for row in [0, 199] {
        assert!(
            find_label_with_text(&preview, &format!("r{row}-")).is_some(),
            "row {row} missing from a large-byte table"
        );
    }
    assert!(preview.buffer_text().contains("TAIL-AFTER-WIDE-BYTES"));
}

#[test]
fn test_wide_cell_table_renders_every_row() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    // Eight cells holding ~192 KiB: few cells, huge bytes. This renders today
    // and must keep rendering.
    let cell = "q".repeat(24 * 1024);
    let mut markdown = String::from("| a | b |\n| --- | --- |\n");
    for row in 0..3 {
        markdown.push_str(&format!("| r{row} {cell} | {cell} |\n"));
    }
    markdown.push_str("\nTAIL-AFTER-WIDE-CELLS\n");

    preview.render_markdown(&markdown);
    wait_until(Duration::from_secs(20), || !preview.render_pending());

    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Complete);
    assert_eq!(
        widgets_with_css_class::<gtk4::Grid>(&preview, "markdown-table").len(),
        1
    );
    assert!(
        widgets_with_css_class::<gtk4::Box>(&preview, "markdown-preview-limit-fallback").is_empty()
    );
    assert!(preview.buffer_text().contains("TAIL-AFTER-WIDE-CELLS"));
}

#[test]
fn test_one_overflowing_loose_list_item_keeps_siblings_and_marks_inside_the_item() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let dense = (0..300).map(|_| "**x** ").collect::<String>();
    let mut markdown = String::from("# Mixed list\n\n");
    for index in 0..60 {
        if index == 17 {
            markdown.push_str(&format!("- {dense}\n\n"));
        } else {
            markdown.push_str(&format!("- item-{index}\n\n"));
        }
    }
    markdown.push_str("\nTAIL-AFTER-MIXED-LIST\n");

    preview.render_markdown(&markdown);
    wait_until(Duration::from_secs(20), || !preview.render_pending());

    assert_eq!(
        preview.evidence().render_state,
        MarkdownRenderState::Simplified
    );
    let text = preview.buffer_text();
    for index in (0..60).filter(|index| *index != 17) {
        assert!(text.contains(&format!("item-{index}")), "sibling {index} lost");
    }
    let markers: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("Markdown preview omitted"))
        .collect();
    assert_eq!(markers.len(), 1, "exactly one in-container marker: {markers:?}");
    assert!(
        markers[0].contains("one list item"),
        "the marker must name the omitted unit: {markers:?}"
    );
    assert!(
        markers[0].contains('\u{2022}'),
        "the overflowing item must still render as an item: {markers:?}"
    );
    assert!(
        widgets_with_css_class::<gtk4::Box>(&preview, "markdown-omission-fallback").is_empty(),
        "a container-segment omission must not add a widget"
    );
    assert!(
        text.contains("Markdown preview complete; 1 block was too complex to render"),
        "{text}"
    );
    assert!(text.contains("TAIL-AFTER-MIXED-LIST"));
}

#[test]
fn test_one_overflowing_table_row_becomes_a_spanning_row_in_the_same_table() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let dense = (0..300).map(|_| "**x** ").collect::<String>();
    let markdown = format!(
        "# Rows\n\n| a | b |\n| --- | --- |\n| keep-0a | keep-0b |\n| {dense} | dropped-b |\n| keep-2a | keep-2b |\n\nTAIL-AFTER-ROW-OMISSION\n"
    );

    preview.render_markdown(&markdown);
    wait_until(Duration::from_secs(20), || !preview.render_pending());

    assert_eq!(
        preview.evidence().render_state,
        MarkdownRenderState::Simplified
    );
    assert_eq!(
        widgets_with_css_class::<gtk4::Grid>(&preview, "markdown-table").len(),
        1,
        "the omitted row must stay inside the same table"
    );
    assert!(
        widgets_with_css_class::<gtk4::Box>(&preview, "markdown-preview-limit-fallback").is_empty(),
        "one dropped row must not replace the whole table"
    );
    for kept in ["keep-0a", "keep-0b", "keep-2a", "keep-2b"] {
        assert!(
            find_label_with_text(&preview, kept).is_some(),
            "sibling cell {kept} lost"
        );
    }
    let omission_rows =
        widgets_with_css_class::<gtk4::Label>(&preview, "markdown-table-omission-row");
    assert_eq!(omission_rows.len(), 1);
    assert!(
        omission_rows[0].text().contains("one table row"),
        "the spanning row must name the omitted unit: {}",
        omission_rows[0].text()
    );
    assert!(
        preview
            .buffer_text()
            .contains("Markdown preview complete; 1 block was too complex to render")
    );
    assert!(preview.buffer_text().contains("TAIL-AFTER-ROW-OMISSION"));
}

#[test]
fn test_top_level_omissions_stop_building_widgets_at_the_placeholder_cap() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let dense = (0..100).map(|_| "**x** ").collect::<String>();
    let blocks = MAX_MARKDOWN_PLACEHOLDER_WIDGETS + 4;
    let mut markdown = String::new();
    for _ in 0..blocks {
        markdown.push_str(&dense);
        markdown.push_str("\n\n");
    }
    markdown.push_str("TAIL-AFTER-MANY-OMISSIONS\n");

    preview.render_markdown(&markdown);
    wait_until(Duration::from_secs(30), || !preview.render_pending());

    assert_eq!(
        preview.evidence().render_state,
        MarkdownRenderState::Simplified
    );
    let fallbacks = widgets_with_css_class::<gtk4::Box>(&preview, "markdown-omission-fallback");
    assert_eq!(
        fallbacks.len(),
        MAX_MARKDOWN_PLACEHOLDER_WIDGETS,
        "omission widgets must stop at the placeholder cap"
    );
    let text = preview.buffer_text();
    let inline_markers = text
        .lines()
        .filter(|line| line.contains("Markdown preview omitted one block"))
        .count();
    assert_eq!(
        inline_markers, 4,
        "omissions past the cap must still be accessible inline text: {text}"
    );
    assert!(
        text.contains(&format!(
            "Markdown preview complete; {blocks} blocks were too complex to render"
        )),
        "the terminal must count every user-visible omission: {text}"
    );
    assert!(text.contains("TAIL-AFTER-MANY-OMISSIONS"));
}

/// Padding and leading span that put a projection boundary exactly on
/// `Start(CodeBlock)`; the planner test
/// `a_turn_boundary_can_fall_between_a_code_block_start_and_its_text` pins that
/// property for this exact shape.
const CODE_BLOCK_BOUNDARY_PAD_ITEMS: usize = 100;
const CODE_BLOCK_BOUNDARY_LEAD_SPANS: usize = 1;

fn code_block_start_boundary_fixture() -> String {
    let mut markdown = String::from("# Boundary\n\n");
    markdown.push_str(&format!(
        "{}\n\n",
        "`a` ".repeat(CODE_BLOCK_BOUNDARY_LEAD_SPANS)
    ));
    for index in 0..CODE_BLOCK_BOUNDARY_PAD_ITEMS {
        markdown.push_str(&format!("- pad-{index}\n"));
    }
    markdown.push_str("- item with code\n\n  ```sh\n  echo tiny\n  ```\n\n");
    markdown.push_str("- after-code\n\nTAIL-AFTER-BOUNDARY\n");
    markdown
}

#[test]
fn test_tiny_code_block_survives_a_content_free_turn_boundary() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();

    preview.render_markdown(&code_block_start_boundary_fixture());
    wait_until(Duration::from_secs(20), || !preview.render_pending());

    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Complete);
    assert!(
        preview.evidence().projection.dispatch_count > 1,
        "the enclosing list must overflow into several turns"
    );
    let views = source_views(&preview);
    assert_eq!(
        views.len(),
        1,
        "a code block opened on one turn and filled on the next is still one surface"
    );
    let buffer = views[0].buffer();
    let rendered = buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string();
    assert!(rendered.contains("echo tiny"), "code text lost: {rendered}");
    let text = preview.buffer_text();
    assert!(text.contains("pad-0"));
    assert!(text.contains("after-code"));
    assert!(text.contains("TAIL-AFTER-BOUNDARY"));
}

#[test]
fn test_continuation_survives_a_constrained_preview_shell() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let window = gtk4::Window::new();
    window.set_default_size(420, 320);
    window.set_child(Some(&preview));
    present_window(&window);

    let markdown = format!(
        "{}\n{}",
        oversized_table_fixture(),
        oversized_ordered_list_fixture()
    );
    preview.render_markdown(&markdown);
    wait_until(Duration::from_secs(20), || !preview.render_pending());
    flush_after_delay(Duration::from_millis(50));

    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Complete);
    assert_eq!(
        widgets_with_css_class::<gtk4::Grid>(&preview, "markdown-table").len(),
        1,
        "the carried table must stay one widget inside a narrow shell"
    );
    let text = preview.buffer_text();
    assert!(text.contains("TAIL-AFTER-TABLE"));
    assert!(text.contains("100. item-100"), "{text}");
    assert!(text.contains("TAIL-AFTER-ORDERED-LIST"));
    window.destroy();
}

#[test]
fn test_rerender_mid_sub_sliced_block_drops_the_stale_continuation() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();

    preview.render_markdown(&oversized_table_fixture());
    assert!(
        preview.render_pending(),
        "the oversized table must still be projecting"
    );
    preview.render_markdown("latest generation");
    wait_until(Duration::from_secs(20), || !preview.render_pending());

    assert_eq!(preview.buffer_text().trim(), "latest generation");
    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Complete);
    assert!(
        widgets_with_css_class::<gtk4::Grid>(&preview, "markdown-table").is_empty(),
        "no stale table widget may survive the generation change"
    );
    let retirement = preview.evidence().retirement;
    let (chars, items) = (retirement.chars_high_water, retirement.items_high_water);
    assert!(chars <= 64 * 1024);
    assert!(items <= 64);
    let retirement = preview.evidence().retirement;
    let (detached, high_water, deferred, limit, _, pending_plain_jobs, _) = (
        retirement.detached_generations,
        retirement.generations_high_water,
        usize::from(retirement.deferred_work_pending),
        retirement.max_generations,
        retirement.plain_jobs,
        retirement.plain_pending,
        retirement.plain_pending_high_water,
    );
    assert_eq!(detached, 0);
    assert!(high_water <= limit);
    assert_eq!(deferred, 0);
    assert_eq!(pending_plain_jobs, 0);
}

#[test]
fn test_cancel_mid_sub_sliced_block_leaves_no_stale_widget() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();

    preview.render_markdown(&oversized_table_fixture());
    assert!(preview.render_pending());
    preview.show_placeholder("Preview closed mid-table");
    wait_until(Duration::from_secs(20), || !preview.render_pending());

    assert!(!preview.is_showing_content());
    assert_eq!(
        preview.evidence().render_state,
        MarkdownRenderState::Cancelled
    );
    assert!(
        widgets_with_css_class::<gtk4::Grid>(&preview, "markdown-table").is_empty(),
        "cancellation must retire the carried table"
    );
    assert_eq!(preview.evidence().retirement.plain_pending, 0);
}

#[test]
fn test_teardown_mid_sub_sliced_block_releases_the_continuation() {
    ensure_gtk_init();
    {
        let preview = LushtextMarkdownPreview::new();
        preview.render_markdown(&oversized_table_fixture());
        assert!(preview.render_pending());
    }
    // Dropping the preview mid-projection must let the idle projector observe a
    // dead weak reference and release the continuation with its plan.
    wait_until(Duration::from_secs(20), || {
        let snapshot = lane_snapshot_for_test();
        snapshot.running_jobs == 0 && snapshot.queued_jobs == 0
    });
    flush_after_delay(Duration::from_millis(50));
}

#[test]
fn test_oversized_fixtures_stay_within_the_projection_slice_budget() {
    ensure_gtk_init();
    for markdown in [
        oversized_table_fixture(),
        oversized_ordered_list_fixture(),
        oversized_blockquote_fixture(),
        oversized_definition_list_fixture(),
        indented_code_block_fixture(),
    ] {
        let preview = LushtextMarkdownPreview::new();
        preview.render_markdown(&markdown);
        wait_until(Duration::from_secs(20), || !preview.render_pending());

        let projection = preview.evidence().projection;
    let (dispatches, high_water) =
        (projection.dispatch_count, projection.high_water_events);
        assert!(dispatches > 1, "every fixture must span several turns");
        assert!(
            high_water <= MARKDOWN_EVENTS_PER_PROJECTION_SLICE,
            "a projection turn applied {high_water} events"
        );
    }
}

#[test]
fn test_render_failure_is_accessible_and_terminal() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();

    preview.show_render_failure("Markdown plan failed");

    assert!(!preview.render_pending());
    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Failed);
    assert_eq!(preview.buffer_text(), "Markdown plan failed");
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::TextBox)
        .properties(&[gtk4::AccessibleProperty::Description])
        .assert_on(&preview.text_view());
}

#[test]
fn test_new_render_generation_rejects_stale_projection_slices() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let old = (0..400)
        .map(|index| format!("obsolete {index}\n\n"))
        .collect::<String>();

    preview.render_markdown(&old);
    preview.render_markdown("latest generation");
    wait_until(Duration::from_secs(10), || !preview.render_pending());

    assert_eq!(preview.buffer_text().trim(), "latest generation");
    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Complete);
    assert!(preview.evidence().retirement.plain_jobs >= 1);
}

#[test]
fn test_placeholder_cancels_background_markdown_plan() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let source = (0..12_000)
        .map(|index| format!("background paragraph {index}\n\n"))
        .collect::<String>();
    assert!(source.len() > 64 * 1024);

    preview.render_markdown(&source);
    preview.show_placeholder("Preview closed");
    flush_after_delay(Duration::from_millis(300));

    assert!(!preview.is_showing_content());
    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Cancelled);
    assert_eq!(
        preview.placeholder_description().as_deref(),
        Some("Preview closed")
    );
}

#[test]
fn test_image_flood_keeps_one_decoder_and_bounded_compact_ownership() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let tempdir = tempfile::tempdir().expect("image flood tempdir");
    let context = MarkdownPreviewRenderContext::new(
        Some(tempdir.path().join("document.md")),
        Vec::new(),
    );
    let markdown = (0..12)
        .map(|index| format!("![image {index}](missing-{index}.png)\n\n"))
        .collect::<String>();

    preview.render_markdown_with_context(&markdown, &context);
    let images = preview.evidence().images;
    let (count_limit, byte_limit) = (images.max_work_items, images.max_work_bytes);
    let images = preview.evidence().images;
    let (high_count, high_bytes) = (images.high_water_count, images.high_water_bytes);
    assert!(high_count <= count_limit);
    assert!(high_bytes <= byte_limit);
    assert_eq!(high_count, count_limit);
    wait_until(Duration::from_secs(5), || !preview.render_pending());
    let images = preview.evidence().images;
    let (owned_count, owned_bytes) = (images.owned_count, images.owned_bytes);
    assert_eq!((owned_count, owned_bytes), (0, 0));
    assert_eq!(
        widgets_with_css_class::<gtk4::Box>(&preview, "markdown-preview-image-fallback").len(),
        12
    );
}

#[test]
fn test_stale_image_completion_cannot_mutate_new_render_generation() {
    ensure_gtk_init();
    let _delay_reset = ImageWorkDelayReset;
    LushtextMarkdownPreview::reset_image_work_observations_for_test();
    LushtextMarkdownPreview::set_image_work_delay_for_test(250);
    let preview = LushtextMarkdownPreview::new();
    let tempdir = tempfile::tempdir().expect("stale image tempdir");
    let context = MarkdownPreviewRenderContext::new(
        Some(tempdir.path().join("document.md")),
        (0..1_000)
            .map(|index| tempdir.path().join(format!("folder-{index:04}")))
            .collect(),
    );

    preview.render_markdown_with_context("![old](missing.png)", &context);
    assert!(preview.render_pending());
    preview.render_markdown("new generation");
    flush_after_delay(Duration::from_millis(400));

    assert_eq!(preview.buffer_text().trim(), "new generation");
    assert_eq!(preview.evidence().render_state, MarkdownRenderState::Complete);
    let images = preview.evidence().images;
    let (owned_count, owned_bytes) = (images.owned_count, images.owned_bytes);
    assert_eq!((owned_count, owned_bytes), (0, 0));
    let images = preview.evidence().images;
    let (inspected, cancelled, decoded, _, _) = (
        images.candidate_inspections,
        images.cancelled_work,
        images.decoded_results,
        images.pixel_drops,
        images.pixel_drops_on_gtk,
    );
    assert_eq!(inspected, 0, "superseded work must stop before candidate I/O");
    assert!(cancelled >= 1);
    assert_eq!(decoded, 0);
}

#[test]
fn test_superseded_decoded_image_pixels_retire_off_the_gtk_thread() {
    ensure_gtk_init();
    let _delay_reset = ImageWorkDelayReset;
    LushtextMarkdownPreview::reset_image_work_observations_for_test();
    LushtextMarkdownPreview::set_image_post_decode_delay_for_test(250);
    let preview = LushtextMarkdownPreview::new();
    let tempdir = tempfile::tempdir().expect("stale decoded image tempdir");
    fixture::write_text(
        &tempdir.path().join("image.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="80"><rect width="120" height="80" fill="#2e7d32"/></svg>"##,
    );
    let context = MarkdownPreviewRenderContext::new(
        Some(tempdir.path().join("document.md")),
        Vec::new(),
    );

    preview.render_markdown_with_context("![old](image.svg)", &context);
    wait_until(Duration::from_secs(5), || {
        preview.evidence().images.decoded_results == 1
    });
    preview.render_markdown("new generation");
    wait_until(Duration::from_secs(5), || !preview.render_pending());
    wait_until(Duration::from_secs(5), || {
        preview.evidence().images.pixel_drops >= 1
    });

    assert_eq!(preview.buffer_text().trim(), "new generation");
    let images = preview.evidence().images;
    let (_, cancelled, decoded, pixel_drops, gtk_pixel_drops) = (
        images.candidate_inspections,
        images.cancelled_work,
        images.decoded_results,
        images.pixel_drops,
        images.pixel_drops_on_gtk,
    );
    assert!(cancelled >= 1);
    assert_eq!(decoded, 1);
    assert_eq!(pixel_drops, 1);
    assert_eq!(gtk_pixel_drops, 0);
}

#[test]
fn test_oversized_local_image_resolves_to_accessible_fallback() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let tempdir = tempfile::tempdir().expect("oversized image tempdir");
    let image_path = tempdir.path().join("oversized.png");
    let source_limit = preview.evidence().images.max_source_bytes;
    fixture::write_repeated_bytes(
        &image_path,
        b"x",
        source_limit.saturating_add(1),
    );
    let context = MarkdownPreviewRenderContext::new(
        Some(tempdir.path().join("document.md")),
        Vec::new(),
    );

    preview.render_markdown_with_context("![oversized](oversized.png)", &context);
    wait_until(Duration::from_secs(5), || !preview.render_pending());

    assert!(find_label_with_text(&preview, "Image could not be loaded").is_some());
    let fallback = widgets_with_css_class::<gtk4::Box>(
        &preview,
        "markdown-preview-image-fallback",
    )
    .into_iter()
    .next()
    .expect("oversized image fallback");
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Img)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&fallback);
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
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Table)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(
            &widgets_with_css_class::<gtk4::Grid>(&preview, "markdown-table")
                .into_iter()
                .next()
                .expect("markdown table"),
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
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::ColumnHeader)
        .properties(&[gtk4::AccessibleProperty::Label])
        .assert_on(&header_cells[0]);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Cell)
        .properties(&[gtk4::AccessibleProperty::Label])
        .assert_on(&body_cells[0]);
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
    let repo_root =
        fs_metadata::canonical_path(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .expect("repo root");
    let context = MarkdownPreviewRenderContext::new(
        Some(repo_root.join("samples/markdown-test.md")),
        Vec::new(),
    );
    preview.render_markdown_with_context(
        "![File-relative preview card sample](assets/preview-secondary.svg)",
        &context,
    );
    wait_until(Duration::from_secs(2), || {
        !widgets_with_css_class::<gtk4::Picture>(&preview, "markdown-preview-image").is_empty()
    });

    assert!(
        !widgets_with_css_class::<gtk4::Picture>(&preview, "markdown-preview-image").is_empty(),
        "Expected the resolved local image to render as a preview picture"
    );
    assert!(
        widgets_with_css_class::<gtk4::Box>(&preview, "markdown-preview-image-fallback")
            .is_empty(),
        "Expected the tracked SVG sample asset to render instead of falling back"
    );
}

#[test]
fn test_render_markdown_uses_first_loadable_workspace_image_candidate() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview(&preview);
    let tempdir = tempfile::tempdir().expect("workspace image tempdir");
    let first_folder = tempdir.path().join("first");
    let second_folder = tempdir.path().join("second");
    fixture::create_dir_all(&first_folder.join("images"));
    fixture::create_dir_all(&second_folder.join("images"));
    fixture::write_bytes(&first_folder.join("images/logo.svg"), b"not an image");
    fixture::write_text(
        &second_folder.join("images/logo.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="80"><rect width="120" height="80" fill="#2e7d32"/></svg>"##,
    );
    let context = MarkdownPreviewRenderContext::new(None, vec![first_folder, second_folder]);

    preview.render_markdown_with_context("![Workspace logo](images/logo.svg)", &context);
    wait_until(Duration::from_secs(2), || {
        !widgets_with_css_class::<gtk4::Picture>(&preview, "markdown-preview-image").is_empty()
    });

    assert!(
        !widgets_with_css_class::<gtk4::Picture>(&preview, "markdown-preview-image").is_empty(),
        "Expected the first loadable workspace-relative image candidate to render as a picture"
    );
    assert!(
        widgets_with_css_class::<gtk4::Box>(&preview, "markdown-preview-image-fallback")
            .is_empty(),
        "Expected an unloadable earlier workspace candidate not to force a fallback"
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
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Img)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&fallback_cards[0]);
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

#[test]
fn test_render_fenced_code_block_adds_embedded_source_view() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview(&preview);

    preview.render_markdown("```\nlet x = 42;\n```");
    wait_until(Duration::from_secs(2), || source_views(&preview).len() == 1);

    let source_view = source_views(&preview).pop().expect("source view");
    assert_eq!(
        source_view_buffer_text(&source_view),
        "let x = 42;\n",
        "Expected code block text to live in one embedded source buffer"
    );
    assert!(
        !preview.buffer_text().contains("let x = 42;"),
        "Expected code block text to no longer be duplicated in the parent preview buffer"
    );
}

#[test]
fn test_render_indented_code_block_adds_embedded_source_view() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview(&preview);

    preview.render_markdown("Indented code\n\n    line one\n    line two");
    wait_until(Duration::from_secs(2), || source_views(&preview).len() == 1);

    let source_view = source_views(&preview).pop().expect("source view");
    let source_text = source_view_buffer_text(&source_view);
    assert!(
        source_text.contains("line one\nline two"),
        "Expected indented Markdown code to render inside one embedded source buffer, got {source_text:?}"
    );
    assert!(
        source_view_source_buffer(&source_view).language().is_none(),
        "Expected indented code blocks to render as plain code without a fenced language hint"
    );
}

#[test]
fn test_render_code_block_with_blank_line_uses_one_embedded_block() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview(&preview);

    preview.render_markdown("```js\nvar foo = function (bar) {\n  return bar++;\n};\n\nconsole.log(foo(5));\n```");
    wait_until(Duration::from_secs(2), || {
        widgets_with_css_class::<gtk4::Box>(&preview, "markdown-code-block").len() == 1
            && source_views(&preview).len() == 1
    });

    let source_view = source_views(&preview).pop().expect("source view");
    let text = source_view_buffer_text(&source_view);
    assert!(
        text.contains("};\n\nconsole.log"),
        "Expected the blank line to remain inside one continuous code buffer, got {text:?}"
    );
    assert_eq!(
        widgets_with_css_class::<gtk4::Box>(&preview, "markdown-code-block").len(),
        1,
        "Expected one visual code block instead of splitting around blank lines"
    );
}

#[test]
fn test_render_supported_fenced_language_applies_source_language() {
    ensure_gtk_init();
    let _language_dir = install_test_source_language("lush-test");
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview(&preview);

    preview.render_markdown("```lush-test\nvar value = 1;\n```");
    wait_until(Duration::from_secs(2), || source_views(&preview).len() == 1);

    let source_view = source_views(&preview).pop().expect("source view");
    let source_buffer = source_view_source_buffer(&source_view);
    assert_eq!(
        source_buffer.language().map(|language| language.id().to_string()),
        Some("lush-test".to_string()),
        "Expected the fenced language to be applied to the embedded source buffer"
    );
    assert!(
        source_buffer.is_highlight_syntax(),
        "Expected syntax highlighting to be enabled when a language resolves"
    );
}

#[test]
fn test_render_unsupported_fenced_language_falls_back_to_plain_source_view() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview(&preview);

    preview.render_markdown("```definitely-not-lush\nplain text\n```");
    wait_until(Duration::from_secs(2), || source_views(&preview).len() == 1);

    let source_view = source_views(&preview).pop().expect("source view");
    let source_buffer = source_view_source_buffer(&source_view);
    assert_eq!(
        source_view_buffer_text(&source_view),
        "plain text\n",
        "Expected unsupported languages to still render readable code text"
    );
    assert!(
        source_buffer.language().is_none(),
        "Expected unsupported language hints to fall back without a source language"
    );
    assert!(
        !source_buffer.is_highlight_syntax(),
        "Expected syntax highlighting to remain disabled for unsupported languages"
    );
}

#[test]
fn test_render_code_block_text_has_nonzero_inset() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview_with_size(&preview, 420, 260);

    preview.render_markdown("```\nlet padded = true;\n```");
    wait_until(Duration::from_secs(2), || {
        widgets_with_css_class::<gtk4::Box>(&preview, "markdown-code-block").len() == 1
            && widgets_with_css_class::<gtk4::ScrolledWindow>(
                &preview,
                "markdown-code-block-scroller",
            )
            .len()
                == 1
    });

    let block = widgets_with_css_class::<gtk4::Box>(&preview, "markdown-code-block")
        .pop()
        .expect("code block container");
    let scroller =
        widgets_with_css_class::<gtk4::ScrolledWindow>(&preview, "markdown-code-block-scroller")
            .pop()
            .expect("code block scroller");

    wait_until(Duration::from_secs(2), || {
        scroller
            .compute_bounds(&block)
            .is_some_and(|bounds| bounds.x() > 0.0 && bounds.y() > 0.0)
    });
    let bounds = scroller
        .compute_bounds(&block)
        .expect("scroller should have bounds inside code block");
    assert!(
        bounds.x() > 0.0 && bounds.y() > 0.0,
        "Expected code text scroller to be inset from block edges, got bounds {bounds:?}"
    );
}

#[test]
fn test_render_code_block_without_false_horizontal_overflow_when_line_fits() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview_with_size(&preview, 640, 260);

    preview.render_markdown("```js\nconst readable = true;\n```");
    wait_for_code_block_layout(&preview);

    let scroller = code_block_scrollers(&preview).pop().expect("code scroller");
    let overflow = horizontal_overflow(&scroller);
    assert!(
        overflow <= 1.0,
        "Expected short code to fit without horizontal overflow, got {overflow}"
    );
}

#[test]
fn test_render_code_block_allows_horizontal_overflow_for_long_line() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview_with_size(&preview, 220, 260);
    let long_line = "const veryLongIdentifier = ".to_string() + &"x".repeat(240);

    preview.render_markdown(&format!("```js\n{long_line}\n```"));
    wait_for_code_block_layout(&preview);

    let scroller = code_block_scrollers(&preview).pop().expect("code scroller");
    let overflow = horizontal_overflow(&scroller);
    assert!(
        overflow > 1.0,
        "Expected a genuinely long code line to expose horizontal overflow"
    );
}

#[test]
fn test_render_code_block_width_updates_after_late_allocation() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();

    preview.render_markdown("```\nallocated later\n```");
    let _window = present_preview_with_size(&preview, 620, 260);
    wait_for_code_block_width(&preview);

    let block = code_block_containers(&preview)
        .pop()
        .expect("code block container");
    let expected_width = preview_text_column_width(&preview);
    assert_eq!(
        block.width_request(),
        expected_width,
        "Expected code block width to refresh after the preview receives its allocation"
    );
}

#[test]
fn test_many_code_blocks_skip_unchanged_deferred_traversal_and_refresh_new_embeds() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview_with_size(&preview, 900, 700);
    let many_blocks = (0..128)
        .map(|index| format!("```rust\nlet value_{index} = {index};\n```\n"))
        .collect::<String>();
    preview.render_markdown(&many_blocks);
    wait_until(Duration::from_secs(5), || {
        code_block_containers(&preview).len() == 128
            && code_block_containers(&preview)
                .iter()
                .all(|block| block.width_request() > 0)
    });

    let settled = Rc::new(std::cell::Cell::new(false));
    let settled_for_callback = settled.clone();
    preview.queue_code_block_width_refresh_for_test(move || settled_for_callback.set(true));
    wait_until(Duration::from_secs(2), || settled.get());
    preview.reset_code_block_width_traversal_count_for_test();

    let unchanged_done = Rc::new(std::cell::Cell::new(false));
    let unchanged_done_for_callback = unchanged_done.clone();
    preview.queue_code_block_width_refresh_for_test(move || {
        unchanged_done_for_callback.set(true);
    });
    wait_until(Duration::from_secs(2), || unchanged_done.get());
    assert_eq!(
        preview.evidence().code_blocks.width_traversal_count,
        0,
        "unchanged immediate, idle, and timed passes should all use the tuple fast path"
    );

    preview.render_markdown(&(many_blocks + "```rust\nlet newest = true;\n```\n"));
    wait_until(Duration::from_secs(5), || {
        code_block_containers(&preview).len() == 129
            && code_block_containers(&preview)
                .iter()
                .all(|block| block.width_request() > 0)
    });
    assert_eq!(
        preview.evidence().code_blocks.width_traversal_count,
        1,
        "new embed membership should force one full pass, then cache deferred repeats"
    );
}

#[test]
fn test_hidden_code_blocks_preserve_invalid_cache_until_late_valid_allocation() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    preview.render_markdown("```rust\nlet allocated_later = true;\n```\n");
    assert_eq!(preview.evidence().code_blocks.width_traversal_count, 0);

    let _window = present_preview_with_size(&preview, 620, 280);
    wait_for_code_block_layout(&preview);
    assert_eq!(preview.evidence().code_blocks.width_traversal_count, 1);
}

#[test]
fn test_root_and_nested_code_blocks_repair_after_resize_at_constrained_width() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let window = present_preview_with_size(&preview, 900, 420);
    preview.render_markdown(concat!(
        "```rust\nlet root = true;\n```\n\n",
        "Term\n\n:   Definition\n\n        let nested = true;\n",
    ));
    wait_until(Duration::from_secs(2), || {
        code_block_containers(&preview).len() == 2
    });
    wait_for_code_block_layout(&preview);

    window.set_default_size(320, 420);
    wait_until(Duration::from_secs(2), || {
        preview.text_view().width() < 500
            && code_block_containers(&preview).iter().all(|block| {
                block.width_request() == expected_code_block_width(&preview, block)
            })
    });
    let blocks = code_block_containers(&preview);
    assert!(blocks.iter().any(|block| block.margin_start() == 0));
    assert!(blocks.iter().any(|block| block.margin_start() > 0));
}

#[test]
fn test_render_markdown_cleans_up_code_block_widgets_on_rerender() {
    ensure_gtk_init();
    let preview = LushtextMarkdownPreview::new();
    let _window = present_preview(&preview);

    preview.render_markdown("```\nstale\n```");
    wait_until(Duration::from_secs(2), || source_views(&preview).len() == 1);

    preview.render_markdown("# No Code");
    wait_until(Duration::from_secs(2), || source_views(&preview).is_empty());

    assert!(
        source_views(&preview).is_empty(),
        "Expected rerender without code blocks to remove the old anchored source view"
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

fn source_views(preview: &LushtextMarkdownPreview) -> Vec<sourceview5::View> {
    widgets_with_css_class::<sourceview5::View>(preview, "markdown-code-block-view")
}

fn code_block_containers(preview: &LushtextMarkdownPreview) -> Vec<gtk4::Box> {
    widgets_with_css_class::<gtk4::Box>(preview, "markdown-code-block")
}

fn code_block_scrollers(preview: &LushtextMarkdownPreview) -> Vec<gtk4::ScrolledWindow> {
    widgets_with_css_class::<gtk4::ScrolledWindow>(preview, "markdown-code-block-scroller")
}

fn preview_text_column_width(preview: &LushtextMarkdownPreview) -> i32 {
    let text_view = preview.text_view();
    text_view.width() - text_view.left_margin() - text_view.right_margin()
}

fn horizontal_overflow(scroller: &gtk4::ScrolledWindow) -> f64 {
    let adjustment = scroller.hadjustment();
    (adjustment.upper() - adjustment.page_size()).max(0.0)
}

fn expected_code_block_width(preview: &LushtextMarkdownPreview, block: &gtk4::Box) -> i32 {
    preview_text_column_width(preview)
        .saturating_sub(block.margin_start() + block.margin_end())
        .max(1)
}

fn code_block_width_is_settled(actual_width: i32, expected_width: i32) -> bool {
    (actual_width - expected_width).abs() <= 3
}

fn assert_nested_code_block_geometry(preview: &LushtextMarkdownPreview) {
    let block = code_block_containers(preview)
        .pop()
        .expect("code block container");
    let scroller = code_block_scrollers(preview).pop().expect("code scroller");
    let expected_width = expected_code_block_width(preview, &block);
    let actual_width = block.width();

    assert!(
        block.margin_start() > 0,
        "Expected nested code block to carry a visible context offset, got margin-start {}",
        block.margin_start()
    );
    assert_eq!(
        block.width_request(),
        expected_width,
        "Expected nested code block width request to account for context margins"
    );
    assert!(
        code_block_width_is_settled(actual_width, expected_width),
        "Expected nested code block allocation to settle near {expected_width}, got {actual_width}"
    );
    assert!(
        scroller.width() <= actual_width,
        "Expected code scroller to stay inside nested block allocation, block={actual_width} scroller={}",
        scroller.width()
    );
}

fn wait_for_code_block_layout(preview: &LushtextMarkdownPreview) {
    wait_until(Duration::from_secs(2), || {
        let Some(block) = code_block_containers(preview).first().cloned() else {
            return false;
        };
        let Some(scroller) = code_block_scrollers(preview).first().cloned() else {
            return false;
        };
        let column_width = preview_text_column_width(preview);
        let expected_width = expected_code_block_width(preview, &block);
        column_width > 0
            && block.width_request() == expected_width
            && code_block_width_is_settled(block.width(), expected_width)
            && scroller.width() > 0
            && scroller.hadjustment().page_size() > 0.0
    });
}

fn wait_for_code_block_width(preview: &LushtextMarkdownPreview) {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut last = None;
    while Instant::now() < deadline {
        let block = code_block_containers(preview).first().cloned();
        let column_width = preview_text_column_width(preview);
        let width_request = block.as_ref().map(gtk4::prelude::WidgetExt::width_request);
        if column_width > 0 && width_request == Some(column_width) {
            return;
        }
        last = Some((column_width, width_request));
        std::thread::sleep(Duration::from_millis(20));
        while glib::MainContext::default().iteration(false) {}
    }
    panic!("code block width did not settle; last column/request: {last:?}");
}

fn source_view_source_buffer(source_view: &sourceview5::View) -> sourceview5::Buffer {
    source_view
        .buffer()
        .downcast::<sourceview5::Buffer>()
        .expect("embedded code block should use a GtkSourceBuffer")
}

fn source_view_buffer_text(source_view: &sourceview5::View) -> String {
    let buffer = source_view.buffer();
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string()
}

fn install_test_source_language(id: &str) -> TempDir {
    let tempdir = tempfile::tempdir().expect("language tempdir");
    let language_path = tempdir.path().join(format!("{id}.lang"));
    fixture::write_text(
        &language_path,
        &format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<language id="{id}" name="Lush Test" version="2.0" _section="Sources">
  <metadata>
    <property name="mimetypes">text/x-{id}</property>
    <property name="globs">*.{id}</property>
  </metadata>
  <styles>
    <style id="keyword" name="Keyword" map-to="def:keyword"/>
  </styles>
  <definitions>
    <context id="{id}">
      <include>
        <context id="keyword" style-ref="keyword">
          <keyword>var</keyword>
        </context>
      </include>
    </context>
  </definitions>
</language>
"#
        ),
    );

    let manager = sourceview5::LanguageManager::default();
    let mut search_paths = vec![tempdir.path().to_string_lossy().to_string()];
    search_paths.extend(manager.search_path().iter().map(ToString::to_string));
    let search_path_refs = search_paths.iter().map(String::as_str).collect::<Vec<_>>();
    manager.set_search_path(&search_path_refs);
    assert!(
        manager.language(id).is_some(),
        "Expected test source language '{id}' to load from {}",
        language_path.display()
    );
    tempdir
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
