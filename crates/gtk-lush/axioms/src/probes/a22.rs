// SPDX-License-Identifier: MIT OR Apache-2.0

//! A22: in a `GtkTreeListModel` built with `passthrough = false` and
//! `autoexpand = false`, `TreeListRow::set_expanded(true)` on a collapsed row
//! calls the create function **synchronously**, before `set_expanded`
//! returns. When it returns a model, the row is expanded on return,
//! `children()` is that same model, and the flattened model already holds the
//! children (its `items-changed` fired inside the call). When it returns
//! `None`, the row stays collapsed, `children()` is `None`, and the flattened
//! model is unchanged. Items appended later to the returned `gio::ListStore`
//! appear at `parent + 1 + index`, and `set_expanded(false)` collapses the row
//! and removes its children synchronously.
//!
//! Recorded beside it: `is_expandable()` on a row whose create function has
//! not run yet runs it and discards the result (a model is not installed; a
//! later `set_expanded(true)` creates a fresh one), and a `None` from either
//! call marks the row empty, so no later `set_expanded(true)` calls again.
//! `GtkTreeExpander` asks `is_expandable()` when it binds a row, so a row on
//! screen has usually been probed already; the expanded state on return is
//! final either way.
//!
//! LushText's workspace path reveal (`ui/sidebar/reveal_execution.rs`)
//! expands an ancestor and reads `is_expanded()` straight back to tell a
//! directory from a known-empty or non-directory row, finds scanned children
//! by walking the flattened model, and collapses what it expanded on failure.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;

use crate::observation::{Recorder, Stop};
use crate::session::flush_events;
use crate::{AxiomId, Observation};

/// The root items, in order. [`EXPANDABLE`] gets a child model,
/// [`CHILDLESS`] gets `None`, the last is never expanded.
pub const ROOT_ITEMS: [&str; 3] = [EXPANDABLE, CHILDLESS, "c"];
/// The root item whose create call returns a child store.
pub const EXPANDABLE: &str = "a";
/// The root item whose create call returns `None`.
pub const CHILDLESS: &str = "b";
/// The children the create function puts in [`EXPANDABLE`]'s store.
pub const INITIAL_CHILDREN: [&str; 2] = ["a/0", "a/1"];
/// The child appended later, as an asynchronous directory scan would.
pub const APPENDED_CHILD: &str = "a/2";

/// A `GtkTreeListModel` (`passthrough = false`, `autoexpand = false`) whose
/// create function counts its calls and keeps the store it returned.
#[derive(Clone, Debug)]
pub struct LazyTree {
    /// The flattened model.
    pub tree: gtk4::TreeListModel,
    calls: Rc<Cell<u32>>,
    returned: Rc<RefCell<Option<gtk4::gio::ListStore>>>,
}

impl LazyTree {
    /// Build the fixture over [`ROOT_ITEMS`].
    #[must_use]
    pub fn new() -> Self {
        let root = gtk4::gio::ListStore::new::<gtk4::StringObject>();
        for name in ROOT_ITEMS {
            root.append(&gtk4::StringObject::new(name));
        }
        let calls = Rc::new(Cell::new(0));
        let returned = Rc::new(RefCell::new(None));
        let counter = calls.clone();
        let keeper = returned.clone();
        let tree = gtk4::TreeListModel::new(root, false, false, move |item| {
            counter.set(counter.get() + 1);
            let name = item.downcast_ref::<gtk4::StringObject>()?.string();
            if name != EXPANDABLE {
                return None;
            }
            let store = gtk4::gio::ListStore::new::<gtk4::StringObject>();
            for child in INITIAL_CHILDREN {
                store.append(&gtk4::StringObject::new(child));
            }
            keeper.replace(Some(store.clone()));
            Some(store.upcast())
        });
        Self {
            tree,
            calls,
            returned,
        }
    }

    /// How many times the create function has run.
    #[must_use]
    pub fn create_calls(&self) -> u32 {
        self.calls.get()
    }

    /// The store the create function last returned for [`EXPANDABLE`].
    #[must_use]
    pub fn returned_store(&self) -> Option<gtk4::gio::ListStore> {
        self.returned.borrow().clone()
    }

