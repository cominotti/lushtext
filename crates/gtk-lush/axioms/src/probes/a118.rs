// SPDX-License-Identifier: MIT OR Apache-2.0

//! A118: on a **mapped**, presented `GtkListView` over a `GtkTreeListModel`,
//! removing rows a list item is bound to destroys those rows' list items
//! (factory `teardown`) **inside the removal's `items-changed`**, and creates
//! fresh ones (factory `setup` and `bind`) for the rows that enter the
//! realized band in the same call. A removed row's list item is never reused
//! for another item: every one of them is finalized. Removing rows **no list
//! item is bound to**, after the band or between the expanded row and the
//! band, destroys and creates none, and leaves the band bound to the same
//! items.
//!
//! The probe expands one directory row over a child store of 256 entries,
//! with 0 or 400 sibling rows after it, and removes the first 16, 64 or 256
//! children in one splice. It then scrolls a 16,384-entry directory (400 rows
//! after it) into its children and removes the unbound child rows after the
//! band and before it. It also records, without asserting, the same 64-row
//! head removal on an **unmapped** list with 400 rows after it, which A110 did
//! not measure, and the layout phase of the frame after a mapped removal
//! (where GTK computes the new rows' styles).
//!
//! LushText's sidebar collapse (`slice-large-directory-collapse` D1/D4) costs
//! the row widgets GTK replaces, not the rows it removes (89 % of a
//! 400:256 collapse in `items-changed`: 62 % destroying, 27 % creating), so it
//! removes the unbound rows first, in chunks, and the bound rows in units of
//! `SIDEBAR_REALIZED_ROWS_PER_FRAME`, one per frame.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::glib;
use gtk4::prelude::*;

use super::SCROLL_SETTLE;
use super::row_lists::{PhaseRecorder, RowCounters, WINDOW_SIZE};
use crate::observation::Recorder;
use crate::session::{Presented, flush_events, settle};
use crate::{AxiomId, Observation};

/// The directory sizes of the bound-row cases.
pub const SMALL_CHILDREN: u32 = 256;
/// The directory size of the unbound-range case.
pub const LARGE_CHILDREN: u32 = 16 * 1024;
/// The sibling rows after the directory.
pub const AFTER_ROWS: u32 = 400;
/// The head removals, in rows.
pub const HEAD_REMOVALS: [u32; 3] = [16, 64, 256];
/// The unmapped head removal, in rows.
pub const UNMAPPED_REMOVAL: u32 = 64;
/// Child rows the unbound case scrolls to before trimming.
pub const SCROLLED_CHILD: u32 = LARGE_CHILDREN / 2;
/// How far the counts may stray from the rows removed (the focus row and the
/// band's edges vary by a few rows between runs).
pub const COUNT_SLACK: u32 = 8;

/// A tree of one directory expanded over `children` entries, then `after`
/// sibling rows, shown by a list whose factory counts its row widgets.
#[derive(Clone)]
pub struct TreeFixture {
    /// The top-level store: the directory, then the siblings.
    pub root: gtk4::gio::ListStore,
    /// The directory's child store.
    pub children: gtk4::gio::ListStore,
    /// The flattened model.
    pub tree: gtk4::TreeListModel,
    /// The list.
    pub list: gtk4::ListView,
    /// Its scroller.
    pub scroller: gtk4::ScrolledWindow,
    /// The window column.
    pub column: gtk4::Box,
    /// Row widget counters.
    pub counters: Rc<RowCounters>,
    /// `setup` emissions seen while a removal ran (inside `items-changed`).
    pub in_call: Rc<Cell<Option<(u32, u32)>>>,
}

fn numbered(prefix: &str, rows: u32) -> gtk4::gio::ListStore {
    let store = gtk4::gio::ListStore::new::<gtk4::StringObject>();
    let items: Vec<gtk4::StringObject> = (0..rows)
        .map(|index| gtk4::StringObject::new(&format!("{prefix}{index:05}")))
        .collect();
    store.splice(0, 0, &items);
    store
}

