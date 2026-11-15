// SPDX-License-Identifier: GPL-3.0-or-later

//! `execution` role for the workspace tree workflow's directory collapse stage
//! order (`slice-large-directory-collapse`).
//!
//! # Role
//!
//! This is the **execution** coordination role for collapsing and expanding a
//! directory row. It is the only LushText caller of
//! `TreeListRow::set_expanded` under `ui/sidebar/` (`make
//! check-workflow-boundaries`), so every LushText route collapses and expands
//! through [`LushtextWorkspaceSection::collapse_directory`] and
//! [`LushtextWorkspaceSection::expand_directory`] (design D5).
//!
//! # Why a collapse is sliced
//!
//! GTK's `set_expanded(false)` removes every child row in one call, and in that
//! call destroys the row widgets bound to removed rows and creates fresh ones
//! for the rows that enter the realized band (ledger A118): about 40 ms with
//! 400 rows after a 256-entry directory, plus as much again to style the new
//! rows in the next frame. A directory whose subtree lists more than
//! `DIRECT_COLLAPSE_ROWS` rows is therefore drained first, one step per frame,
//! and collapsed directly once at most that many rows are left.
//!
//! # Stage order
//!
//! 1. **Intent** (the input dispatch, no row work, design D6): the expander
//!    gets [`COLLAPSING_CLASS`] (the arrow shows collapsed, A120), the row and
//!    its expander report `expanded = false`, the subtree's expansion intent
//!    is pruned, its scans and loads and queued refresh targets are cancelled,
//!    and selection and focus inside the subtree move to the directory row.
//!    A subtree of at most `DIRECT_COLLAPSE_ROWS` rows collapses directly
//!    here instead, until the input has removed one unit of rows that way.
//! 2. **Steps**: each step reads the subtree's expanded stores and GTK's held
//!    positions, asks the pure `policy::next_collapse_step` for one removal,
//!    and applies it: unbound chunks while `turn_continues` holds, and at most
//!    one bound unit, which one window-wide token admits once per frame
//!    across every section's collapse (design D4, D7).
//! 3. **Terminal**: the directory's direct `set_expanded(false)`, after which
//!    the row's own expansion hook retires the drained listing as before.
//!
//! # Resumption points and inversions
//!
//! - **The frame tick.** While the list is mapped, steps resume from a tick
//!   callback on the list view, in the frame's update phase, so the widgets a
//!   unit creates are styled in that frame.
//! - **The low-priority fallback.** While the list is not mapped, or a mapped
//!   list's frame clock has not ticked within `COLLAPSE_FRAME_STARVATION_MS`,
//!   steps resume from a `G_PRIORITY_LOW` timeout with the same bounds. The
//!   first tick after a starved step takes no unit, so two units never share
//!   one layout.
//! - **Inversion: re-expansion.** An expansion of a collapsing row, by any
//!   route, ends the collapse (`CollapseEnd::ReExpanded`) and refills the
//!   partly drained store through `populate_child_store`, the existing scan
//!   path. The row never left GTK's expanded state.
//! - **Inversion: removal.** A refresh splice, a model reset or disposal that
//!   removes the collapsing row ends its collapse (`CollapseEnd::Removed`);
//!   GTK removes what is left directly.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{gio, glib};

use super::super::file_tree_item::FileTreeItem;
use super::LushtextWorkspaceSection;
use crate::model::list_budget::turn_continues;
use crate::ui::accessibility;
use crate::ui::sidebar::policy::{
    COLLAPSE_FRAME_STARVATION_MS, CollapseFacts, CollapseStep, CollapseStore, DIRECT_COLLAPSE_ROWS,
    SIDEBAR_REALIZED_ROWS_PER_FRAME, collapse_band, next_collapse_step,
};

/// The expander class whose application CSS draws the checked arrow as
/// collapsed while the row is still GTK-expanded (ledger A120).
pub(crate) const COLLAPSING_CLASS: &str = "lushtext-collapsing";

/// How often the low-priority fallback source checks for an unmapped list or
/// a starved frame clock: one 120 Hz reference frame, rounded up to whole
/// milliseconds (`model::responsiveness_budget`).
///
/// Kind: interval; not a threshold, so not rounded to a power of two.
const COLLAPSE_FALLBACK_INTERVAL: Duration = Duration::from_millis(9);

/// The route that asked for a collapse (design D5), reported by the evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollapseOrigin {
    /// The expander arrow, clicked.
    Arrow,
    /// One of the expander's class key bindings, with focus on the expander
    /// (ledger A119).
    Keys,
    /// Row activation (Enter, double click).
    Activation,
    /// The folder header's collapse or toggle.
    Header,
    /// A failed workspace path reveal leaving the tree as it found it.
    Reveal,
    /// The reorder drag's defensive collapse of an inert model.
    Reorder,
}

/// Why a collapse ended, reported by the evidence (design D11).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollapseEnd {
    /// The directory collapsed.
    Completed,
    /// The row was expanded again first.
    ReExpanded,
    /// An ancestor's collapse took this one over.
    TakenOver,
    /// The row left the model first (a refresh splice, a reset, disposal).
    Removed,
}

