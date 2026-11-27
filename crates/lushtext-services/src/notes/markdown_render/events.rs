// SPDX-License-Identifier: GPL-3.0-or-later

//! LushText-owned domain event types and plan values for Markdown preview.
//!
//! Encapsulates `pulldown-cmark`'s event/tag representations inside
//! `lushtext-services` so `lushtext-ui` never imports `pulldown_cmark` directly.

use super::{
    MARKDOWN_BYTES_PER_PROJECTION_SLICE, MARKDOWN_EVENTS_PER_PROJECTION_SLICE,
    MAX_MARKDOWN_CARRIED_EMBED_BYTES, MAX_MARKDOWN_CARRIED_TABLE_CELLS, kibibytes,
};

/// Heading level (H1–H6) for a Markdown heading block.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MarkdownHeadingLevel {
    H1 = 1,
    H2 = 2,
    H3 = 3,
    H4 = 4,
    H5 = 5,
    H6 = 6,
}

/// Callout/alert variant for a GitHub-style blockquote.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkdownBlockQuoteKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

/// Column alignment in a Markdown table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkdownAlignment {
    None,
    Left,
    Center,
    Right,
}

/// Link syntax variant carried on link/image tags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkdownLinkType {
    Inline,
    Reference,
    ReferenceUnknown,
    Collapsed,
    CollapsedUnknown,
    Shortcut,
    ShortcutUnknown,
    Autolink,
    Email,
    WikiLink { has_pothole: bool },
}

/// Front-matter metadata block delimiter flavor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkdownMetadataBlockKind {
    YamlStyle,
    PlusesStyle,
}

/// Fenced or indented identity of a code block carried across projection turns.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MarkdownCodeBlockKind {
    Indented,
    Fenced { language: String },
}

impl MarkdownCodeBlockKind {
    /// Convert from the underlying parser's code-block kind.
    #[must_use]
    pub(super) fn from_parser(kind: &pulldown_cmark::CodeBlockKind<'_>) -> Self {
        match kind {
            pulldown_cmark::CodeBlockKind::Indented => Self::Indented,
            pulldown_cmark::CodeBlockKind::Fenced(language) => Self::Fenced {
                language: language.to_string(),
            },
        }
    }
}

/// Opening container or inline formatting tag in a planned Markdown stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MarkdownTag {
    Paragraph,
    Heading {
        level: MarkdownHeadingLevel,
        id: Option<String>,
        classes: Vec<String>,
        attrs: Vec<(String, Option<String>)>,
    },
    BlockQuote(Option<MarkdownBlockQuoteKind>),
    CodeBlock(MarkdownCodeBlockKind),
    HtmlBlock,
    List(Option<u64>),
    Item,
    FootnoteDefinition(String),
    DefinitionList,
    DefinitionListTitle,
    DefinitionListDefinition,
    Table(Vec<MarkdownAlignment>),
    TableHead,
    TableRow,
    TableCell,
    Emphasis,
    Strong,
    Strikethrough,
    Superscript,
    Subscript,
    Link {
        link_type: MarkdownLinkType,
        dest_url: String,
        title: String,
        id: String,
    },
    Image {
        link_type: MarkdownLinkType,
        dest_url: String,
        title: String,
        id: String,
    },
    MetadataBlock(MarkdownMetadataBlockKind),
}

