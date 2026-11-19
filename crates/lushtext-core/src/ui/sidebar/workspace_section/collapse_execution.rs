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
//! `DIRECT_COLLAPSE_ROWS` rows is therefore drained first, in steps of at most
//! `COLLAPSE_STEP_ROWS` bound rows between frames and at most
//! `SIDEBAR_REALIZED_ROWS_PER_FRAME` per frame interval, and collapsed
//! directly once at most that many rows are left.
//!
//! # Stage order
//!
//! 1. **Intent** (the input dispatch, no row work, design D6): the expander
//!    gets [`COLLAPSING_CLASS`] (the arrow shows collapsed, A120), the row and
//!    its expander report `expanded = false`, the subtree's expansion intent
//!    is pruned, its scans and loads and queued refresh targets are cancelled,
//!    and selection and focus inside the subtree move to the directory row.
//!    Every realized row of the subtree is hidden (design D22, below), and
//!    each of its directories' path caches is detached whole, one map
//!    removal each, its entries left to the listing retirement (design D24,
//!    `scan_execution/path_cache_detach.rs`), so no step touches the caches;
//!    a subtree another workspace folder lies inside keeps the per-step
//!    splice instead.
//!    A subtree of at most `DIRECT_COLLAPSE_ROWS` rows collapses directly
//!    here instead, until the input has removed one step of rows
//!    (`COLLAPSE_STEP_ROWS`) that way.
//! 2. **Steps**: each step reads the subtree's expanded stores and GTK's held
//!    positions, asks the pure `policy::next_collapse_step` for one removal,
//!    and applies it: unbound chunks while `turn_continues` holds, and bound
//!    rows within one window-wide token's budget per frame interval, across
//!    every section's collapse (design D4, D7, D25).
//! 3. **Terminal**: the directory's direct `set_expanded(false)`, after which
//!    the row's own expansion hook retires the drained listing as before.
//!
//! # Membership and hidden rows (design D22)
//!
//! Which rows are inside a collapsing subtree is decided by tree ancestry
//! against the collapsing rows, held strongly for the collapse's lifetime,
//! never by path: with overlapping workspace folders a directory is listed
//! twice, and the occurrence that is not collapsing keeps its projection, its
//! visibility and its loads. The arrow and accessible projection (R6.5), the
//! hiding below and `stop_subtree_work` all read that membership; the
//! path-keyed refresh and scan paths still key on the collapsing paths.
//!
//! From the input dispatch until any end of the collapse, the **row widget**
//! (the list's child, the row overlay's parent) of each realized subtree row
//! has opacity 0, `can-target` false and the accessible `hidden` state, so it
//! draws nothing, takes no pointer input and AT-SPI does not present it; none
//! of the three changes the row's measure or allocation (ledger A123). GTK
//! recycles row widgets without resetting them, so a bind projects the state
//! both ways. A row widget the list is creating is not in the list at its bind
//! and is still floating (touching it from Rust would finalize it), and
//! `gtk_widget_map` writes `hidden` false after its `map` emission: the end of
//! each bound step, and a one-shot `G_PRIORITY_HIGH_IDLE` re-apply armed by a
//! created row's bind or a hidden row's `map`, walk the band (about 200 rows,
//! A1) and write the state again. No pass runs on the frame clock.
//!
//! # Resumption points and inversions
//!
//! - **The step source (design D25).** While the list is mapped, every bound
//!   step runs from a one-shot `G_PRIORITY_HIGH_IDLE` source, between frames,
//!   one step per dispatch: above the frame clock and below input, so it runs
//!   before the next frame even when that frame is due (ledger A125), and the
//!   frame then only lays out the rows the step brought in (A124). It is armed
//!   by the input dispatch, by each frame's `after-paint` on the list's frame
//!   clock (one handler per section, connected while a collapse is pending on
//!   a mapped list), by the between-frames idle when the next step is bound,
//!   and by its own dispatch after D23's chunk or while the interval has rows
//!   left. Its first dispatch after the input takes one unbound chunk (D23).
//!   A frame interval removes at most `SIDEBAR_REALIZED_ROWS_PER_FRAME` bound
//!   rows (the token reads the frame counter, which equals the counter of the
//!   frame just painted there).
//! - **The frame tick.** While the list is mapped and a collapse is pending, a
//!   tick callback stays registered, taking no step: it keeps the clock
//!   updating every frame, so every frame emits `after-paint`, and it
//!   refreshes the starvation clock.
//! - **The between-frames idle (design D19).** Unbound chunks leave from a
//!   `G_PRIORITY_DEFAULT_IDLE` source, below the frame clock, while the turn
//!   lasts.
//! - **The low-priority fallback.** While the list is not mapped, or a mapped
//!   list's frame clock has not ticked within `COLLAPSE_FRAME_STARVATION_MS`,
//!   steps resume from a `G_PRIORITY_LOW` timeout with the same bounds. A
//!   starved unit spends its frame interval, so two units never share one
//!   layout.
//! - **Inversion: re-expansion.** An expansion of a collapsing row, by any
//!   route, ends the collapse (`CollapseEnd::ReExpanded`) and refills the
//!   partly drained store through `populate_child_store`, the existing scan
//!   path. The row never left GTK's expanded state. Each refilled
//!   directory's detached cache is adopted again from its mirror in slices
//!   first, and the refill's scan result waits for that (design D24).
//! - **Inversion: removal.** A refresh splice, a model reset or disposal that
//!   removes the collapsing row ends its collapse (`CollapseEnd::Removed`);
//!   GTK removes what is left directly.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{gio, glib};

mod held_keys;

