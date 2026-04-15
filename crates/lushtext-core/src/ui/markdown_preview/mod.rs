// SPDX-License-Identifier: GPL-3.0-or-later

//! Markdown preview widget — read-only rendered view of Markdown content.
//!
//! Most Markdown blocks render directly into a `GtkTextBuffer` with
//! `GtkTextTag`s so the preview stays lightweight and native. Tables and local
//! image blocks are the main exceptions: GTK already supports embedding widgets
//! inside a `GtkTextView` via `GtkTextChildAnchor`, so we use anchored GTK
//! widgets for the cases where plain styled text is not expressive enough.
//!
//! Two display states:
//! - **Content mode**: scrolled text view with rendered Markdown
//! - **Placeholder mode**: `AdwStatusPage` with "Not a Markdown file" message

mod imp;

use gio::prelude::FileExt;
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{self, gdk};
use pulldown_cmark::{Alignment, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use imp::{
    TAG_ALERT_BODY, TAG_BLOCKQUOTE, TAG_BOLD, TAG_CODE, TAG_CODE_BLOCK, TAG_FOOTNOTE_DEF,
    TAG_FOOTNOTE_DEF_LABEL, TAG_FOOTNOTE_REF, TAG_HRULE, TAG_ITALIC, TAG_LINK, TAG_LIST_ITEM,
    TAG_STRIKETHROUGH, TAG_TASK_MARKER, alert_title, alert_title_tag_name,
    ensure_list_item_depth_tag, heading_tag_name,
};

/// Maximum width for one rendered preview image before we scale it down.
///
/// The preview lives inside a `GtkTextView` child-anchor slot, so very large
/// images need a hard ceiling to avoid pushing the text flow into unusable
/// widths while still staying visibly image-like.
const MAX_PREVIEW_IMAGE_WIDTH: i32 = 640;
/// Minimum target size for tiny local images in preview.
///
/// Very small assets such as tiny badges or icons are technically valid, but
/// rendering them at source size makes them feel broken in a document preview.
/// A modest floor keeps them legible without pretending the preview is a full
/// graphics viewer.
const MIN_PREVIEW_IMAGE_SIZE: i32 = 72;

/// Extra render context supplied by the window when previewing a real Markdown file.
///
/// Relative links and images need a stable base path, and workspace-relative
/// image paths need the active sidebar roots. Keeping those inputs in one
/// value object lets the preview stay a reusable widget instead of reaching
/// back into the window shell directly.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarkdownPreviewRenderContext {
    document_path: Option<PathBuf>,
    workspace_roots: Vec<PathBuf>,
}

impl MarkdownPreviewRenderContext {
    /// Create one render context for a Markdown preview pass.
    #[must_use]
    pub fn new(document_path: Option<PathBuf>, workspace_roots: Vec<PathBuf>) -> Self {
        Self {
            document_path,
            workspace_roots,
        }
    }
}

glib::wrapper! {
    pub struct LushtextMarkdownPreview(ObjectSubclass<imp::LushtextMarkdownPreview>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

/// One launchable preview target.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PreviewLaunchTarget {
    /// URI handed to the desktop's default external launcher.
    uri: String,
    /// Absolute local path when the target resolved to a local file.
    local_path: Option<PathBuf>,
}

/// One clickable link range rendered into the text buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderedTextLink {
    /// First buffer offset that belongs to the rendered link.
    start_offset: i32,
    /// First buffer offset after the rendered link.
    end_offset: i32,
    /// Launch target associated with this rendered range.
    target: PreviewLaunchTarget,
}

/// One link tag currently open while the parser is streaming inline events.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ActiveTextLink {
    /// Buffer offset where the rendered link text started.
    start_offset: i32,
    /// Resolved target, if this Markdown destination is launchable.
    target: Option<PreviewLaunchTarget>,
    /// Whether this link pushed the preview's link text tag onto the stack.
    pushed_tag: bool,
}

/// Result of trying to resolve a local filesystem path from Markdown content.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LocalPathResolution {
    /// One unambiguous local path was found.
    Resolved(PathBuf),
    /// No matching local path exists.
    Missing,
    /// More than one workspace-relative path matched, so preview should not guess.
    Ambiguous(Vec<PathBuf>),
}

/// Result of resolving one Markdown image destination.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ResolvedImageTarget {
    /// A local file that should render as a native preview image block.
    LocalFile(PathBuf),
    /// A fallback block that should appear inline instead of silently dropping the image.
    Fallback { title: &'static str, body: String },
}

/// Buffered Markdown image collected from pulldown-cmark's event stream.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BufferedImage {
    /// Raw destination URL from the Markdown image syntax.
    destination: String,
    /// Human-readable alternative text built from the image's child events.
    alt_text: String,
}

impl BufferedImage {
    /// Start buffering one Markdown image destination and its alternative text.
    fn new(destination: &str) -> Self {
        Self {
            destination: destination.to_string(),
            alt_text: String::new(),
        }
    }

    /// Fold one event inside the image into plain alternative text.
    fn push_event(&mut self, event: Event<'_>) {
        match event {
            Event::Text(text) | Event::Code(text) => self.alt_text.push_str(&text),
            Event::SoftBreak | Event::HardBreak => self.alt_text.push(' '),
            _ => {}
        }
    }
}

/// Buffered table representation used between pulldown-cmark's streaming events
/// and the final anchored GTK widget subtree.
#[derive(Debug, Clone, PartialEq)]
struct BufferedTable {
    /// Column alignments emitted by pulldown-cmark for this table.
    alignments: Vec<Alignment>,
    /// Header and body rows in source order.
    rows: Vec<BufferedTableRow>,
}

impl BufferedTable {
    /// Return the widest row width so GTK can build a stable grid.
    fn column_count(&self) -> usize {
        self.rows
            .iter()
            .map(|row| row.cells.len())
            .max()
            .unwrap_or(self.alignments.len())
            .max(1)
    }

    /// Count the header rows at the start of the table so we can place one
    /// separator after the header section instead of between every row.
    fn header_row_count(&self) -> usize {
        self.rows.iter().take_while(|row| row.is_header).count()
    }
}

/// One buffered Markdown table row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BufferedTableRow {
    /// Header rows get stronger styling and a section separator.
    is_header: bool,
    /// Cells in source order.
    cells: Vec<BufferedTableCell>,
}

/// One buffered Markdown table cell.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BufferedTableCell {
    /// Pango markup string applied to the `GtkLabel` for this cell.
    markup: String,
}

