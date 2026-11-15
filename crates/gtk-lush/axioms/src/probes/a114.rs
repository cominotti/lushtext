// SPDX-License-Identifier: MIT OR Apache-2.0

//! A114: a `GtkTreeListModel` keeps no `GtkTreeListRow` object of its own for
//! a node: the row object `get_item` hands out lives only while something
//! references it. Once every reference is gone (no list item shows the row
//! and the caller dropped it), the object is finalized, and the next
//! `get_item` for the same node hands out a **new** row object, which carries
//! none of the dead one's handlers (and no object data: it is a different
//! instance). The node's expansion is kept by the model, not by the row
//! object, so the new object reads the expansion the old one set. A row a
//! list item is bound to keeps its object for as long as it is bound.
//!
//! The probe presents a list over two directory nodes, the first expanded
//! over 4,096 children so the second lies far outside the realized band. It
//! connects a `notify::expanded` handler to the second node's row object,
//! drops the object, and fetches the node again.
//!
//! LushText's sidebar (`slice-large-directory-collapse` R4, D20) recorded
//! expansion intent from a handler on the row objects it saw at a splice, so
//! a collapse through a row object created later (a far root collapsed by the
//! folder header) notified nobody, and a deferred restore expanded it again.
//! Its collapse and expand entry points now record the intent themselves.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

use super::SCROLL_SETTLE;
use super::row_lists::WINDOW_SIZE;
use crate::observation::Recorder;
use crate::session::{Presented, flush_events, settle};
use crate::{AxiomId, Observation};

/// Children of the first directory node, so the second node lies far outside
/// the realized band (A1).
pub const FIRST_CHILDREN: u32 = 4 * 1024;
/// Children of the second directory node.
pub const SECOND_CHILDREN: u32 = 8;

/// Two directory nodes, the first expanded over [`FIRST_CHILDREN`] entries,
/// shown by a list of labelled tree expanders.
pub struct RowObjectFixture {
    /// The flattened model.
    pub tree: gtk4::TreeListModel,
    /// The list.
    pub list: gtk4::ListView,
    /// Its scroller, the window content.
    pub scroller: gtk4::ScrolledWindow,
}

impl RowObjectFixture {
    /// The flattened position of the second directory node.
    #[must_use]
    pub fn second_position(&self) -> u32 {
        FIRST_CHILDREN + 1
    }

    /// The row object the model hands out for the second directory node now.
    #[must_use]
    pub fn second_row(&self) -> Option<gtk4::TreeListRow> {
        self.tree
            .item(self.second_position())
            .and_downcast::<gtk4::TreeListRow>()
    }

    /// Present the fixture in a window of `WINDOW_SIZE`.
    #[must_use]
    pub(crate) fn present(&self) -> Presented {
        let window = libadwaita::Window::builder()
            .default_width(WINDOW_SIZE.0)
            .default_height(WINDOW_SIZE.1)
            .content(&self.scroller)
            .build();
        let shown = Presented::shown(window);
        settle(SCROLL_SETTLE);
        shown
    }
}

fn numbered(prefix: &str, rows: u32) -> gtk4::gio::ListStore {
    let store = gtk4::gio::ListStore::new::<gtk4::StringObject>();
    let items: Vec<gtk4::StringObject> = (0..rows)
        .map(|index| gtk4::StringObject::new(&format!("{prefix}{index:05}")))
        .collect();
    store.splice(0, 0, &items);
    store
}

/// Build the fixture (not presented).
#[must_use]
pub fn row_object_fixture() -> RowObjectFixture {
    let root = gtk4::gio::ListStore::new::<gtk4::StringObject>();
    root.append(&gtk4::StringObject::new("first"));
    root.append(&gtk4::StringObject::new("second"));
    let first = numbered("child-", FIRST_CHILDREN);
    let second = numbered("leaf-", SECOND_CHILDREN);
    let tree = gtk4::TreeListModel::new(root, false, false, move |item| {
        match item.downcast_ref::<gtk4::StringObject>()?.string().as_str() {
            "first" => Some(first.clone().upcast()),
            "second" => Some(second.clone().upcast()),
            _ => None,
        }
    });
    if let Some(row) = tree.row(0) {
        row.set_expanded(true);
    }
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let expander = gtk4::TreeExpander::new();
        expander.set_child(Some(&gtk4::Label::new(None)));
        item.set_child(Some(&expander));
    });
    factory.connect_bind(|_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let row = item.item().and_downcast::<gtk4::TreeListRow>();
        if let Some(expander) = item.child().and_downcast::<gtk4::TreeExpander>() {
            expander.set_list_row(row.as_ref());
            let name = row
                .as_ref()
                .and_then(gtk4::TreeListRow::item)
                .and_downcast::<gtk4::StringObject>()
                .map(|object| object.string());
            if let Some(label) = expander.child().and_downcast::<gtk4::Label>() {
                label.set_label(name.as_deref().unwrap_or_default());
            }
        }
    });
    factory.connect_unbind(|_, object| {
        if let Some(expander) = object
            .downcast_ref::<gtk4::ListItem>()
            .and_then(gtk4::ListItem::child)
            .and_downcast::<gtk4::TreeExpander>()
        {
            expander.set_list_row(None);
        }
    });
    let list = gtk4::ListView::new(
        Some(gtk4::NoSelection::new(Some(tree.clone()))),
        Some(factory),
    );
    let scroller = gtk4::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .build();
    RowObjectFixture {
        tree,
        list,
        scroller,
    }
}