impl MarkdownTag {
    /// Return the matching closing tag variant for a start tag.
    #[must_use]
    pub fn to_end(&self) -> MarkdownTagEnd {
        match self {
            Self::Paragraph => MarkdownTagEnd::Paragraph,
            Self::Heading { level, .. } => MarkdownTagEnd::Heading(*level),
            Self::BlockQuote(kind) => MarkdownTagEnd::BlockQuote(*kind),
            Self::CodeBlock(_) => MarkdownTagEnd::CodeBlock,
            Self::HtmlBlock => MarkdownTagEnd::HtmlBlock,
            Self::List(start) => MarkdownTagEnd::List(start.is_some()),
            Self::Item => MarkdownTagEnd::Item,
            Self::FootnoteDefinition(_) => MarkdownTagEnd::FootnoteDefinition,
            Self::DefinitionList => MarkdownTagEnd::DefinitionList,
            Self::DefinitionListTitle => MarkdownTagEnd::DefinitionListTitle,
            Self::DefinitionListDefinition => MarkdownTagEnd::DefinitionListDefinition,
            Self::Table(_) => MarkdownTagEnd::Table,
            Self::TableHead => MarkdownTagEnd::TableHead,
            Self::TableRow => MarkdownTagEnd::TableRow,
            Self::TableCell => MarkdownTagEnd::TableCell,
            Self::Emphasis => MarkdownTagEnd::Emphasis,
            Self::Strong => MarkdownTagEnd::Strong,
            Self::Strikethrough => MarkdownTagEnd::Strikethrough,
            Self::Superscript => MarkdownTagEnd::Superscript,
            Self::Subscript => MarkdownTagEnd::Subscript,
            Self::Link { .. } => MarkdownTagEnd::Link,
            Self::Image { .. } => MarkdownTagEnd::Image,
            Self::MetadataBlock(kind) => MarkdownTagEnd::MetadataBlock(*kind),
        }
    }
}

/// Closing tag corresponding to [`MarkdownTag`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkdownTagEnd {
    Paragraph,
    Heading(MarkdownHeadingLevel),
    BlockQuote(Option<MarkdownBlockQuoteKind>),
    CodeBlock,
    HtmlBlock,
    List(bool),
    Item,
    FootnoteDefinition,
    DefinitionList,
    DefinitionListTitle,
    DefinitionListDefinition,
    Table,
    TableHead,
    TableRow,
    TableCell,
    Emphasis,
    Strong,
    Strikethrough,
    Superscript,
    Subscript,
    Link,
    Image,
    MetadataBlock(MarkdownMetadataBlockKind),
}

/// One parsed, GTK-free Markdown event carried inside a projection batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MarkdownEvent {
    Start(MarkdownTag),
    End(MarkdownTagEnd),
    Text(String),
    Code(String),
    InlineMath(String),
    DisplayMath(String),
    Html(String),
    InlineHtml(String),
    FootnoteReference(String),
    SoftBreak,
    HardBreak,
    Rule,
    TaskListMarker(bool),
}

pub(super) fn convert_heading_level(level: pulldown_cmark::HeadingLevel) -> MarkdownHeadingLevel {
    match level {
        pulldown_cmark::HeadingLevel::H1 => MarkdownHeadingLevel::H1,
        pulldown_cmark::HeadingLevel::H2 => MarkdownHeadingLevel::H2,
        pulldown_cmark::HeadingLevel::H3 => MarkdownHeadingLevel::H3,
        pulldown_cmark::HeadingLevel::H4 => MarkdownHeadingLevel::H4,
        pulldown_cmark::HeadingLevel::H5 => MarkdownHeadingLevel::H5,
        pulldown_cmark::HeadingLevel::H6 => MarkdownHeadingLevel::H6,
    }
}

pub(super) fn convert_blockquote_kind(
    kind: pulldown_cmark::BlockQuoteKind,
) -> MarkdownBlockQuoteKind {
    match kind {
        pulldown_cmark::BlockQuoteKind::Note => MarkdownBlockQuoteKind::Note,
        pulldown_cmark::BlockQuoteKind::Tip => MarkdownBlockQuoteKind::Tip,
        pulldown_cmark::BlockQuoteKind::Important => MarkdownBlockQuoteKind::Important,
        pulldown_cmark::BlockQuoteKind::Warning => MarkdownBlockQuoteKind::Warning,
        pulldown_cmark::BlockQuoteKind::Caution => MarkdownBlockQuoteKind::Caution,
    }
}

