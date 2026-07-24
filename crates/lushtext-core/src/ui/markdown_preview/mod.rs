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
mod code_blocks;
mod images;
mod imp;
mod inline_footnotes;
mod links;
mod tables;

use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
#[cfg(any(test, feature = "fuzzing"))]
use pulldown_cmark::Parser;
use pulldown_cmark::{Event, HeadingLevel, Tag, TagEnd};
use sourceview5::prelude::*;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(feature = "test-utils")]
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[cfg(feature = "test-utils")]
use std::sync::{Mutex, MutexGuard};

pub use crate::services::markdown_render::MarkdownRenderState;
use crate::services::markdown_render::{
    MAX_MARKDOWN_RETAINED_BYTES, MAX_MARKDOWN_SOURCE_BYTES, MarkdownEventBatch, MarkdownPlanLimit,
    MarkdownPlanMetrics, MarkdownRenderPlan, markdown_render_options, plan_markdown,
    plan_markdown_cancellable, source_limited_markdown_plan,
};
use crate::ui::accessibility;
use crate::ui::buffer_snapshot::{BufferSnapshotHandle, BufferSnapshotPayload};
use crate::ui::editor_page::{approximate_char_width, readable_column_margin};
use gtk_lush_tasks::spawn_blocking_then;

use code_blocks::{ActiveCodeBlock, CodeBlockTheme};
use images::{ActiveImageWork, BufferedImage, PendingImageWork};
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
use links::resolve_link_target;
use tables::BufferedTableBuilder;

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
/// Inputs above this size are parsed away from GTK before bounded projection.
const MARKDOWN_BACKGROUND_PLAN_THRESHOLD_BYTES: usize = 64 * 1024;
/// Maximum detached text characters removed in one main-loop retirement turn.
const MARKDOWN_RETIREMENT_CHARS_PER_TURN: usize = 64 * 1024;
/// Maximum detached widget/link references released in one retirement turn.
const MARKDOWN_RETIREMENT_ITEMS_PER_TURN: usize = 64;
/// Maximum ordinary detached generations retained before latest-render backpressure.
const MAX_MARKDOWN_RETIREMENT_GENERATIONS: usize = 2;
#[cfg(feature = "test-utils")]
static IMAGE_WORK_DELAY_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "test-utils")]
static IMAGE_POST_DECODE_DELAY_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "test-utils")]
static IMAGE_CANDIDATE_INSPECTIONS: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "test-utils")]
static IMAGE_CANCELLED_WORK: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "test-utils")]
static IMAGE_DECODED_RESULTS: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "test-utils")]
static IMAGE_PIXEL_DROPS: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "test-utils")]
static IMAGE_PIXEL_DROPS_ON_GTK: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "test-utils")]
static IMAGE_TEST_GTK_THREAD: Mutex<Option<std::thread::ThreadId>> = Mutex::new(None);
#[cfg(feature = "test-utils")]
static MARKDOWN_PLAN_DELAY_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "test-utils")]
static MARKDOWN_SOURCE_COPIES: AtomicU64 = AtomicU64::new(0);

/// Extra render context supplied by the window when previewing a real Markdown file.
///
/// Relative links and images need a stable base path, and workspace-relative
/// image paths need the active sidebar folders. Keeping those inputs in one
/// value object lets the preview stay a reusable widget instead of reaching
/// back into the window shell directly.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarkdownPreviewRenderContext {
    document_path: Option<Arc<PathBuf>>,
    workspace_folders: Arc<[PathBuf]>,
}

