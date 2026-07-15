// SPDX-License-Identifier: GPL-3.0-or-later

//! Markdown preview widget — read-only rendered view of Markdown content.
//!
//! Most Markdown blocks render directly into a `GtkTextBuffer` with
//! `GtkTextTag`s so the preview stays lightweight and native. Tables, local
//! image blocks, and Markdown code blocks are the main exceptions: GTK already
//! supports embedding widgets inside a `GtkTextView` via `GtkTextChildAnchor`,
//! so we use anchored GTK widgets for cases where plain styled text is not
//! expressive enough.
//!
//! Two display states:
//! - **Content mode**: scrolled text view with rendered Markdown
//! - **Placeholder mode**: `AdwStatusPage` with "Not a Markdown file" message
//!
//! Dialog note editors can also ask for a placeholder rendered inside content
//! mode so an Edit/Render stack measures the same text surface before and
//! after the first user-visible render.

// Private GObject implementation for the template-backed preview surface.
mod imp;
mod inline_footnotes;

use gio::prelude::FileExt;
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::{self, gdk};
#[cfg(any(test, feature = "fuzzing"))]
use pulldown_cmark::Parser;
use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Tag, TagEnd};
use sourceview5::prelude::*;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(feature = "test-utils")]
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::services::filesystem::{PathStatus, metadata as fs_metadata, read as fs_read};
pub use crate::services::markdown_render::MarkdownRenderState;
use crate::services::markdown_render::{
    MAX_MARKDOWN_SOURCE_BYTES, MarkdownEventBatch, MarkdownPlanLimit, MarkdownPlanMetrics,
    MarkdownRenderPlan, markdown_render_options, plan_markdown, plan_markdown_cancellable,
};
use crate::ui::accessibility;
use crate::ui::buffer_snapshot::BufferSnapshotHandle;
use crate::ui::editor_page::{approximate_char_width, readable_column_margin};
use gtk_lush_tasks::spawn_blocking_then_weak;

use imp::{
    ALERT_BODY_LEFT_MARGIN, ALERT_BODY_RIGHT_MARGIN, DEFINITION_DEF_LEFT_MARGIN,
    DEFINITION_DEF_RIGHT_MARGIN, FOOTNOTE_DEF_LEFT_MARGIN, FOOTNOTE_DEF_RIGHT_MARGIN,
    TAG_ALERT_BODY, TAG_BLOCKQUOTE, TAG_BOLD, TAG_CODE, TAG_DEFINITION_DEF, TAG_DEFINITION_TERM,
    TAG_FOOTNOTE_DEF, TAG_FOOTNOTE_DEF_LABEL, TAG_FOOTNOTE_REF, TAG_HRULE, TAG_ITALIC, TAG_LINK,
    TAG_LIST_ITEM, TAG_STRIKETHROUGH, TAG_TASK_MARKER, alert_title, alert_title_tag_name,
    blockquote_left_margin, blockquote_rail_prefix, ensure_blockquote_depth_tag,
    ensure_list_item_depth_tag, heading_tag_name, list_item_text_margin,
};
use inline_footnotes::{
    InlineFootnoteLowering, lower_inline_footnotes, lower_inline_footnotes_cancellable,
};

fn inline_footnote_limited_plan(source_bytes: usize) -> MarkdownRenderPlan {
    MarkdownRenderPlan {
        batches: Vec::new(),
        metrics: MarkdownPlanMetrics {
            source_bytes,
            ..MarkdownPlanMetrics::default()
        },
        limit: Some(MarkdownPlanLimit::InlineFootnotes),
    }
}

/// Result of fuzzing Markdown preprocessing without constructing GTK widgets.
///
/// The fuzz target only needs to know that the preprocessing and parser setup
/// completed. Counts keep the helper useful for sanity checks without exposing
/// renderer internals as a stable public API.
#[cfg(feature = "fuzzing")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FuzzedMarkdownPreprocess {
    /// Number of pulldown-cmark events produced after preprocessing.
    pub parser_event_count: usize,
    /// Byte length of the Markdown text passed to the parser.
    pub parser_input_len: usize,
    /// Whether markdown-it-style inline footnotes were lowered first.
    pub lowered_inline_footnotes: bool,
}

/// Run the preview's real inline-footnote lowering for feature-gated generated tests.
///
/// Keeping this as a narrow feature-only hook lets generated tests and fuzzing
/// exercise the production lowering path without making the private scanner
/// part of the normal application API.
#[cfg(any(feature = "property-tests", feature = "fuzzing"))]
#[must_use]
fn lower_inline_footnotes_for_generated_test(markdown: &str) -> Option<String> {
    match lower_inline_footnotes(markdown, markdown_render_options()) {
        InlineFootnoteLowering::Lowered(lowered) => Some(lowered),
        InlineFootnoteLowering::Unchanged
        | InlineFootnoteLowering::Limited
        | InlineFootnoteLowering::Cancelled => None,
    }
}

/// Run the preview's real inline-footnote lowering for feature-gated property tests.
///
/// This preserves the original property-test API while sharing the same
/// generated-input hook used by fuzzing.
#[cfg(feature = "property-tests")]
#[must_use]
pub fn lower_inline_footnotes_for_property_test(markdown: &str) -> Option<String> {
    lower_inline_footnotes_for_generated_test(markdown)
}

/// Exercise Markdown preprocessing and parser setup for fuzz targets.
///
/// The helper stops before renderer code that touches `GtkTextBuffer`,
/// `LushtextMarkdownPreview`, links, images, GSettings, or other GTK state.
#[cfg(feature = "fuzzing")]
#[must_use]
pub fn preprocess_markdown_for_fuzzing(markdown: &str) -> FuzzedMarkdownPreprocess {
    let lowered = lower_inline_footnotes_for_generated_test(markdown);
    let lowered_inline_footnotes = lowered.is_some();
    let parser_input = lowered.as_deref().unwrap_or(markdown);
    let options = markdown_render_options();
    let parser_event_count = Parser::new_ext(parser_input, options).count();

    FuzzedMarkdownPreprocess {
        parser_event_count,
        parser_input_len: parser_input.len(),
        lowered_inline_footnotes,
    }
}

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
/// Interior horizontal inset for native code-block widgets.
///
/// The old text-tag renderer painted the block background directly behind the
/// glyphs. Keeping padding on the embedded scroller makes the source text read
/// as one deliberate surface instead of text stuck to a highlight edge.
const CODE_BLOCK_HORIZONTAL_PADDING: i32 = 12;
/// Interior vertical inset for native code-block widgets.
const CODE_BLOCK_VERTICAL_PADDING: i32 = 8;
/// CSS priority for per-render code-block palette fixes.
///
/// The bundled stylesheet gives code blocks their shape, while this provider
/// supplies the active GtkSourceView background after the user-selected scheme
/// is known. A slightly higher priority keeps the two layers from fighting.
const CODE_BLOCK_BACKGROUND_CSS_PRIORITY: u32 = gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION + 2;
/// Maximum source image bytes the preview will decode for one Markdown image.
///
/// A 32 MiB source still covers normal screenshots and exported diagrams, but
/// it prevents accidental camera originals or generated art from dominating a
/// background worker and its post-decode pixel copy.
const MAX_PREVIEW_IMAGE_SOURCE_BYTES: u64 = 32 * 1024 * 1024;
/// Maximum source pixels accepted before the preview falls back to a compact notice.
///
/// 64 megapixels is intentionally above ordinary desktop images while below
/// the point where RGB/RGBA decode buffers can spike into hundreds of MiB.
const MAX_PREVIEW_IMAGE_SOURCE_PIXELS: i64 = 64_000_000;
/// Maximum local-image descriptors that may own queued or active work.
const MAX_PREVIEW_IMAGE_WORK_ITEMS: usize = 4;
/// Conservative ownership charge for source bytes plus one bounded RGBA decode.
const PREVIEW_IMAGE_WORK_CHARGE_BYTES: u64 = MAX_PREVIEW_IMAGE_SOURCE_BYTES.saturating_add(
    (MAX_PREVIEW_IMAGE_WIDTH as u64)
        .saturating_pow(2)
        .saturating_mul(4),
);
/// Total conservative bytes admitted across active and compact queued image work.
const MAX_PREVIEW_IMAGE_WORK_BYTES: u64 =
    PREVIEW_IMAGE_WORK_CHARGE_BYTES.saturating_mul(MAX_PREVIEW_IMAGE_WORK_ITEMS as u64);
/// Maximum literal text bytes highlighted inside one native code-block widget.
///
/// GtkSourceView highlighting is excellent for excerpts, but 64 KiB keeps a
/// single fenced block from monopolizing a render turn when preview refreshes.
const MAX_PREVIEW_CODE_BLOCK_BYTES: usize = 64 * 1024;
/// Maximum table cells materialized as GTK labels in a single render turn.
///
/// One thousand labels leaves room for realistic reference tables while keeping
/// pathological CSV-like Markdown from allocating a giant widget tree at once.
const MAX_PREVIEW_TABLE_CELLS: usize = 1_000;
/// Inputs above this size are parsed away from GTK before bounded projection.
const MARKDOWN_BACKGROUND_PLAN_THRESHOLD_BYTES: usize = 64 * 1024;
/// Maximum detached text characters removed in one main-loop retirement turn.
const MARKDOWN_RETIREMENT_CHARS_PER_TURN: usize = 64 * 1024;
/// Maximum detached widget/link references released in one retirement turn.
const MARKDOWN_RETIREMENT_ITEMS_PER_TURN: usize = 64;
#[cfg(feature = "test-utils")]
static IMAGE_WORK_DELAY_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "test-utils")]
static MARKDOWN_PLAN_DELAY_MS: AtomicU64 = AtomicU64::new(0);

/// Extra render context supplied by the window when previewing a real Markdown file.
///
/// Relative links and images need a stable base path, and workspace-relative
/// image paths need the active sidebar folders. Keeping those inputs in one
/// value object lets the preview stay a reusable widget instead of reaching
/// back into the window shell directly.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarkdownPreviewRenderContext {
    document_path: Option<PathBuf>,
    workspace_folders: Vec<PathBuf>,
}

impl MarkdownPreviewRenderContext {
    /// Create one render context for a Markdown preview pass.
    #[must_use]
    pub fn new(document_path: Option<PathBuf>, workspace_folders: Vec<PathBuf>) -> Self {
        Self {
            document_path,
            workspace_folders,
        }
    }
}

glib::wrapper! {
    // Exposes the private preview implementation as a regular GTK widget for
    // editor tabs, note dialogs, and widget tests.
    /// Public Markdown preview widget used by editor tabs and note surfaces.
    ///
    /// The wrapper exposes render and navigation methods; the private
    /// implementation owns GtkTextView tags, anchors, and launch state.
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

/// Captured horizontal context for one embedded Markdown block.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct EmbeddedBlockLayout {
    /// Outer offset before the embedded widget, matching the active text column.
    margin_start: i32,
    /// Outer offset after the embedded widget, matching the active text column.
    margin_end: i32,
}

impl EmbeddedBlockLayout {
    /// Fold one active text-tag margin into the embedded-widget context.
    fn include_margin(&mut self, margin_start: i32, margin_end: i32) {
        // GtkTextTag block margins act like competing paragraph properties, not
        // nested boxes. Use the widest active margin so child anchors stay in
        // the same effective column as nearby tagged text.
        self.margin_start = self.margin_start.max(margin_start);
        self.margin_end = self.margin_end.max(margin_end);
    }

    /// Return the width a code block can use inside this context.
    fn code_block_width(self, preview_text_column_width: i32) -> i32 {
        preview_text_column_width
            .saturating_sub(self.margin_start.saturating_add(self.margin_end))
            .max(1)
    }
}

/// One widget anchored into the preview plus the layout context active at insertion.
#[derive(Clone)]
pub(super) struct RenderedEmbed {
    /// Widget added to the `GtkTextView` at a child anchor.
    widget: gtk4::Widget,
    /// Captured block context used by later allocation refreshes.
    layout: EmbeddedBlockLayout,
}