/// One running collapse, as the evidence reports it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollapseEvidence {
    /// The collapsing directory.
    pub path: PathBuf,
    /// The route that started it.
    pub origin: CollapseOrigin,
    /// Steps it has taken.
    pub steps: u64,
    /// Rows of its subtree GTK listed at its last step.
    pub rows_left: u32,
    /// Of those, the rows GTK held a row object for.
    pub bound_rows_left: u32,
}

/// An expanded descendant directory of a collapsing row: its path and row.
///
/// Held strongly: `GtkTreeListModel` keeps no row object of its own for a
/// node no widget shows, so a weak reference to a row outside the realized
/// band dies at once, and its node's expansion with it would be unreachable.
type ExpandedDescendant = (PathBuf, gtk4::TreeListRow);

/// One queued collapse.
struct CollapseRecord {
    /// The collapsing row, held strongly for the reason `ExpandedDescendant`
    /// gives.
    row: gtk4::TreeListRow,
    path: PathBuf,
    origin: CollapseOrigin,
    /// The expanded descendant rows captured at the start, shallowest first.
    descendants: Vec<ExpandedDescendant>,
    steps: u64,
    rows_left: u32,
    bound_rows_left: u32,
}

/// The section's collapse state (design D7, D11).
#[derive(Default)]
pub(super) struct CollapseRuntimeState {
    /// Running collapses, in request order.
    queue: RefCell<Vec<CollapseRecord>>,
    /// Every path shown collapsed while GTK still lists it expanded: the
    /// queued directories and the descendants an ancestor took over. The row
    /// factory reads it on bind, so a rebound row keeps its projection.
    collapsing_paths: RefCell<HashSet<PathBuf>>,
    tick: RefCell<Option<gtk4::TickCallbackId>>,
    fallback: RefCell<Option<glib::SourceId>>,
    /// `g_get_monotonic_time()` of the last step source run.
    last_step_us: Cell<i64>,
    last_end: Cell<Option<CollapseEnd>>,
    steps_total: Cell<u64>,
    direct_total: Cell<u64>,
    starved_steps_total: Cell<u64>,
}

thread_local! {
    /// The frame counter, per frame clock, of the last frame a bound unit
    /// ran in: the window-wide one-unit-per-frame token (design D4). A
    /// starved step stores the next frame's counter, so the first tick after
    /// it takes no unit.
    static UNIT_FRAMES: RefCell<HashMap<usize, i64>> = RefCell::new(HashMap::new());
}

fn clock_key(clock: &gtk4::gdk::FrameClock) -> usize {
    clock.as_ptr() as usize
}

fn unit_free_in_frame(clock: &gtk4::gdk::FrameClock) -> bool {
    let frame = clock.frame_counter();
    UNIT_FRAMES.with(|frames| {
        frames
            .borrow()
            .get(&clock_key(clock))
            .is_none_or(|taken| *taken < frame)
    })
}

fn take_unit(clock: &gtk4::gdk::FrameClock, frame: i64) {
    UNIT_FRAMES.with(|frames| {
        let mut frames = frames.borrow_mut();
        // One entry per window that ever collapsed; drop the rest if a
        // long session accumulates windows.
        if frames.len() > 64 {
            frames.clear();
        }
        frames.insert(clock_key(clock), frame);
    });
}

/// What one step reads from GTK: the policy's facts plus the objects they
/// name, index-aligned with `stores`.
struct StepFacts {
    stores: Vec<CollapseStore>,
    store_objects: Vec<gio::ListStore>,
    rows: Vec<gtk4::TreeListRow>,
    band: Vec<u32>,
}

impl LushtextWorkspaceSection {
    /// Collapse the directory `row` by the route `origin` (design D5): the only
    /// entry point for a LushText collapse.
    pub(in crate::ui::sidebar) fn collapse_directory(
        &self,
        row: &gtk4::TreeListRow,
        origin: CollapseOrigin,
    ) {
        self.collapse_directories(std::slice::from_ref(row), origin);
    }

    /// Collapse every directory of `rows` from one input (the folder header):
    /// small subtrees directly until the input has removed one unit of rows
    /// that way, the rest sliced (design D7).
    pub(in crate::ui::sidebar) fn collapse_directories(
        &self,
        rows: &[gtk4::TreeListRow],
        origin: CollapseOrigin,
    ) {
        let mut direct_rows = 0u32;
        for row in rows {
            let Some(path) = directory_path(row) else {
                continue;
            };
            if !row.is_expanded() || self.path_is_collapsing(&path) {
                continue;
            }
            let Some((span, descendants)) = self.capture_subtree(row, &path) else {
                // The subtree's rows could not be located (a path shown twice by
                // overlapping folders): GTK's direct collapse, as before.
                self.move_selection_and_focus_to(row, u32::MAX);
                self.collapse_now(row);
                continue;
            };
            self.move_selection_and_focus_to(row, span);
            if span <= DIRECT_COLLAPSE_ROWS
                && direct_rows.saturating_add(span) <= SIDEBAR_REALIZED_ROWS_PER_FRAME
            {
                direct_rows += span;
                self.collapse_now(row);
                continue;
            }
            self.start_sliced_collapse(row, path, origin, descendants);
        }
        self.arm_collapse_source();
    }