pub(super) fn convert_alignment(alignment: pulldown_cmark::Alignment) -> MarkdownAlignment {
    match alignment {
        pulldown_cmark::Alignment::None => MarkdownAlignment::None,
        pulldown_cmark::Alignment::Left => MarkdownAlignment::Left,
        pulldown_cmark::Alignment::Center => MarkdownAlignment::Center,
        pulldown_cmark::Alignment::Right => MarkdownAlignment::Right,
    }
}

fn convert_link_type(link_type: pulldown_cmark::LinkType) -> MarkdownLinkType {
    match link_type {
        pulldown_cmark::LinkType::Inline => MarkdownLinkType::Inline,
        pulldown_cmark::LinkType::Reference => MarkdownLinkType::Reference,
        pulldown_cmark::LinkType::ReferenceUnknown => MarkdownLinkType::ReferenceUnknown,
        pulldown_cmark::LinkType::Collapsed => MarkdownLinkType::Collapsed,
        pulldown_cmark::LinkType::CollapsedUnknown => MarkdownLinkType::CollapsedUnknown,
        pulldown_cmark::LinkType::Shortcut => MarkdownLinkType::Shortcut,
        pulldown_cmark::LinkType::ShortcutUnknown => MarkdownLinkType::ShortcutUnknown,
        pulldown_cmark::LinkType::Autolink => MarkdownLinkType::Autolink,
        pulldown_cmark::LinkType::Email => MarkdownLinkType::Email,
        pulldown_cmark::LinkType::WikiLink { has_pothole } => {
            MarkdownLinkType::WikiLink { has_pothole }
        }
    }
}

fn convert_metadata_block_kind(
    kind: pulldown_cmark::MetadataBlockKind,
) -> MarkdownMetadataBlockKind {
    match kind {
        pulldown_cmark::MetadataBlockKind::YamlStyle => MarkdownMetadataBlockKind::YamlStyle,
        pulldown_cmark::MetadataBlockKind::PlusesStyle => MarkdownMetadataBlockKind::PlusesStyle,
    }
}

fn convert_tag(tag: pulldown_cmark::Tag<'_>) -> MarkdownTag {
    match tag {
        pulldown_cmark::Tag::Paragraph => MarkdownTag::Paragraph,
        pulldown_cmark::Tag::Heading {
            level,
            id,
            classes,
            attrs,
        } => MarkdownTag::Heading {
            level: convert_heading_level(level),
            id: id.map(pulldown_cmark::CowStr::into_string),
            classes: classes
                .into_iter()
                .map(pulldown_cmark::CowStr::into_string)
                .collect(),
            attrs: attrs
                .into_iter()
                .map(|(k, v)| (k.into_string(), v.map(pulldown_cmark::CowStr::into_string)))
                .collect(),
        },
        pulldown_cmark::Tag::BlockQuote(kind) => {
            MarkdownTag::BlockQuote(kind.map(convert_blockquote_kind))
        }
        pulldown_cmark::Tag::CodeBlock(kind) => {
            MarkdownTag::CodeBlock(MarkdownCodeBlockKind::from_parser(&kind))
        }
        pulldown_cmark::Tag::HtmlBlock => MarkdownTag::HtmlBlock,
        pulldown_cmark::Tag::List(start) => MarkdownTag::List(start),
        pulldown_cmark::Tag::Item => MarkdownTag::Item,
        pulldown_cmark::Tag::FootnoteDefinition(label) => {
            MarkdownTag::FootnoteDefinition(label.into_string())
        }
        pulldown_cmark::Tag::DefinitionList => MarkdownTag::DefinitionList,
        pulldown_cmark::Tag::DefinitionListTitle => MarkdownTag::DefinitionListTitle,
        pulldown_cmark::Tag::DefinitionListDefinition => MarkdownTag::DefinitionListDefinition,
        pulldown_cmark::Tag::Table(alignments) => {
            MarkdownTag::Table(alignments.into_iter().map(convert_alignment).collect())
        }
        pulldown_cmark::Tag::TableHead => MarkdownTag::TableHead,
        pulldown_cmark::Tag::TableRow => MarkdownTag::TableRow,
        pulldown_cmark::Tag::TableCell => MarkdownTag::TableCell,
        pulldown_cmark::Tag::Emphasis => MarkdownTag::Emphasis,
        pulldown_cmark::Tag::Strong => MarkdownTag::Strong,
        pulldown_cmark::Tag::Strikethrough => MarkdownTag::Strikethrough,
        pulldown_cmark::Tag::Superscript => MarkdownTag::Superscript,
        pulldown_cmark::Tag::Subscript => MarkdownTag::Subscript,
        pulldown_cmark::Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        } => MarkdownTag::Link {
            link_type: convert_link_type(link_type),
            dest_url: dest_url.into_string(),
            title: title.into_string(),
            id: id.into_string(),
        },
        pulldown_cmark::Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        } => MarkdownTag::Image {
            link_type: convert_link_type(link_type),
            dest_url: dest_url.into_string(),
            title: title.into_string(),
            id: id.into_string(),
        },
        pulldown_cmark::Tag::MetadataBlock(kind) => {
            MarkdownTag::MetadataBlock(convert_metadata_block_kind(kind))
        }
    }
}

