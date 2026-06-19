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

use crate::ui::accessibility;

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

/// Font scale factors for heading levels (h1=2.0x down to h6=1.0x).
const HEADING_SCALES: [f64; 6] = [2.0, 1.65, 1.35, 1.2, 1.1, 1.0];
/// Vertical space before each heading level, in text-buffer pixels.
const HEADING_PIXELS_ABOVE: [i32; 6] = [24, 20, 16, 12, 10, 8];
/// Vertical space after each heading level, in text-buffer pixels.
const HEADING_PIXELS_BELOW: [i32; 6] = [10, 8, 6, 5, 4, 4];
/// Marker-column margin for top-level list items in the preview.
const LIST_ITEM_MARKER_MARGIN: i32 = 24;
/// Space reserved between a list marker and the wrapped item text column.
const LIST_ITEM_MARKER_SLOT: i32 = 36;
/// Extra indentation applied for each additional nested list level.
const LIST_ITEM_DEPTH_STEP: i32 = 28;
/// Base left margin for generic blockquotes once the rail glyph is inserted.
const BLOCKQUOTE_BASE_MARGIN: i32 = 18;
/// Extra indentation applied for each additional nested generic blockquote.
const BLOCKQUOTE_DEPTH_STEP: i32 = 20;
/// Left inset for typed alert callout bodies.
///
/// This matches the readable card-like indent used for alert text. If it is too
/// small, alert bodies blend into surrounding prose; too large wastes preview
/// width in narrow panes.
pub(super) const ALERT_BODY_LEFT_MARGIN: i32 = 24;
/// Right inset for typed alert callout bodies.
///
/// A modest right margin keeps wrapped alert text from touching the preview
/// edge while leaving enough width for code and links.
pub(super) const ALERT_BODY_RIGHT_MARGIN: i32 = 16;
/// Left inset for rendered footnote definitions.
///
/// Footnotes use the same visual column as definition bodies so their generated
/// labels and wrapped prose stay compact but still distinct from normal text.
pub(super) const FOOTNOTE_DEF_LEFT_MARGIN: i32 = 32;
/// Right inset for rendered footnote definitions.
///
/// This mirrors other indented preview blocks to keep wrapped content off the
/// far edge without making the footnote column feel cramped.
pub(super) const FOOTNOTE_DEF_RIGHT_MARGIN: i32 = 16;
/// Left inset for rendered definition-list bodies.
///
/// Definition bodies need enough offset to read as content under a term, while
/// preserving room for nested paragraphs, lists, quotes, and code blocks.
pub(super) const DEFINITION_DEF_LEFT_MARGIN: i32 = 32;
/// Right inset for rendered definition-list bodies.
///
/// This balances the left definition offset so nested wrapped content keeps a
/// comfortable line length in both side-by-side and preview-only modes.
pub(super) const DEFINITION_DEF_RIGHT_MARGIN: i32 = 16;
/// Visible rail glyph used to replace Markdown's raw `>` source marker.
pub(super) const BLOCKQUOTE_RAIL: &str = "\u{2502}";

/// Stored override used by widget tests to observe link activation without
/// launching an external desktop handler.
type LinkActivationCallback = Box<dyn Fn(String)>;
/// One-shot callback fired after the latest deferred code-block width repair.
pub(super) type CodeBlockRefreshCompletionCallback = Box<dyn Fn()>;

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
pub(super) const TAG_DEFINITION_TERM: &str = "definition-term";
pub(super) const TAG_DEFINITION_DEF: &str = "definition-definition";

/// Returns a heading tag name for the given level (0-indexed).
pub(super) fn heading_tag_name(level_idx: usize) -> String {
    format!("heading{}", level_idx + 1)
}

/// Returns the dynamic tag name used for one list nesting depth.
pub(super) fn list_item_tag_name(depth: usize) -> String {
    format!("list-item-depth-{depth}")
}

/// Return the marker-column margin used for one list nesting depth.
pub(super) fn list_item_marker_margin(depth: usize) -> i32 {
    let extra_depth = depth.saturating_sub(1);
    let extra_margin = i32::try_from(extra_depth)
        .ok()
        .and_then(|depth| depth.checked_mul(LIST_ITEM_DEPTH_STEP))
        .unwrap_or(i32::MAX - LIST_ITEM_MARKER_MARGIN);
    LIST_ITEM_MARKER_MARGIN.saturating_add(extra_margin)
}