impl MarkdownPreviewRenderContext {
    /// Create one render context for a Markdown preview pass.
    #[must_use]
    pub fn new(document_path: Option<PathBuf>, workspace_folders: Vec<PathBuf>) -> Self {
        Self {
            document_path: document_path.map(Arc::new),
            workspace_folders: Arc::from(workspace_folders),
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
pub(super) struct PreviewLaunchTarget {
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

/// Latest document-sized planning request retained behind one active worker.
pub(super) struct PendingMarkdownPlan {
    generation: u64,
    source: String,
    context: MarkdownPreviewRenderContext,
}

#[cfg(feature = "test-utils")]
fn lock_markdown_capacity<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

type GuardedPendingMarkdownPlan = crate::ui::plain_disposal::DisposalOwned<PendingMarkdownPlan>;
type GuardedMarkdownPlan = crate::ui::plain_disposal::DisposalOwned<MarkdownRenderPlan>;

enum SnapshotMarkdownPlanOutcome {
    Planned(GuardedMarkdownPlan),
    Empty,
    SourceLimited { source_bytes: usize },
    CapacityUnavailable { source_bytes: usize },
}

fn try_guard_markdown_source(
    generation: u64,
    markdown: &str,
    context: &MarkdownPreviewRenderContext,
) -> Option<GuardedPendingMarkdownPlan> {
    let weight = MARKDOWN_PLAN_RESERVATION_BYTES;
    let reservation = crate::ui::plain_disposal::try_reserve_for_gtk(weight)?;
    Some(reservation.own(pending_markdown_plan(generation, markdown, context.clone())))
}

fn pending_markdown_plan(
    generation: u64,
    markdown: &str,
    context: MarkdownPreviewRenderContext,
) -> PendingMarkdownPlan {
    #[cfg(feature = "test-utils")]
    MARKDOWN_SOURCE_COPIES.fetch_add(1, Ordering::AcqRel);
    let mut source = String::with_capacity(markdown.len());
    source.push_str(markdown);
    PendingMarkdownPlan {
        generation,
        source,
        context,
    }
}

/// Latest render request retained while detached GTK generations drain.
pub(super) enum PendingMarkdownRender {
    Source(GuardedPendingMarkdownPlan),
    /// Compact terminal request that avoids retaining an already over-limit source.
    SourceLimited {
        generation: u64,
        source_bytes: usize,
        context: MarkdownPreviewRenderContext,
    },
}

impl PendingMarkdownRender {
    fn generation(&self) -> u64 {
        match self {
            Self::Source(request) => request.generation,
            Self::SourceLimited { generation, .. } => *generation,
        }
    }
}

/// Latest work retained behind detached-generation backpressure.
pub(super) enum PendingMarkdownWork {
    Render(PendingMarkdownRender),
    Projection {
        generation: u64,
        plan: GuardedMarkdownPlan,
        context: MarkdownPreviewRenderContext,
    },
}

impl PendingMarkdownWork {
    fn generation(&self) -> u64 {
        match self {
            Self::Render(request) => request.generation(),
            Self::Projection { generation, .. } => *generation,
        }
    }
}

struct PlainRetirementTerminal(Arc<AtomicUsize>);

impl Drop for PlainRetirementTerminal {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

const MARKDOWN_PLAN_RESERVATION_BYTES: u64 =
    (MAX_MARKDOWN_RETAINED_BYTES + MAX_MARKDOWN_SOURCE_BYTES) as u64;

struct GuardedMarkdownProjection {
    batches: VecDeque<MarkdownEventBatch>,
    limit: Option<MarkdownPlanLimit>,
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

    /// Plan a captured editor buffer without first coalescing its chunks on GTK.
    pub(crate) fn render_snapshot_with_context(
        &self,
        snapshot: BufferSnapshotPayload,
        context: MarkdownPreviewRenderContext,
    ) {
        self.render_snapshot_with_context_or_placeholder(snapshot, context, None);
    }

    /// Plan a captured buffer on a worker and preserve an editor-specific empty state.
    pub(crate) fn render_snapshot_with_context_or_placeholder(
        &self,
        snapshot: BufferSnapshotPayload,
        context: MarkdownPreviewRenderContext,
        empty_placeholder: Option<&'static str>,
    ) {
        let generation = self.begin_render_session();
        self.show_pending_markdown_plan();
        spawn_blocking_then(
            (self.downgrade(), context),
            move || {
                let source = snapshot.into_guarded_string_on_worker();
                if empty_placeholder.is_some() && source.trim().is_empty() {
                    drop(source);
                    return SnapshotMarkdownPlanOutcome::Empty;
                }
                let source_bytes = source.len();
                if source_bytes > MAX_MARKDOWN_SOURCE_BYTES {
                    drop(source);
                    return SnapshotMarkdownPlanOutcome::SourceLimited { source_bytes };
                }
                let reservation = if let Some(source_weight) = source.reservation_weight() {
                    crate::ui::plain_disposal::try_reserve_replacement_for_gtk(
                        MARKDOWN_PLAN_RESERVATION_BYTES,
                        source_weight,
                    )
                } else {
                    crate::ui::plain_disposal::try_reserve_for_gtk(MARKDOWN_PLAN_RESERVATION_BYTES)
                };
                let Some(reservation) = reservation else {
                    drop(source);
                    return SnapshotMarkdownPlanOutcome::CapacityUnavailable { source_bytes };
                };
                let source = source.into_inner_on_worker();
                let plan = match lower_inline_footnotes(&source, markdown_render_options()) {
                    InlineFootnoteLowering::Lowered(lowered) => plan_markdown(&lowered),
                    InlineFootnoteLowering::Unchanged => plan_markdown(&source),
                    InlineFootnoteLowering::Limited => inline_footnote_limited_plan(source.len()),
                    InlineFootnoteLowering::Cancelled => source_limited_markdown_plan(source.len()),
                };
                SnapshotMarkdownPlanOutcome::Planned(reservation.own(plan))
            },
            move |(preview_weak, context), outcome| {
                if let Some(preview) = preview_weak.upgrade() {
                    match outcome {
                        SnapshotMarkdownPlanOutcome::Planned(plan) => {
                            preview.start_render_plan(generation, plan, context);
                        }
                        SnapshotMarkdownPlanOutcome::Empty => {
                            if let Some(description) = empty_placeholder {
                                preview.show_content_placeholder(description);
                            }
                        }
                        SnapshotMarkdownPlanOutcome::SourceLimited { source_bytes } => {
                            preview.start_render_plan(
                                generation,
                                crate::ui::plain_disposal::DisposalOwned::small_unreserved(
                                    source_limited_markdown_plan(source_bytes),
                                ),
                                context,
                            );
                        }
                        SnapshotMarkdownPlanOutcome::CapacityUnavailable { source_bytes } => {
                            if preview.imp().render_session.borrow().is_current(generation) {
                                preview.finish_markdown_capacity_pressure(generation, source_bytes);
                            }
                        }
                    }
                }
            },
        );
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

        if self.markdown_retirement_at_capacity() {
            self.defer_markdown_render(generation, markdown, context);
            return;
        }

        self.render_markdown_generation(generation, markdown, context);
    }

    fn render_markdown_generation(
        &self,
        generation: u64,
        markdown: &str,
        context: MarkdownPreviewRenderContext,
    ) {
        if markdown.len() > MAX_MARKDOWN_SOURCE_BYTES {
            self.start_render_plan(
                generation,
                crate::ui::plain_disposal::DisposalOwned::small_unreserved(
                    source_limited_markdown_plan(markdown.len()),
                ),
                context,
            );
        } else if markdown.len() > MARKDOWN_BACKGROUND_PLAN_THRESHOLD_BYTES {
            self.enqueue_markdown_plan(generation, markdown, context);
        } else {
            let Some(reservation) =
                crate::ui::plain_disposal::try_reserve_for_gtk(MARKDOWN_PLAN_RESERVATION_BYTES)
            else {
                self.finish_markdown_capacity_pressure(generation, markdown.len());
                return;
            };
            let plan = match lower_inline_footnotes(markdown, markdown_render_options()) {
                InlineFootnoteLowering::Lowered(lowered) => plan_markdown(&lowered),
                InlineFootnoteLowering::Unchanged => plan_markdown(markdown),
                InlineFootnoteLowering::Limited => inline_footnote_limited_plan(markdown.len()),
                InlineFootnoteLowering::Cancelled => return,
            };
            self.start_render_plan(generation, reservation.own(plan), context);
        }
    }

    fn show_markdown_memory_pressure(&self) {
        const MESSAGE: &str = "Markdown preview paused while memory pressure clears.";
        self.show_content_view();
        let buffer = self.imp().text_view.buffer();
        let already_visible = usize::try_from(buffer.char_count()).ok()
            == Some(MESSAGE.chars().count())
            && buffer.text(&buffer.start_iter(), &buffer.end_iter(), true) == MESSAGE;
        if !already_visible {
            self.clear_rendered_state(false);
            self.imp().text_view.buffer().set_text(MESSAGE);
        }
        accessibility::set_description(
            &*self.imp().text_view,
            "Markdown preview deferred by bounded plain-data capacity",
        );
    }

    fn finish_markdown_capacity_pressure(&self, generation: u64, source_bytes: usize) {
        debug_assert!(source_bytes <= MAX_MARKDOWN_SOURCE_BYTES);
        self.show_markdown_memory_pressure();
        self.imp()
            .render_session
            .borrow_mut()
            .transition(generation, MarkdownRenderState::Failed);
    }

    fn start_pending_markdown_render(&self, request: PendingMarkdownRender) {
        match request {
            PendingMarkdownRender::Source(request) => {
                self.render_owned_markdown_generation(request);
            }
            PendingMarkdownRender::SourceLimited {
                generation,
                source_bytes,
                context,
            } => self.start_render_plan(
                generation,
                crate::ui::plain_disposal::DisposalOwned::small_unreserved(
                    source_limited_markdown_plan(source_bytes),
                ),
                context,
            ),
        }
    }

    fn render_owned_markdown_generation(&self, request: GuardedPendingMarkdownPlan) {
        let source_bytes = request.source.len();
        if source_bytes > MAX_MARKDOWN_SOURCE_BYTES {
            let generation = request.generation;
            let context = request.context.clone();
            self.retire_guarded_markdown(request);
            self.start_render_plan(
                generation,
                crate::ui::plain_disposal::DisposalOwned::small_unreserved(
                    source_limited_markdown_plan(source_bytes),
                ),
                context,
            );
        } else {
            self.show_pending_markdown_plan();
            self.enqueue_owned_markdown_plan(request);
        }
    }

    fn show_pending_markdown_plan(&self) {
        self.show_content_view();
        self.clear_rendered_state(false);
        self.imp()
            .text_view
            .buffer()
            .set_text("Rendering Markdown preview…");
        accessibility::set_description(
            &*self.imp().text_view,
            "Markdown preview rendering is pending",
        );
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
        if let Some(cancel) = imp.planning_cancel_token.borrow().as_ref() {
            cancel.store(true, Ordering::Release);
        }
        self.cancel_queued_image_work();
        let generation = imp.render_session.borrow_mut().begin();
        imp.render_generation.store(generation, Ordering::Release);
        #[cfg(feature = "test-utils")]
        {
            imp.projection_dispatch_count.set(0);
            imp.projection_high_water_events.set(0);
            imp.image_admission.borrow_mut().reset_high_water();
        }
        generation
    }

    fn markdown_retirement_at_capacity(&self) -> bool {
        self.imp()
            .retirement
            .borrow()
            .as_ref()
            .is_some_and(|session| session.states.len() >= MAX_MARKDOWN_RETIREMENT_GENERATIONS)
    }

    fn defer_markdown_work(&self, work: PendingMarkdownWork) {
        let imp = self.imp();
        if let Some(retired) = imp.deferred_work.replace(Some(work)) {
            self.retire_markdown_work(retired);
        }
        #[cfg(feature = "test-utils")]
        imp.deferred_work_high_water.set(1);
    }

    /// Coalesce repeated source updates in the retained latest allocation.
    fn defer_markdown_render(
        &self,
        generation: u64,
        markdown: &str,
        context: MarkdownPreviewRenderContext,
    ) {
        let mut deferred = self.imp().deferred_work.borrow_mut();
        match deferred.as_mut() {
            Some(PendingMarkdownWork::Render(PendingMarkdownRender::Source(request)))
                if markdown.len() <= MAX_MARKDOWN_SOURCE_BYTES =>
            {
                request.generation = generation;
                request.source.clear();
                request.source.push_str(markdown);
                request.context = context;
                return;
            }
            Some(PendingMarkdownWork::Render(PendingMarkdownRender::SourceLimited {
                generation: retained_generation,
                source_bytes,
                context: retained_context,
            })) if markdown.len() > MAX_MARKDOWN_SOURCE_BYTES => {
                *retained_generation = generation;
                *source_bytes = markdown.len();
                *retained_context = context;
                return;
            }
            _ => {}
        }
        let request = if markdown.len() > MAX_MARKDOWN_SOURCE_BYTES {
            PendingMarkdownRender::SourceLimited {
                generation,
                source_bytes: markdown.len(),
                context,
            }
        } else if let Some(request) = try_guard_markdown_source(generation, markdown, &context) {
            PendingMarkdownRender::Source(request)
        } else {
            let retired = deferred.take();
            drop(deferred);
            if let Some(retired) = retired {
                self.retire_markdown_work(retired);
            }
            self.finish_markdown_capacity_pressure(generation, markdown.len());
            return;
        };
        let retired = deferred.replace(PendingMarkdownWork::Render(request));
        drop(deferred);
        if let Some(retired) = retired {
            self.retire_markdown_work(retired);
        }
        #[cfg(feature = "test-utils")]
        self.imp().deferred_work_high_water.set(1);
    }

    fn cancel_deferred_markdown_work(&self) {
        if let Some(retired) = self.imp().deferred_work.take() {
            self.retire_markdown_work(retired);
        }
    }

    fn resume_deferred_markdown_work(&self) {
        let Some(work) = self.imp().deferred_work.take() else {
            return;
        };
        if !self
            .imp()
            .render_session
            .borrow()
            .is_current(work.generation())
        {
            self.retire_markdown_work(work);
            return;
        }
        match work {
            PendingMarkdownWork::Render(request) => self.start_pending_markdown_render(request),
            PendingMarkdownWork::Projection {
                generation,
                plan,
                context,
            } => self.start_render_plan(generation, plan, context),
        }
    }

    fn retire_guarded_markdown<T: Send + 'static>(
        &self,
        retired: crate::ui::plain_disposal::DisposalOwned<T>,
    ) {
        let imp = self.imp();
        imp.plain_retirement_jobs.fetch_add(1, Ordering::AcqRel);
        let pending = imp
            .plain_retirement_pending
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1);
        imp.plain_retirement_pending_high_water
            .fetch_max(pending, Ordering::AcqRel);
        let terminal = PlainRetirementTerminal(imp.plain_retirement_pending.clone());
        drop(retired.with_disposal_terminal(move || {
            drop(terminal);
        }));
    }

    fn retire_markdown_work(&self, work: PendingMarkdownWork) {
        match work {
            PendingMarkdownWork::Render(PendingMarkdownRender::Source(request)) => {
                self.retire_guarded_markdown(request);
            }
            PendingMarkdownWork::Render(PendingMarkdownRender::SourceLimited { .. }) => {}
            PendingMarkdownWork::Projection { plan, .. } => {
                self.retire_guarded_markdown(plan);
            }
        }
    }

    /// Keep one document-sized planner active and one replaceable latest source.
    fn enqueue_markdown_plan(
        &self,
        generation: u64,
        markdown: &str,
        context: MarkdownPreviewRenderContext,
    ) {
        let imp = self.imp();
        if imp.planning_worker_running.get() {
            if let Some(cancel) = imp.planning_cancel_token.borrow().as_ref() {
                cancel.store(true, Ordering::Release);
            }
            if let Some(queued) = imp.queued_plan.borrow_mut().as_mut() {
                queued.generation = generation;
                queued.source.clear();
                queued.source.push_str(markdown);
                queued.context = context;
                return;
            }
            let Some(request) = try_guard_markdown_source(generation, markdown, &context) else {
                self.finish_markdown_capacity_pressure(generation, markdown.len());
                return;
            };
            if let Some(retired) = imp.queued_plan.replace(Some(request)) {
                self.retire_guarded_markdown(retired);
            }
            return;
        }
        let Some(request) = try_guard_markdown_source(generation, markdown, &context) else {
            self.finish_markdown_capacity_pressure(generation, markdown.len());
            return;
        };
        self.show_pending_markdown_plan();
        self.spawn_markdown_plan(request);
    }

    fn enqueue_owned_markdown_plan(&self, request: GuardedPendingMarkdownPlan) {
        let imp = self.imp();
        if imp.planning_worker_running.get() {
            if let Some(cancel) = imp.planning_cancel_token.borrow().as_ref() {
                cancel.store(true, Ordering::Release);
            }
            if let Some(retired) = imp.queued_plan.replace(Some(request)) {
                self.retire_guarded_markdown(retired);
            }
        } else {
            self.spawn_markdown_plan(request);
        }
    }

    fn spawn_markdown_plan(&self, request: GuardedPendingMarkdownPlan) {
        let imp = self.imp();
        imp.planning_worker_running.set(true);
        let cancel = Arc::new(AtomicBool::new(false));
        imp.planning_cancel_token.replace(Some(cancel.clone()));
        let generation = request.generation;
        let context = request.context.clone();
        let request = request;
        let generation_counter = imp.render_generation.clone();
        let retirement_jobs = imp.plain_retirement_jobs.clone();
        let preview_weak = self.downgrade();
        spawn_blocking_then(
            preview_weak,
            move || {
                let outcome = request.map_preserving_reservation(|request| {
                    let PendingMarkdownPlan {
                        generation,
                        source,
                        context: _,
                    } = request;
                    #[cfg(feature = "test-utils")]
                    std::thread::sleep(std::time::Duration::from_millis(
                        MARKDOWN_PLAN_DELAY_MS.load(Ordering::Acquire),
                    ));
                    if cancel.load(Ordering::Acquire) {
                        return None;
                    }
                    let plan = match lower_inline_footnotes_cancellable(
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
                    };
                    if generation_counter.load(Ordering::Acquire) == generation {
                        plan
                    } else {
                        if plan.is_some() {
                            retirement_jobs.fetch_add(1, Ordering::AcqRel);
                        }
                        None
                    }
                });
                if outcome.is_some() {
                    Some(outcome.map_preserving_reservation(|outcome| {
                        outcome.expect("checked Markdown plan outcome exists")
                    }))
                } else {
                    drop(outcome);
                    None
                }
            },
            move |preview_weak, plan| {
                let Some(preview) = preview_weak.upgrade() else {
                    drop(plan);
                    return;
                };
                let imp = preview.imp();
                imp.planning_worker_running.set(false);
                imp.planning_cancel_token.take();
                if let Some(plan) = plan {
                    if imp.render_session.borrow().is_current(generation) {
                        preview.start_render_plan(generation, plan, context);
                    } else {
                        preview.retire_guarded_markdown(plan);
                    }
                }
                let queued = imp.queued_plan.take();
                if let Some(queued) = queued {
                    if imp.render_session.borrow().is_current(queued.generation) {
                        preview.spawn_markdown_plan(queued);
                    } else {
                        preview.retire_guarded_markdown(queued);
                    }
                }
            },
        );
    }

    fn cancel_pending_markdown_planning(&self) {
        let imp = self.imp();
        if let Some(cancel) = imp.planning_cancel_token.borrow().as_ref() {
            cancel.store(true, Ordering::Release);
        }
        if let Some(retired) = imp.queued_plan.take() {
            self.retire_guarded_markdown(retired);
        }
    }

    /// Accept a current immutable plan and project at most one batch per GTK turn.
    fn start_render_plan(
        &self,
        generation: u64,
        plan: GuardedMarkdownPlan,
        context: MarkdownPreviewRenderContext,
    ) {
        if !self.imp().render_session.borrow().is_current(generation) {
            self.retire_guarded_markdown(plan);
            return;
        }
        if self.markdown_retirement_at_capacity() {
            self.defer_markdown_work(PendingMarkdownWork::Projection {
                generation,
                plan,
                context,
            });
            return;
        }
        self.show_content_view();
        self.clear_rendered_state(false);
        self.imp()
            .render_session
            .borrow_mut()
            .transition(generation, MarkdownRenderState::Projecting);

        let mut projection =
            Some(
                plan.map_preserving_reservation(|plan| GuardedMarkdownProjection {
                    batches: VecDeque::from(plan.batches),
                    limit: plan.limit,
                }),
            );
        // Preserve immediate small-document rendering while the planner's
        // event-and-byte ceilings bound this initial GTK turn exactly like
        // every deferred projection slice.
        if let Some(batch) = projection
            .as_mut()
            .and_then(|projection| projection.batches.pop_front())
        {
            self.apply_render_batch(generation, &batch);
            self.render_event_batch(batch, &context);
        }
        if projection
            .as_ref()
            .is_none_or(|projection| projection.batches.is_empty())
        {
            let limit = projection.as_ref().and_then(|projection| projection.limit);
            drop(projection.take());
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
                drop(projection.take());
                return glib::ControlFlow::Break;
            };
            if !preview.imp().render_session.borrow().is_current(generation) {
                if let Some(retired) = projection.take() {
                    preview.retire_guarded_markdown(retired);
                }
                return glib::ControlFlow::Break;
            }
            if let Some(batch) = projection
                .as_mut()
                .and_then(|projection| projection.batches.pop_front())
            {
                preview.apply_render_batch(generation, &batch);
                preview.render_event_batch(batch, &context);
            }
            if projection
                .as_ref()
                .is_none_or(|projection| projection.batches.is_empty())
            {
                let limit = projection.as_ref().and_then(|projection| projection.limit);
                drop(projection.take());
                preview.finish_render_plan(generation, limit);
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    /// Record direct slice evidence before applying a current batch.
    fn apply_render_batch(&self, generation: u64, batch: &MarkdownEventBatch) {
        debug_assert!(self.imp().render_session.borrow().is_current(generation));
        #[cfg(not(feature = "test-utils"))]
        let _ = batch;
        #[cfg(feature = "test-utils")]
        {
            let imp = self.imp();
            imp.projection_dispatch_count
                .set(imp.projection_dispatch_count.get().wrapping_add(1));
            imp.projection_high_water_events
                .set(imp.projection_high_water_events.get().max(batch.len()));
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
        self.cancel_deferred_markdown_work();
        self.cancel_queued_image_work();
        let generation = self.imp().render_session.borrow_mut().cancel();
        self.imp()
            .render_generation
            .store(generation, Ordering::Release);
    }

    /// Whether current planning/projection blocks exact preview readiness.
    #[must_use]
    pub fn render_pending(&self) -> bool {
        self.imp().render_session.borrow().pending()
            || self.imp().planning_worker_running.get()
            || self.imp().queued_plan.borrow().is_some()
            || self.imp().deferred_work.borrow().is_some()
            || self.imp().plain_retirement_pending.load(Ordering::Acquire) > 0
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

    /// Count source copies admitted into guarded Markdown planning ownership.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn markdown_source_copies_for_test() -> u64 {
        MARKDOWN_SOURCE_COPIES.load(Ordering::Acquire)
    }

    /// Render an owned direct snapshot through the worker reservation boundary.
    #[cfg(feature = "test-utils")]
    pub fn render_snapshot_for_test(&self, source: String) {
        self.render_snapshot_with_context(
            BufferSnapshotPayload::direct(source),
            MarkdownPreviewRenderContext::default(),
        );
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

    /// Direct detached-generation, latest-work, and plain-disposal evidence.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn retirement_backlog_counters_for_test(
        &self,
    ) -> (usize, usize, usize, usize, u64, usize, usize) {
        let imp = self.imp();
        (
            imp.retirement
                .borrow()
                .as_ref()
                .map_or(0, |session| session.states.len()),
            imp.retirement_generations_high_water.get(),
            usize::from(imp.deferred_work.borrow().is_some()),
            MAX_MARKDOWN_RETIREMENT_GENERATIONS,
            imp.plain_retirement_jobs.load(Ordering::Acquire),
            imp.plain_retirement_pending.load(Ordering::Acquire),
            imp.plain_retirement_pending_high_water
                .load(Ordering::Acquire),
        )
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
        self.clear_rendered_state(true);
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
        self.clear_rendered_state(true);
        self.imp().text_view.buffer().set_text(description);
    }

    /// Clear the rendered content without showing the placeholder.
    pub fn clear(&self) {
        self.cancel_render_session();
        self.clear_rendered_state(true);
    }

    /// Render an accessible terminal when a caller cannot produce a plan.
    pub fn show_render_failure(&self, description: &str) {
        self.cancel_render_session();
        self.show_content_view();
        self.clear_rendered_state(true);
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
    fn clear_rendered_state(&self, allow_escape_generation: bool) {
        let imp = self.imp();
        let retirement_len = imp
            .retirement
            .borrow()
            .as_ref()
            .map_or(0, |session| session.states.len());
        if allow_escape_generation
            && retirement_len >= MAX_MARKDOWN_RETIREMENT_GENERATIONS.saturating_add(1)
        {
            // The first terminal transition may consume one escape generation.
            // Further terminal updates own only the small current buffer, so
            // reuse it instead of growing detached GTK generations without bound.
            debug_assert!(imp.rendered_embeds.borrow().is_empty());
            debug_assert!(imp.text_link_targets.borrow().is_empty());
            imp.text_view.buffer().set_text("");
            self.advance_rendered_embed_generation();
            return;
        }
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
            let mut retirement = imp.retirement.borrow_mut();
            let session = retirement.get_or_insert_with(MarkdownRetirementSession::default);
            debug_assert!(
                session.states.len()
                    < MAX_MARKDOWN_RETIREMENT_GENERATIONS + usize::from(allow_escape_generation)
            );
            session.states.push_back(retired);
            #[cfg(feature = "test-utils")]
            imp.retirement_generations_high_water.set(
                imp.retirement_generations_high_water
                    .get()
                    .max(session.states.len()),
            );
            drop(retirement);
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
        let should_resume = session.states.len() < MAX_MARKDOWN_RETIREMENT_GENERATIONS
            && imp.deferred_work.borrow().is_some();
        if session.states.is_empty() {
            session_slot.take();
        }
        let retirement_pending = session_slot.is_some();
        drop(session_slot);
        if should_resume {
            self.resume_deferred_markdown_work();
        }
        retirement_pending || self.imp().retirement.borrow().is_some()
    }

    /// Insert one already-built GTK widget into the preview text flow.
    pub(super) fn insert_embedded_widget(
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

/// Pop the last tag from the stack. No-op if the stack is empty.
fn pop_tag(stack: &mut Vec<String>) {
    stack.pop();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::markdown_preview::imp::list_item_text_margin;

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
    fn render_context_clones_share_one_path_graph() {
        let context = MarkdownPreviewRenderContext::new(
            Some(PathBuf::from("/workspace/document.md")),
            (0..1_000)
                .map(|index| PathBuf::from(format!("/workspace/folder-{index}")))
                .collect(),
        );
        let clones = (0..1_000).map(|_| context.clone()).collect::<Vec<_>>();

        assert!(clones.iter().all(|clone| {
            Arc::ptr_eq(
                context.document_path.as_ref().expect("document path"),
                clone.document_path.as_ref().expect("document path"),
            ) && Arc::ptr_eq(&context.workspace_folders, &clone.workspace_folders)
        }));
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
