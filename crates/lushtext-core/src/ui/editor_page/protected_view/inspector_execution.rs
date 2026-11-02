// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination (`inspector_execution`, the Line Inspector and
//! whole-line copy stage of the Protected View workflow; design D6,
//! `protected-view-inspector-and-surfaces` tasks 1.3–1.6).
//!
//! The inspector shows one window of at most `N_INSPECT` characters of one
//! paragraph. Every window is read **on a worker** from the session's
//! `ProtectedTextSource` (never from the protected buffer, ledger A66), under
//! a generation: a newer request supersedes an older one, and a completion
//! publishes only while its generation and the Protected View session it was
//! dispatched under are both current. While a read runs, the editor holds the
//! `line-inspector` readiness blocker.
//!
//! Copy Whole Line copies the paragraph's bytes out of the source on a worker
//! (only the paragraph, so the clipboard never keeps the whole document alive),
//! then installs a `SourceTextProvider` on the clipboard: a lazy
//! `GdkContentProvider` that writes the paragraph to a reader's stream one
//! `COPY_CHUNK_BYTES` chunk per main-loop dispatch. The line is never
//! materialized on the GTK thread.

use std::cell::{Cell, RefCell};

use gtk4::prelude::*;
use libadwaita::prelude::AdwDialogExt;

use super::inspector::{InspectorPresentation, InspectorRequest, LushtextLineInspector};
use super::policy::{N_INSPECT_WIDTH, inspector_window_containing, whole_line_copied_text};
use super::seams::InspectorWindowText;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::folded_text::SourceTextProvider;

/// The window the inspector shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ShownWindow {
    paragraph: u64,
    paragraph_chars: u64,
    start: u64,
    end: u64,
}

/// The editor's Line Inspector state.
#[derive(Debug, Default)]
pub(crate) struct LineInspectorState {
    dialog: RefCell<Option<LushtextLineInspector>>,
    generation: Cell<u64>,
    read_pending: Cell<bool>,
    shown: Cell<Option<ShownWindow>>,
    lines: Cell<u64>,
    /// A find match to select once its window is shown: (paragraph, column,
    /// length).
    highlight: Cell<Option<(u64, u64, u64)>>,
    /// Window reads published (evidence).
    windows_published: Cell<u64>,
    /// Whole-line copies installed on the clipboard (evidence).
    whole_line_copies: Cell<u64>,
}

impl LushtextEditorPage {
    /// Open (or re-target) the Line Inspector on the window containing
    /// `column` of `paragraph`. Does nothing outside Protected View.
    pub fn open_line_inspector(&self, paragraph: u64, column: u64) {
        if !self.is_protected_view() {
            return;
        }
        let state = self.line_inspector_state();
        if state.dialog.borrow().is_none() {
            let dialog = LushtextLineInspector::new();
            let page = self.downgrade();
            dialog.connect_request(move |request| {
                if let Some(page) = page.upgrade() {
                    page.answer_inspector_request(request);
                }
            });
            let page = self.downgrade();
            dialog.connect_closed(move |_| {
                if let Some(page) = page.upgrade() {
                    page.forget_line_inspector();
                }
            });
            state.dialog.replace(Some(dialog.clone()));
            dialog.present(Some(self));
        }
        self.read_inspector_window(paragraph, column, false);
    }

    /// Open the inspector on a find match in a hidden tail, selecting the
    /// match inside the window.
    pub(crate) fn open_line_inspector_on_match(&self, paragraph: u64, column: u64, chars: u64) {
        self.line_inspector_state()
            .highlight
            .set(Some((paragraph, column, chars)));
        self.open_line_inspector(paragraph, column);
    }

    /// Open the inspector from the keyboard or the command palette: on the
    /// first folded paragraph in the viewport, else the first visible line.
    pub fn open_line_inspector_at_viewport(&self) {
        if let Some(paragraph) = self.first_folded_paragraph_in_viewport() {
            self.open_line_inspector(paragraph, 0);
        }
    }