    /// The row whose item is the string `name`, found by a linear walk of
    /// the flattened model, with its position.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<(u32, gtk4::TreeListRow)> {
        (0..self.tree.n_items()).find_map(|position| {
            let row = self.tree.row(position)?;
            let text = row.item().and_downcast::<gtk4::StringObject>()?.string();
            (text == name).then_some((position, row))
        })
    }

    /// Append [`APPENDED_CHILD`] to the returned store, if there is one, and
    /// return the appended object.
    #[must_use]
    pub fn append_child(&self) -> Option<gtk4::StringObject> {
        let store = self.returned_store()?;
        let object = gtk4::StringObject::new(APPENDED_CHILD);
        store.append(&object);
        Some(object)
    }
}

impl Default for LazyTree {
    fn default() -> Self {
        Self::new()
    }
}

/// Counts `items-changed` emissions, and those emitted while `inside` is set.
#[derive(Clone, Default)]
struct ItemsChanged {
    total: Rc<Cell<u32>>,
    inside_call: Rc<Cell<u32>>,
    inside: Rc<Cell<bool>>,
}

impl ItemsChanged {
    fn watch(tree: &gtk4::TreeListModel) -> Self {
        let counts = Self::default();
        let watcher = counts.clone();
        tree.connect_items_changed(move |_, _, _, _| {
            watcher.total.set(watcher.total.get() + 1);
            if watcher.inside.get() {
                watcher.inside_call.set(watcher.inside_call.get() + 1);
            }
        });
        counts
    }

    /// Run `call`, returning how many emissions happened inside it.
    fn during(&self, call: impl FnOnce()) -> u32 {
        let before = self.inside_call.get();
        self.inside.set(true);
        call();
        self.inside.set(false);
        self.inside_call.get() - before
    }
}

/// Probe A22. See the module documentation. It needs GTK initialized but no
/// window: a tree list model is a plain model.
#[must_use]
pub fn probe_a22() -> Observation {
    Recorder::run(AxiomId::new(22), |recorder| {
        let fixture = LazyTree::new();
        let tree = &fixture.tree;
        let signals = ItemsChanged::watch(tree);

        recorder.measure("root_n_items", tree.n_items());
        recorder.measure("create_calls_at_rest", fixture.create_calls());
        recorder.control(
            tree.n_items() == 3 && fixture.create_calls() == 0,
            "control: autoexpand=false creates no child model before any expansion",
        )?;
        let (_, expandable) = recorder.require(
            fixture.find(EXPANDABLE),
            "control: the expandable row exists",
        )?;
        // Not `is_expandable()`: on a row never probed it runs the create
        // function itself (measured by `probe_is_expandable` below).
        recorder.control(
            !expandable.is_expanded() && fixture.create_calls() == 0,
            "control: the expandable row starts collapsed, its create function not yet run",
        )?;

        expand_with_model(recorder, &fixture, &signals, &expandable)?;
        expand_without_model(recorder, &fixture, &signals)?;
        append_and_collapse(recorder, &fixture, &signals, &expandable)?;
        probe_is_expandable(recorder)
    })
}

/// Clauses 1 and 2: the create call is synchronous, and a returned model is
/// installed before `set_expanded` returns.
fn expand_with_model(
    recorder: &mut Recorder,
    fixture: &LazyTree,
    signals: &ItemsChanged,
    row: &gtk4::TreeListRow,
) -> Result<(), Stop> {
    let tree = &fixture.tree;
    let mut calls_inside = 0;
    let mut expanded_on_return = false;
    let mut n_items_on_return = 0;
    let emissions = signals.during(|| {
        row.set_expanded(true);
        calls_inside = fixture.create_calls();
        expanded_on_return = row.is_expanded();
        n_items_on_return = tree.n_items();
    });
    let children = row.children();
    let same_model = match (children.as_ref(), fixture.returned_store()) {
        (Some(children), Some(store)) => children == store.upcast_ref::<gtk4::gio::ListModel>(),
        _ => false,
    };
    flush_events();
    recorder.measure("expand_create_calls_on_return", calls_inside);
    recorder.measure("expand_create_calls_after_flush", fixture.create_calls());
    recorder.measure("expand_items_changed_inside_call", emissions);
    recorder.measure("expand_is_expanded_on_return", expanded_on_return);
    recorder.measure("expand_children_is_returned_store", same_model);
    recorder.measure("expand_n_items_on_return", n_items_on_return);
    recorder.measure("expand_n_items_after_flush", tree.n_items());
    recorder.axiom(
        calls_inside == 1,
        "A22: set_expanded(true) calls the create function before it returns",
    )?;
    recorder.axiom(
        fixture.create_calls() == 1,
        "A22: no further create call follows from the main loop",
    )?;
    recorder.axiom(
        expanded_on_return,
        "A22: a row whose create function returned a model is expanded on return",
    )?;
    recorder.axiom(
        same_model,
        "A22: children() is the very model the create function returned",
    )?;
    recorder.axiom(
        n_items_on_return == 5 && emissions >= 1,
        "A22: the children are in the flattened model, items-changed fired inside the call",
    )?;
    let first = fixture
        .find(INITIAL_CHILDREN[0])
        .map(|(position, _)| position);
    let second = fixture
        .find(INITIAL_CHILDREN[1])
        .map(|(position, _)| position);
    recorder.measure("first_child_position", format!("{first:?}"));
    recorder.measure("second_child_position", format!("{second:?}"));
    recorder.axiom(
        first == Some(1) && second == Some(2),
        "A22: initial children sit at parent + 1 + index",
    )?;
    Ok(())
}