    /// Expand the directory `row` (design D5): the only entry point for a
    /// LushText expansion. A collapsing row, or one inside a collapsing
    /// subtree, ends that collapse and refills its listing. Returns whether
    /// the row is expanded afterwards.
    pub(in crate::ui::sidebar) fn expand_directory(&self, row: &gtk4::TreeListRow) -> bool {
        if let Some(path) = directory_path(row)
            && self.path_is_collapsing(&path)
        {
            self.cancel_collapses_covering(&path);
        }
        // A row the collapse left GTK-expanded is expanded again by the
        // refill; a collapsed descendant inside the subtree (a reveal's next
        // ancestor) still needs its own expansion.
        if !row.is_expanded() && row.position() != gtk4::INVALID_LIST_POSITION {
            row.set_expanded(true);
        }
        row.is_expanded()
    }

    /// Whether `row` reads as expanded to the user: GTK-expanded and not
    /// collapsing.
    pub(in crate::ui::sidebar) fn row_shows_expanded(&self, row: &gtk4::TreeListRow) -> bool {
        row.is_expanded() && directory_path(row).is_none_or(|path| !self.path_is_collapsing(&path))
    }

    /// Whether `path` is collapsing or lies under a collapsing directory.
    pub(super) fn path_is_collapsing(&self, path: &Path) -> bool {
        let collapsing = self.imp().collapse_runtime.collapsing_paths.borrow();
        !collapsing.is_empty()
            && path
                .ancestors()
                .any(|ancestor| collapsing.contains(ancestor))
    }

    /// Re-apply the collapse projection to a freshly bound row (design D6):
    /// GTK's bind sets the expander's accessible state from the row, which is
    /// still expanded while it collapses.
    pub(super) fn apply_collapse_projection(
        &self,
        overlay: &gtk4::Overlay,
        expander: &gtk4::TreeExpander,
        row: &gtk4::TreeListRow,
    ) {
        let collapsing = directory_path(row).is_some_and(|path| {
            self.imp()
                .collapse_runtime
                .collapsing_paths
                .borrow()
                .contains(&path)
        });
        if collapsing {
            expander.add_css_class(COLLAPSING_CLASS);
            accessibility::set_expanded(overlay, Some(false));
            accessibility::set_expanded(expander, Some(false));
        } else {
            expander.remove_css_class(COLLAPSING_CLASS);
        }
    }

    /// Whether a collapse still runs (`workspace-refresh-complete`, design D7).
    pub(crate) fn collapse_blocks_readiness(&self) -> bool {
        !self.imp().collapse_runtime.queue.borrow().is_empty()
    }

    /// The collapse queue and totals, for the evidence surface. Reads only
    /// `Cell`/`RefCell` state.
    pub(crate) fn collapse_evidence(
        &self,
    ) -> (Vec<CollapseEvidence>, Option<CollapseEnd>, u64, u64, u64) {
        let runtime = &self.imp().collapse_runtime;
        let collapses = runtime
            .queue
            .borrow()
            .iter()
            .map(|record| CollapseEvidence {
                path: record.path.clone(),
                origin: record.origin,
                steps: record.steps,
                rows_left: record.rows_left,
                bound_rows_left: record.bound_rows_left,
            })
            .collect();
        (
            collapses,
            runtime.last_end.get(),
            runtime.steps_total.get(),
            runtime.direct_total.get(),
            runtime.starved_steps_total.get(),
        )
    }

    /// End every collapse: the model is being reset or the section disposed.
    pub(super) fn cancel_all_collapses(&self) {
        let runtime = &self.imp().collapse_runtime;
        let ended = !runtime.queue.borrow().is_empty();
        runtime.queue.borrow_mut().clear();
        runtime.collapsing_paths.borrow_mut().clear();
        if ended {
            runtime.last_end.set(Some(CollapseEnd::Removed));
        }
        self.stop_collapse_sources();
    }

    /// Start or stop the step sources for the list's mapped state.
    pub(super) fn arm_collapse_source(&self) {
        let runtime = &self.imp().collapse_runtime;
        if runtime.queue.borrow().is_empty() {
            self.stop_collapse_sources();
            return;
        }
        if runtime.last_step_us.get() == 0 {
            runtime.last_step_us.set(glib::monotonic_time());
        }
        let list = &*self.imp().file_tree_view;
        let ticks = frame_ticks_allowed();
        if ticks && list.is_mapped() && runtime.tick.borrow().is_none() {
            let section = self.downgrade();
            let id = list.add_tick_callback(move |_, clock| {
                let Some(section) = section.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                section
                    .imp()
                    .collapse_runtime
                    .last_step_us
                    .set(glib::monotonic_time());
                let unit = unit_free_in_frame(clock);
                if section.run_collapse_step(unit, false) {
                    take_unit(clock, clock.frame_counter());
                }
                if section.imp().collapse_runtime.queue.borrow().is_empty() {
                    section.imp().collapse_runtime.tick.borrow_mut().take();
                    section.stop_collapse_sources();
                    return glib::ControlFlow::Break;
                }
                glib::ControlFlow::Continue
            });
            runtime.tick.replace(Some(id));
        }
        if runtime.fallback.borrow().is_none() {
            let section = self.downgrade();
            let id = glib::timeout_add_local_full(
                COLLAPSE_FALLBACK_INTERVAL,
                glib::Priority::LOW,
                move || {
                    let Some(section) = section.upgrade() else {
                        return glib::ControlFlow::Break;
                    };
                    section.run_fallback_step();
                    if section.imp().collapse_runtime.queue.borrow().is_empty() {
                        section.imp().collapse_runtime.fallback.borrow_mut().take();
                        section.stop_collapse_sources();
                        return glib::ControlFlow::Break;
                    }
                    glib::ControlFlow::Continue
                },
            );
            runtime.fallback.replace(Some(id));
        }
    }

