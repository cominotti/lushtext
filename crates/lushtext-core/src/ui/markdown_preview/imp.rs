// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the Markdown preview widget.
//!
//! Contains a read-only `GtkTextView` inside a `GtkScrolledWindow` for rendered
//! Markdown output, and an `AdwStatusPage` placeholder for non-Markdown files.
//! TextTags are created in `constructed()` and updated on dark/light mode changes
//! via `StyleManager::connect_dark_notify()`. Tables and local image blocks are
//! embedded as anchored child widgets so the preview can stay GTK-native
//! without replacing the surrounding text-buffer renderer.

use glib::translate::IntoGlib;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, glib, pango};
use libadwaita::prelude::*;
use pulldown_cmark::BlockQuoteKind;
use std::cell::{Cell, RefCell};

/// Adwaita-matching accent color (blue) for headings and links.
const ACCENT_LIGHT: &str = "#1c71d8";
const ACCENT_DARK: &str = "#78aeed";

/// Adwaita-matching background for code spans and code blocks.
const CODE_BG_LIGHT: &str = "#f6f5f4";
const CODE_BG_DARK: &str = "#3d3846";

/// Adwaita-matching dim label color for blockquotes and horizontal rules.
const DIM_LIGHT: &str = "#5e5c64";
const DIM_DARK: &str = "#9a9996";

/// Accent-tinted background for read-only alert callouts in light mode.
const ALERT_BG_LIGHT: &str = "#f3f7ff";
/// Accent-tinted background for read-only alert callouts in dark mode.
const ALERT_BG_DARK: &str = "#263548";

/// Per-callout title colors for light mode so alert kinds stay visually distinct.
const ALERT_TITLE_NOTE_LIGHT: &str = "#1c71d8";
const ALERT_TITLE_TIP_LIGHT: &str = "#2b7a0b";
const ALERT_TITLE_IMPORTANT_LIGHT: &str = "#9141ac";
const ALERT_TITLE_WARNING_LIGHT: &str = "#c88800";
const ALERT_TITLE_CAUTION_LIGHT: &str = "#c01c28";

/// Per-callout title colors for dark mode so alert kinds stay visually distinct.
const ALERT_TITLE_NOTE_DARK: &str = "#78aeed";
const ALERT_TITLE_TIP_DARK: &str = "#57e389";
const ALERT_TITLE_IMPORTANT_DARK: &str = "#dc8add";
const ALERT_TITLE_WARNING_DARK: &str = "#f8e45c";
const ALERT_TITLE_CAUTION_DARK: &str = "#ff7b63";

/// Font scale factors for heading levels (h1=1.6x down to h6=1.05x).
const HEADING_SCALES: [f64; 6] = [1.6, 1.4, 1.2, 1.1, 1.05, 1.0];
/// Base left margin for top-level list items in the preview.
const LIST_ITEM_BASE_MARGIN: i32 = 24;
/// Extra indentation applied for each additional nested list level.
const LIST_ITEM_DEPTH_STEP: i32 = 20;

/// Stored override used by widget tests to observe link activation without
/// launching an external desktop handler.
type LinkActivationCallback = Box<dyn Fn(String)>;

/// Tag names used in the TextBuffer. Keep in sync with `create_or_update_tags()`.
pub(super) const TAG_BOLD: &str = "bold";
pub(super) const TAG_ITALIC: &str = "italic";
pub(super) const TAG_STRIKETHROUGH: &str = "strikethrough";
pub(super) const TAG_CODE: &str = "code";
pub(super) const TAG_CODE_BLOCK: &str = "code-block";
pub(super) const TAG_LINK: &str = "link";
pub(super) const TAG_BLOCKQUOTE: &str = "blockquote";
pub(super) const TAG_LIST_ITEM: &str = "list-item";
pub(super) const TAG_TASK_MARKER: &str = "task-marker";
pub(super) const TAG_HRULE: &str = "horizontal-rule";
pub(super) const TAG_ALERT_BODY: &str = "alert-body";
pub(super) const TAG_FOOTNOTE_REF: &str = "footnote-ref";
pub(super) const TAG_FOOTNOTE_DEF: &str = "footnote-def";
pub(super) const TAG_FOOTNOTE_DEF_LABEL: &str = "footnote-def-label";