/// Mutable builder that accumulates table rows and cells while pulldown-cmark
/// is inside one `Tag::Table(..)` block.
#[derive(Debug, Default)]
struct BufferedTableBuilder {
    /// Column alignments emitted by pulldown-cmark for this table.
    alignments: Vec<Alignment>,
    /// Render context reused by the inline builders for this table.
    render_context: MarkdownPreviewRenderContext,
    /// Completed header and body rows.
    rows: Vec<BufferedTableRow>,
    /// Whether subsequent rows are still part of the header section.
    in_header: bool,
    /// The row currently being assembled.
    current_row: Option<BufferedTableRow>,
    /// The cell currently receiving inline content.
    current_cell: Option<TableCellMarkupBuilder>,
}

impl BufferedTableBuilder {
    /// Start buffering one Markdown table.
    fn new(alignments: Vec<Alignment>, render_context: MarkdownPreviewRenderContext) -> Self {
        Self {
            alignments,
            render_context,
            in_header: false,
            ..Self::default()
        }
    }

    /// Fold one event from inside the active table into the buffered model.
    fn push_event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(Tag::TableHead) => {
                self.in_header = true;
                self.current_row = Some(BufferedTableRow {
                    is_header: true,
                    cells: Vec::new(),
                });
            }
            Event::End(TagEnd::TableHead) => {
                if let Some(row) = self.current_row.take() {
                    self.rows.push(row);
                }
                self.in_header = false;
            }
            Event::Start(Tag::TableRow) => {
                self.current_row = Some(BufferedTableRow {
                    is_header: self.in_header,
                    cells: Vec::new(),
                });
            }
            Event::End(TagEnd::TableRow) => {
                if let Some(row) = self.current_row.take() {
                    self.rows.push(row);
                }
            }
            Event::Start(Tag::TableCell) => {
                self.current_cell = Some(TableCellMarkupBuilder::new(self.render_context.clone()));
            }
            Event::End(TagEnd::TableCell) => {
                if let (Some(row), Some(cell)) =
                    (self.current_row.as_mut(), self.current_cell.take())
                {
                    row.cells.push(BufferedTableCell {
                        markup: cell.finish(),
                    });
                }
            }
            other => {
                if let Some(cell) = &mut self.current_cell {
                    cell.push_event(other);
                }
            }
        }
    }

    /// Finish buffering and return the final immutable table model.
    fn finish(mut self) -> BufferedTable {
        // pulldown-cmark should close rows before the table ends, but the
        // fallback keeps malformed edge cases from silently dropping content.
        if let Some(cell) = self.current_cell.take()
            && let Some(row) = &mut self.current_row
        {
            row.cells.push(BufferedTableCell {
                markup: cell.finish(),
            });
        }
        if let Some(row) = self.current_row.take() {
            self.rows.push(row);
        }
        BufferedTable {
            alignments: self.alignments,
            rows: self.rows,
        }
    }
}

/// Inline markup subset supported inside table cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableCellStyle {
    Bold,
    Italic,
    Strikethrough,
    Link,
}

impl TableCellStyle {
    /// Opening Pango markup tag for this style.
    fn open_tag(self, href: Option<&str>) -> String {
        match self {
            Self::Bold => "<b>".to_string(),
            Self::Italic => "<i>".to_string(),
            Self::Strikethrough => "<s>".to_string(),
            Self::Link => format!(
                "<a href=\"{}\">",
                glib::markup_escape_text(href.expect("table-cell link href should exist"))
            ),
        }
    }

    /// Closing Pango markup tag for this style.
    fn close_tag(self) -> &'static str {
        match self {
            Self::Bold => "</b>",
            Self::Italic => "</i>",
            Self::Strikethrough => "</s>",
            Self::Link => "</a>",
        }
    }
}

/// Markup builder for one buffered table cell.
#[derive(Debug)]
struct TableCellMarkupBuilder {
    /// Accumulated Pango markup.
    markup: String,
    /// Render context used to resolve relative table-cell links.
    render_context: MarkdownPreviewRenderContext,
    /// Nested inline styles that need closing tags in reverse order.
    style_stack: Vec<TableCellStyle>,
    /// Href values associated with currently open link spans.
    link_href_stack: Vec<String>,
}

impl TableCellMarkupBuilder {
    /// Create one table-cell markup builder for the current preview context.
    fn new(render_context: MarkdownPreviewRenderContext) -> Self {
        Self {
            markup: String::new(),
            render_context,
            style_stack: Vec::new(),
            link_href_stack: Vec::new(),
        }
    }

    /// Fold one pulldown-cmark event into the limited markup subset we support
    /// for `GtkLabel` table cells.
    fn push_event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(Tag::Strong) => self.push_style(TableCellStyle::Bold),
            Event::End(TagEnd::Strong) => self.pop_style(TableCellStyle::Bold),
            Event::Start(Tag::Emphasis) => self.push_style(TableCellStyle::Italic),
            Event::End(TagEnd::Emphasis) => self.pop_style(TableCellStyle::Italic),
            Event::Start(Tag::Strikethrough) => self.push_style(TableCellStyle::Strikethrough),
            Event::End(TagEnd::Strikethrough) => self.pop_style(TableCellStyle::Strikethrough),
            Event::Start(Tag::Link { dest_url, .. }) => {
                if let Some(target) = resolve_link_target(dest_url.as_ref(), &self.render_context) {
                    self.push_link(target.uri);
                }
            }
            Event::End(TagEnd::Link) => self.pop_style(TableCellStyle::Link),
            Event::Text(text) => self.push_text(&text),
            Event::Code(code) => {
                self.markup.push_str("<tt>");
                self.push_text(&code);
                self.markup.push_str("</tt>");
            }
            Event::SoftBreak => self.markup.push(' '),
            Event::HardBreak => self.markup.push('\n'),
            _ => {}
        }
    }

    /// Finish the cell markup and close any still-open inline tags so GTK sees
    /// valid Pango markup even if the source was malformed.
    fn finish(mut self) -> String {
        while let Some(style) = self.style_stack.pop() {
            self.markup.push_str(style.close_tag());
            if matches!(style, TableCellStyle::Link) {
                self.link_href_stack.pop();
            }
        }
        self.markup
    }

    /// Push an inline style start marker.
    fn push_style(&mut self, style: TableCellStyle) {
        let href = if matches!(style, TableCellStyle::Link) {
            self.link_href_stack.last().map(String::as_str)
        } else {
            None
        };
        self.markup.push_str(&style.open_tag(href));
        self.style_stack.push(style);
    }

    /// Close the matching style if the parser says that nested span ended.
    fn pop_style(&mut self, style: TableCellStyle) {
        if self.style_stack.last() == Some(&style) {
            self.style_stack.pop();
            self.markup.push_str(style.close_tag());
            if matches!(style, TableCellStyle::Link) {
                self.link_href_stack.pop();
            }
        }
    }

    /// Push one resolved link target into the current table cell.
    fn push_link(&mut self, href: String) {
        self.link_href_stack.push(href);
        self.push_style(TableCellStyle::Link);
    }

    /// Escape literal cell text so it is safe to pass into `GtkLabel::set_markup`.
    fn push_text(&mut self, text: &str) {
        self.markup.push_str(&glib::markup_escape_text(text));
    }
}

