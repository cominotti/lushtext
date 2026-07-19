// SPDX-License-Identifier: GPL-3.0-or-later

//! GTK-free Markdown planning with deterministic retention and slice budgets.
//!
//! The parser owns every retained event before it crosses back to the GTK
//! adapter. Complete top-level blocks are packed into projection batches, so a
//! batch boundary never loses inline/list/table state when GTK yields.

use pulldown_cmark::{Event, Options, Parser, Tag};
use std::sync::atomic::{AtomicBool, Ordering};

/// Largest source accepted for automatic Markdown preview planning.
pub const MAX_MARKDOWN_SOURCE_BYTES: usize = 4 * 1024 * 1024;
/// Maximum parser events retained by one render generation.
pub const MAX_MARKDOWN_EVENTS: usize = 50_000;
/// Maximum structural nesting accepted from the event stream.
pub const MAX_MARKDOWN_STRUCTURE_DEPTH: usize = 128;
/// Maximum table, code-block, and image descriptors retained in one plan.
pub const MAX_MARKDOWN_EMBED_DESCRIPTORS: usize = 256;
/// Maximum bytes retained by event text and link/embed descriptors.
pub const MAX_MARKDOWN_RETAINED_BYTES: usize = 8 * 1024 * 1024;
/// Maximum event/node work one GTK projection turn may apply.
pub const MARKDOWN_EVENTS_PER_PROJECTION_SLICE: usize = 256;
/// Maximum retained text/link bytes one GTK projection turn may consume.
pub const MARKDOWN_BYTES_PER_PROJECTION_SLICE: usize = 256 * 1024;

/// Current generation-owned terminal state of the preview renderer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MarkdownRenderState {
    #[default]
    Idle,
    Planning,
    Projecting,
    Complete,
    Limited,
    Failed,
    Cancelled,
}

/// GTK-free generation and readiness state for one Markdown preview adapter.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MarkdownRenderSession {
    generation: u64,
    state: MarkdownRenderState,
    pending: bool,
}

impl MarkdownRenderSession {
    /// Invalidate older work and begin one pending planning generation.
    pub fn begin(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.state = MarkdownRenderState::Planning;
        self.pending = true;
        self.generation
    }

    /// Transition current work without allowing stale generations to publish.
    pub fn transition(&mut self, generation: u64, state: MarkdownRenderState) -> bool {
        if generation != self.generation {
            return false;
        }
        self.state = state;
        self.pending = matches!(
            state,
            MarkdownRenderState::Planning | MarkdownRenderState::Projecting
        );
        true
    }

    /// Invalidate current work and publish the cancelled terminal.
    pub fn cancel(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.state = MarkdownRenderState::Cancelled;
        self.pending = false;
        self.generation
    }

    #[must_use]
    pub fn generation(self) -> u64 {
        self.generation
    }

    #[must_use]
    pub fn state(self) -> MarkdownRenderState {
        self.state
    }

    #[must_use]
    pub fn pending(self) -> bool {
        self.pending
    }

    #[must_use]
    pub fn is_current(self, generation: u64) -> bool {
        self.generation == generation
    }
}

/// Direct count and byte ownership evidence for lazy Markdown image work.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MarkdownImageAdmissionSnapshot {
    pub owned_count: usize,
    pub owned_bytes: u64,
    pub high_water_count: usize,
    pub high_water_bytes: u64,
}

/// Saturating GTK-free admission state for queued and active image descriptors.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MarkdownImageAdmission {
    snapshot: MarkdownImageAdmissionSnapshot,
}

impl MarkdownImageAdmission {
    /// Admit one conservatively charged descriptor without exceeding either ceiling.
    pub fn try_admit(&mut self, charge_bytes: u64, max_count: usize, max_bytes: u64) -> bool {
        let next_count = self.snapshot.owned_count.saturating_add(1);
        let next_bytes = self.snapshot.owned_bytes.saturating_add(charge_bytes);
        if next_count > max_count || next_bytes > max_bytes {
            return false;
        }
        self.snapshot.owned_count = next_count;
        self.snapshot.owned_bytes = next_bytes;
        self.snapshot.high_water_count = self.snapshot.high_water_count.max(next_count);
        self.snapshot.high_water_bytes = self.snapshot.high_water_bytes.max(next_bytes);
        true
    }

    /// Release the exact scalar charge owned by one completed or cancelled descriptor.
    pub fn release(&mut self, charge_bytes: u64) {
        self.snapshot.owned_count = self.snapshot.owned_count.saturating_sub(1);
        self.snapshot.owned_bytes = self.snapshot.owned_bytes.saturating_sub(charge_bytes);
    }

