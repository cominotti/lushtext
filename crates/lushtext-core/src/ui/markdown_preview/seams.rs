// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: seam value objects for the Markdown preview workflow.
//!
//! Two families, both reified because they cross module boundaries and were
//! reconstructed at more than one call site.
//!
//! **Freshness and ownership seams** — `PendingMarkdownRender`,
//! `PendingMarkdownWork`, `GuardedMarkdownProjection`,
//! `SnapshotMarkdownPlanOutcome`, `PlainRetirementTerminal`, and
//! `RetiredMarkdownRender`. These carry a render generation, an owned
//! document-sized payload, or both, from admission through planning, projection,
//! and retirement. Each exists so a stale completion is a **type-level**
//! question rather than a remembered convention: a projection that cannot prove
//! its generation cannot publish.
//!
//! **Render-time projection values** — `EmbeddedBlockLayout`, `RenderedEmbed`,
//! `RenderedTextLink`, `ActiveTextLink`, `ListMarker`, `ListFrame`,
//! `ListItemRenderState`, and `DefinitionRenderState`. These cross between the
//! projection coordination role and the topical renderers (`tables`,
//! `code_blocks`, `links`, `text_flow`), and several survive *across* main-loop
//! turns, which is why they are values rather than locals.
//! `EmbeddedBlockLayout` in particular is the anchored-width contract the
//! TextView Child Anchors rule governs.
//!
//! `MarkdownPreviewRenderContext` is here too: it is the window's rendering
//! intent — a base path for relative links plus the workspace folders — reified
//! so the preview stays a reusable widget instead of reaching into the shell.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(feature = "test-utils")]
use std::sync::{Mutex, MutexGuard};

use gtk4::prelude::*;

use crate::services::markdown_render::{
    MAX_MARKDOWN_RETAINED_BYTES, MAX_MARKDOWN_SOURCE_BYTES, MarkdownEventBatch, MarkdownPlanLimit,
    MarkdownRenderPlan,
};

use super::continuation::MarkdownProjectionContinuation;
#[cfg(feature = "test-utils")]
use super::test_policy::MARKDOWN_SOURCE_COPIES;

/// Extra render context supplied by the window when previewing a real Markdown file.
///
/// Relative links and images need a stable base path, and workspace-relative
/// image paths need the active sidebar folders. Keeping those inputs in one
/// value object lets the preview stay a reusable widget instead of reaching
/// back into the window shell directly.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarkdownPreviewRenderContext {
    pub(super) document_path: Option<Arc<PathBuf>>,
    pub(super) workspace_folders: Arc<[PathBuf]>,
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

/// One launchable preview target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PreviewLaunchTarget {
    /// URI handed to the desktop's default external launcher.
    pub(super) uri: String,
    /// Absolute local path when the target resolved to a local file.
    pub(super) local_path: Option<PathBuf>,
}

/// One clickable link range rendered into the text buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RenderedTextLink {
    /// First buffer offset that belongs to the rendered link.
    pub(super) start_offset: i32,
    /// First buffer offset after the rendered link.
    pub(super) end_offset: i32,
    /// Launch target associated with this rendered range.
    pub(super) target: PreviewLaunchTarget,
}

/// Captured horizontal context for one embedded Markdown block.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct EmbeddedBlockLayout {
    /// Outer offset before the embedded widget, matching the active text column.
    pub(super) margin_start: i32,
    /// Outer offset after the embedded widget, matching the active text column.
    pub(super) margin_end: i32,
}

impl EmbeddedBlockLayout {
    /// Fold one active text-tag margin into the embedded-widget context.
    pub(super) fn include_margin(&mut self, margin_start: i32, margin_end: i32) {
        // GtkTextTag block margins act like competing paragraph properties, not
        // nested boxes. Use the widest active margin so child anchors stay in
        // the same effective column as nearby tagged text.
        self.margin_start = self.margin_start.max(margin_start);
        self.margin_end = self.margin_end.max(margin_end);
    }

    /// Return the width a code block can use inside this context.
    pub(super) fn code_block_width(self, preview_text_column_width: i32) -> i32 {
        preview_text_column_width
            .saturating_sub(self.margin_start.saturating_add(self.margin_end))
            .max(1)
    }
}

