// SPDX-License-Identifier: GPL-3.0-or-later

//! GTK-free Markdown planning with deterministic retention and slice budgets.
//!
//! The parser owns every retained event before it crosses back to the GTK
//! adapter. Events are packed into projection batches whose boundaries fall
//! only where no inline state is open — every frame the event stream still has
//! open is a block container — so a batch may end *inside* a table, list,
//! blockquote, code block, or definition list without losing inline state when
//! GTK yields. Each batch names the open containers it expects at its first
//! event and the ones it leaves open at its last ([`MarkdownCarrySignature`]),
//! and consecutive batches chain through those signatures; the projector holds
//! the matching continuation across turns and refuses a batch that does not
//! chain.
//!
//! A unit that fits no slice and has no admissible interior cut is not a stop:
//! its events are dropped, one [`MarkdownBlockOmission`] marker is recorded at
//! that position, and planning continues. Only the global ceilings
//! ([`MAX_MARKDOWN_EVENTS`], [`MAX_MARKDOWN_RETAINED_BYTES`],
//! [`MAX_MARKDOWN_EMBED_DESCRIPTORS`], [`MAX_MARKDOWN_STRUCTURE_DEPTH`]) end
//! planning early, recorded as a [`MarkdownPlanLimit`].

use lushtext_model::values::RenderGeneration;
pub mod events;
pub mod inline_footnotes;

#[cfg(any(test, feature = "property-tests", feature = "fuzzing"))]
pub use events::count_markdown_events;
#[cfg(any(test, feature = "test-utils"))]
pub use events::parse_markdown_events;
pub use events::{
    MarkdownAlignment, MarkdownBlockOmission, MarkdownBlockQuoteKind, MarkdownCarrySignature,
    MarkdownCodeBlockKind, MarkdownEvent, MarkdownEventBatch, MarkdownHeadingLevel,
    MarkdownLinkType, MarkdownMetadataBlockKind, MarkdownOmissionMarker, MarkdownOmissionReason,
    MarkdownOmissionScope, MarkdownOpenContainer, MarkdownPlanLimit, MarkdownPlanMetrics,
    MarkdownRenderPlan, MarkdownTag, MarkdownTagEnd, UnretainedEmbedCounts,
};

#[cfg(feature = "property-tests")]
pub use inline_footnotes::lower_inline_footnotes_for_property_test;
#[cfg(feature = "fuzzing")]
pub use inline_footnotes::{
    FuzzedMarkdownPreprocess, lowered_markdown_for_fuzzing, preprocess_markdown_for_fuzzing,
};
pub use inline_footnotes::{
    INLINE_FOOTNOTE_LABEL_PREFIX, InlineFootnoteLowering, LOWERING_CANCEL_CHECK_BYTES,
    MAX_INLINE_FOOTNOTE_REPLACEMENTS, lower_inline_footnotes, lower_inline_footnotes_cancellable,
};

use pulldown_cmark::{Event, Options, Parser, Tag};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use events::{convert_alignment, convert_blockquote_kind, convert_event};