    fn stop_collapse_sources(&self) {
        let runtime = &self.imp().collapse_runtime;
        if let Some(tick) = runtime.tick.borrow_mut().take() {
            tick.remove();
        }
        if let Some(source) = runtime.fallback.borrow_mut().take() {
            source.remove();
        }
        runtime.last_step_us.set(0);
    }

    /// The fallback: steps while nothing draws the list, or once a mapped
    /// list's frame clock has starved (design D4).
    fn run_fallback_step(&self) {
        let runtime = &self.imp().collapse_runtime;
        let list = &*self.imp().file_tree_view;
        let now = glib::monotonic_time();
        let ticks = frame_ticks_allowed();
        if list.is_mapped() && ticks {
            if runtime.tick.borrow().is_none() {
                // Mapped again since the last check: pace from frames.
                self.arm_collapse_source();
            }
            let idle_us = now.saturating_sub(runtime.last_step_us.get());
            let starvation_us =
                i64::try_from(COLLAPSE_FRAME_STARVATION_MS * 1_000).unwrap_or(i64::MAX);
            if idle_us < starvation_us {
                return;
            }
            runtime.last_step_us.set(now);
            let clock = list.frame_clock();
            let unit = clock.as_ref().is_none_or(unit_free_in_frame);
            if self.run_collapse_step(unit, true)
                && let Some(clock) = clock
            {
                take_unit(&clock, clock.frame_counter().saturating_add(1));
            }
            return;
        }
        runtime.last_step_us.set(now);
        let starved = list.is_mapped();
        self.run_collapse_step(true, starved);
    }

    /// One step over the queue: unbound chunks while the turn lasts, and at
    /// most one bound unit if `unit_allowed`. Returns whether a unit ran.
    fn run_collapse_step(&self, unit_allowed: bool, starved: bool) -> bool {
        if steps_held() {
            return false;
        }
        let started = glib::monotonic_time();
        let mut chunks = 0usize;
        let mut unit_taken = false;
        let mut index = 0;
        let runtime = &self.imp().collapse_runtime;
        loop {
            // Take the record out while it steps: removals emit
            // `items-changed`, whose handlers read the queue.
            let record = {
                let mut queue = runtime.queue.borrow_mut();
                if index >= queue.len() {
                    break;
                }
                queue.remove(index)
            };
            let mut record = record;
            let mut ended = None;
            loop {
                let elapsed =
                    u64::try_from(glib::monotonic_time().saturating_sub(started)).unwrap_or(0);
                if !turn_continues(elapsed, chunks) {
                    break;
                }
                let Some(facts) = self.step_facts(&record) else {
                    ended = Some(CollapseEnd::Removed);
                    break;
                };
                let step = next_collapse_step(&CollapseFacts {
                    stores: &facts.stores,
                    band: &facts.band,
                });
                record.rows_left = facts.stores.first().map_or(0, |store| store.span);
                record.bound_rows_left = held_in(&facts);
                if step.takes_unit() {
                    if unit_taken || !unit_allowed {
                        break;
                    }
                    unit_taken = true;
                }
                record.steps += 1;
                runtime.steps_total.set(runtime.steps_total.get() + 1);
                if starved {
                    runtime
                        .starved_steps_total
                        .set(runtime.starved_steps_total.get() + 1);
                }
                chunks += 1;
                match step {
                    CollapseStep::RemoveUnbound {
                        store,
                        start,
                        count,
                    }
                    | CollapseStep::RemoveBound {
                        store,
                        start,
                        count,
                    } => {
                        super::scan_execution::drain_child_store(
                            self,
                            &facts.store_objects[store],
                            start,
                            count,
                        );
                    }
                    // GTK's collapse emits `notify::expanded`, whose hook
                    // retires the listing only when nothing else holds its
                    // store (`retire_child_store`): drop the step's
                    // references first, or the listing's mirror and path
                    // entries outlive it.
                    CollapseStep::CollapseDescendant(store) => {
                        let row = facts.rows[store].clone();
                        drop(facts);
                        row.set_expanded(false);
                    }
                    CollapseStep::CollapseRow => {
                        let row = facts.rows[0].clone();
                        drop(facts);
                        row.set_expanded(false);
                        record.rows_left = 0;
                        record.bound_rows_left = 0;
                        ended = Some(CollapseEnd::Completed);
                        break;
                    }
                }
            }
            if let Some(end) = ended {
                self.finish_collapse(&record, end);
            } else {
                runtime.queue.borrow_mut().insert(index, record);
                index += 1;
            }
        }
        unit_taken
    }