use super::super::file_tree_item::FileTreeItem;
use super::LushtextWorkspaceSection;
use crate::model::list_budget::turn_continues;
use crate::ui::accessibility;
use crate::ui::sidebar::policy::{
    COLLAPSE_FRAME_STARVATION_MS, CollapseFacts, CollapseStep, CollapseStore, FrameUnitToken,
    bound_step_rows, collapse_band, collapse_removal_is_destroy_only, interval_rows_left,
    next_collapse_step, record_interval_rows,
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
    /// Collapsing descendants this collapse took over, with their routes: a
    /// re-expansion of this directory collapses them again, since the user
    /// collapsed them first (design R6.4).
    taken_over: Vec<(PathBuf, CollapseOrigin)>,
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
    /// The rows of those directories, held strongly, so a child row's
    /// `TreeListRow::parent` returns these objects: membership in a
    /// collapsing subtree is decided by tree ancestry against them, never by
    /// path (design D22, R6.5), so the second occurrence of a directory under
    /// an overlapping workspace folder is not inside. Unlike the queue, it
    /// keeps a record's row while that record's step runs.
    collapsing_rows: RefCell<Vec<gtk4::TreeListRow>>,
    tick: RefCell<Option<gtk4::TickCallbackId>>,
    fallback: RefCell<Option<glib::SourceId>>,
    /// The `G_PRIORITY_DEFAULT_IDLE` source removing unbound rows between
    /// frames (design D19), while one is armed.
    between: RefCell<Option<glib::SourceId>>,
    /// `g_get_monotonic_time()` of the last step source run.
    last_step_us: Cell<i64>,
    last_end: Cell<Option<CollapseEnd>>,
    steps_total: Cell<u64>,
    direct_total: Cell<u64>,
    starved_steps_total: Cell<u64>,
    /// Steps the between-frames idle ran (design D19, task 10.3).
    between_steps_total: Cell<u64>,
    /// The longest `run_collapse_step` call since the queue last went from
    /// empty to running, in microseconds of `glib::monotonic_time` around
    /// the call (never read from GTK): the step's own time against
    /// `B_INTERACT_US`, apart from the frame that carries it (design D2,
    /// task 10.1).
    step_us_max: Cell<u64>,
    /// The one-shot `G_PRIORITY_HIGH_IDLE` step source (design D25), while
    /// armed. Its callback takes the id first, so nothing removes a source
    /// that is dispatching.
    step_source: RefCell<Option<glib::SourceId>>,
    /// The section's `after-paint` handler on the list's frame clock (design
    /// D25): at most one, connected while a collapse is pending on a mapped
    /// list, disconnected when the queue empties, the list unmaps, the
    /// section is disposed, and with the tick.
    after_paint: RefCell<Option<(glib::WeakRef<gtk4::gdk::FrameClock>, glib::SignalHandlerId)>>,
    /// Whether the next step-source dispatch may take D23's one unbound
    /// chunk: set when a sliced collapse starts.
    first_chunk_pending: Cell<bool>,
    /// `after-paint` emissions the section's handler ran.
    after_paint_runs: Cell<u64>,
    /// The longest step-source dispatch (the whole callback) or
    /// between-frames turn since the queue last started running, in
    /// microseconds (design R14: what the 6 ms margin and `B_INTERACT_US`
    /// read).
    step_dispatch_us_max: Cell<u64>,
    /// The step-source dispatches and between-frames turns since the queue
    /// last started running, at most `STEP_DISPATCH_LOG_ROWS`.
    step_dispatches: RefCell<Vec<StepDispatch>>,
    /// Row widgets the factory created (`setup`), ever.
    row_setups: Cell<u64>,
    /// The row widgets of the collapsing subtrees' realized rows that D22
    /// hid, each with its `map` handler (design D22, ledger A123).
    hidden_rows: RefCell<Vec<(glib::WeakRef<gtk4::Widget>, glib::SignalHandlerId)>>,
    /// The one-shot `G_PRIORITY_HIGH_IDLE` re-apply of D22's hiding, while
    /// armed: a created row's bind (its row widget is not yet in the list,
    /// A123) and a hidden row's `map` (GTK writes `hidden` false after the
    /// `map` emission) arm it.
    hidden_reapply: RefCell<Option<glib::SourceId>>,
    /// Passes over the band that projected D22's hiding, ever.
    hidden_syncs: Cell<u64>,
    /// Of those, the passes the re-apply source ran.
    hidden_reapply_runs: Cell<u64>,
    /// The navigation and row keys held while a collapse runs (design D22,
    /// task 10.14).
    held_keys: held_keys::HeldKeys,
}

/// Step dispatches the evidence keeps per collapse run: more than the
/// longest measured collapse's step-source dispatches and between-frames
/// turns (about 130 at 400:131,072).
///
/// Kind: cap; an evidence log, rounded down.
const STEP_DISPATCH_LOG_ROWS: usize = 1024;

/// What one step-source dispatch or between-frames turn did (design D25,
/// R14).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StepDispatchKind {
    /// Removed bound rows of a store.
    Bound,
    /// Collapsed a drained descendant directly.
    Descendant,
    /// Collapsed the directory: the terminal step.
    Terminal,
    /// Removed one unbound chunk (D23's, from the step source).
    Chunk,
    /// A between-frames turn: unbound chunks while the turn lasted.
    Between,
    /// Took no step (the interval was spent, or the next step was unbound).
    Idle,
}

/// One step-source dispatch or between-frames turn, as the evidence reports
/// it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StepDispatch {
    /// `glib::monotonic_time()` at its start, microseconds.
    pub start_mono_us: i64,
    /// The whole callback's time, microseconds.
    pub took_us: u64,
    /// What it did.
    pub kind: StepDispatchKind,
    /// Rows it removed.
    pub rows: u32,
    /// Row widgets the factory created inside it.
    pub rows_created: u32,
    /// The list's frame counter it read (-1 with no clock): the interval it
    /// charged.
    pub frame_counter: i64,
}

/// The collapse queue and its totals, as the evidence reports them.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CollapseTotals {
    /// Running collapses in request order.
    pub collapses: Vec<CollapseEvidence>,
    /// Why the most recent collapse ended.
    pub last_end: Option<CollapseEnd>,
    /// Collapse steps taken.
    pub steps_total: u64,
    /// Collapses GTK's direct call made whole.
    pub direct_total: u64,
    /// Steps the low-priority fallback ran.
    pub starved_steps_total: u64,
    /// Steps the between-frames idle ran: unbound chunks removed outside
    /// any frame (design D19).
    pub between_steps_total: u64,
    /// The longest step since the queue last started running, in
    /// microseconds.
    pub step_us_max: u64,
    /// The longest step-source dispatch or between-frames turn since the
    /// queue last started running, in microseconds.
    pub step_dispatch_us_max: u64,
    /// Those dispatches and turns, in order.
    pub step_dispatches: Vec<StepDispatch>,
    /// Whether the section's `after-paint` handler is connected.
    pub after_paint_connected: bool,
    /// `after-paint` emissions its handler ran, ever.
    pub after_paint_runs: u64,
    /// Row widgets the factory created, ever.
    pub row_setups: u64,
    /// Row widgets hidden now (design D22).
    pub hidden_rows: u32,
    /// Passes over the band that projected the hiding, ever.
    pub hidden_syncs: u64,
    /// Of those, the passes the re-apply source ran.
    pub hidden_reapply_runs: u64,
    /// Keys waiting now behind a collapse: held, or put back and not yet
    /// delivered again (design D22, task 10.14).
    pub held_keys_waiting: u32,
    /// Keys held, ever.
    pub keys_held_total: u64,
    /// Keys put back on GDK's queue, ever.
    pub keys_replayed_total: u64,
    /// Held keys dropped because the focus had left the list, ever.
    pub keys_dropped_total: u64,
}

thread_local! {
    /// The window-wide one-unit-per-frame token (design D4), one entry per
    /// live frame clock, keyed by a weak reference so a closed window's entry
    /// goes with its clock (design R6.3).
    static UNIT_TOKENS: RefCell<Vec<(glib::WeakRef<gtk4::gdk::FrameClock>, FrameUnitToken)>> =
        const { RefCell::new(Vec::new()) };
}

/// Read and update `clock`'s token through `update`, dropping dead clocks'.
fn with_unit_token<T>(
    clock: &gtk4::gdk::FrameClock,
    update: impl FnOnce(FrameUnitToken) -> (T, FrameUnitToken),
) -> T {
    UNIT_TOKENS.with(|tokens| {
        let mut tokens = tokens.borrow_mut();
        tokens.retain(|(weak, _)| weak.upgrade().is_some());
        let index = tokens
            .iter()
            .position(|(weak, _)| weak.upgrade().as_ref() == Some(clock));
        let token = index.map_or_else(FrameUnitToken::default, |index| tokens[index].1);
        let (result, token) = update(token);
        match index {
            Some(index) => tokens[index].1 = token,
            None => tokens.push((clock.downgrade(), token)),
        }
        result
    })
}

/// Bound rows the interval after `clock`'s last painted frame may still
/// remove (`policy::interval_rows_left`, design D25).
fn clock_rows_left(clock: &gtk4::gdk::FrameClock, budget: u32) -> u32 {
    let frame = clock.frame_counter();
    with_unit_token(clock, |token| {
        (interval_rows_left(token, frame, budget), token)
    })
}

/// Charge `rows` bound rows to the interval after `clock`'s last painted
/// frame.
fn charge_clock_rows(clock: &gtk4::gdk::FrameClock, rows: u32) {
    let frame = clock.frame_counter();
    with_unit_token(clock, |token| {
        ((), record_interval_rows(token, frame, rows))
    });
}