impl RenderedEmbed {
    /// Store one child-anchor widget and its insertion-time layout context.
    fn new(widget: gtk4::Widget, layout: EmbeddedBlockLayout) -> Self {
        Self { widget, layout }
    }
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

/// Visual inputs shared by all code blocks in one render pass.
#[derive(Debug, Clone)]
struct CodeBlockTheme {
    /// GtkSourceView scheme used for syntax token colors.
    style_scheme: Option<sourceview5::StyleScheme>,
    /// CSS background applied to both the outer block and inner text area.
    background_css: String,
}

impl CodeBlockTheme {
    /// Resolve the current editor palette once so many code blocks stay cheap.
    fn from_settings(settings: &gtk4::gio::Settings) -> Self {
        let style_scheme = crate::ui::theme::active_sourceview_scheme(settings);
        let palette = crate::ui::theme::resolve_tab_content_palette(settings);
        Self {
            style_scheme,
            background_css: crate::ui::theme::css_rgba_with_alpha(&palette.text_bg, 1.0),
        }
    }
}

/// Marker style for the current Markdown list frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListMarker {
    /// Unordered list items render with a native bullet glyph.
    Unordered,
    /// Ordered list items render with the next number from the source list.
    Ordered(u64),
}

impl ListMarker {
    /// Return the visible marker prefix for the next item in this list frame.
    fn prefix(self) -> String {
        match self {
            Self::Unordered => "\u{2022} ".to_string(),
            Self::Ordered(number) => format!("{number}. "),
        }
    }

    /// Advance ordered list counters after one item has finished rendering.
    fn advance(&mut self) {
        if let Self::Ordered(number) = self {
            *number += 1;
        }
    }
}

/// One active Markdown list level in the streaming renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ListFrame {
    /// Marker and counter state for this nesting depth.
    marker: ListMarker,
}

impl ListFrame {
    /// Create a list frame from pulldown-cmark's optional ordered-list start.
    fn new(start_num: Option<u64>) -> Self {
        Self {
            marker: start_num.map_or(ListMarker::Unordered, ListMarker::Ordered),
        }
    }

    /// Return the marker prefix for the next list item.
    fn prefix(self) -> String {
        self.marker.prefix()
    }

    /// Advance this list's counter after one item.
    fn advance(&mut self) {
        self.marker.advance();
    }
}

/// Per-item row-flow state for Markdown lists.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct ListItemRenderState {
    /// Whether this item has emitted any visible marker or content.
    has_content: bool,
    /// Whether the previous paragraph ended and a following paragraph should
    /// keep the intentional loose-list blank row.
    paragraph_ended: bool,
}

/// Per-definition row-flow state for Markdown definition lists.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct DefinitionRenderState {
    /// Whether this definition has emitted any visible text or anchored block.
    has_content: bool,
    /// Whether a paragraph ended and the next paragraph needs the visible
    /// separation pulldown-cmark represents inside a loose definition body.
    paragraph_ended: bool,
}

/// Result of trying to resolve a local filesystem path from Markdown content.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LocalPathResolution {
    /// One unambiguous local path candidate was formed.
    Resolved(PathBuf),
    /// No document or workspace base can produce a local path candidate.
    Missing,
    /// More than one workspace-relative path is possible, so link activation should not guess.
    Ambiguous(Vec<PathBuf>),
}

/// Result of resolving one Markdown image destination.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ResolvedImageTarget {
    /// A local file that should render as a native preview image block.
    LocalFile(PathBuf),
    /// Ordered relative image candidates resolved off the GTK thread.
    OrderedCandidates(Vec<PathBuf>),
    /// A fallback block that should appear inline instead of silently dropping the image.
    Fallback { title: &'static str, body: String },
}

/// Decoded image pixels that can safely cross back from a worker thread.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DecodedImage {
    /// Image width after preview-size bounding.
    width: i32,
    /// Image height after preview-size bounding.
    height: i32,
    /// Number of bytes between rows.
    stride: usize,
    /// Whether the pixels include an alpha channel.
    has_alpha: bool,
    /// Owned RGB/RGBA bytes copied out of the background pixbuf decode.
    pixels: Vec<u8>,
}

/// Result of checking workspace-relative image candidates on a background thread.
#[derive(Debug, Clone, PartialEq, Eq)]
enum OrderedImageCandidateResult {
    /// The first decodable candidate in workspace-folder order, already scaled.
    Loadable { path: PathBuf, image: DecodedImage },
    /// At least one candidate existed, but none could be decoded as an image.
    Unloadable { path: PathBuf, error: String },
    /// None of the candidate paths were present as decodable files.
    Missing { raw_target: String },
}

/// Buffered Markdown image collected from pulldown-cmark's event stream.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BufferedImage {
    /// Raw destination URL from the Markdown image syntax.
    destination: String,
    /// Human-readable alternative text built from the image's child events.
    alt_text: String,
}

/// Compact queued local-image work owned by one render generation.
struct PendingImageWork {
    generation: u64,
    raw_target: String,
    paths: Vec<PathBuf>,
    container: glib::WeakRef<gtk4::Box>,
    charge_bytes: u64,
}

/// Scalar completion identity retained while one image worker is active.
struct ActiveImageWork {
    generation: u64,
    container: glib::WeakRef<gtk4::Box>,
    charge_bytes: u64,
}

/// Latest document-sized planning request retained behind one active worker.
struct PendingMarkdownPlan {
    generation: u64,
    source: String,
    context: MarkdownPreviewRenderContext,
}

/// One detached render generation awaiting bounded main-loop cleanup.
pub(super) struct RetiredMarkdownRender {
    buffer: gtk4::TextBuffer,
    embeds: VecDeque<RenderedEmbed>,
    links: VecDeque<RenderedTextLink>,
}

impl RetiredMarkdownRender {
    fn is_empty(&self) -> bool {
        self.buffer.char_count() == 0 && self.embeds.is_empty() && self.links.is_empty()
    }
}

/// Serial disposer for detached Markdown render generations.
#[derive(Default)]
pub(super) struct MarkdownRetirementSession {
    states: VecDeque<RetiredMarkdownRender>,
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

/// Buffered Markdown code block collected before we create an anchored source view.
#[derive(Debug, Clone, PartialEq)]
struct BufferedCodeBlock {
    /// Original pulldown-cmark block kind, including fenced info string.
    kind: CodeBlockKind<'static>,
    /// Literal code text emitted between code-block start and end tags.
    text: String,
    /// Total source bytes observed, even after preview storage stops.
    source_bytes: usize,
}

impl BufferedCodeBlock {
    /// Start buffering one code block from pulldown-cmark's borrowed event data.
    fn new(kind: CodeBlockKind<'_>) -> Self {
        Self {
            kind: kind.into_static(),
            text: String::new(),
            source_bytes: 0,
        }
    }

    /// Fold one event inside the code block into literal text.
    fn push_event(&mut self, event: Event<'_>) {
        match event {
            Event::Text(text) | Event::Code(text) => self.push_literal(&text),
            Event::SoftBreak | Event::HardBreak => self.push_literal("\n"),
            _ => {}
        }
    }

    /// Add one literal chunk while keeping the preview-owned buffer bounded.
    fn push_literal(&mut self, text: &str) {
        self.source_bytes = self.source_bytes.saturating_add(text.len());
        let remaining = MAX_PREVIEW_CODE_BLOCK_BYTES.saturating_sub(self.text.len());
        if remaining == 0 {
            return;
        }

        let end = text
            .char_indices()
            .map(|(index, _)| index)
            .take_while(|index| *index <= remaining)
            .last()
            .unwrap_or(0);
        let end = if text.len() <= remaining {
            text.len()
        } else {
            end
        };
        self.text.push_str(&text[..end]);
    }

    /// Return the first info-string word from a fenced block, if present.
    fn language_hint(&self) -> Option<&str> {
        match &self.kind {
            CodeBlockKind::Fenced(info) => info
                .split_whitespace()
                .next()
                .filter(|hint| !hint.is_empty()),
            CodeBlockKind::Indented => None,
        }
    }

    /// Whether this block is too large for a syntax-highlighted GTK subtree.
    fn exceeds_preview_widget_budget(&self) -> bool {
        self.source_byte_len() > MAX_PREVIEW_CODE_BLOCK_BYTES
    }

    /// Total source bytes represented by this buffered block.
    fn source_byte_len(&self) -> usize {
        self.source_bytes.max(self.text.len())
    }
}

/// Code block being collected together with the layout context active at its start.
struct ActiveCodeBlock {
    /// Literal code block data collected from pulldown-cmark events.
    code_block: BufferedCodeBlock,
    /// Text-column context captured before child-anchor insertion.
    layout: EmbeddedBlockLayout,
}

impl ActiveCodeBlock {
    /// Start buffering one code block and remember where it should be laid out.
    fn new(kind: CodeBlockKind<'_>, layout: EmbeddedBlockLayout) -> Self {
        Self {
            code_block: BufferedCodeBlock::new(kind),
            layout,
        }
    }

