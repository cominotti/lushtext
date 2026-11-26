// SPDX-License-Identifier: MIT OR Apache-2.0

//! A135: `GtkTreeExpander` queries `GtkTreeListRow::is_expandable()`
//! synchronously when a row is set on it (`set_list_row`) and again on row
//! property notifications other than `expanded` and `item` (including
//! `notify::children`), regardless of `hide-expander`. `is_expandable()`
//! caches only negative (`None`) answers (`node->empty`); a positive answer is
//! never cached, so every query on a collapsed, non-empty row runs the create
//! function again, and any returned model held by nothing else is finalized
//! before the query returns. A model installed by `set_expanded(true)` stays
//! alive while the row is expanded and equals `children()`, and collapsing
//! finalizes it and triggers one fresh create call on a bound expander. After a
//! `None` answer, further queries and rebinds call the create function no more.
//!
//! LushText's workspace sidebar (`ui/sidebar/workspace_section/materialization_execution.rs`
//! and the create closure in `folder_execution.rs`) relies on all five facts:
//! the create function returns an empty store and queues a weak handle, and
//! only stores GTK actually installed stay alive until the `Priority::HIGH`
//! materialization drain runs.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::{AxiomId, Observation};

/// The root items in order: [`EXPANDABLE`] gets a one-item store,
/// [`CHILDLESS`] gets `None`, `"c"` is an unexpanded control.
pub const ROOT_ITEMS: [&str; 3] = [EXPANDABLE, CHILDLESS, "c"];
/// The root item whose create function returns a child store.
pub const EXPANDABLE: &str = "a";
/// The root item whose create function returns `None`.
pub const CHILDLESS: &str = "b";
/// The child item placed in [`EXPANDABLE`]'s store.
pub const CHILD_ITEM: &str = "a/0";

/// A `TreeListModel` (`passthrough = false`, `autoexpand = false`) that counts
/// create calls and tracks weak references to every returned child store.
#[derive(Clone, Debug)]
pub struct ExpanderTree {
    /// The flattened model.
    pub tree: gtk4::TreeListModel,
    calls: Rc<Cell<u32>>,
    returned_weak: Rc<RefCell<Vec<gtk4::glib::WeakRef<gtk4::gio::ListStore>>>>,
}

impl ExpanderTree {
    /// Build the fixture over [`ROOT_ITEMS`].
    #[must_use]
    pub fn new() -> Self {
        let root = gtk4::gio::ListStore::new::<gtk4::StringObject>();
        for name in ROOT_ITEMS {
            root.append(&gtk4::StringObject::new(name));
        }
        let calls = Rc::new(Cell::new(0));
        let returned_weak = Rc::new(RefCell::new(Vec::new()));
        let counter = calls.clone();
        let weak_log = returned_weak.clone();
        let tree = gtk4::TreeListModel::new(root, false, false, move |item| {
            counter.set(counter.get() + 1);
            let name = item.downcast_ref::<gtk4::StringObject>()?.string();
            if name != EXPANDABLE {
                return None;
            }
            let store = gtk4::gio::ListStore::new::<gtk4::StringObject>();
            store.append(&gtk4::StringObject::new(CHILD_ITEM));
            weak_log.borrow_mut().push(store.downgrade());
            Some(store.upcast())
        });
        Self {
            tree,
            calls,
            returned_weak,
        }
    }

    /// How many times the create function has run.
    #[must_use]
    pub fn create_calls(&self) -> u32 {
        self.calls.get()
    }

    /// How many non-`None` stores the create function has returned.
    #[must_use]
    pub fn returned_store_count(&self) -> usize {
        self.returned_weak.borrow().len()
    }

    /// Whether the most recently returned store is still alive.
    #[must_use]
    pub fn latest_store_alive(&self) -> bool {
        self.returned_weak
            .borrow()
            .last()
            .and_then(gtk4::glib::WeakRef::upgrade)
            .is_some()
    }

    /// Upgrade the store returned at zero-based index `index`, if still alive.
    #[must_use]
    pub fn store_at(&self, index: usize) -> Option<gtk4::gio::ListStore> {
        self.returned_weak
            .borrow()
            .get(index)
            .and_then(gtk4::glib::WeakRef::upgrade)
    }

    /// Find the row whose item is `name`.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<(u32, gtk4::TreeListRow)> {
        (0..self.tree.n_items()).find_map(|position| {
            let row = self.tree.row(position)?;
            let text = row.item().and_downcast::<gtk4::StringObject>()?.string();
            (text == name).then_some((position, row))
        })
    }
}

impl Default for ExpanderTree {
    fn default() -> Self {
        Self::new()
    }
}

