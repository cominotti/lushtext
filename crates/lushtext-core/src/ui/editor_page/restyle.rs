// SPDX-License-Identifier: GPL-3.0-or-later

//! Paced restyle of dense documents (`bound-post-open-settle`, design D9).
//!
//! **Cross-cutting, no role.** Its callers are the word-wrap preference
//! projection, the zoom and font CSS (`ui/theme.rs`, through
//! [`prepare_paced_restyle_everywhere`] and [`start_paced_reveal_everywhere`]),
//! the tab-width formatting (EditorConfig and the preference), and a hidden
//! tab's reattach (`view_attachment`, through `prepare_paced_reattach`), so it serves
//! several workflows; its pure plan, `next_restyle_range`, lives in the
//! document-load workflow's `policy.rs` beside `batch_exceeds_slice`.
//!
//! A change of the view's default text style (wrap mode, tabs, the
//! `.monospace` font) invalidates **every** line, and on a dense document
//! (`batch_exceeds_slice`) each 2000 px validation batch then costs up to
//! 21.6 ms in the lane. Mechanism A of the design (a tag carrying the style)
//! fails ledger A72 (the gutter takes its font from CSS, and the empty last
//! line has no character to tag), so every such change uses **mechanism B,
//! hide and reveal**, which A72 shows keeps the top line and the insert mark:
//!
//! 1. in the interaction dispatch, every paragraph outside the visible ones is
//!    hidden with an `invisible` tag (hidden text adds no layout, A24), and
//!    the change is applied, so only the visible paragraphs revalidate;
//! 2. then `G_PRIORITY_LOW` dispatches reveal the rest outward in
//!    paragraph-aligned ranges of at most `V_SLICE` visible characters
//!    (`next_restyle_range`); each reveal invalidates only its range, which the
//!    next frame lays out (A37);
//! 3. a newer change supersedes the running one: it hides again around the
//!    visible paragraphs and restarts from them.
//!
//! An editor that is not mapped hides everything and reveals once it is next
//! mapped, so only the selected editor is paced first (review R22). While a
//! reveal runs, a default search and the minimap see only the revealed text
//! (A33, A72); tags do not dirty the buffer or record undo (A27).
//!
//! Two guards keep hidden text from being lost or stranded:
//!
//! - GtkTextBuffer's clipboard serializer copies **visible** text only
//!   (`gtk_text_iter_get_visible_text`), so a cut of a selection spanning
//!   hidden paragraphs would delete text the clipboard never received. A
//!   selection appearing while a reveal runs therefore reveals everything at
//!   once, before any copy, cut or drag can read it.
//! - Edits during a reveal shift the line numbers its cursor walks, so the
//!   last reveal removes the tag from the whole buffer, never only its range.

use std::cell::{Cell, RefCell};

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;

use super::LushtextEditorPage;
use super::load::policy::{ParagraphSpan, RestyleCursor, next_restyle_range};
use crate::model::buffer_replacement::V_SLICE;

/// The most lines a document may have for its reattach to be paced
/// (`prepare_paced_reattach`): about 6.5 ms of hidden-line validation in the
/// first batch at the measured 0.4 µs per line, under half `B_SETTLE_US`.
///
/// Kind: cap; a power of two, rounded down.
const PACED_REATTACH_MAX_LINES: u32 = 1 << 14;

/// One editor's paced-restyle state.
#[derive(Default)]
pub struct RestyleState {
    tag: RefCell<Option<gtk4::TextTag>>,
    cursor: Cell<Option<RestyleCursor>>,
    /// The visible paragraphs when the restyle started: revealed already.
    visible: Cell<ParagraphSpan>,
    generation: Cell<u64>,
    source: RefCell<Option<glib::SourceId>>,
    map_handler: RefCell<Option<glib::SignalHandlerId>>,
    /// Evidence: restyles started, reveal dispatches, and the most visible
    /// characters one reveal dispatch made visible.
    restyles: Cell<u64>,
    reveals: Cell<u64>,
    largest_reveal_chars: Cell<u64>,
}

/// Observable paced-restyle state, projected into the load evidence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RestyleEvidence {
    /// Whether a reveal is still running (or waits for the editor's mapping).
    pub pending: bool,
    /// Paced restyles started.
    pub restyles: u64,
    /// Reveal dispatches run.
    pub reveals: u64,
    /// The most visible characters one reveal dispatch made visible.
    pub largest_reveal_chars: u64,
}

impl LushtextEditorPage {
    /// Apply a change of the view's default text style, paced on a dense
    /// document (module docs) and direct on any other.
    pub(crate) fn restyle_paced(&self, change: impl FnOnce(&Self)) {
        let paced = self.prepare_paced_restyle();
        change(self);
        if paced {
            self.start_paced_reveal();
        }
    }