/// Clause 3: a create function returning `None` leaves the row collapsed.
fn expand_without_model(
    recorder: &mut Recorder,
    fixture: &LazyTree,
    signals: &ItemsChanged,
) -> Result<(), Stop> {
    let tree = &fixture.tree;
    let (position, row) =
        recorder.require(fixture.find(CHILDLESS), "control: the childless row exists")?;
    recorder.measure("childless_position", position);
    let calls_before = fixture.create_calls();
    let n_items_before = tree.n_items();
    let mut calls_inside = 0;
    let mut expanded_on_return = true;
    let emissions = signals.during(|| {
        row.set_expanded(true);
        calls_inside = fixture.create_calls() - calls_before;
        expanded_on_return = row.is_expanded();
    });
    flush_events();
    recorder.measure("none_create_calls_on_return", calls_inside);
    recorder.measure("none_items_changed_inside_call", emissions);
    recorder.measure("none_is_expanded_on_return", expanded_on_return);
    recorder.measure("none_children_is_none", row.children().is_none());
    recorder.measure("none_n_items", tree.n_items());
    recorder.axiom(
        calls_inside == 1,
        "A22: set_expanded(true) calls the create function even when it returns None",
    )?;
    recorder.axiom(
        !expanded_on_return && row.children().is_none(),
        "A22: a row whose create function returned None stays collapsed with no children",
    )?;
    recorder.axiom(
        tree.n_items() == n_items_before && emissions == 0,
        "A22: a None create call leaves the flattened model unchanged",
    )?;

    // GTK marks the row empty after a None, so a retry never calls again.
    let calls_before_retry = fixture.create_calls();
    row.set_expanded(true);
    let retry_calls = fixture.create_calls() - calls_before_retry;
    recorder.measure("none_retry_create_calls", retry_calls);
    recorder.measure("none_retry_is_expanded", row.is_expanded());
    let calls_before_query = fixture.create_calls();
    recorder.measure("none_is_expandable_after", row.is_expandable());
    recorder.measure(
        "none_is_expandable_create_calls",
        fixture.create_calls() - calls_before_query,
    );
    recorder.axiom(
        !row.is_expanded() && retry_calls == 0,
        "A22: a second set_expanded(true) on that row stays collapsed and calls nothing",
    )
}

