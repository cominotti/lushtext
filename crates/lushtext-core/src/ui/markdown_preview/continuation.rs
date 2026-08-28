// SPDX-License-Identifier: GPL-3.0-or-later

//! Generation-owned Markdown projection continuation.
//!
//! One projection generation applies its plan over several bounded GTK turns,
//! and the planner may end a turn *inside* a block wherever no inline state is
//! open. Everything the renderer needs to resume mid-block therefore lives in
//! one continuation value owned by that generation, rather than in per-turn
//! function locals: the tag stack, blockquote depth, list/definition flow
//! state, the delayed list marker, footnote numbering, and at most one
//! in-flight embedded-block buffer.
//!
//! The continuation also owns the projector's half of the batch seam. It tracks
//! the open block containers it is holding and compares them, by identity and
//! in order, against the continuation each batch says it expects. That check is
//! strictly stronger than a depth comparison: a mis-chained carry with the
//! right depth still fails it, and the failure resolves to an explicit terminal
//! instead of silently corrupted rendered content.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;
use pulldown_cmark::{Event, Tag, TagEnd};
use std::collections::HashMap;

use crate::services::markdown_render::{
    MAX_MARKDOWN_PLACEHOLDER_WIDGETS, MarkdownBlockOmission, MarkdownCarrySignature,
    MarkdownCodeBlockKind, MarkdownEventBatch, MarkdownOmissionMarker, MarkdownOmissionReason,
    MarkdownOmissionScope, MarkdownOpenContainer,
};

use super::LushtextMarkdownPreview;
use super::code_blocks::{ActiveCodeBlock, CodeBlockTheme};
use super::images::BufferedImage;
use super::imp::{
    TAG_ALERT_BODY, TAG_BLOCKQUOTE, TAG_BOLD, TAG_CODE, TAG_DEFINITION_DEF, TAG_DEFINITION_TERM,
    TAG_FOOTNOTE_DEF, TAG_FOOTNOTE_DEF_LABEL, TAG_FOOTNOTE_REF, TAG_HRULE, TAG_ITALIC, TAG_LINK,
    TAG_LIST_ITEM, TAG_STRIKETHROUGH, alert_title, alert_title_tag_name,
    ensure_blockquote_depth_tag, ensure_list_item_depth_tag, heading_tag_name,
};
use super::links::resolve_link_target;
use super::seams::{
    ActiveTextLink, DefinitionRenderState, ListFrame, ListItemRenderState, ListMarker,
    MarkdownPreviewRenderContext, RenderedTextLink,
};
use super::tables::BufferedTableBuilder;
use super::text_flow::{
    clear_current_definition_paragraph_end, clear_current_list_item_paragraph_end,
    current_definition_needs_paragraph_separator, current_list_item_needs_paragraph_separator,
    embedded_block_layout, ensure_rendered_line_break, flush_pending_list_prefix, footnote_number,
    heading_level_to_index, insert_blockquote_rail_if_needed, insert_task_list_marker,
    insert_with_tags, mark_current_definition_content, mark_current_definition_paragraph_end,
    mark_current_list_item_content, mark_current_list_item_paragraph_end, pop_tag,
    should_flush_pending_list_prefix,
};
use super::widgets::build_preview_limit_fallback_widget;

/// Accessible title of the widget that replaces one unprojectable block.
const OMISSION_FALLBACK_TITLE: &str = "Block not rendered";
/// Style class carried by the omission fallback widget.
const OMISSION_FALLBACK_CSS_CLASS: &str = "markdown-omission-fallback";

/// Why the projector refused to apply a batch to the continuation it holds.
///
/// Each variant is a contract violation between the planner and the projector,
/// not a document property, so the projector publishes an explicit terminal for
/// it rather than rendering content it cannot vouch for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ContinuationBreach {
    /// The batch expects open containers the projector is not holding.
    CarrySignature,
    /// A turn ended with inline state still open.
    OpenInlineState,
    /// A carried-embed charge arrived with no matching in-flight block.
    MissingEmbedCharge,
}

impl ContinuationBreach {
    /// Accessible terminal copy naming the refused batch.
    pub(super) fn description(self) -> &'static str {
        match self {
            Self::CarrySignature => {
                "Markdown preview stopped because a projection batch did not match its expected structure"
            }
            Self::OpenInlineState => {
                "Markdown preview stopped because a projection batch ended inside inline content"
            }
            Self::MissingEmbedCharge => {
                "Markdown preview stopped because an embedded block report had no matching block"
            }
        }
    }
}