    /// Hide every paragraph outside the visible ones, if the document is
    /// dense; returns whether it did. Pair with [`Self::start_paced_reveal`]
    /// after the change.
    pub(crate) fn prepare_paced_restyle(&self) -> bool {
        self.prepare_paced_restyle_around(None)
    }

    /// [`Self::prepare_paced_restyle`] for a view about to get its document
    /// back (`view_attachment`, review of `defer-background-tab-layout`): a
    /// reattached view lays the whole document out again from nothing (A83),
    /// which on a dense document is the same 2000 px batches a restyle pays
    /// (27–29 ms on `eviction/switch-hidden-dense`). The view is not mapped
    /// yet, so the paragraphs kept visible are the ones from `top_line` that
    /// fit one reveal's `V_SLICE` characters; the reveal then runs once the
    /// view is mapped.
    ///
    /// Only for a document of at most `PACED_REATTACH_MAX_LINES` lines: hidden
    /// lines still cost GTK about 0.4 µs each to validate, and they add no
    /// height, so one 2000 px batch walks all of them (an 8 MiB document of
    /// 204,601 lines took 78–80 ms in one dispatch, `eviction/reselect`
    /// `switch-hidden`). A longer document reattaches unpaced; its short lines
    /// keep each batch small (6.0–6.3 ms there).
    pub(crate) fn prepare_paced_reattach(&self, top_line: u32) -> bool {
        if !self.is_dense_document() {
            return false;
        }
        let buffer = self.buffer();
        let count = u32::try_from(crate::ui::protected_fold::document_line_count(
            buffer.upcast_ref(),
        ))
        .unwrap_or(u32::MAX);
        if count > PACED_REATTACH_MAX_LINES {
            return false;
        }
        let first = top_line.min(count);
        let mut end = first;
        let mut chars = 0u64;
        while end < count && chars < V_SLICE {
            let Some(line) = buffer.iter_at_line(i32::try_from(end).unwrap_or(i32::MAX)) else {
                break;
            };
            chars = chars.saturating_add(u64::try_from(line.chars_in_line()).unwrap_or(0));
            end += 1;
        }
        self.prepare_paced_restyle_around(Some(ParagraphSpan {
            first,
            end: end.max(first.saturating_add(1)).min(count),
        }))
    }

    fn prepare_paced_restyle_around(&self, shown: Option<ParagraphSpan>) -> bool {
        // A selection already there would let a copy read around the hidden
        // paragraphs (module docs), and an attached search context with a
        // query would find, count and replace around them (A33; review round
        // 2, SF4), so either runs the restyle unpaced. An empty query finds
        // nothing, so it paces (round 3, SF-C); the query becoming non-empty
        // reveals everything (`search_bar::execution`).
        if !self.is_dense_document()
            || self.search_has_query()
            || self.content_is_installing()
            || self.buffer().has_selection()
        {
            return false;
        }
        let state = &self.imp().restyle;
        state.generation.set(state.generation.get().wrapping_add(1));
        if let Some(source) = state.source.take() {
            source.remove();
        }
        let buffer = self.buffer();
        let count = u32::try_from(crate::ui::protected_fold::document_line_count(
            buffer.upcast_ref(),
        ))
        .unwrap_or(u32::MAX);
        let visible = shown.unwrap_or_else(|| self.visible_paragraphs(count));
        let tag = self.restyle_tag();
        self.remove_restyle_tag_everywhere();
        if let Some(first) = buffer.iter_at_line(i32::try_from(visible.first).unwrap_or(i32::MAX))
            && visible.first > 0
        {
            buffer.apply_tag(&tag, &buffer.start_iter(), &first);
        }
        if visible.end < count
            && let Some(after) = buffer.iter_at_line(i32::try_from(visible.end).unwrap_or(i32::MAX))
        {
            buffer.apply_tag(&tag, &after, &buffer.end_iter());
        }
        state.visible.set(visible);
        state.cursor.set(Some(RestyleCursor::new(visible, count)));
        state.restyles.set(state.restyles.get().saturating_add(1));
        true
    }

    /// Start revealing the hidden paragraphs outward, from low-priority
    /// dispatches once the editor is mapped.
    pub(crate) fn start_paced_reveal(&self) {
        let state = &self.imp().restyle;
        if state.cursor.get().is_none() {
            return;
        }
        let view = self.imp().source_view.get();
        if !view.is_mapped() {
            if state.map_handler.borrow().is_none() {
                let editor = self.downgrade();
                let handler = view.connect_map(move |_| {
                    if let Some(editor) = editor.upgrade() {
                        editor.arm_reveal();
                    }
                });
                state.map_handler.replace(Some(handler));
            }
            return;
        }
        self.arm_reveal();
    }

