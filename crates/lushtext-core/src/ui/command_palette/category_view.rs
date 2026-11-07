// SPDX-License-Identifier: GPL-3.0-or-later

//! The command palette's category view (`drill-down-palette-categories`):
//! entering it from a "More" row or the automation action, leaving it one
//! level per Escape, Backspace on an empty query, the back button or a mode
//! change, and projecting a category ranking onto the lazy
//! `category_model::PaletteCategoryModel` with its title, notices and
//! announcements.
//!
//! # Role
//!
//! This module carries **no role**. It is a **called presentation surface** of
//! `WFR-COMMAND-PALETTE`: it projects the query flight's category branch onto
//! the palette's widgets, beside `imp`, which it extends. Every decision it
//! renders comes from `policy` (the wording, the notice, the selection on
//! leaving); the flight itself is `query_execution`'s. Named in that
//! workflow's matrix row.

use std::sync::Arc;

use gtk4::prelude::*;

use crate::model::palette::PaletteCategory;
use crate::services::palette::CategoryRanking;
use crate::ui::accessibility::{self, AnnouncementLane};

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::subclass::prelude::ObjectSubclassExt;

use super::category_model::CategorySources;
use super::imp::LushtextCommandPalette;
use super::policy;
use super::query_execution::PaletteQueryRequest;

/// What one Escape the palette consumed did (`drill-down-palette-categories`
/// D5). The window latches every one, so its own Escape controller does not
/// act on the same key press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteEscape {
    /// The category view was left; the palette stays open.
    LeftCategory,
    /// The grouped view was shown: the palette asks to be closed.
    CloseRequested,
}

impl super::LushtextCommandPalette {
    /// Register a callback for every Escape the palette consumes: leaving a
    /// category view, or a request to close the palette.
    pub fn connect_escape_consumed<F: Fn(PaletteEscape) + 'static>(&self, f: F) {
        *self.imp().escape_callback.borrow_mut() = Some(Box::new(f));
    }

    /// The category view shown, `None` for the grouped results.
    #[must_use]
    pub fn category(&self) -> Option<PaletteCategory> {
        self.imp().category.get()
    }

    /// Open `category`'s view through the same path as its "More" row; a
    /// category the current mode does not show is refused (`false`).
    #[must_use]
    pub fn show_category(&self, category: PaletteCategory) -> bool {
        let imp = self.imp();
        if imp.category.get() == Some(category) {
            return true;
        }
        // Refuse before leaving: a refused id changes nothing, also while
        // another category view is shown (review finding R1).
        if !category.belongs_to(imp.mode.get()) {
            return false;
        }
        let _ = imp.leave_category_view();
        imp.leaving_category.set(None);
        let shown = imp.enter_category(category);
        imp.search_entry.grab_focus();
        shown
    }

    /// Return to the grouped results with the query kept (Escape, Backspace
    /// on an empty query, the back button); `false` when none was shown.
    #[must_use]
    pub fn leave_category(&self) -> bool {
        self.imp().leave_category()
    }

    /// Record how the note source's byte ceilings cut it, for the note
    /// category views' notice: rows held, rows found, and whether found is a
    /// floor; `None` when nothing was cut.
    pub(crate) fn set_note_source_cut(&self, cut: Option<(usize, usize, bool)>) {
        self.imp().note_source_cut.set(cut);
    }
}

impl LushtextCommandPalette {
    /// Escape in the query entry (`stop-search`, A97): leave the category
    /// view, else ask the window to close the palette. Either way the window
    /// is told, so it latches this Escape and its own controller does not act
    /// on the same key press (`drill-down-palette-categories` D5).
    pub(super) fn dismiss_one_level(&self) {
        let outcome = if self.leave_category() {
            PaletteEscape::LeftCategory
        } else {
            PaletteEscape::CloseRequested
        };
        if let Some(ref callback) = *self.escape_callback.borrow() {
            callback(outcome);
        }
    }

    /// Open `category`'s view: every result it matches, ranked as the grouped
    /// view ranks them. Refused (`false`) for a category the mode does not
    /// show.
    pub(super) fn enter_category(&self, category: PaletteCategory) -> bool {
        if !category.belongs_to(self.mode.get()) {
            return false;
        }
        let label = self.category_label(category);
        self.category.set(Some(category));
        self.leaving_category.set(None);
        self.category_published_query.replace(None);
        self.cancel_category_clear();
        self.category_model.clear();
        self.category_matching.set(0);
        self.category_limited.set(false);
        self.announced_notice.replace(None);
        let mut tracker = self.backspace.get();
        tracker.reset(self.search_entry.text().is_empty());
        self.backspace.set(tracker);

        self.results_scroll.set_visible(false);
        self.no_results_label.set_visible(false);
        self.category_bar.set_visible(true);
        self.category_scroll.set_visible(true);
        self.category_title_label.set_label(&label);
        self.category_notice_label.set_visible(false);
        accessibility::set_labelled_description(
            &*self.category_view,
            &policy::category_list_label(&label),
            "Every matching result in this category",
        );
        accessibility::announce_with_lane(
            &*self.category_view,
            &policy::category_entry_announcement(&label),
            AnnouncementLane::StatusUpdate,
        );
        self.category_entry_announcements
            .set(self.category_entry_announcements.get().saturating_add(1));
        self.restart_query();
        true
    }