/// Returns a heading tag name for the given level (0-indexed).
pub(super) fn heading_tag_name(level_idx: usize) -> String {
    format!("heading{}", level_idx + 1)
}

/// Returns the dynamic tag name used for one list nesting depth.
pub(super) fn list_item_tag_name(depth: usize) -> String {
    format!("list-item-depth-{depth}")
}

/// Return the left margin used for one list nesting depth.
pub(super) fn list_item_left_margin(depth: usize) -> i32 {
    let extra_depth = depth.saturating_sub(1);
    let extra_margin = i32::try_from(extra_depth)
        .ok()
        .and_then(|depth| depth.checked_mul(LIST_ITEM_DEPTH_STEP))
        .unwrap_or(i32::MAX - LIST_ITEM_BASE_MARGIN);
    LIST_ITEM_BASE_MARGIN.saturating_add(extra_margin)
}

/// Return the tag name used for a typed alert callout title.
pub(super) fn alert_title_tag_name(kind: BlockQuoteKind) -> &'static str {
    match kind {
        BlockQuoteKind::Note => "alert-title-note",
        BlockQuoteKind::Tip => "alert-title-tip",
        BlockQuoteKind::Important => "alert-title-important",
        BlockQuoteKind::Warning => "alert-title-warning",
        BlockQuoteKind::Caution => "alert-title-caution",
    }
}