    /// Observable paced-restyle state.
    pub(crate) fn restyle_evidence(&self) -> RestyleEvidence {
        let state = &self.imp().restyle;
        RestyleEvidence {
            pending: state.cursor.get().is_some(),
            restyles: state.restyles.get(),
            reveals: state.reveals.get(),
            largest_reveal_chars: state.largest_reveal_chars.get(),
        }
    }

    /// Retire the reveal source and handler (from `dispose`).
    pub(crate) fn dispose_restyle(&self) {
        let state = &self.imp().restyle;
        state.generation.set(state.generation.get().wrapping_add(1));
        if let Some(source) = state.source.take() {
            source.remove();
        }
        if let Some(handler) = state.map_handler.take() {
            self.imp().source_view.disconnect(handler);
        }
    }

    fn restyle_tag(&self) -> gtk4::TextTag {
        let state = &self.imp().restyle;
        if let Some(tag) = state.tag.borrow().as_ref() {
            return tag.clone();
        }
        let tag = gtk4::TextTag::builder().invisible(true).build();
        let buffer = self.buffer();
        buffer.tag_table().add(&tag);
        // The clipboard reads visible text only (module docs).
        let editor = self.downgrade();
        buffer.connect_has_selection_notify(move |buffer| {
            if buffer.has_selection()
                && let Some(editor) = editor.upgrade()
            {
                editor.reveal_everything_now();
            }
        });
        state.tag.replace(Some(tag.clone()));
        tag
    }

    /// Whether the in-tab search context is attached with a non-empty query.
    fn search_has_query(&self) -> bool {
        let bar = &self.imp().search_bar;
        bar.search_context().is_some() && !bar.search_entry().text().is_empty()
    }

    /// End a running reveal by showing every hidden paragraph in this
    /// dispatch. Every reader that must see the whole document through
    /// GtkTextBuffer's visible-text APIs calls it first: the in-tab search
    /// context (find, the occurrence count, Replace and Replace All skip
    /// invisible text, A33) and a selection (copy, cut, drag).
    pub(crate) fn reveal_everything_now(&self) {
        let state = &self.imp().restyle;
        if state.cursor.get().is_none() {
            return;
        }
        state.generation.set(state.generation.get().wrapping_add(1));
        if let Some(source) = state.source.take() {
            source.remove();
        }
        state.cursor.set(None);
        self.remove_restyle_tag_everywhere();
    }

    /// Remove the hiding tag from every run that still carries it, and only
    /// from those: `gtk_text_buffer_remove_tag` invalidates the whole range it
    /// is given when the tag changes line sizes, whether or not the tag was
    /// there, so removing it from the whole buffer made GtkTextView lay out
    /// the whole dense document again after every paced reveal (a 23 ms
    /// validation dispatch on the lane's dense/710 wrap toggle).
    fn remove_restyle_tag_everywhere(&self) {
        let buffer = self.buffer();
        let tag = self.restyle_tag();
        let mut iter = buffer.start_iter();
        loop {
            if !iter.has_tag(&tag) && !iter.forward_to_tag_toggle(Some(&tag)) {
                return;
            }
            let start = iter;
            if !iter.forward_to_tag_toggle(Some(&tag)) {
                iter = buffer.end_iter();
            }
            buffer.remove_tag(&tag, &start, &iter);
            if iter.is_end() {
                return;
            }
        }
    }

    /// The paragraphs the view shows now; all of them when it is not mapped
    /// (nothing is visible, so everything waits for the mapping).
    fn visible_paragraphs(&self, count: u32) -> ParagraphSpan {
        let view = self.imp().source_view.get();
        if !view.is_mapped() {
            return ParagraphSpan { first: 0, end: 0 };
        }
        let rect = view.visible_rect();
        let (top, _) = view.line_at_y(rect.y());
        let (bottom, _) = view.line_at_y(rect.y() + rect.height());
        let first = u32::try_from(top.line()).unwrap_or(0);
        let end = u32::try_from(bottom.line())
            .unwrap_or(0)
            .saturating_add(1)
            .min(count);
        ParagraphSpan {
            first,
            end: end.max(first),
        }
    }

    fn arm_reveal(&self) {
        let state = &self.imp().restyle;
        if let Some(handler) = state.map_handler.take() {
            self.imp().source_view.disconnect(handler);
        }
        if state.source.borrow().is_some() || state.cursor.get().is_none() {
            return;
        }
        self.queue_reveal(state.generation.get());
    }