    /// Start new generation evidence at the ownership still draining from older work.
    pub fn reset_high_water(&mut self) {
        self.snapshot.high_water_count = self.snapshot.owned_count;
        self.snapshot.high_water_bytes = self.snapshot.owned_bytes;
    }

    #[must_use]
    pub fn snapshot(self) -> MarkdownImageAdmissionSnapshot {
        self.snapshot
    }
}

/// Parser options shared by planning, preprocessing, fuzzing, and projection.
#[must_use]
pub fn markdown_render_options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_GFM);
    options.insert(Options::ENABLE_DEFINITION_LIST);
    options
}

/// Deterministic reason automatic rendering stopped before the full document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkdownPlanLimit {
    SourceBytes,
    Events,
    StructuralDepth,
    EmbedDescriptors,
    RetainedBytes,
    TopLevelBlock,
    ProjectionBytes,
    InlineFootnotes,
}

impl MarkdownPlanLimit {
    /// Accessible user-facing explanation of the enforced limit.
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Self::SourceBytes => "Markdown preview paused because the source exceeds 4 MiB",
            Self::Events => "Markdown preview limited after 50,000 render events",
            Self::StructuralDepth => {
                "Markdown preview limited because structural nesting exceeds 128 levels"
            }
            Self::EmbedDescriptors => {
                "Markdown preview limited after 256 tables, code blocks, or images"
            }
            Self::RetainedBytes => {
                "Markdown preview limited because rendered content exceeds 8 MiB"
            }
            Self::TopLevelBlock => {
                "Markdown preview limited because one block exceeds a projection slice"
            }
            Self::ProjectionBytes => {
                "Markdown preview limited because one block exceeds the projection byte budget"
            }
            Self::InlineFootnotes => {
                "Markdown preview limited because inline footnote expansion exceeds its budget"
            }
        }
    }
}

/// Direct resource counters for one immutable plan.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MarkdownPlanMetrics {
    pub source_bytes: usize,
    pub events: usize,
    pub max_depth: usize,
    pub embed_descriptors: usize,
    pub retained_bytes: usize,
}

/// One complete-block GTK projection batch.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkdownEventBatch {
    events: Vec<Event<'static>>,
    retained_bytes: usize,
}

impl MarkdownEventBatch {
    #[must_use]
    pub fn events(&self) -> &[Event<'static>] {
        &self.events
    }

    #[must_use]
    pub fn into_events(self) -> Vec<Event<'static>> {
        self.events
    }

    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

/// Immutable GTK-free plan owned by one render generation.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkdownRenderPlan {
    pub batches: Vec<MarkdownEventBatch>,
    pub metrics: MarkdownPlanMetrics,
    pub limit: Option<MarkdownPlanLimit>,
}

impl MarkdownRenderPlan {
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.limit.is_none()
    }

    #[must_use]
    pub fn projected_events(&self) -> usize {
        self.batches.iter().map(MarkdownEventBatch::len).sum()
    }
}

/// Parse owned Markdown into complete-block, bounded projection batches.
///
/// # Panics
///
/// Panics only if the internal cancellable planner reports cancellation when
/// no cancellation token was supplied, which would violate its contract.
#[must_use]
pub fn plan_markdown(markdown: &str) -> MarkdownRenderPlan {
    plan_markdown_inner(markdown, None).expect("uncancelled Markdown planning cannot cancel")
}

/// Build the compact terminal used when a deferred request already knows its
/// source exceeds the automatic preview budget.
#[must_use]
pub fn source_limited_markdown_plan(source_bytes: usize) -> MarkdownRenderPlan {
    MarkdownRenderPlan {
        batches: Vec::new(),
        metrics: MarkdownPlanMetrics {
            source_bytes,
            ..MarkdownPlanMetrics::default()
        },
        limit: Some(MarkdownPlanLimit::SourceBytes),
    }
}

/// Parse Markdown with bounded cancellation checkpoints for single-flight workers.
#[must_use]
pub fn plan_markdown_cancellable(
    markdown: &str,
    cancel: &AtomicBool,
) -> Option<MarkdownRenderPlan> {
    plan_markdown_inner(markdown, Some(cancel))
}