impl LushtextMarkdownPreview {
    #[must_use]
    pub fn new() -> Self {
        Object::builder().build()
    }

    /// Render Markdown content with no filesystem context.
    ///
    /// This keeps existing tests and call sites lightweight while the richer
    /// `render_markdown_with_context` path handles relative links and images
    /// for real Markdown files opened from the window shell.
    pub fn render_markdown(&self, markdown: &str) {
        self.render_markdown_with_context(markdown, &MarkdownPreviewRenderContext::default());
    }

    /// Render Markdown content into the text view, replacing any previous content.
    ///
    /// Switches to content mode (text view visible, placeholder hidden). The
    /// renderer walks pulldown-cmark's event stream and maps text blocks to
    /// `GtkTextTag`s while buffering tables and image blocks into anchored GTK
    /// widgets when plain text is not expressive enough.
    ///
    /// # Panics
    ///
    /// Panics if the parser reports the end of a buffered table or image while
    /// the internal buffered state is missing.
    pub fn render_markdown_with_context(
        &self,
        markdown: &str,
        context: &MarkdownPreviewRenderContext,
    ) {
        self.show_content_view();
        self.clear_rendered_state();

        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_TASKLISTS);
        options.insert(Options::ENABLE_FOOTNOTES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_GFM);
        let parser = Parser::new_ext(markdown, options);
        let mut iter = buffer.end_iter();

        // Tag stack: tracks which TextTag names are currently active.
        // When we insert text, all tags in the stack are applied.
        let mut tag_stack: Vec<String> = Vec::new();

        // Track list nesting: None = not in a list, Some(None) = unordered,
        // Some(Some(n)) = ordered starting at n.
        let mut list_stack: Vec<Option<u64>> = Vec::new();
        // List markers need one event of lookahead because task list state
        // arrives after `Tag::Item`; delay insertion until real item content.
        let mut pending_list_prefix: Option<String> = None;
        // Keep track of launchable text-buffer links so click and hover
        // controllers can resolve them after the render is complete.
        let mut active_text_links: Vec<ActiveTextLink> = Vec::new();

        // Track whether we need a paragraph separator before the next block.
        let mut needs_block_separator = false;

        // Tables need one complete buffered pass before GTK can lay out rows and
        // columns correctly, so we accumulate them separately from text blocks.
        let mut active_table: Option<BufferedTableBuilder> = None;
        // Images become anchored GTK widgets, so we buffer their alt text until
        // pulldown-cmark closes the image span.
        let mut active_image: Option<BufferedImage> = None;
        // Footnote numbering stays local to the preview render so references and
        // definitions can agree on a stable ordinal without a second parse pass.
        let mut footnote_numbers: HashMap<String, usize> = HashMap::new();
        let mut next_footnote_number = 1usize;

