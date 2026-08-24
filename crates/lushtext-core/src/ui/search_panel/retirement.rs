// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded retirement of detached workspace-search result generations.
//!
//! Clearing, replacing, or closing results detaches a whole generation of GTK
//! rows, navigation caches, and streamed match data. Releasing that in one turn
//! would stall the main loop, so this module hands each detached generation to a
//! bounded idle disposer that releases at most
//! `SEARCH_RETIREMENT_ROWS_PER_SLICE` references per turn under the pure
//! [`SearchRetirementSliceBudget`] policy.
//!
//! Control inversion: [`LushtextSearchPanel::retire_detached_results`] returns
//! immediately after arming one `glib::idle_add_local` callback. Work resumes in
//! that callback, once per GTK turn, until the queue drains. The final turn is
//! also where latest-query backpressure is released: a query deferred because
//! two generations were still retiring restarts from there.

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{self, glib};

use super::LushtextSearchPanel;
use super::item::SearchResultItem;
use super::policy::SearchRetirementSliceBudget;
use super::{SearchFileGroup, SearchMatchLocation};

/// Maximum GTK objects/cached rows released by one idle retirement turn.
const SEARCH_RETIREMENT_ROWS_PER_SLICE: usize = 250;
/// Maximum detached generations retained before latest-query backpressure applies.
pub(super) const MAX_SEARCH_RETIREMENT_GENERATIONS: usize = 2;

/// Actual detached ownership held before or after one bounded retirement turn.
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SearchRetirementOwnership {
    pub root_rows: usize,
    pub file_groups: usize,
    pub child_rows: usize,
    pub cached_match_rows: usize,
    pub cached_file_rows: usize,
    pub accepted_snapshot_refs: usize,
    pub streamed_matches: usize,
    pub accepted_matches: usize,
    pub positions: usize,
}

#[cfg(feature = "test-utils")]
impl SearchRetirementOwnership {
    #[must_use]
    pub fn total(self) -> usize {
        self.root_rows
            .saturating_add(self.file_groups)
            .saturating_add(self.child_rows)
            .saturating_add(self.cached_match_rows)
            .saturating_add(self.cached_file_rows)
            .saturating_add(self.accepted_snapshot_refs)
            .saturating_add(self.streamed_matches)
            .saturating_add(self.accepted_matches)
            .saturating_add(self.positions)
    }

    fn released_to(self, after: Self) -> Self {
        Self {
            root_rows: self.root_rows.saturating_sub(after.root_rows),
            file_groups: self.file_groups.saturating_sub(after.file_groups),
            child_rows: self.child_rows.saturating_sub(after.child_rows),
            cached_match_rows: self
                .cached_match_rows
                .saturating_sub(after.cached_match_rows),
            cached_file_rows: self.cached_file_rows.saturating_sub(after.cached_file_rows),
            accepted_snapshot_refs: self
                .accepted_snapshot_refs
                .saturating_sub(after.accepted_snapshot_refs),
            streamed_matches: self.streamed_matches.saturating_sub(after.streamed_matches),
            accepted_matches: self.accepted_matches.saturating_sub(after.accepted_matches),
            positions: self.positions.saturating_sub(after.positions),
        }
    }
}

/// Compact evidence from one actual GTK retirement turn.
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchRetirementSliceObservation {
    pub generation: u64,
    pub before: SearchRetirementOwnership,
    pub after: SearchRetirementOwnership,
    pub released: SearchRetirementOwnership,
    pub charged: usize,
    pub pending: bool,
    pub terminal_drain: bool,
}

/// One detached result generation whose GTK-owned references are no longer visible.
struct RetiredSearchGtkState {
    generation: u64,
    root_store: gtk4::gio::ListStore,
    file_groups: BTreeMap<PathBuf, SearchFileGroup>,
    match_rows: Vec<Option<gtk4::TreeListRow>>,
    file_rows: BTreeMap<PathBuf, gtk4::TreeListRow>,
    search_matches: Vec<crate::model::content_search::SearchMatch>,
    accepted_matches: Option<Arc<Vec<crate::model::content_search::SearchMatch>>>,
    accepted_match_rows: Vec<crate::model::content_search::SearchMatch>,
    match_positions: Vec<SearchMatchLocation>,
}

