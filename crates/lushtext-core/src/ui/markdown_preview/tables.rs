// SPDX-License-Identifier: GPL-3.0-or-later

//! Markdown-preview table rendering.
//!
//! Buffers pulldown-cmark table events into a bounded `BufferedTable`, builds
//! its inline cell markup, and constructs the anchored native GTK grid widget
//! inserted into the preview text view. Behavior is unchanged from when this
//! lived in `mod.rs`; only the code location moved.

use gtk4::prelude::*;
use pulldown_cmark::{Alignment, Event, Tag, TagEnd};

use crate::ui::accessibility;

use super::LushtextMarkdownPreview;
use super::links::resolve_link_target;
use super::seams::{EmbeddedBlockLayout, MarkdownPreviewRenderContext, PreviewLaunchTarget};
use super::widgets::build_preview_limit_fallback_widget;

/// Maximum table cells materialized as GTK labels in a single render turn.
///
/// One thousand labels leaves room for realistic reference tables while keeping
/// pathological CSV-like Markdown from allocating a giant widget tree at once.
const MAX_PREVIEW_TABLE_CELLS: usize = 1_000;

/// Buffered table representation used between pulldown-cmark's streaming events
/// and the final anchored GTK widget subtree.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct BufferedTable {
    /// Column alignments emitted by pulldown-cmark for this table.
    alignments: Vec<Alignment>,
    /// Header and body rows in source order.
    rows: Vec<BufferedTableRow>,
    /// Total source cells observed, even after row buffering stops.
    observed_cells: usize,
}

impl BufferedTable {
    /// Return the widest row width so GTK can build a stable grid.
    fn column_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| !row.spans_all_columns)
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

    /// Number of GTK label cells needed to render the table at full fidelity.
    ///
    /// Spanning omission rows contribute one label each rather than a full row
    /// of cells, so they are counted as one instead of as `column_count`:
    /// counting them as a whole row could push a table that is otherwise inside
    /// the budget over it just because one of its rows could not be projected,
    /// while ignoring them entirely would under-count the labels actually built.
    fn cell_count(&self) -> usize {
        let mut source_rows = 0usize;
        let mut spanning_rows = 0usize;
        for row in &self.rows {
            if row.spans_all_columns {
                spanning_rows += 1;
            } else {
                source_rows += 1;
            }
        }
        self.observed_cells.max(
            source_rows
                .saturating_mul(self.column_count())
                .saturating_add(spanning_rows),
        )
    }

    /// Whether this table would create too many child widgets in one render.
    fn exceeds_preview_widget_budget(&self) -> bool {
        self.cell_count() > MAX_PREVIEW_TABLE_CELLS
    }
}

/// One buffered Markdown table row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BufferedTableRow {
    /// Header rows get stronger styling and a section separator.
    is_header: bool,
    /// Cells in source order.
    cells: Vec<BufferedTableCell>,
    /// Whether this row is one label spanning the table instead of source cells.
    ///
    /// Omission rows use this so a row the planner could not project still
    /// reads as one full-width row at its own position inside the same table,
    /// rather than as a stray first-column cell.
    spans_all_columns: bool,
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
pub(super) struct BufferedTableBuilder {
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
    /// Source table cells seen so far.
    observed_cells: usize,
    /// Whether the table exceeded the preview widget budget while buffering.
    over_budget: bool,
}

impl BufferedTableBuilder {
    /// Start buffering one Markdown table.
    pub(super) fn new(
        alignments: Vec<Alignment>,
        render_context: MarkdownPreviewRenderContext,
    ) -> Self {
        Self {
            alignments,
            render_context,
            in_header: false,
            ..Self::default()
        }
    }

    /// Charge cells the planner counted for this table but did not retain.
    ///
    /// A carried-embed crossing stops retaining a table's remaining cells and
    /// forwards their true count instead, so the widget budget below must be
    /// evaluated against the block's real total. Charging here keeps
    /// `exceeds_preview_widget_budget()` deciding exactly what it decides when
    /// the whole table arrives in one projection turn.
    pub(super) fn charge_unretained_cells(&mut self, cells: usize) {
        self.observed_cells = self.observed_cells.saturating_add(cells);
        if self.observed_cells > MAX_PREVIEW_TABLE_CELLS {
            self.over_budget = true;
            self.current_cell = None;
            self.current_row = None;
        }
    }