        for event in parser {
            if let Some(table) = &mut active_table {
                match event {
                    Event::End(TagEnd::Table) => {
                        let table = active_table.take().expect("active table should exist");
                        let table = table.finish();
                        self.insert_table_widget(&buffer, &mut iter, &table);
                        buffer.insert(&mut iter, "\n");
                        needs_block_separator = true;
                    }
                    other => table.push_event(other),
                }
                continue;
            }

            if let Some(image) = &mut active_image {
                match event {
                    Event::End(TagEnd::Image) => {
                        let image = active_image.take().expect("active image should exist");
                        self.insert_image_widget(&buffer, &mut iter, &image, context);
                        buffer.insert(&mut iter, "\n");
                        needs_block_separator = true;
                    }
                    other => image.push_event(other),
                }
                continue;
            }

            if pending_list_prefix.is_some() && should_flush_pending_list_prefix(&event) {
                flush_pending_list_prefix(&buffer, &mut iter, &tag_stack, &mut pending_list_prefix);
            }

            match event {
                Event::Start(Tag::Table(alignments)) => {
                    if needs_block_separator {
                        buffer.insert(&mut iter, "\n");
                    }
                    active_table = Some(BufferedTableBuilder::new(alignments, context.clone()));
                    needs_block_separator = false;
                }
                Event::Start(tag) => match tag {
                    Tag::Heading { level, .. } => {
                        if needs_block_separator {
                            buffer.insert(&mut iter, "\n");
                        }
                        let idx = heading_level_to_index(level);
                        tag_stack.push(heading_tag_name(idx));
                        needs_block_separator = false;
                    }
                    Tag::Paragraph => {
                        if needs_block_separator {
                            buffer.insert(&mut iter, "\n");
                        }
                        needs_block_separator = false;
                    }
                    Tag::BlockQuote(kind) => {
                        if needs_block_separator {
                            buffer.insert(&mut iter, "\n");
                        }
                        if let Some(kind) = kind {
                            let mut title_tags: Vec<&str> =
                                tag_stack.iter().map(std::string::String::as_str).collect();
                            title_tags.push(TAG_ALERT_BODY);
                            title_tags.push(alert_title_tag_name(kind));
                            insert_with_tags(
                                &buffer,
                                &mut iter,
                                &format!("{}\n", alert_title(kind)),
                                &title_tags,
                            );
                            tag_stack.push(TAG_ALERT_BODY.to_string());
                        } else {
                            tag_stack.push(TAG_BLOCKQUOTE.to_string());
                        }
                        needs_block_separator = false;
                    }
                    Tag::CodeBlock(_kind) => {
                        if needs_block_separator {
                            buffer.insert(&mut iter, "\n");
                        }
                        tag_stack.push(TAG_CODE_BLOCK.to_string());
                        needs_block_separator = false;
                    }
                    Tag::List(start_num) => {
                        if needs_block_separator {
                            buffer.insert(&mut iter, "\n");
                        }
                        list_stack.push(start_num);
                        needs_block_separator = false;
                    }
                    Tag::Item => {
                        pending_list_prefix = Some(match list_stack.last() {
                            Some(Some(start)) => format!("{start}. "),
                            _ => "\u{2022} ".to_string(),
                        });
                        let depth_tag =
                            ensure_list_item_depth_tag(&buffer, list_stack.len().max(1));
                        tag_stack.push(TAG_LIST_ITEM.to_string());
                        tag_stack.push(depth_tag);
                    }
                    Tag::FootnoteDefinition(label) => {
                        if needs_block_separator {
                            buffer.insert(&mut iter, "\n");
                        }
                        tag_stack.push(TAG_FOOTNOTE_DEF.to_string());
                        let number = footnote_number(
                            &mut footnote_numbers,
                            &mut next_footnote_number,
                            label.as_ref(),
                        );
                        let mut tags: Vec<&str> =
                            tag_stack.iter().map(std::string::String::as_str).collect();
                        tags.push(TAG_FOOTNOTE_DEF_LABEL);
                        insert_with_tags(&buffer, &mut iter, &format!("[{number}] "), &tags);
                        needs_block_separator = false;
                    }
                    Tag::Emphasis => tag_stack.push(TAG_ITALIC.to_string()),
                    Tag::Strong => tag_stack.push(TAG_BOLD.to_string()),
                    Tag::Strikethrough => tag_stack.push(TAG_STRIKETHROUGH.to_string()),
                    Tag::Link { dest_url, .. } => {
                        let target = resolve_link_target(dest_url.as_ref(), context);
                        let pushed_tag = target.is_some();
                        if pushed_tag {
                            tag_stack.push(TAG_LINK.to_string());
                        }
                        active_text_links.push(ActiveTextLink {
                            start_offset: iter.offset(),
                            target,
                            pushed_tag,
                        });
                    }
                    Tag::Image { dest_url, .. } => {
                        if needs_block_separator || (!iter.starts_line() && iter.offset() > 0) {
                            buffer.insert(&mut iter, "\n");
                        }
                        active_image = Some(BufferedImage::new(dest_url.as_ref()));
                        needs_block_separator = false;
                    }
                    // Skip elements we don't render natively (HTML, math, metadata, etc.).
                    _ => {}
                },
                Event::End(tag_end) => match tag_end {
                    TagEnd::Heading(_) => {
                        pop_tag(&mut tag_stack);
                        buffer.insert(&mut iter, "\n");
                        needs_block_separator = true;
                    }
                    TagEnd::Paragraph => {
                        buffer.insert(&mut iter, "\n");
                        needs_block_separator = true;
                    }
                    TagEnd::BlockQuote(_) | TagEnd::FootnoteDefinition => {
                        pop_tag(&mut tag_stack);
                        needs_block_separator = true;
                    }
                    TagEnd::CodeBlock => {
                        let tags: Vec<&str> =
                            tag_stack.iter().map(std::string::String::as_str).collect();
                        insert_with_tags(&buffer, &mut iter, "\n", &tags);
                        pop_tag(&mut tag_stack);
                        needs_block_separator = true;
                    }
                    TagEnd::List(_) => {
                        list_stack.pop();
                        needs_block_separator = true;
                    }
                    TagEnd::Item => {
                        flush_pending_list_prefix(
                            &buffer,
                            &mut iter,
                            &tag_stack,
                            &mut pending_list_prefix,
                        );
                        pop_tag(&mut tag_stack);
                        pop_tag(&mut tag_stack);
                        buffer.insert(&mut iter, "\n");
                        if let Some(Some(n)) = list_stack.last_mut() {
                            *n += 1;
                        }
                    }
                    TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                        pop_tag(&mut tag_stack);
                    }
                    TagEnd::Link => {
                        if let Some(link) = active_text_links.pop() {
                            if link.pushed_tag {
                                pop_tag(&mut tag_stack);
                            }
                            if let Some(target) = link.target
                                && link.start_offset < iter.offset()
                            {
                                imp.text_link_targets.borrow_mut().push(RenderedTextLink {
                                    start_offset: link.start_offset,
                                    end_offset: iter.offset(),
                                    target,
                                });
                            }
                        }
                    }
                    _ => {}
                },
                Event::Text(text) => {
                    let tags: Vec<&str> =
                        tag_stack.iter().map(std::string::String::as_str).collect();
                    insert_with_tags(&buffer, &mut iter, &text, &tags);
                }
                Event::Code(code) => {
                    let mut tags: Vec<&str> =
                        tag_stack.iter().map(std::string::String::as_str).collect();
                    tags.push(TAG_CODE);
                    insert_with_tags(&buffer, &mut iter, &code, &tags);
                }
                Event::FootnoteReference(label) => {
                    let number = footnote_number(
                        &mut footnote_numbers,
                        &mut next_footnote_number,
                        label.as_ref(),
                    );
                    let mut tags: Vec<&str> =
                        tag_stack.iter().map(std::string::String::as_str).collect();
                    tags.push(TAG_FOOTNOTE_REF);
                    insert_with_tags(&buffer, &mut iter, &format!("[{number}]"), &tags);
                }
                Event::TaskListMarker(checked) => {
                    insert_task_list_marker(
                        &buffer,
                        &mut iter,
                        &tag_stack,
                        &mut pending_list_prefix,
                        checked,
                    );
                }
                Event::SoftBreak => {
                    buffer.insert(&mut iter, " ");
                }
                Event::HardBreak => {
                    buffer.insert(&mut iter, "\n");
                }
                Event::Rule => {
                    if needs_block_separator {
                        buffer.insert(&mut iter, "\n");
                    }
                    insert_with_tags(
                        &buffer,
                        &mut iter,
                        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
                        &[TAG_HRULE],
                    );
                    buffer.insert(&mut iter, "\n");
                    needs_block_separator = true;
                }
                // Skip HTML, math, images, and metadata — out of scope for native rendering.
                _ => {}
            }
        }
    }

    /// Register one callback that overrides the default external link launcher.
    ///
    /// The production window leaves this unset so preview links open through
    /// the desktop's default handler. Widget tests install a callback instead
    /// so they can assert which URI would have been launched.
    pub fn connect_link_activated<F: Fn(&str) + 'static>(&self, f: F) {
        self.imp()
            .link_activation_callback
            .replace(Some(Box::new(move |uri| f(&uri))));
    }

    /// Return the internal `GtkTextView` used for rendered preview content.
    ///
    /// Tests use this to inspect controllers, coordinates, and anchored child
    /// widgets through normal GTK APIs.
    #[must_use]
    pub fn text_view(&self) -> gtk4::TextView {
        self.imp().text_view.get()
    }

    /// Install click and hover controllers for launchable text-buffer links.
    fn setup_link_interaction(&self) {
        let click = gtk4::GestureClick::new();
        let obj_weak = self.downgrade();
        click.connect_pressed(move |_, _press_count, x, y| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.activate_link_at_view_position(x, y);
            }
        });
        self.imp().text_view.add_controller(click);

        let motion = gtk4::EventControllerMotion::new();
        let obj_weak = self.downgrade();
        motion.connect_motion(move |_, x, y| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.update_link_cursor(x, y);
            }
        });
        let obj_weak = self.downgrade();
        motion.connect_leave(move |_| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.imp().text_view.set_cursor_from_name(None);
            }
        });
        self.imp().text_view.add_controller(motion);
    }

    /// Try to activate the rendered link at one text-view position.
    fn activate_link_at_view_position(&self, x: f64, y: f64) -> bool {
        let Some(target) = self.link_target_at_view_position(x, y) else {
            return false;
        };
        self.activate_link_target(&target)
    }

    /// Update the pointer cursor based on whether the current position is clickable.
    fn update_link_cursor(&self, x: f64, y: f64) {
        let cursor_name = if self.link_target_at_view_position(x, y).is_some() {
            Some("pointer")
        } else {
            None
        };
        self.imp().text_view.set_cursor_from_name(cursor_name);
    }

    /// Resolve one rendered text link from widget-local coordinates.
    fn link_target_at_view_position(&self, x: f64, y: f64) -> Option<PreviewLaunchTarget> {
        let text_view = self.imp().text_view.get();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Preview hit-testing uses GTK widget coordinates, which fit within i32 here."
        )]
        let (buffer_x, buffer_y) = text_view.window_to_buffer_coords(
            gtk4::TextWindowType::Widget,
            x.round() as i32,
            y.round() as i32,
        );
        let iter = text_view.iter_at_location(buffer_x, buffer_y)?;
        self.link_target_at_buffer_offset(iter.offset())
    }

    /// Resolve one launchable text link from a buffer offset.
    fn link_target_at_buffer_offset(&self, offset: i32) -> Option<PreviewLaunchTarget> {
        self.imp()
            .text_link_targets
            .borrow()
            .iter()
            .find(|link| offset >= link.start_offset && offset < link.end_offset)
            .map(|link| link.target.clone())
    }

    /// Launch one previously resolved preview target.
    fn activate_link_target(&self, target: &PreviewLaunchTarget) -> bool {
        if let Some(callback) = self.imp().link_activation_callback.borrow().as_ref() {
            callback(target.uri.clone());
            return true;
        }

        match gio::AppInfo::launch_default_for_uri(
            &target.uri,
            Option::<&gio::AppLaunchContext>::None,
        ) {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!("failed to launch preview link '{}': {error}", target.uri);
                false
            }
        }
    }

    /// Clear the rendered content and show the placeholder for non-Markdown files.
    pub fn show_placeholder(&self, description: &str) {
        let imp = self.imp();
        imp.placeholder.set_description(Some(description));
        imp.scrolled_window.set_visible(false);
        imp.placeholder.set_visible(true);
        self.clear_rendered_state();
        imp.showing_content.set(false);
    }

    /// Clear the rendered content without showing the placeholder.
    pub fn clear(&self) {
        self.clear_rendered_state();
    }

    /// Whether the widget is currently showing rendered Markdown content.
    #[must_use]
    pub fn is_showing_content(&self) -> bool {
        self.imp().showing_content.get()
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
    fn show_content_view(&self) {
        let imp = self.imp();
        if !imp.showing_content.get() {
            imp.scrolled_window.set_visible(true);
            imp.placeholder.set_visible(false);
            imp.showing_content.set(true);
        }
    }

    /// Remove any previously anchored widgets and clear the backing text buffer.
    fn clear_rendered_state(&self) {
        let imp = self.imp();
        {
            let mut rendered_embeds = imp.rendered_embeds.borrow_mut();
            for widget in rendered_embeds.drain(..) {
                imp.text_view.remove(&widget);
            }
        }
        imp.text_link_targets.borrow_mut().clear();
        imp.text_view.buffer().set_text("");
    }

    /// Insert one buffered table as a native GTK grid anchored into the text flow.
    fn insert_table_widget(
        &self,
        buffer: &gtk4::TextBuffer,
        iter: &mut gtk4::TextIter,
        table: &BufferedTable,
    ) {
        let grid = build_table_grid(self, table);
        self.insert_embedded_widget(buffer, iter, grid.upcast_ref::<gtk4::Widget>());
    }

    /// Insert one buffered Markdown image into the preview flow.
    fn insert_image_widget(
        &self,
        buffer: &gtk4::TextBuffer,
        iter: &mut gtk4::TextIter,
        image: &BufferedImage,
        context: &MarkdownPreviewRenderContext,
    ) {
        match resolve_image_target(&image.destination, context) {
            ResolvedImageTarget::LocalFile(path) => match build_image_paintable(&path) {
                Ok(paintable) => buffer.insert_paintable(iter, &paintable),
                Err(error) => {
                    let widget = build_image_fallback_widget(
                        "Image could not be loaded",
                        &format!("{}\n{error}", path.display()),
                    );
                    self.insert_embedded_widget(buffer, iter, widget.upcast_ref::<gtk4::Widget>());
                }
            },
            ResolvedImageTarget::Fallback { title, body } => {
                let widget = build_image_fallback_widget(title, &body);
                self.insert_embedded_widget(buffer, iter, widget.upcast_ref::<gtk4::Widget>());
            }
        }
    }

    /// Insert one already-built GTK widget into the preview text flow.
    fn insert_embedded_widget(
        &self,
        buffer: &gtk4::TextBuffer,
        iter: &mut gtk4::TextIter,
        widget: &gtk4::Widget,
    ) {
        let anchor = buffer.create_child_anchor(iter);
        self.imp().text_view.add_child_at_anchor(widget, &anchor);
        self.imp().rendered_embeds.borrow_mut().push(widget.clone());
    }
}