/// Build the fixture (not yet shown) and expand its directory row.
#[must_use]
pub fn tree_fixture(children: u32, after: u32) -> TreeFixture {
    let root = numbered("after-", after);
    root.insert(0, &gtk4::StringObject::new("directory"));
    let child_store = numbered("child-", children);
    let tree = {
        let child_store = child_store.clone();
        gtk4::TreeListModel::new(root.clone(), false, false, move |item| {
            let name = item.downcast_ref::<gtk4::StringObject>()?.string();
            (name == "directory").then(|| child_store.clone().upcast())
        })
    };
    if let Some(row) = tree.row(0) {
        row.set_expanded(true);
    }
    let counters = Rc::new(RowCounters::default());
    let factory = gtk4::SignalListItemFactory::new();
    {
        let counters = Rc::clone(&counters);
        factory.connect_setup(move |_, object| {
            let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            counters.setups.set(counters.setups.get() + 1);
            let expander = gtk4::TreeExpander::new();
            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            row.append(&gtk4::Image::from_icon_name("text-x-generic-symbolic"));
            row.append(&gtk4::Label::new(None));
            expander.set_child(Some(&row));
            item.set_child(Some(&expander));
        });
    }
    {
        let counters = Rc::clone(&counters);
        factory.connect_teardown(move |_, _| {
            counters.teardowns.set(counters.teardowns.get() + 1);
        });
    }
    {
        let counters = Rc::clone(&counters);
        factory.connect_bind(move |_, object| {
            let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            let Some(row) = item.item().and_downcast::<gtk4::TreeListRow>() else {
                return;
            };
            if let Some(expander) = item.child().and_downcast::<gtk4::TreeExpander>() {
                expander.set_list_row(Some(&row));
                if let Some(label) = expander.child().and_then(|row| row.last_child())
                    && let Some(label) = label.downcast_ref::<gtk4::Label>()
                    && let Some(text) = row.item().and_downcast::<gtk4::StringObject>()
                {
                    label.set_text(&text.string());
                }
            }
            counters.bound.borrow_mut().push(item.downgrade());
        });
    }
    {
        let counters = Rc::clone(&counters);
        factory.connect_unbind(move |_, object| {
            if let Some(expander) = object
                .downcast_ref::<gtk4::ListItem>()
                .and_then(gtk4::ListItem::child)
                .and_downcast::<gtk4::TreeExpander>()
            {
                expander.set_list_row(None::<&gtk4::TreeListRow>);
            }
            counters
                .bound
                .borrow_mut()
                .retain(|bound| bound.upgrade().is_some_and(|bound| &bound != object));
        });
    }
    let selection = gtk4::SingleSelection::new(Some(tree.clone()));
    selection.set_autoselect(false);
    let list = gtk4::ListView::new(Some(selection), Some(factory));
    list.add_css_class("navigation-sidebar");
    let scroller = gtk4::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    column.append(&gtk4::Label::new(Some("header")));
    column.append(&scroller);
    let in_call: Rc<Cell<Option<(u32, u32)>>> = Rc::default();
    TreeFixture {
        root,
        children: child_store,
        tree,
        list,
        scroller,
        column,
        counters,
        in_call,
    }
}

impl TreeFixture {
    /// Present the fixture in a window of [`WINDOW_SIZE`].
    #[must_use]
    pub(crate) fn present(&self) -> Presented {
        let window = libadwaita::Window::builder()
            .default_width(WINDOW_SIZE.0)
            .default_height(WINDOW_SIZE.1)
            .content(&self.column)
            .build();
        let shown = Presented::shown(window);
        settle(SCROLL_SETTLE);
        shown
    }

    /// The bound list items now, with the flattened position each is bound to.
    #[must_use]
    pub fn bound(&self) -> Vec<(u32, gtk4::ListItem)> {
        let mut bound: Vec<(u32, gtk4::ListItem)> = self
            .counters
            .bound
            .borrow()
            .iter()
            .filter_map(glib::WeakRef::upgrade)
            .map(|item| (item.position(), item))
            .filter(|(position, _)| *position != gtk4::INVALID_LIST_POSITION)
            .collect();
        bound.sort_by_key(|(position, _)| *position);
        bound
    }