/// Probe A114. See the module documentation.
#[must_use]
pub fn probe_a114() -> Observation {
    Recorder::run(AxiomId::new(114), |recorder| {
        let fixture = row_object_fixture();
        let shown = fixture.present();
        recorder.control(shown.realized(), "control: the window received a size")?;

        // Control: the first node is bound to a list item, so its row object
        // stays one object across fetches.
        let bound_first = recorder.require(
            fixture.tree.item(0).and_downcast::<gtk4::TreeListRow>(),
            "control: the first directory row",
        )?;
        let bound_again = fixture.tree.item(0).and_downcast::<gtk4::TreeListRow>();
        let bound_stable = bound_again.as_ref() == Some(&bound_first);
        recorder.control(
            bound_stable,
            "control: a row a list item is bound to keeps its object",
        )?;
        drop(bound_again);
        drop(bound_first);

        // The far node: its row object while held, then after every
        // reference is dropped.
        let notified = Rc::new(Cell::new(0u32));
        let held = recorder.require(fixture.second_row(), "control: the second directory row")?;
        let held_name = held
            .item()
            .and_downcast::<gtk4::StringObject>()
            .map(|object| object.string());
        recorder.control(
            held_name.as_deref() == Some("second"),
            "control: the far position shows the second node",
        )?;
        {
            let notified = Rc::clone(&notified);
            held.connect_notify_local(Some("expanded"), move |_, _| {
                notified.set(notified.get() + 1);
            });
        }
        let while_held = fixture.second_row().as_ref() == Some(&held);
        recorder.control(
            while_held,
            "control: a held row object is handed out again while held",
        )?;
        // The handler fires on the object it is connected to (the control
        // that the counter can move).
        held.set_expanded(true);
        held.set_expanded(false);
        let notified_on_held = notified.get();
        recorder.control(
            notified_on_held == 2,
            "control: the handler fires on the held row object",
        )?;
        let weak = held.downgrade();
        drop(held);
        flush_events();
        let died = weak.upgrade().is_none();

        let fresh = recorder.require(fixture.second_row(), "control: the second row again")?;
        let fresh_name = fresh
            .item()
            .and_downcast::<gtk4::StringObject>()
            .map(|object| object.string());
        let same_node = fresh_name.as_deref() == Some("second");
        let before = notified.get();
        fresh.set_expanded(true);
        let reached_dead_handler = notified.get() != before;
        let fresh_expanded = fresh.is_expanded();
        let fresh_weak = fresh.downgrade();
        drop(fresh);
        flush_events();
        let fresh_died = fresh_weak.upgrade().is_none();
        // The node keeps the expansion the dropped object set.
        let third =
            recorder.require(fixture.second_row(), "control: the second row a third time")?;
        let node_kept_expansion = third.is_expanded();
        third.set_expanded(false);
        drop(third);

        recorder.measure(
            "far_row(died,fresh_is_same_node,dead_handler_reached,fresh_expanded,fresh_died,node_kept_expansion)",
            format!(
                "{died},{same_node},{reached_dead_handler},{fresh_expanded},{fresh_died},{node_kept_expansion}"
            ),
        );
        drop(shown);
        flush_events();
        recorder.axiom(
            died && same_node
                && !reached_dead_handler
                && fresh_expanded
                && fresh_died
                && node_kept_expansion,
            "A114: a tree list row object nothing references dies; the next fetch of its node \
             hands out a new object without the dead one's handlers, and the node keeps its \
             expansion",
        )
    })
}
