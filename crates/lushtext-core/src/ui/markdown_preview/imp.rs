// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the Markdown preview widget.
//!
//! Contains a read-only `GtkTextView` inside a `GtkScrolledWindow` for rendered
//! Markdown output, and an `AdwStatusPage` placeholder for non-Markdown files.
//! TextTags are created in `constructed()` and updated on dark/light mode changes
//! via `StyleManager::connect_dark_notify()`.

use glib::translate::IntoGlib;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, glib, pango};
use libadwaita::prelude::*;
use std::cell::Cell;

/// Adwaita-matching accent color (blue) for headings and links.
const ACCENT_LIGHT: &str = "#1c71d8";
const ACCENT_DARK: &str = "#78aeed";

/// Adwaita-matching background for code spans and code blocks.
const CODE_BG_LIGHT: &str = "#f6f5f4";
const CODE_BG_DARK: &str = "#3d3846";

/// Adwaita-matching dim label color for blockquotes and horizontal rules.
const DIM_LIGHT: &str = "#5e5c64";
const DIM_DARK: &str = "#9a9996";

/// Font scale factors for heading levels (h1=1.6x down to h6=1.05x).
const HEADING_SCALES: [f64; 6] = [1.6, 1.4, 1.2, 1.1, 1.05, 1.0];

/// Tag names used in the TextBuffer. Keep in sync with `create_or_update_tags()`.
pub(super) const TAG_BOLD: &str = "bold";
pub(super) const TAG_ITALIC: &str = "italic";
pub(super) const TAG_STRIKETHROUGH: &str = "strikethrough";
pub(super) const TAG_CODE: &str = "code";
pub(super) const TAG_CODE_BLOCK: &str = "code-block";
pub(super) const TAG_LINK: &str = "link";
pub(super) const TAG_BLOCKQUOTE: &str = "blockquote";
pub(super) const TAG_LIST_ITEM: &str = "list-item";
pub(super) const TAG_HRULE: &str = "horizontal-rule";

/// Returns a heading tag name for the given level (0-indexed).
pub(super) fn heading_tag_name(level_idx: usize) -> String {
    format!("heading{}", level_idx + 1)
}

#[derive(CompositeTemplate, Default)]
#[template(resource = "/dev/cominotti/lushtext/ui/markdown-preview.ui")]
pub struct LushtextMarkdownPreview {
    #[template_child]
    pub text_view: TemplateChild<gtk4::TextView>,
    #[template_child]
    pub scrolled_window: TemplateChild<gtk4::ScrolledWindow>,
    #[template_child]
    pub placeholder: TemplateChild<libadwaita::StatusPage>,

    /// Whether we're currently showing the rendered content (true) or the placeholder (false).
    pub showing_content: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextMarkdownPreview {
    const NAME: &'static str = "LushtextMarkdownPreview";
    type Type = super::LushtextMarkdownPreview;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextMarkdownPreview {
    fn constructed(&self) {
        self.parent_constructed();

        // Create the initial tag table based on current theme.
        let is_dark = libadwaita::StyleManager::default().is_dark();
        create_or_update_tags(&self.text_view.buffer(), is_dark);

        // Re-create tags when the dark/light mode changes so colors stay correct.
        let obj_weak = self.obj().downgrade();
        libadwaita::StyleManager::default().connect_dark_notify(move |sm| {
            if let Some(obj) = obj_weak.upgrade() {
                create_or_update_tags(&obj.imp().text_view.buffer(), sm.is_dark());
            }
        });
    }
}

impl WidgetImpl for LushtextMarkdownPreview {}
impl BoxImpl for LushtextMarkdownPreview {}

/// Create (or update in-place) all TextTags used by the Markdown renderer.
///
/// If a tag already exists in the buffer's tag table, its color properties are
/// updated rather than recreated. This preserves any text that's already been
/// inserted with those tags — re-creating tags would orphan references.
fn create_or_update_tags(buffer: &gtk4::TextBuffer, is_dark: bool) {
    let accent = if is_dark { ACCENT_DARK } else { ACCENT_LIGHT };
    let code_bg = if is_dark { CODE_BG_DARK } else { CODE_BG_LIGHT };
    let dim = if is_dark { DIM_DARK } else { DIM_LIGHT };

    let table = buffer.tag_table();

    // Helper: get existing tag or create a new one with the given name.
    let get_or_create = |name: &str| -> gtk4::TextTag {
        if let Some(tag) = table.lookup(name) {
            tag
        } else {
            let tag = gtk4::TextTag::new(Some(name));
            table.add(&tag);
            tag
        }
    };

    // Heading tags (h1 through h6): scaled size, bold, accent foreground.
    for (i, &scale) in HEADING_SCALES.iter().enumerate() {
        let tag = get_or_create(&heading_tag_name(i));
        tag.set_scale(scale);
        tag.set_weight(pango::Weight::Bold.into_glib());
        tag.set_foreground(Some(accent));
        // Add vertical spacing above headings for visual separation.
        tag.set_pixels_above_lines(if i == 0 { 12 } else { 8 });
        tag.set_pixels_below_lines(4);
    }

    // Inline style tags.
    let bold = get_or_create(TAG_BOLD);
    bold.set_weight(pango::Weight::Bold.into_glib());

    let italic = get_or_create(TAG_ITALIC);
    italic.set_style(pango::Style::Italic);

    let strikethrough = get_or_create(TAG_STRIKETHROUGH);
    strikethrough.set_strikethrough(true);

    // Inline code: monospace with subtle background.
    let code = get_or_create(TAG_CODE);
    code.set_family(Some("Monospace"));
    code.set_background(Some(code_bg));

    // Fenced code block: monospace, full-width background, indented.
    let code_block = get_or_create(TAG_CODE_BLOCK);
    code_block.set_family(Some("Monospace"));
    code_block.set_paragraph_background(Some(code_bg));
    code_block.set_left_margin(16);
    code_block.set_right_margin(16);
    code_block.set_pixels_above_lines(4);
    code_block.set_pixels_below_lines(4);

    // Links: accent color with underline.
    let link = get_or_create(TAG_LINK);
    link.set_foreground(Some(accent));
    link.set_underline(pango::Underline::Single);

    // Blockquotes: dim color, italic, left indent.
    let blockquote = get_or_create(TAG_BLOCKQUOTE);
    blockquote.set_foreground(Some(dim));
    blockquote.set_style(pango::Style::Italic);
    blockquote.set_left_margin(24);
    blockquote.set_pixels_above_lines(2);
    blockquote.set_pixels_below_lines(2);

    // List items: left indent for bullet/number alignment.
    let list_item = get_or_create(TAG_LIST_ITEM);
    list_item.set_left_margin(24);

    // Horizontal rule: centered dim text.
    let hrule = get_or_create(TAG_HRULE);
    hrule.set_foreground(Some(dim));
    hrule.set_justification(gtk4::Justification::Center);
    hrule.set_pixels_above_lines(8);
    hrule.set_pixels_below_lines(8);
}