/// Largest source accepted for automatic Markdown preview planning.
///
/// Kind: presentation cap; a power of two, never rounded down (`power-of-two-thresholds` D12).
pub const MAX_MARKDOWN_SOURCE_BYTES: usize = 4 * 1024 * 1024;
/// Maximum parser events retained by one render generation.
///
/// Kind: presentation cap; 50,000 rounded up to a power of two
/// (`power-of-two-thresholds` D12; its bound measured in task 9.5 M13).
pub const MAX_MARKDOWN_EVENTS: usize = 64 * 1024;
/// Maximum structural nesting accepted from the event stream.
///
/// Kind: presentation cap; a power of two, never rounded down (`power-of-two-thresholds` D12).
pub const MAX_MARKDOWN_STRUCTURE_DEPTH: usize = 128;
/// Maximum table, code-block, and image descriptors retained in one plan.
///
/// Kind: presentation cap; a power of two, never rounded down (`power-of-two-thresholds` D12).
pub const MAX_MARKDOWN_EMBED_DESCRIPTORS: usize = 256;
/// Maximum bytes retained by event text and link/embed descriptors.
///
/// Kind: presentation cap; a power of two, never rounded down (`power-of-two-thresholds` D12).
pub const MAX_MARKDOWN_RETAINED_BYTES: usize = 8 * 1024 * 1024;
/// Maximum event/node work one GTK projection turn may apply.
///
/// Kind: cap; a power of two, rounded down.
pub const MARKDOWN_EVENTS_PER_PROJECTION_SLICE: usize = 256;
/// Maximum retained text/link bytes one GTK projection turn may consume.
///
/// Kind: cap; a power of two, rounded down.
pub const MARKDOWN_BYTES_PER_PROJECTION_SLICE: usize = 256 * 1024;
/// Maximum retained bytes one in-flight **code block** may carry across turns.
///
/// The value equals the projection-side `MAX_PREVIEW_CODE_BLOCK_BYTES` budget
/// in `ui/markdown_preview/mod.rs`, above which the preview already discards
/// code text and substitutes its in-place fallback widget, so retaining more
/// than this is dead memory by construction. It sits well below
/// [`MARKDOWN_BYTES_PER_PROJECTION_SLICE`], so a code block crosses this
/// ceiling before the slice byte budget.
///
/// The ceiling bounds *retention only*: crossing it carries the unretained
/// source byte count forward on the emitted omission, so the projector still
/// evaluates the block's true total size and picks the same presentation it
/// picks today. It deliberately does **not** apply to tables, whose retention
/// is bounded by [`MAX_MARKDOWN_CARRIED_TABLE_CELLS`] instead, because a
/// byte ceiling would truncate large-byte tables that render completely today.
///
/// Kind: presentation cap; a power of two, never rounded down (`power-of-two-thresholds` D12).
pub const MAX_MARKDOWN_CARRIED_EMBED_BYTES: usize = 64 * 1024;
/// Maximum cells one in-flight table may retain while carried across turns.
///
/// This mirrors `MAX_PREVIEW_TABLE_CELLS` in
/// `ui/markdown_preview/tables.rs`. Services must not import the GTK adapter,
/// so the value is duplicated deliberately; change both together. Past this
/// ceiling the projector already replaces the whole table with one in-place
/// fallback widget, so a crossing here can never change a rendered outcome,
/// and every table within the ceiling retains and renders in full regardless
/// of its byte size. Retention stays bounded without a byte ceiling because
/// cell count times the minimum per-cell markup is bounded by
/// [`MAX_MARKDOWN_SOURCE_BYTES`].
///
/// Kind: presentation cap; 1,000 rounded up to a power of two
/// (`power-of-two-thresholds` D12; its bound measured in task 9.5 M13).
pub const MAX_MARKDOWN_CARRIED_TABLE_CELLS: usize = 1_024;
/// Maximum top-level omission placeholder widgets one render generation builds.
///
/// Further top-level omissions project as accessible inline text markers, and
/// container-segment omissions never build a widget at all, so a pathological
/// document cannot turn omission markers into unbounded widget work.
///
/// Kind: presentation cap; a power of two, never rounded down (`power-of-two-thresholds` D12).
pub const MAX_MARKDOWN_PLACEHOLDER_WIDGETS: usize = 64;

/// Render a byte budget in the KiB units the user-facing copy uses.
pub(super) const fn kibibytes(bytes: usize) -> usize {
    bytes / 1024
}