fn plan_markdown_inner(markdown: &str, cancel: Option<&AtomicBool>) -> Option<MarkdownRenderPlan> {
    if cancel.is_some_and(|cancel| cancel.load(Ordering::Acquire)) {
        return None;
    }
    let source_bytes = markdown.len();
    let mut metrics = MarkdownPlanMetrics {
        source_bytes,
        ..MarkdownPlanMetrics::default()
    };
    if source_bytes > MAX_MARKDOWN_SOURCE_BYTES {
        return Some(source_limited_markdown_plan(source_bytes));
    }

    let mut batches = Vec::new();
    let mut batch = Vec::new();
    let mut batch_retained_bytes = 0usize;
    let mut block = Vec::new();
    let mut block_retained_bytes = 0usize;
    let mut depth = 0usize;
    let mut limit = None;

    for (event_index, event) in Parser::new_ext(markdown, markdown_render_options()).enumerate() {
        if event_index % 64 == 0 && cancel.is_some_and(|cancel| cancel.load(Ordering::Acquire)) {
            return None;
        }
        let retained_bytes = event_retained_bytes(&event);
        let next_events = metrics.events.saturating_add(1);
        let next_retained = metrics.retained_bytes.saturating_add(retained_bytes);
        let next_embeds = metrics
            .embed_descriptors
            .saturating_add(usize::from(is_embed_start(&event)));
        if next_events > MAX_MARKDOWN_EVENTS {
            limit = Some(MarkdownPlanLimit::Events);
            break;
        }
        if next_retained > MAX_MARKDOWN_RETAINED_BYTES {
            limit = Some(MarkdownPlanLimit::RetainedBytes);
            break;
        }
        if next_embeds > MAX_MARKDOWN_EMBED_DESCRIPTORS {
            limit = Some(MarkdownPlanLimit::EmbedDescriptors);
            break;
        }

        if matches!(event, Event::Start(_)) {
            depth = depth.saturating_add(1);
            if depth > MAX_MARKDOWN_STRUCTURE_DEPTH {
                limit = Some(MarkdownPlanLimit::StructuralDepth);
                break;
            }
            metrics.max_depth = metrics.max_depth.max(depth);
        }
        metrics.events = next_events;
        metrics.retained_bytes = next_retained;
        metrics.embed_descriptors = next_embeds;
        block_retained_bytes = block_retained_bytes.saturating_add(retained_bytes);
        block.push(event.into_static());
        if matches!(block.last(), Some(Event::End(_))) {
            depth = depth.saturating_sub(1);
        }

        if depth == 0 {
            if block.len() > MARKDOWN_EVENTS_PER_PROJECTION_SLICE {
                block.clear();
                limit = Some(MarkdownPlanLimit::TopLevelBlock);
                break;
            }
            if block_retained_bytes > MARKDOWN_BYTES_PER_PROJECTION_SLICE {
                block.clear();
                limit = Some(MarkdownPlanLimit::ProjectionBytes);
                break;
            }
            if (batch.len().saturating_add(block.len()) > MARKDOWN_EVENTS_PER_PROJECTION_SLICE
                || batch_retained_bytes.saturating_add(block_retained_bytes)
                    > MARKDOWN_BYTES_PER_PROJECTION_SLICE)
                && !batch.is_empty()
            {
                batches.push(MarkdownEventBatch {
                    events: std::mem::take(&mut batch),
                    retained_bytes: std::mem::take(&mut batch_retained_bytes),
                });
            }
            batch.append(&mut block);
            batch_retained_bytes = batch_retained_bytes.saturating_add(block_retained_bytes);
            block_retained_bytes = 0;
        }
    }

    if !batch.is_empty() {
        batches.push(MarkdownEventBatch {
            events: batch,
            retained_bytes: batch_retained_bytes,
        });
    }

    Some(MarkdownRenderPlan {
        batches,
        metrics,
        limit,
    })
}

fn is_embed_start(event: &Event<'_>) -> bool {
    matches!(
        event,
        Event::Start(Tag::Table(_) | Tag::CodeBlock(_) | Tag::Image { .. })
    )
}

