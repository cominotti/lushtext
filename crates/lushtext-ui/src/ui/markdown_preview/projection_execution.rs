// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination — execution, qualified by stage order (projection).
//!
//! Turns an immutable plan into GTK text tags and anchored widgets, **at most
//! one batch per main-loop turn**. The per-turn ceiling is the point: a long
//! document renders progressively instead of freezing the frame, and the
//! projection yields with its cross-turn state reified in
//! `MarkdownProjectionContinuation` so the next turn resumes where the last
//! stopped rather than re-deriving position from the buffer.
//!
//! Resumption point: each queued batch turn. Every one revalidates the render
//! session generation first, so a superseded projection stops without
//! publishing. A `ContinuationBreach` is the case where cross-turn state no
//! longer describes the buffer; it fails the render loudly rather than
//! projecting onto a document it cannot account for.

use std::sync::atomic::Ordering;

use glib::subclass::prelude::ObjectSubclassIsExt;
use glib::translate::IntoGlib;
use gtk4::pango;
use gtk4::prelude::*;

use std::collections::VecDeque;

use super::code_blocks::CodeBlockTheme;
use lushtext_model::values::RenderGeneration;
use lushtext_services::notes::markdown_render::{MarkdownBlockQuoteKind, MarkdownRenderState};

use crate::ui::accessibility;
use lushtext_services::notes::markdown_render::MarkdownEventBatch;

use super::continuation::{ContinuationBreach, MarkdownProjectionContinuation};
use super::seams::{
    GuardedMarkdownPlan, GuardedMarkdownProjection, MarkdownProjectionTerminal,
    PendingMarkdownWork, simplified_render_description,
};
use super::{LushtextMarkdownPreview, MarkdownPreviewRenderContext};