/// Render state one projection generation carries between GTK turns.
///
/// Everything here is bounded by structural nesting depth or by an already
/// charged in-flight embedded block, never by document size: the `Vec` stacks
/// are depth-bounded by `MAX_MARKDOWN_STRUCTURE_DEPTH`, and the table/code
/// buffers are the planner's carried-embed tracks, whose retention the planner
/// caps before handing anything over.
pub(super) struct MarkdownProjectionContinuation {
    /// Active `GtkTextTag` names applied to inserted text.
    tag_stack: Vec<String>,
    /// Generic (non-alert) blockquote nesting depth.
    generic_blockquote_depth: usize,
    /// Open list frames with their marker style and ordinal.
    list_stack: Vec<ListFrame>,
    /// Row-flow state for each open list item.
    list_item_stack: Vec<ListItemRenderState>,
    /// Row-flow state for each open definition body.
    definition_stack: Vec<DefinitionRenderState>,
    /// Delayed list marker awaiting the item's first real content.
    pending_list_prefix: Option<String>,
    /// Whether the next block needs a separating blank row.
    needs_block_separator: bool,
    /// The single in-flight table, which may span projection turns.
    active_table: Option<BufferedTableBuilder>,
    /// The single in-flight code block, which may span projection turns.
    ///
    /// A checkpoint is admissible immediately after `Start(CodeBlock)`, so this
    /// must survive a turn boundary that carries no code text at all — not only
    /// the large-block case.
    active_code_block: Option<ActiveCodeBlock>,
    /// Generation-scoped footnote numbering.
    ///
    /// Numbering is owned by the render generation rather than one batch, so a
    /// reference and its definition agree even when they land in different GTK
    /// turns. Its size is bounded by the plan's retained footnote events, which
    /// the global event budget and the inline-footnote expansion budget already
    /// bound.
    footnote_numbers: HashMap<String, usize>,
    /// Next unassigned footnote ordinal for this generation.
    next_footnote_number: usize,
    /// Open block containers this continuation is holding, in order.
    held: Vec<MarkdownOpenContainer>,
    /// Omission fallback widgets already built for this generation.
    placeholder_widgets: usize,
}

impl MarkdownProjectionContinuation {
    /// Start one generation's continuation with nothing open.
    pub(super) fn new() -> Self {
        Self {
            tag_stack: Vec::new(),
            generic_blockquote_depth: 0,
            list_stack: Vec::new(),
            list_item_stack: Vec::new(),
            definition_stack: Vec::new(),
            pending_list_prefix: None,
            needs_block_separator: false,
            active_table: None,
            active_code_block: None,
            footnote_numbers: HashMap::new(),
            next_footnote_number: 1,
            held: Vec::new(),
            placeholder_widgets: 0,
        }
    }

    /// Whether the continuation expects exactly the containers a batch requires.
    ///
    /// Container identity is compared, not just count: a table carried where a
    /// list item is expected, or an ordered list resumed at the wrong ordinal,
    /// is a mismatch even though the depth agrees.
    fn matches(&self, expected: &MarkdownCarrySignature) -> bool {
        self.held == expected.containers()
    }

    /// Refuse a batch whose expected continuation this projector is not holding.
    ///
    /// Kept separate from `apply_batch` so the seam check is unit-testable
    /// without constructing a GTK widget.
    fn admit_batch(&self, batch: &MarkdownEventBatch) -> Result<(), ContinuationBreach> {
        if self.matches(batch.expected_carry()) {
            Ok(())
        } else {
            Err(ContinuationBreach::CarrySignature)
        }
    }

    /// Refuse a finished batch that did not leave open what it promised.
    ///
    /// This matters most on the *last* batch: nothing after it would compare
    /// signatures, so a divergence there would otherwise abandon an unclosed
    /// embedded block and still publish a complete preview.
    fn confirm_open_carry(
        &self,
        open_carry: &MarkdownCarrySignature,
    ) -> Result<(), ContinuationBreach> {
        if self.matches(open_carry) {
            Ok(())
        } else {
            Err(ContinuationBreach::CarrySignature)
        }
    }