fn event_retained_bytes(event: &Event<'_>) -> usize {
    match event {
        Event::Text(value)
        | Event::Code(value)
        | Event::InlineMath(value)
        | Event::DisplayMath(value)
        | Event::Html(value)
        | Event::InlineHtml(value)
        | Event::FootnoteReference(value) => value.len(),
        Event::Start(
            Tag::Link {
                dest_url,
                title,
                id,
                ..
            }
            | Tag::Image {
                dest_url,
                title,
                id,
                ..
            },
        ) => dest_url
            .len()
            .saturating_add(title.len())
            .saturating_add(id.len()),
        Event::Start(Tag::FootnoteDefinition(label)) => label.len(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    #[test]
    fn ordinary_blocks_are_packed_without_splitting() {
        let mut markdown = String::new();
        for index in 0..400 {
            writeln!(markdown, "paragraph {index}\n").expect("write paragraph fixture");
        }
        let plan = plan_markdown(&markdown);
        assert!(plan.is_complete());
        assert!(plan.batches.len() > 1);
        assert!(
            plan.batches
                .iter()
                .all(|batch| batch.len() <= MARKDOWN_EVENTS_PER_PROJECTION_SLICE)
        );
        assert!(
            plan.batches
                .iter()
                .all(|batch| batch.retained_bytes() <= MARKDOWN_BYTES_PER_PROJECTION_SLICE)
        );
        assert_eq!(plan.projected_events(), plan.metrics.events);
    }

    #[test]
    fn one_dense_block_uses_an_explicit_limited_terminal() {
        let markdown = (0..300).map(|_| "**x** ").collect::<String>();
        let plan = plan_markdown(&markdown);
        assert_eq!(plan.limit, Some(MarkdownPlanLimit::TopLevelBlock));
        assert!(plan.batches.is_empty());
    }

    #[test]
    fn image_flood_stops_at_descriptor_budget() {
        let mut markdown = String::new();
        for index in 0..=MAX_MARKDOWN_EMBED_DESCRIPTORS {
            writeln!(markdown, "![image](image-{index}.png)\n").expect("write image-flood fixture");
        }
        let plan = plan_markdown(&markdown);
        assert_eq!(plan.limit, Some(MarkdownPlanLimit::EmbedDescriptors));
        assert_eq!(
            plan.metrics.embed_descriptors,
            MAX_MARKDOWN_EMBED_DESCRIPTORS
        );
    }

    #[test]
    fn oversized_source_retains_no_events() {
        let markdown = "x".repeat(MAX_MARKDOWN_SOURCE_BYTES + 1);
        let plan = plan_markdown(&markdown);
        assert_eq!(plan.limit, Some(MarkdownPlanLimit::SourceBytes));
        assert_eq!(plan.metrics.events, 0);
        assert!(plan.batches.is_empty());
    }

    #[test]
    fn one_large_text_block_uses_projection_byte_terminal() {
        let markdown = "x".repeat(MARKDOWN_BYTES_PER_PROJECTION_SLICE + 1);
        let plan = plan_markdown(&markdown);
        assert_eq!(plan.limit, Some(MarkdownPlanLimit::ProjectionBytes));
        assert!(plan.batches.is_empty());
    }

    #[test]
    fn cancelled_planner_retains_no_partial_plan() {
        let cancel = AtomicBool::new(true);
        assert!(plan_markdown_cancellable("paragraph", &cancel).is_none());
    }

    #[test]
    fn stale_generation_cannot_replace_a_new_terminal() {
        let mut session = MarkdownRenderSession::default();
        let stale = session.begin();
        assert!(session.transition(stale, MarkdownRenderState::Projecting));
        let current = session.begin();
        assert!(!session.transition(stale, MarkdownRenderState::Complete));
        assert!(session.transition(current, MarkdownRenderState::Limited));
        assert_eq!(session.state(), MarkdownRenderState::Limited);
        assert!(!session.pending());
    }

    #[test]
    fn cancellation_invalidates_pending_generation() {
        let mut session = MarkdownRenderSession::default();
        let stale = session.begin();
        let cancelled_generation = session.cancel();
        assert_ne!(stale, cancelled_generation);
        assert_eq!(session.state(), MarkdownRenderState::Cancelled);
        assert!(!session.pending());
        assert!(!session.transition(stale, MarkdownRenderState::Complete));
    }

    #[test]
    fn image_admission_enforces_count_and_bytes_then_reuses_released_capacity() {
        let mut admission = MarkdownImageAdmission::default();
        for _ in 0..4 {
            assert!(admission.try_admit(100, 4, 400));
        }
        assert!(!admission.try_admit(1, 4, 400));
        assert_eq!(
            admission.snapshot(),
            MarkdownImageAdmissionSnapshot {
                owned_count: 4,
                owned_bytes: 400,
                high_water_count: 4,
                high_water_bytes: 400,
            }
        );
        admission.release(100);
        assert!(admission.try_admit(100, 4, 400));
        admission.release(u64::MAX);
        assert_eq!(admission.snapshot().owned_bytes, 0);
    }
}