    /// Read one step's facts, or `None` once the row has left the model or
    /// been collapsed by something else.
    fn step_facts(&self, record: &CollapseRecord) -> Option<StepFacts> {
        let root = record.row.clone();
        if !self.row_is_live(&root) || !root.is_expanded() {
            return None;
        }
        let root_store = root.children().and_downcast::<gio::ListStore>()?;
        let mut paths = vec![record.path.clone()];
        let mut rows = vec![root.clone()];
        let mut store_objects = vec![root_store];
        for (path, row) in &record.descendants {
            if !row.is_expanded() || row.position() == gtk4::INVALID_LIST_POSITION {
                continue;
            }
            let Some(store) = row.children().and_downcast::<gio::ListStore>() else {
                continue;
            };
            paths.push(path.clone());
            rows.push(row.clone());
            store_objects.push(store);
        }
        let root_depth = root.depth();
        let mut stores: Vec<CollapseStore> = rows
            .iter()
            .zip(&store_objects)
            .map(|(row, store)| CollapseStore {
                depth: row.depth().saturating_sub(root_depth),
                first_row: row.position().saturating_add(1),
                length: store.n_items(),
                span: store.n_items(),
            })
            .collect();
        // Spans bottom-up: each store adds its descendants' spans to its
        // immediate expanded ancestor's.
        let mut order: Vec<usize> = (1..stores.len()).collect();
        order.sort_by_key(|index| std::cmp::Reverse(stores[*index].depth));
        for index in order {
            let parent = (0..stores.len())
                .filter(|candidate| {
                    *candidate != index
                        && paths[index].starts_with(&paths[*candidate])
                        && paths[index] != paths[*candidate]
                })
                .max_by_key(|candidate| stores[*candidate].depth)
                .unwrap_or(0);
            stores[parent].span = stores[parent].span.saturating_add(stores[index].span);
        }
        let mut band = Vec::with_capacity(256);
        super::for_each_realized_file_row_overlay(self, |overlay| {
            if let Some(row) = overlay
                .child()
                .and_downcast::<gtk4::TreeExpander>()
                .and_then(|expander| expander.list_row())
            {
                let position = row.position();
                if position != gtk4::INVALID_LIST_POSITION {
                    band.push(position);
                }
            }
        });
        if let Some(selection) = self
            .imp()
            .file_tree_view
            .model()
            .and_downcast::<gtk4::SingleSelection>()
        {
            let selected = selection.selected();
            if selected != gtk4::INVALID_LIST_POSITION {
                band.push(selected);
            }
        }
        Some(StepFacts {
            stores,
            store_objects,
            rows,
            band: collapse_band(band),
        })
    }

    /// Whether `row` is still the row the model lists at its position.
    fn row_is_live(&self, row: &gtk4::TreeListRow) -> bool {
        let position = row.position();
        if position == gtk4::INVALID_LIST_POSITION {
            return false;
        }
        self.imp()
            .tree_model
            .try_borrow()
            .ok()
            .and_then(|model| model.as_ref().and_then(|model| model.item(position)))
            .and_downcast::<gtk4::TreeListRow>()
            .is_some_and(|live| live == *row)
    }

    /// The subtree's flattened rows and its expanded descendant rows,
    /// shallowest first, or `None` if a descendant row cannot be located.
    fn capture_subtree(
        &self,
        row: &gtk4::TreeListRow,
        path: &Path,
    ) -> Option<(u32, Vec<ExpandedDescendant>)> {
        let root_store = row.children().and_downcast::<gio::ListStore>()?;
        let mut span = root_store.n_items();
        let mut expanded: Vec<PathBuf> = self
            .imp()
            .retained_child_stores
            .borrow()
            .keys()
            .filter(|candidate| candidate.starts_with(path) && candidate.as_path() != path)
            .cloned()
            .collect();
        expanded.sort_by_key(|candidate| candidate.components().count());
        let mut found: Vec<(PathBuf, gtk4::TreeListRow)> = vec![(path.to_path_buf(), row.clone())];
        for candidate in expanded {
            let Some((parent_path, parent_row)) = found
                .iter()
                .filter(|(found_path, _)| candidate.starts_with(found_path))
                .max_by_key(|(found_path, _)| found_path.components().count())
                .cloned()
            else {
                continue;
            };
            // Only an immediate child of an expanded row is listed.
            if candidate.parent() != Some(parent_path.as_path()) {
                continue;
            }
            let location = self
                .imp()
                .item_locations
                .borrow()
                .get(&candidate)
                .cloned()?;
            if location.parent_dir.as_deref() != Some(parent_path.as_path()) {
                return None;
            }
            let child = parent_row.child_row(u32::try_from(location.index).ok()?)?;
            if directory_path(&child).as_deref() != Some(candidate.as_path()) {
                return None;
            }
            if !child.is_expanded() {
                continue;
            }
            let store = child.children().and_downcast::<gio::ListStore>()?;
            span = span.saturating_add(store.n_items());
            found.push((candidate, child));
        }
        let descendants = found.into_iter().skip(1).collect();
        Some((span, descendants))
    }