    /// Fold one parser event into the underlying literal code buffer.
    fn push_event(&mut self, event: Event<'_>) {
        self.code_block.push_event(event);
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
    /// Total source cells observed, even after row buffering stops.
    observed_cells: usize,
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

    /// Number of GTK label cells needed to render the table at full fidelity.
    fn cell_count(&self) -> usize {
        self.observed_cells
            .max(self.rows.len().saturating_mul(self.column_count()))
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
    /// Source table cells seen so far.
    observed_cells: usize,
    /// Whether the table exceeded the preview widget budget while buffering.
    over_budget: bool,
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
                if self.over_budget {
                    self.current_row = None;
                    return;
                }
                self.in_header = true;
                self.current_row = Some(BufferedTableRow {
                    is_header: true,
                    cells: Vec::new(),
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
    fn finish(mut self) -> BufferedTable {
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
    /// Replace the app-local note-source capture owned by this preview.
    pub(crate) fn replace_source_snapshot(&self, snapshot: Option<BufferSnapshotHandle>) {
        if let Some(previous) = self.imp().source_snapshot.replace(snapshot) {
            previous.dispose();
        }
    }

    /// Release the completed note-source capture without touching its callback.
    pub(crate) fn clear_source_snapshot(&self) {
        self.imp().source_snapshot.take();
    }

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
        let generation = self.begin_render_session();
        let context = context.clone();

        if markdown.len() > MAX_MARKDOWN_SOURCE_BYTES {
            self.start_render_plan(generation, plan_markdown(markdown), context);
        } else if markdown.len() > MARKDOWN_BACKGROUND_PLAN_THRESHOLD_BYTES {
            self.show_content_view();
            self.clear_rendered_state();
            self.imp()
                .text_view
                .buffer()
                .set_text("Rendering Markdown preview…");
            accessibility::set_description(
                &*self.imp().text_view,
                "Markdown preview rendering is pending",
            );
            self.enqueue_markdown_plan(PendingMarkdownPlan {
                generation,
                source: markdown.to_string(),
                context,
            });
        } else {
            let plan = match lower_inline_footnotes(markdown, markdown_render_options()) {
                InlineFootnoteLowering::Lowered(lowered) => plan_markdown(&lowered),
                InlineFootnoteLowering::Unchanged => plan_markdown(markdown),
                InlineFootnoteLowering::Limited => inline_footnote_limited_plan(markdown.len()),
                InlineFootnoteLowering::Cancelled => return,
            };
            self.start_render_plan(generation, plan, context);
        }
    }

    /// Apply one complete-block event batch. State never crosses a batch
    /// boundary, so the planner only emits boundaries at top-level depth zero.
    fn render_event_batch(
        &self,
        batch: MarkdownEventBatch,
        context: &MarkdownPreviewRenderContext,
    ) {
        let imp = self.imp();
        let buffer = imp.text_view.buffer();
        let mut iter = buffer.end_iter();
        let code_block_theme =
            CodeBlockTheme::from_settings(&gtk4::gio::Settings::new(crate::config::APP_ID));

        // Tag stack: tracks which TextTag names are currently active.
        // When we insert text, all tags in the stack are applied.
        let mut tag_stack: Vec<String> = Vec::new();
        // Generic blockquote depth is tracked separately from typed GFM alerts
        // so alert callouts can keep their card-like rendering while ordinary
        // nested quotes get depth-aware rail glyphs.
        let mut generic_blockquote_depth = 0usize;

        // Track list nesting and the active list items separately so paragraph
        // row-flow inside lists cannot leak into top-level block spacing.
        let mut list_stack: Vec<ListFrame> = Vec::new();
        let mut list_item_stack: Vec<ListItemRenderState> = Vec::new();
        // Definition entries have no markers, so they track paragraph flow
        // separately from ordinary lists while sharing the same inline tag path.
        let mut definition_stack: Vec<DefinitionRenderState> = Vec::new();
        // List markers need one event of lookahead because task list state
        // arrives after `Tag::Item`; delay insertion until real item content.
        let mut pending_list_prefix: Option<String> = None;
        // Keep track of launchable text-buffer links so click and hover
        // controllers can resolve them after the render is complete.
        let mut active_text_links: Vec<ActiveTextLink> = Vec::new();

        // Track whether we need a paragraph separator before the next block.
        let mut needs_block_separator = false;

        // Tables and code blocks need one complete buffered pass before GTK can
        // lay out their embedded widgets, so we accumulate them separately from
        // text blocks.
        let mut active_table: Option<BufferedTableBuilder> = None;
        let mut active_code_block: Option<ActiveCodeBlock> = None;
        // Images become anchored GTK widgets, so we buffer their alt text until
        // pulldown-cmark closes the image span.
        let mut active_image: Option<BufferedImage> = None;
        // Footnote numbering stays local to the preview render so references and
        // definitions can agree on a stable ordinal without a second parse pass.
        let mut footnote_numbers: HashMap<String, usize> = HashMap::new();
        let mut next_footnote_number = 1usize;

        for event in batch.into_events() {
            if let Some(table) = &mut active_table {
                match event {
                    Event::End(TagEnd::Table) => {
                        let table = active_table.take().expect("active table should exist");
                        let table = table.finish();
                        self.insert_table_widget(&buffer, &mut iter, &table);
                        buffer.insert(&mut iter, "\n");
                        mark_current_definition_content(&mut definition_stack);
                        needs_block_separator = true;
                    }
                    other => table.push_event(other),
                }
                continue;
            }

            if let Some(code_block) = &mut active_code_block {
                match event {
                    Event::End(TagEnd::CodeBlock) => {
                        let active_code_block = active_code_block
                            .take()
                            .expect("active code block should exist");
                        self.insert_code_block_widget(
                            &buffer,
                            &mut iter,
                            &active_code_block.code_block,
                            &code_block_theme,
                            active_code_block.layout,
                        );
                        buffer.insert(&mut iter, "\n");
                        mark_current_list_item_content(&mut list_item_stack);
                        mark_current_definition_content(&mut definition_stack);
                        needs_block_separator = true;
                    }
                    other => code_block.push_event(other),
                }
                continue;
            }

            if let Some(image) = &mut active_image {
                match event {
                    Event::End(TagEnd::Image) => {
                        let image = active_image.take().expect("active image should exist");
                        self.insert_image_widget(&buffer, &mut iter, &image, context);
                        buffer.insert(&mut iter, "\n");
                        mark_current_definition_content(&mut definition_stack);
                        needs_block_separator = true;
                    }
                    other => image.push_event(other),
                }
                continue;
            }

            if pending_list_prefix.is_some() && should_flush_pending_list_prefix(&event) {
                insert_blockquote_rail_if_needed(
                    &buffer,
                    &mut iter,
                    &tag_stack,
                    generic_blockquote_depth,
                );
                if flush_pending_list_prefix(
                    &buffer,
                    &mut iter,
                    &tag_stack,
                    &mut pending_list_prefix,
                ) {
                    mark_current_list_item_content(&mut list_item_stack);
                }
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
                        if current_list_item_needs_paragraph_separator(&list_item_stack) {
                            buffer.insert(&mut iter, "\n");
                            clear_current_list_item_paragraph_end(&mut list_item_stack);
                        } else if current_definition_needs_paragraph_separator(&definition_stack) {
                            buffer.insert(&mut iter, "\n");
                            clear_current_definition_paragraph_end(&mut definition_stack);
                        } else if needs_block_separator
                            && (list_item_stack.is_empty() || !definition_stack.is_empty())
                        {
                            buffer.insert(&mut iter, "\n");
                        }
                        needs_block_separator = false;
                    }
                    Tag::DefinitionList => {
                        if needs_block_separator {
                            buffer.insert(&mut iter, "\n");
                        }
                        needs_block_separator = false;
                    }
                    Tag::DefinitionListTitle => {
                        if needs_block_separator {
                            buffer.insert(&mut iter, "\n");
                        } else {
                            ensure_rendered_line_break(&buffer, &mut iter);
                        }
                        tag_stack.push(TAG_DEFINITION_TERM.to_string());
                        needs_block_separator = false;
                    }
                    Tag::DefinitionListDefinition => {
                        if needs_block_separator {
                            buffer.insert(&mut iter, "\n");
                        } else {
                            ensure_rendered_line_break(&buffer, &mut iter);
                        }
                        tag_stack.push(TAG_DEFINITION_DEF.to_string());
                        definition_stack.push(DefinitionRenderState::default());
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
                            generic_blockquote_depth += 1;
                            tag_stack.push(TAG_BLOCKQUOTE.to_string());
                            let depth_tag =
                                ensure_blockquote_depth_tag(&buffer, generic_blockquote_depth);
                            tag_stack.push(depth_tag);
                        }
                        needs_block_separator = false;
                    }
                    Tag::CodeBlock(kind) => {
                        if needs_block_separator {
                            buffer.insert(&mut iter, "\n");
                        }
                        let layout = embedded_block_layout(
                            &tag_stack,
                            &list_stack,
                            &list_item_stack,
                            generic_blockquote_depth,
                            &definition_stack,
                        );
                        active_code_block = Some(ActiveCodeBlock::new(kind, layout));
                        needs_block_separator = false;
                    }
                    Tag::List(start_num) => {
                        if !list_item_stack.is_empty() {
                            if flush_pending_list_prefix(
                                &buffer,
                                &mut iter,
                                &tag_stack,
                                &mut pending_list_prefix,
                            ) {
                                mark_current_list_item_content(&mut list_item_stack);
                            }
                            ensure_rendered_line_break(&buffer, &mut iter);
                            clear_current_list_item_paragraph_end(&mut list_item_stack);
                        } else if needs_block_separator {
                            buffer.insert(&mut iter, "\n");
                        }
                        list_stack.push(ListFrame::new(start_num));
                        needs_block_separator = false;
                    }
                    Tag::Item => {
                        pending_list_prefix = Some(match list_stack.last() {
                            Some(frame) => frame.prefix(),
                            None => ListMarker::Unordered.prefix(),
                        });
                        let depth_tag =
                            ensure_list_item_depth_tag(&buffer, list_stack.len().max(1));
                        tag_stack.push(TAG_LIST_ITEM.to_string());
                        tag_stack.push(depth_tag);
                        list_item_stack.push(ListItemRenderState::default());
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
                        insert_blockquote_rail_if_needed(
                            &buffer,
                            &mut iter,
                            &tag_stack,
                            generic_blockquote_depth,
                        );
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
                        if list_item_stack.is_empty() {
                            ensure_rendered_line_break(&buffer, &mut iter);
                            if definition_stack.is_empty() {
                                needs_block_separator = true;
                            } else {
                                mark_current_definition_paragraph_end(&mut definition_stack);
                                needs_block_separator = false;
                            }
                        } else {
                            ensure_rendered_line_break(&buffer, &mut iter);
                            mark_current_list_item_paragraph_end(&mut list_item_stack);
                            needs_block_separator = false;
                        }
                    }
                    TagEnd::BlockQuote(kind) => {
                        if kind.is_some() {
                            pop_tag(&mut tag_stack);
                        } else {
                            pop_tag(&mut tag_stack);
                            pop_tag(&mut tag_stack);
                            generic_blockquote_depth = generic_blockquote_depth.saturating_sub(1);
                        }
                        needs_block_separator = true;
                    }
                    TagEnd::FootnoteDefinition => {
                        pop_tag(&mut tag_stack);
                        needs_block_separator = true;
                    }
                    TagEnd::DefinitionList => {
                        ensure_rendered_line_break(&buffer, &mut iter);
                        needs_block_separator = true;
                    }
                    TagEnd::DefinitionListTitle => {
                        pop_tag(&mut tag_stack);
                        ensure_rendered_line_break(&buffer, &mut iter);
                        needs_block_separator = false;
                    }
                    TagEnd::DefinitionListDefinition => {
                        pop_tag(&mut tag_stack);
                        ensure_rendered_line_break(&buffer, &mut iter);
                        definition_stack.pop();
                        needs_block_separator = false;
                    }
                    TagEnd::List(_) => {
                        list_stack.pop();
                        if list_stack.is_empty() {
                            needs_block_separator = true;
                        } else {
                            mark_current_list_item_content(&mut list_item_stack);
                            needs_block_separator = false;
                        }
                    }
                    TagEnd::Item => {
                        if flush_pending_list_prefix(
                            &buffer,
                            &mut iter,
                            &tag_stack,
                            &mut pending_list_prefix,
                        ) {
                            mark_current_list_item_content(&mut list_item_stack);
                        }
                        pop_tag(&mut tag_stack);
                        pop_tag(&mut tag_stack);
                        ensure_rendered_line_break(&buffer, &mut iter);
                        list_item_stack.pop();
                        if let Some(frame) = list_stack.last_mut() {
                            frame.advance();
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
                    insert_blockquote_rail_if_needed(
                        &buffer,
                        &mut iter,
                        &tag_stack,
                        generic_blockquote_depth,
                    );
                    let tags: Vec<&str> =
                        tag_stack.iter().map(std::string::String::as_str).collect();
                    insert_with_tags(&buffer, &mut iter, &text, &tags);
                    mark_current_list_item_content(&mut list_item_stack);
                    mark_current_definition_content(&mut definition_stack);
                }
                Event::Code(code) => {
                    insert_blockquote_rail_if_needed(
                        &buffer,
                        &mut iter,
                        &tag_stack,
                        generic_blockquote_depth,
                    );
                    let mut tags: Vec<&str> =
                        tag_stack.iter().map(std::string::String::as_str).collect();
                    tags.push(TAG_CODE);
                    insert_with_tags(&buffer, &mut iter, &code, &tags);
                    mark_current_list_item_content(&mut list_item_stack);
                    mark_current_definition_content(&mut definition_stack);
                }
                Event::FootnoteReference(label) => {
                    insert_blockquote_rail_if_needed(
                        &buffer,
                        &mut iter,
                        &tag_stack,
                        generic_blockquote_depth,
                    );
                    let number = footnote_number(
                        &mut footnote_numbers,
                        &mut next_footnote_number,
                        label.as_ref(),
                    );
                    let mut tags: Vec<&str> =
                        tag_stack.iter().map(std::string::String::as_str).collect();
                    tags.push(TAG_FOOTNOTE_REF);
                    insert_with_tags(&buffer, &mut iter, &format!("[{number}]"), &tags);
                    mark_current_list_item_content(&mut list_item_stack);
                    mark_current_definition_content(&mut definition_stack);
                }
                Event::TaskListMarker(checked) => {
                    insert_blockquote_rail_if_needed(
                        &buffer,
                        &mut iter,
                        &tag_stack,
                        generic_blockquote_depth,
                    );
                    insert_task_list_marker(
                        &buffer,
                        &mut iter,
                        &tag_stack,
                        &mut pending_list_prefix,
                        checked,
                    );
                    mark_current_list_item_content(&mut list_item_stack);
                    mark_current_definition_content(&mut definition_stack);
                }
                Event::SoftBreak => {
                    buffer.insert(&mut iter, " ");
                }
                Event::HardBreak => {
                    buffer.insert(&mut iter, "\n");
                    mark_current_list_item_content(&mut list_item_stack);
                    mark_current_definition_content(&mut definition_stack);
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

    /// Invalidate older work and open one new generation-owned render session.
    fn begin_render_session(&self) -> u64 {
        let imp = self.imp();
        self.cancel_pending_markdown_planning();
        self.cancel_queued_image_work();
        let generation = imp.render_session.borrow_mut().begin();
        #[cfg(feature = "test-utils")]
        {
            imp.projection_dispatch_count.set(0);
            imp.projection_high_water_events.set(0);
            imp.image_admission.borrow_mut().reset_high_water();
        }
        generation
    }

    /// Keep one document-sized planner active and one replaceable latest source.
    fn enqueue_markdown_plan(&self, request: PendingMarkdownPlan) {
        let imp = self.imp();
        if imp.planning_worker_running.get() {
            if let Some(cancel) = imp.planning_cancel_token.borrow().as_ref() {
                cancel.store(true, Ordering::Release);
            }
            imp.queued_plan.replace(Some(request));
            return;
        }
        self.spawn_markdown_plan(request);
    }

    fn spawn_markdown_plan(&self, request: PendingMarkdownPlan) {
        let imp = self.imp();
        imp.planning_worker_running.set(true);
        let cancel = Arc::new(AtomicBool::new(false));
        imp.planning_cancel_token.replace(Some(cancel.clone()));
        let PendingMarkdownPlan {
            generation,
            source,
            context,
        } = request;
        spawn_blocking_then_weak(
            self,
            move || {
                #[cfg(feature = "test-utils")]
                std::thread::sleep(std::time::Duration::from_millis(
                    MARKDOWN_PLAN_DELAY_MS.load(Ordering::Acquire),
                ));
                if cancel.load(Ordering::Acquire) {
                    return None;
                }
                match lower_inline_footnotes_cancellable(
                    &source,
                    markdown_render_options(),
                    &cancel,
                ) {
                    InlineFootnoteLowering::Lowered(lowered) => {
                        plan_markdown_cancellable(&lowered, &cancel)
                    }
                    InlineFootnoteLowering::Unchanged => {
                        plan_markdown_cancellable(&source, &cancel)
                    }
                    InlineFootnoteLowering::Limited => {
                        Some(inline_footnote_limited_plan(source.len()))
                    }
                    InlineFootnoteLowering::Cancelled => None,
                }
            },
            move |preview, plan| {
                let imp = preview.imp();
                imp.planning_worker_running.set(false);
                imp.planning_cancel_token.take();
                if let Some(plan) = plan
                    && imp.render_session.borrow().is_current(generation)
                {
                    preview.start_render_plan(generation, plan, context);
                }
                let queued = imp.queued_plan.take();
                if let Some(queued) = queued
                    && imp.render_session.borrow().is_current(queued.generation)
                {
                    preview.spawn_markdown_plan(queued);
                }
            },
        );
    }

    fn cancel_pending_markdown_planning(&self) {
        let imp = self.imp();
        if let Some(cancel) = imp.planning_cancel_token.borrow().as_ref() {
            cancel.store(true, Ordering::Release);
        }
        imp.queued_plan.take();
    }

    /// Accept a current immutable plan and project at most one batch per GTK turn.
    fn start_render_plan(
        &self,
        generation: u64,
        plan: MarkdownRenderPlan,
        context: MarkdownPreviewRenderContext,
    ) {
        if !self.imp().render_session.borrow().is_current(generation) {
            return;
        }
        self.show_content_view();
        self.clear_rendered_state();
        self.imp()
            .render_session
            .borrow_mut()
            .transition(generation, MarkdownRenderState::Projecting);

        let MarkdownRenderPlan {
            batches,
            metrics: _,
            limit,
        } = plan;
        let mut batches = VecDeque::from(batches);
        // Preserve immediate small-document rendering while the planner's
        // event-and-byte ceilings bound this initial GTK turn exactly like
        // every deferred projection slice.
        if let Some(batch) = batches.pop_front() {
            self.apply_render_batch(generation, &batch);
            self.render_event_batch(batch, &context);
        }
        if batches.is_empty() {
            self.finish_render_plan(generation, limit);
            return;
        }
        accessibility::set_description(
            &*self.imp().text_view,
            "Markdown preview projection is pending",
        );

        let preview_weak = self.downgrade();
        glib::idle_add_local(move || {
            let Some(preview) = preview_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if !preview.imp().render_session.borrow().is_current(generation) {
                return glib::ControlFlow::Break;
            }
            if let Some(batch) = batches.pop_front() {
                preview.apply_render_batch(generation, &batch);
                preview.render_event_batch(batch, &context);
            }
            if batches.is_empty() {
                preview.finish_render_plan(generation, limit);
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    /// Record direct slice evidence before applying a current batch.
    fn apply_render_batch(&self, generation: u64, _batch: &MarkdownEventBatch) {
        debug_assert!(self.imp().render_session.borrow().is_current(generation));
        #[cfg(feature = "test-utils")]
        {
            let imp = self.imp();
            imp.projection_dispatch_count
                .set(imp.projection_dispatch_count.get().wrapping_add(1));
            imp.projection_high_water_events
                .set(imp.projection_high_water_events.get().max(_batch.len()));
        }
    }

    /// Publish the complete or explicit limited terminal for the current generation.
    fn finish_render_plan(&self, generation: u64, limit: Option<MarkdownPlanLimit>) {
        let imp = self.imp();
        if !imp.render_session.borrow().is_current(generation) {
            return;
        }
        if let Some(limit) = limit {
            let description = limit.description();
            let buffer = imp.text_view.buffer();
            let mut end = buffer.end_iter();
            if end.offset() > 0 {
                buffer.insert(&mut end, "\n\n");
            }
            buffer.insert(&mut end, description);
            accessibility::set_description(&*imp.text_view, description);
            imp.render_session
                .borrow_mut()
                .transition(generation, MarkdownRenderState::Limited);
        } else {
            accessibility::set_description(&*imp.text_view, "Rendered Markdown preview");
            imp.render_session
                .borrow_mut()
                .transition(generation, MarkdownRenderState::Complete);
        }
        self.queue_code_block_width_refresh();
    }

    /// Cancel current planning/projection work without retaining stale payloads.
    fn cancel_render_session(&self) {
        self.cancel_pending_markdown_planning();
        self.cancel_queued_image_work();
        self.imp().render_session.borrow_mut().cancel();
    }

    /// Whether current planning/projection blocks exact preview readiness.
    #[must_use]
    pub fn render_pending(&self) -> bool {
        self.imp().render_session.borrow().pending()
            || self.imp().planning_worker_running.get()
            || self.imp().queued_plan.borrow().is_some()
            || self.imp().current_image_work_count.get() > 0
            || self.imp().retirement.borrow().is_some()
    }

    /// Current renderer state for deterministic widget and automation tests.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn render_state_for_test(&self) -> MarkdownRenderState {
        self.imp().render_session.borrow().state()
    }

    /// Direct projection-slice counters for boundedness assertions.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn projection_counters_for_test(&self) -> (u64, usize) {
        let imp = self.imp();
        (
            imp.projection_dispatch_count.get(),
            imp.projection_high_water_events.get(),
        )
    }

    /// Direct one-active-plus-latest planning ownership counters.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn planning_counters_for_test(&self) -> (usize, usize) {
        let imp = self.imp();
        (
            usize::from(imp.planning_worker_running.get()),
            usize::from(imp.queued_plan.borrow().is_some()),
        )
    }

    /// Delay Markdown planning workers for deterministic supersession tests.
    #[cfg(feature = "test-utils")]
    pub fn set_markdown_plan_delay_for_test(delay_ms: u64) {
        MARKDOWN_PLAN_DELAY_MS.store(delay_ms, Ordering::Release);
    }

    /// Direct detached-render retirement high-water counters.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn retirement_counters_for_test(&self) -> (usize, usize) {
        let imp = self.imp();
        (
            imp.retirement_chars_high_water.get(),
            imp.retirement_items_high_water.get(),
        )
    }

    /// Direct image ownership counters for count/byte bound assertions.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn image_admission_counters_for_test(&self) -> (usize, u64, usize, u64) {
        let snapshot = self.imp().image_admission.borrow().snapshot();
        (
            snapshot.owned_count,
            snapshot.owned_bytes,
            snapshot.high_water_count,
            snapshot.high_water_bytes,
        )
    }

    /// Configured image count and conservative-byte ceilings.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn image_admission_limits_for_test() -> (usize, u64) {
        (MAX_PREVIEW_IMAGE_WORK_ITEMS, MAX_PREVIEW_IMAGE_WORK_BYTES)
    }

    /// Per-file source byte and decoded source-pixel ceilings.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn image_source_limits_for_test() -> (u64, i64) {
        (
            MAX_PREVIEW_IMAGE_SOURCE_BYTES,
            MAX_PREVIEW_IMAGE_SOURCE_PIXELS,
        )
    }

    /// Delay image workers so stale-generation completion is deterministic.
    #[cfg(feature = "test-utils")]
    pub fn set_image_work_delay_for_test(delay_ms: u64) {
        IMAGE_WORK_DELAY_MS.store(delay_ms, Ordering::Release);
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

    /// Pin the preview scroller to one natural size for compact embedded uses.
    pub(crate) fn set_scroller_content_size(&self, width: i32, height: i32) {
        let scroller = self.imp().scrolled_window.get();
        scroller.set_min_content_width(width);
        scroller.set_max_content_width(width);
        scroller.set_min_content_height(height);
        scroller.set_max_content_height(height);
        scroller.set_propagate_natural_width(false);
        scroller.set_propagate_natural_height(false);
    }

    /// Apply or clear Focus Mode readable-column margins for rendered Markdown.
    pub(crate) fn set_focus_mode_readable_column(&self, active: bool, target_columns: u32) {
        let text_view = self.text_view();
        if active {
            let margin = readable_column_margin(
                text_view.width(),
                approximate_char_width(text_view.upcast_ref::<gtk4::Widget>()),
                target_columns,
            );
            text_view.set_left_margin(margin);
            text_view.set_right_margin(margin);
        } else {
            text_view.set_left_margin(16);
            text_view.set_right_margin(16);
        }
        self.queue_code_block_width_refresh();
    }

    #[must_use]
    pub fn content_margins(&self) -> (i32, i32) {
        let text_view = self.text_view();
        (text_view.left_margin(), text_view.right_margin())
    }

    /// Return the current document-surface opacity used by the preview.
    ///
    /// Widget tests use this to verify that preview mode tracks the same
    /// transparency preference as the editor surface.
    #[must_use]
    pub fn background_opacity(&self) -> f64 {
        gtk4::gio::Settings::new(crate::config::APP_ID)
            .double(crate::config::keys::TAB_CONTENT_OPACITY)
            .clamp(0.0, 1.0)
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
        self.cancel_render_session();
        let imp = self.imp();
        imp.placeholder.set_description(Some(description));
        imp.scrolled_window.set_visible(false);
        imp.placeholder.set_visible(true);
        accessibility::set_description(&*imp.placeholder, description);
        accessibility::set_hidden(&*imp.scrolled_window, true);
        accessibility::set_hidden(&*imp.text_view, true);
        accessibility::set_hidden(&*imp.placeholder, false);
        self.clear_rendered_state();
        imp.showing_content.set(false);
    }

    /// Show placeholder copy inside the rendered text surface.
    ///
    /// Note editors use this while their Render page is hidden inside a
    /// `GtkStack`: the final scrolled text surface must be part of the first
    /// measurement pass, otherwise a later placeholder-to-content swap can make
    /// the surrounding dialog resize by a pixel when Render is clicked.
    pub(crate) fn show_content_placeholder(&self, description: &str) {
        self.cancel_render_session();
        self.show_content_view();
        self.clear_rendered_state();
        self.imp().text_view.buffer().set_text(description);
    }

    /// Clear the rendered content without showing the placeholder.
    pub fn clear(&self) {
        self.cancel_render_session();
        self.clear_rendered_state();
    }

    /// Render an accessible terminal when a caller cannot produce a plan.
    pub fn show_render_failure(&self, description: &str) {
        self.cancel_render_session();
        self.show_content_view();
        self.clear_rendered_state();
        self.imp().text_view.buffer().set_text(description);
        accessibility::set_description(&*self.imp().text_view, description);
        let mut session = self.imp().render_session.borrow_mut();
        let generation = session.generation();
        session.transition(generation, MarkdownRenderState::Failed);
    }

    /// Whether the widget is currently showing rendered Markdown content.
    #[must_use]
    pub fn is_showing_content(&self) -> bool {
        self.imp().showing_content.get()
    }

    /// Current placeholder description, exposed only for widget assertions.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn placeholder_description_for_test(&self) -> Option<String> {
        self.imp()
            .placeholder
            .description()
            .map(|description| description.to_string())
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
            accessibility::set_hidden(&*imp.scrolled_window, false);
            accessibility::set_hidden(&*imp.text_view, false);
            accessibility::set_hidden(&*imp.placeholder, true);
            imp.showing_content.set(true);
        }
    }

    /// Detach the visible buffer in O(1) and retire its payload in bounded turns.
    fn clear_rendered_state(&self) {
        let imp = self.imp();
        let old_buffer = imp.text_view.buffer();
        let new_buffer = gtk4::TextBuffer::new(None::<&gtk4::TextTagTable>);
        imp::create_or_update_tags(&new_buffer, libadwaita::StyleManager::default().is_dark());
        imp.text_view.set_buffer(Some(&new_buffer));

        let retired = RetiredMarkdownRender {
            buffer: old_buffer,
            embeds: std::mem::take(&mut *imp.rendered_embeds.borrow_mut()).into(),
            links: std::mem::take(&mut *imp.text_link_targets.borrow_mut()).into(),
        };
        if !retired.is_empty() {
            imp.retirement
                .borrow_mut()
                .get_or_insert_with(MarkdownRetirementSession::default)
                .states
                .push_back(retired);
            self.arm_markdown_retirement();
        }
        self.advance_rendered_embed_generation();
    }

    /// Keep exactly one idle source draining detached render state.
    fn arm_markdown_retirement(&self) {
        let imp = self.imp();
        if imp.retirement_armed.replace(true) {
            return;
        }
        let preview_weak = self.downgrade();
        glib::idle_add_local(move || {
            let Some(preview) = preview_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if preview.retire_markdown_slice() {
                glib::ControlFlow::Continue
            } else {
                preview.imp().retirement_armed.set(false);
                glib::ControlFlow::Break
            }
        });
    }

    /// Release a bounded amount of detached text, widgets, and links.
    fn retire_markdown_slice(&self) -> bool {
        let imp = self.imp();
        let mut session_slot = imp.retirement.borrow_mut();
        let Some(session) = session_slot.as_mut() else {
            return false;
        };
        let Some(state) = session.states.front_mut() else {
            session_slot.take();
            return false;
        };

        let mut retired_items = 0usize;
        while retired_items < MARKDOWN_RETIREMENT_ITEMS_PER_TURN {
            let widget = state.embeds.pop_front().map(|embed| embed.widget);
            if let Some(widget) = widget {
                if widget.parent().as_ref() == Some(imp.text_view.upcast_ref()) {
                    imp.text_view.remove(&widget);
                }
                retired_items += 1;
                continue;
            }
            if state.links.pop_front().is_some() {
                retired_items += 1;
                continue;
            }
            break;
        }

        let char_count = usize::try_from(state.buffer.char_count()).unwrap_or_default();
        let retired_chars = char_count.min(MARKDOWN_RETIREMENT_CHARS_PER_TURN);
        if retired_chars > 0 {
            let mut end = state.buffer.end_iter();
            let start_offset = end
                .offset()
                .saturating_sub(i32::try_from(retired_chars).unwrap_or(i32::MAX));
            let mut start = state.buffer.iter_at_offset(start_offset);
            state.buffer.delete(&mut start, &mut end);
        }

        #[cfg(feature = "test-utils")]
        {
            imp.retirement_chars_high_water
                .set(imp.retirement_chars_high_water.get().max(retired_chars));
            imp.retirement_items_high_water
                .set(imp.retirement_items_high_water.get().max(retired_items));
        }

        if state.is_empty() {
            session.states.pop_front();
        }
        if session.states.is_empty() {
            session_slot.take();
            false
        } else {
            true
        }
    }

    /// Insert one buffered table as a native GTK grid anchored into the text flow.
    fn insert_table_widget(
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

    /// Insert one buffered Markdown code block into the preview flow.
    fn insert_code_block_widget(
        &self,
        buffer: &gtk4::TextBuffer,
        iter: &mut gtk4::TextIter,
        code_block: &BufferedCodeBlock,
        theme: &CodeBlockTheme,
        layout: EmbeddedBlockLayout,
    ) {
        let widget = build_code_block_widget(code_block, theme);
        widget.set_margin_start(layout.margin_start);
        widget.set_margin_end(layout.margin_end);
        self.insert_embedded_widget(buffer, iter, widget.upcast_ref::<gtk4::Widget>(), layout);
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
            ResolvedImageTarget::LocalFile(path) => {
                self.insert_async_image_placeholder(
                    buffer,
                    iter,
                    &image.destination,
                    vec![path],
                    EmbeddedBlockLayout::default(),
                );
            }
            ResolvedImageTarget::OrderedCandidates(paths) => {
                self.insert_async_image_placeholder(
                    buffer,
                    iter,
                    &image.destination,
                    paths,
                    EmbeddedBlockLayout::default(),
                );
            }
            ResolvedImageTarget::Fallback { title, body } => {
                let widget = build_image_fallback_widget(title, &body);
                self.insert_embedded_widget(
                    buffer,
                    iter,
                    widget.upcast_ref::<gtk4::Widget>(),
                    EmbeddedBlockLayout::default(),
                );
            }
        }
    }

    /// Insert a placeholder while local image decode runs off the GTK thread.
    fn insert_async_image_placeholder(
        &self,
        buffer: &gtk4::TextBuffer,
        iter: &mut gtk4::TextIter,
        raw_target: &str,
        paths: Vec<PathBuf>,
        layout: EmbeddedBlockLayout,
    ) {
        let generation = self.imp().render_session.borrow().generation();
        if !self.imp().image_admission.borrow_mut().try_admit(
            PREVIEW_IMAGE_WORK_CHARGE_BYTES,
            MAX_PREVIEW_IMAGE_WORK_ITEMS,
            MAX_PREVIEW_IMAGE_WORK_BYTES,
        ) {
            let fallback = build_image_fallback_widget(
                "Image preview limited",
                "Only four local images are loaded automatically per render",
            );
            self.insert_embedded_widget(buffer, iter, &fallback, layout);
            return;
        }

        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        container.append(&build_image_fallback_widget("Loading image", raw_target));
        self.insert_embedded_widget(buffer, iter, container.upcast_ref::<gtk4::Widget>(), layout);

        let imp = self.imp();
        imp.current_image_work_count
            .set(imp.current_image_work_count.get().saturating_add(1));
        imp.image_queue.borrow_mut().push_back(PendingImageWork {
            generation,
            raw_target: raw_target.to_string(),
            paths,
            container: container.downgrade(),
            charge_bytes: PREVIEW_IMAGE_WORK_CHARGE_BYTES,
        });
        self.start_next_image_work();
    }

    /// Start at most one decoder while queued descriptors remain payload-free.
    fn start_next_image_work(&self) {
        let imp = self.imp();
        if imp.active_image.borrow().is_some() {
            return;
        }
        let Some(work) = imp.image_queue.borrow_mut().pop_front() else {
            return;
        };
        let PendingImageWork {
            generation,
            raw_target,
            paths,
            container,
            charge_bytes,
        } = work;
        imp.active_image.replace(Some(ActiveImageWork {
            generation,
            container,
            charge_bytes,
        }));
        spawn_blocking_then_weak(
            self,
            move || first_loadable_ordered_image(raw_target, paths),
            move |preview, result| preview.finish_image_work(result),
        );
    }

    /// Release exact scalar ownership and apply only a current image completion.
    fn finish_image_work(&self, result: OrderedImageCandidateResult) {
        let imp = self.imp();
        let Some(active) = imp.active_image.take() else {
            return;
        };
        imp.image_admission
            .borrow_mut()
            .release(active.charge_bytes);
        if imp.render_session.borrow().is_current(active.generation) {
            imp.current_image_work_count
                .set(imp.current_image_work_count.get().saturating_sub(1));
            if let Some(container) = active.container.upgrade() {
                Self::replace_ordered_image_placeholder(&container, result);
            }
        }
        self.start_next_image_work();
    }

    /// Release queued descriptor ownership while an active worker drains safely.
    fn cancel_queued_image_work(&self) {
        let imp = self.imp();
        let queued = imp.image_queue.borrow_mut().drain(..).collect::<Vec<_>>();
        for work in queued {
            imp.image_admission.borrow_mut().release(work.charge_bytes);
        }
        imp.current_image_work_count.set(0);
    }

    /// Replace an async image placeholder with the resolved image or fallback.
    fn replace_ordered_image_placeholder(
        container: &gtk4::Box,
        result: OrderedImageCandidateResult,
    ) {
        clear_box_children(container);
        match result {
            OrderedImageCandidateResult::Loadable { path, image } => {
                container.append(&build_decoded_image_widget(&path, image));
            }
            OrderedImageCandidateResult::Unloadable { path, error } => {
                container.append(&build_image_fallback_widget(
                    "Image could not be loaded",
                    &format!("{}\n{error}", path.display()),
                ));
            }
            OrderedImageCandidateResult::Missing { raw_target } => {
                container.append(&build_image_fallback_widget(
                    "Image file not found",
                    &raw_target,
                ));
            }
        }
    }

    /// Insert one already-built GTK widget into the preview text flow.
    fn insert_embedded_widget(
        &self,
        buffer: &gtk4::TextBuffer,
        iter: &mut gtk4::TextIter,
        widget: &gtk4::Widget,
        layout: EmbeddedBlockLayout,
    ) {
        let anchor = buffer.create_child_anchor(iter);
        self.imp().text_view.add_child_at_anchor(widget, &anchor);
        self.imp()
            .rendered_embeds
            .borrow_mut()
            .push(RenderedEmbed::new(widget.clone(), layout));
        self.advance_rendered_embed_generation();
    }

    fn advance_rendered_embed_generation(&self) {
        let imp = self.imp();
        imp.rendered_embed_generation
            .set(imp.rendered_embed_generation.get().wrapping_add(1));
    }

    /// Refresh anchored code blocks after GTK has allocated the preview text view.
    ///
    /// `GtkTextView` child anchors do not expand anchored widgets to the text
    /// column automatically, so code-block containers need an explicit width
    /// request based on the current visible text column.
    fn refresh_code_block_widths(&self) {
        let Some(column_width) = preview_text_column_width(&self.imp().text_view.get()) else {
            return;
        };
        let embed_generation = self.imp().rendered_embed_generation.get();
        if self.imp().last_code_block_layout.get() == Some((column_width, embed_generation)) {
            return;
        }

        #[cfg(feature = "test-utils")]
        self.imp().code_block_width_traversal_count.set(
            self.imp()
                .code_block_width_traversal_count
                .get()
                .wrapping_add(1),
        );

        let mut changed = false;
        for embed in self.imp().rendered_embeds.borrow().iter() {
            if embed.widget.has_css_class("markdown-code-block") {
                let width = embed.layout.code_block_width(column_width);
                if embed.widget.width_request() != width {
                    embed.widget.set_width_request(width);
                    embed.widget.queue_resize();
                    changed = true;
                }
            }
        }

        if changed {
            self.imp().text_view.queue_resize();
            self.queue_resize();
        }
        self.imp()
            .last_code_block_layout
            .set(Some((column_width, embed_generation)));
    }

    /// Return the number of full embed traversals for performance assertions.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn code_block_width_traversal_count_for_test(&self) -> u64 {
        self.imp().code_block_width_traversal_count.get()
    }

    /// Reset only the performance counter without changing layout cache state.
    #[cfg(feature = "test-utils")]
    pub fn reset_code_block_width_traversal_count_for_test(&self) {
        self.imp().code_block_width_traversal_count.set(0);
    }

    /// Queue the production deferred repair sequence for widget assertions.
    #[cfg(feature = "test-utils")]
    pub fn queue_code_block_width_refresh_for_test<F: Fn() + 'static>(&self, callback: F) {
        self.queue_code_block_width_refresh_after(callback);
    }

    /// Refresh code-block widths across the current GTK layout turn.
    pub(super) fn queue_code_block_width_refresh(&self) {
        let generation = self
            .imp()
            .code_block_refresh_generation
            .get()
            .wrapping_add(1);
        self.imp().code_block_refresh_generation.set(generation);
        self.refresh_code_block_widths();
        self.replace_deferred_code_block_width_refresh(generation);
    }

    /// Refresh code-block widths and run `callback` after the final deferred pass.
    pub(crate) fn queue_code_block_width_refresh_after<F: Fn() + 'static>(&self, callback: F) {
        self.imp()
            .code_block_refresh_completion_callbacks
            .borrow_mut()
            .push(Box::new(callback));
        self.queue_code_block_width_refresh();
    }