/// One widget anchored into the preview plus the layout context active at insertion.
#[derive(Clone)]
pub(super) struct RenderedEmbed {
    /// Widget added to the `GtkTextView` at a child anchor.
    pub(super) widget: gtk4::Widget,
    /// Captured block context used by later allocation refreshes.
    pub(super) layout: EmbeddedBlockLayout,
}

impl RenderedEmbed {
    /// Store one child-anchor widget and its insertion-time layout context.
    pub(super) fn new(widget: gtk4::Widget, layout: EmbeddedBlockLayout) -> Self {
        Self { widget, layout }
    }
}

/// One link tag currently open while the parser is streaming inline events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ActiveTextLink {
    /// Buffer offset where the rendered link text started.
    pub(super) start_offset: i32,
    /// Resolved target, if this Markdown destination is launchable.
    pub(super) target: Option<PreviewLaunchTarget>,
    /// Whether this link pushed the preview's link text tag onto the stack.
    pub(super) pushed_tag: bool,
}

/// Marker style for the current Markdown list frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ListMarker {
    /// Unordered list items render with a native bullet glyph.
    Unordered,
    /// Ordered list items render with the next number from the source list.
    Ordered(u64),
}

impl ListMarker {
    /// Return the visible marker prefix for the next item in this list frame.
    pub(super) fn prefix(self) -> String {
        match self {
            Self::Unordered => "\u{2022} ".to_string(),
            Self::Ordered(number) => format!("{number}. "),
        }
    }

    /// Advance ordered list counters after one item has finished rendering.
    pub(super) fn advance(&mut self) {
        if let Self::Ordered(number) = self {
            *number += 1;
        }
    }
}

/// One active Markdown list level in the streaming renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ListFrame {
    /// Marker and counter state for this nesting depth.
    pub(super) marker: ListMarker,
}

impl ListFrame {
    /// Create a list frame from pulldown-cmark's optional ordered-list start.
    pub(super) fn new(start_num: Option<u64>) -> Self {
        Self {
            marker: start_num.map_or(ListMarker::Unordered, ListMarker::Ordered),
        }
    }

    /// Return the marker prefix for the next list item.
    pub(super) fn prefix(self) -> String {
        self.marker.prefix()
    }

    /// Advance this list's counter after one item.
    pub(super) fn advance(&mut self) {
        self.marker.advance();
    }
}

/// Per-item row-flow state for Markdown lists.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct ListItemRenderState {
    /// Whether this item has emitted any visible marker or content.
    pub(super) has_content: bool,
    /// Whether the previous paragraph ended and a following paragraph should
    /// keep the intentional loose-list blank row.
    pub(super) paragraph_ended: bool,
}

/// Per-definition row-flow state for Markdown definition lists.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct DefinitionRenderState {
    /// Whether this definition has emitted any visible text or anchored block.
    pub(super) has_content: bool,
    /// Whether a paragraph ended and the next paragraph needs the visible
    /// separation pulldown-cmark represents inside a loose definition body.
    pub(super) paragraph_ended: bool,
}

/// Latest document-sized planning request retained behind one active worker.
pub(super) struct PendingMarkdownPlan {
    pub(super) generation: u64,
    pub(super) source: String,
    pub(super) context: MarkdownPreviewRenderContext,
}

#[cfg(feature = "test-utils")]
pub(super) fn lock_markdown_capacity<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(super) type GuardedPendingMarkdownPlan =
    crate::ui::plain_disposal::DisposalOwned<PendingMarkdownPlan>;
pub(super) type GuardedMarkdownPlan = crate::ui::plain_disposal::DisposalOwned<MarkdownRenderPlan>;

pub(super) enum SnapshotMarkdownPlanOutcome {
    Planned(GuardedMarkdownPlan),
    Empty,
    SourceLimited { source_bytes: usize },
    CapacityUnavailable { source_bytes: usize },
}

pub(super) fn try_guard_markdown_source(
    generation: u64,
    markdown: &str,
    context: &MarkdownPreviewRenderContext,
) -> Option<GuardedPendingMarkdownPlan> {
    let weight = MARKDOWN_PLAN_RESERVATION_BYTES;
    let reservation = crate::ui::plain_disposal::try_reserve_for_gtk(weight)?;
    Some(reservation.own(pending_markdown_plan(generation, markdown, context.clone())))
}