    /// Move the selection and the list's focus row out of the subtree of `span`
    /// rows under `row` onto `row` itself (design D7).
    fn move_selection_and_focus_to(&self, row: &gtk4::TreeListRow, span: u32) {
        let position = row.position();
        if position == gtk4::INVALID_LIST_POSITION {
            return;
        }
        let end = position.saturating_add(1).saturating_add(span);
        let inside = |candidate: u32| candidate > position && candidate < end;
        let list = &*self.imp().file_tree_view;
        if let Some(selection) = list.model().and_downcast::<gtk4::SingleSelection>() {
            let selected = selection.selected();
            if selected != gtk4::INVALID_LIST_POSITION && inside(selected) {
                selection.set_selected(position);
            }
        }
        let focused = list
            .focus_child()
            .and_then(|item| item.first_child())
            .and_downcast::<gtk4::Overlay>()
            .and_then(|overlay| overlay.child().and_downcast::<gtk4::TreeExpander>())
            .and_then(|expander| expander.list_row())
            .map(|focused| focused.position());
        if focused.is_some_and(inside) {
            list.scroll_to(position, gtk4::ListScrollFlags::FOCUS, None);
        }
    }

    /// GTK's direct collapse of `row`. Like a sliced collapse, it first stops
    /// every scan, load and queued refresh under the directory: a collapsed
    /// directory's retained store would otherwise keep receiving load batches
    /// nobody sees (design D7).
    fn collapse_now(&self, row: &gtk4::TreeListRow) {
        if let Some(path) = directory_path(row) {
            self.stop_subtree_work(&path);
        }
        row.set_expanded(false);
        let runtime = &self.imp().collapse_runtime;
        runtime.direct_total.set(runtime.direct_total.get() + 1);
    }

    /// Stage 1 of a sliced collapse (design D6, D7).
    fn start_sliced_collapse(
        &self,
        row: &gtk4::TreeListRow,
        path: PathBuf,
        origin: CollapseOrigin,
        descendants: Vec<ExpandedDescendant>,
    ) {
        let imp = self.imp();
        // A collapsing descendant is taken over: its rows are drained as part
        // of this subtree, and it keeps its collapsed projection.
        let taken_over: Vec<usize> = imp
            .collapse_runtime
            .queue
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, record)| record.path.starts_with(&path))
            .map(|(index, _)| index)
            .collect();
        for index in taken_over.into_iter().rev() {
            imp.collapse_runtime.queue.borrow_mut().remove(index);
            imp.collapse_runtime
                .last_end
                .set(Some(CollapseEnd::TakenOver));
        }
        imp.collapse_runtime
            .collapsing_paths
            .borrow_mut()
            .insert(path.clone());
        self.project_collapsing(row, true);
        // Intent: the subtree is collapsed from now on.
        imp.expanded_paths
            .borrow_mut()
            .retain(|candidate| !candidate.starts_with(&path));
        // No load, scan or refresh may append to a store being drained.
        self.stop_subtree_work(&path);
        imp.collapse_runtime
            .queue
            .borrow_mut()
            .push(CollapseRecord {
                row: row.clone(),
                path,
                origin,
                descendants,
                steps: 0,
                rows_left: 0,
                bound_rows_left: 0,
            });
    }

    /// Cancel every scan, load turn and queued refresh target in the subtree
    /// of `path` (the directory's own pending refresh stays: it refreshes the
    /// directory's row, not its children).
    fn stop_subtree_work(&self, path: &Path) {
        let imp = self.imp();
        let store_keys: Vec<usize> = imp
            .child_store_paths
            .borrow()
            .iter()
            .filter(|(_, store_path)| store_path.starts_with(path))
            .map(|(key, _)| *key)
            .collect();
        for key in store_keys {
            super::scan_execution::cancel_child_store_runtime(self, key);
        }
        imp.refresh_runtime
            .pending_paths
            .borrow_mut()
            .retain(|pending| !pending.starts_with(path) || pending == path);
        super::scan_execution::sync_child_scan_busy_state(self);
    }

    /// End one collapse with `end`.
    fn finish_collapse(&self, record: &CollapseRecord, end: CollapseEnd) {
        let runtime = &self.imp().collapse_runtime;
        runtime
            .collapsing_paths
            .borrow_mut()
            .retain(|candidate| !candidate.starts_with(&record.path));
        runtime.last_end.set(Some(end));
        self.project_collapsing(&record.row, false);
    }

    /// End every collapse whose subtree holds `path` because `path` is being
    /// expanded again, and refill what each one drained (design D7).
    fn cancel_collapses_covering(&self, path: &Path) {
        let imp = self.imp();
        let covering: Vec<CollapseRecord> = {
            let mut queue = imp.collapse_runtime.queue.borrow_mut();
            let mut kept = Vec::with_capacity(queue.len());
            let mut covering = Vec::new();
            for record in queue.drain(..) {
                if path.starts_with(&record.path) {
                    covering.push(record);
                } else {
                    kept.push(record);
                }
            }
            *queue = kept;
            covering
        };
        if covering.is_empty() {
            // A descendant an ancestor took over, whose ancestor is gone.
            imp.collapse_runtime
                .collapsing_paths
                .borrow_mut()
                .retain(|candidate| !path.starts_with(candidate));
            return;
        }
        for record in covering {
            self.finish_collapse(&record, CollapseEnd::ReExpanded);
            let root = record.row.clone();
            if !root.is_expanded() {
                continue;
            }
            let mut refill = vec![(record.path.clone(), root)];
            for (descendant, row) in &record.descendants {
                if row.is_expanded() && self.row_is_live(row) {
                    refill.push((descendant.clone(), row.clone()));
                }
            }
            for (path, row) in refill {
                imp.expanded_paths.borrow_mut().insert(path.clone());
                if let Some(store) = row.children().and_downcast::<gio::ListStore>() {
                    super::scan_execution::populate_child_store(self, &path, &store);
                }
            }
        }
        self.arm_collapse_source();
    }

    /// Show `row` collapsed (or restore GTK's state) on its realized widget.
    fn project_collapsing(&self, row: &gtk4::TreeListRow, collapsing: bool) {
        super::for_each_realized_file_row_overlay(self, |overlay| {
            let Some(expander) = overlay.child().and_downcast::<gtk4::TreeExpander>() else {
                return;
            };
            if expander.list_row().as_ref() != Some(row) {
                return;
            }
            let expanded = row.is_expanded() && !collapsing;
            if collapsing {
                expander.add_css_class(COLLAPSING_CLASS);
            } else {
                expander.remove_css_class(COLLAPSING_CLASS);
            }
            accessibility::set_expanded(&overlay, Some(expanded));
            accessibility::set_expanded(&expander, Some(expanded));
            if let Some(item) = row.item().and_downcast::<FileTreeItem>() {
                accessibility::set_description(
                    &overlay,
                    &super::row_accessibility::file_tree_row_description(&item, row, self),
                );
            }
        });
    }
}

