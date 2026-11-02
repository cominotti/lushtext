// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination (`find_execution`, the worker-find stage of the
//! Protected View workflow; design D5, tasks 6.1–6.3).
//!
//! A protected document attaches no `GtkSourceSearchContext` (it scans the
//! whole buffer from an idle and skips hidden text, ledger A33). The in-tab
//! search bar routes its query, option toggles and next/previous here
//! instead (`LushtextSearchBar::set_external_find`), and a worker searches the
//! document's text source, hidden tails included. One request is active: a
//! newer one cancels it through a shared flag and a generation check discards
//! its result. A match is published as (paragraph, column, length) so
//! navigation never positions an iterator inside a hidden tail (A66): a match
//! in a visible head is tagged and scrolled to through a `protected-nav` mark;
//! a match in a hidden tail scrolls to its paragraph's start and, when the
//! user navigates to it, opens the Line Inspector on its window
//! (`policy::navigation_target`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gtk4::prelude::*;

use super::policy::{
    FIND_RESULT_CAP, N_INSPECT_WIDTH, NavigationTarget, ParagraphExtent, navigation_target,
};
use crate::model::paragraph_boundary::next_paragraph_break;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::search_bar::{ExternalFindHandler, ExternalFindRequest, SearchOption};

/// The tag on the current match while it lies in visible text.
const CURRENT_MATCH_TAG: &str = "lushtext-protected-find-current";
/// The mark navigation scrolls to; never the insert mark.
const NAVIGATION_MARK: &str = "lushtext-protected-nav";

/// One match: its paragraph, its column there and its length, in characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FindHit {
    pub(crate) paragraph: u64,
    pub(crate) column: u64,
    pub(crate) chars: u64,
}

/// The editor's protected find state.
#[derive(Debug, Default)]
pub(crate) struct ProtectedFindState {
    generation: Cell<u64>,
    cancel: RefCell<Option<Arc<AtomicBool>>>,
    hits: RefCell<Vec<FindHit>>,
    current: Cell<Option<usize>>,
}

impl LushtextEditorPage {
    /// Route the search bar's find here while protected (`on`), or back.
    pub(in crate::ui::editor_page) fn set_protected_find_routing(&self, on: bool) {
        let handler: Option<ExternalFindHandler> = on.then(|| {
            let page = self.downgrade();
            Rc::new(move |request: ExternalFindRequest| {
                if let Some(page) = page.upgrade() {
                    page.handle_protected_find(request);
                }
            }) as ExternalFindHandler
        });
        self.search_bar().set_external_find(handler);
        // Routing changes with every install and exit: results of the
        // previous content never survive it.
        self.clear_protected_find();
    }

    fn handle_protected_find(&self, request: ExternalFindRequest) {
        match request {
            ExternalFindRequest::QueryChanged => self.start_protected_find(),
            ExternalFindRequest::Next => self.step_protected_find(true),
            ExternalFindRequest::Previous => self.step_protected_find(false),
        }
    }