    /// Apply one batch's markers and events, resuming mid-block if needed.
    ///
    /// `code_block_theme` is resolved once per generation by the caller rather
    /// than per turn. It cannot live in the continuation itself: the
    /// continuation travels inside the guarded projection payload that the
    /// disposal lane frees off the GTK thread, and a `GtkSourceStyleScheme` is
    /// not `Send`.
    pub(super) fn apply_batch(
        &mut self,
        preview: &LushtextMarkdownPreview,
        batch: MarkdownEventBatch,
        context: &MarkdownPreviewRenderContext,
        code_block_theme: &CodeBlockTheme,
    ) -> Result<(), ContinuationBreach> {
        self.admit_batch(&batch)?;
        let markers: Vec<MarkdownOmissionMarker> = batch.omissions().to_vec();
        let open_carry = batch.open_carry().clone();

        let imp = preview.imp();
        let buffer = imp.text_view.buffer();
        let mut iter = buffer.end_iter();

        // Link and image spans are inline, so an admissible batch boundary can
        // never fall inside one. They stay turn-local, and the invariant is
        // asserted below rather than assumed.
        let mut active_text_links: Vec<ActiveTextLink> = Vec::new();
        let mut active_image: Option<BufferedImage> = None;

        let mut next_marker = 0usize;
        for (index, event) in batch.into_events().into_iter().enumerate() {
            self.project_markers_at(
                preview,
                &buffer,
                &mut iter,
                index,
                &markers,
                &mut next_marker,
            )?;

            if let Some(table) = &mut self.active_table {
                match event {
                    Event::End(TagEnd::Table) => {
                        let table = self
                            .active_table
                            .take()
                            .expect("active table should exist")
                            .finish();
                        preview.insert_table_widget(&buffer, &mut iter, &table);
                        buffer.insert(&mut iter, "\n");
                        mark_current_definition_content(&mut self.definition_stack);
                        self.needs_block_separator = true;
                        self.pop_held(&MarkdownOpenContainer::Table {
                            alignments: Vec::new(),
                        });
                    }
                    other => table.push_event(other),
                }
                continue;
            }

            if let Some(code_block) = &mut self.active_code_block {
                match event {
                    Event::End(TagEnd::CodeBlock) => {
                        let active_code_block = self
                            .active_code_block
                            .take()
                            .expect("active code block should exist");
                        preview.insert_code_block_widget(
                            &buffer,
                            &mut iter,
                            &active_code_block.code_block,
                            code_block_theme,
                            active_code_block.layout,
                        );
                        buffer.insert(&mut iter, "\n");
                        mark_current_list_item_content(&mut self.list_item_stack);
                        mark_current_definition_content(&mut self.definition_stack);
                        self.needs_block_separator = true;
                        self.pop_held(&MarkdownOpenContainer::CodeBlock {
                            kind: MarkdownCodeBlockKind::Indented,
                        });
                    }
                    other => code_block.push_event(other),
                }
                continue;
            }

            if let Some(image) = &mut active_image {
                match event {
                    Event::End(TagEnd::Image) => {
                        let image = active_image.take().expect("active image should exist");
                        preview.insert_image_widget(&buffer, &mut iter, &image, context);
                        buffer.insert(&mut iter, "\n");
                        mark_current_definition_content(&mut self.definition_stack);
                        self.needs_block_separator = true;
                    }
                    other => image.push_event(other),
                }
                continue;
            }

            if self.pending_list_prefix.is_some() && should_flush_pending_list_prefix(&event) {
                insert_blockquote_rail_if_needed(
                    &buffer,
                    &mut iter,
                    &self.tag_stack,
                    self.generic_blockquote_depth,
                );
                if flush_pending_list_prefix(
                    &buffer,
                    &mut iter,
                    &self.tag_stack,
                    &mut self.pending_list_prefix,
                ) {
                    mark_current_list_item_content(&mut self.list_item_stack);
                }
            }

            match event {
                Event::Start(Tag::Table(alignments)) => {
                    if self.needs_block_separator {
                        buffer.insert(&mut iter, "\n");
                    }
                    self.held.push(MarkdownOpenContainer::Table {
                        alignments: alignments.clone(),
                    });
                    self.active_table =
                        Some(BufferedTableBuilder::new(alignments, context.clone()));
                    self.needs_block_separator = false;
                }
                Event::Start(tag) => match tag {
                    Tag::Heading { level, .. } => {
                        if self.needs_block_separator {
                            buffer.insert(&mut iter, "\n");
                        }
                        let idx = heading_level_to_index(level);
                        self.tag_stack.push(heading_tag_name(idx));
                        self.needs_block_separator = false;
                    }
                    Tag::Paragraph => {
                        if current_list_item_needs_paragraph_separator(&self.list_item_stack) {
                            buffer.insert(&mut iter, "\n");
                            clear_current_list_item_paragraph_end(&mut self.list_item_stack);
                        } else if current_definition_needs_paragraph_separator(
                            &self.definition_stack,
                        ) {
                            buffer.insert(&mut iter, "\n");
                            clear_current_definition_paragraph_end(&mut self.definition_stack);
                        } else if self.needs_block_separator
                            && (self.list_item_stack.is_empty()
                                || !self.definition_stack.is_empty())
                        {
                            buffer.insert(&mut iter, "\n");
                        }
                        self.needs_block_separator = false;
                    }
                    Tag::DefinitionList => {
                        if self.needs_block_separator {
                            buffer.insert(&mut iter, "\n");
                        }
                        self.held.push(MarkdownOpenContainer::DefinitionList);
                        self.needs_block_separator = false;
                    }
                    Tag::DefinitionListTitle => {
                        if self.needs_block_separator {
                            buffer.insert(&mut iter, "\n");
                        } else {
                            ensure_rendered_line_break(&buffer, &mut iter);
                        }
                        self.tag_stack.push(TAG_DEFINITION_TERM.to_string());
                        self.needs_block_separator = false;
                    }
                    Tag::DefinitionListDefinition => {
                        if self.needs_block_separator {
                            buffer.insert(&mut iter, "\n");
                        } else {
                            ensure_rendered_line_break(&buffer, &mut iter);
                        }
                        self.tag_stack.push(TAG_DEFINITION_DEF.to_string());
                        self.definition_stack.push(DefinitionRenderState::default());
                        self.held
                            .push(MarkdownOpenContainer::DefinitionListDefinition);
                        self.needs_block_separator = false;
                    }
                    Tag::BlockQuote(kind) => {
                        if self.needs_block_separator {
                            buffer.insert(&mut iter, "\n");
                        }
                        if let Some(kind) = kind {
                            let mut title_tags: Vec<&str> = self
                                .tag_stack
                                .iter()
                                .map(std::string::String::as_str)
                                .collect();
                            title_tags.push(TAG_ALERT_BODY);
                            title_tags.push(alert_title_tag_name(kind));
                            insert_with_tags(
                                &buffer,
                                &mut iter,
                                &format!("{}\n", alert_title(kind)),
                                &title_tags,
                            );
                            self.tag_stack.push(TAG_ALERT_BODY.to_string());
                        } else {
                            self.generic_blockquote_depth += 1;
                            self.tag_stack.push(TAG_BLOCKQUOTE.to_string());
                            let depth_tag =
                                ensure_blockquote_depth_tag(&buffer, self.generic_blockquote_depth);
                            self.tag_stack.push(depth_tag);
                        }
                        self.held.push(MarkdownOpenContainer::BlockQuote { kind });
                        self.needs_block_separator = false;
                    }
                    Tag::CodeBlock(kind) => {
                        if self.needs_block_separator {
                            buffer.insert(&mut iter, "\n");
                        }
                        let layout = embedded_block_layout(
                            &self.tag_stack,
                            &self.list_stack,
                            &self.list_item_stack,
                            self.generic_blockquote_depth,
                            &self.definition_stack,
                        );
                        self.held.push(MarkdownOpenContainer::CodeBlock {
                            kind: MarkdownCodeBlockKind::from_tag(&kind),
                        });
                        self.active_code_block = Some(ActiveCodeBlock::new(kind, layout));
                        self.needs_block_separator = false;
                    }
                    Tag::List(start_num) => {
                        if !self.list_item_stack.is_empty() {
                            if flush_pending_list_prefix(
                                &buffer,
                                &mut iter,
                                &self.tag_stack,
                                &mut self.pending_list_prefix,
                            ) {
                                mark_current_list_item_content(&mut self.list_item_stack);
                            }
                            ensure_rendered_line_break(&buffer, &mut iter);
                            clear_current_list_item_paragraph_end(&mut self.list_item_stack);
                        } else if self.needs_block_separator {
                            buffer.insert(&mut iter, "\n");
                        }
                        self.list_stack.push(ListFrame::new(start_num));
                        self.held.push(MarkdownOpenContainer::List {
                            ordered: start_num.is_some(),
                            next_number: start_num.unwrap_or_default(),
                        });
                        self.needs_block_separator = false;
                    }
                    Tag::Item => {
                        self.pending_list_prefix = Some(match self.list_stack.last() {
                            Some(frame) => frame.prefix(),
                            None => ListMarker::Unordered.prefix(),
                        });
                        let depth_tag =
                            ensure_list_item_depth_tag(&buffer, self.list_stack.len().max(1));
                        self.tag_stack.push(TAG_LIST_ITEM.to_string());
                        self.tag_stack.push(depth_tag);
                        self.list_item_stack.push(ListItemRenderState::default());
                        self.held.push(MarkdownOpenContainer::Item);
                    }
                    Tag::FootnoteDefinition(label) => {
                        if self.needs_block_separator {
                            buffer.insert(&mut iter, "\n");
                        }
                        self.tag_stack.push(TAG_FOOTNOTE_DEF.to_string());
                        let number = footnote_number(
                            &mut self.footnote_numbers,
                            &mut self.next_footnote_number,
                            label.as_ref(),
                        );
                        let mut tags: Vec<&str> = self
                            .tag_stack
                            .iter()
                            .map(std::string::String::as_str)
                            .collect();
                        tags.push(TAG_FOOTNOTE_DEF_LABEL);
                        insert_with_tags(&buffer, &mut iter, &format!("[{number}] "), &tags);
                        self.needs_block_separator = false;
                    }
                    Tag::Emphasis => self.tag_stack.push(TAG_ITALIC.to_string()),
                    Tag::Strong => self.tag_stack.push(TAG_BOLD.to_string()),
                    Tag::Strikethrough => self.tag_stack.push(TAG_STRIKETHROUGH.to_string()),
                    Tag::Link { dest_url, .. } => {
                        insert_blockquote_rail_if_needed(
                            &buffer,
                            &mut iter,
                            &self.tag_stack,
                            self.generic_blockquote_depth,
                        );
                        let target = resolve_link_target(dest_url.as_ref(), context);
                        let pushed_tag = target.is_some();
                        if pushed_tag {
                            self.tag_stack.push(TAG_LINK.to_string());
                        }
                        active_text_links.push(ActiveTextLink {
                            start_offset: iter.offset(),
                            target,
                            pushed_tag,
                        });
                    }
                    Tag::Image { dest_url, .. } => {
                        if self.needs_block_separator || (!iter.starts_line() && iter.offset() > 0)
                        {
                            buffer.insert(&mut iter, "\n");
                        }
                        active_image = Some(BufferedImage::new(dest_url.as_ref()));
                        self.needs_block_separator = false;
                    }
                    // Skip elements we don't render natively (HTML, math, metadata, etc.).
                    _ => {}
                },
                Event::End(tag_end) => match tag_end {
                    TagEnd::Heading(_) => {
                        pop_tag(&mut self.tag_stack);
                        buffer.insert(&mut iter, "\n");
                        self.needs_block_separator = true;
                    }
                    TagEnd::Paragraph => {
                        if self.list_item_stack.is_empty() {
                            ensure_rendered_line_break(&buffer, &mut iter);
                            if self.definition_stack.is_empty() {
                                self.needs_block_separator = true;
                            } else {
                                mark_current_definition_paragraph_end(&mut self.definition_stack);
                                self.needs_block_separator = false;
                            }
                        } else {
                            ensure_rendered_line_break(&buffer, &mut iter);
                            mark_current_list_item_paragraph_end(&mut self.list_item_stack);
                            self.needs_block_separator = false;
                        }
                    }
                    TagEnd::BlockQuote(kind) => {
                        if kind.is_some() {
                            pop_tag(&mut self.tag_stack);
                        } else {
                            pop_tag(&mut self.tag_stack);
                            pop_tag(&mut self.tag_stack);
                            self.generic_blockquote_depth =
                                self.generic_blockquote_depth.saturating_sub(1);
                        }
                        self.pop_held(&MarkdownOpenContainer::BlockQuote { kind });
                        self.needs_block_separator = true;
                    }
                    TagEnd::FootnoteDefinition => {
                        pop_tag(&mut self.tag_stack);
                        self.needs_block_separator = true;
                    }
                    TagEnd::DefinitionList => {
                        ensure_rendered_line_break(&buffer, &mut iter);
                        self.pop_held(&MarkdownOpenContainer::DefinitionList);
                        self.needs_block_separator = true;
                    }
                    TagEnd::DefinitionListTitle => {
                        pop_tag(&mut self.tag_stack);
                        ensure_rendered_line_break(&buffer, &mut iter);
                        self.needs_block_separator = false;
                    }
                    TagEnd::DefinitionListDefinition => {
                        pop_tag(&mut self.tag_stack);
                        ensure_rendered_line_break(&buffer, &mut iter);
                        self.definition_stack.pop();
                        self.pop_held(&MarkdownOpenContainer::DefinitionListDefinition);
                        self.needs_block_separator = false;
                    }
                    TagEnd::List(_) => {
                        self.list_stack.pop();
                        self.pop_held(&MarkdownOpenContainer::List {
                            ordered: false,
                            next_number: 0,
                        });
                        if self.list_stack.is_empty() {
                            self.needs_block_separator = true;
                        } else {
                            mark_current_list_item_content(&mut self.list_item_stack);
                            self.needs_block_separator = false;
                        }
                    }
                    TagEnd::Item => {
                        if flush_pending_list_prefix(
                            &buffer,
                            &mut iter,
                            &self.tag_stack,
                            &mut self.pending_list_prefix,
                        ) {
                            mark_current_list_item_content(&mut self.list_item_stack);
                        }
                        pop_tag(&mut self.tag_stack);
                        pop_tag(&mut self.tag_stack);
                        ensure_rendered_line_break(&buffer, &mut iter);
                        self.list_item_stack.pop();
                        if let Some(frame) = self.list_stack.last_mut() {
                            frame.advance();
                        }
                        self.pop_held(&MarkdownOpenContainer::Item);
                        self.advance_held_list_ordinal();
                    }
                    TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                        pop_tag(&mut self.tag_stack);
                    }
                    TagEnd::Link => {
                        if let Some(link) = active_text_links.pop() {
                            if link.pushed_tag {
                                pop_tag(&mut self.tag_stack);
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
                        &self.tag_stack,
                        self.generic_blockquote_depth,
                    );
                    let tags: Vec<&str> = self
                        .tag_stack
                        .iter()
                        .map(std::string::String::as_str)
                        .collect();
                    insert_with_tags(&buffer, &mut iter, &text, &tags);
                    mark_current_list_item_content(&mut self.list_item_stack);
                    mark_current_definition_content(&mut self.definition_stack);
                }
                Event::Code(code) => {
                    insert_blockquote_rail_if_needed(
                        &buffer,
                        &mut iter,
                        &self.tag_stack,
                        self.generic_blockquote_depth,
                    );
                    let mut tags: Vec<&str> = self
                        .tag_stack
                        .iter()
                        .map(std::string::String::as_str)
                        .collect();
                    tags.push(TAG_CODE);
                    insert_with_tags(&buffer, &mut iter, &code, &tags);
                    mark_current_list_item_content(&mut self.list_item_stack);
                    mark_current_definition_content(&mut self.definition_stack);
                }
                Event::FootnoteReference(label) => {
                    insert_blockquote_rail_if_needed(
                        &buffer,
                        &mut iter,
                        &self.tag_stack,
                        self.generic_blockquote_depth,
                    );
                    let number = footnote_number(
                        &mut self.footnote_numbers,
                        &mut self.next_footnote_number,
                        label.as_ref(),
                    );
                    let mut tags: Vec<&str> = self
                        .tag_stack
                        .iter()
                        .map(std::string::String::as_str)
                        .collect();
                    tags.push(TAG_FOOTNOTE_REF);
                    insert_with_tags(&buffer, &mut iter, &format!("[{number}]"), &tags);
                    mark_current_list_item_content(&mut self.list_item_stack);
                    mark_current_definition_content(&mut self.definition_stack);
                }
                Event::TaskListMarker(checked) => {
                    insert_blockquote_rail_if_needed(
                        &buffer,
                        &mut iter,
                        &self.tag_stack,
                        self.generic_blockquote_depth,
                    );
                    insert_task_list_marker(
                        &buffer,
                        &mut iter,
                        &self.tag_stack,
                        &mut self.pending_list_prefix,
                        checked,
                    );
                    mark_current_list_item_content(&mut self.list_item_stack);
                    mark_current_definition_content(&mut self.definition_stack);
                }
                Event::SoftBreak => {
                    buffer.insert(&mut iter, " ");
                }
                Event::HardBreak => {
                    buffer.insert(&mut iter, "\n");
                    mark_current_list_item_content(&mut self.list_item_stack);
                    mark_current_definition_content(&mut self.definition_stack);
                }
                Event::Rule => {
                    if self.needs_block_separator {
                        buffer.insert(&mut iter, "\n");
                    }
                    insert_with_tags(
                        &buffer,
                        &mut iter,
                        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
                        &[TAG_HRULE],
                    );
                    buffer.insert(&mut iter, "\n");
                    self.needs_block_separator = true;
                }
                // Skip HTML, math, images, and metadata — out of scope for native rendering.
                _ => {}
            }
        }

        // A marker recorded at the batch's end position belongs to this turn.
        self.project_markers_at(
            preview,
            &buffer,
            &mut iter,
            usize::MAX,
            &markers,
            &mut next_marker,
        )?;
        debug_assert_eq!(
            next_marker,
            markers.len(),
            "every marker in a batch must be projected or charged in that turn"
        );

        // Inline spans must be closed at an admissible boundary. In debug this
        // is a hard invariant; in release the batch is refused rather than
        // rendered with half an inline span left open.
        debug_assert!(
            active_text_links.is_empty(),
            "a batch boundary must not fall inside a link span"
        );
        debug_assert!(
            active_image.is_none(),
            "a batch boundary must not fall inside an image span"
        );
        if !active_text_links.is_empty() || active_image.is_some() {
            return Err(ContinuationBreach::OpenInlineState);
        }
        debug_assert!(
            self.matches(&open_carry),
            "the continuation must hold exactly what the batch left open"
        );
        self.confirm_open_carry(&open_carry)
    }

    /// Project every marker recorded at or before one batch event position.
    fn project_markers_at(
        &mut self,
        preview: &LushtextMarkdownPreview,
        buffer: &gtk4::TextBuffer,
        iter: &mut gtk4::TextIter,
        upto: usize,
        markers: &[MarkdownOmissionMarker],
        next_marker: &mut usize,
    ) -> Result<(), ContinuationBreach> {
        while let Some(marker) = markers
            .get(*next_marker)
            .filter(|marker| marker.at_event <= upto)
        {
            let omission = marker.omission;
            *next_marker += 1;
            self.project_one_marker(preview, buffer, iter, omission)?;
        }
        Ok(())
    }

    /// Charge a carried-embed report, or render one user-visible marker.
    fn project_one_marker(
        &mut self,
        preview: &LushtextMarkdownPreview,
        buffer: &gtk4::TextBuffer,
        iter: &mut gtk4::TextIter,
        omission: MarkdownBlockOmission,
    ) -> Result<(), ContinuationBreach> {
        match omission.reason {
            MarkdownOmissionReason::CarriedEmbedBytes
            | MarkdownOmissionReason::CarriedEmbedCells => self.charge_embed(omission),
            MarkdownOmissionReason::SliceEvents | MarkdownOmissionReason::SliceBytes => {
                self.render_visible_marker(preview, buffer, iter, omission);
                Ok(())
            }
        }
    }

    /// Charge one carried-embed report onto the in-flight block it describes.
    ///
    /// These reasons are charge carriers, not user-visible omissions: the
    /// projector's pre-existing in-place fallback already replaces that block
    /// and names its true size, so moving the counts across the seam is the
    /// whole job. A report with no matching in-flight block means the carry
    /// desynchronized, which is a breach rather than something to ignore.
    fn charge_embed(&mut self, omission: MarkdownBlockOmission) -> Result<(), ContinuationBreach> {
        match omission.reason {
            MarkdownOmissionReason::CarriedEmbedBytes => {
                let Some(code_block) = self.active_code_block.as_mut() else {
                    return Err(ContinuationBreach::MissingEmbedCharge);
                };
                code_block.charge_unretained_source_bytes(omission.unretained.source_bytes);
                Ok(())
            }
            MarkdownOmissionReason::CarriedEmbedCells => {
                let Some(table) = self.active_table.as_mut() else {
                    return Err(ContinuationBreach::MissingEmbedCharge);
                };
                table.charge_unretained_cells(omission.unretained.cells);
                Ok(())
            }
            MarkdownOmissionReason::SliceEvents | MarkdownOmissionReason::SliceBytes => Ok(()),
        }
    }

    /// Render one omission the reader would otherwise silently miss.
    fn render_visible_marker(
        &mut self,
        preview: &LushtextMarkdownPreview,
        buffer: &gtk4::TextBuffer,
        iter: &mut gtk4::TextIter,
        omission: MarkdownBlockOmission,
    ) {
        let budget = omission.marker_text();
        // The unit name comes from the live continuation, not from the omission
        // value and not from the batch signature: an in-flight embedded block
        // is the most specific enclosing container, and a container can be open
        // at the marker's position even when the batch's own signature is empty.
        if let Some(table) = self.active_table.as_mut() {
            table.push_omission_row(&container_marker_text(&budget, "table row"));
            return;
        }
        if let Some(code_block) = self.active_code_block.as_mut() {
            code_block.push_omission_line(&container_marker_text(&budget, "code block line"));
            return;
        }
        match omission.scope {
            MarkdownOmissionScope::ContainerSegment => {
                let text = match self.enclosing_unit_name() {
                    Some(unit) => container_marker_text(&budget, unit),
                    None => format!("[{budget}]"),
                };
                self.insert_container_marker(buffer, iter, &text);
            }
            MarkdownOmissionScope::TopLevelBlock => {
                if self.placeholder_widgets < MAX_MARKDOWN_PLACEHOLDER_WIDGETS {
                    self.insert_fallback_widget(preview, buffer, iter, &budget);
                } else {
                    // Past the widget cap the marker stays accessible text, so a
                    // pathological document cannot turn omissions into unbounded
                    // widget work.
                    self.insert_container_marker(buffer, iter, &format!("[{budget}]"));
                }
            }
        }
    }

    /// Name the innermost container the marker landed inside, if any.
    fn enclosing_unit_name(&self) -> Option<&'static str> {
        self.held.last().map(|container| match container {
            MarkdownOpenContainer::Item | MarkdownOpenContainer::List { .. } => "list item",
            MarkdownOpenContainer::Table { .. } => "table row",
            MarkdownOpenContainer::BlockQuote { .. } => "quoted paragraph",
            MarkdownOpenContainer::CodeBlock { .. } => "code block line",
            MarkdownOpenContainer::DefinitionList
            | MarkdownOpenContainer::DefinitionListDefinition => "definition body",
        })
    }