    /// The first folded paragraph whose line is in the visible rectangle, or
    /// the first visible line when none is folded. Two `line_at_y` lookups and
    /// one binary search over the fold plan: bounded, whatever the viewport.
    pub(crate) fn first_folded_paragraph_in_viewport(&self) -> Option<u64> {
        let view = self.source_view();
        let rect = view.visible_rect();
        let (top, _) = view.line_at_y(rect.y());
        let (bottom, _) = view.line_at_y(rect.y() + rect.height().max(1) - 1);
        let top = u64::try_from(top.line()).ok()?;
        let bottom = u64::try_from(bottom.line()).ok()?;
        let folded = self.protected_fold_plan().and_then(|plan| {
            let ranges = plan.ranges();
            let index = ranges.partition_point(|range| range.paragraph < top);
            ranges
                .get(index)
                .map(|range| range.paragraph)
                .filter(|paragraph| *paragraph <= bottom)
        });
        Some(folded.unwrap_or(top))
    }

    /// Navigation from another workflow (a bookmark, a workspace-search hit,
    /// a local-history target) to 0-based `line`: in Protected View, scroll
    /// the navigation mark there and return true, so the caller places no
    /// cursor (design D7). Returns false outside Protected View.
    pub(crate) fn navigate_protected_line(&self, line: u64) -> bool {
        if !self.is_protected_view() {
            return false;
        }
        self.scroll_protected_navigation_mark(line, 0);
        true
    }

    fn answer_inspector_request(&self, request: InspectorRequest) {
        let state = self.line_inspector_state();
        let Some(shown) = state.shown.get() else {
            return;
        };
        match request {
            InspectorRequest::PreviousWindow => {
                if let Some(column) = shown.start.checked_sub(1) {
                    self.read_inspector_window(shown.paragraph, column, true);
                }
            }
            InspectorRequest::NextWindow => {
                if shown.end < shown.paragraph_chars {
                    self.read_inspector_window(shown.paragraph, shown.end, true);
                }
            }
            InspectorRequest::GoTo { line, column } => {
                let paragraph = line
                    .saturating_sub(1)
                    .min(state.lines.get().saturating_sub(1));
                self.read_inspector_window(paragraph, column.saturating_sub(1), true);
            }
            InspectorRequest::CopyWindow => {
                if let Some(dialog) = state.dialog.borrow().as_ref() {
                    let buffer = dialog.window_view().buffer();
                    let (start, end) = buffer.bounds();
                    let text = buffer.text(&start, &end, true);
                    self.source_view().clipboard().set_text(&text);
                }
            }
            InspectorRequest::CopyWholeLine => self.copy_protected_whole_line(shown.paragraph),
            InspectorRequest::PreviousFoldedLine | InspectorRequest::NextFoldedLine => {
                let next = request == InspectorRequest::NextFoldedLine;
                if let Some(paragraph) = self.neighbouring_folded_paragraph(shown.paragraph, next) {
                    self.read_inspector_window(paragraph, 0, true);
                }
            }
        }
    }