    /// Replace any queued deferred refresh with the latest layout generation.
    fn replace_deferred_code_block_width_refresh(&self, generation: u32) {
        if let Some(source_id) = self.imp().code_block_idle_source_id.take() {
            source_id.remove();
        }
        if let Some(source_id) = self.imp().code_block_timeout_source_id.take() {
            source_id.remove();
        }

        let idle_preview_weak = self.downgrade();
        let idle_source_id = glib::idle_add_local_once(move || {
            let Some(preview) = idle_preview_weak.upgrade() else {
                return;
            };

            let _ = preview.imp().code_block_idle_source_id.take();
            if preview.imp().code_block_refresh_generation.get() == generation {
                preview.refresh_code_block_widths();
            }
        });
        let _ = self
            .imp()
            .code_block_idle_source_id
            .replace(Some(idle_source_id));

        // `GtkPaned` and `GtkTextView` can settle their final child-anchor
        // column one frame after the idle pass in Fedora's headless CI stack.
        // A short replaceable timer keeps production preview geometry honest
        // without accumulating stale callbacks during active resizing.
        let timed_preview_weak = self.downgrade();
        let timeout_source_id =
            glib::timeout_add_local_once(std::time::Duration::from_millis(50), move || {
                let Some(preview) = timed_preview_weak.upgrade() else {
                    return;
                };

                let _ = preview.imp().code_block_timeout_source_id.take();
                if preview.imp().code_block_refresh_generation.get() == generation {
                    preview.refresh_code_block_widths();
                    let callbacks = preview.imp().code_block_refresh_completion_callbacks.take();
                    for callback in callbacks {
                        callback();
                    }
                }
            });
        let _ = self
            .imp()
            .code_block_timeout_source_id
            .replace(Some(timeout_source_id));
    }