/// Return the user-facing title inserted at the start of a typed alert callout.
pub(super) fn alert_title(kind: BlockQuoteKind) -> &'static str {
    match kind {
        BlockQuoteKind::Note => "Note",
        BlockQuoteKind::Tip => "Tip",
        BlockQuoteKind::Important => "Important",
        BlockQuoteKind::Warning => "Warning",
        BlockQuoteKind::Caution => "Caution",
    }
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
    /// Anchored widgets currently embedded into the text view.
    ///
    /// `GtkTextChildAnchor` makes tables and image blocks pleasantly native,
    /// but GTK does not manage rerender cleanup for us at the application
    /// level. We keep strong refs here so `render_markdown`, `clear`, and
    /// `show_placeholder` can remove stale embeds before rebuilding.
    pub(super) rendered_embeds: RefCell<Vec<gtk4::Widget>>,
    /// Launchable link spans rendered directly into the text buffer.
    ///
    /// The preview rerenders whole documents, so this list is rebuilt from
    /// scratch on every render and then reused by the click and hover
    /// controllers for hit-testing.
    pub(super) text_link_targets: RefCell<Vec<super::RenderedTextLink>>,
    /// Optional override used by tests to capture preview link activations
    /// without spawning an external desktop handler.
    pub(super) link_activation_callback: RefCell<Option<LinkActivationCallback>>,
    /// Current document-surface opacity used for the preview background.
    pub background_opacity: Cell<f64>,
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
        self.obj().setup_link_interaction();
        self.background_opacity.set(
            gtk4::gio::Settings::new(crate::config::APP_ID)
                .double(crate::config::keys::TAB_CONTENT_OPACITY)
                .clamp(0.0, 1.0),
        );

        {
            let settings = gtk4::gio::Settings::new(crate::config::APP_ID);
            let preview_weak = self.obj().downgrade();
            settings.connect_changed(
                Some(crate::config::keys::TAB_CONTENT_OPACITY),
                move |s, _| {
                    if let Some(preview) = preview_weak.upgrade() {
                        preview.imp().background_opacity.set(
                            s.double(crate::config::keys::TAB_CONTENT_OPACITY)
                                .clamp(0.0, 1.0),
                        );
                    }
                },
            );
        }

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
    let alert_bg = if is_dark {
        ALERT_BG_DARK
    } else {
        ALERT_BG_LIGHT
    };

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

    // List items: top-level left indent for bullet/number alignment.
    let list_item = get_or_create(TAG_LIST_ITEM);
    list_item.set_left_margin(LIST_ITEM_BASE_MARGIN);

    // Task list markers use a monospaced accent so checked and unchecked state
    // stays readable even when the surrounding item text uses proportional fonts.
    let task_marker = get_or_create(TAG_TASK_MARKER);
    task_marker.set_family(Some("Monospace"));
    task_marker.set_foreground(Some(accent));
    task_marker.set_weight(pango::Weight::Bold.into_glib());

    // Horizontal rule: centered dim text.
    let hrule = get_or_create(TAG_HRULE);
    hrule.set_foreground(Some(dim));
    hrule.set_justification(gtk4::Justification::Center);
    hrule.set_pixels_above_lines(8);
    hrule.set_pixels_below_lines(8);

    // Alert callouts stay on the text-buffer path, so the body tag provides the
    // native card-like spacing while per-kind title tags carry the alert identity.
    let alert_body = get_or_create(TAG_ALERT_BODY);
    alert_body.set_left_margin(24);
    alert_body.set_right_margin(16);
    alert_body.set_paragraph_background(Some(alert_bg));
    alert_body.set_pixels_above_lines(4);
    alert_body.set_pixels_below_lines(4);

    for (kind, light, dark) in [
        (
            BlockQuoteKind::Note,
            ALERT_TITLE_NOTE_LIGHT,
            ALERT_TITLE_NOTE_DARK,
        ),
        (
            BlockQuoteKind::Tip,
            ALERT_TITLE_TIP_LIGHT,
            ALERT_TITLE_TIP_DARK,
        ),
        (
            BlockQuoteKind::Important,
            ALERT_TITLE_IMPORTANT_LIGHT,
            ALERT_TITLE_IMPORTANT_DARK,
        ),
        (
            BlockQuoteKind::Warning,
            ALERT_TITLE_WARNING_LIGHT,
            ALERT_TITLE_WARNING_DARK,
        ),
        (
            BlockQuoteKind::Caution,
            ALERT_TITLE_CAUTION_LIGHT,
            ALERT_TITLE_CAUTION_DARK,
        ),
    ] {
        let tag = get_or_create(alert_title_tag_name(kind));
        tag.set_foreground(Some(if is_dark { dark } else { light }));
        tag.set_weight(pango::Weight::Bold.into_glib());
        tag.set_scale(1.05);
    }

    // Footnote references stay inline while definitions are rendered as compact
    // indented blocks to preserve the source document's flow in preview mode.
    let footnote_ref = get_or_create(TAG_FOOTNOTE_REF);
    footnote_ref.set_foreground(Some(accent));
    footnote_ref.set_scale(0.85);
    footnote_ref.set_weight(pango::Weight::Bold.into_glib());

    let footnote_def = get_or_create(TAG_FOOTNOTE_DEF);
    footnote_def.set_left_margin(32);
    footnote_def.set_right_margin(16);
    footnote_def.set_pixels_above_lines(2);
    footnote_def.set_pixels_below_lines(2);

    let footnote_def_label = get_or_create(TAG_FOOTNOTE_DEF_LABEL);
    footnote_def_label.set_family(Some("Monospace"));
    footnote_def_label.set_foreground(Some(accent));
    footnote_def_label.set_weight(pango::Weight::Bold.into_glib());
}

/// Ensure the tag used for a given list nesting depth exists and return its name.
pub(super) fn ensure_list_item_depth_tag(buffer: &gtk4::TextBuffer, depth: usize) -> String {
    let name = list_item_tag_name(depth);
    if buffer.tag_table().lookup(&name).is_some() {
        return name;
    }

    let tag = gtk4::TextTag::new(Some(&name));
    tag.set_left_margin(list_item_left_margin(depth));
    buffer.tag_table().add(&tag);
    name
}
