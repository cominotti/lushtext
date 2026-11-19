// SPDX-License-Identifier: GPL-3.0-or-later

//! **Called presentation surface** of `WFR-TAB-OVERFLOW-NAVIGATION` — not a
//! role: `LushtextAllTabsPopover`, the searchable list of every open tab
//! that the strip's All Tabs button presents (design D6).
//!
//! It shows a snapshot the window takes (`tab_overflow/all_tabs.rs`), filters
//! and ranks it with the workflow's pure policy (`policy::rank_all_tabs`),
//! and leaves through one callback when a row is activated. It holds no
//! document text, selects no tab, and moves no focus outside itself: the
//! window does all three. Its rows draw text with `LushtextRowText` (a filter
//! rebinds them, ledger A108), its first publish is staged
//! (`ui::list_growth`), and its realized rows are retired in slices once it
//! has closed (`ui::list_retirement`, A110).

mod imp;
pub mod item;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::{gio, glib};

pub use item::AllTabsRow;

glib::wrapper! {
    /// The All Tabs popover of the tab strip.
    pub struct LushtextAllTabsPopover(ObjectSubclass<imp::LushtextAllTabsPopover>)
        @extends gtk4::Popover, gtk4::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk4::Accessible, gtk4::Buildable,
                    gtk4::ConstraintTarget, gtk4::Native, gtk4::ShortcutManager;
}

/// What the workflow's evidence reads of the popover, without creating a row
/// object (`test-utils`; readiness reads `publishing`).
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct AllTabsPopoverFacts {
    /// Rows the current query lists, listed or still growing.
    pub(crate) rows: u32,
    /// Rows GTK is told about.
    pub(crate) listed_rows: u32,
    /// The query's length in characters.
    pub(crate) query_len: u32,
    /// The highlighted row's position.
    pub(crate) highlighted_position: Option<u32>,
    /// Whether a staged first publish still grows.
    pub(crate) growth_pending: bool,
    /// Whether realized rows are still being retired.
    pub(crate) retirement_pending: bool,
    /// Whether the no-match state shows instead of the list.
    pub(crate) empty_state_visible: bool,
    /// Match-count announcements made.
    pub(crate) announcements: u32,
    /// Whether a match-count announcement waits for the query to settle.
    pub(crate) announcement_pending: bool,
    /// Row objects the list model handed out that are still alive.
    pub(crate) row_objects: u32,
}

impl LushtextAllTabsPopover {
    /// Create an unwired popover.
    #[must_use]
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    /// Replace the snapshot of open tabs, in strip order, keeping the query
    /// and the highlighted tab.
    pub(crate) fn set_rows(&self, rows: Vec<AllTabsRow>) {
        self.imp().set_rows(rows);
    }

    /// Before the popover shows: an empty query, the selected tab
    /// highlighted and scrolled to, the search entry focused.
    pub(crate) fn prepare_to_show(&self, rows: Vec<AllTabsRow>) {
        self.imp().prepare_to_show(rows);
    }

    /// Type `query` into the search entry.
    pub(crate) fn set_query(&self, query: &str) {
        self.imp().set_query(query);
    }

    /// Run `callback` with the activated row's tab, `None` when that tab
    /// closed meanwhile; the popover has already closed.
    pub(crate) fn connect_row_activated(
        &self,
        callback: impl Fn(Option<libadwaita::TabPage>) + 'static,
    ) {
        self.imp()
            .activate_callback
            .replace(Some(Box::new(callback)));
    }

    /// Whether the last close followed a row activation.
    pub(crate) fn closed_by_activation(&self) -> bool {
        self.imp().activated.get()
    }

    /// The facts the evidence surface reads.
    #[cfg(feature = "test-utils")]
    pub(crate) fn facts(&self) -> AllTabsPopoverFacts {
        self.imp().facts()
    }

    /// Whether the popover is still publishing or retiring rows.
    pub(crate) fn publishing(&self) -> bool {
        self.imp().publishing()
    }

    /// The search entry, for visual geometry and tests.
    #[must_use]
    pub fn search_entry(&self) -> gtk4::SearchEntry {
        self.imp().search_entry.get()
    }

    /// The scrolled list, for visual geometry and tests.
    #[must_use]
    pub fn list_scroller(&self) -> gtk4::ScrolledWindow {
        self.imp().list_scroller.get()
    }

    /// The list view, for tests that read realized rows.
    #[must_use]
    pub fn list_view(&self) -> gtk4::ListView {
        self.imp().list_view.get()
    }

    /// The no-match state, for visual geometry and tests.
    #[must_use]
    pub fn empty_state(&self) -> gtk4::Box {
        self.imp().empty_state.get()
    }
}

impl Default for LushtextAllTabsPopover {
    fn default() -> Self {
        Self::new()
    }
}