impl Default for LushtextMarkdownPreview {
    fn default() -> Self {
        Self::new()
    }
}

/// Insert text at the given iter with the specified tag names applied.
fn insert_with_tags(
    buffer: &gtk4::TextBuffer,
    iter: &mut gtk4::TextIter,
    text: &str,
    tag_names: &[&str],
) {
    if tag_names.is_empty() {
        buffer.insert(iter, text);
        return;
    }

    let start_offset = iter.offset();
    buffer.insert(iter, text);
    let start = buffer.iter_at_offset(start_offset);

    for name in tag_names {
        if let Some(tag) = buffer.tag_table().lookup(name) {
            buffer.apply_tag(&tag, &start, iter);
        }
    }
}

/// Return whether the current event should force any delayed list marker to be
/// inserted before the renderer processes the event itself.
fn should_flush_pending_list_prefix(event: &Event<'_>) -> bool {
    !matches!(event, Event::TaskListMarker(_) | Event::End(TagEnd::Item))
}

/// Insert a delayed list marker using whatever formatting tags are active for
/// the current list item.
fn flush_pending_list_prefix(
    buffer: &gtk4::TextBuffer,
    iter: &mut gtk4::TextIter,
    tag_stack: &[String],
    pending_list_prefix: &mut Option<String>,
) {
    let Some(prefix) = pending_list_prefix.take() else {
        return;
    };

    let tags: Vec<&str> = tag_stack.iter().map(std::string::String::as_str).collect();
    insert_with_tags(buffer, iter, &prefix, &tags);
}