pub(super) fn pending_markdown_plan(
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
    pub(super) fn generation(&self) -> u64 {
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
    pub(super) fn generation(&self) -> u64 {
        match self {
            Self::Render(request) => request.generation(),
            Self::Projection { generation, .. } => *generation,
        }
    }
}

pub(super) struct PlainRetirementTerminal(pub(super) Arc<AtomicUsize>);

impl Drop for PlainRetirementTerminal {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(super) const MARKDOWN_PLAN_RESERVATION_BYTES: u64 =
    (MAX_MARKDOWN_RETAINED_BYTES + MAX_MARKDOWN_SOURCE_BYTES) as u64;

/// One generation's remaining batches plus the continuation they resume into.
///
/// The continuation travels with the projection so a generation change,
/// cancellation, or widget teardown releases both together through the same
/// guarded disposal path, and any in-flight embedded-block text it still owns is
/// freed off the GTK thread.
pub(super) struct GuardedMarkdownProjection {
    pub(super) batches: VecDeque<MarkdownEventBatch>,
    pub(super) limit: Option<MarkdownPlanLimit>,
    /// Omissions a reader can notice, which is what the terminal reports.
    pub(super) user_visible_omissions: usize,
    pub(super) continuation: MarkdownProjectionContinuation,
}

/// What one finished projection publishes as its terminal state.
///
/// Reading this once, when the last batch has been applied, keeps the terminal
/// decision out of the per-turn loop and keeps the two independent facts — a
/// global budget stopped planning, and the document contained omissions — from
/// being collapsed into one flag.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct MarkdownProjectionTerminal {
    pub(super) limit: Option<MarkdownPlanLimit>,
    pub(super) user_visible_omissions: usize,
}

impl GuardedMarkdownProjection {
    /// Describe the terminal this projection's plan reached.
    pub(super) fn terminal(&self) -> MarkdownProjectionTerminal {
        MarkdownProjectionTerminal {
            limit: self.limit,
            user_visible_omissions: self.user_visible_omissions,
        }
    }
}

/// User-facing completion copy for a preview that rendered with omissions.
///
/// The count is the plan's user-visible omission total, never the raw marker
/// total: the carried-embed markers are charge carriers for a block the preview
/// already replaces with its own in-place fallback, so counting them would
/// report omissions for a document that renders exactly as it does today.
pub(super) fn simplified_render_description(user_visible_omissions: usize) -> String {
    if user_visible_omissions == 1 {
        "Markdown preview complete; 1 block was too complex to render".to_string()
    } else {
        format!(
            "Markdown preview complete; {user_visible_omissions} blocks were too complex to render"
        )
    }
}

/// One detached render generation awaiting bounded main-loop cleanup.
pub(super) struct RetiredMarkdownRender {
    pub(super) buffer: gtk4::TextBuffer,
    pub(super) embeds: VecDeque<RenderedEmbed>,
    pub(super) links: VecDeque<RenderedTextLink>,
}

impl RetiredMarkdownRender {
    pub(super) fn is_empty(&self) -> bool {
        self.buffer.char_count() == 0 && self.embeds.is_empty() && self.links.is_empty()
    }
}

/// Serial disposer for detached Markdown render generations.
#[derive(Default)]
pub(super) struct MarkdownRetirementSession {
    pub(super) states: VecDeque<RetiredMarkdownRender>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::markdown_render::{MarkdownPlanLimit, markdown_render_options};
    use crate::ui::markdown_preview::imp::list_item_text_margin;
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};

    #[test]
    fn simplified_terminal_copy_reports_completion_and_a_count() {
        assert_eq!(
            simplified_render_description(1),
            "Markdown preview complete; 1 block was too complex to render"
        );
        assert_eq!(
            simplified_render_description(7),
            "Markdown preview complete; 7 blocks were too complex to render"
        );
    }

    #[test]
    fn a_projection_terminal_separates_a_global_stop_from_omissions() {
        let projection = MarkdownProjectionTerminal::default();
        assert_eq!(projection.limit, None);
        assert_eq!(projection.user_visible_omissions, 0);
        // A stopped preview keeps its own budget copy, which never changes.
        assert_eq!(
            MarkdownPlanLimit::Events.description(),
            "Markdown preview limited after 50,000 render events"
        );
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