    /// Read the window containing `column` of `paragraph` on a worker.
    fn read_inspector_window(&self, paragraph: u64, column: u64, announce: bool) {
        let state = self.line_inspector_state();
        let Some(source) = self.protected_source() else {
            // An edit-growth entry still capturing its source: the capture's
            // terminal re-reads (`refresh_line_inspector_for_new_source`).
            state.shown.set(Some(ShownWindow {
                paragraph,
                paragraph_chars: 0,
                start: column,
                end: column,
            }));
            return;
        };
        let Some(ticket) = self.protected_view_ticket() else {
            return;
        };
        // The first read builds the source's paragraph index on the worker;
        // its completion re-charges the live-editor estimate (task 1.1).
        let index_was_built = source.has_index();
        let generation = state.generation.get().wrapping_add(1);
        state.generation.set(generation);
        state.read_pending.set(true);
        // Within the shown paragraph the window is known now: navigation that
        // follows before the worker answers (two quick Previous presses)
        // starts from it, not from the window still on screen.
        if let Some(shown) = state.shown.get()
            && shown.paragraph == paragraph
            && shown.paragraph_chars > 0
            && let Some((_, window)) = inspector_window_containing(
                shown.paragraph_chars,
                N_INSPECT_WIDTH,
                column.min(shown.paragraph_chars - 1),
            )
        {
            state.shown.set(Some(ShownWindow {
                start: window.start,
                end: window.end,
                ..shown
            }));
        }
        gtk_lush_tasks::spawn_blocking_then(
            self.clone(),
            move || {
                let lines = source.paragraph_count();
                let paragraph = paragraph.min(lines.saturating_sub(1));
                source
                    .read_inspector_window(paragraph, column)
                    .map(|window| (window, lines))
            },
            move |page, read| {
                let state = page.line_inspector_state();
                if state.generation.get() != generation {
                    return;
                }
                state.read_pending.set(false);
                if !page.protected_view_current(&ticket) {
                    return;
                }
                if !index_was_built {
                    page.notify_memory_policy_changed();
                }
                if let Some((window, lines)) = read {
                    page.publish_inspector_window(&window, lines, announce);
                }
            },
        );
    }

    fn publish_inspector_window(&self, read: &InspectorWindowText, lines: u64, announce: bool) {
        let state = self.line_inspector_state();
        let Some(dialog) = state.dialog.borrow().clone() else {
            return;
        };
        state.lines.set(lines);
        state.shown.set(Some(ShownWindow {
            paragraph: read.paragraph,
            paragraph_chars: read.paragraph_chars,
            start: read.window.start,
            end: read.window.end,
        }));
        state
            .windows_published
            .set(state.windows_published.get().saturating_add(1));
        let hidden_chars = self
            .protected_fold_plan()
            .and_then(|plan| plan.range_of(read.paragraph))
            .map_or(0, |range| range.tail_chars);
        let presentation = InspectorPresentation {
            line: read.paragraph + 1,
            lines,
            start: read.window.start,
            end: read.window.end,
            paragraph_chars: read.paragraph_chars,
            has_previous: read.window.start > 0,
            has_next: read.window.end < read.paragraph_chars,
            hidden_chars,
            has_previous_folded: self
                .neighbouring_folded_paragraph(read.paragraph, false)
                .is_some(),
            has_next_folded: self
                .neighbouring_folded_paragraph(read.paragraph, true)
                .is_some(),
        };
        dialog.present_window(&presentation, &read.text, announce);
        if let Some((paragraph, column, chars)) = state.highlight.take()
            && paragraph == read.paragraph
            && read.window.contains(column)
        {
            dialog.select_window_range(column - read.window.start, chars);
        }
    }

    /// The nearest folded paragraph after (`next`) or before `paragraph`: one
    /// binary search of the fold plan.
    fn neighbouring_folded_paragraph(&self, paragraph: u64, next: bool) -> Option<u64> {
        let plan = self.protected_fold_plan()?;
        let ranges = plan.ranges();
        if next {
            let index = ranges.partition_point(|range| range.paragraph <= paragraph);
            ranges.get(index).map(|range| range.paragraph)
        } else {
            let index = ranges.partition_point(|range| range.paragraph < paragraph);
            index
                .checked_sub(1)
                .and_then(|index| ranges.get(index))
                .map(|range| range.paragraph)
        }
    }

    /// The dialog closed: forget it and any read in flight.
    fn forget_line_inspector(&self) {
        let state = self.line_inspector_state();
        state.dialog.replace(None);
        state.shown.set(None);
        state.generation.set(state.generation.get().wrapping_add(1));
        state.read_pending.set(false);
    }

