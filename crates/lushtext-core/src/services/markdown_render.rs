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

use pulldown_cmark::{Alignment, BlockQuoteKind, CodeBlockKind, Event, Options, Parser, Tag};
use std::rc::Rc;
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
pub const MAX_MARKDOWN_CARRIED_TABLE_CELLS: usize = 1_000;
/// Maximum top-level omission placeholder widgets one render generation builds.
///
/// Further top-level omissions project as accessible inline text markers, and
/// container-segment omissions never build a widget at all, so a pathological
/// document cannot turn omission markers into unbounded widget work.
pub const MAX_MARKDOWN_PLACEHOLDER_WIDGETS: usize = 64;

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
            Self::InlineFootnotes => {
                "Markdown preview limited because inline footnote expansion exceeds its budget"
            }
        }
    }
}

/// Why one unit of Markdown content could not be projected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkdownOmissionReason {
    /// The unit needs more events than one projection slice permits.
    SliceEvents,
    /// The unit needs more retained bytes than one projection slice permits.
    SliceBytes,
    /// An open code block crossed [`MAX_MARKDOWN_CARRIED_EMBED_BYTES`].
    CarriedEmbedBytes,
    /// An open table crossed [`MAX_MARKDOWN_CARRIED_TABLE_CELLS`].
    CarriedEmbedCells,
}

impl MarkdownOmissionReason {
    /// Whether this omission is content the preview genuinely cannot render.
    ///
    /// The two slice reasons replace content the user would otherwise have
    /// read, so they are projected as markers and counted toward the
    /// complete-with-omissions terminal. The two carried-embed reasons are
    /// charge carriers: they only move unretained counts across the
    /// planner/projector seam for a block the projector already replaces
    /// wholesale with its own in-place fallback, which names that block's true
    /// size. Rendering a second marker beside that fallback would duplicate the
    /// explanation, and counting it would report omissions for a document that
    /// renders exactly as it does today.
    #[must_use]
    pub fn is_user_visible(self) -> bool {
        match self {
            Self::SliceEvents | Self::SliceBytes => true,
            Self::CarriedEmbedBytes | Self::CarriedEmbedCells => false,
        }
    }
}

/// How much of the document one omission replaces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkdownOmissionScope {
    /// A whole top-level block with no inline-safe interior checkpoint.
    TopLevelBlock,
    /// One unit inside a container whose sibling units still render.
    ContainerSegment,
}

/// Content the planner parsed and charged but deliberately did not retain.
///
/// Only a carried-embed crossing produces non-zero counts. Projection charges
/// them onto the in-flight embedded-block buffer before the block is finished,
/// so the projection-side widget budgets still decide the block's presentation
/// from its true total size: a crossed table has already exceeded the
/// projector's cell budget and keeps its single fallback widget, and a crossed
/// code block reports its full observed byte count.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UnretainedEmbedCounts {
    pub source_bytes: usize,
    pub cells: usize,
}

/// One unit of Markdown the planner replaced with an accessible marker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MarkdownBlockOmission {
    pub reason: MarkdownOmissionReason,
    pub scope: MarkdownOmissionScope,
    pub unretained: UnretainedEmbedCounts,
}

impl MarkdownBlockOmission {
    /// Accessible marker copy naming the crossed budget.
    ///
    /// The copy deliberately does not name the omitted unit; the projector owns
    /// that wording because only it knows which container the marker landed in.
    #[must_use]
    pub fn marker_text(self) -> String {
        let subject = match self.scope {
            MarkdownOmissionScope::TopLevelBlock => "Markdown preview omitted one block",
            MarkdownOmissionScope::ContainerSegment => "Markdown preview omitted part of one block",
        };
        match self.reason {
            MarkdownOmissionReason::SliceEvents => format!(
                "{subject} that exceeds {MARKDOWN_EVENTS_PER_PROJECTION_SLICE} render events"
            ),
            MarkdownOmissionReason::SliceBytes => format!(
                "{subject} that exceeds {} KiB",
                kibibytes(MARKDOWN_BYTES_PER_PROJECTION_SLICE)
            ),
            MarkdownOmissionReason::CarriedEmbedBytes => format!(
                "{subject} after {} KiB of carried content",
                kibibytes(MAX_MARKDOWN_CARRIED_EMBED_BYTES)
            ),
            MarkdownOmissionReason::CarriedEmbedCells => {
                format!("{subject} after {MAX_MARKDOWN_CARRIED_TABLE_CELLS} carried table cells")
            }
        }
    }
}

/// Render a byte budget in the KiB units the user-facing copy uses.
const fn kibibytes(bytes: usize) -> usize {
    bytes / 1024
}

/// One omission marker at its own position inside a projection batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MarkdownOmissionMarker {
    /// Number of the batch's events that precede this marker.
    pub at_event: usize,
    pub omission: MarkdownBlockOmission,
}

/// Fenced or indented identity of a code block carried across projection turns.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MarkdownCodeBlockKind {
    Indented,
    Fenced { language: String },
}

impl MarkdownCodeBlockKind {
    /// Describe one open code block for a carry signature.
    ///
    /// Both sides of the seam construct this from the same parser tag: the
    /// planner when it records a checkpoint, and the projector when it opens
    /// the matching in-flight block, so the two descriptions are comparable.
    #[must_use]
    pub fn from_tag(kind: &CodeBlockKind<'_>) -> Self {
        match kind {
            CodeBlockKind::Indented => Self::Indented,
            CodeBlockKind::Fenced(language) => Self::Fenced {
                language: language.to_string(),
            },
        }
    }
}

/// One block container left open at a projection batch boundary.
#[derive(Clone, Debug, PartialEq)]
pub enum MarkdownOpenContainer {
    List { ordered: bool, next_number: u64 },
    Item,
    Table { alignments: Vec<Alignment> },
    BlockQuote { kind: Option<BlockQuoteKind> },
    CodeBlock { kind: MarkdownCodeBlockKind },
    DefinitionList,
    DefinitionListDefinition,
}

/// Structural continuation one projection batch expects or leaves open.
///
/// This is the seam value object between the GTK-free planner and the GTK
/// projector: batch *n*'s open signature is batch *n+1*'s expected signature,
/// so a projector holding the wrong continuation is a checkable mismatch rather
/// than invisible render corruption.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MarkdownCarrySignature {
    containers: Vec<MarkdownOpenContainer>,
}

impl MarkdownCarrySignature {
    #[must_use]
    pub fn containers(&self) -> &[MarkdownOpenContainer] {
        &self.containers
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.containers.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.containers.is_empty()
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
    pub omissions: usize,
}

/// One inline-safe GTK projection batch.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkdownEventBatch {
    events: Vec<Event<'static>>,
    retained_bytes: usize,
    omissions: Vec<MarkdownOmissionMarker>,
    expected_carry: MarkdownCarrySignature,
    open_carry: MarkdownCarrySignature,
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

    /// Omission markers to project in order at their own event positions.
    #[must_use]
    pub fn omissions(&self) -> &[MarkdownOmissionMarker] {
        &self.omissions
    }

    /// Continuation this batch requires before its first event is applied.
    #[must_use]
    pub fn expected_carry(&self) -> &MarkdownCarrySignature {
        &self.expected_carry
    }

    /// Continuation this batch leaves open after its last event is applied.
    #[must_use]
    pub fn open_carry(&self) -> &MarkdownCarrySignature {
        &self.open_carry
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
    /// Whether a global budget stopped planning before the end of the document.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.limit.is_none()
    }

    /// Units the planner replaced with a marker while still planning on.
    #[must_use]
    pub fn omissions(&self) -> usize {
        self.metrics.omissions
    }

    /// Omissions a reader can actually notice, which is what the terminal state
    /// and the announced count report.
    ///
    /// This deliberately excludes the carried-embed charge carriers (see
    /// [`MarkdownOmissionReason::is_user_visible`]), so a plan whose only
    /// omissions are crossings publishes the ordinary complete terminal. Owning
    /// the distinction here keeps marker rendering, terminal choice, and the
    /// announcement reading one number instead of each re-deriving the policy.
    #[must_use]
    pub fn user_visible_omissions(&self) -> usize {
        self.batches
            .iter()
            .flat_map(|batch| batch.omissions.iter())
            .filter(|marker| marker.omission.reason.is_user_visible())
            .count()
    }

    /// Top-level omissions, which are the only ones that may build a widget.
    #[must_use]
    pub fn top_level_omissions(&self) -> usize {
        self.batches
            .iter()
            .flat_map(|batch| batch.omissions.iter())
            .filter(|marker| marker.omission.scope == MarkdownOmissionScope::TopLevelBlock)
            .count()
    }