    /// Return to the grouped view with the query kept, and start a grouped
    /// flight; the selection returns to the category's "More" row when the
    /// rows arrive. `false` when no category view is shown.
    pub(super) fn leave_category(&self) -> bool {
        if !self.leave_category_view() {
            return false;
        }
        self.restart_query();
        true
    }

    /// The widget half of leaving: no flight (a mode change restarts its own).
    pub(super) fn leave_category_view(&self) -> bool {
        let Some(category) = self.category.take() else {
            return false;
        };
        self.leaving_category.set(Some(category));
        self.category_matching.set(0);
        self.category_limited.set(false);
        self.announced_notice.replace(None);
        self.category_bar.set_visible(false);
        self.category_scroll.set_visible(false);
        self.results_scroll.set_visible(true);
        self.schedule_category_clear();
        self.refresh_accessibility_state();
        true
    }

    /// Clear the hidden category model, releasing its source snapshots, from
    /// a low-priority idle after the leave's own dispatch: removing the rows
    /// unbinds the about 205 the list realized (A1), which measured 4.7–11.2 ms
    /// inside the Escape dispatch (`drill-down-palette-categories` M6). The
    /// list is hidden first, so nothing it still holds is shown; entering a
    /// category before the idle runs clears at once instead.
    fn schedule_category_clear(&self) {
        self.cancel_category_clear();
        let obj = self.obj().downgrade();
        let source = glib::idle_add_local_full(glib::Priority::LOW, move || {
            if let Some(obj) = obj.upgrade() {
                let imp = obj.imp();
                imp.category_clear_source.borrow_mut().take();
                if imp.category.get().is_none() {
                    imp.category_model.clear();
                }
            }
            glib::ControlFlow::Break
        });
        self.category_clear_source.replace(Some(source));
    }

    /// Drop a pending deferred clear (entering again, disposal).
    pub(super) fn cancel_category_clear(&self) {
        if let Some(source) = self.category_clear_source.borrow_mut().take() {
            source.remove();
        }
    }

    /// The visible label of `category` in the current mode.
    fn category_label(&self, category: PaletteCategory) -> String {
        category
            .label(self.mode.get(), &self.workspace_group_label.borrow())
            .to_string()
    }

    /// Query stage 4's widget mutation for a category view: hand the ranked
    /// ordinals to the lazy model (its cost does not grow with them), set the
    /// title and notice, and select the first row.
    pub(super) fn publish_category_ranking(
        &self,
        ranking: CategoryRanking,
        request: &PaletteQueryRequest,
    ) {
        let Some(category) = self.category.get() else {
            return;
        };
        let listed = ranking.ordinals.len();
        self.category_matching.set(ranking.matching);
        self.category_limited.set(ranking.limited);
        // Read the selection before publishing: a shrinking publish removes
        // the selected row and `GtkSingleSelection` forgets it.
        let selected_before = self
            .category_selection()
            .map(|selection| selection.selected())
            .filter(|position| *position != gtk4::INVALID_LIST_POSITION);
        self.category_model.publish(
            ranking.ordinals,
            category,
            CategorySources {
                index: Arc::clone(&request.index),
                open_tabs: Arc::clone(&request.open_tabs),
                note_entries: Arc::clone(&request.note_entries),
            },
        );
        let label = self.category_label(category);
        self.category_title_label
            .set_label(&policy::category_title(&label, ranking.matching));
        self.refresh_category_notice(category, listed, ranking.matching);
        self.no_results_label
            .set_visible(policy::no_results_visible(
                listed > 0,
                request.query.is_empty(),
            ));
        let same_query = self.category_published_query.borrow().as_deref() == Some(&*request.query);
        self.category_published_query
            .replace(Some(request.query.to_string()));
        if let Some(selection) = self.category_selection() {
            let selected = selected_before;
            let length = u32::try_from(listed).unwrap_or(u32::MAX);
            if let Some((position, scroll)) =
                policy::category_publish_selection(same_query, selected, length)
            {
                selection.set_selected(position);
                if scroll {
                    self.category_view
                        .scroll_to(position, gtk4::ListScrollFlags::NONE, None);
                }
            }
        }
        self.refresh_accessibility_state();
    }

    /// Show the budget notice, else a note category's note-source notice;
    /// announce it once per change.
    fn refresh_category_notice(&self, category: PaletteCategory, listed: usize, matching: usize) {
        let notice = policy::category_budget_notice(listed, matching).or_else(|| {
            matches!(category, PaletteCategory::Note(_))
                .then(|| self.note_source_cut.get())
                .flatten()
                .map(|(held, found, lower)| policy::note_source_notice(held, found, lower))
        });
        self.category_notice_label.set_visible(notice.is_some());
        self.category_notice_label
            .set_label(notice.as_deref().unwrap_or_default());
        if notice.is_some() && *self.announced_notice.borrow() != notice {
            if let Some(text) = notice.as_deref() {
                accessibility::announce_with_lane(
                    &*self.category_notice_label,
                    text,
                    AnnouncementLane::StatusUpdate,
                );
            }
            self.category_notice_announcements
                .set(self.category_notice_announcements.get().saturating_add(1));
        }
        self.announced_notice.replace(notice);
    }
}