/// Probe A135. Needs GTK initialized but no window: `GtkTreeExpander` and
/// `GtkTreeListModel` run the query and lifecycle synchronously without mapping.
#[must_use]
pub fn probe_a135() -> Observation {
    Recorder::run(AxiomId::new(135), |recorder| {
        let fixture = ExpanderTree::new();
        let expander = gtk4::TreeExpander::new();
        let (_, row_a) = recorder.require(fixture.find(EXPANDABLE), "control: row a exists")?;
        let (_, row_b) = recorder.require(fixture.find(CHILDLESS), "control: row b exists")?;

        recorder.control(
            fixture.create_calls() == 0 && !row_a.is_expanded() && !row_b.is_expanded(),
            "control: fresh TreeListModel made zero create calls",
        )?;

        // 1. First bind on a collapsed, never-probed row.
        expander.set_list_row(Some(&row_a));
        let step1_calls = fixture.create_calls();
        let step1_alive = fixture.latest_store_alive();
        recorder.measure("step1_bind_calls", step1_calls);
        recorder.measure("step1_probe_store_alive", step1_alive);
        recorder.axiom(
            step1_calls == 1 && !step1_alive,
            "A135: set_list_row queries is_expandable synchronously and discards the probe store",
        )?;

        // 2. Unbind and rebind: no positive cache, fresh probe store discarded.
        expander.set_list_row(None::<&gtk4::TreeListRow>);
        let calls_after_unbind = fixture.create_calls();
        expander.set_list_row(Some(&row_a));
        let step2_calls = fixture.create_calls() - calls_after_unbind;
        let step2_alive = fixture.latest_store_alive();
        recorder.measure("step2_unbind_calls", calls_after_unbind - step1_calls);
        recorder.measure("step2_rebind_calls", step2_calls);
        recorder.measure("step2_probe_store_alive", step2_alive);
        recorder.axiom(
            calls_after_unbind == step1_calls && step2_calls == 1 && !step2_alive,
            "A135: a positive answer is not cached; rebinding queries and discards again",
        )?;

        // 3. With hide-expander = true, setting the flag and rebinding still query and discard.
        let calls_before_hide = fixture.create_calls();
        expander.set_hide_expander(true);
        let calls_on_hide = fixture.create_calls() - calls_before_hide;
        let hide_alive = fixture.latest_store_alive();
        expander.set_list_row(None::<&gtk4::TreeListRow>);
        let calls_before_hidden_rebind = fixture.create_calls();
        expander.set_list_row(Some(&row_a));
        let step3_rebind_calls = fixture.create_calls() - calls_before_hidden_rebind;
        let step3_alive = fixture.latest_store_alive();
        recorder.measure("step3_set_hide_expander_calls", calls_on_hide);
        recorder.measure("step3_hidden_rebind_calls", step3_rebind_calls);
        recorder.measure("step3_probe_store_alive", hide_alive || step3_alive);
        recorder.axiom(
            calls_on_hide == 1 && !hide_alive && step3_rebind_calls == 1 && !step3_alive,
            "A135: hide-expander = true does not suppress the synchronous is_expandable query",
        )?;

        // Restore hide-expander = false while unbound, then rebind row a.
        expander.set_list_row(None::<&gtk4::TreeListRow>);
        expander.set_hide_expander(false);
        expander.set_list_row(Some(&row_a));

        // 4. Expanding row a while bound installs a live store without extra notify queries.
        let calls_before_expand = fixture.create_calls();
        let store_idx_before_expand = fixture.returned_store_count();
        row_a.set_expanded(true);
        let step4_calls = fixture.create_calls() - calls_before_expand;
        let installed_store = fixture.store_at(store_idx_before_expand);
        let step4_alive = installed_store.is_some();
        let step4_matches_children = match (installed_store.as_ref(), row_a.children()) {
            (Some(store), Some(children)) => {
                *store.upcast_ref::<gtk4::gio::ListModel>() == children
            }
            _ => false,
        };
        drop(installed_store);
        recorder.measure("step4_expand_calls", step4_calls);
        recorder.measure("step4_installed_store_alive", step4_alive);
        recorder.measure("step4_installed_equals_children", step4_matches_children);
        recorder.axiom(
            step4_calls == 1 && step4_alive && step4_matches_children,
            "A135: set_expanded(true) installs a live model equal to children() with no extra notify query",
        )?;

        // 5. Collapsing row a while bound finalizes the installed store and triggers one fresh probe.
        let calls_before_collapse = fixture.create_calls();
        row_a.set_expanded(false);
        let step5_calls = fixture.create_calls() - calls_before_collapse;
        let installed_after_collapse = fixture.store_at(store_idx_before_expand).is_some();
        let step5_probe_alive = fixture.latest_store_alive();
        recorder.measure(
            "step5_installed_alive_after_collapse",
            installed_after_collapse,
        );
        recorder.measure("step5_collapse_follow_up_calls", step5_calls);
        recorder.measure("step5_collapse_probe_alive", step5_probe_alive);
        recorder.axiom(
            !installed_after_collapse && step5_calls == 1 && !step5_probe_alive,
            "A135: collapsing finalizes the installed model and causes one fresh discarded probe",
        )?;

        // 6. Control: negative answers are final across rebinds and set_expanded(true).
        let calls_before_b_first = fixture.create_calls();
        expander.set_list_row(Some(&row_b));
        let b_first_calls = fixture.create_calls() - calls_before_b_first;
        let calls_before_b_rebinds = fixture.create_calls();
        for _ in 0..3 {
            expander.set_list_row(None::<&gtk4::TreeListRow>);
            expander.set_list_row(Some(&row_b));
        }
        row_b.set_expanded(true);
        let b_expandable_after = row_b.is_expandable();
        let b_followup_calls = fixture.create_calls() - calls_before_b_rebinds;
        recorder.measure("step6_childless_first_bind_calls", b_first_calls);
        recorder.measure("step6_childless_followup_calls", b_followup_calls);
        recorder.measure("step6_childless_is_expandable", b_expandable_after);
        recorder.axiom(
            b_first_calls == 1
                && b_followup_calls == 0
                && !b_expandable_after
                && !row_b.is_expanded(),
            "A135: a None return marks the row empty; further rebinds and expansions call nothing",
        )
    })
}