/// Which source runs a step, and so which steps it may take (design D4,
/// D19).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StepSource {
    /// The step source (design D25): one step, either D23's one unbound
    /// chunk (`first_chunk`) or bound rows within the interval's budget.
    Step { first_chunk: bool },
    /// The idle between frames: unbound chunks only, while the turn lasts.
    Between,
    /// The low-priority fallback (unmapped list, or a mapped list's clock
    /// starved): unbound chunks and at most one bound unit, which spends the
    /// frame interval.
    Fallback { starved: bool },
}

/// What one turn of a step source did.
#[derive(Clone, Copy, Debug, Default)]
struct StepOutcome {
    /// The bound step it ran: its kind and rows.
    bound: Option<(StepDispatchKind, u32)>,
    /// Unbound chunks it removed, and their rows.
    chunks: u32,
    chunk_rows: u32,
    /// Whether it stopped at a bound step it could not take here.
    stopped_at_bound: bool,
}

/// What one step reads from GTK: the policy's facts plus the objects they
/// name, index-aligned with `stores`.
struct StepFacts {
    stores: Vec<CollapseStore>,
    store_objects: Vec<gio::ListStore>,
    rows: Vec<gtk4::TreeListRow>,
    band: Vec<u32>,
    /// Flattened rows after the last realized row (design D18).
    rows_after: u32,
    /// Flattened rows before the first realized row (design D18).
    rows_before: u32,
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
        let runtime = &self.imp().collapse_runtime;
        if runtime.queue.borrow().is_empty() {
            runtime.step_us_max.set(0);
            runtime.step_dispatch_us_max.set(0);
            runtime.step_dispatches.borrow_mut().clear();
        }
        for row in rows {
            let Some(path) = directory_path(row) else {
                continue;
            };
            if !row.is_expanded() || self.row_is_collapsing(row) {
                continue;
            }
            let Some((span, descendants)) = self.capture_subtree(row, &path) else {
                // The subtree's rows could not be located (a path shown twice by
                // overlapping folders): GTK's direct collapse, as before,
                // leaving selection and focus where they are (design R6.10).
                self.collapse_now(row);
                continue;
            };
            self.move_selection_and_focus_to(row, span);
            // At most one step of rows collapses directly in the input
            // dispatch, the bound a step-source dispatch keeps (design R13's
            // S2); the rest are paced.
            if span <= crate::ui::sidebar::policy::slc_attribution_direct_rows()
                && direct_rows.saturating_add(span)
                    <= crate::ui::sidebar::policy::slc_attribution_step_rows()
            {
                direct_rows += span;
                self.collapse_now(row);
                continue;
            }
            self.start_sliced_collapse(row, path, origin, descendants);
            runtime.first_chunk_pending.set(true);
        }
        // D22: the subtree's realized rows draw nothing and take no pointer
        // from the input dispatch on (G3: about 0.3 ms for the band, A123).
        self.sync_hidden_rows();
        self.arm_collapse_source();
    }

    /// Expand the directory `row` (design D5): the only entry point for a
    /// LushText expansion. A collapsing row, or one inside a collapsing
    /// subtree, ends that collapse and refills its listing. Returns whether
    /// the row is expanded afterwards.
    pub(in crate::ui::sidebar) fn expand_directory(&self, row: &gtk4::TreeListRow) -> bool {
        if let Some(path) = directory_path(row)
            && self.row_is_collapsing(row)
        {
            self.cancel_collapses_covering(&path);
        }
        // A row the collapse left GTK-expanded is expanded again by the
        // refill; a collapsed descendant inside the subtree (a reveal's next
        // ancestor) still needs its own expansion.
        if !row.is_expanded() && row.position() != gtk4::INVALID_LIST_POSITION {
            row.set_expanded(true);
            self.record_entry_point_transition(row);
        }
        row.is_expanded()
    }

    /// Whether `row` reads as expanded to the user: GTK-expanded and not
    /// collapsing.
    pub(in crate::ui::sidebar) fn row_shows_expanded(&self, row: &gtk4::TreeListRow) -> bool {
        row.is_expanded() && (directory_path(row).is_none() || !self.row_is_collapsing(row))
    }

    /// Whether `row` lies strictly inside a collapsing subtree: its
    /// disclosure routes do nothing (design R6.5).
    pub(in crate::ui::sidebar) fn row_inside_collapsing_subtree(
        &self,
        row: &gtk4::TreeListRow,
    ) -> bool {
        self.row_in_collapsing_subtree(row)
    }

    /// Collapse `row` again, from an idle, after GTK expanded it inside a
    /// collapsing subtree (design R6.7): its store holds at most a loading
    /// row, and collapsing inside the `notify::expanded` that reported the
    /// expansion would re-enter the tree list model.
    pub(super) fn undo_expansion_inside_collapse(&self, row: &gtk4::TreeListRow) {
        let section = self.downgrade();
        let row = row.downgrade();
        glib::idle_add_local_once(move || {
            let (Some(section), Some(row)) = (section.upgrade(), row.upgrade()) else {
                return;
            };
            if row.is_expanded() && section.row_inside_collapsing_subtree(&row) {
                if let Some(path) = directory_path(&row) {
                    section.stop_subtree_work(&row, &path, &[]);
                }
                row.set_expanded(false);
                section.refresh_workspace_watch_row(&row);
            }
        });
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
        // A collapsing directory and every expanded directory inside its
        // subtree show collapsed (design R6.5).
        let collapsing =
            row.is_expanded() && directory_path(row).is_some() && self.row_is_collapsing(row);
        if collapsing {
            expander.add_css_class(COLLAPSING_CLASS);
            accessibility::set_expanded(overlay, Some(false));
            accessibility::set_expanded(expander, Some(false));
        } else {
            expander.remove_css_class(COLLAPSING_CLASS);
        }
    }

    /// Whether a collapse still runs, or keys held behind one still wait
    /// (`workspace-refresh-complete`, design D7, D22).
    pub(crate) fn collapse_blocks_readiness(&self) -> bool {
        let runtime = &self.imp().collapse_runtime;
        !runtime.queue.borrow().is_empty() || runtime.held_keys.any_waiting()
    }

    /// The collapse queue and totals, for the evidence surface. Reads only
    /// `Cell`/`RefCell` state.
    pub(crate) fn collapse_evidence(&self) -> CollapseTotals {
        let runtime = &self.imp().collapse_runtime;
        let keys = runtime.held_keys.totals();
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
        CollapseTotals {
            collapses,
            last_end: runtime.last_end.get(),
            steps_total: runtime.steps_total.get(),
            direct_total: runtime.direct_total.get(),
            starved_steps_total: runtime.starved_steps_total.get(),
            between_steps_total: runtime.between_steps_total.get(),
            step_us_max: runtime.step_us_max.get(),
            step_dispatch_us_max: runtime.step_dispatch_us_max.get(),
            step_dispatches: runtime.step_dispatches.borrow().clone(),
            after_paint_connected: runtime.after_paint.borrow().is_some(),
            after_paint_runs: runtime.after_paint_runs.get(),
            row_setups: runtime.row_setups.get(),
            hidden_rows: u32::try_from(
                runtime
                    .hidden_rows
                    .borrow()
                    .iter()
                    .filter(|(row, _)| row.upgrade().is_some())
                    .count(),
            )
            .unwrap_or(u32::MAX),
            hidden_syncs: runtime.hidden_syncs.get(),
            hidden_reapply_runs: runtime.hidden_reapply_runs.get(),
            held_keys_waiting: keys.waiting,
            keys_held_total: keys.held_total,
            keys_replayed_total: keys.replayed_total,
            keys_dropped_total: keys.dropped_total,
        }
    }

    /// Count one row widget the factory created (design R13: the step tail is
    /// GTK's creation inside a removal; the evidence reports it per step).
    pub(super) fn count_row_setup(&self) {
        let runtime = &self.imp().collapse_runtime;
        runtime
            .row_setups
            .set(runtime.row_setups.get().saturating_add(1));
    }

    /// End every collapse: the model is being reset or the section disposed.
    pub(super) fn cancel_all_collapses(&self) {
        let runtime = &self.imp().collapse_runtime;
        let ended = !runtime.queue.borrow().is_empty();
        runtime.queue.borrow_mut().clear();
        runtime.collapsing_paths.borrow_mut().clear();
        runtime.collapsing_rows.borrow_mut().clear();
        if ended {
            runtime.last_end.set(Some(CollapseEnd::Removed));
        }
        self.stop_collapse_sources();
        self.clear_hidden_rows();
        // A model reset ends the collapse: held keys act on the new rows.
        self.arm_held_key_replay();
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
        if frame_ticks_allowed() && list.is_mapped() {
            if runtime.tick.borrow().is_none() {
                // The frame requester and starvation clock (design D25): it
                // takes no step.
                let section = self.downgrade();
                let id = list.add_tick_callback(move |_, _| {
                    let Some(section) = section.upgrade() else {
                        return glib::ControlFlow::Break;
                    };
                    let runtime = &section.imp().collapse_runtime;
                    runtime.last_step_us.set(glib::monotonic_time());
                    if runtime.queue.borrow().is_empty() {
                        runtime.tick.borrow_mut().take();
                        section.stop_collapse_sources();
                        return glib::ControlFlow::Break;
                    }
                    glib::ControlFlow::Continue
                });
                runtime.tick.replace(Some(id));
            }
            self.connect_after_paint();
            self.arm_step_source();
        } else {
            // No frame draws the list: nothing may arm a step from frames.
            self.disconnect_after_paint();
        }
        self.arm_between_frames_source();
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

    /// Connect the section's one `after-paint` handler to the list's current
    /// frame clock (design D25), replacing one on a clock the list no longer
    /// has. `unmap` disconnects it (through `arm_collapse_source`), which also
    /// covers an unrealize, since a widget is unmapped before it unrealizes;
    /// `map` connects it again to the clock the list has then.
    fn connect_after_paint(&self) {
        let runtime = &self.imp().collapse_runtime;
        let Some(clock) = self.imp().file_tree_view.frame_clock() else {
            return;
        };
        if runtime
            .after_paint
            .borrow()
            .as_ref()
            .is_some_and(|(connected, _)| connected.upgrade().as_ref() == Some(&clock))
        {
            return;
        }
        self.disconnect_after_paint();
        let section = self.downgrade();
        let id = clock.connect_after_paint(move |_| {
            if let Some(section) = section.upgrade() {
                let runtime = &section.imp().collapse_runtime;
                runtime
                    .after_paint_runs
                    .set(runtime.after_paint_runs.get().saturating_add(1));
                section.arm_step_source();
            }
        });
        runtime.after_paint.replace(Some((clock.downgrade(), id)));
    }

    fn disconnect_after_paint(&self) {
        let connected = self.imp().collapse_runtime.after_paint.borrow_mut().take();
        if let Some((clock, id)) = connected
            && let Some(clock) = clock.upgrade()
        {
            clock.disconnect(id);
        }
    }

    /// Arm the one-shot step source (design D25) while a collapse is pending
    /// on a mapped list that paces from frames; at most one is armed.
    fn arm_step_source(&self) {
        let runtime = &self.imp().collapse_runtime;
        if runtime.queue.borrow().is_empty()
            || runtime.step_source.borrow().is_some()
            || !frame_ticks_allowed()
            || !self.imp().file_tree_view.is_mapped()
        {
            return;
        }
        let section = self.downgrade();
        let id = glib::idle_add_local_full(glib::Priority::HIGH_IDLE, move || {
            if let Some(section) = section.upgrade() {
                section
                    .imp()
                    .collapse_runtime
                    .step_source
                    .borrow_mut()
                    .take();
                section.run_step_source_dispatch();
            }
            glib::ControlFlow::Break
        });
        runtime.step_source.replace(Some(id));
    }

    /// One step-source dispatch (design D25): one step, timed as a whole, and
    /// the source armed again after D23's chunk or while the interval has
    /// bound rows left.
    fn run_step_source_dispatch(&self) {
        // A list unmapped since the source was armed paces from the
        // fallback; `map` arms the source again.
        if steps_held() || !frame_ticks_allowed() || !self.imp().file_tree_view.is_mapped() {
            return;
        }
        let runtime = &self.imp().collapse_runtime;
        let started = glib::monotonic_time();
        runtime.last_step_us.set(started);
        let setups_before = runtime.row_setups.get();
        let clock = self.imp().file_tree_view.frame_clock();
        let frame_counter = clock
            .as_ref()
            .map_or(-1, gtk4::gdk::FrameClock::frame_counter);
        let first_chunk = runtime.first_chunk_pending.replace(false);
        let outcome = self.run_collapse_step(StepSource::Step { first_chunk });
        let pending = !runtime.queue.borrow().is_empty();
        if pending {
            let budget = crate::ui::sidebar::policy::slc_attribution_unit_rows();
            let rows_left = clock
                .as_ref()
                .map_or(0, |clock| clock_rows_left(clock, budget));
            if outcome.chunks > 0 || (outcome.bound.is_some() && rows_left > 0) {
                self.arm_step_source();
            }
            // Unbound rows may be next (a step left a drained descendant's
            // tail, or the next step is a chunk). Not after a dispatch that
            // stopped at a bound step it could not take: the between-frames
            // idle would arm this source again, which would stop there again,
            // a busy loop between frames.
            if !outcome.stopped_at_bound {
                self.arm_between_frames_source();
            }
        } else {
            self.stop_collapse_sources();
        }
        let (kind, rows) = match outcome.bound {
            Some(bound) => bound,
            None if outcome.chunks > 0 => (StepDispatchKind::Chunk, outcome.chunk_rows),
            None => (StepDispatchKind::Idle, 0),
        };
        self.record_step_dispatch(started, kind, rows, setups_before, frame_counter);
    }

    /// Add one dispatch to the evidence log (bounded) and its time to the
    /// maximum.
    fn record_step_dispatch(
        &self,
        started: i64,
        kind: StepDispatchKind,
        rows: u32,
        setups_before: u64,
        frame_counter: i64,
    ) {
        let runtime = &self.imp().collapse_runtime;
        let took_us = u64::try_from(glib::monotonic_time().saturating_sub(started)).unwrap_or(0);
        runtime
            .step_dispatch_us_max
            .set(runtime.step_dispatch_us_max.get().max(took_us));
        let rows_created = u32::try_from(runtime.row_setups.get().saturating_sub(setups_before))
            .unwrap_or(u32::MAX);
        let mut log = runtime.step_dispatches.borrow_mut();
        if log.len() < STEP_DISPATCH_LOG_ROWS {
            log.push(StepDispatch {
                start_mono_us: started,
                took_us,
                kind,
                rows,
                rows_created,
                frame_counter,
            });
        }
    }

    /// Arm the idle that removes unbound rows between frames (design D19):
    /// they create no row widget (A118), so they need no frame, and below
    /// the frame clock's priority they never delay one. It stops at the
    /// first turn that finds only unit steps; the tick re-arms it.
    fn arm_between_frames_source(&self) {
        let runtime = &self.imp().collapse_runtime;
        if runtime.queue.borrow().is_empty() || runtime.between.borrow().is_some() {
            return;
        }
        let section = self.downgrade();
        let id = glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
            let Some(section) = section.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let worked = section.run_between_frames_step();
            if worked && !section.imp().collapse_runtime.queue.borrow().is_empty() {
                return glib::ControlFlow::Continue;
            }
            section.imp().collapse_runtime.between.borrow_mut().take();
            glib::ControlFlow::Break
        });
        runtime.between.replace(Some(id));
    }

    fn stop_collapse_sources(&self) {
        let runtime = &self.imp().collapse_runtime;
        if let Some(tick) = runtime.tick.borrow_mut().take() {
            tick.remove();
        }
        if let Some(source) = runtime.fallback.borrow_mut().take() {
            source.remove();
        }
        if let Some(source) = runtime.between.borrow_mut().take() {
            source.remove();
        }
        if let Some(source) = runtime.step_source.borrow_mut().take() {
            source.remove();
        }
        self.disconnect_after_paint();
        runtime.first_chunk_pending.set(false);
        runtime.last_step_us.set(0);
    }

    /// One turn of the between-frames idle; whether it removed any rows. A
    /// turn that ends at a bound step arms the step source, so a unit that
    /// becomes next mid-interval runs before the next frame (design D25).
    fn run_between_frames_step(&self) -> bool {
        let runtime = &self.imp().collapse_runtime;
        let started = glib::monotonic_time();
        let setups_before = runtime.row_setups.get();
        let frame_counter = self
            .imp()
            .file_tree_view
            .frame_clock()
            .map_or(-1, |clock| clock.frame_counter());
        let outcome = self.run_collapse_step(StepSource::Between);
        if outcome.chunks > 0 {
            self.record_step_dispatch(
                started,
                StepDispatchKind::Between,
                outcome.chunk_rows,
                setups_before,
                frame_counter,
            );
        }
        if outcome.stopped_at_bound {
            self.arm_step_source();
        }
        outcome.chunks > 0
    }

    /// The fallback: steps while nothing draws the list, or once a mapped
    /// list's frame clock has starved (design D4). A starved step takes a
    /// unit whatever the token holds, and flags it so the next tick takes
    /// none (design R6.2).
    fn run_fallback_step(&self) {
        let runtime = &self.imp().collapse_runtime;
        let list = &*self.imp().file_tree_view;
        let now = glib::monotonic_time();
        if list.is_mapped() && !frame_ticks_off_unmapped() {
            if frame_ticks_allowed()
                && (runtime.tick.borrow().is_none() || runtime.after_paint.borrow().is_none())
            {
                // Mapped again since the last check, or frames allowed again
                // after a map that could not connect them: pace from frames.
                // The tick alone keeps `last_step_us` fresh, so without its
                // `after-paint` handler no step would ever run.
                self.arm_collapse_source();
            }
            let idle_us = now.saturating_sub(runtime.last_step_us.get());
            let starvation_us =
                i64::try_from(COLLAPSE_FRAME_STARVATION_MS * 1_000).unwrap_or(i64::MAX);
            if idle_us < starvation_us {
                return;
            }
            runtime.last_step_us.set(now);
            self.run_collapse_step(StepSource::Fallback { starved: true });
            return;
        }
        runtime.last_step_us.set(now);
        let starved = list.is_mapped();
        self.run_collapse_step(StepSource::Fallback { starved });
    }

    /// One step over the queue from `source`: unbound chunks while the turn
    /// lasts (one, for the step source's D23 chunk), and at most one bound
    /// step within the frame interval's budget, as `source` allows.
    fn run_collapse_step(&self, source: StepSource) -> StepOutcome {
        let outcome = self.run_collapse_step_inner(source);
        // D22: rows a bound removal created entered the list inside the step,
        // after their bind (A123); hide them before the next frame.
        if outcome.bound.is_some() && !self.imp().collapse_runtime.queue.borrow().is_empty() {
            self.sync_hidden_rows();
        }
        outcome
    }

    fn run_collapse_step_inner(&self, source: StepSource) -> StepOutcome {
        if steps_held() {
            return StepOutcome::default();
        }
        let started = glib::monotonic_time();
        let outcome = self.run_collapse_step_from(started, source);
        let took = u64::try_from(glib::monotonic_time().saturating_sub(started)).unwrap_or(0);
        let runtime = &self.imp().collapse_runtime;
        runtime.step_us_max.set(runtime.step_us_max.get().max(took));
        outcome
    }

    /// The bound rows `source` may still remove in this frame interval, of
    /// `budget`: the step source reads the window-wide token (design D25); the
    /// fallback takes a unit whatever it holds (design R6.2).
    fn interval_rows_left_for(&self, source: StepSource, budget: u32) -> u32 {
        match source {
            StepSource::Step { .. } => self
                .imp()
                .file_tree_view
                .frame_clock()
                .map_or(0, |clock| clock_rows_left(&clock, budget)),
            StepSource::Fallback { .. } => budget,
            StepSource::Between => 0,
        }
    }

    /// Charge a bound step's rows to the interval: the step source its rows,
    /// a starved fallback unit the whole budget, so the frame that lays it out
    /// carries no other unit (design R6.2, D25).
    fn charge_interval(&self, source: StepSource, rows: u32, budget: u32) {
        let Some(clock) = self.imp().file_tree_view.frame_clock() else {
            return;
        };
        match source {
            StepSource::Step { .. } => charge_clock_rows(&clock, rows),
            StepSource::Fallback { starved: true } => charge_clock_rows(&clock, budget),
            StepSource::Fallback { starved: false } | StepSource::Between => {}
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one step loop over the queue; ATTRIBUTION PATCH (task 11.3) adds the \
                  interval budget, which task 11.4 moves into the policy"
    )]
    fn run_collapse_step_from(&self, started: i64, source: StepSource) -> StepOutcome {
        let (unbound_allowed, bound_allowed, chunk_limit, starved) = match source {
            StepSource::Step { first_chunk } => (first_chunk, true, 1, false),
            StepSource::Between => (true, false, usize::MAX, false),
            StepSource::Fallback { starved } => (true, true, usize::MAX, starved),
        };
        let budget = crate::ui::sidebar::policy::slc_attribution_unit_rows();
        let step_rows = crate::ui::sidebar::policy::slc_attribution_step_rows();
        let one_step = matches!(source, StepSource::Step { .. });
        let mut outcome = StepOutcome::default();
        let mut chunks = 0usize;
        let mut index = 0;
        let runtime = &self.imp().collapse_runtime;
        'records: loop {
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
            let mut dispatch_done = false;
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
                let policy_facts = CollapseFacts {
                    stores: &facts.stores,
                    band: &facts.band,
                    rows_after: facts.rows_after,
                    rows_before: facts.rows_before,
                };
                let mut step = next_collapse_step(&policy_facts);
                record.rows_left = facts.stores.first().map_or(0, |store| store.span);
                record.bound_rows_left = held_in(&facts);
                if step.takes_unit() {
                    if outcome.bound.is_some() || !bound_allowed {
                        outcome.stopped_at_bound = true;
                        break;
                    }
                    let rows = match step {
                        CollapseStep::CollapseRow => facts.stores[0].span,
                        CollapseStep::CollapseDescendant(store) => facts.stores[store].span,
                        CollapseStep::RemoveBound { .. } | CollapseStep::RemoveUnbound { .. } => 0,
                    };
                    let destroy_only = matches!(step, CollapseStep::RemoveBound { .. })
                        && collapse_removal_is_destroy_only(&policy_facts);
                    let rows_left = self.interval_rows_left_for(source, budget);
                    let Some((charge, count)) =
                        bound_step_rows(step, rows, destroy_only, rows_left, budget, step_rows)
                    else {
                        // The interval is spent: the next frame's
                        // `after-paint` arms the next step.
                        outcome.stopped_at_bound = true;
                        break;
                    };
                    if let CollapseStep::RemoveBound { store, start, .. } = step {
                        step = CollapseStep::RemoveBound {
                            store,
                            start,
                            count,
                        };
                    }
                    self.charge_interval(source, charge, budget);
                    outcome.bound = Some((
                        match step {
                            CollapseStep::CollapseRow => StepDispatchKind::Terminal,
                            CollapseStep::CollapseDescendant(_) => StepDispatchKind::Descendant,
                            CollapseStep::RemoveBound { .. }
                            | CollapseStep::RemoveUnbound { .. } => StepDispatchKind::Bound,
                        },
                        count,
                    ));
                } else if !unbound_allowed || chunks >= chunk_limit {
                    // Unbound rows leave between frames (design D19), except
                    // D23's one chunk.
                    break;
                } else if let CollapseStep::RemoveUnbound { count, .. } = step {
                    outcome.chunks += 1;
                    outcome.chunk_rows = outcome.chunk_rows.saturating_add(count);
                }
                record.steps += 1;
                runtime.steps_total.set(runtime.steps_total.get() + 1);
                if starved {
                    runtime
                        .starved_steps_total
                        .set(runtime.starved_steps_total.get() + 1);
                }
                if matches!(source, StepSource::Between) {
                    runtime
                        .between_steps_total
                        .set(runtime.between_steps_total.get() + 1);
                }
                chunks += 1;
                dispatch_done = one_step;
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
                        self.record_entry_point_transition(&row);
                    }
                    CollapseStep::CollapseRow => {
                        let row = facts.rows[0].clone();
                        drop(facts);
                        row.set_expanded(false);
                        self.record_entry_point_transition(&row);
                        record.rows_left = 0;
                        record.bound_rows_left = 0;
                        ended = Some(CollapseEnd::Completed);
                        break;
                    }
                }
                if dispatch_done {
                    break;
                }
            }
            if let Some(end) = ended {
                self.finish_collapse(&record, end);
            } else {
                runtime.queue.borrow_mut().insert(index, record);
                index += 1;
            }
            if dispatch_done {
                break 'records;
            }
        }
        outcome
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
        let mut first_realized = None::<u32>;
        let mut last_realized = None::<u32>;
        super::for_each_realized_file_row_overlay(self, |overlay| {
            if let Some(row) = overlay
                .child()
                .and_downcast::<gtk4::TreeExpander>()
                .and_then(|expander| expander.list_row())
            {
                let position = row.position();
                if position != gtk4::INVALID_LIST_POSITION {
                    band.push(position);
                    last_realized = Some(last_realized.map_or(position, |last| last.max(position)));
                    first_realized =
                        Some(first_realized.map_or(position, |first| first.min(position)));
                }
            }
        });
        let listed = self
            .imp()
            .tree_model
            .try_borrow()
            .ok()
            .and_then(|model| model.as_ref().map(ListModelExt::n_items))
            .unwrap_or(0);
        let rows_after = last_realized.map_or(listed, |last| listed.saturating_sub(last + 1));
        let rows_before = first_realized.unwrap_or(0);
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
            rows_after,
            rows_before,
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
        // The directories of collapses this one will take over, with their
        // rows: their caches were detached at their own intent (design D24),
        // so the collapse records locate them, not the path caches.
        let collapsing: Vec<(PathBuf, gtk4::TreeListRow)> = self
            .imp()
            .collapse_runtime
            .queue
            .borrow()
            .iter()
            .filter(|record| record.path.starts_with(path) && record.path.as_path() != path)
            .flat_map(|record| {
                std::iter::once((record.path.clone(), record.row.clone()))
                    .chain(record.descendants.iter().cloned())
            })
            .filter(|(_, candidate)| row_descends_from(candidate, row))
            .collect();
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
            let child = if let Some((_, taken)) = collapsing
                .iter()
                .find(|(collapsing_path, _)| *collapsing_path == candidate)
            {
                if !self.row_is_live(taken) {
                    return None;
                }
                taken.clone()
            } else {
                let location = self.live_item_location(&candidate)?;
                if location.parent_dir.as_deref() != Some(parent_path.as_path()) {
                    return None;
                }
                parent_row.child_row(u32::try_from(location.index).ok()?)?
            };
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
            self.stop_subtree_work(row, &path, &[]);
        }
        row.set_expanded(false);
        self.record_entry_point_transition(row);
        let runtime = &self.imp().collapse_runtime;
        runtime.direct_total.set(runtime.direct_total.get() + 1);
    }

    /// Record the expansion intent of a transition an entry point made, and
    /// refresh the row's own watch contribution (design R4, D20). The row's
    /// expansion hook does both for a row object it lives on, but GTK hands
    /// out a fresh row object, without the hook, for a row no widget shows
    /// (ledger A114); both calls are idempotent with the hook's.
    fn record_entry_point_transition(&self, row: &gtk4::TreeListRow) {
        self.record_row_expansion_transition(row);
        self.refresh_workspace_watch_row(row);
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
        let mut taken_over = Vec::new();
        {
            let mut queue = imp.collapse_runtime.queue.borrow_mut();
            let mut index = 0;
            while index < queue.len() {
                if queue[index].path.starts_with(&path) {
                    let record = queue.remove(index);
                    taken_over.push((record.path, record.origin));
                    taken_over.extend(record.taken_over);
                    imp.collapse_runtime
                        .last_end
                        .set(Some(CollapseEnd::TakenOver));
                } else {
                    index += 1;
                }
            }
        }
        imp.collapse_runtime
            .collapsing_paths
            .borrow_mut()
            .insert(path.clone());
        imp.collapse_runtime
            .collapsing_rows
            .borrow_mut()
            .push(row.clone());
        self.reproject_realized_rows();
        // Intent: the subtree is collapsed from now on.
        imp.expanded_paths
            .borrow_mut()
            .retain(|candidate| !candidate.starts_with(&path));
        // No load, scan or refresh may append to a store being drained.
        self.stop_subtree_work(row, &path, &descendants);
        // The steps never touch the path caches: each directory's cache
        // leaves whole now, after `capture_subtree` and the selection move
        // read the locations they need, and the listing retirement removes
        // its entries in slices (design D24).
        let stores: Vec<gio::ListStore> = std::iter::once(row)
            .chain(descendants.iter().map(|(_, descendant)| descendant))
            .filter_map(|candidate| candidate.children().and_downcast::<gio::ListStore>())
            .collect();
        super::scan_execution::detach_child_caches(self, &path, &stores);
        // Several collapses drain bottom-up (design R5, D7): a later
        // subtree's rows leave while unbound, so an earlier one's units bring
        // in only rows already collapsed. Removals never reorder two disjoint
        // subtrees, so the order holds for the queue's lifetime.
        let position = row.position();
        let mut queue = imp.collapse_runtime.queue.borrow_mut();
        let at = queue
            .iter()
            .position(|record| record.row.position() < position)
            .unwrap_or(queue.len());
        queue.insert(
            at,
            CollapseRecord {
                row: row.clone(),
                path,
                origin,
                descendants,
                taken_over,
                steps: 0,
                rows_left: 0,
                bound_rows_left: 0,
            },
        );
    }

    /// Cancel every scan, load turn and queued refresh target in the subtree
    /// of `path` (the directory's own pending refresh stays: it refreshes the
    /// directory's row, not its children).
    fn stop_subtree_work(
        &self,
        row: &gtk4::TreeListRow,
        path: &Path,
        descendants: &[ExpandedDescendant],
    ) {
        let imp = self.imp();
        // Membership by the subtree's own rows (design D22): the stores of
        // the row and its captured descendants are this occurrence's. A store
        // or refresh under the path that another workspace folder also lists
        // (overlapping folders) belongs, or may belong, to that folder's
        // occurrence, and keeps loading; on the direct path, which captured
        // no descendants, such a store of this occurrence is detached by
        // GTK's collapse, and its stale batches are dropped as any detached
        // store's are.
        let own_stores: Vec<usize> = std::iter::once(row)
            .chain(descendants.iter().map(|(_, descendant)| descendant))
            .filter_map(|candidate| candidate.children().and_downcast::<gio::ListStore>())
            .map(|store| super::scan_execution::child_store_key(&store))
            .collect();
        let other_roots = self.other_folder_roots(row);
        let listed_elsewhere =
            |candidate: &Path| other_roots.iter().any(|root| candidate.starts_with(root));
        let store_keys: Vec<usize> = imp
            .child_store_paths
            .borrow()
            .iter()
            .filter(|(key, store_path)| {
                store_path.starts_with(path)
                    && (own_stores.contains(key) || !listed_elsewhere(store_path))
            })
            .map(|(key, _)| *key)
            .collect();
        for key in store_keys {
            super::scan_execution::cancel_child_store_runtime(self, key);
        }
        imp.refresh_runtime
            .pending_paths
            .borrow_mut()
            .retain(|pending| {
                !pending.starts_with(path) || pending == path || listed_elsewhere(pending)
            });
        super::scan_execution::sync_child_scan_busy_state(self);
    }

    /// The workspace folder roots other than the one `row` lies under.
    fn other_folder_roots(&self, row: &gtk4::TreeListRow) -> Vec<PathBuf> {
        let mut top = row.clone();
        while let Some(parent) = top.parent() {
            top = parent;
        }
        let own = directory_path(&top);
        let Some(model) = self.imp().tree_model.borrow().as_ref().cloned() else {
            return Vec::new();
        };
        let roots = model.model();
        (0..roots.n_items())
            .filter_map(|index| roots.item(index).and_downcast::<FileTreeItem>())
            .filter_map(|item| item.path())
            .filter(|root| Some(root) != own.as_ref())
            .collect()
    }

    /// End one collapse with `end`.
    fn finish_collapse(&self, record: &CollapseRecord, end: CollapseEnd) {
        let runtime = &self.imp().collapse_runtime;
        runtime
            .collapsing_paths
            .borrow_mut()
            .retain(|candidate| !candidate.starts_with(&record.path));
        runtime.collapsing_rows.borrow_mut().retain(|candidate| {
            candidate != &record.row && !row_descends_from(candidate, &record.row)
        });
        runtime.last_end.set(Some(end));
        self.reproject_realized_rows();
        // D22: by any end, no row of the subtree stays hidden.
        self.sync_hidden_rows();
        // D22: keys held while the section collapsed act once its last
        // collapse has ended and a frame has laid out the final tree.
        if runtime.queue.borrow().is_empty() {
            self.arm_held_key_replay();
        }
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
            imp.collapse_runtime
                .collapsing_rows
                .borrow_mut()
                .retain(|candidate| {
                    directory_path(candidate).is_none_or(|own| !path.starts_with(own))
                });
            return;
        }
        let mut recollapse: Vec<(gtk4::TreeListRow, CollapseOrigin)> = Vec::new();
        for record in covering {
            self.finish_collapse(&record, CollapseEnd::ReExpanded);
            let root = record.row.clone();
            if !root.is_expanded() {
                continue;
            }
            // A taken-over descendant the expansion does not reach ends as
            // the user left it: collapsed (design R6.4).
            let stays_collapsed = |candidate: &Path| {
                record
                    .taken_over
                    .iter()
                    .any(|(taken, _)| candidate.starts_with(taken) && !path.starts_with(taken))
            };
            let mut refill = vec![(record.path.clone(), root)];
            for (descendant, row) in &record.descendants {
                if stays_collapsed(descendant) {
                    continue;
                }
                if row.is_expanded() && self.row_is_live(row) {
                    refill.push((descendant.clone(), row.clone()));
                }
            }
            for (taken, origin) in &record.taken_over {
                if path.starts_with(taken) {
                    continue;
                }
                if let Some((_, row)) = record
                    .descendants
                    .iter()
                    .find(|(descendant, _)| descendant == taken)
                    && row.is_expanded()
                    && self.row_is_live(row)
                {
                    recollapse.push((row.clone(), *origin));
                }
            }
            for (path, row) in refill {
                imp.expanded_paths.borrow_mut().insert(path.clone());
                if let Some(store) = row.children().and_downcast::<gio::ListStore>() {
                    // Cache the rows the steps left before the refill's first
                    // batch splices the caches (design D24).
                    super::scan_execution::adopt_child_cache(self, &path, &store);
                    super::scan_execution::populate_child_store(self, &path, &store);
                }
            }
        }
        for (row, origin) in recollapse {
            self.collapse_directory(&row, origin);
        }
        self.arm_collapse_source();
    }

    /// Whether `row` lies strictly inside a collapsing subtree, by tree
    /// ancestry against the collapsing rows (design D22, R6.5): never by
    /// path, so a second occurrence of a directory under an overlapping
    /// workspace folder is not inside.
    pub(super) fn row_in_collapsing_subtree(&self, row: &gtk4::TreeListRow) -> bool {
        let collapsing = self.imp().collapse_runtime.collapsing_rows.borrow();
        !collapsing.is_empty()
            && collapsing
                .iter()
                .any(|candidate| row_descends_from(row, candidate))
    }

    /// Whether `row` is a collapsing directory or lies inside one's subtree,
    /// by tree ancestry (design R6.5).
    fn row_is_collapsing(&self, row: &gtk4::TreeListRow) -> bool {
        let collapsing = self.imp().collapse_runtime.collapsing_rows.borrow();
        !collapsing.is_empty()
            && collapsing
                .iter()
                .any(|candidate| candidate == row || row_descends_from(row, candidate))
    }

    /// Project D22's hiding on a freshly bound row (design D22). A row widget
    /// the list is creating is not in the list yet at bind, and is still
    /// floating: a reference to it taken here would sink that reference, and
    /// dropping it would finalize the widget before GTK inserts it (ledger
    /// A123). Such a row (its overlay has no root) only arms the re-apply,
    /// which hides it once it is in the list; a rebound row is hidden or
    /// cleared here, because GTK recycles row widgets without resetting them.
    pub(super) fn project_collapse_hiding(&self, overlay: &gtk4::Overlay, row: &gtk4::TreeListRow) {
        let runtime = &self.imp().collapse_runtime;
        if runtime.collapsing_rows.borrow().is_empty() && runtime.hidden_rows.borrow().is_empty() {
            return;
        }
        let inside = self.row_in_collapsing_subtree(row);
        if overlay.root().is_none() {
            if inside {
                self.arm_hidden_reapply();
            }
            return;
        }
        let Some(row_widget) = overlay.parent() else {
            return;
        };
        if inside {
            self.hide_row_widget(&row_widget);
        } else {
            self.clear_row_widget(&row_widget);
        }
    }

    /// One pass over the realized band (about 200 rows, A1): every row
    /// inside a collapsing subtree is hidden, every other row the hiding
    /// touched is cleared, and the `hidden` state is written again on the
    /// hidden rows, which GTK's `map` resets (A123).
    fn sync_hidden_rows(&self) {
        let runtime = &self.imp().collapse_runtime;
        runtime.hidden_syncs.set(runtime.hidden_syncs.get() + 1);
        if runtime.queue.borrow().is_empty() {
            self.clear_hidden_rows();
            return;
        }
        let mut row_widget = self.imp().file_tree_view.first_child();
        while let Some(widget) = row_widget {
            row_widget = widget.next_sibling();
            let Some(row) = widget
                .first_child()
                .and_downcast::<gtk4::Overlay>()
                .and_then(|overlay| overlay.child().and_downcast::<gtk4::TreeExpander>())
                .and_then(|expander| expander.list_row())
            else {
                continue;
            };
            if self.row_in_collapsing_subtree(&row) {
                self.hide_row_widget(&widget);
            } else {
                self.clear_row_widget(&widget);
            }
        }
        runtime
            .hidden_rows
            .borrow_mut()
            .retain(|(row, _)| row.upgrade().is_some());
        for (row, _) in runtime.hidden_rows.borrow().iter() {
            if let Some(row) = row.upgrade() {
                accessibility::set_hidden(&row, true);
            }
        }
    }

    /// Hide one row widget (opacity 0, `can-target` false, accessible
    /// `hidden`: paint-only, A123), once.
    fn hide_row_widget(&self, row_widget: &gtk4::Widget) {
        let runtime = &self.imp().collapse_runtime;
        if runtime
            .hidden_rows
            .borrow()
            .iter()
            .any(|(hidden, _)| hidden.upgrade().as_ref() == Some(row_widget))
        {
            return;
        }
        row_widget.set_opacity(0.0);
        row_widget.set_can_target(false);
        accessibility::set_hidden(row_widget, true);
        let section = self.downgrade();
        let handler = row_widget.connect_map(move |_| {
            if let Some(section) = section.upgrade() {
                section.arm_hidden_reapply();
            }
        });
        runtime
            .hidden_rows
            .borrow_mut()
            .push((row_widget.downgrade(), handler));
    }

    /// Clear what [`Self::hide_row_widget`] set on `row_widget`, if it set
    /// anything. The `hidden` state is reset, not written false, so GTK's map
    /// writes govern it again.
    fn clear_row_widget(&self, row_widget: &gtk4::Widget) {
        let runtime = &self.imp().collapse_runtime;
        let entry = {
            let mut hidden = runtime.hidden_rows.borrow_mut();
            hidden
                .iter()
                .position(|(row, _)| row.upgrade().as_ref() == Some(row_widget))
                .map(|index| hidden.swap_remove(index))
        };
        if let Some((_, handler)) = entry {
            row_widget.disconnect(handler);
            row_widget.set_opacity(1.0);
            row_widget.set_can_target(true);
            accessibility::set_hidden(row_widget, false);
        }
    }

    /// Clear every row the hiding touched: no collapse is pending.
    fn clear_hidden_rows(&self) {
        let runtime = &self.imp().collapse_runtime;
        let hidden: Vec<_> = runtime.hidden_rows.borrow_mut().drain(..).collect();
        for (row, handler) in hidden {
            if let Some(row) = row.upgrade() {
                row.disconnect(handler);
                row.set_opacity(1.0);
                row.set_can_target(true);
                accessibility::set_hidden(&row, false);
            }
        }
        if let Some(source) = runtime.hidden_reapply.borrow_mut().take() {
            source.remove();
        }
    }

    /// Arm the one-shot re-apply of D22's hiding: at `G_PRIORITY_HIGH_IDLE`,
    /// so it runs once the dispatch that created or mapped a row has
    /// returned, before the next frame when it is armed between frames
    /// (A125). It runs no step.
    fn arm_hidden_reapply(&self) {
        let runtime = &self.imp().collapse_runtime;
        if runtime.hidden_reapply.borrow().is_some() || runtime.queue.borrow().is_empty() {
            return;
        }
        let section = self.downgrade();
        let id = glib::idle_add_local_full(glib::Priority::HIGH_IDLE, move || {
            if let Some(section) = section.upgrade() {
                let runtime = &section.imp().collapse_runtime;
                runtime.hidden_reapply.borrow_mut().take();
                runtime
                    .hidden_reapply_runs
                    .set(runtime.hidden_reapply_runs.get() + 1);
                section.sync_hidden_rows();
            }
            glib::ControlFlow::Break
        });
        runtime.hidden_reapply.replace(Some(id));
    }

    /// Re-project every realized directory row from the collapse state:
    /// the rows of a collapsing subtree show collapsed, and a row whose path
    /// left the collapsing set shows GTK's state again (design R6.4, R6.5).
    /// One pass over the realized band (about 200 rows, ledger A1).
    fn reproject_realized_rows(&self) {
        super::for_each_realized_file_row_overlay(self, |overlay| {
            let Some(expander) = overlay.child().and_downcast::<gtk4::TreeExpander>() else {
                return;
            };
            let Some(row) = expander.list_row() else {
                return;
            };
            if directory_path(&row).is_none() {
                return;
            }
            let collapsing = row.is_expanded() && self.row_is_collapsing(&row);
            let shown_collapsing = expander.has_css_class(COLLAPSING_CLASS);
            if collapsing == shown_collapsing {
                return;
            }
            if collapsing {
                expander.add_css_class(COLLAPSING_CLASS);
            } else {
                expander.remove_css_class(COLLAPSING_CLASS);
            }
            let expanded = row.is_expanded() && !collapsing;
            accessibility::set_expanded(&overlay, Some(expanded));
            accessibility::set_expanded(&expander, Some(expanded));
            if let Some(item) = row.item().and_downcast::<FileTreeItem>() {
                accessibility::set_description(
                    &overlay,
                    &super::row_accessibility::file_tree_row_description(&item, &row, self),
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
            let Some(event) = controller.current_event() else {
                return glib::Propagation::Proceed;
            };
            let Some(key) = event.downcast_ref::<gtk4::gdk::KeyEvent>() else {
                return glib::Propagation::Proceed;
            };
            let expander = section.focused_expander();
            let intent = expander
                .as_ref()
                .and_then(|expander| disclosure_intent(key, expander));
            // Design D22: while a collapse runs, navigation keys (and row
            // keys behind them) wait for its end.
            if section.hold_key(&event, key, intent.is_some()) {
                return glib::Propagation::Stop;
            }
            let (Some(expander), Some(intent)) = (expander, intent) else {
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
    /// section's list, if any. Only the expander itself counts (design
    /// R6.1): a widget inside it, such as the inline rename entry, takes its
    /// keys first, as on `main`, and a key it does not consume reaches GTK's
    /// binding, a residual route.
    fn focused_expander(&self) -> Option<gtk4::TreeExpander> {
        let window = self.root().and_downcast::<gtk4::Window>()?;
        let expander = gtk4::prelude::GtkWindowExt::focus(&window)?
            .downcast::<gtk4::TreeExpander>()
            .ok()?;
        expander
            .is_ancestor(&*self.imp().file_tree_view)
            .then_some(expander)
    }

    /// Act on `intent` for `expander`'s row where the collapse entry points
    /// own it; return false to let GTK's binding run.
    fn take_disclosure_key(&self, expander: &gtk4::TreeExpander, intent: DisclosureKey) -> bool {
        let Some(row) = expander.list_row() else {
            return false;
        };
        if directory_path(&row).is_none() {
            return false;
        }
        if self.row_inside_collapsing_subtree(&row) {
            // A row whose ancestor collapses shows collapsed and ignores
            // disclosure (design R6.5): no key may end the ancestor's
            // collapse or start GTK's own toggle.
            return true;
        }
        let collapsing = self.row_is_collapsing(&row);
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

/// Whether a test runs the fallback as if the list were unmapped, skipping
/// the starvation timeout (`test_policy::CollapsePacingForTest::NoFrameClock`).
fn frame_ticks_off_unmapped() -> bool {
    #[cfg(feature = "test-utils")]
    {
        crate::ui::sidebar::test_policy::collapse_pacing()
            == crate::ui::sidebar::test_policy::CollapsePacingForTest::NoFrameClock
    }
    #[cfg(not(feature = "test-utils"))]
    {
        false
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
/// Whether `ancestor` is a strict ancestor of `row` in the tree list model.
fn row_descends_from(row: &gtk4::TreeListRow, ancestor: &gtk4::TreeListRow) -> bool {
    let mut parent = row.parent();
    while let Some(candidate) = parent {
        if &candidate == ancestor {
            return true;
        }
        parent = candidate.parent();
    }
    false
}

fn directory_path(row: &gtk4::TreeListRow) -> Option<PathBuf> {
    row.item()
        .and_downcast::<FileTreeItem>()
        .filter(|item| item.is_dir() && !item.is_placeholder())
        .and_then(|item| item.path())
}