impl RetiredSearchGtkState {
    fn retire_slice(&mut self, limit: usize) -> usize {
        let mut budget = SearchRetirementSliceBudget::new(limit);
        while !budget.exhausted() && self.root_store.n_items() > 0 {
            let count = u32::try_from(
                budget.take(usize::try_from(self.root_store.n_items()).unwrap_or(usize::MAX)),
            )
            .unwrap_or(u32::MAX);
            let start = self.root_store.n_items().saturating_sub(count);
            self.root_store
                .splice(start, count, &[] as &[SearchResultItem]);
        }
        while !budget.exhausted() {
            let Some((path, group)) = self.file_groups.pop_first() else {
                break;
            };
            let count = u32::try_from(
                budget.take(usize::try_from(group.child_store.n_items()).unwrap_or(usize::MAX)),
            )
            .unwrap_or(u32::MAX);
            if count > 0 {
                let start = group.child_store.n_items().saturating_sub(count);
                group
                    .child_store
                    .splice(start, count, &[] as &[SearchResultItem]);
            }
            if group.child_store.n_items() > 0 || budget.exhausted() {
                self.file_groups.insert(path, group);
                break;
            }
            let group_charged = budget.take_one();
            debug_assert!(group_charged);
        }
        let match_row_count = budget.take(self.match_rows.len());
        self.match_rows
            .truncate(self.match_rows.len().saturating_sub(match_row_count));
        while !budget.exhausted() && !self.file_rows.is_empty() {
            let row_charged = budget.take_one();
            debug_assert!(row_charged);
            let removed = self.file_rows.pop_first();
            debug_assert!(removed.is_some());
        }
        let accepted_charged =
            !budget.exhausted() && self.accepted_matches.is_some() && budget.take_one();
        if accepted_charged {
            let accepted = self.accepted_matches.take();
            debug_assert!(accepted.is_some());
            let accepted = accepted.expect("accepted snapshot checked before retirement charge");
            if Arc::strong_count(&accepted) == 1 {
                self.accepted_match_rows =
                    Arc::try_unwrap(accepted).expect("unique accepted search snapshot");
            }
        }
        retire_vec_tail(&mut self.search_matches, &mut budget);
        retire_vec_tail(&mut self.accepted_match_rows, &mut budget);
        retire_vec_tail(&mut self.match_positions, &mut budget);
        budget.retired()
    }

    fn is_empty(&self) -> bool {
        self.root_store.n_items() == 0
            && self.file_groups.is_empty()
            && self.match_rows.is_empty()
            && self.file_rows.is_empty()
            && self.search_matches.is_empty()
            && self.accepted_matches.is_none()
            && self.accepted_match_rows.is_empty()
            && self.match_positions.is_empty()
    }

    #[cfg(feature = "test-utils")]
    fn ownership(&self) -> SearchRetirementOwnership {
        let accepted_snapshot_refs = usize::from(self.accepted_matches.is_some());
        let uniquely_owned_accepted = self.accepted_matches.as_ref().map_or(0, |accepted| {
            if Arc::strong_count(accepted) == 1 {
                accepted.len()
            } else {
                0
            }
        });
        SearchRetirementOwnership {
            root_rows: usize::try_from(self.root_store.n_items()).unwrap_or(usize::MAX),
            file_groups: self.file_groups.len(),
            child_rows: self.file_groups.values().fold(0usize, |total, group| {
                total.saturating_add(
                    usize::try_from(group.child_store.n_items()).unwrap_or(usize::MAX),
                )
            }),
            cached_match_rows: self.match_rows.len(),
            cached_file_rows: self.file_rows.len(),
            accepted_snapshot_refs,
            streamed_matches: self.search_matches.len(),
            accepted_matches: self
                .accepted_match_rows
                .len()
                .saturating_add(uniquely_owned_accepted),
            positions: self.match_positions.len(),
        }
    }
}

fn retire_vec_tail<T>(items: &mut Vec<T>, budget: &mut SearchRetirementSliceBudget) {
    let count = budget.take(items.len());
    items.truncate(items.len().saturating_sub(count));
}

/// Coalesced queue drained by one bounded idle callback.
pub(super) struct SearchRetirementSession {
    states: VecDeque<RetiredSearchGtkState>,
}

impl LushtextSearchPanel {
    /// Whether latest-query backpressure applies because detached generations
    /// are still being released.
    ///
    /// The third queue slot is reserved for the immediate close/clear escape
    /// path, so a new non-empty query defers at the second detached generation.
    pub(super) fn result_retirement_saturated(&self) -> bool {
        self.imp()
            .runtime
            .retirement
            .borrow()
            .as_ref()
            .is_some_and(|session| session.states.len() >= MAX_SEARCH_RETIREMENT_GENERATIONS)
    }