    /// The child-store indices a list item is bound to (flattened position
    /// minus the directory row), ascending.
    #[must_use]
    pub fn bound_children(&self) -> Vec<u32> {
        let len = self.children.n_items();
        self.bound()
            .into_iter()
            .filter_map(|(position, _)| position.checked_sub(1))
            .filter(|child| *child < len)
            .collect()
    }

    /// The items the band is bound to, by identity.
    #[must_use]
    pub fn bound_items(&self) -> Vec<glib::Object> {
        self.bound()
            .into_iter()
            .filter_map(|(_, item)| {
                item.item()
                    .and_downcast::<gtk4::TreeListRow>()
                    .and_then(|row| row.item())
            })
            .collect()
    }

    /// Remove `count` children from `start` in one splice, counting the row
    /// widgets created and destroyed **inside** the call.
    #[must_use]
    pub fn remove_children(&self, start: u32, count: u32) -> Duration {
        self.counters.reset();
        let started = Instant::now();
        self.children.splice(start, count, &[] as &[glib::Object]);
        let took = started.elapsed();
        self.in_call.set(Some((
            self.counters.setups.get(),
            self.counters.teardowns.get(),
        )));
        took
    }
}

/// What one head removal on a mapped list did.
#[derive(Clone, Debug, Default)]
pub struct HeadRemoval {
    /// Bound children removed.
    pub bound_removed: u32,
    /// Row widgets created and destroyed inside the call.
    pub in_call: (u32, u32),
    /// Removed rows' list items still alive after the dispatch that follows.
    pub survivors: u32,
    /// The call's time, microseconds.
    pub call_us: u64,
    /// The next frame's layout phase, microseconds.
    pub next_layout_us: Option<u64>,
}

fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

/// Remove the first `count` children of a presented 256-entry directory with
/// `after` rows after it.
#[must_use]
pub fn head_removal(count: u32, after: u32) -> HeadRemoval {
    let fixture = tree_fixture(SMALL_CHILDREN, after);
    let _shown = fixture.present();
    let removed: Vec<glib::WeakRef<gtk4::ListItem>> = fixture
        .bound()
        .into_iter()
        .filter(|(position, _)| (1..=count).contains(position))
        .map(|(_, item)| item.downgrade())
        .collect();
    let bound_removed = u32::try_from(removed.len()).unwrap_or(u32::MAX);
    let recorder = PhaseRecorder::new(&fixture.list);
    let call = fixture.remove_children(0, count);
    let in_call = fixture.in_call.get().unwrap_or_default();
    let next_layout_us = recorder
        .as_ref()
        .and_then(|recorder| recorder.next_frame(Duration::from_millis(500)))
        .map(|phases| phases.layout_us);
    flush_events();
    let survivors = removed
        .iter()
        .filter(|weak| weak.upgrade().is_some())
        .count();
    HeadRemoval {
        bound_removed,
        in_call,
        survivors: u32::try_from(survivors).unwrap_or(u32::MAX),
        call_us: micros(call),
        next_layout_us,
    }
}