    /// Recheck embedded code-block widths after an outer preview-shell transition.
    ///
    /// The main window can reveal the preview from a hidden Adwaita slot or move
    /// it into a different layout after Markdown has already rendered. Calling
    /// this at shell boundaries keeps child-anchor code blocks tied to the final
    /// text column rather than to an intermediate allocation.
    pub(crate) fn refresh_embedded_code_block_layouts(&self) {
        self.queue_code_block_width_refresh();
    }
}

impl DecodedImage {
    /// Decode and scale one local image on a worker thread.
    fn from_path(path: &Path) -> Result<Self, String> {
        let bytes = read_preview_image_bytes(path)?;
        let pixbuf = decode_preview_pixbuf_from_bytes(&bytes)?;
        let (display_width, display_height) = bounded_image_size(pixbuf.width(), pixbuf.height());
        let pixbuf = if display_width != pixbuf.width() || display_height != pixbuf.height() {
            pixbuf
                .scale_simple(
                    display_width,
                    display_height,
                    gtk4::gdk_pixbuf::InterpType::Bilinear,
                )
                .ok_or_else(|| "failed to scale image for preview".to_string())?
        } else {
            pixbuf
        };
        let channels = pixbuf.n_channels();
        if channels != 3 && channels != 4 {
            return Err(format!("unsupported image channel count: {channels}"));
        }
        let stride = usize::try_from(pixbuf.rowstride())
            .map_err(|_| "invalid image rowstride".to_string())?;

        Ok(Self {
            width: pixbuf.width(),
            height: pixbuf.height(),
            stride,
            has_alpha: channels == 4,
            pixels: pixbuf.read_pixel_bytes().as_ref().to_vec(),
        })
    }
}

fn read_preview_image_bytes(path: &Path) -> Result<Vec<u8>, String> {
    read_preview_image_bytes_with_limit(path, MAX_PREVIEW_IMAGE_SOURCE_BYTES, || {})
}

fn read_preview_image_bytes_with_limit<F>(
    path: &Path,
    byte_limit: u64,
    after_facts: F,
) -> Result<Vec<u8>, String>
where
    F: FnOnce(),
{
    let facts = fs_metadata::file_facts(path).map_err(|error| error.to_string())?;
    if facts.byte_size > byte_limit {
        return Err(format!(
            "image is too large for preview ({} bytes, limit {} bytes)",
            facts.byte_size, byte_limit
        ));
    }
    after_facts();
    let bytes =
        fs_read::bounded_bytes(path, byte_limit, facts.byte_size, || false).map_err(|error| {
            match error {
                fs_read::BoundedFileReadError::LimitExceeded { .. } => {
                    format!("image grew beyond the {byte_limit}-byte preview limit")
                }
                fs_read::BoundedFileReadError::Cancelled => {
                    "image preview read was cancelled".to_string()
                }
                fs_read::BoundedFileReadError::Io(source) => source.to_string(),
            }
        })?;
    let current = fs_metadata::file_facts(path).map_err(|error| error.to_string())?;
    if current.identity != facts.identity
        || current.byte_size != facts.byte_size
        || current.modified_at_nanos != facts.modified_at_nanos
    {
        return Err("image changed while it was being read for preview".to_string());
    }
    Ok(bytes)
}

/// Decode one preview image from already-boundary-read bytes.
fn decode_preview_pixbuf_from_bytes(bytes: &[u8]) -> Result<gtk4::gdk_pixbuf::Pixbuf, String> {
    let loader = gtk4::gdk_pixbuf::PixbufLoader::new();
    let source_too_large = std::rc::Rc::new(std::cell::Cell::new(false));
    let source_too_large_for_signal = source_too_large.clone();
    loader.connect_size_prepared(move |loader, width, height| {
        let source_pixels = i64::from(width).saturating_mul(i64::from(height));
        if source_pixels > MAX_PREVIEW_IMAGE_SOURCE_PIXELS {
            source_too_large_for_signal.set(true);
            // The loader still needs a legal target size before `close()`, but
            // the result will be rejected; 1x1 avoids allocating the source.
            loader.set_size(1, 1);
            return;
        }

        let (display_width, display_height) = bounded_image_size(width, height);
        loader.set_size(display_width, display_height);
    });
    loader.write(bytes).map_err(|error| error.to_string())?;
    loader.close().map_err(|error| error.to_string())?;

    if source_too_large.get() {
        return Err(format!(
            "image dimensions exceed preview limit ({MAX_PREVIEW_IMAGE_SOURCE_PIXELS} pixels)"
        ));
    }

    loader
        .pixbuf()
        .ok_or_else(|| "image loader did not produce a pixbuf".to_string())
}

impl Default for LushtextMarkdownPreview {
    fn default() -> Self {
        Self::new()
    }
}

/// Derive the effective text column for an embedded block from active Markdown state.
fn embedded_block_layout(
    tag_stack: &[String],
    list_stack: &[ListFrame],
    list_item_stack: &[ListItemRenderState],
    generic_blockquote_depth: usize,
    definition_stack: &[DefinitionRenderState],
) -> EmbeddedBlockLayout {
    let mut layout = EmbeddedBlockLayout::default();

    if !definition_stack.is_empty() {
        layout.include_margin(DEFINITION_DEF_LEFT_MARGIN, DEFINITION_DEF_RIGHT_MARGIN);
    }

    if !list_item_stack.is_empty() {
        layout.include_margin(list_item_text_margin(list_stack.len().max(1)), 0);
    }

    if generic_blockquote_depth > 0 {
        layout.include_margin(blockquote_left_margin(generic_blockquote_depth), 0);
    }

    if tag_stack.iter().any(|tag| tag == TAG_ALERT_BODY) {
        layout.include_margin(ALERT_BODY_LEFT_MARGIN, ALERT_BODY_RIGHT_MARGIN);
    }

    if tag_stack.iter().any(|tag| tag == TAG_FOOTNOTE_DEF) {
        layout.include_margin(FOOTNOTE_DEF_LEFT_MARGIN, FOOTNOTE_DEF_RIGHT_MARGIN);
    }

    layout
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

/// Insert the visible generic blockquote rail when the next rendered content
/// starts a quoted line.
///
/// The rail carries only quote-structure tags so a line that starts with
/// emphasis or a link does not make the structural rail look like inline text.
fn insert_blockquote_rail_if_needed(
    buffer: &gtk4::TextBuffer,
    iter: &mut gtk4::TextIter,
    tag_stack: &[String],
    depth: usize,
) {
    if depth == 0 || !iter.starts_line() {
        return;
    }

    let tags: Vec<&str> = tag_stack
        .iter()
        .map(std::string::String::as_str)
        .filter(|name| *name == TAG_BLOCKQUOTE || name.starts_with("blockquote-depth-"))
        .collect();
    insert_with_tags(buffer, iter, &blockquote_rail_prefix(depth), &tags);
}

/// Insert one newline only when the current rendered position is mid-row.
fn ensure_rendered_line_break(buffer: &gtk4::TextBuffer, iter: &mut gtk4::TextIter) {
    if iter.offset() > 0 && !iter.starts_line() {
        buffer.insert(iter, "\n");
    }
}

/// Mark the current list item as having emitted visible content.
fn mark_current_list_item_content(items: &mut [ListItemRenderState]) {
    if let Some(item) = items.last_mut() {
        item.has_content = true;
    }
}

/// Record that a paragraph ended inside the current list item.
fn mark_current_list_item_paragraph_end(items: &mut [ListItemRenderState]) {
    if let Some(item) = items.last_mut() {
        item.paragraph_ended = true;
    }
}

/// Clear the pending loose-list paragraph separator for the current item.
fn clear_current_list_item_paragraph_end(items: &mut [ListItemRenderState]) {
    if let Some(item) = items.last_mut() {
        item.paragraph_ended = false;
    }
}

/// Return whether the next paragraph in this list item should be separated.
fn current_list_item_needs_paragraph_separator(items: &[ListItemRenderState]) -> bool {
    items
        .last()
        .is_some_and(|item| item.has_content && item.paragraph_ended)
}

/// Mark the current definition as having emitted visible content.
fn mark_current_definition_content(definitions: &mut [DefinitionRenderState]) {
    if let Some(definition) = definitions.last_mut() {
        definition.has_content = true;
    }
}

/// Record that a paragraph ended inside the current definition body.
fn mark_current_definition_paragraph_end(definitions: &mut [DefinitionRenderState]) {
    if let Some(definition) = definitions.last_mut() {
        definition.paragraph_ended = true;
    }
}

/// Clear the pending loose-definition paragraph separator for the current body.
fn clear_current_definition_paragraph_end(definitions: &mut [DefinitionRenderState]) {
    if let Some(definition) = definitions.last_mut() {
        definition.paragraph_ended = false;
    }
}

/// Return whether the next paragraph in this definition should be separated.
fn current_definition_needs_paragraph_separator(definitions: &[DefinitionRenderState]) -> bool {
    definitions
        .last()
        .is_some_and(|definition| definition.has_content && definition.paragraph_ended)
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
) -> bool {
    let Some(prefix) = pending_list_prefix.take() else {
        return false;
    };

    let tags: Vec<&str> = tag_stack.iter().map(std::string::String::as_str).collect();
    insert_with_tags(buffer, iter, &prefix, &tags);
    true
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

/// Return the current Markdown text column width inside the preview text view.
fn preview_text_column_width(text_view: &gtk4::TextView) -> Option<i32> {
    let width = text_view.width();
    if width <= 0 {
        return None;
    }

    let column_width = width.saturating_sub(text_view.left_margin() + text_view.right_margin());
    (column_width > 0).then_some(column_width)
}

/// Resolve one code-block language hint using IDs, common aliases, and filename guessing.
fn resolve_code_block_language_hint(raw_hint: &str) -> Option<sourceview5::Language> {
    let hint = normalize_code_block_language_hint(raw_hint)?;
    let manager = sourceview5::LanguageManager::default();
    if let Some(language) = manager.language(&hint) {
        return Some(language);
    }

    let alias = code_block_language_alias(&hint);
    if alias != hint
        && let Some(language) = manager.language(alias)
    {
        return Some(language);
    }

    let filename = format!("sample.{alias}");
    manager
        .guess_language(Some(Path::new(&filename)), None)
        .or_else(|| manager.guess_language(Some(Path::new(&format!("sample.{hint}"))), None))
}

/// Normalize Markdown renderer language classes and casing into source IDs.
fn normalize_code_block_language_hint(raw_hint: &str) -> Option<String> {
    let hint = raw_hint.trim().trim_start_matches("language-").trim();
    if hint.is_empty() {
        None
    } else {
        Some(hint.to_ascii_lowercase())
    }
}

/// Map common Markdown fence aliases to GtkSourceView language IDs.
fn code_block_language_alias(hint: &str) -> &str {
    match hint {
        "bash" | "zsh" | "shell" => "sh",
        "cjs" | "js" | "mjs" => "javascript",
        "py" => "python3",
        "rs" => "rust",
        "ts" => "typescript",
        other => other,
    }
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
            return Some(PreviewLaunchTarget {
                uri: raw_target.to_string(),
                local_path: Some(path),
            });
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
                Some(path) => ResolvedImageTarget::LocalFile(path),
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

    match resolve_local_image_path(raw_target, context) {
        ImagePathResolution::Resolved(path) => ResolvedImageTarget::LocalFile(path),
        ImagePathResolution::OrderedCandidates(paths) => {
            ResolvedImageTarget::OrderedCandidates(paths)
        }
        ImagePathResolution::Missing => ResolvedImageTarget::Fallback {
            title: "Image file not found",
            body: raw_target.to_string(),
        },
    }
}

/// Result of resolving a Markdown image's local filesystem target.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ImagePathResolution {
    /// One direct file-relative, absolute, or URI-backed path should be tried.
    Resolved(PathBuf),
    /// Relative candidates should be checked and decoded in the listed scope order.
    OrderedCandidates(Vec<PathBuf>),
    /// No file-relative or workspace-relative candidate can be formed.
    Missing,
}

/// Resolve one Markdown image path through file-relative and ordered workspace candidates.
fn resolve_local_image_path(
    raw_target: &str,
    context: &MarkdownPreviewRenderContext,
) -> ImagePathResolution {
    let path = Path::new(raw_target);
    if path.is_absolute() {
        return ImagePathResolution::Resolved(path.to_path_buf());
    }

    let mut candidates = Vec::new();
    if let Some(document_path) = &context.document_path
        && let Some(parent) = document_path.parent()
    {
        candidates.push(parent.join(path));
    }
    candidates.extend(
        context
            .workspace_folders
            .iter()
            .map(|folder| folder.join(path)),
    );

    match candidates.len() {
        0 => ImagePathResolution::Missing,
        1 if context.workspace_folders.is_empty() => {
            ImagePathResolution::Resolved(candidates.pop().expect("one candidate exists"))
        }
        _ => ImagePathResolution::OrderedCandidates(candidates),
    }
}

/// Resolve one Markdown local path against the current document and workspace folders.
fn resolve_local_path(
    raw_target: &str,
    context: &MarkdownPreviewRenderContext,
) -> LocalPathResolution {
    let path = Path::new(raw_target);
    if path.is_absolute() {
        return LocalPathResolution::Resolved(path.to_path_buf());
    }

    if let Some(document_path) = &context.document_path
        && let Some(parent) = document_path.parent()
    {
        return LocalPathResolution::Resolved(parent.join(path));
    }

    let candidates = context
        .workspace_folders
        .iter()
        .map(|folder| folder.join(path))
        .collect::<Vec<_>>();

    match candidates.len() {
        0 => LocalPathResolution::Missing,
        1 => LocalPathResolution::Resolved(
            candidates.into_iter().next().expect("one candidate exists"),
        ),
        _ => LocalPathResolution::Ambiguous(candidates),
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

/// Build one native source-view widget for a buffered Markdown code block.
fn build_code_block_widget(code_block: &BufferedCodeBlock, theme: &CodeBlockTheme) -> gtk4::Widget {
    if code_block.exceeds_preview_widget_budget() {
        return build_preview_limit_fallback_widget(
            "Code block not rendered",
            &format!(
                "This code block is {} bytes; the preview renders highlighted code blocks up to {} bytes.",
                code_block.source_byte_len(),
                MAX_PREVIEW_CODE_BLOCK_BYTES
            ),
            "markdown-code-block-fallback",
        );
    }

    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    container.set_margin_top(8);
    container.set_margin_bottom(8);
    container.set_hexpand(true);
    container.set_halign(gtk4::Align::Fill);
    container.add_css_class("markdown-code-block");
    let language_hint = code_block.language_hint().unwrap_or("plain text");
    let code_block_label = format!("Markdown {language_hint} code block");
    accessibility::set_role(&container, gtk4::AccessibleRole::Group);
    accessibility::set_labelled_description(
        &container,
        &code_block_label,
        "Read-only code block embedded in the rendered Markdown preview",
    );

    let source_buffer = sourceview5::Buffer::new(None);
    let language = code_block
        .language_hint()
        .and_then(resolve_code_block_language_hint);
    source_buffer.set_language(language.as_ref());
    source_buffer.set_highlight_syntax(language.is_some());
    source_buffer.set_style_scheme(theme.style_scheme.as_ref());
    source_buffer.set_text(&code_block.text);

    let source_view = sourceview5::View::with_buffer(&source_buffer);
    source_view.set_editable(false);
    source_view.set_cursor_visible(false);
    source_view.set_show_line_numbers(false);
    source_view.set_highlight_current_line(false);
    source_view.set_monospace(true);
    source_view.set_wrap_mode(gtk4::WrapMode::None);
    source_view.set_left_margin(0);
    source_view.set_right_margin(0);
    source_view.set_top_margin(0);
    source_view.set_bottom_margin(0);
    source_view.set_hexpand(true);
    source_view.set_halign(gtk4::Align::Fill);
    source_view.add_css_class("monospace");
    source_view.add_css_class("markdown-code-block-view");
    // GtkSourceView already exposes a text-box role; assigning it again is a
    // GTK critical, so this projection only supplies the code-block name/state.
    accessibility::set_labelled_description(
        &source_view,
        &code_block_label,
        "Read-only source text for this Markdown code block",
    );
    accessibility::set_read_only(&source_view, true);
    accessibility::set_multi_line(&source_view, true);
    apply_code_block_background_css(
        container.upcast_ref::<gtk4::Widget>(),
        &source_view,
        &theme.background_css,
    );

    let scroller = gtk4::ScrolledWindow::new();
    scroller.set_child(Some(&source_view));
    scroller.set_policy(gtk4::PolicyType::Automatic, gtk4::PolicyType::Never);
    scroller.set_propagate_natural_height(true);
    scroller.set_propagate_natural_width(false);
    scroller.set_hexpand(true);
    scroller.set_halign(gtk4::Align::Fill);
    scroller.set_margin_top(CODE_BLOCK_VERTICAL_PADDING);
    scroller.set_margin_bottom(CODE_BLOCK_VERTICAL_PADDING);
    scroller.set_margin_start(CODE_BLOCK_HORIZONTAL_PADDING);
    scroller.set_margin_end(CODE_BLOCK_HORIZONTAL_PADDING);
    scroller.add_css_class("markdown-code-block-scroller");

    container.append(&scroller);
    container.upcast()
}

/// Apply one resolved background to both layers of the embedded code surface.
#[expect(
    deprecated,
    reason = "GTK4's non-deprecated provider API is display-wide, but this preview needs a widget-scoped palette override."
)]
fn apply_code_block_background_css(
    container: &gtk4::Widget,
    source_view: &sourceview5::View,
    background: &str,
) {
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(&code_block_background_css(background));
    container
        .style_context()
        .add_provider(&provider, CODE_BLOCK_BACKGROUND_CSS_PRIORITY);
    source_view
        .style_context()
        .add_provider(&provider, CODE_BLOCK_BACKGROUND_CSS_PRIORITY);
}

/// Build the CSS that keeps the block frame and source text on one surface.
fn code_block_background_css(background: &str) -> String {
    format!(
        r#"
.markdown-code-block {{
  background-color: {background};
}}

.markdown-code-block-view,
.markdown-code-block-view text {{
  background-color: {background};
}}
"#
    )
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
    accessibility::set_labelled_description(
        &grid,
        "Markdown table",
        &format!(
            "Rendered table with {} rows and {column_count} columns",
            table.rows.len()
        ),
    );

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

/// Build one GTK picture from bytes decoded on a worker thread.
fn build_decoded_image_widget(path: &Path, image: DecodedImage) -> gtk4::Widget {
    let format = if image.has_alpha {
        gdk::MemoryFormat::R8g8b8a8
    } else {
        gdk::MemoryFormat::R8g8b8
    };
    let bytes = glib::Bytes::from_owned(image.pixels);
    let texture = gdk::MemoryTexture::new(image.width, image.height, format, &bytes, image.stride);
    let picture = gtk4::Picture::for_paintable(&texture);
    picture.add_css_class("markdown-preview-image");
    accessibility::set_role(&picture, gtk4::AccessibleRole::Img);
    accessibility::set_labelled_description(
        &picture,
        "Markdown image",
        &format!("Rendered image {}", path.display()),
    );
    picture.upcast()
}

/// Remove every current child from a GTK box before replacing async image content.
fn clear_box_children(container: &gtk4::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

/// Find the first candidate that the platform image loaders can decode.
fn first_loadable_ordered_image(
    raw_target: String,
    paths: Vec<PathBuf>,
) -> OrderedImageCandidateResult {
    #[cfg(feature = "test-utils")]
    {
        let delay_ms = IMAGE_WORK_DELAY_MS.load(Ordering::Acquire);
        if delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
    }
    let mut first_unloadable = None;
    for path in paths {
        match fs_metadata::path_status(&path) {
            Ok(PathStatus::Missing) => continue,
            Ok(PathStatus::File) | Err(_) => match DecodedImage::from_path(&path) {
                Ok(image) => return OrderedImageCandidateResult::Loadable { path, image },
                Err(error) if first_unloadable.is_none() => {
                    first_unloadable = Some((path, error));
                }
                Err(_) => {}
            },
            Ok(PathStatus::Directory | PathStatus::Other) if first_unloadable.is_none() => {
                first_unloadable = Some((path, "not a regular image file".to_string()));
            }
            Ok(PathStatus::Directory | PathStatus::Other) => {}
        }
    }

    first_unloadable.map_or(
        OrderedImageCandidateResult::Missing { raw_target },
        |(path, error)| OrderedImageCandidateResult::Unloadable { path, error },
    )
}

/// Build one compact fallback for Markdown structures that exceed preview budgets.
fn build_preview_limit_fallback_widget(title: &str, body: &str, css_class: &str) -> gtk4::Widget {
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    container.set_margin_top(8);
    container.set_margin_bottom(8);
    container.set_margin_start(10);
    container.set_margin_end(10);
    container.set_halign(gtk4::Align::Start);
    container.set_width_request(280);
    container.add_css_class("card");
    container.add_css_class("markdown-preview-limit-fallback");
    container.add_css_class(css_class);
    accessibility::set_role(&container, gtk4::AccessibleRole::Group);
    accessibility::set_labelled_description(&container, title, body);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);

    let title_label = gtk4::Label::new(Some(title));
    title_label.set_xalign(0.0);
    title_label.set_wrap(true);
    title_label.add_css_class("heading");

    let body_label = gtk4::Label::new(Some(body));
    body_label.set_xalign(0.0);
    body_label.set_wrap(true);
    body_label.set_selectable(false);
    body_label.add_css_class("dim-label");

    content.append(&title_label);
    content.append(&body_label);
    container.append(&content);
    container.upcast()
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
    accessibility::set_role(&container, gtk4::AccessibleRole::Img);
    accessibility::set_labelled_description(&container, &format!("Markdown image: {title}"), body);

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

    if max_dimension <= MAX_PREVIEW_IMAGE_WIDTH {
        return (width, height);
    }

    let scaled_width = i32::try_from(
        i64::from(width).saturating_mul(i64::from(MAX_PREVIEW_IMAGE_WIDTH))
            / i64::from(max_dimension),
    )
    .unwrap_or(width);
    let scaled_height = i32::try_from(
        i64::from(height).saturating_mul(i64::from(MAX_PREVIEW_IMAGE_WIDTH))
            / i64::from(max_dimension),
    )
    .unwrap_or(height);
    (scaled_width.max(1), scaled_height.max(1))
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
    use crate::services::filesystem::fixture;
    use crate::ui::markdown_preview::imp::list_item_text_margin;
    use pulldown_cmark::LinkType;
    use tempfile::tempdir;

    #[test]
    fn preview_image_read_rejects_growth_after_metadata_without_unbounded_read() {
        let dir = tempdir().expect("image growth tempdir");
        let path = dir.path().join("growing-image.bin");
        fixture::write_bytes(&path, b"small");

        let error = read_preview_image_bytes_with_limit(&path, 16, || {
            fixture::write_repeated_bytes(&path, b"x", 17);
        })
        .expect_err("growth beyond the image limit must fail");

        assert!(error.contains("grew beyond"));
    }

    #[test]
    fn test_definition_list_parser_events_for_colon_syntax() {
        let events = parser_event_labels("Term\n: Definition");

        assert_event_order(
            &events,
            &[
                "Start::DefinitionList",
                "Start::DefinitionListTitle",
                "Text::Term",
                "End::DefinitionListTitle",
                "Start::DefinitionListDefinition",
                "Text::Definition",
                "End::DefinitionListDefinition",
                "End::DefinitionList",
            ],
        );
    }

    #[test]
    fn test_definition_list_parser_events_cover_inline_and_nested_blocks() {
        let events = parser_event_labels(
            "*Term*\n\n:   Definition with **strong** text\n\n        let nested = true;\n\n    > quoted",
        );

        assert_event_order(
            &events,
            &[
                "Start::DefinitionList",
                "Start::DefinitionListTitle",
                "Start::Emphasis",
                "Text::Term",
                "End::Emphasis",
                "Start::DefinitionListDefinition",
                "Start::Paragraph",
                "Start::Strong",
                "Text::strong",
                "End::Strong",
                "End::Paragraph",
                "Start::CodeBlock",
                "Text::let nested = true;\n",
                "End::CodeBlock",
                "Start::BlockQuote",
                "Text::quoted",
                "End::BlockQuote",
                "End::DefinitionListDefinition",
                "End::DefinitionList",
            ],
        );
    }

    #[test]
    fn test_definition_list_parser_ignores_tilde_marker_syntax() {
        let events = parser_event_labels("Term ~ Definition");

        assert!(
            events.iter().all(|event| !event.contains("DefinitionList")),
            "Expected markdown-it tilde syntax to stay outside pulldown-cmark definition-list events, got {events:?}"
        );
        assert_event_order(
            &events,
            &[
                "Start::Paragraph",
                "Text::Term ~ Definition",
                "End::Paragraph",
            ],
        );
    }

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
        let workspace_folder = tempdir.path().join("workspace");
        fixture::create_dir_all(&document_dir);
        fixture::create_dir_all(&workspace_folder.join("images"));
        fixture::write_bytes(&document_dir.join("logo.png"), b"doc");
        fixture::write_bytes(&workspace_folder.join("logo.png"), b"workspace");

        let context = MarkdownPreviewRenderContext::new(
            Some(document_dir.join("guide.md")),
            vec![workspace_folder],
        );

        assert_eq!(
            resolve_local_path("logo.png", &context),
            LocalPathResolution::Resolved(document_dir.join("logo.png"))
        );
    }

    #[test]
    fn test_resolve_local_path_defers_existence_checks_to_activation() {
        let tempdir = tempdir().expect("tempdir");
        let document_path = tempdir.path().join("docs/guide.md");
        let context = MarkdownPreviewRenderContext::new(Some(document_path.clone()), Vec::new());

        assert_eq!(
            resolve_local_path("missing.png", &context),
            LocalPathResolution::Resolved(
                document_path
                    .parent()
                    .expect("document path has parent")
                    .join("missing.png")
            )
        );
    }

    #[test]
    fn test_resolve_local_path_reports_ambiguous_workspace_candidates() {
        let tempdir = tempdir().expect("tempdir");
        let folder_a = tempdir.path().join("folder-a");
        let folder_b = tempdir.path().join("folder-b");
        fixture::create_dir_all(&folder_a.join("images"));
        fixture::create_dir_all(&folder_b.join("images"));
        fixture::write_bytes(&folder_a.join("images/logo.png"), b"a");
        fixture::write_bytes(&folder_b.join("images/logo.png"), b"b");

        let context =
            MarkdownPreviewRenderContext::new(None, vec![folder_a.clone(), folder_b.clone()]);

        assert_eq!(
            resolve_local_path("images/logo.png", &context),
            LocalPathResolution::Ambiguous(vec![
                folder_a.join("images/logo.png"),
                folder_b.join("images/logo.png"),
            ])
        );
    }

    #[test]
    fn test_resolve_image_target_prefers_document_relative_file_before_workspace() {
        let tempdir = tempdir().expect("tempdir");
        let document_dir = tempdir.path().join("docs");
        let workspace_folder = tempdir.path().join("workspace");
        fixture::create_dir_all(&document_dir);
        fixture::create_dir_all(&workspace_folder);
        fixture::write_bytes(&document_dir.join("logo.png"), b"doc");
        fixture::write_bytes(&workspace_folder.join("logo.png"), b"workspace");

        let context = MarkdownPreviewRenderContext::new(
            Some(document_dir.join("guide.md")),
            vec![workspace_folder.clone()],
        );

        assert_eq!(
            resolve_image_target("logo.png", &context),
            ResolvedImageTarget::OrderedCandidates(vec![
                document_dir.join("logo.png"),
                workspace_folder.join("logo.png"),
            ])
        );
    }

    #[test]
    fn test_resolve_image_target_falls_back_to_workspace_when_document_relative_missing() {
        let tempdir = tempdir().expect("tempdir");
        let document_dir = tempdir.path().join("docs");
        let workspace_folder = tempdir.path().join("workspace");
        fixture::create_dir_all(&document_dir);
        fixture::create_dir_all(&workspace_folder.join("images"));
        fixture::write_bytes(&workspace_folder.join("images/logo.png"), b"workspace");

        let context = MarkdownPreviewRenderContext::new(
            Some(document_dir.join("guide.md")),
            vec![workspace_folder.clone()],
        );

        assert_eq!(
            resolve_image_target("images/logo.png", &context),
            ResolvedImageTarget::OrderedCandidates(vec![
                document_dir.join("images/logo.png"),
                workspace_folder.join("images/logo.png")
            ])
        );
    }

    #[test]
    fn test_resolve_image_target_uses_folder_order_for_workspace_candidates() {
        let tempdir = tempdir().expect("tempdir");
        let folder_a = tempdir.path().join("folder-a");
        let folder_b = tempdir.path().join("folder-b");
        fixture::create_dir_all(&folder_a.join("images"));
        fixture::create_dir_all(&folder_b.join("images"));
        fixture::write_bytes(&folder_a.join("images/logo.png"), b"a");
        fixture::write_bytes(&folder_b.join("images/logo.png"), b"b");

        let context =
            MarkdownPreviewRenderContext::new(None, vec![folder_b.clone(), folder_a.clone()]);

        assert_eq!(
            resolve_image_target("images/logo.png", &context),
            ResolvedImageTarget::OrderedCandidates(vec![
                folder_b.join("images/logo.png"),
                folder_a.join("images/logo.png"),
            ])
        );
    }

    #[test]
    fn test_first_loadable_ordered_image_skips_missing_and_unloadable_candidates() {
        let tempdir = tempdir().expect("tempdir");
        let missing_folder = tempdir.path().join("missing-folder");
        let invalid_folder = tempdir.path().join("invalid-folder");
        let folder_b = tempdir.path().join("folder-b");
        fixture::create_dir_all(&invalid_folder.join("images"));
        fixture::create_dir_all(&folder_b.join("images"));
        fixture::write_bytes(&invalid_folder.join("images/logo.svg"), b"not an image");
        fixture::write_text(
            &folder_b.join("images/logo.svg"),
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10"/></svg>"##,
        );

        let result = first_loadable_ordered_image(
            "images/logo.svg".to_string(),
            vec![
                missing_folder.join("images/logo.svg"),
                invalid_folder.join("images/logo.svg"),
                folder_b.join("images/logo.svg"),
            ],
        );
        match result {
            OrderedImageCandidateResult::Loadable { path, image } => {
                assert_eq!(path, folder_b.join("images/logo.svg"));
                assert_eq!((image.width, image.height), (72, 72));
                assert!(!image.pixels.is_empty());
            }
            other => panic!("expected a decoded workspace image, got {other:?}"),
        }
    }

    #[test]
    fn test_resolve_image_target_reports_missing_for_zero_folder_scope() {
        assert_eq!(
            resolve_image_target(
                "images/logo.png",
                &MarkdownPreviewRenderContext::new(None, Vec::new()),
            ),
            ResolvedImageTarget::Fallback {
                title: "Image file not found",
                body: "images/logo.png".to_string(),
            }
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
        assert_eq!(bounded_image_size(640, 1280), (320, 640));
        assert_eq!(bounded_image_size(16, 16), (72, 72));
        assert_eq!(bounded_image_size(0, 0), (320, 180));
    }

    #[test]
    fn test_code_block_preview_budget_flags_large_blocks() {
        let mut code_block = BufferedCodeBlock::new(CodeBlockKind::Indented);
        code_block.text = "x".repeat(MAX_PREVIEW_CODE_BLOCK_BYTES);
        assert!(!code_block.exceeds_preview_widget_budget());

        code_block.text.push('x');
        assert!(code_block.exceeds_preview_widget_budget());
    }

    #[test]
    fn test_code_block_buffer_stops_storing_after_preview_budget() {
        let mut code_block = BufferedCodeBlock::new(CodeBlockKind::Indented);
        code_block.push_event(Event::Text(
            "x".repeat(MAX_PREVIEW_CODE_BLOCK_BYTES + 512).into(),
        ));

        assert_eq!(code_block.text.len(), MAX_PREVIEW_CODE_BLOCK_BYTES);
        assert_eq!(
            code_block.source_byte_len(),
            MAX_PREVIEW_CODE_BLOCK_BYTES + 512
        );
        assert!(code_block.exceeds_preview_widget_budget());
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

    #[test]
    fn test_code_block_background_css_uses_one_surface_color() {
        let css = code_block_background_css("rgb(1, 2, 3)");

        assert_eq!(
            css.matches("background-color: rgb(1, 2, 3);").count(),
            2,
            "Expected the generated CSS to apply the same background to the outer block and inner source text area"
        );
        assert!(css.contains(".markdown-code-block {"));
        assert!(css.contains(".markdown-code-block-view text"));
    }

    #[test]
    fn test_list_item_text_margin_increases_with_depth() {
        assert_eq!(list_item_text_margin(1), 60);
        assert_eq!(list_item_text_margin(2), 88);
        assert_eq!(list_item_text_margin(3), 116);
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

    fn parser_event_labels(markdown: &str) -> Vec<String> {
        Parser::new_ext(markdown, markdown_render_options())
            .map(|event| match event {
                Event::Start(tag) => match tag {
                    Tag::BlockQuote(_) => "Start::BlockQuote".to_string(),
                    Tag::CodeBlock(_) => "Start::CodeBlock".to_string(),
                    other => format!("Start::{other:?}"),
                },
                Event::End(tag) => match tag {
                    TagEnd::BlockQuote(_) => "End::BlockQuote".to_string(),
                    TagEnd::CodeBlock => "End::CodeBlock".to_string(),
                    other => format!("End::{other:?}"),
                },
                Event::Text(text) => format!("Text::{text}"),
                Event::Code(code) => format!("Code::{code}"),
                Event::SoftBreak => "SoftBreak".to_string(),
                Event::HardBreak => "HardBreak".to_string(),
                Event::Rule => "Rule".to_string(),
                Event::FootnoteReference(label) => format!("FootnoteReference::{label}"),
                Event::TaskListMarker(checked) => format!("TaskListMarker::{checked}"),
                Event::Html(html) => format!("Html::{html}"),
                Event::InlineHtml(html) => format!("InlineHtml::{html}"),
                Event::InlineMath(math) => format!("InlineMath::{math}"),
                Event::DisplayMath(math) => format!("DisplayMath::{math}"),
            })
            .collect()
    }

    fn assert_event_order(events: &[String], expected: &[&str]) {
        let mut previous = 0usize;
        for expected_event in expected {
            let offset = events[previous..]
                .iter()
                .position(|event| event == expected_event)
                .unwrap_or_else(|| {
                    panic!(
                        "expected event '{expected_event}' after index {previous}, got {events:?}"
                    )
                });
            previous += offset + 1;
        }
    }
}