fn convert_tag_end(tag_end: pulldown_cmark::TagEnd) -> MarkdownTagEnd {
    match tag_end {
        pulldown_cmark::TagEnd::Paragraph => MarkdownTagEnd::Paragraph,
        pulldown_cmark::TagEnd::Heading(level) => {
            MarkdownTagEnd::Heading(convert_heading_level(level))
        }
        pulldown_cmark::TagEnd::BlockQuote(kind) => {
            MarkdownTagEnd::BlockQuote(kind.map(convert_blockquote_kind))
        }
        pulldown_cmark::TagEnd::CodeBlock => MarkdownTagEnd::CodeBlock,
        pulldown_cmark::TagEnd::HtmlBlock => MarkdownTagEnd::HtmlBlock,
        pulldown_cmark::TagEnd::List(ordered) => MarkdownTagEnd::List(ordered),
        pulldown_cmark::TagEnd::Item => MarkdownTagEnd::Item,
        pulldown_cmark::TagEnd::FootnoteDefinition => MarkdownTagEnd::FootnoteDefinition,
        pulldown_cmark::TagEnd::DefinitionList => MarkdownTagEnd::DefinitionList,
        pulldown_cmark::TagEnd::DefinitionListTitle => MarkdownTagEnd::DefinitionListTitle,
        pulldown_cmark::TagEnd::DefinitionListDefinition => {
            MarkdownTagEnd::DefinitionListDefinition
        }
        pulldown_cmark::TagEnd::Table => MarkdownTagEnd::Table,
        pulldown_cmark::TagEnd::TableHead => MarkdownTagEnd::TableHead,
        pulldown_cmark::TagEnd::TableRow => MarkdownTagEnd::TableRow,
        pulldown_cmark::TagEnd::TableCell => MarkdownTagEnd::TableCell,
        pulldown_cmark::TagEnd::Emphasis => MarkdownTagEnd::Emphasis,
        pulldown_cmark::TagEnd::Strong => MarkdownTagEnd::Strong,
        pulldown_cmark::TagEnd::Strikethrough => MarkdownTagEnd::Strikethrough,
        pulldown_cmark::TagEnd::Superscript => MarkdownTagEnd::Superscript,
        pulldown_cmark::TagEnd::Subscript => MarkdownTagEnd::Subscript,
        pulldown_cmark::TagEnd::Link => MarkdownTagEnd::Link,
        pulldown_cmark::TagEnd::Image => MarkdownTagEnd::Image,
        pulldown_cmark::TagEnd::MetadataBlock(kind) => {
            MarkdownTagEnd::MetadataBlock(convert_metadata_block_kind(kind))
        }
    }
}