    /// Insert one accessible text marker inside the container being rendered.
    fn insert_container_marker(
        &mut self,
        buffer: &gtk4::TextBuffer,
        iter: &mut gtk4::TextIter,
        text: &str,
    ) {
        if self.needs_block_separator {
            buffer.insert(iter, "\n");
            self.needs_block_separator = false;
        }
        insert_blockquote_rail_if_needed(
            buffer,
            iter,
            &self.tag_stack,
            self.generic_blockquote_depth,
        );
        // Flushing the delayed marker first keeps an overflowing list item
        // rendered as an item, with the marker inside it.
        if flush_pending_list_prefix(buffer, iter, &self.tag_stack, &mut self.pending_list_prefix) {
            mark_current_list_item_content(&mut self.list_item_stack);
        }
        let tags: Vec<&str> = self
            .tag_stack
            .iter()
            .map(std::string::String::as_str)
            .collect();
        insert_with_tags(buffer, iter, text, &tags);
        ensure_rendered_line_break(buffer, iter);
        mark_current_list_item_content(&mut self.list_item_stack);
        mark_current_definition_content(&mut self.definition_stack);
    }

    /// Insert the in-place fallback widget for one unprojectable top-level block.
    fn insert_fallback_widget(
        &mut self,
        preview: &LushtextMarkdownPreview,
        buffer: &gtk4::TextBuffer,
        iter: &mut gtk4::TextIter,
        body: &str,
    ) {
        if self.needs_block_separator {
            buffer.insert(iter, "\n");
        }
        let widget = build_preview_limit_fallback_widget(
            OMISSION_FALLBACK_TITLE,
            body,
            OMISSION_FALLBACK_CSS_CLASS,
        );
        let layout = embedded_block_layout(
            &self.tag_stack,
            &self.list_stack,
            &self.list_item_stack,
            self.generic_blockquote_depth,
            &self.definition_stack,
        );
        preview.insert_embedded_widget(buffer, iter, &widget, layout);
        buffer.insert(iter, "\n");
        self.placeholder_widgets = self.placeholder_widgets.saturating_add(1);
        self.needs_block_separator = true;
    }

