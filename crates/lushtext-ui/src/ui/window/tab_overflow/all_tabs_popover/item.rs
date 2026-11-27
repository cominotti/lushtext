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

use super::super::policy::{
    ALL_TABS_CURRENT_TOOLTIP, ALL_TABS_PINNED_TOOLTIP, ALL_TABS_UNTITLED_SUBTITLE,
    AllTabsCandidate, AllTabsRowState, all_tabs_row_name, rank_all_tabs,
};
use crate::ui::accessibility::{self, RowAccessibility};
use crate::ui::row_text::LushtextRowText;
use crate::ui::signal_connect;

/// Spacing between a row's marker, text and state icons, in sp.
const ROW_SPACING_SP: i32 = 6;
/// A row's vertical margin, in sp.
const ROW_MARGIN_VERTICAL_SP: i32 = 3;

/// `content-changed` handlers of bound rows, by list-item address.
#[derive(Default)]
pub(super) struct BoundTabRowHandlers {
    handlers: HashMap<usize, (AllTabsItem, glib::SignalHandlerId)>,
}

impl BoundTabRowHandlers {
    pub(super) fn insert(
        &mut self,
        owner: usize,
        item: AllTabsItem,
        handler: glib::SignalHandlerId,
    ) {
        if let Some((old, old_handler)) = self.handlers.insert(owner, (item, handler)) {
            old.disconnect(old_handler);
        }
    }

    pub(super) fn remove(&mut self, owner: usize) {
        if let Some((item, handler)) = self.handlers.remove(&owner) {
            item.disconnect(handler);
        }
    }

    pub(super) fn clear(&mut self) {
        for (_, (item, handler)) in self.handlers.drain() {
            item.disconnect(handler);
        }
    }
}

/// The widgets of one row, found from its child by position.
struct RowWidgets {
    marker: LushtextRowText,
    title: LushtextRowText,
    subtitle: LushtextRowText,
    pinned: gtk4::Image,
    current: gtk4::Image,
}

/// Rank and filter `source` rows against `query`.
pub(super) fn rank_source_rows(
    source: &[AllTabsRow],
    query: &str,
) -> (Vec<u32>, Vec<AllTabsRow>, Option<u32>) {
    let candidates: Vec<AllTabsCandidate<'_>> = source
        .iter()
        .map(|row| AllTabsCandidate {
            title: &row.title,
            folder: row.folder.as_deref(),
            folder_path: row.folder_path.as_deref(),
        })
        .collect();
    let listed = rank_all_tabs(query, &candidates);
    let rows: Vec<AllTabsRow> = listed
        .iter()
        .filter_map(|index| source.get(usize::try_from(*index).ok()?).cloned())
        .collect();
    let selected = source
        .iter()
        .position(|row| row.current)
        .and_then(|index| u32::try_from(index).ok());
    (listed, rows, selected)
}

/// Build the recycled `GtkListView` row factory for the All Tabs list.
pub(super) fn build_all_tabs_row_factory(
    owner: &super::LushtextAllTabsPopover,
) -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, object| {
        let Some(list_item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        list_item.set_child(Some(&build_row()));
    });
    let popover_weak = owner.downgrade();
    factory.connect_bind(move |_, object| {
        let Some(list_item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(item) = list_item
            .item()
            .and_then(|item| item.downcast::<AllTabsItem>().ok())
        else {
            return;
        };
        project_row(list_item, &item);
        let handler = item.connect_content_changed({
            let list_item = list_item.downgrade();
            move |item| {
                if let Some(list_item) = list_item.upgrade() {
                    project_row(&list_item, item);
                }
            }
        });
        if let Some(popover) = popover_weak.upgrade() {
            popover
                .imp()
                .bound
                .borrow_mut()
                .insert(list_item.as_ptr() as usize, item, handler);
        }
    });
    let popover_weak = owner.downgrade();
    factory.connect_unbind(move |_, object| {
        let Some(list_item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        if let Some(popover) = popover_weak.upgrade() {
            popover
                .imp()
                .bound
                .borrow_mut()
                .remove(list_item.as_ptr() as usize);
        }
        accessibility::clear_row_accessibility_for_list_item(list_item);
        if let Some(widgets) = list_item.child().as_ref().and_then(row_widgets) {
            widgets.title.set_text("");
            widgets.subtitle.set_text("");
        }
    });
    factory
}

/// One row: the unsaved-changes dot, the title over its folder, then the
/// pinned and current-tab icons.
fn build_row() -> gtk4::Box {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, ROW_SPACING_SP);
    row.set_margin_top(ROW_MARGIN_VERTICAL_SP);
    row.set_margin_bottom(ROW_MARGIN_VERTICAL_SP);
    row.add_css_class("all-tabs-row");
    let marker = LushtextRowText::new();
    marker.set_ellipsize(gtk4::pango::EllipsizeMode::None);
    marker.set_text("•");
    marker.set_valign(gtk4::Align::Center);
    accessibility::set_role(&marker, gtk4::AccessibleRole::Presentation);
    row.append(&marker);
    let text = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    text.set_hexpand(true);
    text.set_valign(gtk4::Align::Center);
    let title = LushtextRowText::new();
    title.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    title.set_xalign(0.0);
    let subtitle = LushtextRowText::new();
    subtitle.set_ellipsize(gtk4::pango::EllipsizeMode::Start);
    subtitle.set_xalign(0.0);
    subtitle.add_css_class("caption");
    subtitle.add_css_class("dim-label");
    text.append(&title);
    text.append(&subtitle);
    row.append(&text);
    let pinned = gtk4::Image::from_icon_name("view-pin-symbolic");
    pinned.set_tooltip_text(Some(ALL_TABS_PINNED_TOOLTIP));
    accessibility::set_role(&pinned, gtk4::AccessibleRole::Presentation);
    row.append(&pinned);
    let current = gtk4::Image::from_icon_name("object-select-symbolic");
    current.set_tooltip_text(Some(ALL_TABS_CURRENT_TOOLTIP));
    accessibility::set_role(&current, gtk4::AccessibleRole::Presentation);
    row.append(&current);
    row
}

