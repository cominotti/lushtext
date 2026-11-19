// SPDX-License-Identifier: GPL-3.0-or-later

//! **Called presentation surface** of `WFR-TAB-OVERFLOW-NAVIGATION` — not a
//! role: the All Tabs popover's row data, the row object its factory binds,
//! and the lazy list model GTK reads.
//!
//! # Why it is lazy
//!
//! `item(position)` builds one small [`AllTabsItem`] when GTK asks for it,
//! and GTK asks only for the realized range, the selected row and an
//! activated one (ledger A93). A publish swaps the listed rows, retargets the
//! row objects GTK holds below the shorter length in place (their
//! `content-changed` signal rebinds them), and emits `items-changed` for the
//! tail alone: a removal or an addition, never one replacing `items-changed`.
//! A first publish into an empty list is staged (`list_budget::staged_first_rows`,
//! grown by `ui::list_growth::RowGrowth`), and the rows are retired in slices
//! after the popover closes (`ui::list_retirement::RowRetirement`).
//!
//! A row holds no document text: a weak reference to its tab page, the
//! title, the folder or the untitled label, and three flags.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

use super::super::policy::ALL_TABS_UNTITLED_SUBTITLE;
use crate::ui::signal_connect;

/// One open tab as the All Tabs list shows it: no document text.
#[derive(Clone, Debug)]
pub struct AllTabsRow {
    /// The tab this row selects; dead once the tab closed.
    pub page: glib::WeakRef<libadwaita::TabPage>,
    /// The tab's title.
    pub title: String,
    /// The folder of the tab's file, for the subtitle and the filter; `None`
    /// for an untitled document.
    pub folder: Option<String>,
    /// Whether the document has unsaved changes.
    pub modified: bool,
    /// Whether the tab is pinned.
    pub pinned: bool,
    /// Whether the tab is the selected one.
    pub current: bool,
}

impl AllTabsRow {
    /// The row's subtitle: the folder, or the untitled label.
    #[must_use]
    pub fn subtitle(&self) -> &str {
        self.folder.as_deref().unwrap_or(ALL_TABS_UNTITLED_SUBTITLE)
    }
}

mod item_imp {
    use super::{AllTabsRow, RefCell, glib};
    use gtk4::subclass::prelude::*;

    #[derive(Default)]
    pub struct AllTabsItem {
        pub(super) row: RefCell<Option<AllTabsRow>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AllTabsItem {
        const NAME: &'static str = "LushtextAllTabsItem";
        type Type = super::AllTabsItem;
    }

    impl ObjectImpl for AllTabsItem {
        fn signals() -> &'static [glib::subclass::Signal] {
            use std::sync::OnceLock;
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| vec![glib::subclass::Signal::builder("content-changed").build()])
        }
    }
}

glib::wrapper! {
    /// One row of the All Tabs list, created on demand.
    pub struct AllTabsItem(ObjectSubclass<item_imp::AllTabsItem>);
}

impl AllTabsItem {
    fn new(row: AllTabsRow) -> Self {
        let item: Self = glib::Object::new();
        item.imp().row.replace(Some(row));
        item
    }

    /// The row this item shows.
    #[must_use]
    pub fn row(&self) -> Option<AllTabsRow> {
        self.imp().row.borrow().clone()
    }

    /// Point this item at another row and tell its bound widgets to project
    /// it again (`content-changed`).
    fn retarget(&self, row: AllTabsRow) {
        self.imp().row.replace(Some(row));
        signal_connect::emit_void(self, "content-changed", &[]);
    }

    /// Run `f` whenever the item is retargeted; the caller disconnects it.
    pub fn connect_content_changed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        signal_connect::connect_void(self, "content-changed", false, move |values| {
            if let Some(item) = values.first().and_then(|value| value.get::<Self>().ok()) {
                f(&item);
            }
        })
    }
}

mod model_imp {
    use super::{AllTabsRow, Cell, HashMap, Rc, RefCell, gio, glib};
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;