    /// Drop the innermost held container, tolerating an already-empty stack.
    ///
    /// `expected` documents the container the caller believes it is closing and
    /// is verified in debug builds. Release builds pop unconditionally: the
    /// carry-signature check on the next batch is the enforcing gate, and a
    /// mismatch there is reported as an explicit terminal.
    fn pop_held(&mut self, expected: &MarkdownOpenContainer) {
        let popped = self.held.pop();
        debug_assert!(
            popped
                .as_ref()
                .is_some_and(|container| same_container_kind(container, expected)),
            "closed a container the continuation was not holding: {popped:?} vs {expected:?}"
        );
    }

    /// Advance the innermost held ordered list after one item closed.
    fn advance_held_list_ordinal(&mut self) {
        if let Some(MarkdownOpenContainer::List {
            ordered,
            next_number,
        }) = self.held.last_mut()
            && *ordered
        {
            *next_number = next_number.saturating_add(1);
        }
    }
}

/// Compose one in-container marker naming both the budget and the unit.
fn container_marker_text(budget: &str, unit: &str) -> String {
    format!("[{budget}: one {unit}]")
}

/// Whether two container descriptors describe the same kind of container.
fn same_container_kind(left: &MarkdownOpenContainer, right: &MarkdownOpenContainer) -> bool {
    matches!(
        (left, right),
        (
            MarkdownOpenContainer::List { .. },
            MarkdownOpenContainer::List { .. }
        ) | (MarkdownOpenContainer::Item, MarkdownOpenContainer::Item)
            | (
                MarkdownOpenContainer::Table { .. },
                MarkdownOpenContainer::Table { .. }
            )
            | (
                MarkdownOpenContainer::BlockQuote { .. },
                MarkdownOpenContainer::BlockQuote { .. }
            )
            | (
                MarkdownOpenContainer::CodeBlock { .. },
                MarkdownOpenContainer::CodeBlock { .. }
            )
            | (
                MarkdownOpenContainer::DefinitionList,
                MarkdownOpenContainer::DefinitionList
            )
            | (
                MarkdownOpenContainer::DefinitionListDefinition,
                MarkdownOpenContainer::DefinitionListDefinition
            )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::markdown_render::UnretainedEmbedCounts;

    #[test]
    fn container_marker_text_names_both_the_budget_and_the_unit() {
        assert_eq!(
            container_marker_text("Markdown preview omitted part of one block", "list item"),
            "[Markdown preview omitted part of one block: one list item]"
        );
    }

    #[test]
    fn enclosing_unit_name_reads_the_live_continuation() {
        let mut continuation = MarkdownProjectionContinuation::new();
        assert_eq!(continuation.enclosing_unit_name(), None);
        continuation.held.push(MarkdownOpenContainer::List {
            ordered: true,
            next_number: 1,
        });
        assert_eq!(continuation.enclosing_unit_name(), Some("list item"));
        continuation.held.push(MarkdownOpenContainer::Item);
        assert_eq!(continuation.enclosing_unit_name(), Some("list item"));
        continuation
            .held
            .push(MarkdownOpenContainer::BlockQuote { kind: None });
        assert_eq!(continuation.enclosing_unit_name(), Some("quoted paragraph"));
        continuation
            .held
            .push(MarkdownOpenContainer::DefinitionListDefinition);
        assert_eq!(continuation.enclosing_unit_name(), Some("definition body"));
        // An indented code block is the only shape that reaches the code arm:
        // a checkpoint is admissible right after `Start(CodeBlock)`, so a cut
        // can land with the code container still held open.
        continuation.held.push(MarkdownOpenContainer::CodeBlock {
            kind: MarkdownCodeBlockKind::Indented,
        });
        assert_eq!(continuation.enclosing_unit_name(), Some("code block line"));
        continuation.held.push(MarkdownOpenContainer::Table {
            alignments: Vec::new(),
        });
        assert_eq!(continuation.enclosing_unit_name(), Some("table row"));
    }

    #[test]
    fn held_ordinal_advances_only_for_ordered_lists() {
        let mut continuation = MarkdownProjectionContinuation::new();
        continuation.held.push(MarkdownOpenContainer::List {
            ordered: false,
            next_number: 0,
        });
        continuation.advance_held_list_ordinal();
        assert_eq!(
            continuation.held,
            vec![MarkdownOpenContainer::List {
                ordered: false,
                next_number: 0,
            }]
        );

        continuation.held.clear();
        continuation.held.push(MarkdownOpenContainer::List {
            ordered: true,
            next_number: 3,
        });
        continuation.advance_held_list_ordinal();
        assert_eq!(
            continuation.held,
            vec![MarkdownOpenContainer::List {
                ordered: true,
                next_number: 4,
            }]
        );
    }

    #[test]
    fn a_mismatched_carry_signature_is_refused_before_any_rendering() {
        let mut continuation = MarkdownProjectionContinuation::new();
        assert!(continuation.matches(&MarkdownCarrySignature::default()));
        continuation.held.push(MarkdownOpenContainer::Item);
        assert!(!continuation.matches(&MarkdownCarrySignature::default()));
    }

    /// One real planned batch, so the seam is exercised against planner output
    /// rather than a hand-built signature.
    fn first_planned_batch(markdown: &str) -> MarkdownEventBatch {
        crate::services::markdown_render::plan_markdown(markdown)
            .batches
            .into_iter()
            .next()
            .expect("the fixture must plan at least one batch")
    }

    #[test]
    fn a_batch_is_refused_when_the_held_continuation_disagrees() {
        let batch = first_planned_batch("plain paragraph\n");
        assert!(batch.expected_carry().is_empty());

        // A fresh continuation is exactly what the first batch expects.
        let mut continuation = MarkdownProjectionContinuation::new();
        assert_eq!(continuation.admit_batch(&batch), Ok(()));

        // Holding an unexpected container is refused before anything renders.
        continuation.held.push(MarkdownOpenContainer::Table {
            alignments: Vec::new(),
        });
        assert_eq!(
            continuation.admit_batch(&batch),
            Err(ContinuationBreach::CarrySignature)
        );
    }

    #[test]
    fn a_batch_that_leaves_the_wrong_continuation_open_is_refused() {
        // The last batch has nobody after it to compare signatures, so the
        // open-carry contract must be enforced rather than assumed.
        let continuation = MarkdownProjectionContinuation::new();
        assert_eq!(
            continuation.confirm_open_carry(&MarkdownCarrySignature::default()),
            Ok(())
        );

        let mut diverged = MarkdownProjectionContinuation::new();
        diverged.held.push(MarkdownOpenContainer::CodeBlock {
            kind: MarkdownCodeBlockKind::Indented,
        });
        assert_eq!(
            diverged.confirm_open_carry(&MarkdownCarrySignature::default()),
            Err(ContinuationBreach::CarrySignature)
        );
    }

    #[test]
    fn a_carried_embed_charge_without_its_block_is_refused() {
        let mut continuation = MarkdownProjectionContinuation::new();
        for reason in [
            MarkdownOmissionReason::CarriedEmbedBytes,
            MarkdownOmissionReason::CarriedEmbedCells,
        ] {
            let omission = MarkdownBlockOmission {
                reason,
                scope: MarkdownOmissionScope::ContainerSegment,
                unretained: UnretainedEmbedCounts {
                    source_bytes: 4096,
                    cells: 8,
                },
            };
            assert_eq!(
                continuation.charge_embed(omission),
                Err(ContinuationBreach::MissingEmbedCharge),
                "{reason:?} has no in-flight block to charge"
            );
        }
    }

    #[test]
    fn breach_terminals_have_distinct_accessible_copy() {
        assert_eq!(
            ContinuationBreach::CarrySignature.description(),
            "Markdown preview stopped because a projection batch did not match its expected structure"
        );
        assert_eq!(
            ContinuationBreach::OpenInlineState.description(),
            "Markdown preview stopped because a projection batch ended inside inline content"
        );
        assert_eq!(
            ContinuationBreach::MissingEmbedCharge.description(),
            "Markdown preview stopped because an embedded block report had no matching block"
        );
        let descriptions = [
            ContinuationBreach::CarrySignature.description(),
            ContinuationBreach::OpenInlineState.description(),
            ContinuationBreach::MissingEmbedCharge.description(),
        ];
        for (index, left) in descriptions.iter().enumerate() {
            for right in &descriptions[index + 1..] {
                assert_ne!(left, right, "each breach must name its own cause");
            }
        }
    }
}