    /// Detach the visible result generation and hand it to the bounded disposer.
    ///
    /// Every reference the panel is about to stop showing — root rows, per-file
    /// child stores, navigation row caches, streamed matches, and the accepted
    /// snapshot — moves out of live state here, and a fresh empty model is
    /// installed before this returns, so the visible panel never observes a
    /// partially retired generation.
    pub(super) fn detach_visible_results(&self) {
        let imp = self.imp();
        let retirement_generation = imp.runtime.retirement_generation.get().wrapping_add(1);
        imp.runtime.retirement_generation.set(retirement_generation);
        let old_root = imp
            .runtime
            .root_store
            .replace(gtk4::gio::ListStore::new::<SearchResultItem>());
        let retired = RetiredSearchGtkState {
            generation: retirement_generation,
            root_store: old_root,
            file_groups: std::mem::take(&mut *imp.runtime.file_groups.borrow_mut()),
            match_rows: std::mem::take(&mut *imp.navigation.match_rows.borrow_mut()),
            file_rows: std::mem::take(&mut *imp.navigation.file_rows.borrow_mut()),
            search_matches: std::mem::take(&mut *imp.runtime.search_matches.borrow_mut()),
            accepted_matches: imp.runtime.accepted_matches.take(),
            accepted_match_rows: Vec::new(),
            match_positions: std::mem::take(&mut *imp.navigation.match_positions.borrow_mut()),
        };
        imp.install_results_model();
        self.retire_detached_results(retired);
    }

    /// Coalesce detached generations behind one bounded GTK idle disposer.
    fn retire_detached_results(&self, state: RetiredSearchGtkState) {
        let imp = self.imp();
        if !state.is_empty() {
            let mut retirement = imp.runtime.retirement.borrow_mut();
            retirement
                .get_or_insert_with(|| SearchRetirementSession {
                    states: VecDeque::new(),
                })
                .states
                .push_back(state);
            let retained = retirement
                .as_ref()
                .map_or(0, |session| session.states.len());
            debug_assert!(retained <= MAX_SEARCH_RETIREMENT_GENERATIONS + 1);
            imp.runtime.retirement_generations_high_water.set(
                imp.runtime
                    .retirement_generations_high_water
                    .get()
                    .max(retained),
            );
        }
        if imp.runtime.retirement.borrow().is_none() || imp.runtime.retirement_armed.replace(true) {
            return;
        }

        let panel_weak = self.downgrade();
        glib::idle_add_local(move || {
            let Some(panel) = panel_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let imp = panel.imp();
            let mut retirement = imp.runtime.retirement.borrow_mut();
            let Some(session) = retirement.as_mut() else {
                imp.runtime.retirement_armed.set(false);
                return glib::ControlFlow::Break;
            };
            let Some(state) = session.states.front_mut() else {
                retirement.take();
                imp.runtime.retirement_armed.set(false);
                panel.refresh_accessibility_state();
                return glib::ControlFlow::Break;
            };
            debug_assert!(state.generation <= imp.runtime.retirement_generation.get());
            #[cfg(feature = "test-utils")]
            let before = state.ownership();
            let retired_rows = state.retire_slice(SEARCH_RETIREMENT_ROWS_PER_SLICE);
            #[cfg(feature = "test-utils")]
            {
                let after = state.ownership();
                let released = before.released_to(after);
                debug_assert_eq!(released.total(), retired_rows);
                imp.runtime.retirement_observations.borrow_mut().push(
                    SearchRetirementSliceObservation {
                        generation: state.generation,
                        before,
                        after,
                        released,
                        charged: retired_rows,
                        pending: !state.is_empty(),
                        terminal_drain: state.is_empty(),
                    },
                );
            }
            imp.runtime.retirement_rows_per_slice_high_water.set(
                imp.runtime
                    .retirement_rows_per_slice_high_water
                    .get()
                    .max(retired_rows),
            );
            if state.is_empty() {
                session.states.pop_front();
            }
            let resume_spec = if session.states.len() < MAX_SEARCH_RETIREMENT_GENERATIONS {
                imp.runtime.deferred_search.take()
            } else {
                None
            };
            let control = if session.states.is_empty() {
                retirement.take();
                imp.runtime.retirement_armed.set(false);
                panel.refresh_accessibility_state();
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            };
            drop(retirement);
            if let Some(spec) = resume_spec {
                panel.start_search(&spec);
            }
            control
        });
    }

    /// Whether detached-generation GTK references still await bounded disposal.
    pub(super) fn result_retirement_pending(&self) -> bool {
        self.imp().runtime.retirement.borrow().is_some()
    }

    /// Detached generations still queued behind the bounded disposer.
    pub(super) fn result_retirement_backlog(&self) -> usize {
        self.imp()
            .runtime
            .retirement
            .borrow()
            .as_ref()
            .map_or(0, |session| session.states.len())
    }
}