/// Clause 4: later appends land at `parent + 1 + index`, and collapsing is
/// synchronous.
fn append_and_collapse(
    recorder: &mut Recorder,
    fixture: &LazyTree,
    signals: &ItemsChanged,
    row: &gtk4::TreeListRow,
) -> Result<(), Stop> {
    let tree = &fixture.tree;
    let n_items_before = tree.n_items();
    let mut appended = None;
    let emissions = signals.during(|| appended = fixture.append_child());
    let appended = recorder.require(appended, "control: the returned store took an append")?;
    let parent = row.position();
    let found = fixture.find(APPENDED_CHILD);
    let position = found.as_ref().map(|(position, _)| *position);
    let same_object = found
        .as_ref()
        .and_then(|(_, found)| found.item())
        .is_some_and(|item| item == appended.upcast::<gtk4::glib::Object>());
    let depth = found.as_ref().map(|(_, found)| found.depth());
    recorder.measure("append_parent_position", parent);
    recorder.measure("append_items_changed_inside_call", emissions);
    recorder.measure("append_n_items", tree.n_items());
    recorder.measure("append_found_at", format!("{position:?}"));
    recorder.measure("append_same_object", same_object);
    recorder.measure("append_depth", format!("{depth:?}"));
    recorder.axiom(
        tree.n_items() == n_items_before + 1 && emissions >= 1,
        "A22: an item appended to the returned store enters the flattened model at once",
    )?;
    recorder.axiom(
        position == Some(parent + 1 + 2) && same_object && depth == Some(1),
        "A22: the appended child sits at parent + 1 + index, found by a linear walk",
    )?;

    let n_items_expanded = tree.n_items();
    let mut collapsed_on_return = false;
    let mut n_items_on_return = 0;
    let emissions = signals.during(|| {
        row.set_expanded(false);
        collapsed_on_return = !row.is_expanded();
        n_items_on_return = tree.n_items();
    });
    flush_events();
    recorder.measure("collapse_items_changed_inside_call", emissions);
    recorder.measure("collapse_n_items_before", n_items_expanded);
    recorder.measure("collapse_n_items_on_return", n_items_on_return);
    recorder.measure("collapse_children_after", row.children().is_some());
    recorder.measure("items_changed_total", signals.total.get());
    recorder.axiom(
        collapsed_on_return,
        "A22: set_expanded(false) collapses the row before it returns",
    )?;
    recorder.axiom(
        n_items_on_return == u32::try_from(ROOT_ITEMS.len()).unwrap_or(u32::MAX)
            && emissions >= 1
            && row.children().is_none()
            && fixture.find(APPENDED_CHILD).is_none(),
        "A22: collapsing removes every child from the flattened model synchronously",
    )
}

/// Recorded beside the axiom: `is_expandable()` on a row never probed runs
/// the create function and discards what it returns, and a `None` there
/// marks the row empty, so a later `set_expanded(true)` does not call again.
/// `GtkTreeExpander` asks `is_expandable()` when it binds a row, so a bound
/// row has usually been probed before anything expands it.
fn probe_is_expandable(recorder: &mut Recorder) -> Result<(), Stop> {
    let fixture = LazyTree::new();
    let tree = &fixture.tree;
    let (_, expandable) = recorder.require(
        fixture.find(EXPANDABLE),
        "control: the fresh fixture's expandable row exists",
    )?;
    let answer = expandable.is_expandable();
    let probed_store = fixture.returned_store();
    recorder.measure("query_expandable_answer", answer);
    recorder.measure("query_expandable_create_calls", fixture.create_calls());
    recorder.measure("query_expandable_is_expanded", expandable.is_expanded());
    recorder.measure("query_expandable_children", expandable.children().is_some());
    recorder.measure("query_expandable_n_items", tree.n_items());
    recorder.axiom(
        answer
            && fixture.create_calls() == 1
            && !expandable.is_expanded()
            && expandable.children().is_none()
            && tree.n_items() == 3,
        "A22: is_expandable() runs the create function and installs nothing",
    )?;
    expandable.set_expanded(true);
    let installed_is_probed = match (expandable.children(), probed_store) {
        (Some(children), Some(probed)) => children == probed.upcast::<gtk4::gio::ListModel>(),
        _ => false,
    };
    recorder.measure("query_then_expand_create_calls", fixture.create_calls());
    recorder.measure(
        "query_then_expand_installs_probed_store",
        installed_is_probed,
    );
    recorder.axiom(
        fixture.create_calls() == 2 && expandable.is_expanded() && !installed_is_probed,
        "A22: a later set_expanded(true) calls the create function again for a fresh model",
    )?;

    let (_, childless) = recorder.require(
        fixture.find(CHILDLESS),
        "control: the fresh fixture's childless row exists",
    )?;
    let before = fixture.create_calls();
    let answer = childless.is_expandable();
    let query_calls = fixture.create_calls() - before;
    childless.set_expanded(true);
    let expand_calls = fixture.create_calls() - before - query_calls;
    recorder.measure("query_childless_answer", answer);
    recorder.measure("query_childless_create_calls", query_calls);
    recorder.measure("query_childless_then_expand_create_calls", expand_calls);
    recorder.measure(
        "query_childless_then_expand_is_expanded",
        childless.is_expanded(),
    );
    recorder.axiom(
        !answer && query_calls == 1 && expand_calls == 0 && !childless.is_expanded(),
        "A22: a None from is_expandable() marks the row empty; set_expanded(true) skips it",
    )
}