    /// Queue the next reveal dispatch at `G_PRIORITY_LOW`; it waits for the
    /// frame that shows the latest key first (`input_quiet::paced_idle`).
    fn queue_reveal(&self, generation: u64) {
        let (editor, store) = (self.downgrade(), self.downgrade());
        crate::ui::input_quiet::paced_idle(
            glib::Priority::LOW,
            move || {
                let Some(editor) = editor.upgrade() else {
                    return;
                };
                let state = &editor.imp().restyle;
                state.source.borrow_mut().take();
                if state.generation.get() == generation && editor.reveal_one_range() {
                    editor.queue_reveal(generation);
                }
            },
            move |source| {
                if let Some(editor) = store.upgrade() {
                    editor.imp().restyle.source.replace(Some(source));
                }
            },
        );
    }

    /// Reveal the next range that is still hidden; returns whether another
    /// dispatch is needed.
    fn reveal_one_range(&self) -> bool {
        let state = &self.imp().restyle;
        let Some(mut cursor) = state.cursor.get() else {
            return false;
        };
        let buffer = self.buffer();
        let visible = state.visible.get();
        let chars = |line: u32| {
            buffer
                .iter_at_line(i32::try_from(line).unwrap_or(i32::MAX))
                .map_or(0, |iter| u64::try_from(iter.chars_in_line()).unwrap_or(0))
        };
        loop {
            let Some((next, range)) = next_restyle_range(cursor, V_SLICE, chars) else {
                state.cursor.set(None);
                self.remove_restyle_tag_everywhere();
                return false;
            };
            cursor = next;
            // The visible paragraphs were never hidden.
            if range.first >= visible.first && range.end <= visible.end {
                continue;
            }
            let start = buffer.iter_at_line(i32::try_from(range.first).unwrap_or(i32::MAX));
            let end = buffer
                .iter_at_line(i32::try_from(range.end).unwrap_or(i32::MAX))
                .filter(|_| {
                    range.end
                        < u32::try_from(crate::ui::protected_fold::document_line_count(
                            buffer.upcast_ref(),
                        ))
                        .unwrap_or(0)
                })
                .unwrap_or_else(|| {
                    crate::ui::protected_fold::document_end_iter(buffer.upcast_ref())
                });
            if let Some(start) = start {
                // Visible characters, as `V_SLICE` counts them: the range's
                // characters less one delimiter per paragraph.
                let revealed = u64::try_from(end.offset() - start.offset())
                    .unwrap_or(0)
                    .saturating_sub(u64::from(range.len()));
                buffer.remove_tag(&self.restyle_tag(), &start, &end);
                state.reveals.set(state.reveals.get().saturating_add(1));
                state
                    .largest_reveal_chars
                    .set(state.largest_reveal_chars.get().max(revealed));
            }
            let done = cursor.is_done();
            if done {
                // Edits during the reveal may have moved hidden paragraphs
                // past the ranges walked (module docs).
                self.remove_restyle_tag_everywhere();
            }
            state.cursor.set((!done).then_some(cursor));
            return !done;
        }
    }
}

/// Every editor of every window of the running application.
fn every_editor() -> Vec<LushtextEditorPage> {
    gtk4::gio::Application::default()
        .and_then(|app| app.downcast::<gtk4::Application>().ok())
        .map(|app| {
            app.windows()
                .into_iter()
                .filter_map(|window| window.downcast::<crate::ui::window::LushtextWindow>().ok())
                .flat_map(|window| window.open_editors())
                .collect()
        })
        .unwrap_or_default()
}

/// Apply a display-wide `.monospace` change (zoom, font) with every dense
/// editor paced: hide around the visible paragraphs, apply, then reveal.
pub(crate) fn restyle_everywhere(change: impl FnOnce()) {
    let paced = prepare_paced_restyle_everywhere();
    change();
    start_paced_reveal_everywhere(&paced);
}

/// Before a display-wide `.monospace` change (zoom, font): hide around the
/// visible paragraphs of every dense editor; returns the ones that did.
pub fn prepare_paced_restyle_everywhere() -> Vec<LushtextEditorPage> {
    every_editor()
        .into_iter()
        .filter(LushtextEditorPage::prepare_paced_restyle)
        .collect()
}

/// After that change: start their reveals (an editor that is not mapped waits
/// for its mapping).
pub fn start_paced_reveal_everywhere(editors: &[LushtextEditorPage]) {
    for editor in editors {
        editor.start_paced_reveal();
    }
}