    #[must_use]
    pub fn projected_events(&self) -> usize {
        self.batches.iter().map(MarkdownEventBatch::len).sum()
    }
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
            alignments: alignments.clone(),
        }),
        Tag::BlockQuote(kind) => {
            MarkdownFrame::Block(MarkdownOpenContainer::BlockQuote { kind: *kind })
        }
        Tag::CodeBlock(kind) => MarkdownFrame::Block(MarkdownOpenContainer::CodeBlock {
            kind: MarkdownCodeBlockKind::from_tag(kind),
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
    batch: Vec<Event<'static>>,
    batch_retained_bytes: usize,
    batch_omissions: Vec<MarkdownOmissionMarker>,
    batch_expected_carry: MarkdownCarrySignature,
    block: Vec<Event<'static>>,
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
            counts.cells = counts
                .cells
                .saturating_add(usize::from(matches!(event, Event::Start(Tag::TableCell))));
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
        events: Vec<Event<'static>>,
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
                    let events: Vec<Event<'static>> =
                        remaining.by_ref().take(segment_len).collect();
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
            planner.block.push(event.into_static());
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
mod tests {
    use super::*;
    use std::fmt::Write as _;

    /// Concatenate every text-bearing event a plan would hand to the projector.
    fn projected_text(plan: &MarkdownRenderPlan) -> String {
        let mut text = String::new();
        for batch in &plan.batches {
            for event in batch.events() {
                if let Event::Text(value) | Event::Code(value) = event {
                    text.push_str(value);
                    text.push('\n');
                }
            }
        }
        text
    }

    fn helm_readme_fixture() -> String {
        let mut markdown =
            String::from("# Example Chart\n\nIntro prose paragraph.\n\n## Values\n\n");
        markdown.push_str("| key | type | default | description | notes |\n");
        markdown.push_str("| --- | --- | --- | --- | --- |\n");
        for index in 0..40 {
            writeln!(
                markdown,
                "| key-{index} | string | `\"\"` | description {index} | note {index} |"
            )
            .expect("write values row");
        }
        markdown.push_str(
            "\n## Upgrading\n\nTAIL-UPGRADING-PROSE\n\n## License\n\nTAIL-LICENSE-PROSE\n",
        );
        markdown
    }

    /// A fenced block whose *bytes* cross the slice budget.
    ///
    /// `pulldown_cmark` 0.13 coalesces a fenced block's lines into one `Text`
    /// event, so a code block never crosses the per-slice *event* budget and
    /// has no interior checkpoint. The byte budget is its only real trigger.
    fn oversized_code_block_fixture() -> String {
        let mut markdown = String::from("# Script\n\n```sh\n");
        while markdown.len() <= MARKDOWN_BYTES_PER_PROJECTION_SLICE {
            markdown.push_str("echo an-ordinary-shell-line-of-moderate-length\n");
        }
        markdown.push_str("```\n\nTAIL-AFTER-CODE\n");
        markdown
    }

    fn oversized_list_fixture() -> String {
        let mut markdown = String::from("# List\n\n");
        for index in 0..120 {
            writeln!(markdown, "- item-{index}").expect("write list item");
        }
        markdown.push_str("\nTAIL-AFTER-LIST\n");
        markdown
    }

    fn oversized_blockquote_fixture() -> String {
        let mut markdown = String::from("# Quote\n\n");
        for index in 0..90 {
            writeln!(markdown, "> quoted-{index}\n>").expect("write quoted paragraph");
        }
        markdown.push_str("\nTAIL-AFTER-QUOTE\n");
        markdown
    }

    fn list_with_one_overflowing_item_fixture() -> String {
        let mut markdown = String::from("# Mixed list\n\n");
        for index in 0..60 {
            // A loose list puts each item's content inside a `Paragraph`, which
            // is cut-forbidding, so item 17 is one indivisible segment.
            if index == 17 {
                let dense = (0..200).map(|_| "**x** ").collect::<String>();
                writeln!(markdown, "- {dense}\n").expect("write dense item");
            } else {
                writeln!(markdown, "- item-{index}\n").expect("write list item");
            }
        }
        markdown.push_str("\nTAIL-AFTER-MIXED-LIST\n");
        markdown
    }

    /// Entries in [`oversized_definition_list_fixture`].
    const DEFINITION_ENTRIES: usize = 60;

    fn oversized_definition_list_fixture() -> String {
        let mut markdown = String::from("# Definitions\n\n");
        for index in 0..DEFINITION_ENTRIES {
            writeln!(markdown, "term-{index}\n: definition-{index}\n").expect("write definition");
        }
        markdown.push_str("\nTAIL-AFTER-DEFINITIONS\n");
        markdown
    }

    /// Lines in [`indented_code_block_fixture`].
    const INDENTED_CODE_LINES: usize = 400;

    /// An **indented** code block over the per-slice event budget.
    ///
    /// Unlike a fenced block, an indented block emits one `Text` event per
    /// line, so its retained-text checkpoints are reachable and the block is
    /// sub-sliced between lines. Its total stays well under
    /// [`MAX_MARKDOWN_CARRIED_EMBED_BYTES`], so nothing is dropped.
    fn indented_code_block_fixture() -> String {
        let mut markdown = String::from("# Indented\n\n");
        for index in 0..INDENTED_CODE_LINES {
            writeln!(markdown, "    indented-line-{index}").expect("write indented line");
        }
        markdown.push_str("\nTAIL-AFTER-INDENTED-CODE\n");
        markdown
    }

    fn nested_table_in_list_fixture() -> String {
        let mut markdown = String::from("# Nested\n\n- outer item\n\n");
        markdown.push_str("  | a | b | c | d | e |\n  | --- | --- | --- | --- | --- |\n");
        for index in 0..40 {
            writeln!(
                markdown,
                "  | nested-{index} | b{index} | c{index} | d{index} | e{index} |"
            )
            .expect("write nested row");
        }
        markdown.push_str("\nTAIL-AFTER-NESTED\n");
        markdown
    }

    /// Header plus body cells produced by [`huge_byte_table_fixture`].
    const HUGE_BYTE_TABLE_CELLS: usize = 3 + 3 * 200;

    /// A table well inside [`MAX_MARKDOWN_CARRIED_TABLE_CELLS`] whose total
    /// bytes are far above [`MAX_MARKDOWN_CARRIED_EMBED_BYTES`]. It must retain
    /// every cell: the byte ceiling is scoped to code blocks precisely so a
    /// table like this keeps rendering exactly as it does today.
    fn huge_byte_table_fixture() -> String {
        let cell = "z".repeat(200);
        let mut markdown = String::from("# Wide bytes\n\n| a | b | c |\n| --- | --- | --- |\n");
        for _ in 0..200 {
            writeln!(markdown, "| {cell} | {cell} | {cell} |").expect("write wide row");
        }
        markdown.push_str("\nTAIL-AFTER-WIDE\n");
        markdown
    }

    /// Build a table with `columns` header cells and `rows` body rows.
    fn cell_table(columns: usize, rows: usize) -> String {
        let mut markdown = String::new();
        for _ in 0..columns {
            markdown.push_str("| h ");
        }
        markdown.push_str("|\n");
        for _ in 0..columns {
            markdown.push_str("| --- ");
        }
        markdown.push_str("|\n");
        for _ in 0..rows {
            for _ in 0..columns {
                markdown.push_str("| c ");
            }
            markdown.push_str("|\n");
        }
        markdown
    }

    /// Cells produced by a `columns` x `rows` [`cell_table`], header included.
    fn cell_table_cells(columns: usize, rows: usize) -> usize {
        columns * (rows + 1)
    }

    /// Columns and body rows of the table that crosses the cell ceiling.
    const PAST_CEILING_COLUMNS: usize = 4;
    const PAST_CEILING_ROWS: usize = 250;

    /// A table past [`MAX_MARKDOWN_CARRIED_TABLE_CELLS`], where the projector
    /// already replaces the whole table with one fallback widget, so the
    /// crossing cannot change any rendered outcome.
    fn table_past_the_cell_ceiling_fixture() -> String {
        let mut markdown = String::from("# Past ceiling\n\n");
        markdown.push_str(&cell_table(PAST_CEILING_COLUMNS, PAST_CEILING_ROWS));
        markdown.push_str("\nTAIL-AFTER-PAST-CEILING\n");
        markdown
    }

    /// Cells the past-ceiling fixture's table contains.
    fn past_ceiling_cells() -> usize {
        cell_table_cells(PAST_CEILING_COLUMNS, PAST_CEILING_ROWS)
    }

    /// Count retained `Start(TableCell)` events across a whole plan.
    fn retained_cells(plan: &MarkdownRenderPlan) -> usize {
        plan.batches
            .iter()
            .flat_map(MarkdownEventBatch::events)
            .filter(|event| matches!(event, Event::Start(Tag::TableCell)))
            .count()
    }

    /// Sum every batch's retained bytes.
    fn retained_bytes(plan: &MarkdownRenderPlan) -> usize {
        plan.batches
            .iter()
            .map(MarkdownEventBatch::retained_bytes)
            .sum()
    }

    /// Restore the test-only cancellation seam even if an assertion panics.
    struct CancelAfterEvents;

    impl CancelAfterEvents {
        fn arm(events: usize) -> Self {
            CANCEL_AFTER_EVENTS.set(Some(events));
            Self
        }
    }

    impl Drop for CancelAfterEvents {
        fn drop(&mut self) {
            CANCEL_AFTER_EVENTS.set(None);
        }
    }

    /// The single omission a plan is expected to carry.
    fn single_omission(plan: &MarkdownRenderPlan) -> MarkdownBlockOmission {
        let markers: Vec<MarkdownOmissionMarker> = plan
            .batches
            .iter()
            .flat_map(|batch| batch.omissions().iter().copied())
            .collect();
        assert_eq!(markers.len(), 1, "expected exactly one omission marker");
        markers[0].omission
    }

    fn byte_budget_fixture() -> String {
        let mut markdown = String::from("# Bytes\n\n");
        markdown.push_str(&"x".repeat(MARKDOWN_BYTES_PER_PROJECTION_SLICE + 1));
        markdown.push_str("\n\nTAIL-AFTER-BYTES\n");
        markdown
    }

    /// Assert one batch's retained events are structurally balanced.
    ///
    /// A batch may close containers it inherited and leave containers open, but
    /// its net depth change must agree with its carry signatures and every
    /// locally opened tag must be closed by the matching end. This is what
    /// catches a crossing that retained `Start(TableRow)` without its end.
    fn assert_events_balanced(batch: &MarkdownEventBatch) {
        let mut opened: Vec<pulldown_cmark::TagEnd> = Vec::new();
        let mut closed_inherited = 0usize;
        for event in batch.events() {
            match event {
                Event::Start(tag) => opened.push(tag.to_end()),
                Event::End(end) => match opened.pop() {
                    Some(expected) => assert_eq!(
                        expected, *end,
                        "a batch closed a tag it did not open: {end:?}"
                    ),
                    None => closed_inherited += 1,
                },
                _ => {}
            }
        }
        assert!(
            closed_inherited <= batch.expected_carry().len(),
            "a batch closed more inherited containers than it inherited"
        );
        assert_eq!(
            batch.expected_carry().len() - closed_inherited + opened.len(),
            batch.open_carry().len(),
            "a batch's net structural depth must match its carry signatures"
        );
    }

    /// Assert the shared plan invariants every emitted plan must satisfy.
    fn assert_plan_invariants(plan: &MarkdownRenderPlan) {
        let mut expected = MarkdownCarrySignature::default();
        for batch in &plan.batches {
            assert!(
                batch.len() <= MARKDOWN_EVENTS_PER_PROJECTION_SLICE,
                "batch of {} events exceeds the slice event budget",
                batch.len()
            );
            assert!(
                batch.retained_bytes() <= MARKDOWN_BYTES_PER_PROJECTION_SLICE,
                "batch of {} bytes exceeds the slice byte budget",
                batch.retained_bytes()
            );
            assert!(batch.expected_carry().len() <= MAX_MARKDOWN_STRUCTURE_DEPTH);
            assert!(batch.open_carry().len() <= MAX_MARKDOWN_STRUCTURE_DEPTH);
            assert_eq!(*batch.expected_carry(), expected, "batch carry must chain");
            assert!(
                batch
                    .omissions()
                    .iter()
                    .all(|marker| marker.at_event <= batch.len())
            );
            assert_events_balanced(batch);
            expected = batch.open_carry().clone();
        }
        assert!(expected.is_empty(), "the last batch must close everything");
        let markers: usize = plan
            .batches
            .iter()
            .map(|batch| batch.omissions().len())
            .sum();
        assert_eq!(markers, plan.omissions());
    }

    #[test]
    fn helm_style_values_table_is_projected_with_its_document_tail() {
        let plan = plan_markdown(&helm_readme_fixture());
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), 0);
        assert_plan_invariants(&plan);
        let text = projected_text(&plan);
        assert!(text.contains("Intro prose paragraph."));
        assert!(text.contains("key-39"));
        assert!(text.contains("TAIL-UPGRADING-PROSE"));
        assert!(text.contains("TAIL-LICENSE-PROSE"));
        assert!(plan.batches.len() > 1, "an oversized table is sub-sliced");
    }

    #[test]
    fn oversized_code_block_keeps_the_document_tail() {
        let plan = plan_markdown(&oversized_code_block_fixture());
        assert!(plan.is_complete());
        assert_plan_invariants(&plan);
        assert!(projected_text(&plan).contains("TAIL-AFTER-CODE"));
    }

    #[test]
    fn oversized_list_is_projected_completely_with_its_tail() {
        let plan = plan_markdown(&oversized_list_fixture());
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), 0);
        assert_plan_invariants(&plan);
        let text = projected_text(&plan);
        for index in 0..120 {
            assert!(
                text.contains(&format!("item-{index}\n")),
                "item-{index} missing"
            );
        }
        assert!(text.contains("TAIL-AFTER-LIST"));
        assert!(plan.batches.len() > 1, "an oversized list is sub-sliced");
    }

    #[test]
    fn oversized_blockquote_is_projected_completely_with_its_tail() {
        let plan = plan_markdown(&oversized_blockquote_fixture());
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), 0);
        assert_plan_invariants(&plan);
        let text = projected_text(&plan);
        assert!(text.contains("quoted-0\n"));
        assert!(text.contains("quoted-89\n"));
        assert!(text.contains("TAIL-AFTER-QUOTE"));
    }

    #[test]
    fn projection_byte_budget_omits_only_the_oversized_block() {
        let plan = plan_markdown(&byte_budget_fixture());
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), 1);
        assert_plan_invariants(&plan);
        let omission = single_omission(&plan);
        assert_eq!(omission.reason, MarkdownOmissionReason::SliceBytes);
        assert_eq!(omission.scope, MarkdownOmissionScope::TopLevelBlock);
        assert!(projected_text(&plan).contains("TAIL-AFTER-BYTES"));
    }

    #[test]
    fn one_overflowing_list_item_keeps_its_siblings_and_the_tail() {
        let plan = plan_markdown(&list_with_one_overflowing_item_fixture());
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), 1);
        assert_plan_invariants(&plan);
        let omission = single_omission(&plan);
        assert_eq!(omission.reason, MarkdownOmissionReason::SliceEvents);
        assert_eq!(
            omission.scope,
            MarkdownOmissionScope::ContainerSegment,
            "one overflowing item must not be scoped to the whole list"
        );
        let text = projected_text(&plan);
        for index in 0..60 {
            if index == 17 {
                continue;
            }
            assert!(
                text.contains(&format!("item-{index}\n")),
                "item-{index} missing"
            );
        }
        assert!(text.contains("TAIL-AFTER-MIXED-LIST"));
    }

    #[test]
    fn one_overflowing_definition_body_keeps_its_siblings_and_the_tail() {
        // The definition-list analogue of the overflowing list item: one body's
        // inline run is indivisible, so only that body is replaced while every
        // other title and definition still renders.
        // A tight definition body holds its inline events directly, so it is
        // cuttable between bare leaves. Wrapping the whole run in one `Strong`
        // span is what makes it genuinely indivisible.
        // The closing `**` must not be preceded by whitespace or CommonMark
        // refuses the delimiter run and the span never opens.
        let dense = format!("**{}z**", (0..140).map(|_| "a `c` ").collect::<String>());
        let mut markdown = String::from("# Definitions\n\n");
        for index in 0..DEFINITION_ENTRIES {
            if index == 17 {
                writeln!(markdown, "term-{index}\n: {dense}\n").expect("write dense definition");
            } else {
                writeln!(markdown, "term-{index}\n: definition-{index}\n")
                    .expect("write definition");
            }
        }
        markdown.push_str("\nTAIL-AFTER-DEFINITIONS\n");

        let plan = plan_markdown(&markdown);
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), 1);
        assert_plan_invariants(&plan);
        let omission = single_omission(&plan);
        assert_eq!(omission.reason, MarkdownOmissionReason::SliceEvents);
        assert_eq!(
            omission.scope,
            MarkdownOmissionScope::ContainerSegment,
            "one overflowing body must not be scoped to the whole list"
        );
        let text = projected_text(&plan);
        for index in 0..DEFINITION_ENTRIES {
            assert!(
                text.contains(&format!("term-{index}")),
                "term-{index} missing"
            );
            if index != 17 {
                assert!(
                    text.contains(&format!("definition-{index}")),
                    "definition-{index} missing"
                );
            }
        }
        assert!(text.contains("TAIL-AFTER-DEFINITIONS"));
    }

    #[test]
    fn a_flat_oversized_list_carries_its_containers_across_every_turn() {
        // No nesting at all: the carry path must work for the plainest shape
        // there is, and every consecutive pair of batches must chain through a
        // signature that still holds the list open.
        let mut markdown = String::from("# Flat\n\n");
        for index in 0..200 {
            writeln!(markdown, "- flat-{index}").expect("write flat list item");
        }
        markdown.push_str("\nTAIL-AFTER-FLAT-LIST\n");

        let plan = plan_markdown(&markdown);
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), 0);
        assert_plan_invariants(&plan);
        assert!(plan.batches.len() > 1, "a 200-item list must sub-slice");
        let carried = plan
            .batches
            .iter()
            .filter(|batch| {
                batch
                    .open_carry()
                    .containers()
                    .iter()
                    .any(|container| matches!(container, MarkdownOpenContainer::List { .. }))
            })
            .count();
        assert!(
            carried >= plan.batches.len() - 1,
            "every batch but the last must leave the flat list open: {carried} of {}",
            plan.batches.len()
        );
        let text = projected_text(&plan);
        for index in 0..200 {
            assert!(text.contains(&format!("flat-{index}")), "flat-{index} lost");
        }
        assert!(text.contains("TAIL-AFTER-FLAT-LIST"));
    }

    #[test]
    fn a_carried_blockquote_keeps_its_alert_kind_across_a_turn() {
        // A GitHub alert callout is a blockquote with a `kind`. If the carry
        // signature dropped that kind, the projector would silently resume the
        // callout as a plain quote, so the kind must survive the boundary.
        // Each quoted paragraph is separated by a bare `>` so the alert body is
        // many cuttable paragraphs rather than one indivisible lazy-continued
        // paragraph.
        let mut markdown = String::from("> [!WARNING]\n");
        for index in 0..200 {
            writeln!(markdown, "> alert-line-{index}\n>").expect("write alert paragraph");
        }
        markdown.push_str("\nTAIL-AFTER-ALERT\n");

        let plan = plan_markdown(&markdown);
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), 0);
        assert_plan_invariants(&plan);
        assert!(plan.batches.len() > 1, "the alert must span turns");
        let carried_kinds: Vec<Option<BlockQuoteKind>> = plan
            .batches
            .iter()
            .flat_map(|batch| batch.open_carry().containers())
            .filter_map(|container| match container {
                MarkdownOpenContainer::BlockQuote { kind } => Some(*kind),
                _ => None,
            })
            .collect();
        assert!(
            !carried_kinds.is_empty(),
            "the cut must land inside the alert blockquote"
        );
        assert!(
            carried_kinds
                .iter()
                .all(|kind| *kind == Some(BlockQuoteKind::Warning)),
            "every carried frame must keep the alert kind: {carried_kinds:?}"
        );
        assert!(projected_text(&plan).contains("TAIL-AFTER-ALERT"));
    }

    #[test]
    fn an_indented_code_block_past_the_code_ceiling_crosses_on_the_code_track() {
        // The indented shape reaches the code byte ceiling through per-line
        // events, which is the path a fenced block cannot take: the pinned
        // parser coalesces a fence into one `Text` event.
        let line = "    ".to_string() + &"z".repeat(508);
        let lines = MAX_MARKDOWN_CARRIED_EMBED_BYTES / 512 + 8;
        let mut markdown = String::from("# Indented\n\n");
        for _ in 0..lines {
            markdown.push_str(&line);
            markdown.push('\n');
        }
        markdown.push_str("\nTAIL-AFTER-INDENTED-CROSSING\n");

        let plan = plan_markdown(&markdown);
        assert!(plan.is_complete());
        assert_eq!(
            plan.user_visible_omissions(),
            0,
            "a carried-embed crossing is a charge carrier, not a user-visible omission"
        );
        assert_plan_invariants(&plan);
        let omission = single_omission(&plan);
        assert_eq!(omission.reason, MarkdownOmissionReason::CarriedEmbedBytes);
        assert_eq!(
            omission.unretained.cells, 0,
            "a code block has no cells to charge"
        );
        assert!(
            omission.unretained.source_bytes > 0,
            "the unretained remainder must be carried forward for the projector"
        );
        assert!(
            retained_bytes(&plan) <= MAX_MARKDOWN_CARRIED_EMBED_BYTES,
            "retention must stop at the code ceiling"
        );
        assert!(
            projected_text(&plan).contains("TAIL-AFTER-INDENTED-CROSSING"),
            "the document after the crossing must still render"
        );
    }

    #[test]
    fn every_global_budget_is_still_terminal() {
        let events = "x\n\n".repeat(MAX_MARKDOWN_EVENTS);
        assert_eq!(
            plan_markdown(&events).limit,
            Some(MarkdownPlanLimit::Events)
        );
        let depth = format!("{}deep{}", "> ".repeat(200), "\n");
        assert_eq!(
            plan_markdown(&depth).limit,
            Some(MarkdownPlanLimit::StructuralDepth)
        );
    }

    #[test]
    fn table_head_forbids_a_cut_between_header_cells() {
        assert!(matches!(
            classify_frame(&Tag::TableHead),
            MarkdownFrame::InlineFlow
        ));
        assert!(matches!(
            classify_frame(&Tag::Table(vec![Alignment::None])),
            MarkdownFrame::Block(MarkdownOpenContainer::Table { .. })
        ));
        // A cut after `End(TableHead)` leaves only `Table` open, which is
        // admissible; a cut between header cells would leave `TableHead` open.
        let head_open = [
            MarkdownFrame::Block(MarkdownOpenContainer::Table {
                alignments: Vec::new(),
            }),
            classify_frame(&Tag::TableHead),
        ];
        assert!(!MarkdownFrameStack::frames_are_admissible(&head_open));
        assert!(MarkdownFrameStack::frames_are_admissible(&head_open[..1]));
    }

    #[test]
    fn footnote_definitions_are_cut_forbidding_by_decision() {
        assert!(matches!(
            classify_frame(&Tag::FootnoteDefinition("label".into())),
            MarkdownFrame::InlineFlow
        ));
    }

    #[test]
    fn depth_zero_is_the_trivial_admissible_boundary() {
        assert!(MarkdownFrameStack::frames_are_admissible(&[]));
        assert!(MarkdownFrameStack::frames_are_admissible(&[
            MarkdownFrame::Block(MarkdownOpenContainer::Item)
        ]));
        assert!(!MarkdownFrameStack::frames_are_admissible(&[
            MarkdownFrame::Block(MarkdownOpenContainer::Item),
            MarkdownFrame::InlineFlow,
        ]));
    }

    #[test]
    fn ordinary_documents_keep_todays_batch_packing() {
        // Blocks that fit a slice are never sub-sliced, so a document of
        // ordinary blocks packs into exactly the batches it packed into before
        // checkpoint sub-slicing existed: every batch is filled to just under
        // the slice budget by whole blocks.
        let mut markdown = String::new();
        for index in 0..400 {
            writeln!(markdown, "paragraph {index}\n").expect("write paragraph fixture");
        }
        let plan = plan_markdown(&markdown);
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), 0);
        assert_plan_invariants(&plan);
        assert_eq!(plan.batches.len(), 5);
        assert_eq!(
            plan.batches
                .iter()
                .map(MarkdownEventBatch::len)
                .collect::<Vec<_>>(),
            vec![255, 255, 255, 255, 180]
        );
        assert!(
            plan.batches
                .iter()
                .all(|batch| batch.expected_carry().is_empty() && batch.open_carry().is_empty()),
            "whole-block packing never leaves a container open"
        );
    }

    #[test]
    fn sub_sliced_batches_stay_within_both_slice_budgets() {
        for markdown in [
            helm_readme_fixture(),
            oversized_list_fixture(),
            oversized_blockquote_fixture(),
            list_with_one_overflowing_item_fixture(),
            nested_table_in_list_fixture(),
            oversized_definition_list_fixture(),
            indented_code_block_fixture(),
        ] {
            let plan = plan_markdown(&markdown);
            assert_plan_invariants(&plan);
        }
    }

    #[test]
    fn an_oversized_definition_list_is_sub_sliced_at_definition_boundaries() {
        let plan = plan_markdown(&oversized_definition_list_fixture());
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), 0);
        assert_plan_invariants(&plan);
        assert!(
            plan.batches.len() > 1,
            "an oversized definition list is sub-sliced"
        );
        let text = projected_text(&plan);
        for index in 0..DEFINITION_ENTRIES {
            assert!(
                text.contains(&format!("definition-{index}")),
                "definition-{index} missing"
            );
        }
        assert!(text.contains("TAIL-AFTER-DEFINITIONS"));
        let carried = plan.batches.iter().any(|batch| {
            batch.open_carry().containers().iter().any(|container| {
                matches!(
                    container,
                    MarkdownOpenContainer::DefinitionList
                        | MarkdownOpenContainer::DefinitionListDefinition
                )
            })
        });
        assert!(carried, "the cut must land inside the definition list");
    }

    #[test]
    fn an_indented_code_block_is_sub_sliced_at_text_run_boundaries() {
        // The fenced-body coalescing that makes code-block checkpoints
        // unreachable is fenced-only: an indented block emits one `Text` per
        // line, so this is the reachable retained-code-text checkpoint path.
        let plan = plan_markdown(&indented_code_block_fixture());
        assert!(plan.is_complete());
        assert_eq!(
            plan.omissions(),
            0,
            "the block stays inside the carried byte ceiling"
        );
        assert_plan_invariants(&plan);
        assert!(
            plan.batches.len() > 1,
            "an indented code block over one slice is sub-sliced"
        );
        let carried = plan.batches.iter().any(|batch| {
            matches!(
                batch.open_carry().containers(),
                [MarkdownOpenContainer::CodeBlock { .. }]
            )
        });
        assert!(carried, "a cut must leave the code block itself open");
        let text = projected_text(&plan);
        assert!(text.contains("indented-line-0\n"));
        assert!(text.contains(&format!("indented-line-{}\n", INDENTED_CODE_LINES - 1)));
        assert!(text.contains("TAIL-AFTER-INDENTED-CODE"));
    }

    #[test]
    fn an_indivisible_segment_subsumes_a_crossing_marker_inside_it() {
        // A footnote definition is cut-forbidding, so a footnote holding both
        // dense inline content and an over-ceiling table is one indivisible
        // segment. Dropping it must report exactly one omission: the crossing
        // marker recorded inside the dropped unit is consumed, not re-reported,
        // because the segment's own omission already replaces that content.
        let dense = (0..200).map(|_| "**x** ").collect::<String>();
        let mut markdown = format!("See[^{FOOTNOTE_LABEL}].\n\n[^{FOOTNOTE_LABEL}]: {dense}\n\n");
        for line in cell_table(PAST_CEILING_COLUMNS, PAST_CEILING_ROWS).lines() {
            markdown.push_str("    ");
            markdown.push_str(line);
            markdown.push('\n');
        }
        markdown.push_str("\nTAIL-AFTER-SUBSUMED\n");

        let plan = plan_markdown(&markdown);
        assert!(plan.is_complete());
        assert_plan_invariants(&plan);
        assert_eq!(
            plan.omissions(),
            1,
            "a dropped segment must not also report the crossing inside it"
        );
        let omission = single_omission(&plan);
        assert_eq!(omission.reason, MarkdownOmissionReason::SliceEvents);
        assert_eq!(retained_cells(&plan), 0);
        assert!(projected_text(&plan).contains("TAIL-AFTER-SUBSUMED"));
    }

    #[test]
    fn state_extremes_keep_planning_to_the_end_of_the_document() {
        let dense = || (0..300).map(|_| "**x** ").collect::<String>();

        // Empty document.
        let plan = plan_markdown("");
        assert!(plan.is_complete());
        assert!(plan.batches.is_empty());
        assert_eq!(plan.omissions(), 0);
        assert_plan_invariants(&plan);

        // One tiny block.
        let plan = plan_markdown("tiny\n");
        assert_eq!(plan.batches.len(), 1);
        assert_eq!(plan.omissions(), 0);
        assert_plan_invariants(&plan);

        // One oversized block and nothing else.
        let plan = plan_markdown(&dense());
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), 1);
        assert_eq!(plan.projected_events(), 0);
        assert_plan_invariants(&plan);

        // Oversized first, middle, and last.
        for markdown in [
            format!("{}\n\nHEAD-A\n\nHEAD-B\n", dense()),
            format!("HEAD-A\n\n{}\n\nHEAD-B\n", dense()),
            format!("HEAD-A\n\nHEAD-B\n\n{}\n", dense()),
        ] {
            let plan = plan_markdown(&markdown);
            assert!(plan.is_complete());
            assert_eq!(plan.omissions(), 1);
            assert_plan_invariants(&plan);
            let text = projected_text(&plan);
            assert!(text.contains("HEAD-A"), "sibling lost: {text}");
            assert!(text.contains("HEAD-B"), "sibling lost: {text}");
        }

        // Several oversized blocks separated by ordinary prose.
        let markdown = format!(
            "{a}\n\nBETWEEN\n\n{b}\n\nTAIL-SEVERAL\n",
            a = dense(),
            b = dense()
        );
        let plan = plan_markdown(&markdown);
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), 2);
        assert_plan_invariants(&plan);
        let text = projected_text(&plan);
        assert!(text.contains("BETWEEN"));
        assert!(text.contains("TAIL-SEVERAL"));
    }

    #[test]
    fn a_container_whose_every_unit_overflows_keeps_its_shell_and_the_tail() {
        let dense = (0..200).map(|_| "**x** ").collect::<String>();
        let mut markdown = String::from("# Every item overflows\n\n");
        for _ in 0..6 {
            writeln!(markdown, "- {dense}\n").expect("write dense item");
        }
        markdown.push_str("\nTAIL-AFTER-ALL-OVERFLOW\n");

        let plan = plan_markdown(&markdown);
        assert!(plan.is_complete());
        assert_plan_invariants(&plan);
        assert_eq!(plan.omissions(), 6, "each item overflows on its own");
        assert!(
            plan.batches
                .iter()
                .flat_map(MarkdownEventBatch::omissions)
                .all(|marker| marker.omission.scope == MarkdownOmissionScope::ContainerSegment),
            "an overflowing item is a container segment, not the whole list"
        );
        // The list shell and its item structure survive: every `Item` start is
        // its own tiny segment, so the empty items still render.
        let items = plan
            .batches
            .iter()
            .flat_map(MarkdownEventBatch::events)
            .filter(|event| matches!(event, Event::Start(Tag::Item)))
            .count();
        assert_eq!(items, 6);
        assert!(projected_text(&plan).contains("TAIL-AFTER-ALL-OVERFLOW"));
    }

    #[test]
    fn an_oversized_block_followed_by_a_global_stop_reports_the_global_limit() {
        let dense = (0..300).map(|_| "**x** ").collect::<String>();
        let mut markdown = format!("{dense}\n\n");
        while markdown.len() < 512 * 1024 {
            markdown.push_str("filler paragraph\n\n");
        }
        let plan = plan_markdown(&markdown);
        assert_eq!(
            plan.limit,
            Some(MarkdownPlanLimit::Events),
            "the global budget still terminates planning"
        );
        assert!(plan.omissions() >= 1, "the oversized block still omitted");
        assert_plan_invariants(&plan);
    }

    #[test]
    fn an_oversized_block_at_the_depth_ceiling_is_still_omitted_and_continued() {
        let depth = MAX_MARKDOWN_STRUCTURE_DEPTH / 2;
        let dense = (0..300).map(|_| "**x** ").collect::<String>();
        let quote = "> ".repeat(depth);
        let markdown = format!("{quote}{dense}\n\nTAIL-AT-DEPTH\n");
        let plan = plan_markdown(&markdown);
        assert_eq!(
            plan.limit, None,
            "the fixture must stay inside the depth ceiling"
        );
        assert!(plan.metrics.max_depth >= depth);
        assert_plan_invariants(&plan);
        assert_eq!(plan.omissions(), 1);
        assert_eq!(
            single_omission(&plan).scope,
            MarkdownOmissionScope::ContainerSegment,
            "a nested oversized paragraph is a container segment"
        );
        assert!(projected_text(&plan).contains("TAIL-AT-DEPTH"));
    }

    #[test]
    fn a_table_nested_in_a_list_item_is_still_sub_sliced() {
        let plan = plan_markdown(&nested_table_in_list_fixture());
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), 0);
        assert_plan_invariants(&plan);
        let text = projected_text(&plan);
        assert!(text.contains("nested-39"));
        assert!(text.contains("TAIL-AFTER-NESTED"));
        let carried = plan.batches.iter().any(|batch| {
            batch
                .open_carry()
                .containers()
                .iter()
                .any(|container| matches!(container, MarkdownOpenContainer::Table { .. }))
        });
        assert!(carried, "a nested table must be cut at a row boundary");
    }

    #[test]
    fn a_large_byte_table_within_the_cell_ceiling_retains_every_cell() {
        let plan = plan_markdown(&huge_byte_table_fixture());
        assert!(plan.is_complete());
        assert_plan_invariants(&plan);
        assert_eq!(
            plan.omissions(),
            0,
            "the code-block byte ceiling must not reach tables"
        );
        assert_eq!(retained_cells(&plan), HUGE_BYTE_TABLE_CELLS);
        assert_eq!(retained_bytes(&plan), plan.metrics.retained_bytes);
        assert!(
            retained_bytes(&plan) > MAX_MARKDOWN_CARRIED_EMBED_BYTES,
            "the fixture must actually exceed the code-block byte ceiling"
        );
        assert!(projected_text(&plan).contains("TAIL-AFTER-WIDE"));
    }

    #[test]
    fn a_small_table_of_huge_cells_still_renders_completely() {
        // Few events but ~192 KiB of cells: this renders completely today, so
        // no ceiling may drop a cell from it.
        let cell = "q".repeat(32 * 1024);
        let mut markdown = String::from("| a | b |\n| --- | --- |\n");
        for _ in 0..3 {
            writeln!(markdown, "| {cell} | {cell} |").expect("write huge row");
        }
        markdown.push_str("\nTAIL-AFTER-HUGE-CELLS\n");
        let plan = plan_markdown(&markdown);
        assert!(plan.is_complete());
        assert_plan_invariants(&plan);
        assert_eq!(plan.omissions(), 0);
        assert_eq!(retained_cells(&plan), 2 + 3 * 2);
        assert_eq!(retained_bytes(&plan), plan.metrics.retained_bytes);
        assert!(projected_text(&plan).contains("TAIL-AFTER-HUGE-CELLS"));
    }

    #[test]
    fn a_table_past_the_cell_ceiling_reports_its_unretained_counts() {
        let plan = plan_markdown(&table_past_the_cell_ceiling_fixture());
        assert!(plan.is_complete());
        assert_plan_invariants(&plan);
        let omission = single_omission(&plan);
        assert_eq!(omission.reason, MarkdownOmissionReason::CarriedEmbedCells);
        assert_eq!(omission.scope, MarkdownOmissionScope::ContainerSegment);

        // The projector charges these counts onto the in-flight table, so its
        // cell check sees the block's true total and fires exactly as today.
        let total_cells = past_ceiling_cells();
        assert!(total_cells > MAX_MARKDOWN_CARRIED_TABLE_CELLS);
        assert_eq!(
            retained_cells(&plan) + omission.unretained.cells,
            total_cells
        );
        assert!(retained_cells(&plan) <= MAX_MARKDOWN_CARRIED_TABLE_CELLS);
        assert!(omission.unretained.cells > 0);
        assert_eq!(
            retained_bytes(&plan) + omission.unretained.source_bytes,
            plan.metrics.retained_bytes,
            "every charged byte is retained or reported as unretained"
        );
        assert!(projected_text(&plan).contains("TAIL-AFTER-PAST-CEILING"));
    }

    #[test]
    fn a_crossing_marker_sits_inside_its_still_open_container() {
        let plan = plan_markdown(&table_past_the_cell_ceiling_fixture());
        assert_plan_invariants(&plan);
        let batch = plan
            .batches
            .iter()
            .find(|batch| !batch.omissions().is_empty())
            .expect("the crossing emits a marker");
        let marker = batch.omissions()[0];
        assert!(
            matches!(batch.events().get(marker.at_event), Some(Event::End(_))),
            "the marker must precede the container's own end event"
        );
        // The crossing withdrew its partial row, so the last retained event
        // before the marker closes a row rather than opening one.
        assert!(matches!(
            batch.events().get(marker.at_event.saturating_sub(1)),
            Some(Event::End(pulldown_cmark::TagEnd::TableRow))
        ));
    }

    #[test]
    fn carried_embed_crossing_reports_code_block_bytes() {
        let mut markdown = String::from("```text\n");
        markdown.push_str(&"c".repeat(MAX_MARKDOWN_CARRIED_EMBED_BYTES + 4096));
        markdown.push_str("\n```\n\nTAIL-AFTER-BIG-CODE\n");
        let plan = plan_markdown(&markdown);
        assert!(plan.is_complete());
        assert_plan_invariants(&plan);
        let omission = single_omission(&plan);
        assert_eq!(omission.reason, MarkdownOmissionReason::CarriedEmbedBytes);
        assert_eq!(omission.unretained.cells, 0);
        assert!(omission.unretained.source_bytes > MAX_MARKDOWN_CARRIED_EMBED_BYTES);
        assert_eq!(
            retained_bytes(&plan) + omission.unretained.source_bytes,
            plan.metrics.retained_bytes
        );
        assert!(projected_text(&plan).contains("TAIL-AFTER-BIG-CODE"));
    }

    /// Lines and per-line length of the footnote-nested code block.
    const FOOTNOTE_CODE_LINES: usize = 80;
    const FOOTNOTE_CODE_LINE_LEN: usize = 900;
    /// Intro paragraph inside the footnote definition, before the embed.
    const FOOTNOTE_INTRO: &str = "an intro paragraph that belongs to the footnote, not the embed";
    /// A deliberately long label, so a withdrawal that reached below the
    /// embed's own start would visibly leak the footnote's own bytes.
    const FOOTNOTE_LABEL: &str = "an-unusually-long-footnote-label-for-leak-detection";

    /// Code text bytes the footnote-nested code block contributes.
    fn footnote_code_bytes() -> usize {
        FOOTNOTE_CODE_LINES * (FOOTNOTE_CODE_LINE_LEN + 1)
    }

    /// A code block past [`MAX_MARKDOWN_CARRIED_EMBED_BYTES`] nested inside a
    /// `FootnoteDefinition`, after an intro paragraph in the same footnote.
    ///
    /// `FootnoteDefinition` is cut-forbidding, so it suppresses every
    /// checkpoint inside itself and the embedded block has no checkpoint of its
    /// own. The withdrawal must still floor at the embed's own start, or the
    /// intro paragraph and the container starts go with it.
    fn footnote_nested_code_fixture() -> String {
        let mut markdown =
            format!("See[^{FOOTNOTE_LABEL}].\n\n[^{FOOTNOTE_LABEL}]: {FOOTNOTE_INTRO}\n\n");
        let line = "c".repeat(FOOTNOTE_CODE_LINE_LEN);
        for _ in 0..FOOTNOTE_CODE_LINES {
            markdown.push_str("        ");
            markdown.push_str(&line);
            markdown.push('\n');
        }
        markdown.push_str("\nTAIL-AFTER-FOOTNOTE-CODE\n");
        markdown
    }

    /// A table past [`MAX_MARKDOWN_CARRIED_TABLE_CELLS`] nested inside a
    /// `FootnoteDefinition`, after an intro paragraph in the same footnote.
    fn footnote_nested_table_fixture() -> String {
        let mut markdown =
            format!("See[^{FOOTNOTE_LABEL}].\n\n[^{FOOTNOTE_LABEL}]: {FOOTNOTE_INTRO}\n\n");
        for line in cell_table(PAST_CEILING_COLUMNS, PAST_CEILING_ROWS).lines() {
            markdown.push_str("    ");
            markdown.push_str(line);
            markdown.push('\n');
        }
        markdown.push_str("\nTAIL-AFTER-FOOTNOTE-TABLE\n");
        markdown
    }

    #[test]
    fn a_code_block_crossing_inside_a_footnote_definition_stays_balanced() {
        assert!(
            footnote_code_bytes() > MAX_MARKDOWN_CARRIED_EMBED_BYTES,
            "the fixture must cross the carried byte ceiling"
        );
        let plan = plan_markdown(&footnote_nested_code_fixture());
        assert!(plan.is_complete());
        // The balance assertion is the point: a withdrawal that reached below
        // the code block's own start would drop `Start(FootnoteDefinition)` and
        // `Start(CodeBlock)` while their ends stay retained.
        assert_plan_invariants(&plan);

        let omission = single_omission(&plan);
        assert_eq!(omission.reason, MarkdownOmissionReason::CarriedEmbedBytes);
        assert_eq!(omission.scope, MarkdownOmissionScope::ContainerSegment);
        assert_eq!(omission.unretained.cells, 0);
        assert_eq!(
            omission.unretained.source_bytes,
            footnote_code_bytes(),
            "only the code block's own text may be charged as unretained"
        );
        assert_eq!(
            retained_bytes(&plan) + omission.unretained.source_bytes,
            plan.metrics.retained_bytes
        );
        let text = projected_text(&plan);
        assert!(
            text.contains(FOOTNOTE_INTRO),
            "the footnote's own intro paragraph must survive"
        );
        assert!(text.contains("TAIL-AFTER-FOOTNOTE-CODE"));
    }

    #[test]
    fn a_table_crossing_inside_a_footnote_definition_stays_balanced() {
        let plan = plan_markdown(&footnote_nested_table_fixture());
        assert!(plan.is_complete());
        assert_plan_invariants(&plan);

        let omission = single_omission(&plan);
        assert_eq!(omission.reason, MarkdownOmissionReason::CarriedEmbedCells);
        assert_eq!(omission.scope, MarkdownOmissionScope::ContainerSegment);
        // With no checkpoint inside the footnote the withdrawal floors at the
        // table's own start, so the whole table body is reported unretained and
        // the projector's cell check still sees the real total.
        assert_eq!(retained_cells(&plan), 0);
        assert_eq!(omission.unretained.cells, past_ceiling_cells());
        // Every cell of `cell_table` holds exactly one text byte, so the
        // unretained byte count is the cell count. The footnote's label and
        // intro paragraph are far larger than one byte, so either of them
        // leaking into this charge would break the equality.
        assert_eq!(
            omission.unretained.source_bytes,
            past_ceiling_cells(),
            "no footnote-owned byte may be folded into the table's charge"
        );
        assert_eq!(
            retained_bytes(&plan) + omission.unretained.source_bytes,
            plan.metrics.retained_bytes
        );
        let text = projected_text(&plan);
        assert!(
            text.contains(FOOTNOTE_INTRO),
            "the footnote's own intro paragraph must survive"
        );
        assert!(text.contains("TAIL-AFTER-FOOTNOTE-TABLE"));
    }

    #[test]
    fn a_code_block_exactly_at_the_carried_byte_ceiling_is_retained_whole() {
        // The fenced text event carries its trailing newline, so the body is
        // one byte short of the ceiling to land exactly on it.
        let markdown = format!(
            "```text\n{}\n```\n\nTAIL-AT-CEILING\n",
            "c".repeat(MAX_MARKDOWN_CARRIED_EMBED_BYTES - 1)
        );
        let plan = plan_markdown(&markdown);
        assert!(plan.is_complete());
        assert_plan_invariants(&plan);
        assert_eq!(plan.omissions(), 0, "the ceiling itself is admissible");
        assert_eq!(retained_bytes(&plan), plan.metrics.retained_bytes);
        assert!(projected_text(&plan).contains("TAIL-AT-CEILING"));

        let over = format!(
            "```text\n{}\n```\n\nTAIL-OVER-CEILING\n",
            "c".repeat(MAX_MARKDOWN_CARRIED_EMBED_BYTES)
        );
        let plan = plan_markdown(&over);
        assert_eq!(plan.omissions(), 1, "one byte over must cross");
        assert_eq!(
            single_omission(&plan).reason,
            MarkdownOmissionReason::CarriedEmbedBytes
        );
        assert!(projected_text(&plan).contains("TAIL-OVER-CEILING"));
    }

    #[test]
    fn a_table_exactly_at_the_carried_cell_ceiling_is_retained_whole() {
        let columns = 4;
        let rows = MAX_MARKDOWN_CARRIED_TABLE_CELLS / columns - 1;
        assert_eq!(
            cell_table_cells(columns, rows),
            MAX_MARKDOWN_CARRIED_TABLE_CELLS
        );
        let plan = plan_markdown(&cell_table(columns, rows));
        assert!(plan.is_complete());
        assert_plan_invariants(&plan);
        assert_eq!(plan.omissions(), 0, "the ceiling itself is admissible");
        assert_eq!(retained_cells(&plan), MAX_MARKDOWN_CARRIED_TABLE_CELLS);

        let plan = plan_markdown(&cell_table(columns, rows + 1));
        assert_eq!(plan.omissions(), 1, "one row over must cross");
        assert_eq!(
            single_omission(&plan).reason,
            MarkdownOmissionReason::CarriedEmbedCells
        );
    }

    /// One paragraph whose inline run makes the paragraph exactly `events` long.
    ///
    /// `Start(Paragraph)` and `End(Paragraph)` are two of the events, and the
    /// remainder is filled with `Text`/`Code` alternations, which the pinned
    /// parser never coalesces across an inline code span.
    fn paragraph_of_exactly(events: usize) -> String {
        // "**s**" is Start(Strong), Text, End(Strong); each " `a`" adds a Text
        // and a Code; the optional trailing " end" adds one more Text. Each
        // pair contributes two events, so the trailing text is what makes an
        // odd total reachable.
        let with_trailing_text = events.is_multiple_of(2);
        let fixed = if with_trailing_text { 6 } else { 5 };
        assert!(events >= fixed, "paragraph is too short to build");
        let pairs = (events - fixed) / 2;
        let mut paragraph = String::from("**s**");
        for _ in 0..pairs {
            paragraph.push_str(" `a`");
        }
        if with_trailing_text {
            paragraph.push_str(" end");
        }
        paragraph.push('\n');
        paragraph
    }

    #[test]
    fn two_blocks_exactly_filling_one_slice_share_one_batch() {
        // The single-block fixture below cannot pin the packing comparison,
        // because a batch is never cut while it is still empty. Two blocks that
        // together land exactly on the budget can: the boundary must admit the
        // second block rather than open a batch for it.
        let mut markdown = String::from("a\n\n");
        markdown.push_str(&paragraph_of_exactly(
            MARKDOWN_EVENTS_PER_PROJECTION_SLICE - 3,
        ));
        let plan = plan_markdown(&markdown);
        assert_eq!(
            plan.metrics.events, MARKDOWN_EVENTS_PER_PROJECTION_SLICE,
            "the fixture must sit exactly on the event budget"
        );
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), 0);
        assert_eq!(
            plan.batches.len(),
            1,
            "the budget itself must not open a second batch"
        );
        assert_plan_invariants(&plan);

        // One event over must split, so the assertion above is a boundary and
        // not an accident of the fixture size.
        let mut markdown = String::from("a\n\n");
        markdown.push_str(&paragraph_of_exactly(
            MARKDOWN_EVENTS_PER_PROJECTION_SLICE - 2,
        ));
        let plan = plan_markdown(&markdown);
        assert_eq!(
            plan.metrics.events,
            MARKDOWN_EVENTS_PER_PROJECTION_SLICE + 1
        );
        assert_eq!(plan.batches.len(), 2);
        assert_eq!(plan.omissions(), 0);
        assert_plan_invariants(&plan);
    }

    #[test]
    fn a_sub_sliced_block_retains_a_segment_exactly_at_the_event_budget() {
        // A loose list whose first item's paragraph is exactly one slice long,
        // followed by enough siblings that the list itself must sub-slice. The
        // segment on the budget is admissible, so nothing is omitted.
        let mut markdown = String::new();
        writeln!(
            markdown,
            "- {}",
            paragraph_of_exactly(MARKDOWN_EVENTS_PER_PROJECTION_SLICE).trim_end()
        )
        .expect("write loose list fixture");
        for index in 0..3 {
            writeln!(markdown, "\n- item-{index}").expect("write loose list sibling");
        }
        let plan = plan_markdown(&markdown);
        assert!(
            plan.metrics.events > MARKDOWN_EVENTS_PER_PROJECTION_SLICE,
            "the list must be large enough to sub-slice"
        );
        assert!(plan.batches.len() >= 2, "the list must actually sub-slice");
        assert_eq!(
            plan.omissions(),
            0,
            "a segment exactly on the budget must be retained, not omitted"
        );
        assert!(plan.is_complete());
        let text = projected_text(&plan);
        assert!(text.contains("end"), "the on-budget segment must render");
        for index in 0..3 {
            assert!(text.contains(&format!("item-{index}")));
        }
        assert_plan_invariants(&plan);
    }

    #[test]
    fn two_blocks_exactly_filling_the_slice_byte_budget_share_one_batch() {
        // The byte half of the packing comparison, pinned the same way as the
        // event half: only a second block can exercise it, because a batch is
        // never cut while empty.
        let markdown = format!(
            "a\n\n{}\n",
            "x".repeat(MARKDOWN_BYTES_PER_PROJECTION_SLICE - 1)
        );
        let plan = plan_markdown(&markdown);
        assert_eq!(
            plan.metrics.retained_bytes, MARKDOWN_BYTES_PER_PROJECTION_SLICE,
            "the fixture must sit exactly on the byte budget"
        );
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), 0);
        assert_eq!(
            plan.batches.len(),
            1,
            "the byte budget itself must not open a second batch"
        );
        assert_plan_invariants(&plan);

        let markdown = format!("a\n\n{}\n", "x".repeat(MARKDOWN_BYTES_PER_PROJECTION_SLICE));
        let plan = plan_markdown(&markdown);
        assert_eq!(plan.batches.len(), 2, "one byte over must split");
        assert_eq!(plan.omissions(), 0);
        assert_plan_invariants(&plan);
    }

    #[test]
    fn a_structure_exactly_at_the_depth_ceiling_still_completes() {
        // The ceiling is a maximum, not the first rejected depth, and the
        // `every_global_budget_is_still_terminal` case above only proves the
        // rejecting side.
        // The innermost paragraph is itself a frame, so N blockquotes reach
        // depth N + 1.
        let at_ceiling = format!("{}deep\n", "> ".repeat(MAX_MARKDOWN_STRUCTURE_DEPTH - 1));
        let plan = plan_markdown(&at_ceiling);
        assert_eq!(plan.limit, None, "the ceiling itself must still complete");
        assert_eq!(plan.metrics.max_depth, MAX_MARKDOWN_STRUCTURE_DEPTH);
        assert!(projected_text(&plan).contains("deep"));

        let over = format!("{}deep\n", "> ".repeat(MAX_MARKDOWN_STRUCTURE_DEPTH));
        assert_eq!(
            plan_markdown(&over).limit,
            Some(MarkdownPlanLimit::StructuralDepth)
        );
    }

    #[test]
    fn a_segment_on_the_event_budget_but_over_the_byte_budget_reports_bytes() {
        // Both slice budgets can be crossed by one segment. The reported reason
        // must come from the budget that was actually exceeded: a segment
        // sitting exactly *on* the event budget has not exceeded it, so an
        // over-byte segment there is a byte omission, not an event omission.
        let span_bytes = MARKDOWN_BYTES_PER_PROJECTION_SLICE / 100;
        let mut dense = String::from("**s**");
        for _ in 0..125 {
            write!(dense, " `{}`", "y".repeat(span_bytes)).expect("write dense inline fixture");
        }
        dense.push_str(" end");
        let mut markdown = format!("- {dense}\n");
        for index in 0..3 {
            writeln!(markdown, "\n- item-{index}").expect("write loose list sibling");
        }

        let plan = plan_markdown(&markdown);
        let omission = single_omission(&plan);
        assert_eq!(
            omission.reason,
            MarkdownOmissionReason::SliceBytes,
            "the byte budget is the one that was exceeded"
        );
        assert_eq!(omission.scope, MarkdownOmissionScope::ContainerSegment);
        let text = projected_text(&plan);
        for index in 0..3 {
            assert!(
                text.contains(&format!("item-{index}")),
                "sibling {index} lost"
            );
        }
        assert_plan_invariants(&plan);
    }

    #[test]
    fn a_sub_sliced_block_retains_a_segment_exactly_at_the_byte_budget() {
        // The byte twin of the segment-budget boundary above. The first loose
        // item's paragraph retains exactly one slice of bytes, and the siblings
        // push the list past a slice so the block really is sub-sliced.
        let mut markdown = format!("- {}\n", "x".repeat(MARKDOWN_BYTES_PER_PROJECTION_SLICE));
        for index in 0..3 {
            writeln!(markdown, "\n- item-{index}").expect("write loose list sibling");
        }
        let plan = plan_markdown(&markdown);
        assert!(
            plan.metrics.retained_bytes > MARKDOWN_BYTES_PER_PROJECTION_SLICE,
            "the list must be large enough to sub-slice"
        );
        assert!(plan.batches.len() >= 2, "the list must actually sub-slice");
        assert_eq!(
            plan.omissions(),
            0,
            "a segment exactly on the byte budget must be retained, not omitted"
        );
        assert!(plan.is_complete());
        let text = projected_text(&plan);
        for index in 0..3 {
            assert!(
                text.contains(&format!("item-{index}")),
                "sibling {index} lost"
            );
        }
        assert_plan_invariants(&plan);

        // One byte over the same segment must omit, so the assertion above is a
        // boundary rather than an artifact of the fixture size.
        let mut markdown = format!(
            "- {}\n",
            "x".repeat(MARKDOWN_BYTES_PER_PROJECTION_SLICE + 1)
        );
        for index in 0..3 {
            writeln!(markdown, "\n- item-{index}").expect("write loose list sibling");
        }
        let plan = plan_markdown(&markdown);
        let omission = single_omission(&plan);
        assert_eq!(omission.reason, MarkdownOmissionReason::SliceBytes);
        assert_eq!(omission.scope, MarkdownOmissionScope::ContainerSegment);
        assert_plan_invariants(&plan);
    }

    #[test]
    fn documented_planning_ceilings_keep_their_published_values() {
        // README.md and AGENTS.md quote these numbers to users, and
        // `MAX_MARKDOWN_CARRIED_EMBED_BYTES` / `MAX_MARKDOWN_CARRIED_TABLE_CELLS`
        // must equal the projection-side widget budgets they mirror, which
        // `services` cannot import. Pin them here so a silent edit on either
        // side fails.
        assert_eq!(MAX_MARKDOWN_SOURCE_BYTES, 4 * 1024 * 1024);
        assert_eq!(MAX_MARKDOWN_EVENTS, 50_000);
        assert_eq!(MAX_MARKDOWN_STRUCTURE_DEPTH, 128);
        assert_eq!(MAX_MARKDOWN_EMBED_DESCRIPTORS, 256);
        assert_eq!(MAX_MARKDOWN_RETAINED_BYTES, 8 * 1024 * 1024);
        assert_eq!(MARKDOWN_EVENTS_PER_PROJECTION_SLICE, 256);
        assert_eq!(MARKDOWN_BYTES_PER_PROJECTION_SLICE, 256 * 1024);
        assert_eq!(MAX_MARKDOWN_CARRIED_EMBED_BYTES, 64 * 1024);
        assert_eq!(MAX_MARKDOWN_CARRIED_TABLE_CELLS, 1_000);
        assert_eq!(MAX_MARKDOWN_PLACEHOLDER_WIDGETS, 64);
        // The pinned pair above already implies the ordering the code ceiling
        // depends on: a code block crosses 64 KiB before the 256 KiB slice
        // budget, so the crossing reason is always `CarriedEmbedBytes`.
    }

    #[test]
    fn plan_metrics_report_the_source_byte_count() {
        let markdown = "one paragraph\n";
        let plan = plan_markdown(markdown);
        assert_eq!(plan.metrics.source_bytes, markdown.len());

        let oversized = MAX_MARKDOWN_SOURCE_BYTES + 1;
        let plan = source_limited_markdown_plan(oversized);
        assert_eq!(plan.metrics.source_bytes, oversized);
        assert_eq!(plan.limit, Some(MarkdownPlanLimit::SourceBytes));
    }

    #[test]
    fn a_block_exactly_at_the_slice_budgets_packs_as_one_batch() {
        let mut paragraph = String::from("**s**");
        for _ in 0..125 {
            paragraph.push_str(" `a`");
        }
        paragraph.push_str(" end\n");
        let plan = plan_markdown(&paragraph);
        assert_eq!(
            plan.metrics.events, MARKDOWN_EVENTS_PER_PROJECTION_SLICE,
            "the fixture must sit exactly on the event budget"
        );
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), 0);
        assert_eq!(plan.batches.len(), 1, "the budget itself must not split");
        assert_plan_invariants(&plan);

        let exact_bytes = "x".repeat(MARKDOWN_BYTES_PER_PROJECTION_SLICE);
        let plan = plan_markdown(&exact_bytes);
        assert_eq!(
            plan.metrics.retained_bytes, MARKDOWN_BYTES_PER_PROJECTION_SLICE,
            "the fixture must sit exactly on the byte budget"
        );
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), 0);
        assert_eq!(plan.batches.len(), 1);
        assert_plan_invariants(&plan);
    }

    #[test]
    fn a_global_stop_after_a_crossed_block_leaves_no_uncounted_omission() {
        // The crossed table sits inside a list item, and the global event
        // budget fires before that list closes. The crossing's marker therefore
        // never reaches a batch, so it must not be counted either.
        let mut markdown = String::from("- outer item\n\n");
        for line in cell_table(PAST_CEILING_COLUMNS, PAST_CEILING_ROWS).lines() {
            markdown.push_str("  ");
            markdown.push_str(line);
            markdown.push('\n');
        }
        markdown.push('\n');
        while markdown.len() < 256 * 1024 {
            markdown.push_str("- filler item\n\n");
        }
        let plan = plan_markdown(&markdown);
        assert_eq!(plan.limit, Some(MarkdownPlanLimit::Events));
        assert_eq!(
            plan.batches.len(),
            0,
            "the enclosing list never closed, so no batch was emitted"
        );
        assert_eq!(
            plan.omissions(),
            0,
            "an omission no batch carries must not be counted"
        );
        assert_plan_invariants(&plan);
    }

    #[test]
    fn omitted_units_still_charge_the_global_budgets() {
        // An omitted unit's events and retained bytes are charged exactly as
        // today, so a hostile document cannot bypass a global ceiling by making
        // every block indivisible.
        let dense = "z".repeat(MARKDOWN_BYTES_PER_PROJECTION_SLICE + 1);
        let plan = plan_markdown(&dense);
        assert_eq!(plan.omissions(), 1);
        assert_eq!(
            plan.projected_events(),
            0,
            "the omitted block retains nothing"
        );
        assert_eq!(plan.metrics.events, 3);
        assert!(
            plan.metrics.retained_bytes > MARKDOWN_BYTES_PER_PROJECTION_SLICE,
            "the omitted block's bytes are still charged"
        );

        // A document of indivisible blocks still reaches a global terminal
        // instead of planning forever.
        let block = (0..200).map(|_| "**x** ").collect::<String>();
        let mut markdown = String::new();
        while markdown.len() < 512 * 1024 {
            markdown.push_str(&block);
            markdown.push_str("\n\n");
        }
        let plan = plan_markdown(&markdown);
        assert_eq!(plan.limit, Some(MarkdownPlanLimit::Events));
        assert_eq!(plan.metrics.events, MAX_MARKDOWN_EVENTS);
        assert!(plan.omissions() > 0);
    }

    #[test]
    fn retention_charge_arithmetic_is_pinned_for_a_retention_heavy_document() {
        // The retained-byte *terminal* is unreachable under the 4 MiB source
        // cap, so this pins the charge arithmetic instead of the terminal: even
        // the largest admissible source stays under the ceiling (hence
        // `limit == None`), and bytes are charged before any per-slice decision
        // so an omitted block cannot duck the ceiling on the way past.
        let plan = plan_markdown(&"w".repeat(MAX_MARKDOWN_SOURCE_BYTES));
        assert!(plan.metrics.retained_bytes >= MAX_MARKDOWN_SOURCE_BYTES);
        assert!(plan.metrics.retained_bytes <= MAX_MARKDOWN_RETAINED_BYTES);
        assert_eq!(plan.limit, None);
        assert_eq!(plan.omissions(), 1);
    }

    #[test]
    fn omission_markers_do_not_consume_embed_descriptors() {
        let plan = plan_markdown(&list_with_one_overflowing_item_fixture());
        assert_eq!(plan.omissions(), 1);
        assert_eq!(plan.metrics.embed_descriptors, 0);

        let plan = plan_markdown(&table_past_the_cell_ceiling_fixture());
        assert_eq!(plan.omissions(), 1);
        assert_eq!(
            plan.metrics.embed_descriptors, 1,
            "only the table itself is a descriptor"
        );
    }

    #[test]
    fn omission_marker_copy_names_the_crossed_budget() {
        fn copy(reason: MarkdownOmissionReason, scope: MarkdownOmissionScope) -> String {
            MarkdownBlockOmission {
                reason,
                scope,
                unretained: UnretainedEmbedCounts::default(),
            }
            .marker_text()
        }

        assert_eq!(
            copy(
                MarkdownOmissionReason::SliceEvents,
                MarkdownOmissionScope::TopLevelBlock
            ),
            "Markdown preview omitted one block that exceeds 256 render events"
        );
        assert_eq!(
            copy(
                MarkdownOmissionReason::SliceBytes,
                MarkdownOmissionScope::ContainerSegment
            ),
            "Markdown preview omitted part of one block that exceeds 256 KiB"
        );
        assert_eq!(
            copy(
                MarkdownOmissionReason::CarriedEmbedBytes,
                MarkdownOmissionScope::ContainerSegment
            ),
            "Markdown preview omitted part of one block after 64 KiB of carried content"
        );
        assert_eq!(
            copy(
                MarkdownOmissionReason::CarriedEmbedCells,
                MarkdownOmissionScope::ContainerSegment
            ),
            "Markdown preview omitted part of one block after 1000 carried table cells"
        );
    }

    /// Padding that lands a batch boundary exactly on `Start(CodeBlock)`.
    ///
    /// The list is loose (the code-block item's blank line makes it so), so each
    /// padding item is five events and the leading inline span shifts the whole
    /// stream by one. Together they align the second boundary onto the code
    /// block's own start event.
    const CODE_BLOCK_BOUNDARY_PAD_ITEMS: usize = 100;
    const CODE_BLOCK_BOUNDARY_LEAD_SPANS: usize = 1;

    /// A list whose projection boundary falls between a *tiny* code block's
    /// start and its text, because the enclosing list overflows the slice.
    fn code_block_start_boundary_fixture() -> String {
        let mut markdown = String::from("# Boundary\n\n");
        markdown.push_str(&format!(
            "{}\n\n",
            "`a` ".repeat(CODE_BLOCK_BOUNDARY_LEAD_SPANS)
        ));
        for index in 0..CODE_BLOCK_BOUNDARY_PAD_ITEMS {
            writeln!(markdown, "- pad-{index}").expect("write padding item");
        }
        markdown.push_str("- item with code\n\n  ```sh\n  echo tiny\n  ```\n\n");
        markdown.push_str("- after-code\n\nTAIL-AFTER-BOUNDARY\n");
        markdown
    }

    #[test]
    fn a_turn_boundary_can_fall_between_a_code_block_start_and_its_text() {
        // A carried code block is not only the large-embed case: a checkpoint is
        // admissible immediately after `Start(CodeBlock)`, so a projector that
        // rebuilt its in-flight block per turn would lose a *tiny* block whose
        // enclosing list overflowed.
        let plan = plan_markdown(&code_block_start_boundary_fixture());
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), 0);
        assert_plan_invariants(&plan);

        let boundary = plan
            .batches
            .iter()
            .position(|batch| {
                matches!(batch.events().last(), Some(Event::Start(Tag::CodeBlock(_))))
            })
            .expect("a batch must end exactly at the code block's start");
        assert!(
            matches!(
                plan.batches[boundary].open_carry().containers(),
                [
                    MarkdownOpenContainer::List { .. },
                    MarkdownOpenContainer::Item,
                    MarkdownOpenContainer::CodeBlock {
                        kind: MarkdownCodeBlockKind::Fenced { .. }
                    },
                ]
            ),
            "the boundary must carry the open code block itself: {:?}",
            plan.batches[boundary].open_carry().containers()
        );

        // The turn that opened the block carried no code text at all, and the
        // next turn resumes into the same block.
        let next = plan
            .batches
            .get(boundary + 1)
            .expect("the code block continues in a later batch");
        assert_eq!(next.expected_carry(), plan.batches[boundary].open_carry());
        assert!(
            matches!(next.events().first(), Some(Event::Text(text)) if text.contains("echo tiny")),
            "the next turn must resume with the code text: {:?}",
            next.events().first()
        );
        let text = projected_text(&plan);
        assert!(text.contains("echo tiny"));
        assert!(text.contains("TAIL-AFTER-BOUNDARY"));
    }

    #[test]
    fn only_slice_reasons_count_as_user_visible_omissions() {
        // A carried-embed crossing is a charge carrier: the projector's own
        // in-place fallback already replaces that block and names its true
        // size, so the plan must not report a user-visible omission for it.
        for markdown in [
            table_past_the_cell_ceiling_fixture(),
            format!(
                "```text\n{}\n```\n\ntail\n",
                "c".repeat(MAX_MARKDOWN_CARRIED_EMBED_BYTES + 4096)
            ),
        ] {
            let plan = plan_markdown(&markdown);
            assert_eq!(plan.omissions(), 1, "the crossing is still recorded");
            assert_eq!(
                plan.user_visible_omissions(),
                0,
                "a crossing must not be reported to the reader"
            );
        }

        // Both slice reasons are user visible.
        let plan = plan_markdown(&byte_budget_fixture());
        assert_eq!(plan.user_visible_omissions(), 1);
        let dense = (0..300).map(|_| "**x** ").collect::<String>();
        let plan = plan_markdown(&dense);
        assert_eq!(plan.user_visible_omissions(), 1);

        // A plan with no omissions reports none.
        let plan = plan_markdown(&oversized_list_fixture());
        assert_eq!(plan.user_visible_omissions(), 0);
    }

    #[test]
    fn placeholder_widget_cap_bounds_top_level_omissions() {
        let dense = (0..100).map(|_| "**x** ").collect::<String>();
        let mut markdown = String::new();
        for _ in 0..(MAX_MARKDOWN_PLACEHOLDER_WIDGETS + 4) {
            markdown.push_str(&dense);
            markdown.push_str("\n\n");
        }
        let plan = plan_markdown(&markdown);
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), MAX_MARKDOWN_PLACEHOLDER_WIDGETS + 4);
        assert_eq!(plan.top_level_omissions(), plan.omissions());
        assert!(
            plan.top_level_omissions() > MAX_MARKDOWN_PLACEHOLDER_WIDGETS,
            "projection must be able to see that the widget cap is exceeded"
        );
    }

    #[test]
    fn batch_carry_signatures_chain_across_a_sub_sliced_block() {
        let plan = plan_markdown(&helm_readme_fixture());
        assert_plan_invariants(&plan);
        let mid = plan
            .batches
            .iter()
            .find(|batch| !batch.open_carry().is_empty())
            .expect("a sub-sliced table leaves its table open");
        assert!(matches!(
            mid.open_carry().containers(),
            [MarkdownOpenContainer::Table { .. }]
        ));
    }

    #[test]
    fn carry_signature_tracks_ordered_list_numbering() {
        let mut markdown = String::from("# Ordered\n\n");
        for index in 1..=120 {
            writeln!(markdown, "{index}. ordered-item-{index}").expect("write ordered item");
        }
        let plan = plan_markdown(&markdown);
        assert_plan_invariants(&plan);
        let open = plan
            .batches
            .iter()
            .find_map(|batch| match batch.open_carry().containers() {
                [
                    MarkdownOpenContainer::List {
                        ordered,
                        next_number,
                    },
                ] => Some((*ordered, *next_number)),
                _ => None,
            })
            .expect("an oversized ordered list is cut at an item boundary");
        assert!(open.0);
        assert!(open.1 > 1, "numbering continues across the cut");
    }

    #[test]
    fn earlier_batches_survive_a_carried_embed_crossing() {
        let plan = plan_markdown(&table_past_the_cell_ceiling_fixture());
        let unchanged = plan_markdown(&table_past_the_cell_ceiling_fixture());
        assert_eq!(plan, unchanged, "planning stays deterministic");
        assert_plan_invariants(&plan);
        let crossing_batch = plan
            .batches
            .iter()
            .position(|batch| !batch.omissions().is_empty())
            .expect("the crossing emits a marker");
        assert!(
            plan.batches[..crossing_batch]
                .iter()
                .all(|batch| batch.omissions().is_empty()),
            "batches emitted before the crossing are never rewritten"
        );
    }

    #[test]
    fn cancellation_before_planning_retains_no_partial_plan() {
        let cancel = AtomicBool::new(true);
        let markdown = helm_readme_fixture();
        assert!(plan_markdown_cancellable(&markdown, &cancel).is_none());
    }

    #[test]
    fn cancellation_at_a_checkpoint_inside_a_sub_sliced_block_retains_no_plan() {
        let markdown = helm_readme_fixture();
        let uncancelled = plan_markdown(&markdown);
        assert!(uncancelled.batches.len() > 1, "the fixture is sub-sliced");
        // Land well inside the values table, past its first emitted batch.
        let inside_block = 320;
        assert!(inside_block < uncancelled.metrics.events);

        let cancel = AtomicBool::new(false);
        let _armed = CancelAfterEvents::arm(inside_block);
        let plan = plan_markdown_cancellable(&markdown, &cancel);
        assert!(
            plan.is_none(),
            "cancellation mid-block must retain no partial plan"
        );
        assert!(
            cancel.load(Ordering::Acquire),
            "the seam must have flipped the caller's token mid-parse"
        );
    }

    #[test]
    fn ordinary_blocks_are_packed_without_splitting() {
        let mut markdown = String::new();
        for index in 0..400 {
            writeln!(markdown, "paragraph {index}\n").expect("write paragraph fixture");
        }
        let plan = plan_markdown(&markdown);
        assert!(plan.is_complete());
        assert!(plan.batches.len() > 1);
        assert_plan_invariants(&plan);
        assert_eq!(plan.projected_events(), plan.metrics.events);
    }

    #[test]
    fn one_dense_block_is_omitted_and_planning_continues() {
        let dense = (0..300).map(|_| "**x** ").collect::<String>();
        let markdown = format!("{dense}\n\nTAIL-AFTER-DENSE\n");
        let plan = plan_markdown(&markdown);
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), 1);
        assert_plan_invariants(&plan);
        let omission = single_omission(&plan);
        assert_eq!(omission.reason, MarkdownOmissionReason::SliceEvents);
        assert_eq!(omission.scope, MarkdownOmissionScope::TopLevelBlock);
        assert!(projected_text(&plan).contains("TAIL-AFTER-DENSE"));
    }

    #[test]
    fn image_flood_stops_at_descriptor_budget() {
        // Exactly the budget is admissible: the ceiling is a maximum, not the
        // first rejected value.
        let mut markdown = String::new();
        for index in 0..MAX_MARKDOWN_EMBED_DESCRIPTORS {
            writeln!(markdown, "![image](image-{index}.png)\n").expect("write image-flood fixture");
        }
        let plan = plan_markdown(&markdown);
        assert_eq!(plan.limit, None, "the budget itself must still complete");
        assert_eq!(
            plan.metrics.embed_descriptors,
            MAX_MARKDOWN_EMBED_DESCRIPTORS
        );

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
    fn one_large_text_block_is_omitted_and_planning_continues() {
        let markdown = format!(
            "{}\n\nTAIL-AFTER-LARGE\n",
            "x".repeat(MARKDOWN_BYTES_PER_PROJECTION_SLICE + 1)
        );
        let plan = plan_markdown(&markdown);
        assert!(plan.is_complete());
        assert_eq!(plan.omissions(), 1);
        assert_plan_invariants(&plan);
        assert_eq!(
            single_omission(&plan).reason,
            MarkdownOmissionReason::SliceBytes
        );
        assert!(projected_text(&plan).contains("TAIL-AFTER-LARGE"));
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