/// Insert the checked or unchecked marker for a task list item and clear the
/// delayed default bullet/number prefix for that item.
fn insert_task_list_marker(
    buffer: &gtk4::TextBuffer,
    iter: &mut gtk4::TextIter,
    tag_stack: &[String],
    pending_list_prefix: &mut Option<String>,
    checked: bool,
) {
    pending_list_prefix.take();

    let mut tags: Vec<&str> = tag_stack.iter().map(std::string::String::as_str).collect();
    tags.push(TAG_TASK_MARKER);
    let marker = if checked { "\u{2611} " } else { "\u{2610} " };
    insert_with_tags(buffer, iter, marker, &tags);
}

/// Resolve one Markdown link destination into a launchable target, if possible.
fn resolve_link_target(
    raw_target: &str,
    context: &MarkdownPreviewRenderContext,
) -> Option<PreviewLaunchTarget> {
    if raw_target.trim().is_empty() {
        return None;
    }

    if let Some(scheme) = glib::Uri::parse_scheme(raw_target) {
        if scheme.as_str() == "file" {
            let file = gio::File::for_uri(raw_target);
            let path = file.path()?;
            if path.exists() {
                return Some(PreviewLaunchTarget {
                    uri: raw_target.to_string(),
                    local_path: Some(path),
                });
            }
            return None;
        }

        return Some(PreviewLaunchTarget {
            uri: raw_target.to_string(),
            local_path: None,
        });
    }

    match resolve_local_path(raw_target, context) {
        LocalPathResolution::Resolved(path) => Some(PreviewLaunchTarget {
            uri: gio::File::for_path(&path).uri().to_string(),
            local_path: Some(path),
        }),
        LocalPathResolution::Missing | LocalPathResolution::Ambiguous(_) => None,
    }
}

/// Resolve one Markdown image destination into a local image or an explicit fallback.
fn resolve_image_target(
    raw_target: &str,
    context: &MarkdownPreviewRenderContext,
) -> ResolvedImageTarget {
    if raw_target.trim().is_empty() {
        return ResolvedImageTarget::Fallback {
            title: "Image path missing",
            body: "Markdown image syntax did not include a usable destination.".to_string(),
        };
    }

    if let Some(scheme) = glib::Uri::parse_scheme(raw_target) {
        if scheme.as_str() == "file" {
            let file = gio::File::for_uri(raw_target);
            return match file.path() {
                Some(path) if path.exists() => ResolvedImageTarget::LocalFile(path),
                _ => ResolvedImageTarget::Fallback {
                    title: "Image file not found",
                    body: raw_target.to_string(),
                },
            };
        }

        return ResolvedImageTarget::Fallback {
            title: "Remote images are not supported",
            body: raw_target.to_string(),
        };
    }

    match resolve_local_path(raw_target, context) {
        LocalPathResolution::Resolved(path) => ResolvedImageTarget::LocalFile(path),
        LocalPathResolution::Missing => ResolvedImageTarget::Fallback {
            title: "Image file not found",
            body: raw_target.to_string(),
        },
        LocalPathResolution::Ambiguous(paths) => ResolvedImageTarget::Fallback {
            title: "Image path is ambiguous",
            body: paths
                .into_iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        },
    }
}

/// Resolve one Markdown local path against the current document and workspace roots.
fn resolve_local_path(
    raw_target: &str,
    context: &MarkdownPreviewRenderContext,
) -> LocalPathResolution {
    let path = Path::new(raw_target);
    if path.is_absolute() {
        return if path.exists() {
            LocalPathResolution::Resolved(path.to_path_buf())
        } else {
            LocalPathResolution::Missing
        };
    }

    if let Some(document_path) = &context.document_path
        && let Some(parent) = document_path.parent()
    {
        let candidate = parent.join(path);
        if candidate.exists() {
            return LocalPathResolution::Resolved(candidate);
        }
    }

    let matches = context
        .workspace_roots
        .iter()
        .map(|root| root.join(path))
        .filter(|candidate| candidate.exists())
        .collect::<Vec<_>>();

    match matches.len() {
        0 => LocalPathResolution::Missing,
        1 => LocalPathResolution::Resolved(matches.into_iter().next().expect("one match exists")),
        _ => LocalPathResolution::Ambiguous(matches),
    }
}

/// Assign or look up the stable preview-local number for one footnote label.
fn footnote_number(
    footnote_numbers: &mut HashMap<String, usize>,
    next_footnote_number: &mut usize,
    label: &str,
) -> usize {
    if let Some(number) = footnote_numbers.get(label) {
        return *number;
    }

    let number = *next_footnote_number;
    *next_footnote_number += 1;
    footnote_numbers.insert(label.to_string(), number);
    number
}

/// Build the anchored `GtkGrid` used for one rendered Markdown table.
fn build_table_grid(preview: &LushtextMarkdownPreview, table: &BufferedTable) -> gtk4::Grid {
    let grid = gtk4::Grid::builder()
        .column_spacing(8)
        .row_spacing(6)
        .margin_top(8)
        .margin_bottom(8)
        .build();
    grid.set_hexpand(true);
    grid.set_halign(gtk4::Align::Fill);
    grid.add_css_class("markdown-table");

    let column_count = table.column_count();
    let header_rows = table.header_row_count();
    let mut grid_row = 0usize;

    for (row_index, row) in table.rows.iter().enumerate() {
        for column_index in 0..column_count {
            let markup = row
                .cells
                .get(column_index)
                .map_or("", |cell| cell.markup.as_str());
            let alignment = table
                .alignments
                .get(column_index)
                .copied()
                .unwrap_or(Alignment::None);
            let label = build_table_cell_label(preview, markup, row.is_header, alignment);
            grid.attach(
                &label,
                usize_to_i32(column_index),
                usize_to_i32(grid_row),
                1,
                1,
            );
        }

        grid_row += 1;

        if header_rows > 0 && row_index + 1 == header_rows {
            let separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
            separator.add_css_class("markdown-table-header-separator");
            grid.attach(
                &separator,
                0,
                usize_to_i32(grid_row),
                usize_to_i32(column_count),
                1,
            );
            grid_row += 1;
        }
    }

    grid
}