    /// Start one worker search over the source, superseding any active one.
    fn start_protected_find(&self) {
        let state = self.protected_find_state();
        if let Some(previous) = state.cancel.take() {
            previous.store(true, Ordering::Relaxed);
        }
        let generation = state.generation.get().wrapping_add(1);
        state.generation.set(generation);
        state.hits.borrow_mut().clear();
        state.current.set(None);
        self.clear_current_match_tag();
        let bar = self.search_bar();
        let query = bar.search_entry().text().to_string();
        let Some(source) = self.protected_source() else {
            // An edit-entered session still capturing its source restarts
            // this request when the capture lands.
            bar.set_match_count(0, 0);
            return;
        };
        if query.is_empty() {
            bar.set_match_count(0, 0);
            return;
        }
        let pattern = if bar.option_enabled(SearchOption::Regex) {
            query
        } else {
            regex::escape(&query)
        };
        let pattern = if bar.option_enabled(SearchOption::WholeWord) {
            format!(r"\b(?:{pattern})\b")
        } else {
            pattern
        };
        let case_insensitive = !bar.option_enabled(SearchOption::CaseSensitive);
        let Some(ticket) = self.protected_view_ticket() else {
            return;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        state.cancel.replace(Some(Arc::clone(&cancel)));
        gtk_lush_tasks::spawn_blocking_then(
            self.clone(),
            move || {
                let regex = regex::RegexBuilder::new(&pattern)
                    .case_insensitive(case_insensitive)
                    .build()
                    .ok()?;
                Some(find_hits(source.text(), &regex, &cancel))
            },
            move |page, hits| {
                let state = page.protected_find_state();
                if state.generation.get() != generation || !page.protected_view_current(&ticket) {
                    return;
                }
                state.cancel.replace(None);
                let hits = hits.unwrap_or_default();
                let first = (!hits.is_empty()).then_some(0);
                state.hits.replace(hits);
                state.current.set(first);
                page.publish_protected_find(false);
            },
        );
    }

    /// Move to the next or previous match, wrapping.
    fn step_protected_find(&self, forward: bool) {
        let state = self.protected_find_state();
        let total = state.hits.borrow().len();
        if total == 0 {
            return;
        }
        let next = match (state.current.get(), forward) {
            (None, _) => 0,
            (Some(index), true) => (index + 1) % total,
            (Some(index), false) => (index + total - 1) % total,
        };
        state.current.set(Some(next));
        self.publish_protected_find(true);
    }

    /// Show the current match and the counter. `activated` is an explicit
    /// navigation (next or previous): a match in a hidden tail then opens the
    /// Line Inspector on its window. The automatic first match of a new query
    /// only scrolls, so typing never pops a dialog over the search bar; an
    /// inspector that is already open follows it.
    fn publish_protected_find(&self, activated: bool) {
        let state = self.protected_find_state();
        let total = state.hits.borrow().len();
        let current = state.current.get();
        let to_i32 = |value: usize| i32::try_from(value).unwrap_or(i32::MAX);
        self.search_bar()
            .set_match_count(current.map_or(0, |index| to_i32(index + 1)), to_i32(total));
        self.clear_current_match_tag();
        let Some(hit) = current.and_then(|index| state.hits.borrow().get(index).copied()) else {
            return;
        };
        let Ok(line) = i32::try_from(hit.paragraph) else {
            return;
        };
        let range = self
            .protected_fold_plan()
            .and_then(|plan| plan.range_of(hit.paragraph));
        let visible =
            range.is_none_or(|range| hit.column.saturating_add(hit.chars) <= range.head_chars);
        if visible {
            let buffer = self.buffer();
            let (Ok(column), Ok(chars)) = (i32::try_from(hit.column), i32::try_from(hit.chars))
            else {
                return;
            };
            let Some(start) = buffer.iter_at_line_offset(line, column) else {
                return;
            };
            let mut end = start;
            end.forward_chars(chars);
            buffer.apply_tag(&current_match_tag(buffer.upcast_ref()), &start, &end);
            self.scroll_protected_navigation_iter(&start, false);
            return;
        }
        self.scroll_protected_navigation_mark(hit.paragraph, 0);
        if activated || self.line_inspector_visible() {
            let extent = range.and_then(|range| {
                ParagraphExtent::new(range.head_chars, range.head_chars + range.tail_chars)
            });
            if let Some(NavigationTarget::Inspect { paragraph, .. }) = extent
                .map(|extent| navigation_target(hit.paragraph, hit.column, extent, N_INSPECT_WIDTH))
            {
                self.open_line_inspector_on_match(paragraph, hit.column, hit.chars);
            }
        }
    }

    /// Scroll the navigation mark to `column` of `paragraph` (a visible
    /// column; callers pass 0 for a paragraph start). Never moves the insert
    /// mark (design D7).
    pub(crate) fn scroll_protected_navigation_mark(&self, paragraph: u64, column: u64) {
        self.scroll_protected_navigation(paragraph, column, false);
    }

    /// As [`Self::scroll_protected_navigation_mark`], with the paragraph at
    /// the top of the view (a restored scroll position).
    pub(crate) fn scroll_protected_navigation_to_top(&self, paragraph: u64) {
        self.scroll_protected_navigation(paragraph, 0, true);
    }

    fn scroll_protected_navigation(&self, paragraph: u64, column: u64, top: bool) {
        let buffer = self.buffer();
        let (Ok(line), Ok(column)) = (i32::try_from(paragraph), i32::try_from(column)) else {
            return;
        };
        let target = buffer
            .iter_at_line_offset(line, column)
            .or_else(|| buffer.iter_at_line(line))
            .unwrap_or_else(|| buffer.end_iter());
        self.scroll_protected_navigation_iter(&target, top);
    }

    fn scroll_protected_navigation_iter(&self, target: &gtk4::TextIter, top: bool) {
        let buffer = self.buffer();
        let mark = buffer
            .mark(NAVIGATION_MARK)
            .unwrap_or_else(|| buffer.create_mark(Some(NAVIGATION_MARK), target, true));
        buffer.move_mark(&mark, target);
        if top {
            self.source_view()
                .scroll_to_mark(&mark, 0.0, true, 0.0, 0.0);
        } else {
            self.source_view()
                .scroll_to_mark(&mark, 0.1, false, 0.0, 0.0);
        }
    }

    fn clear_current_match_tag(&self) {
        let buffer = self.buffer();
        if let Some(tag) = buffer.tag_table().lookup(CURRENT_MATCH_TAG) {
            let (start, end) = buffer.bounds();
            buffer.remove_tag(&tag, &start, &end);
        }
    }

    fn clear_protected_find(&self) {
        let state = self.protected_find_state();
        if let Some(previous) = state.cancel.take() {
            previous.store(true, Ordering::Relaxed);
        }
        state.generation.set(state.generation.get().wrapping_add(1));
        state.hits.borrow_mut().clear();
        state.current.set(None);
        self.clear_current_match_tag();
    }

    /// Whether a protected find request is running on its worker, or waits
    /// for an edit-entered session's source capture.
    pub(crate) fn protected_find_pending(&self) -> bool {
        self.protected_find_state().cancel.borrow().is_some()
            || (self.protected_source_capture_pending()
                && !self.search_bar().search_entry().text().is_empty())
    }

    /// A new source landed: rerun the bar's current query over it.
    pub(in crate::ui::editor_page) fn restart_protected_find_for_new_source(&self) {
        if !self.search_bar().search_entry().text().is_empty() {
            self.start_protected_find();
        }
    }

    /// The protected find results: the match count and the current index.
    pub(crate) fn protected_find_counts(&self) -> (usize, Option<usize>) {
        let state = self.protected_find_state();
        (state.hits.borrow().len(), state.current.get())
    }

    fn protected_find_state(&self) -> &ProtectedFindState {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        &self.imp().protected_find
    }
}

fn current_match_tag(buffer: &gtk4::TextBuffer) -> gtk4::TextTag {
    let table = buffer.tag_table();
    if let Some(tag) = table.lookup(CURRENT_MATCH_TAG) {
        return tag;
    }
    let tag = gtk4::TextTag::builder()
        .name(CURRENT_MATCH_TAG)
        .background("#f6d32d")
        .build();
    table.add(&tag);
    tag
}

/// Every match of `regex` in `text`, at most `FIND_RESULT_CAP`, as
/// (paragraph, column, length) in characters, with GTK's paragraph
/// delimiters (`\n`, `\r`, `\r\n`, U+2029). Columns are counted
/// incrementally, so many matches in one giant paragraph stay linear.
fn find_hits(text: &str, regex: &regex::Regex, cancel: &AtomicBool) -> Vec<FindHit> {
    let cap = usize::try_from(FIND_RESULT_CAP).unwrap_or(usize::MAX);
    let mut hits = Vec::new();
    let mut paragraph = 0u64;
    let mut next_break = next_paragraph_break(text, 0);
    let mut column_byte = 0usize;
    let mut column = 0u64;
    for found in regex.find_iter(text) {
        if hits.len() >= cap || cancel.load(Ordering::Relaxed) {
            break;
        }
        if found.is_empty() {
            continue;
        }
        while let Some(paragraph_break) = next_break
            && paragraph_break.next_start <= found.start()
        {
            paragraph += 1;
            column_byte = paragraph_break.next_start;
            column = 0;
            next_break = next_paragraph_break(text, paragraph_break.next_start);
        }
        column += count_chars(&text[column_byte..found.start()]);
        column_byte = found.start();
        hits.push(FindHit {
            paragraph,
            column,
            chars: count_chars(found.as_str()),
        });
    }
    hits
}

fn count_chars(text: &str) -> u64 {
    u64::try_from(text.chars().count()).unwrap_or(u64::MAX)
}
