// SPDX-License-Identifier: GPL-3.0-or-later

//! Markdown preview widget — read-only rendered view of Markdown content.
//!
//! Most Markdown blocks render directly into a `GtkTextBuffer` with
//! `GtkTextTag`s so the preview stays lightweight and native. Tables are the
//! main exception: GTK already supports embedding widgets inside a `GtkTextView`
//! via `GtkTextChildAnchor`, so we use a buffered `GtkGrid` for table blocks
//! instead of simulating columns with padded text.
//!
//! Two display states:
//! - **Content mode**: scrolled text view with rendered Markdown
//! - **Placeholder mode**: `AdwStatusPage` with "Not a Markdown file" message

mod imp;

use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;
use pulldown_cmark::{Alignment, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use imp::{
    TAG_BLOCKQUOTE, TAG_BOLD, TAG_CODE, TAG_CODE_BLOCK, TAG_HRULE, TAG_ITALIC, TAG_LINK,
    TAG_LIST_ITEM, TAG_STRIKETHROUGH, heading_tag_name,
};

glib::wrapper! {
    pub struct LushtextMarkdownPreview(ObjectSubclass<imp::LushtextMarkdownPreview>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
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
    fn new(alignments: Vec<Alignment>) -> Self {
        Self {
            alignments,
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
                self.current_cell = Some(TableCellMarkupBuilder::default());
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
}

impl TableCellStyle {
    /// Opening Pango markup tag for this style.
    fn open_tag(self) -> &'static str {
        match self {
            Self::Bold => "<b>",
            Self::Italic => "<i>",
            Self::Strikethrough => "<s>",
        }
    }

    /// Closing Pango markup tag for this style.
    fn close_tag(self) -> &'static str {
        match self {
            Self::Bold => "</b>",
            Self::Italic => "</i>",
            Self::Strikethrough => "</s>",
        }
    }
}

/// Markup builder for one buffered table cell.
#[derive(Debug, Default)]
struct TableCellMarkupBuilder {
    /// Accumulated Pango markup.
    markup: String,
    /// Nested inline styles that need closing tags in reverse order.
    style_stack: Vec<TableCellStyle>,
}

impl TableCellMarkupBuilder {
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
        }
        self.markup
    }

    /// Push an inline style start marker.
    fn push_style(&mut self, style: TableCellStyle) {
        self.markup.push_str(style.open_tag());
        self.style_stack.push(style);
    }

    /// Close the matching style if the parser says that nested span ended.
    fn pop_style(&mut self, style: TableCellStyle) {
        if self.style_stack.pop() == Some(style) {
            self.markup.push_str(style.close_tag());
        }
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

    /// Render Markdown content into the text view, replacing any previous content.
    ///
    /// Switches to content mode (text view visible, placeholder hidden).
    /// The rendering walks the `pulldown-cmark` event stream and maps text
    /// blocks to `GtkTextTag`s while buffering tables into anchored `GtkGrid`s.
    pub fn render_markdown(&self, markdown: &str) {
        self.show_content_view();
        self.clear_rendered_state();

        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        let parser = Parser::new_ext(markdown, options);
        let mut iter = buffer.end_iter();

        // Tag stack: tracks which TextTag names are currently active.
        // When we insert text, all tags in the stack are applied.
        let mut tag_stack: Vec<String> = Vec::new();

        // Track list nesting: None = not in a list, Some(None) = unordered,
        // Some(Some(n)) = ordered starting at n.
        let mut list_stack: Vec<Option<u64>> = Vec::new();

        // Track whether we need a paragraph separator before the next block.
        let mut needs_block_separator = false;

        // Tables need one complete buffered pass before GTK can lay out rows and
        // columns correctly, so we accumulate them separately from text blocks.
        let mut active_table: Option<BufferedTableBuilder> = None;

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

            match event {
                Event::Start(Tag::Table(alignments)) => {
                    if needs_block_separator {
                        buffer.insert(&mut iter, "\n");
                    }
                    active_table = Some(BufferedTableBuilder::new(alignments));
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
                    Tag::BlockQuote(_) => {
                        if needs_block_separator {
                            buffer.insert(&mut iter, "\n");
                        }
                        tag_stack.push(TAG_BLOCKQUOTE.to_string());
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
                        let prefix = match list_stack.last() {
                            Some(Some(start)) => format!("{start}. "),
                            _ => "\u{2022} ".to_string(),
                        };
                        let tags: Vec<&str> =
                            tag_stack.iter().map(std::string::String::as_str).collect();
                        let mut all_tags = tags.clone();
                        all_tags.push(TAG_LIST_ITEM);
                        insert_with_tags(&buffer, &mut iter, &prefix, &all_tags);
                        tag_stack.push(TAG_LIST_ITEM.to_string());
                    }
                    Tag::Emphasis => tag_stack.push(TAG_ITALIC.to_string()),
                    Tag::Strong => tag_stack.push(TAG_BOLD.to_string()),
                    Tag::Strikethrough => tag_stack.push(TAG_STRIKETHROUGH.to_string()),
                    Tag::Link { .. } => tag_stack.push(TAG_LINK.to_string()),
                    // Skip elements we don't render natively (images, metadata, raw HTML, etc.).
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
                    TagEnd::BlockQuote(_) => {
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
                        pop_tag(&mut tag_stack);
                        buffer.insert(&mut iter, "\n");
                        if let Some(Some(n)) = list_stack.last_mut() {
                            *n += 1;
                        }
                    }
                    TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                        pop_tag(&mut tag_stack);
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
                // Skip HTML, math, footnotes — out of scope for native rendering.
                _ => {}
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
    /// GTK child anchors are not plain text, so embedded table widgets do not
    /// appear in this string. Tests use it for surrounding text flow only.
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

    /// Remove any previously anchored table widgets and clear the backing text buffer.
    fn clear_rendered_state(&self) {
        let imp = self.imp();
        {
            let mut rendered_tables = imp.rendered_tables.borrow_mut();
            for widget in rendered_tables.drain(..) {
                imp.text_view.remove(&widget);
            }
        }
        imp.text_view.buffer().set_text("");
    }

    /// Insert one buffered table as a native GTK grid anchored into the text flow.
    fn insert_table_widget(
        &self,
        buffer: &gtk4::TextBuffer,
        iter: &mut gtk4::TextIter,
        table: &BufferedTable,
    ) {
        let grid = build_table_grid(table);
        let anchor = buffer.create_child_anchor(iter);
        self.imp().text_view.add_child_at_anchor(&grid, &anchor);
        self.imp()
            .rendered_tables
            .borrow_mut()
            .push(grid.upcast::<gtk4::Widget>());
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

/// Build the anchored `GtkGrid` used for one rendered Markdown table.
fn build_table_grid(table: &BufferedTable) -> gtk4::Grid {
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
            let label = build_table_cell_label(markup, row.is_header, alignment);
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
fn build_table_cell_label(markup: &str, is_header: bool, alignment: Alignment) -> gtk4::Label {
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
    label.add_css_class("markdown-table-cell");
    if is_header {
        label.add_css_class("markdown-table-header-cell");
    }
    label
}

/// Map Markdown table alignment to the matching GTK label alignment value.
fn alignment_xalign(alignment: Alignment) -> f32 {
    match alignment {
        Alignment::Left | Alignment::None => 0.0,
        Alignment::Center => 0.5,
        Alignment::Right => 1.0,
    }
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

    #[test]
    fn test_table_cell_markup_builder_supports_inline_subset() {
        let mut builder = TableCellMarkupBuilder::default();
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
    fn test_buffered_table_builder_keeps_header_rows_and_cells() {
        let mut builder = BufferedTableBuilder::new(vec![Alignment::Left, Alignment::Right]);
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
}