/// What one of `GtkTreeExpander`'s class key bindings asks for
/// (`gtktreeexpander.c:654-690`, ledger A119).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DisclosureKey {
    Expand,
    Collapse,
    Toggle,
}

/// The expander's class key bindings, as GTK installs them, with their
/// left-to-right intent; `true` marks a direction binding, whose intent
/// right-to-left text reverses.
const DISCLOSURE_BINDINGS: [(gtk4::gdk::Key, gtk4::gdk::ModifierType, DisclosureKey, bool); 18] = {
    use gtk4::gdk::{Key, ModifierType};
    const NONE: ModifierType = ModifierType::empty();
    const SHIFT: ModifierType = ModifierType::SHIFT_MASK;
    const CTRL: ModifierType = ModifierType::CONTROL_MASK;
    const CTRL_SHIFT: ModifierType = CTRL.union(SHIFT);
    [
        (Key::plus, NONE, DisclosureKey::Expand, false),
        (Key::KP_Add, NONE, DisclosureKey::Expand, false),
        (Key::asterisk, NONE, DisclosureKey::Expand, false),
        (Key::KP_Multiply, NONE, DisclosureKey::Expand, false),
        (Key::minus, NONE, DisclosureKey::Collapse, false),
        (Key::KP_Subtract, NONE, DisclosureKey::Collapse, false),
        (Key::slash, NONE, DisclosureKey::Collapse, false),
        (Key::KP_Divide, NONE, DisclosureKey::Collapse, false),
        (Key::Right, SHIFT, DisclosureKey::Expand, true),
        (Key::KP_Right, SHIFT, DisclosureKey::Expand, true),
        (Key::Right, CTRL_SHIFT, DisclosureKey::Expand, true),
        (Key::KP_Right, CTRL_SHIFT, DisclosureKey::Expand, true),
        (Key::Left, SHIFT, DisclosureKey::Collapse, true),
        (Key::KP_Left, SHIFT, DisclosureKey::Collapse, true),
        (Key::Left, CTRL_SHIFT, DisclosureKey::Collapse, true),
        (Key::KP_Left, CTRL_SHIFT, DisclosureKey::Collapse, true),
        (Key::space, CTRL, DisclosureKey::Toggle, false),
        (Key::KP_Space, CTRL, DisclosureKey::Toggle, false),
    ]
};