/// Return the wrapped-text margin used for one list nesting depth.
pub(super) fn list_item_text_margin(depth: usize) -> i32 {
    list_item_marker_margin(depth).saturating_add(LIST_ITEM_MARKER_SLOT)
}

/// Returns the dynamic tag name used for one generic blockquote depth.
pub(super) fn blockquote_depth_tag_name(depth: usize) -> String {
    format!("blockquote-depth-{depth}")
}

/// Return the left margin used for one generic blockquote depth.
pub(super) fn blockquote_left_margin(depth: usize) -> i32 {
    let extra_depth = depth.saturating_sub(1);
    let extra_margin = i32::try_from(extra_depth)
        .ok()
        .and_then(|depth| depth.checked_mul(BLOCKQUOTE_DEPTH_STEP))
        .unwrap_or(i32::MAX - BLOCKQUOTE_BASE_MARGIN);
    BLOCKQUOTE_BASE_MARGIN.saturating_add(extra_margin)
}

/// Return the visible rail prefix for one generic blockquote nesting depth.
pub(super) fn blockquote_rail_prefix(depth: usize) -> String {
    (0..depth)
        .map(|_| BLOCKQUOTE_RAIL)
        .collect::<Vec<_>>()
        .join(" ")
        + " "
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
    /// level. We keep strong refs plus the layout context captured at insertion
    /// time so `render_markdown`, `clear`, and `show_placeholder` can remove
    /// stale embeds and resize code blocks after later allocations.
    pub(super) rendered_embeds: RefCell<Vec<super::RenderedEmbed>>,
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
    /// Generation counter for deferred code-block width refreshes.
    ///
    /// Preview shell transitions can emit several allocation and margin changes
    /// before the embedded `GtkTextView` column settles. The counter lets later
    /// refresh requests supersede older idle/timer callbacks so only the most
    /// recent layout pass can resize anchored code blocks.
    pub(super) code_block_refresh_generation: Cell<u32>,
    /// Pending idle refresh for child-anchor code-block widths.
    ///
    /// Layout and render paths can request many refreshes in one GTK turn. Keep
    /// only the newest idle callback so resize storms do not queue no-op work.
    pub(super) code_block_idle_source_id: RefCell<Option<glib::SourceId>>,
    /// Pending timed refresh for child-anchor code-block widths.
    ///
    /// The timed pass catches preview shell allocations that settle just after
    /// idle callbacks while still letting newer refresh requests replace it.
    pub(super) code_block_timeout_source_id: RefCell<Option<glib::SourceId>>,
    /// One-shot callbacks waiting for the current deferred repair sequence.
    ///
    /// Window-level visual readiness uses this to avoid reporting ready before
    /// the preview's own idle and timed child-anchor repairs have run.
    pub(super) code_block_refresh_completion_callbacks:
        RefCell<Vec<CodeBlockRefreshCompletionCallback>>,
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
        self.apply_accessibility_metadata();
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

        // Code blocks are embedded as child-anchor widgets, so they need to
        // follow the final text-view column width rather than the outer box.
        for property_name in ["width", "left-margin", "right-margin"] {
            let obj_weak = self.obj().downgrade();
            self.text_view
                .connect_notify_local(Some(property_name), move |_, _| {
                    if let Some(obj) = obj_weak.upgrade() {
                        obj.queue_code_block_width_refresh();
                    }
                });
        }

        let obj_weak = self.obj().downgrade();
        self.obj().connect_map(move |_| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.queue_code_block_width_refresh();
            }
        });
    }

    fn dispose(&self) {
        if let Some(source_id) = self.code_block_idle_source_id.take() {
            source_id.remove();
        }
        if let Some(source_id) = self.code_block_timeout_source_id.take() {
            source_id.remove();
        }
        self.code_block_refresh_completion_callbacks
            .borrow_mut()
            .clear();
    }
}

impl WidgetImpl for LushtextMarkdownPreview {
    fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
        self.parent_size_allocate(width, height, baseline);
        self.obj().queue_code_block_width_refresh();
    }
}
impl BoxImpl for LushtextMarkdownPreview {}