/// Build one `GtkLabel` cell for the anchored Markdown table grid.
fn build_table_cell_label(
    preview: &LushtextMarkdownPreview,
    markup: &str,
    is_header: bool,
    alignment: Alignment,
) -> gtk4::Label {
    let label = gtk4::Label::new(None);
    label.set_hexpand(true);
    label.set_halign(gtk4::Align::Fill);
    label.set_xalign(alignment_xalign(alignment));
    // Table cells keep explicit line breaks, but they do not auto-wrap. GTK's
    // wrapped labels report a very small minimum width, which makes anchored
    // tables collapse into unreadable vertical stacks inside the preview.
    label.set_wrap(false);
    label.set_selectable(false);
    if markup.is_empty() {
        label.set_label("");
    } else {
        label.set_markup(markup);
    }
    if markup.contains("<a href=") {
        let preview_weak = preview.downgrade();
        label.connect_activate_link(move |_, uri| {
            if let Some(preview) = preview_weak.upgrade() {
                preview.activate_link_target(&PreviewLaunchTarget {
                    uri: uri.to_string(),
                    local_path: None,
                });
            }
            glib::Propagation::Stop
        });
    }
    label.add_css_class("markdown-table-cell");
    if is_header {
        label.add_css_class("markdown-table-header-cell");
    }
    label
}

/// Build one scaled paintable for insertion into the text buffer.
fn build_image_paintable(path: &Path) -> Result<gdk::Paintable, glib::Error> {
    let file = gio::File::for_path(path);
    let texture = gdk::Texture::from_file(&file)?;
    let (display_width, display_height) = bounded_image_size(texture.width(), texture.height());
    let snapshot = gtk4::Snapshot::new();
    snapshot.append_texture(
        &texture,
        &gtk4::graphene::Rect::new(0.0, 0.0, display_width as f32, display_height as f32),
    );
    snapshot
        .to_paintable(Some(&gtk4::graphene::Size::new(
            display_width as f32,
            display_height as f32,
        )))
        .ok_or_else(|| glib::Error::new(gio::IOErrorEnum::Failed, "failed to snapshot image"))
}

/// Build one fallback block for unsupported or unresolved Markdown images.
fn build_image_fallback_widget(title: &str, body: &str) -> gtk4::Widget {
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    container.set_margin_top(8);
    container.set_margin_bottom(8);
    container.set_margin_start(10);
    container.set_margin_end(10);
    container.set_halign(gtk4::Align::Start);
    container.set_width_request(240);
    container.add_css_class("card");
    container.add_css_class("markdown-preview-image-fallback");

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);

    let title_label = gtk4::Label::new(Some(title));
    title_label.set_xalign(0.0);
    title_label.set_wrap(true);
    title_label.add_css_class("heading");
    title_label.add_css_class("markdown-preview-image-fallback-title");

    let body_label = gtk4::Label::new(Some(body));
    body_label.set_xalign(0.0);
    body_label.set_wrap(true);
    body_label.set_selectable(false);
    body_label.add_css_class("dim-label");
    body_label.add_css_class("monospace");
    body_label.add_css_class("markdown-preview-image-fallback-body");

    content.append(&title_label);
    content.append(&body_label);
    container.append(&content);
    container.upcast()
}

/// Map Markdown table alignment to the matching GTK label alignment value.
fn alignment_xalign(alignment: Alignment) -> f32 {
    match alignment {
        Alignment::Left | Alignment::None => 0.0,
        Alignment::Center => 0.5,
        Alignment::Right => 1.0,
    }
}

/// Bound one decoded image to a readable preview size while preserving aspect ratio.
fn bounded_image_size(width: i32, height: i32) -> (i32, i32) {
    if width <= 0 || height <= 0 {
        return (MAX_PREVIEW_IMAGE_WIDTH.min(320), 180);
    }

    let max_dimension = width.max(height);
    if max_dimension < MIN_PREVIEW_IMAGE_SIZE {
        let scaled_width = i32::try_from(
            i64::from(width).saturating_mul(i64::from(MIN_PREVIEW_IMAGE_SIZE))
                / i64::from(max_dimension),
        )
        .unwrap_or(width);
        let scaled_height = i32::try_from(
            i64::from(height).saturating_mul(i64::from(MIN_PREVIEW_IMAGE_SIZE))
                / i64::from(max_dimension),
        )
        .unwrap_or(height);
        return (scaled_width.max(1), scaled_height.max(1));
    }

    if width <= MAX_PREVIEW_IMAGE_WIDTH {
        return (width, height);
    }

    let scaled_width = MAX_PREVIEW_IMAGE_WIDTH;
    let scaled_height = i32::try_from(
        i64::from(height).saturating_mul(i64::from(MAX_PREVIEW_IMAGE_WIDTH)) / i64::from(width),
    )
    .unwrap_or(height);
    (scaled_width, scaled_height.max(1))
}

/// Convert a `HeadingLevel` to a 0-based index for the tag name array.
fn heading_level_to_index(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 0,
        HeadingLevel::H2 => 1,
        HeadingLevel::H3 => 2,
        HeadingLevel::H4 => 3,
        HeadingLevel::H5 => 4,
        HeadingLevel::H6 => 5,
    }
}

/// Convert a small `usize` index into GTK's `i32` grid coordinates.
fn usize_to_i32(value: usize) -> i32 {
    i32::try_from(value).expect("markdown table dimensions fit in i32")
}