impl LushtextWorkspaceSection {
    /// Install the key route of design D5: a capture-phase key controller on
    /// the list view's slice bin that takes exactly the expander's class key
    /// bindings
    /// GTK would honour for the focused expander, and only where they would
    /// collapse a directory or end a running collapse. Everything else
    /// propagates, so GTK's expansion and every other key are `main`'s. The
    /// bindings reach the expander only once Tab has moved the focus from the
    /// row onto it; the focus the list gives a row stays on the row widget,
    /// because the row's child is an overlay (ledger A119). The controller
    /// sits on the bin, the list view's parent, so capture reaches it before
    /// the list view's own key controllers and the list view keeps the
    /// sidebar's Space peek as its first key controller.
    pub(super) fn setup_disclosure_key_route(&self) {
        let controller = gtk4::EventControllerKey::new();
        controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let section_weak = self.downgrade();
        controller.connect_key_pressed(move |controller, _, _, _| {
            let Some(section) = section_weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let Some(event) = controller
                .current_event()
                .and_then(|event| event.downcast::<gtk4::gdk::KeyEvent>().ok())
            else {
                return glib::Propagation::Proceed;
            };
            let Some(expander) = section.focused_expander() else {
                return glib::Propagation::Proceed;
            };
            let Some(intent) = disclosure_intent(&event, &expander) else {
                return glib::Propagation::Proceed;
            };
            if section.take_disclosure_key(&expander, intent) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.imp().file_tree_slice.add_controller(controller);
    }

    /// The expander holding the window's keyboard focus inside this
    /// section's list, if any: the expander itself or an ancestor of the
    /// focused widget, as GTK delivers the class bindings.
    fn focused_expander(&self) -> Option<gtk4::TreeExpander> {
        let window = self.root().and_downcast::<gtk4::Window>()?;
        let list = self
            .imp()
            .file_tree_view
            .upcast_ref::<gtk4::Widget>()
            .clone();
        let mut current = gtk4::prelude::GtkWindowExt::focus(&window);
        while let Some(widget) = current {
            if widget == list {
                return None;
            }
            if let Some(expander) = widget.downcast_ref::<gtk4::TreeExpander>() {
                return Some(expander.clone());
            }
            current = widget.parent();
        }
        None
    }

    /// Act on `intent` for `expander`'s row where the collapse entry points
    /// own it; return false to let GTK's binding run.
    fn take_disclosure_key(&self, expander: &gtk4::TreeExpander, intent: DisclosureKey) -> bool {
        let Some(row) = expander.list_row() else {
            return false;
        };
        let Some(path) = directory_path(&row) else {
            return false;
        };
        let collapsing = self.path_is_collapsing(&path);
        let shown_expanded = row.is_expanded() && !collapsing;
        match intent {
            DisclosureKey::Collapse | DisclosureKey::Toggle if shown_expanded => {
                self.collapse_directory(&row, CollapseOrigin::Keys);
                true
            }
            // GTK's collapse would only finish a collapse already shown.
            DisclosureKey::Collapse if collapsing => true,
            DisclosureKey::Expand | DisclosureKey::Toggle if collapsing => {
                if self.expand_directory(&row) && intent == DisclosureKey::Toggle {
                    let _ = expander.activate_action("listitem.scroll-to", None);
                }
                true
            }
            _ => false,
        }
    }
}

/// The disclosure intent `event` asks of `expander`, matching the class
/// bindings as GTK's own triggers do (`gdk_key_event_matches`).
fn disclosure_intent(
    event: &gtk4::gdk::KeyEvent,
    expander: &gtk4::TreeExpander,
) -> Option<DisclosureKey> {
    let rtl = expander.direction() == gtk4::TextDirection::Rtl;
    DISCLOSURE_BINDINGS
        .iter()
        .find(|(key, modifiers, _, _)| {
            event.matches(*key, *modifiers) == gtk4::gdk::KeyMatch::Exact
        })
        .map(|(_, _, intent, directional)| match (*directional, rtl) {
            (true, true) => match intent {
                DisclosureKey::Expand => DisclosureKey::Collapse,
                DisclosureKey::Collapse => DisclosureKey::Expand,
                DisclosureKey::Toggle => DisclosureKey::Toggle,
            },
            _ => *intent,
        })
}

/// Whether collapses may pace from the frame clock (a test may stand in for a
/// starved clock, `test_policy`).
fn frame_ticks_allowed() -> bool {
    #[cfg(feature = "test-utils")]
    {
        crate::ui::sidebar::test_policy::collapse_pacing()
            == crate::ui::sidebar::test_policy::CollapsePacingForTest::FrameClock
    }
    #[cfg(not(feature = "test-utils"))]
    {
        true
    }
}

/// Whether a test holds every collapse where it is (`test_policy`).
fn steps_held() -> bool {
    #[cfg(feature = "test-utils")]
    {
        crate::ui::sidebar::test_policy::collapse_pacing()
            == crate::ui::sidebar::test_policy::CollapsePacingForTest::Hold
    }
    #[cfg(not(feature = "test-utils"))]
    {
        false
    }
}

/// The rows of the subtree GTK holds a row object for.
fn held_in(facts: &StepFacts) -> u32 {
    let Some(root) = facts.stores.first() else {
        return 0;
    };
    let end = root.first_row.saturating_add(root.span);
    let held = facts
        .band
        .iter()
        .filter(|position| **position >= root.first_row && **position < end)
        .count();
    u32::try_from(held).unwrap_or(u32::MAX)
}

/// The path of the directory `row` shows, if it shows a real directory.
fn directory_path(row: &gtk4::TreeListRow) -> Option<PathBuf> {
    row.item()
        .and_downcast::<FileTreeItem>()
        .filter(|item| item.is_dir() && !item.is_placeholder())
        .and_then(|item| item.path())
}