/// Probe A118. See the module documentation.
#[must_use]
pub fn probe_a118() -> Observation {
    Recorder::run(AxiomId::new(118), |recorder| {
        // Bound rows on a mapped list.
        let mut cases = Vec::new();
        for after in [0, AFTER_ROWS] {
            for count in HEAD_REMOVALS {
                let removal = head_removal(count, after);
                recorder.measure(
                    "head_removal",
                    format!(
                        "after={after} count={count} bound_removed={} in_call(setups,teardowns)={:?} survivors={} call_us={} next_layout_us={:?}",
                        removal.bound_removed,
                        removal.in_call,
                        removal.survivors,
                        removal.call_us,
                        removal.next_layout_us
                    ),
                );
                cases.push((after, count, removal));
            }
        }
        recorder.control(
            cases.iter().all(|(_, count, removal)| {
                removal.bound_removed + COUNT_SLACK >= (*count).min(150)
            }),
            "control: the removed head rows were bound (the band starts at the directory)",
        )?;
        recorder.axiom(
            cases.iter().all(|(_, _, removal)| {
                removal.in_call.1 + COUNT_SLACK >= removal.bound_removed && removal.survivors == 0
            }),
            "A118: removing bound rows destroys their row widgets inside items-changed, and none \
             is reused for another item",
        )?;
        recorder.axiom(
            cases.iter().all(|(after, _, removal)| {
                *after == 0
                    || (removal.in_call.0 + COUNT_SLACK >= removal.bound_removed
                        && removal.in_call.0 <= removal.bound_removed + COUNT_SLACK)
            }),
            "A118: with rows after the directory, the same call creates one fresh row widget per \
             removed bound row, for the rows entering the band",
        )?;

        // Unbound rows, scrolled into the children, mapped.
        let fixture = tree_fixture(LARGE_CHILDREN, AFTER_ROWS);
        let shown = fixture.present();
        recorder.control(shown.realized(), "control: the window received a size")?;
        fixture
            .list
            .scroll_to(SCROLLED_CHILD + 1, gtk4::ListScrollFlags::NONE, None);
        settle(SCROLL_SETTLE);
        let bound_children = fixture.bound_children();
        let (Some(&low), Some(&high)) = (bound_children.first(), bound_children.last()) else {
            return recorder.control(false, "control: the band is in the children");
        };
        recorder.measure("scrolled_band_children", format!("{low}..={high}"));
        recorder.control(
            fixture.list.is_mapped() && low > 1_000 && high + 1_000 < LARGE_CHILDREN,
            "control: the band is in the middle of the children",
        )?;
        let items_before = fixture.bound_items();
        let after_us = micros(fixture.remove_children(high + 1, LARGE_CHILDREN - high - 1));
        flush_events();
        let after_counts = (
            fixture.counters.setups.get(),
            fixture.counters.teardowns.get(),
        );
        // The rows between the directory and the band. GTK may hold the
        // focus row at child 0 (A110), so only rows between held rows go.
        let held: Vec<u32> = fixture
            .bound_children()
            .into_iter()
            .filter(|child| *child < low)
            .collect();
        let first_unbound = held.last().map_or(0, |child| child + 1);
        let before_count = low.saturating_sub(first_unbound);
        let before_us = micros(fixture.remove_children(first_unbound, before_count));
        settle(SCROLL_SETTLE);
        let before_counts = (
            fixture.counters.setups.get(),
            fixture.counters.teardowns.get(),
        );
        let items_after = fixture.bound_items();
        recorder.measure("unbound_after_band_us", after_us);
        recorder.measure("unbound_after_band_counts", format!("{after_counts:?}"));
        recorder.measure("unbound_before_band_rows", before_count);
        recorder.measure("unbound_before_band_us", before_us);
        recorder.measure("unbound_before_band_counts", format!("{before_counts:?}"));
        recorder.measure("band_rows", items_before.len());
        recorder.measure("children_left", fixture.children.n_items());
        recorder.axiom(
            after_counts == (0, 0) && before_counts == (0, 0),
            "A118: removing the unbound rows after and before the band destroys and creates no \
             row widget on a mapped list",
        )?;
        recorder.axiom(
            items_before == items_after,
            "A118: removing the unbound rows leaves the band bound to the same items",
        )?;
        drop(shown);
        drop(fixture);
        flush_events();

        // Head removal with rows after it on an unmapped list (recorded).
        let hidden = tree_fixture(SMALL_CHILDREN, AFTER_ROWS);
        let hidden_shown = hidden.present();
        hidden.scroller.set_visible(false);
        flush_events();
        let unmapped_us = micros(hidden.remove_children(0, UNMAPPED_REMOVAL));
        let unmapped_in_call = hidden.in_call.get().unwrap_or_default();
        flush_events();
        recorder.measure("unmapped_list_mapped", hidden.list.is_mapped());
        recorder.measure("unmapped_head_removal_us", unmapped_us);
        recorder.measure(
            "unmapped_head_removal_in_call",
            format!("{unmapped_in_call:?}"),
        );
        recorder.measure(
            "unmapped_head_removal_after_flush",
            format!(
                "({}, {})",
                hidden.counters.setups.get(),
                hidden.counters.teardowns.get()
            ),
        );
        drop(hidden_shown);
        drop(hidden);
        flush_events();
        Ok(())
    })
}