/// Current generation-owned terminal state of the preview renderer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MarkdownRenderState {
    #[default]
    Idle,
    Planning,
    Projecting,
    Complete,
    /// A complete projection that still contains at least one user-visible
    /// omission marker.
    ///
    /// This is terminal and not pending, exactly like [`Self::Limited`], so
    /// readiness semantics are unchanged. It is deliberately distinct from
    /// `Limited`: the document was planned to its end, and only named units
    /// inside it were replaced by markers.
    Simplified,
    Limited,
    Failed,
    Cancelled,
}

/// GTK-free generation and readiness state for one Markdown preview adapter.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MarkdownRenderSession {
    generation: RenderGeneration,
    state: MarkdownRenderState,
    pending: bool,
}

impl MarkdownRenderSession {
    /// Invalidate older work and begin one pending planning generation.
    pub fn begin(&mut self) -> RenderGeneration {
        self.generation = self.generation.next();
        self.state = MarkdownRenderState::Planning;
        self.pending = true;
        self.generation
    }

    /// Transition current work without allowing stale generations to publish.
    pub fn transition(&mut self, generation: RenderGeneration, state: MarkdownRenderState) -> bool {
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
    pub fn cancel(&mut self) -> RenderGeneration {
        self.generation = self.generation.next();
        self.state = MarkdownRenderState::Cancelled;
        self.pending = false;
        self.generation
    }

    #[must_use]
    pub fn generation(self) -> RenderGeneration {
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
    pub fn is_current(self, generation: RenderGeneration) -> bool {
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
pub(super) fn markdown_render_options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_GFM);
    options.insert(Options::ENABLE_DEFINITION_LIST);
    options
}

/// Parse owned Markdown into inline-safe, bounded projection batches.
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

/// One open structural frame, classified for batch-boundary admissibility.
#[derive(Clone, Debug)]
enum MarkdownFrame {
    /// A block container: a batch boundary may occur while it is open.
    Block(MarkdownOpenContainer),
    /// A frame with inline or text-flow state: no boundary may occur inside it.
    InlineFlow,
}

/// Classify one opening tag as cut-permitting or cut-forbidding.
///
/// The match is deliberately wildcard-free over every `pulldown_cmark::Tag`
/// variant, so a parser upgrade that adds a tag is a compile error rather than
/// a silently cuttable default.
///
/// `FootnoteDefinition` is cut-forbidding by decision. It is structurally a
/// block container, but it has no checkpoint taxonomy row, segment unit, or
/// spec scenario, so an oversized footnote definition takes the top-level
/// omission path instead. `TableHead` is cut-forbidding because header cells
/// are emitted directly inside it with no intervening `TableRow`; cutting after
/// `End(TableHead)` is still admissible because only `Table` remains open.
fn classify_frame(tag: &Tag<'_>) -> MarkdownFrame {
    match tag {
        Tag::List(start) => MarkdownFrame::Block(MarkdownOpenContainer::List {
            ordered: start.is_some(),
            next_number: start.unwrap_or_default(),
        }),
        Tag::Item => MarkdownFrame::Block(MarkdownOpenContainer::Item),
        Tag::Table(alignments) => MarkdownFrame::Block(MarkdownOpenContainer::Table {
            alignments: alignments.iter().copied().map(convert_alignment).collect(),
        }),
        Tag::BlockQuote(kind) => MarkdownFrame::Block(MarkdownOpenContainer::BlockQuote {
            kind: kind.map(convert_blockquote_kind),
        }),
        Tag::CodeBlock(kind) => MarkdownFrame::Block(MarkdownOpenContainer::CodeBlock {
            kind: MarkdownCodeBlockKind::from_parser(kind),
        }),
        Tag::DefinitionList => MarkdownFrame::Block(MarkdownOpenContainer::DefinitionList),
        Tag::DefinitionListDefinition => {
            MarkdownFrame::Block(MarkdownOpenContainer::DefinitionListDefinition)
        }
        Tag::Paragraph
        | Tag::Heading { .. }
        | Tag::HtmlBlock
        | Tag::FootnoteDefinition(_)
        | Tag::DefinitionListTitle
        | Tag::TableHead
        | Tag::TableRow
        | Tag::TableCell
        | Tag::Emphasis
        | Tag::Strong
        | Tag::Strikethrough
        | Tag::Superscript
        | Tag::Subscript
        | Tag::Link { .. }
        | Tag::Image { .. }
        | Tag::MetadataBlock(_) => MarkdownFrame::InlineFlow,
    }
}

/// Open frames plus a memoized carry signature for their current shape.
///
/// Every event asks whether a boundary is admissible, so materializing a fresh
/// signature per event would allocate once per event even for blocks that fit
/// one slice. The signature is rebuilt only when the stack actually changes and
/// is shared by reference until then.
#[derive(Default)]
struct MarkdownFrameStack {
    frames: Vec<MarkdownFrame>,
    revision: u64,
    cached: Option<(u64, Rc<MarkdownCarrySignature>)>,
}

impl MarkdownFrameStack {
    fn len(&self) -> usize {
        self.frames.len()
    }

    fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Whether a batch boundary here would leave no inline state open.
    fn is_admissible(&self) -> bool {
        Self::frames_are_admissible(&self.frames)
    }

    fn frames_are_admissible(frames: &[MarkdownFrame]) -> bool {
        frames
            .iter()
            .all(|frame| matches!(frame, MarkdownFrame::Block(_)))
    }

    fn push(&mut self, frame: MarkdownFrame) {
        self.frames.push(frame);
        self.revision = self.revision.wrapping_add(1);
    }

    fn pop(&mut self) -> Option<MarkdownFrame> {
        self.revision = self.revision.wrapping_add(1);
        self.frames.pop()
    }

    /// Advance the innermost ordered list's next ordinal after one closed item.
    fn advance_enclosing_list_ordinal(&mut self) {
        if let Some(MarkdownFrame::Block(MarkdownOpenContainer::List {
            ordered,
            next_number,
        })) = self.frames.last_mut()
            && *ordered
        {
            *next_number = next_number.saturating_add(1);
            self.revision = self.revision.wrapping_add(1);
        }
    }

    /// Describe the open block containers at an admissible boundary.
    fn signature(&mut self) -> Rc<MarkdownCarrySignature> {
        if let Some((revision, signature)) = &self.cached
            && *revision == self.revision
        {
            return Rc::clone(signature);
        }
        let signature = Rc::new(MarkdownCarrySignature {
            containers: self
                .frames
                .iter()
                .filter_map(|frame| match frame {
                    MarkdownFrame::Block(container) => Some(container.clone()),
                    MarkdownFrame::InlineFlow => None,
                })
                .collect(),
        });
        self.cached = Some((self.revision, Rc::clone(&signature)));
        signature
    }
}

/// A position inside the current top-level block where a batch may be cut.
struct BlockCheckpoint {
    events: usize,
    retained_bytes: usize,
    carry: Rc<MarkdownCarrySignature>,
}

/// Which ceiling bounds the retention of the innermost open embedded block.
enum EmbedBound {
    /// A code block, bounded by [`MAX_MARKDOWN_CARRIED_EMBED_BYTES`].
    CodeBlockBytes { bytes: usize },
    /// A table, bounded by [`MAX_MARKDOWN_CARRIED_TABLE_CELLS`].
    TableCells { cells: usize },
}

/// Running retention charge for the innermost open table or code block.
struct EmbedCharge {
    frame_index: usize,
    /// Block position immediately after this container's own `Start` event.
    ///
    /// A withdrawal may never reach below this, or the container's `Start`
    /// would be dropped while its `End` is still retained. The last recorded
    /// checkpoint is not a sufficient floor: a cut-forbidding ancestor such as
    /// `FootnoteDefinition` suppresses every checkpoint inside itself, so an
    /// embedded block nested there has no checkpoint of its own.
    block_start: usize,
    /// Block byte prefix immediately after this container's own `Start` event.
    block_start_bytes: usize,
    bound: EmbedBound,
    unretained: Option<UnretainedEmbedCounts>,
}

impl EmbedCharge {
    /// The omission reason this container's ceiling reports when crossed.
    fn crossing_reason(&self) -> MarkdownOmissionReason {
        match self.bound {
            EmbedBound::CodeBlockBytes { .. } => MarkdownOmissionReason::CarriedEmbedBytes,
            EmbedBound::TableCells { .. } => MarkdownOmissionReason::CarriedEmbedCells,
        }
    }

    /// Whether retaining this event would cross the container's ceiling.
    fn would_cross(&self, event: &Event<'_>, retained_bytes: usize) -> bool {
        match self.bound {
            EmbedBound::CodeBlockBytes { bytes } => {
                bytes.saturating_add(retained_bytes) > MAX_MARKDOWN_CARRIED_EMBED_BYTES
            }
            EmbedBound::TableCells { cells } => {
                matches!(event, Event::Start(Tag::TableCell))
                    && cells.saturating_add(1) > MAX_MARKDOWN_CARRIED_TABLE_CELLS
            }
        }
    }

    /// Charge one retained event against the container's ceiling.
    fn charge(&mut self, event: &Event<'_>, retained_bytes: usize) {
        match &mut self.bound {
            EmbedBound::CodeBlockBytes { bytes } => {
                *bytes = bytes.saturating_add(retained_bytes);
            }
            EmbedBound::TableCells { cells } => {
                *cells = cells
                    .saturating_add(usize::from(matches!(event, Event::Start(Tag::TableCell))));
            }
        }
    }
}

/// Append-only accumulator turning one event stream into bounded batches.
#[derive(Default)]
struct MarkdownPlanner {
    batches: Vec<MarkdownEventBatch>,
    batch: Vec<MarkdownEvent>,
    batch_retained_bytes: usize,
    batch_omissions: Vec<MarkdownOmissionMarker>,
    batch_expected_carry: MarkdownCarrySignature,
    block: Vec<MarkdownEvent>,
    block_retained_bytes: usize,
    block_checkpoints: Vec<BlockCheckpoint>,
    block_omissions: Vec<(usize, MarkdownBlockOmission)>,
    omissions: usize,
}

impl MarkdownPlanner {
    fn record_checkpoint(&mut self, carry: Rc<MarkdownCarrySignature>) {
        self.block_checkpoints.push(BlockCheckpoint {
            events: self.block.len(),
            retained_bytes: self.block_retained_bytes,
            carry,
        });
    }

    fn close_batch(&mut self, open_carry: MarkdownCarrySignature) {
        let expected_carry = std::mem::replace(&mut self.batch_expected_carry, open_carry.clone());
        self.batches.push(MarkdownEventBatch {
            events: std::mem::take(&mut self.batch),
            retained_bytes: std::mem::take(&mut self.batch_retained_bytes),
            omissions: std::mem::take(&mut self.batch_omissions),
            expected_carry,
            open_carry,
        });
    }

    /// Record one marker and count it, so the plan's omission count and the
    /// markers its batches actually carry can never disagree.
    fn record_omission(&mut self, at_event: usize, omission: MarkdownBlockOmission) {
        self.omissions = self.omissions.saturating_add(1);
        self.batch_omissions
            .push(MarkdownOmissionMarker { at_event, omission });
    }

    /// Roll the retained block back to its last admissible checkpoint, never
    /// below the crossed container's own `Start`.
    ///
    /// A carried-embed crossing stops retaining mid-row, mid-cell, or mid-text,
    /// so the partially retained unit must be withdrawn or the projector would
    /// see an unbalanced event stream and a carry signature that disagrees with
    /// the containers it actually holds open. The container's own start is the
    /// hard floor: withdrawing past it would drop a `Start` whose `End` is
    /// still retained, and would fold the *enclosing* container's content into
    /// this container's unretained charge. Nothing of the current block has been
    /// emitted yet, so this stays append-only with respect to closed batches.
    fn withdraw_partial_unit(
        &mut self,
        floor_events: usize,
        floor_bytes: usize,
        counts: &mut UnretainedEmbedCounts,
    ) {
        debug_assert!(floor_events <= self.block.len());
        let (events, retained_bytes) = match self.block_checkpoints.last() {
            Some(checkpoint) if checkpoint.events >= floor_events => {
                (checkpoint.events, checkpoint.retained_bytes)
            }
            _ => (floor_events, floor_bytes),
        };
        debug_assert!(events <= self.block.len());
        debug_assert!(
            self.block_omissions
                .iter()
                .all(|(position, _)| *position <= events),
            "an earlier marker cannot sit inside the withdrawn unit"
        );
        for event in self.block.drain(events..) {
            counts.cells = counts.cells.saturating_add(usize::from(matches!(
                event,
                MarkdownEvent::Start(MarkdownTag::TableCell)
            )));
        }
        counts.source_bytes = counts
            .source_bytes
            .saturating_add(self.block_retained_bytes.saturating_sub(retained_bytes));
        self.block_retained_bytes = retained_bytes;
    }

    /// Pack one indivisible run of block events, cutting the batch if needed.
    ///
    /// Returns the batch index the unit's first event landed at, so a marker
    /// recorded inside the unit keeps its own position.
    fn append_unit(
        &mut self,
        events: Vec<MarkdownEvent>,
        bytes: usize,
        start_carry: &MarkdownCarrySignature,
    ) -> usize {
        if (self.batch.len().saturating_add(events.len()) > MARKDOWN_EVENTS_PER_PROJECTION_SLICE
            || self.batch_retained_bytes.saturating_add(bytes)
                > MARKDOWN_BYTES_PER_PROJECTION_SLICE)
            && !self.batch.is_empty()
        {
            self.close_batch(start_carry.clone());
        }
        let base = self.batch.len();
        self.batch.extend(events);
        self.batch_retained_bytes = self.batch_retained_bytes.saturating_add(bytes);
        base
    }

    /// Flush carried-embed markers recorded at or before one block position.
    ///
    /// `placement` maps a block position into the batch the unit just landed
    /// in. `None` means that unit was dropped, so its own omission already
    /// replaces this content and a nested crossing marker inside it would be a
    /// second marker for the same hole; the marker is consumed, not recorded.
    fn flush_block_markers(
        &mut self,
        upto: usize,
        next_marker: &mut usize,
        placement: Option<(usize, usize)>,
    ) {
        while self
            .block_omissions
            .get(*next_marker)
            .is_some_and(|(position, _)| *position <= upto)
        {
            let (position, omission) = self.block_omissions[*next_marker];
            *next_marker += 1;
            if let Some((unit_start, base)) = placement {
                let at_event = base.saturating_add(position.saturating_sub(unit_start));
                self.record_omission(at_event, omission);
            }
        }
    }

    /// Commit one finished top-level block, sub-slicing it only if it overflows.
    fn commit_block(&mut self) {
        let block_events = self.block.len();
        if block_events == 0 && self.block_omissions.is_empty() {
            self.block_checkpoints.clear();
            self.block_retained_bytes = 0;
            return;
        }
        let fits = block_events <= MARKDOWN_EVENTS_PER_PROJECTION_SLICE
            && self.block_retained_bytes <= MARKDOWN_BYTES_PER_PROJECTION_SLICE;
        let mut next_marker = 0usize;
        if fits {
            let events = std::mem::take(&mut self.block);
            let bytes = self.block_retained_bytes;
            let base = self.append_unit(events, bytes, &MarkdownCarrySignature::default());
            self.flush_block_markers(block_events, &mut next_marker, Some((0, base)));
        } else {
            self.sub_slice_block(&mut next_marker);
        }
        self.block_retained_bytes = 0;
        self.block_checkpoints.clear();
        self.block_omissions.clear();
    }

    /// Emit an overflowing block one checkpoint-delimited segment at a time.
    fn sub_slice_block(&mut self, next_marker: &mut usize) {
        let block = std::mem::take(&mut self.block);
        let block_events = block.len();
        let checkpoints = std::mem::take(&mut self.block_checkpoints);
        // Move each segment out of the block instead of cloning it: the blocks
        // this path handles are exactly the large ones.
        let mut remaining = block.into_iter();
        let mut start = 0usize;
        let mut start_bytes = 0usize;
        let mut start_carry = MarkdownCarrySignature::default();
        for checkpoint in &checkpoints {
            let segment_bytes = checkpoint.retained_bytes.saturating_sub(start_bytes);
            let segment_len = checkpoint.events.saturating_sub(start);
            let mut placement = None;
            if segment_len > 0 {
                if segment_len > MARKDOWN_EVENTS_PER_PROJECTION_SLICE
                    || segment_bytes > MARKDOWN_BYTES_PER_PROJECTION_SLICE
                {
                    let reason = if segment_len > MARKDOWN_EVENTS_PER_PROJECTION_SLICE {
                        MarkdownOmissionReason::SliceEvents
                    } else {
                        MarkdownOmissionReason::SliceBytes
                    };
                    let scope = if checkpoint.carry.is_empty() {
                        MarkdownOmissionScope::TopLevelBlock
                    } else {
                        MarkdownOmissionScope::ContainerSegment
                    };
                    remaining.by_ref().take(segment_len).for_each(drop);
                    let at_event = self.batch.len();
                    self.record_omission(
                        at_event,
                        MarkdownBlockOmission {
                            reason,
                            scope,
                            unretained: UnretainedEmbedCounts::default(),
                        },
                    );
                } else {
                    let events: Vec<MarkdownEvent> = remaining.by_ref().take(segment_len).collect();
                    placement =
                        Some((start, self.append_unit(events, segment_bytes, &start_carry)));
                }
            }
            self.flush_block_markers(checkpoint.events, next_marker, placement);
            start = checkpoint.events;
            start_bytes = checkpoint.retained_bytes;
            start_carry = (*checkpoint.carry).clone();
        }
        debug_assert_eq!(
            start, block_events,
            "a block ends at an admissible boundary"
        );
    }

    fn finish(
        mut self,
        metrics: MarkdownPlanMetrics,
        limit: Option<MarkdownPlanLimit>,
    ) -> MarkdownRenderPlan {
        if !self.batch.is_empty() || !self.batch_omissions.is_empty() {
            self.close_batch(MarkdownCarrySignature::default());
        }
        MarkdownRenderPlan {
            batches: self.batches,
            metrics: MarkdownPlanMetrics {
                omissions: self.omissions,
                ..metrics
            },
            limit,
        }
    }
}

// Test-only cancellation seam.
//
// A caller-owned `AtomicBool` can only be flipped before planning starts, so
// the 64-event checkpoint *inside* a sub-sliced block would never be exercised.
// Arming this thread-local flips the caller's token once the parse reaches the
// requested event, deterministically and on the same thread. The storage does
// not compile outside tests.
#[cfg(test)]
thread_local! {
    static CANCEL_AFTER_EVENTS: std::cell::Cell<Option<usize>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn apply_test_cancellation(event_index: usize, cancel: Option<&AtomicBool>) {
    if let Some(cancel) = cancel
        && CANCEL_AFTER_EVENTS
            .get()
            .is_some_and(|after| event_index >= after)
    {
        cancel.store(true, Ordering::Release);
    }
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

    let mut planner = MarkdownPlanner::default();
    let mut frames = MarkdownFrameStack::default();
    let mut embed: Option<EmbedCharge> = None;
    let mut limit = None;

    for (event_index, event) in Parser::new_ext(markdown, markdown_render_options()).enumerate() {
        #[cfg(test)]
        apply_test_cancellation(event_index, cancel);
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

        if let Event::Start(tag) = &event {
            if frames.len().saturating_add(1) > MAX_MARKDOWN_STRUCTURE_DEPTH {
                limit = Some(MarkdownPlanLimit::StructuralDepth);
                break;
            }
            frames.push(classify_frame(tag));
            metrics.max_depth = metrics.max_depth.max(frames.len());
            if embed.is_none() {
                let bound = match tag {
                    Tag::CodeBlock(_) => Some(EmbedBound::CodeBlockBytes { bytes: 0 }),
                    Tag::Table(_) => Some(EmbedBound::TableCells { cells: 0 }),
                    _ => None,
                };
                if let Some(bound) = bound {
                    // This event is the container's own start, and it is always
                    // retained: no crossing can fire on it, because a fresh
                    // charge is empty and neither ceiling counts a
                    // `Start(Table)` or a `Start(CodeBlock)`.
                    embed = Some(EmbedCharge {
                        frame_index: frames.len().saturating_sub(1),
                        block_start: planner.block.len().saturating_add(1),
                        block_start_bytes: planner
                            .block_retained_bytes
                            .saturating_add(retained_bytes),
                        bound,
                        unretained: None,
                    });
                }
            }
        }
        metrics.events = next_events;
        metrics.retained_bytes = next_retained;
        metrics.embed_descriptors = next_embeds;

        let closes_embed = embed.as_ref().is_some_and(|charge| {
            matches!(event, Event::End(_)) && frames.len() == charge.frame_index.saturating_add(1)
        });
        let mut retain = true;
        let mut crossed_here = false;
        if let Some(charge) = embed.as_mut() {
            if charge.unretained.is_some() {
                retain = closes_embed;
            } else if !closes_embed && charge.would_cross(&event, retained_bytes) {
                charge.unretained = Some(UnretainedEmbedCounts::default());
                crossed_here = true;
                retain = false;
            } else {
                charge.charge(&event, retained_bytes);
            }
            if !retain && let Some(counts) = charge.unretained.as_mut() {
                counts.source_bytes = counts.source_bytes.saturating_add(retained_bytes);
                counts.cells = counts
                    .cells
                    .saturating_add(usize::from(matches!(event, Event::Start(Tag::TableCell))));
            }
        }
        // Withdraw the partially retained row/cell/line the crossing
        // interrupted so the retained stream stays structurally balanced.
        if crossed_here
            && let Some(charge) = embed.as_mut()
            && let Some(counts) = charge.unretained.as_mut()
        {
            planner.withdraw_partial_unit(charge.block_start, charge.block_start_bytes, counts);
        }

        if closes_embed
            && let Some(charge) = embed.as_ref()
            && let Some(counts) = charge.unretained
        {
            planner.block_omissions.push((
                planner.block.len(),
                MarkdownBlockOmission {
                    reason: charge.crossing_reason(),
                    scope: MarkdownOmissionScope::ContainerSegment,
                    unretained: counts,
                },
            ));
        }

        let is_end = matches!(event, Event::End(_));
        if retain {
            planner.block_retained_bytes =
                planner.block_retained_bytes.saturating_add(retained_bytes);
            planner.block.push(convert_event(event));
        }

        if is_end {
            if matches!(
                frames.pop(),
                Some(MarkdownFrame::Block(MarkdownOpenContainer::Item))
            ) {
                frames.advance_enclosing_list_ordinal();
            }
            if closes_embed {
                embed = None;
            }
        }

        if frames.is_admissible() {
            let carry = frames.signature();
            planner.record_checkpoint(carry);
            if frames.is_empty() {
                planner.commit_block();
            }
        }
    }

    Some(planner.finish(metrics, limit))
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
mod tests;