    /// Close the inspector (the session left Protected View or the editor is
    /// going away).
    pub(in crate::ui::editor_page) fn close_line_inspector(&self) {
        let dialog = self.line_inspector_state().dialog.borrow().clone();
        if let Some(dialog) = dialog {
            dialog.force_close();
        }
        self.forget_line_inspector();
    }

    /// A new source landed (an eviction reload, or an edit-growth capture):
    /// re-read the shown window from it.
    pub(in crate::ui::editor_page) fn refresh_line_inspector_for_new_source(&self) {
        let state = self.line_inspector_state();
        if state.dialog.borrow().is_none() {
            return;
        }
        if let Some(shown) = state.shown.get() {
            self.read_inspector_window(shown.paragraph, shown.start, false);
        }
    }

    /// Copy Whole Line: locate the paragraph on a worker, then put a lazy,
    /// chunked provider on the clipboard.
    pub fn copy_protected_whole_line(&self, paragraph: u64) {
        let Some(source) = self.protected_source() else {
            return;
        };
        let Some(ticket) = self.protected_view_ticket() else {
            return;
        };
        gtk_lush_tasks::spawn_blocking_then(
            self.clone(),
            // The paragraph's bytes are copied into their own allocation here,
            // so the clipboard never keeps the whole source alive (review S1).
            move || {
                source.paragraph(paragraph).map(|located| {
                    let bytes = std::sync::Arc::<[u8]>::from(
                        &source.text().as_bytes()[located.start_byte..located.end_byte],
                    );
                    (bytes, located.chars)
                })
            },
            move |page, copied| {
                if !page.protected_view_current(&ticket) {
                    return;
                }
                let Some((bytes, chars)) = copied else { return };
                let provider = SourceTextProvider::ready(bytes);
                if page
                    .source_view()
                    .clipboard()
                    .set_content(Some(provider.upcast_ref::<gtk4::gdk::ContentProvider>()))
                    .is_ok()
                {
                    let state = page.line_inspector_state();
                    state
                        .whole_line_copies
                        .set(state.whole_line_copies.get().saturating_add(1));
                    page.announce_whole_line_copied(paragraph, chars);
                }
            },
        );
    }

    /// Say that Copy Whole Line put the line on the clipboard (review S6): in
    /// the status bar and to assistive technology, which an Info status
    /// message alone does not reach.
    fn announce_whole_line_copied(&self, paragraph: u64, chars: u64) {
        let text = whole_line_copied_text(paragraph, chars);
        if let Some(window) = self
            .root()
            .and_downcast::<crate::ui::window::LushtextWindow>()
        {
            window.publish_status_message(&text, crate::ui::status_bar::MessageKind::Info);
        }
        crate::ui::accessibility::announce(
            self.source_view(),
            &text,
            gtk4::AccessibleAnnouncementPriority::Medium,
        );
    }

    /// Whether the inspector is open.
    pub(crate) fn line_inspector_visible(&self) -> bool {
        self.line_inspector_state().dialog.borrow().is_some()
    }

    /// Whether a window read is running (the `line-inspector` blocker).
    pub(crate) fn line_inspector_read_pending(&self) -> bool {
        self.line_inspector_state().read_pending.get()
    }

    /// The inspector dialog, while open.
    #[must_use]
    pub fn line_inspector(&self) -> Option<LushtextLineInspector> {
        self.line_inspector_state().dialog.borrow().clone()
    }

    /// `(windows published, whole-line copies installed)`, for the evidence.
    pub(crate) fn line_inspector_counts(&self) -> (u64, u64) {
        let state = self.line_inspector_state();
        (state.windows_published.get(), state.whole_line_copies.get())
    }

    /// The shown window as (paragraph, start column, end column), for the
    /// evidence.
    pub(crate) fn line_inspector_window(&self) -> Option<(u64, u64, u64)> {
        self.line_inspector_state()
            .shown
            .get()
            .map(|shown| (shown.paragraph, shown.start, shown.end))
    }

    fn line_inspector_state(&self) -> &LineInspectorState {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        &self.imp().line_inspector
    }
}