/// Pop the last tag from the stack. No-op if the stack is empty.
fn pop_tag(stack: &mut Vec<String>) {
    stack.pop();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::markdown_preview::imp::list_item_left_margin;
    use pulldown_cmark::LinkType;
    use tempfile::tempdir;

    #[test]
    fn test_table_cell_markup_builder_supports_inline_subset() {
        let mut builder = TableCellMarkupBuilder::new(MarkdownPreviewRenderContext::default());
        builder.push_event(Event::Start(Tag::Strong));
        builder.push_event(Event::Text("bold".into()));
        builder.push_event(Event::End(TagEnd::Strong));
        builder.push_event(Event::Text(" ".into()));
        builder.push_event(Event::Start(Tag::Emphasis));
        builder.push_event(Event::Text("italic".into()));
        builder.push_event(Event::End(TagEnd::Emphasis));
        builder.push_event(Event::Text(" ".into()));
        builder.push_event(Event::Start(Tag::Strikethrough));
        builder.push_event(Event::Text("strike".into()));
        builder.push_event(Event::End(TagEnd::Strikethrough));
        builder.push_event(Event::Text(" ".into()));
        builder.push_event(Event::Code("code".into()));
        builder.push_event(Event::HardBreak);
        builder.push_event(Event::Text("<escaped>".into()));

        assert_eq!(
            builder.finish(),
            "<b>bold</b> <i>italic</i> <s>strike</s> <tt>code</tt>\n&lt;escaped&gt;"
        );
    }

    #[test]
    fn test_table_cell_markup_builder_renders_launchable_links() {
        let mut builder = TableCellMarkupBuilder::new(MarkdownPreviewRenderContext::default());
        builder.push_event(Event::Start(Tag::Link {
            link_type: LinkType::Inline,
            dest_url: "https://example.com".into(),
            title: "".into(),
            id: "".into(),
        }));
        builder.push_event(Event::Text("example".into()));
        builder.push_event(Event::End(TagEnd::Link));

        assert_eq!(
            builder.finish(),
            "<a href=\"https://example.com\">example</a>"
        );
    }

    #[test]
    fn test_buffered_table_builder_keeps_header_rows_and_cells() {
        let mut builder = BufferedTableBuilder::new(
            vec![Alignment::Left, Alignment::Right],
            MarkdownPreviewRenderContext::default(),
        );
        builder.push_event(Event::Start(Tag::TableHead));
        builder.push_event(Event::Start(Tag::TableCell));
        builder.push_event(Event::Text("Name".into()));
        builder.push_event(Event::End(TagEnd::TableCell));
        builder.push_event(Event::Start(Tag::TableCell));
        builder.push_event(Event::Text("Value".into()));
        builder.push_event(Event::End(TagEnd::TableCell));
        builder.push_event(Event::End(TagEnd::TableHead));
        builder.push_event(Event::Start(Tag::TableRow));
        builder.push_event(Event::Start(Tag::TableCell));
        builder.push_event(Event::Text("one".into()));
        builder.push_event(Event::End(TagEnd::TableCell));
        builder.push_event(Event::Start(Tag::TableCell));
        builder.push_event(Event::Text("two".into()));
        builder.push_event(Event::End(TagEnd::TableCell));
        builder.push_event(Event::End(TagEnd::TableRow));

        let table = builder.finish();
        assert_eq!(table.header_row_count(), 1);
        assert_eq!(table.column_count(), 2);
        assert_eq!(table.rows.len(), 2);
        assert!(table.rows[0].is_header);
        assert_eq!(table.rows[0].cells[0].markup, "Name");
        assert_eq!(table.rows[1].cells[1].markup, "two");
    }

    #[test]
    fn test_resolve_local_path_prefers_document_relative_match() {
        let tempdir = tempdir().expect("tempdir");
        let document_dir = tempdir.path().join("docs");
        let workspace_root = tempdir.path().join("workspace");
        std::fs::create_dir_all(&document_dir).expect("document dir");
        std::fs::create_dir_all(workspace_root.join("images")).expect("workspace dir");
        std::fs::write(document_dir.join("logo.png"), b"doc").expect("document-relative file");
        std::fs::write(workspace_root.join("logo.png"), b"workspace").expect("workspace file");

        let context = MarkdownPreviewRenderContext::new(
            Some(document_dir.join("guide.md")),
            vec![workspace_root],
        );

        assert_eq!(
            resolve_local_path("logo.png", &context),
            LocalPathResolution::Resolved(document_dir.join("logo.png"))
        );
    }

    #[test]
    fn test_resolve_local_path_reports_ambiguous_workspace_matches() {
        let tempdir = tempdir().expect("tempdir");
        let root_a = tempdir.path().join("root-a");
        let root_b = tempdir.path().join("root-b");
        std::fs::create_dir_all(root_a.join("images")).expect("root a dir");
        std::fs::create_dir_all(root_b.join("images")).expect("root b dir");
        std::fs::write(root_a.join("images/logo.png"), b"a").expect("root a file");
        std::fs::write(root_b.join("images/logo.png"), b"b").expect("root b file");

        let context = MarkdownPreviewRenderContext::new(None, vec![root_a.clone(), root_b.clone()]);

        assert_eq!(
            resolve_local_path("images/logo.png", &context),
            LocalPathResolution::Ambiguous(vec![
                root_a.join("images/logo.png"),
                root_b.join("images/logo.png"),
            ])
        );
    }

    #[test]
    fn test_resolve_image_target_rejects_remote_urls() {
        assert_eq!(
            resolve_image_target(
                "https://example.com/logo.png",
                &MarkdownPreviewRenderContext::default(),
            ),
            ResolvedImageTarget::Fallback {
                title: "Remote images are not supported",
                body: "https://example.com/logo.png".to_string(),
            }
        );
    }

    #[test]
    fn test_bounded_image_size_scales_down_wide_images() {
        assert_eq!(bounded_image_size(128, 128), (128, 128));
        assert_eq!(bounded_image_size(1280, 640), (640, 320));
        assert_eq!(bounded_image_size(16, 16), (72, 72));
        assert_eq!(bounded_image_size(0, 0), (320, 180));
    }

    #[test]
    fn test_list_item_left_margin_increases_with_depth() {
        assert_eq!(list_item_left_margin(1), 24);
        assert_eq!(list_item_left_margin(2), 44);
        assert_eq!(list_item_left_margin(3), 64);
    }

    #[test]
    fn test_footnote_number_reuses_existing_labels() {
        let mut footnote_numbers = HashMap::new();
        let mut next_footnote_number = 1;

        assert_eq!(
            footnote_number(&mut footnote_numbers, &mut next_footnote_number, "alpha"),
            1
        );
        assert_eq!(
            footnote_number(&mut footnote_numbers, &mut next_footnote_number, "beta"),
            2
        );
        assert_eq!(
            footnote_number(&mut footnote_numbers, &mut next_footnote_number, "alpha"),
            1
        );
        assert_eq!(next_footnote_number, 3);
    }
}