    #[derive(Default)]
    pub struct AllTabsRowModel {
        /// The rows a publish lists, in list order.
        pub(super) rows: RefCell<Rc<[AllTabsRow]>>,
        /// While a first publish grows, the rows listed so far; `None` once
        /// every row is listed.
        pub(super) listed: Cell<Option<u32>>,
        /// Row objects handed out, by position.
        pub(super) live: RefCell<HashMap<u32, glib::WeakRef<super::AllTabsItem>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AllTabsRowModel {
        const NAME: &'static str = "LushtextAllTabsRowModel";
        type Type = super::AllTabsRowModel;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for AllTabsRowModel {}

    impl ListModelImpl for AllTabsRowModel {
        fn item_type(&self) -> glib::Type {
            super::AllTabsItem::static_type()
        }

        fn n_items(&self) -> u32 {
            self.listed_len()
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            // Answer by the reported length, as `GListModel` requires: a
            // shrinking publish announces its removal while the old rows are
            // still held, and a growing first publish holds rows it does not
            // list yet.
            if position >= self.listed_len() {
                return None;
            }
            let row = self
                .rows
                .borrow()
                .get(usize::try_from(position).ok()?)?
                .clone();
            let mut live = self.live.borrow_mut();
            if let Some(item) = live.get(&position).and_then(glib::WeakRef::upgrade) {
                return Some(item.upcast());
            }
            if live.len() >= super::LIVE_ROW_PRUNE_AT {
                live.retain(|_, item| item.upgrade().is_some());
            }
            let item = super::AllTabsItem::new(row);
            live.insert(position, item.downgrade());
            Some(item.upcast())
        }
    }

    impl AllTabsRowModel {
        /// The rows GTK is told about.
        pub(super) fn listed_len(&self) -> u32 {
            let full = u32::try_from(self.rows.borrow().len()).unwrap_or(u32::MAX);
            self.listed.get().map_or(full, |listed| listed.min(full))
        }
    }
}

/// Entries in the map of handed-out row objects past which dead ones are
/// pruned; GTK holds about 205 (A1) plus the selection.
///
/// Kind: cap; a power of two, rounded down.
const LIVE_ROW_PRUNE_AT: usize = 1_024;

glib::wrapper! {
    /// The All Tabs list's lazy model.
    pub struct AllTabsRowModel(ObjectSubclass<model_imp::AllTabsRowModel>)
        @implements gio::ListModel;
}

impl Default for AllTabsRowModel {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl AllTabsRowModel {
    /// List `rows`: a staged first publish into an empty list, otherwise a
    /// removal or an addition of the tail and a retarget of the held rows.
    pub fn publish(&self, rows: Vec<AllTabsRow>) {
        let new_len = u32::try_from(rows.len()).unwrap_or(u32::MAX);
        let old_len = self.len();
        if old_len == 0 && new_len > 0 {
            let first = crate::model::list_budget::staged_first_rows(new_len);
            self.imp().rows.replace(Rc::from(rows));
            self.imp().listed.set((first < new_len).then_some(first));
            self.imp().live.borrow_mut().clear();
            self.items_changed(0, 0, first);
            return;
        }
        self.freeze_growth();
        let kept = old_len.min(new_len);
        if old_len > new_len {
            // Remove the tail while the model still lists the old rows cut to
            // the new length, then swap them.
            {
                let cut: Rc<[AllTabsRow]> = self
                    .imp()
                    .rows
                    .borrow()
                    .iter()
                    .take(usize::try_from(new_len).unwrap_or(usize::MAX))
                    .cloned()
                    .collect();
                self.imp().rows.replace(cut);
            }
            self.items_changed(new_len, old_len - new_len, 0);
            self.imp().rows.replace(Rc::from(rows));
        } else {
            self.imp().rows.replace(Rc::from(rows));
            if new_len > old_len {
                self.items_changed(old_len, 0, new_len - old_len);
            }
        }
        self.retarget_live_rows(kept, new_len);
    }

    /// Retarget every live row object below `kept`, and forget those at or
    /// past `len`.
    fn retarget_live_rows(&self, kept: u32, len: u32) {
        let items: Vec<(u32, AllTabsItem)> = {
            let mut live = self.imp().live.borrow_mut();
            live.retain(|position, item| *position < len && item.upgrade().is_some());
            live.iter()
                .filter(|(position, _)| **position < kept)
                .filter_map(|(position, item)| item.upgrade().map(|item| (*position, item)))
                .collect()
        };
        for (position, item) in items {
            let row = self.row_at(position);
            if let Some(row) = row {
                item.retarget(row);
            }
        }
    }

    /// The row at `position`, without creating a row object.
    #[must_use]
    pub fn row_at(&self, position: u32) -> Option<AllTabsRow> {
        self.imp()
            .rows
            .borrow()
            .get(usize::try_from(position).ok()?)
            .cloned()
    }