pub(super) fn convert_event(event: pulldown_cmark::Event<'_>) -> MarkdownEvent {
    match event {
        pulldown_cmark::Event::Start(tag) => MarkdownEvent::Start(convert_tag(tag)),
        pulldown_cmark::Event::End(tag_end) => MarkdownEvent::End(convert_tag_end(tag_end)),
        pulldown_cmark::Event::Text(text) => MarkdownEvent::Text(text.into_string()),
        pulldown_cmark::Event::Code(code) => MarkdownEvent::Code(code.into_string()),
        pulldown_cmark::Event::InlineMath(math) => MarkdownEvent::InlineMath(math.into_string()),
        pulldown_cmark::Event::DisplayMath(math) => MarkdownEvent::DisplayMath(math.into_string()),
        pulldown_cmark::Event::Html(html) => MarkdownEvent::Html(html.into_string()),
        pulldown_cmark::Event::InlineHtml(html) => MarkdownEvent::InlineHtml(html.into_string()),
        pulldown_cmark::Event::FootnoteReference(label) => {
            MarkdownEvent::FootnoteReference(label.into_string())
        }
        pulldown_cmark::Event::SoftBreak => MarkdownEvent::SoftBreak,
        pulldown_cmark::Event::HardBreak => MarkdownEvent::HardBreak,
        pulldown_cmark::Event::Rule => MarkdownEvent::Rule,
        pulldown_cmark::Event::TaskListMarker(checked) => MarkdownEvent::TaskListMarker(checked),
    }
}

/// Parse `markdown` with the preview parser options into domain events (for tests).
#[cfg(any(test, feature = "test-utils"))]
#[must_use]
pub fn parse_markdown_events(markdown: &str) -> Vec<MarkdownEvent> {
    pulldown_cmark::Parser::new_ext(markdown, super::markdown_render_options())
        .map(convert_event)
        .collect()
}

/// Count parsed events emitted under preview options (for property tests and fuzzing).
#[cfg(any(test, feature = "property-tests", feature = "fuzzing"))]
#[must_use]
pub fn count_markdown_events(markdown: &str) -> usize {
    pulldown_cmark::Parser::new_ext(markdown, super::markdown_render_options()).count()
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

/// One omission marker at its own position inside a projection batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MarkdownOmissionMarker {
    /// Number of the batch's events that precede this marker.
    pub at_event: usize,
    pub omission: MarkdownBlockOmission,
}

/// One block container left open at a projection batch boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MarkdownOpenContainer {
    List {
        ordered: bool,
        next_number: u64,
    },
    Item,
    Table {
        alignments: Vec<MarkdownAlignment>,
    },
    BlockQuote {
        kind: Option<MarkdownBlockQuoteKind>,
    },
    CodeBlock {
        kind: MarkdownCodeBlockKind,
    },
    DefinitionList,
    DefinitionListDefinition,
}

/// Structural continuation one projection batch expects or leaves open.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MarkdownCarrySignature {
    pub(super) containers: Vec<MarkdownOpenContainer>,
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkdownEventBatch {
    pub(super) events: Vec<MarkdownEvent>,
    pub(super) retained_bytes: usize,
    pub(super) omissions: Vec<MarkdownOmissionMarker>,
    pub(super) expected_carry: MarkdownCarrySignature,
    pub(super) open_carry: MarkdownCarrySignature,
}

impl MarkdownEventBatch {
    #[must_use]
    pub fn events(&self) -> &[MarkdownEvent] {
        &self.events
    }

    #[must_use]
    pub fn into_events(self) -> Vec<MarkdownEvent> {
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
#[derive(Clone, Debug, Eq, PartialEq)]
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