impl LushtextMarkdownPreview {
    /// Mark the preview as a read-only document surface before any content is rendered.
    fn apply_accessibility_metadata(&self) {
        accessibility::set_role(&*self.obj(), gtk4::AccessibleRole::Document);
        accessibility::set_labelled_description(
            &*self.obj(),
            "Markdown preview",
            "Read-only rendered Markdown document preview",
        );
        accessibility::set_role(&*self.scrolled_window, gtk4::AccessibleRole::Region);
        accessibility::set_labelled_description(
            &*self.scrolled_window,
            "Markdown preview scroll area",
            "Scrollable read-only rendered Markdown content",
        );
        // GtkTextView already owns GTK_ACCESSIBLE_ROLE_TEXT_BOX; keep the
        // projection to state and names so GTK does not emit duplicate-role criticals.
        accessibility::set_labelled_description(
            &*self.text_view,
            "Rendered Markdown content",
            "Read-only rendered Markdown text",
        );
        accessibility::set_read_only(&*self.text_view, true);
        accessibility::set_multi_line(&*self.text_view, true);
        accessibility::set_role(&*self.placeholder, gtk4::AccessibleRole::Status);
        accessibility::set_labelled_description(
            &*self.placeholder,
            "Markdown preview placeholder",
            "Open a Markdown file to see a rendered preview",
        );
        accessibility::set_hidden(&*self.placeholder, true);
    }
}

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
        tag.set_pixels_above_lines(HEADING_PIXELS_ABOVE[i]);
        tag.set_pixels_below_lines(HEADING_PIXELS_BELOW[i]);
        tag.set_underline(if i < 2 {
            pango::Underline::Single
        } else {
            pango::Underline::None
        });
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

    // Blockquotes: dim color and modest paragraph spacing. Depth-specific tags
    // own indentation so nested quotes can remain visually distinct.
    let blockquote = get_or_create(TAG_BLOCKQUOTE);
    blockquote.set_foreground(Some(dim));
    blockquote.set_pixels_above_lines(2);
    blockquote.set_pixels_below_lines(2);

    // List items: depth-specific tags own list layout; this shared tag remains
    // a semantic grouping point for item-wide styling and tests.
    let list_item = get_or_create(TAG_LIST_ITEM);
    list_item.set_left_margin(0);
    list_item.set_indent(0);

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
    alert_body.set_left_margin(ALERT_BODY_LEFT_MARGIN);
    alert_body.set_right_margin(ALERT_BODY_RIGHT_MARGIN);
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
    footnote_def.set_left_margin(FOOTNOTE_DEF_LEFT_MARGIN);
    footnote_def.set_right_margin(FOOTNOTE_DEF_RIGHT_MARGIN);
    footnote_def.set_pixels_above_lines(2);
    footnote_def.set_pixels_below_lines(2);

    let footnote_def_label = get_or_create(TAG_FOOTNOTE_DEF_LABEL);
    footnote_def_label.set_family(Some("Monospace"));
    footnote_def_label.set_foreground(Some(accent));
    footnote_def_label.set_weight(pango::Weight::Bold.into_glib());

    // Definition lists are parser-native pulldown-cmark blocks. Terms need to
    // read like labels, while definitions need a stable text column that can
    // also host nested paragraphs, lists, quotes, and code anchors.
    let definition_term = get_or_create(TAG_DEFINITION_TERM);
    definition_term.set_weight(pango::Weight::Bold.into_glib());
    definition_term.set_pixels_above_lines(4);
    definition_term.set_pixels_below_lines(1);

    let definition_def = get_or_create(TAG_DEFINITION_DEF);
    definition_def.set_left_margin(DEFINITION_DEF_LEFT_MARGIN);
    definition_def.set_right_margin(DEFINITION_DEF_RIGHT_MARGIN);
    definition_def.set_pixels_above_lines(1);
    definition_def.set_pixels_below_lines(2);
}

/// Ensure the tag used for a given list nesting depth exists and return its name.
pub(super) fn ensure_list_item_depth_tag(buffer: &gtk4::TextBuffer, depth: usize) -> String {
    let name = list_item_tag_name(depth);
    let tag = if let Some(tag) = buffer.tag_table().lookup(&name) {
        tag
    } else {
        let tag = gtk4::TextTag::new(Some(&name));
        buffer.tag_table().add(&tag);
        tag
    };
    tag.set_left_margin(list_item_text_margin(depth));
    tag.set_indent(-LIST_ITEM_MARKER_SLOT);
    name
}

/// Ensure the tag used for a generic blockquote depth exists and return its name.
pub(super) fn ensure_blockquote_depth_tag(buffer: &gtk4::TextBuffer, depth: usize) -> String {
    let name = blockquote_depth_tag_name(depth);
    if buffer.tag_table().lookup(&name).is_some() {
        return name;
    }

    let tag = gtk4::TextTag::new(Some(&name));
    tag.set_left_margin(blockquote_left_margin(depth));
    buffer.tag_table().add(&tag);
    name
}