    /// The position of the row that selects `page`, without creating a row
    /// object: one linear scan.
    #[must_use]
    pub fn position_of(&self, page: &libadwaita::TabPage) -> Option<u32> {
        self.imp()
            .rows
            .borrow()
            .iter()
            .position(|row| row.page.upgrade().as_ref() == Some(page))
            .and_then(|position| u32::try_from(position).ok())
    }

    /// List the next growth step of a staged first publish; `false` once
    /// every row is listed.
    pub fn grow(&self) -> bool {
        let Some(listed) = self.imp().listed.get() else {
            return false;
        };
        let full = self.full_len();
        let Some(next) = crate::model::list_budget::next_growth(listed, full) else {
            self.imp().listed.set(None);
            return false;
        };
        self.imp().listed.set((next < full).then_some(next));
        self.items_changed(listed, 0, next - listed);
        next < full
    }

    /// List every remaining row of a growing first publish at once (a key
    /// that moves the highlight, a highlight past the listed rows).
    pub fn finish_growth(&self) {
        let Some(listed) = self.imp().listed.take() else {
            return;
        };
        let full = self.full_len();
        if full > listed {
            self.items_changed(listed, 0, full - listed);
        }
    }

    /// Rows a growing first publish still has to list.
    #[must_use]
    pub fn growing_rows(&self) -> u32 {
        self.full_len() - self.len()
    }

    /// Rows the publish holds, listed or still growing.
    #[must_use]
    pub fn full_len(&self) -> u32 {
        u32::try_from(self.imp().rows.borrow().len()).unwrap_or(u32::MAX)
    }

    /// Rows listed.
    #[must_use]
    pub fn len(&self) -> u32 {
        self.imp().listed_len()
    }

    /// Row objects handed out and still alive (a read creates none).
    #[must_use]
    pub fn live_rows(&self) -> u32 {
        let live = self.imp().live.borrow();
        u32::try_from(
            live.values()
                .filter(|item| item.upgrade().is_some())
                .count(),
        )
        .unwrap_or(u32::MAX)
    }

    /// Whether nothing is listed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// End a growth where it stands: only the listed rows remain.
    fn freeze_growth(&self) {
        if let Some(listed) = self.imp().listed.take() {
            let cut: Rc<[AllTabsRow]> = self
                .imp()
                .rows
                .borrow()
                .iter()
                .take(usize::try_from(listed).unwrap_or(usize::MAX))
                .cloned()
                .collect();
            self.imp().rows.replace(cut);
        }
    }

    /// Stop listing `count` rows from `start`, as one removal (a
    /// retirement): held row objects past the range move down.
    fn cut_range(&self, start: u32, count: u32) {
        self.freeze_growth();
        let old_len = self.len();
        let start = start.min(old_len);
        let count = count.min(old_len - start);
        if count == 0 {
            return;
        }
        {
            let (from, to) = (
                usize::try_from(start).unwrap_or(usize::MAX),
                usize::try_from(start + count).unwrap_or(usize::MAX),
            );
            let kept: Rc<[AllTabsRow]> = {
                let rows = self.imp().rows.borrow();
                rows.get(..from)
                    .unwrap_or_default()
                    .iter()
                    .chain(rows.get(to..).unwrap_or_default())
                    .cloned()
                    .collect()
            };
            self.imp().rows.replace(kept);
        }
        {
            let mut live = self.imp().live.borrow_mut();
            let shifted = live
                .drain()
                .filter(|(position, _)| *position < start || *position >= start + count)
                .map(|(position, item)| {
                    (
                        if position >= start + count {
                            position - count
                        } else {
                            position
                        },
                        item,
                    )
                })
                .collect();
            *live = shifted;
        }
        self.items_changed(start, count, 0);
    }
}

impl crate::ui::list_retirement::RetiringRows for AllTabsRowModel {
    fn rows_len(&self) -> u32 {
        self.len()
    }

    fn held_positions(&self) -> Vec<u32> {
        crate::ui::list_retirement::held_positions(&self.imp().live.borrow())
    }

    fn remove_range(&self, start: u32, count: u32) {
        self.cut_range(start, count);
    }

    fn release(&self) {
        self.imp().listed.set(None);
        self.imp().rows.replace(Rc::from(Vec::new()));
        self.imp().live.borrow_mut().clear();
    }
}

impl crate::ui::list_growth::GrowingRows for AllTabsRowModel {
    fn grow(&self) -> bool {
        Self::grow(self)
    }
}