fn row_widgets(child: &gtk4::Widget) -> Option<RowWidgets> {
    let marker = child.first_child()?;
    let text = marker.next_sibling()?;
    let pinned = text.next_sibling()?;
    let current = pinned.next_sibling()?;
    let title = text.first_child()?;
    let subtitle = title.next_sibling()?;
    Some(RowWidgets {
        marker: marker.downcast().ok()?,
        title: title.downcast().ok()?,
        subtitle: subtitle.downcast().ok()?,
        pinned: pinned.downcast().ok()?,
        current: current.downcast().ok()?,
    })
}

fn project_row(list_item: &gtk4::ListItem, item: &AllTabsItem) {
    let (Some(row), Some(widgets)) = (item.row(), list_item.child().as_ref().and_then(row_widgets))
    else {
        return;
    };
    // The dot keeps its width when hidden, so titles stay aligned.
    widgets
        .marker
        .set_opacity(if row.modified { 1.0 } else { 0.0 });
    widgets.title.set_text(&row.title);
    widgets.subtitle.set_text(row.subtitle());
    widgets.subtitle.set_tooltip_text(Some(row.subtitle()));
    widgets.pinned.set_visible(row.pinned);
    widgets.current.set_visible(row.current);
    let name = all_tabs_row_name(
        &row.title,
        AllTabsRowState {
            modified: row.modified,
            pinned: row.pinned,
            current: row.current,
        },
    );
    accessibility::apply_list_item_row_accessibility(
        list_item,
        RowAccessibility::new(&name)
            .description(row.subtitle())
            .selected(list_item.is_selected()),
    );
}

/// One open tab as the All Tabs list shows it: no document text.
#[derive(Clone, Debug)]
pub struct AllTabsRow {
    /// The tab this row selects; dead once the tab closed.
    pub page: glib::WeakRef<libadwaita::TabPage>,
    /// The tab's title.
    pub title: String,
    /// The folder of the tab's file as the subtitle shows it (the home
    /// folder shortened to `~`), for the subtitle and the filter; `None` for
    /// an untitled document.
    pub folder: Option<String>,
    /// The same folder as an absolute path, which the filter matches too.
    pub folder_path: Option<String>,
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
        const NAME: &str = "LushtextAllTabsItem";
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
        const NAME: &str = "LushtextAllTabsRowModel";
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
            let first = lushtext_model::budgets::list_budget::staged_first_rows(new_len);
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
            self.truncate_rows(new_len);
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
        let Some(next) = lushtext_model::budgets::list_budget::next_growth(listed, full) else {
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
            self.truncate_rows(listed);
        }
    }

    /// Keep only the first `len` rows the publish holds.
    fn truncate_rows(&self, len: u32) {
        let cut: Rc<[AllTabsRow]> = self
            .imp()
            .rows
            .borrow()
            .iter()
            .take(usize::try_from(len).unwrap_or(usize::MAX))
            .cloned()
            .collect();
        self.imp().rows.replace(cut);
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