    /// Append one full-width row standing in for a row that was not projected.
    pub(super) fn push_omission_row(&mut self, text: &str) {
        // The marker is not source content, so it is never charged against the
        // cell budget and never joins the header section.
        self.current_cell = None;
        if let Some(row) = self.current_row.take() {
            self.rows.push(row);
        }
        self.rows.push(BufferedTableRow {
            is_header: false,
            cells: vec![BufferedTableCell {
                markup: glib::markup_escape_text(text).to_string(),
            }],
            spans_all_columns: true,
        });
    }

    /// Fold one event from inside the active table into the buffered model.
    pub(super) fn push_event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(Tag::TableHead) => {
                if self.over_budget {
                    self.current_row = None;
                    return;
                }
                self.in_header = true;
                self.current_row = Some(BufferedTableRow {
                    is_header: true,
                    cells: Vec::new(),
                    spans_all_columns: false,
                });
            }
            Event::End(TagEnd::TableHead) => {
                if !self.over_budget
                    && let Some(row) = self.current_row.take()
                {
                    self.rows.push(row);
                }
                self.in_header = false;
            }
            Event::Start(Tag::TableRow) => {
                if self.over_budget {
                    self.current_row = None;
                    return;
                }
                self.current_row = Some(BufferedTableRow {
                    is_header: self.in_header,
                    cells: Vec::new(),
                    spans_all_columns: false,
                });
            }
            Event::End(TagEnd::TableRow) => {
                if !self.over_budget
                    && let Some(row) = self.current_row.take()
                {
                    self.rows.push(row);
                }
            }
            Event::Start(Tag::TableCell) => {
                self.observed_cells = self.observed_cells.saturating_add(1);
                if self.observed_cells > MAX_PREVIEW_TABLE_CELLS {
                    self.over_budget = true;
                    self.current_cell = None;
                    self.current_row = None;
                } else {
                    self.current_cell =
                        Some(TableCellMarkupBuilder::new(self.render_context.clone()));
                }
            }
            Event::End(TagEnd::TableCell) => {
                if !self.over_budget
                    && let (Some(row), Some(cell)) =
                        (self.current_row.as_mut(), self.current_cell.take())
                {
                    row.cells.push(BufferedTableCell {
                        markup: cell.finish(),
                    });
                }
            }
            other => {
                if !self.over_budget
                    && let Some(cell) = &mut self.current_cell
                {
                    cell.push_event(other);
                }
            }
        }
    }

    /// Finish buffering and return the final immutable table model.
    pub(super) fn finish(mut self) -> BufferedTable {
        // pulldown-cmark should close rows before the table ends, but the
        // fallback keeps malformed edge cases from silently dropping content.
        if !self.over_budget
            && let Some(cell) = self.current_cell.take()
            && let Some(row) = &mut self.current_row
        {
            row.cells.push(BufferedTableCell {
                markup: cell.finish(),
            });
        }
        if !self.over_budget
            && let Some(row) = self.current_row.take()
        {
            self.rows.push(row);
        }
        BufferedTable {
            alignments: self.alignments,
            rows: self.rows,
            observed_cells: self.observed_cells,
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
    pub(super) fn new(render_context: MarkdownPreviewRenderContext) -> Self {
        Self {
            markup: String::new(),
            render_context,
            style_stack: Vec::new(),
            link_href_stack: Vec::new(),
        }
    }

    /// Fold one pulldown-cmark event into the limited markup subset we support
    /// for `GtkLabel` table cells.
    pub(super) fn push_event(&mut self, event: Event<'_>) {
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
    pub(super) fn finish(mut self) -> String {
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

/// Build the anchored widget used for one rendered Markdown table.
fn build_table_widget(preview: &LushtextMarkdownPreview, table: &BufferedTable) -> gtk4::Widget {
    if table.exceeds_preview_widget_budget() {
        return build_preview_limit_fallback_widget(
            "Table not rendered",
            &format!(
                "This table has {} cells; the preview renders tables up to {} cells.",
                table.cell_count(),
                MAX_PREVIEW_TABLE_CELLS
            ),
            "markdown-table-fallback",
        );
    }

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
    accessibility::set_role(&grid, gtk4::AccessibleRole::Table);
    // Report the same row population `column_count()` is derived from — source
    // rows only — so the announced shape matches the rendered grid rather than
    // counting preview-owned omission rows as table data.
    let source_row_count = table
        .rows
        .iter()
        .filter(|row| !row.spans_all_columns)
        .count();
    accessibility::set_labelled_description(
        &grid,
        "Markdown table",
        &format!("Rendered table with {source_row_count} rows and {column_count} columns"),
    );

    let header_rows = table.header_row_count();
    let mut grid_row = 0usize;

    for (row_index, row) in table.rows.iter().enumerate() {
        if row.spans_all_columns {
            let markup = row.cells.first().map_or("", |cell| cell.markup.as_str());
            let label = build_table_cell_label(preview, markup, false, Alignment::Left);
            label.set_wrap(true);
            label.add_css_class("markdown-table-omission-row");
            accessibility::set_label(&label, &label.text());
            // `column_count()` is already floored at 1; keep the floor explicit
            // here because `Grid::attach` treats a zero width as a programmer
            // error rather than an empty span.
            grid.attach(
                &label,
                0,
                usize_to_i32(grid_row),
                usize_to_i32(column_count.max(1)),
                1,
            );
            grid_row += 1;
            continue;
        }
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

    grid.upcast()
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
    let cell_text = label.text();
    let cell_text = if cell_text.is_empty() {
        "Blank".into()
    } else {
        cell_text
    };
    if is_header {
        accessibility::set_role(&label, gtk4::AccessibleRole::ColumnHeader);
        accessibility::set_label(&label, &format!("Table header {cell_text}"));
    } else {
        accessibility::set_role(&label, gtk4::AccessibleRole::Cell);
        accessibility::set_label(&label, &format!("Table cell {cell_text}"));
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

/// Convert a small `usize` index into GTK's `i32` grid coordinates.
fn usize_to_i32(value: usize) -> i32 {
    i32::try_from(value).expect("markdown table dimensions fit in i32")
}

impl LushtextMarkdownPreview {
    /// Insert one buffered table as a native GTK grid anchored into the text flow.
    pub(super) fn insert_table_widget(
        &self,
        buffer: &gtk4::TextBuffer,
        iter: &mut gtk4::TextIter,
        table: &BufferedTable,
    ) {
        let widget = build_table_widget(self, table);
        self.insert_embedded_widget(
            buffer,
            iter,
            widget.upcast_ref::<gtk4::Widget>(),
            EmbeddedBlockLayout::default(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::markdown_preview::MarkdownPreviewRenderContext;
    use pulldown_cmark::{Alignment, Event, LinkType, Tag, TagEnd};

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
    fn test_table_preview_budget_counts_materialized_cells() {
        let column_count = 10;
        let row_count = (MAX_PREVIEW_TABLE_CELLS / column_count) + 1;
        let table = BufferedTable {
            alignments: vec![Alignment::Left; column_count],
            rows: (0..row_count)
                .map(|_| BufferedTableRow {
                    is_header: false,
                    spans_all_columns: false,
                    cells: (0..column_count)
                        .map(|_| BufferedTableCell {
                            markup: "cell".to_string(),
                        })
                        .collect(),
                })
                .collect(),
            observed_cells: row_count * column_count,
        };

        assert_eq!(table.cell_count(), row_count * column_count);
        assert!(table.exceeds_preview_widget_budget());
    }

    #[test]
    fn test_table_builder_stops_buffering_after_preview_budget() {
        let mut builder = BufferedTableBuilder::new(
            vec![Alignment::Left],
            MarkdownPreviewRenderContext::default(),
        );
        for index in 0..=MAX_PREVIEW_TABLE_CELLS {
            builder.push_event(Event::Start(Tag::TableRow));
            builder.push_event(Event::Start(Tag::TableCell));
            builder.push_event(Event::Text(format!("cell {index}").into()));
            builder.push_event(Event::End(TagEnd::TableCell));
            builder.push_event(Event::End(TagEnd::TableRow));
        }

        let table = builder.finish();
        assert_eq!(table.cell_count(), MAX_PREVIEW_TABLE_CELLS + 1);
        assert!(table.exceeds_preview_widget_budget());
        assert!(
            table.rows.len() <= MAX_PREVIEW_TABLE_CELLS,
            "oversized tables should stop retaining per-row markup once the fallback is inevitable"
        );
    }
}