impl LushtextMarkdownPreview {
    /// Accept a current immutable plan and project at most one batch per GTK turn.
    pub(super) fn start_render_plan(
        &self,
        generation: RenderGeneration,
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
        self.detach_rendered_buffer(false);
        self.imp()
            .render_session
            .borrow_mut()
            .transition(generation, MarkdownRenderState::Projecting);

        let user_visible_omissions = plan.user_visible_omissions();
        // One palette per generation instead of one per projection turn. It stays
        // outside the guarded projection payload because a GtkSourceView style
        // scheme is not `Send` and that payload is retired off the GTK thread.
        let code_block_theme = CodeBlockTheme::from_settings(&crate::ui::settings_writes::shared());
        let mut projection =
            Some(
                plan.map_preserving_reservation(|plan| GuardedMarkdownProjection {
                    batches: VecDeque::from(plan.batches),
                    limit: plan.limit,
                    user_visible_omissions,
                    continuation: MarkdownProjectionContinuation::new(),
                }),
            );
        // Preserve immediate small-document rendering while the planner's
        // event-and-byte ceilings bound this initial GTK turn exactly like
        // every deferred projection slice.
        if let Some(breach) = self.project_next_batch(
            generation,
            projection.as_deref_mut(),
            &context,
            &code_block_theme,
        ) {
            // A refused batch leaves the continuation holding an in-flight embed
            // buffer, so this takes the same accounted retirement path the
            // deferred slice uses: dropping it here would skip the pending
            // counter `render_pending()` consults and let readiness report
            // settled while a document-sized payload is still queued.
            if let Some(retired) = projection.take() {
                self.retire_guarded_markdown(retired);
            }
            self.finish_render_breach(generation, breach);
            return;
        }
        if projection
            .as_ref()
            .is_none_or(|projection| projection.batches.is_empty())
        {
            let terminal = projection
                .as_deref()
                .map(GuardedMarkdownProjection::terminal);
            drop(projection.take());
            self.finish_render_plan(generation, terminal.unwrap_or_default());
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
            if let Some(breach) = preview.project_next_batch(
                generation,
                projection.as_deref_mut(),
                &context,
                &code_block_theme,
            ) {
                if let Some(retired) = projection.take() {
                    preview.retire_guarded_markdown(retired);
                }
                preview.finish_render_breach(generation, breach);
                return glib::ControlFlow::Break;
            }
            if projection
                .as_ref()
                .is_none_or(|projection| projection.batches.is_empty())
            {
                let terminal = projection
                    .as_deref()
                    .map(GuardedMarkdownProjection::terminal);
                drop(projection.take());
                preview.finish_render_plan(generation, terminal.unwrap_or_default());
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    /// Apply at most one batch into the generation-owned continuation.
    ///
    /// Returns the breach that made the batch unusable, if any: the continuation
    /// validates each batch's expected structure before applying it, so a
    /// mis-chained carry becomes an explicit terminal instead of corrupted
    /// rendered content.
    pub(super) fn project_next_batch(
        &self,
        generation: RenderGeneration,
        projection: Option<&mut GuardedMarkdownProjection>,
        context: &MarkdownPreviewRenderContext,
        code_block_theme: &CodeBlockTheme,
    ) -> Option<ContinuationBreach> {
        let projection = projection?;
        let batch = projection.batches.pop_front()?;
        self.apply_render_batch(generation, &batch);
        projection
            .continuation
            .apply_batch(self, batch, context, code_block_theme)
            .err()
    }

    /// Record direct slice evidence before applying a current batch.
    pub(super) fn apply_render_batch(
        &self,
        generation: RenderGeneration,
        batch: &MarkdownEventBatch,
    ) {
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

    /// Publish the terminal this generation's projection reached.
    ///
    /// Three terminals stay distinguishable here. A global budget still stops
    /// planning and publishes `Limited` with that budget's own copy. A plan that
    /// reached the end of the document but replaced named units with markers
    /// publishes `Simplified` and reports the count once, rather than announcing
    /// each marker as it is projected. Everything else publishes `Complete`.
    pub(super) fn finish_render_plan(
        &self,
        generation: RenderGeneration,
        terminal: MarkdownProjectionTerminal,
    ) {
        let imp = self.imp();
        if !imp.render_session.borrow().is_current(generation) {
            return;
        }
        let (state, description) = match terminal.limit {
            Some(limit) => (
                MarkdownRenderState::Limited,
                Some(limit.description().to_string()),
            ),
            None if terminal.user_visible_omissions > 0 => (
                MarkdownRenderState::Simplified,
                Some(simplified_render_description(
                    terminal.user_visible_omissions,
                )),
            ),
            None => (MarkdownRenderState::Complete, None),
        };
        match description {
            Some(description) => {
                let buffer = imp.text_view.buffer();
                let mut end = buffer.end_iter();
                if end.offset() > 0 {
                    buffer.insert(&mut end, "\n\n");
                }
                buffer.insert(&mut end, &description);
                accessibility::set_description(&*imp.text_view, &description);
            }
            None => accessibility::set_description(&*imp.text_view, "Rendered Markdown preview"),
        }
        imp.render_session
            .borrow_mut()
            .transition(generation, state);
        self.queue_code_block_width_refresh();
    }

    /// Publish the explicit terminal for a batch the continuation refused.
    pub(super) fn finish_render_breach(
        &self,
        generation: RenderGeneration,
        breach: ContinuationBreach,
    ) {
        let imp = self.imp();
        if !imp.render_session.borrow().is_current(generation) {
            return;
        }
        let description = breach.description();
        let buffer = imp.text_view.buffer();
        let mut end = buffer.end_iter();
        if end.offset() > 0 {
            buffer.insert(&mut end, "\n\n");
        }
        buffer.insert(&mut end, description);
        accessibility::set_description(&*imp.text_view, description);
        imp.render_session
            .borrow_mut()
            .transition(generation, MarkdownRenderState::Failed);
        self.queue_code_block_width_refresh();
    }

    /// Cancel current planning/projection work without retaining stale payloads.
    pub(super) fn cancel_render_session(&self) {
        self.cancel_pending_markdown_planning();
        self.cancel_deferred_markdown_work();
        self.cancel_queued_image_work();
        let generation = self.imp().render_session.borrow_mut().cancel().get();
        self.imp()
            .render_generation
            .store(generation, Ordering::Release);
    }
}

/// Adwaita-matching accent color (blue) for headings and links.
const ACCENT_LIGHT: &str = "#1c71d8";
const ACCENT_DARK: &str = "#78aeed";

/// Adwaita-matching background for code spans and code blocks.
const CODE_BG_LIGHT: &str = "#f6f5f4";
const CODE_BG_DARK: &str = "#3d3846";

/// Adwaita-matching dim label color for blockquotes and horizontal rules.
const DIM_LIGHT: &str = "#5e5c64";
const DIM_DARK: &str = "#9a9996";

/// Accent-tinted background for read-only alert callouts in light mode.
const ALERT_BG_LIGHT: &str = "#f3f7ff";
/// Accent-tinted background for read-only alert callouts in dark mode.
const ALERT_BG_DARK: &str = "#263548";

/// Per-callout title colors for light mode so alert kinds stay visually distinct.
const ALERT_TITLE_NOTE_LIGHT: &str = "#1c71d8";
const ALERT_TITLE_TIP_LIGHT: &str = "#2b7a0b";
const ALERT_TITLE_IMPORTANT_LIGHT: &str = "#9141ac";
const ALERT_TITLE_WARNING_LIGHT: &str = "#c88800";
const ALERT_TITLE_CAUTION_LIGHT: &str = "#c01c28";

/// Per-callout title colors for dark mode so alert kinds stay visually distinct.
const ALERT_TITLE_NOTE_DARK: &str = "#78aeed";
const ALERT_TITLE_TIP_DARK: &str = "#57e389";
const ALERT_TITLE_IMPORTANT_DARK: &str = "#dc8add";
const ALERT_TITLE_WARNING_DARK: &str = "#f8e45c";
const ALERT_TITLE_CAUTION_DARK: &str = "#ff7b63";

/// Font scale factors for heading levels (h1=2.0x down to h6=1.0x).
const HEADING_SCALES: [f64; 6] = [2.0, 1.65, 1.35, 1.2, 1.1, 1.0];
/// Vertical space before each heading level, in text-buffer pixels.
const HEADING_PIXELS_ABOVE: [i32; 6] = [24, 20, 16, 12, 10, 8];
/// Vertical space after each heading level, in text-buffer pixels.
const HEADING_PIXELS_BELOW: [i32; 6] = [10, 8, 6, 5, 4, 4];
/// Marker-column margin for top-level list items in the preview.
const LIST_ITEM_MARKER_MARGIN: i32 = 24;
/// Space reserved between a list marker and the wrapped item text column.
const LIST_ITEM_MARKER_SLOT: i32 = 36;
/// Extra indentation applied for each additional nested list level.
const LIST_ITEM_DEPTH_STEP: i32 = 28;
/// Base left margin for generic blockquotes once the rail glyph is inserted.
const BLOCKQUOTE_BASE_MARGIN: i32 = 18;
/// Extra indentation applied for each additional nested generic blockquote.
const BLOCKQUOTE_DEPTH_STEP: i32 = 20;
/// Left inset for typed alert callout bodies.
pub(super) const ALERT_BODY_LEFT_MARGIN: i32 = 24;
/// Right inset for typed alert callout bodies.
pub(super) const ALERT_BODY_RIGHT_MARGIN: i32 = 16;
/// Left inset for rendered footnote definitions.
pub(super) const FOOTNOTE_DEF_LEFT_MARGIN: i32 = 32;
/// Right inset for rendered footnote definitions.
pub(super) const FOOTNOTE_DEF_RIGHT_MARGIN: i32 = 16;
/// Left inset for rendered definition-list bodies.
pub(super) const DEFINITION_DEF_LEFT_MARGIN: i32 = 32;
/// Right inset for rendered definition-list bodies.
pub(super) const DEFINITION_DEF_RIGHT_MARGIN: i32 = 16;
/// Visible rail glyph used to replace Markdown's raw `>` source marker.
const BLOCKQUOTE_RAIL: &str = "\u{2502}";

/// Tag names used in the TextBuffer. Keep in sync with `create_or_update_tags()`.
pub(super) const TAG_BOLD: &str = "bold";
pub(super) const TAG_ITALIC: &str = "italic";
pub(super) const TAG_STRIKETHROUGH: &str = "strikethrough";
pub(super) const TAG_CODE: &str = "code";
pub(super) const TAG_CODE_BLOCK: &str = "code-block";
pub(super) const TAG_LINK: &str = "link";
pub(super) const TAG_BLOCKQUOTE: &str = "blockquote";
pub(super) const TAG_LIST_ITEM: &str = "list-item";
pub(super) const TAG_TASK_MARKER: &str = "task-marker";
pub(super) const TAG_HRULE: &str = "horizontal-rule";
pub(super) const TAG_ALERT_BODY: &str = "alert-body";
pub(super) const TAG_FOOTNOTE_REF: &str = "footnote-ref";
pub(super) const TAG_FOOTNOTE_DEF: &str = "footnote-def";
pub(super) const TAG_FOOTNOTE_DEF_LABEL: &str = "footnote-def-label";
pub(super) const TAG_DEFINITION_TERM: &str = "definition-term";
pub(super) const TAG_DEFINITION_DEF: &str = "definition-definition";

/// Returns a heading tag name for the given level (0-indexed).
pub(super) fn heading_tag_name(level_idx: usize) -> String {
    format!("heading{}", level_idx + 1)
}

/// Returns the dynamic tag name used for one list nesting depth.
pub(super) fn list_item_tag_name(depth: usize) -> String {
    format!("list-item-depth-{depth}")
}

/// Return the marker-column margin used for one list nesting depth.
fn list_item_marker_margin(depth: usize) -> i32 {
    let extra_depth = depth.saturating_sub(1);
    let extra_margin = i32::try_from(extra_depth)
        .ok()
        .and_then(|depth| depth.checked_mul(LIST_ITEM_DEPTH_STEP))
        .unwrap_or(i32::MAX - LIST_ITEM_MARKER_MARGIN);
    LIST_ITEM_MARKER_MARGIN.saturating_add(extra_margin)
}

/// Return the wrapped-text margin used for one list nesting depth.
pub(super) fn list_item_text_margin(depth: usize) -> i32 {
    list_item_marker_margin(depth).saturating_add(LIST_ITEM_MARKER_SLOT)
}

/// Returns the dynamic tag name used for one generic blockquote depth.
fn blockquote_depth_tag_name(depth: usize) -> String {
    format!("blockquote-depth-{depth}")
}

/// Return the left margin used for one generic blockquote depth.
pub(super) fn blockquote_left_margin(depth: usize) -> i32 {
    let extra_depth = depth.saturating_sub(1);
    let extra_margin = i32::try_from(extra_depth)
        .ok()
        .and_then(|depth| depth.checked_mul(BLOCKQUOTE_DEPTH_STEP))
        .unwrap_or(i32::MAX - BLOCKQUOTE_BASE_MARGIN);
    BLOCKQUOTE_BASE_MARGIN.saturating_add(extra_margin)
}

/// Return the visible rail prefix for one generic blockquote nesting depth.
pub(super) fn blockquote_rail_prefix(depth: usize) -> String {
    (0..depth)
        .map(|_| BLOCKQUOTE_RAIL)
        .collect::<Vec<_>>()
        .join(" ")
        + " "
}

/// Return the tag name used for a typed alert callout title.
pub(super) fn alert_title_tag_name(kind: MarkdownBlockQuoteKind) -> &'static str {
    match kind {
        MarkdownBlockQuoteKind::Note => "alert-title-note",
        MarkdownBlockQuoteKind::Tip => "alert-title-tip",
        MarkdownBlockQuoteKind::Important => "alert-title-important",
        MarkdownBlockQuoteKind::Warning => "alert-title-warning",
        MarkdownBlockQuoteKind::Caution => "alert-title-caution",
    }
}

/// Return the user-facing title inserted at the start of a typed alert callout.
pub(super) fn alert_title(kind: MarkdownBlockQuoteKind) -> &'static str {
    match kind {
        MarkdownBlockQuoteKind::Note => "Note",
        MarkdownBlockQuoteKind::Tip => "Tip",
        MarkdownBlockQuoteKind::Important => "Important",
        MarkdownBlockQuoteKind::Warning => "Warning",
        MarkdownBlockQuoteKind::Caution => "Caution",
    }
}

/// Create (or update in-place) all TextTags used by the Markdown renderer.
///
/// If a tag already exists in the buffer's tag table, its color properties are
/// updated rather than recreated. This preserves any text that's already been
/// inserted with those tags — re-creating tags would orphan references.
pub(super) fn create_or_update_tags(buffer: &gtk4::TextBuffer, is_dark: bool) {
    let accent = if is_dark { ACCENT_DARK } else { ACCENT_LIGHT };
    let code_bg = if is_dark { CODE_BG_DARK } else { CODE_BG_LIGHT };
    let dim = if is_dark { DIM_DARK } else { DIM_LIGHT };
    let alert_bg = if is_dark {
        ALERT_BG_DARK
    } else {
        ALERT_BG_LIGHT
    };

    let table = buffer.tag_table();

    let get_or_create = |name: &str| -> gtk4::TextTag {
        if let Some(tag) = table.lookup(name) {
            tag
        } else {
            let tag = gtk4::TextTag::new(Some(name));
            table.add(&tag);
            tag
        }
    };

    for (i, &scale) in HEADING_SCALES.iter().enumerate() {
        let tag = get_or_create(&heading_tag_name(i));
        tag.set_scale(scale);
        tag.set_weight(pango::Weight::Bold.into_glib());
        tag.set_foreground(Some(accent));
        tag.set_pixels_above_lines(HEADING_PIXELS_ABOVE[i]);
        tag.set_pixels_below_lines(HEADING_PIXELS_BELOW[i]);
        tag.set_underline(if i < 2 {
            pango::Underline::Single
        } else {
            pango::Underline::None
        });
    }

    let bold = get_or_create(TAG_BOLD);
    bold.set_weight(pango::Weight::Bold.into_glib());

    let italic = get_or_create(TAG_ITALIC);
    italic.set_style(pango::Style::Italic);

    let strikethrough = get_or_create(TAG_STRIKETHROUGH);
    strikethrough.set_strikethrough(true);

    let code = get_or_create(TAG_CODE);
    code.set_family(Some("Monospace"));
    code.set_background(Some(code_bg));

    let code_block = get_or_create(TAG_CODE_BLOCK);
    code_block.set_family(Some("Monospace"));
    code_block.set_paragraph_background(Some(code_bg));
    code_block.set_left_margin(16);
    code_block.set_right_margin(16);
    code_block.set_pixels_above_lines(4);
    code_block.set_pixels_below_lines(4);

    let link = get_or_create(TAG_LINK);
    link.set_foreground(Some(accent));
    link.set_underline(pango::Underline::Single);

    let blockquote = get_or_create(TAG_BLOCKQUOTE);
    blockquote.set_foreground(Some(dim));
    blockquote.set_pixels_above_lines(2);
    blockquote.set_pixels_below_lines(2);

    let list_item = get_or_create(TAG_LIST_ITEM);
    list_item.set_left_margin(0);
    list_item.set_indent(0);

    let task_marker = get_or_create(TAG_TASK_MARKER);
    task_marker.set_family(Some("Monospace"));
    task_marker.set_foreground(Some(accent));
    task_marker.set_weight(pango::Weight::Bold.into_glib());

    let hrule = get_or_create(TAG_HRULE);
    hrule.set_foreground(Some(dim));
    hrule.set_justification(gtk4::Justification::Center);
    hrule.set_pixels_above_lines(8);
    hrule.set_pixels_below_lines(8);

    let alert_body = get_or_create(TAG_ALERT_BODY);
    alert_body.set_left_margin(ALERT_BODY_LEFT_MARGIN);
    alert_body.set_right_margin(ALERT_BODY_RIGHT_MARGIN);
    alert_body.set_paragraph_background(Some(alert_bg));
    alert_body.set_pixels_above_lines(4);
    alert_body.set_pixels_below_lines(4);

    for (kind, light, dark) in [
        (
            MarkdownBlockQuoteKind::Note,
            ALERT_TITLE_NOTE_LIGHT,
            ALERT_TITLE_NOTE_DARK,
        ),
        (
            MarkdownBlockQuoteKind::Tip,
            ALERT_TITLE_TIP_LIGHT,
            ALERT_TITLE_TIP_DARK,
        ),
        (
            MarkdownBlockQuoteKind::Important,
            ALERT_TITLE_IMPORTANT_LIGHT,
            ALERT_TITLE_IMPORTANT_DARK,
        ),
        (
            MarkdownBlockQuoteKind::Warning,
            ALERT_TITLE_WARNING_LIGHT,
            ALERT_TITLE_WARNING_DARK,
        ),
        (
            MarkdownBlockQuoteKind::Caution,
            ALERT_TITLE_CAUTION_LIGHT,
            ALERT_TITLE_CAUTION_DARK,
        ),
    ] {
        let tag = get_or_create(alert_title_tag_name(kind));
        tag.set_foreground(Some(if is_dark { dark } else { light }));
        tag.set_weight(pango::Weight::Bold.into_glib());
        tag.set_scale(1.05);
    }

    let footnote_ref = get_or_create(TAG_FOOTNOTE_REF);
    footnote_ref.set_foreground(Some(accent));
    footnote_ref.set_scale(0.85);
    footnote_ref.set_weight(pango::Weight::Bold.into_glib());

    let footnote_def = get_or_create(TAG_FOOTNOTE_DEF);
    footnote_def.set_left_margin(FOOTNOTE_DEF_LEFT_MARGIN);
    footnote_def.set_right_margin(FOOTNOTE_DEF_RIGHT_MARGIN);
    footnote_def.set_pixels_above_lines(2);
    footnote_def.set_pixels_below_lines(2);

    let footnote_def_label = get_or_create(TAG_FOOTNOTE_DEF_LABEL);
    footnote_def_label.set_family(Some("Monospace"));
    footnote_def_label.set_foreground(Some(accent));
    footnote_def_label.set_weight(pango::Weight::Bold.into_glib());

    let definition_term = get_or_create(TAG_DEFINITION_TERM);
    definition_term.set_weight(pango::Weight::Bold.into_glib());
    definition_term.set_pixels_above_lines(4);
    definition_term.set_pixels_below_lines(1);

    let definition_def = get_or_create(TAG_DEFINITION_DEF);
    definition_def.set_left_margin(DEFINITION_DEF_LEFT_MARGIN);
    definition_def.set_right_margin(DEFINITION_DEF_RIGHT_MARGIN);
    definition_def.set_pixels_above_lines(1);
    definition_def.set_pixels_below_lines(2);
}

/// Ensure the tag used for a given list nesting depth exists and return its name.
pub(super) fn ensure_list_item_depth_tag(buffer: &gtk4::TextBuffer, depth: usize) -> String {
    let name = list_item_tag_name(depth);
    let tag = if let Some(tag) = buffer.tag_table().lookup(&name) {
        tag
    } else {
        let tag = gtk4::TextTag::new(Some(&name));
        buffer.tag_table().add(&tag);
        tag
    };
    tag.set_left_margin(list_item_text_margin(depth));
    tag.set_indent(-LIST_ITEM_MARKER_SLOT);
    name
}

/// Ensure the tag used for a generic blockquote depth exists and return its name.
pub(super) fn ensure_blockquote_depth_tag(buffer: &gtk4::TextBuffer, depth: usize) -> String {
    let name = blockquote_depth_tag_name(depth);
    if buffer.tag_table().lookup(&name).is_some() {
        return name;
    }

    let tag = gtk4::TextTag::new(Some(&name));
    tag.set_left_margin(blockquote_left_margin(depth));
    buffer.tag_table().add(&tag);
    name
}
