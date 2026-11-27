// SPDX-License-Identifier: GPL-3.0-or-later

//! All Tabs popover implementation: template binding, the row factory, the
//! filter and highlight, keyboard navigation, and the staged publish and
//! sliced retirement of its rows.

use std::cell::{Cell, RefCell};
use std::time::Duration;

use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{CompositeTemplate, gdk, glib};

use super::super::TOGGLE_ALL_TABS_ACCEL;
use super::super::policy::{
    ALL_TABS_ANNOUNCE_DELAY_MS, ALL_TABS_LIST_DESCRIPTION, ALL_TABS_LIST_LABEL,
    ALL_TABS_NO_MATCH_TITLE, ALL_TABS_SEARCH_DESCRIPTION, ALL_TABS_SEARCH_LABEL, AllTabsRowState,
    all_tabs_highlight, all_tabs_list_value_text, all_tabs_match_announcement, all_tabs_row_name,
};
#[cfg(feature = "test-utils")]
use super::AllTabsPopoverFacts;
use super::item::{
    AllTabsItem, AllTabsRow, AllTabsRowModel, BoundTabRowHandlers, build_all_tabs_row_factory,
    rank_source_rows,
};
use crate::ui::accessibility::{self, AnnouncementLane, AnnouncementThrottler};
use crate::ui::list_growth::RowGrowth;
use crate::ui::list_retirement::RowRetirement;
use crate::ui::row_text::LushtextRowText;

type ActivateCallback = Box<dyn Fn(Option<libadwaita::TabPage>)>;

/// The announcement throttle key of the match count.
const ANNOUNCEMENT_KEY: &str = "all-tabs-results";

#[derive(CompositeTemplate, Default)]
#[template(resource = "/dev/cominotti/lushtext/ui/all-tabs-popover.ui")]
pub struct LushtextAllTabsPopover {
    #[template_child]
    pub search_entry: TemplateChild<gtk4::SearchEntry>,
    #[template_child]
    pub stack: TemplateChild<gtk4::Stack>,
    #[template_child]
    pub empty_state: TemplateChild<gtk4::Box>,
    #[template_child]
    pub empty_title: TemplateChild<gtk4::Label>,
    #[template_child]
    pub list_scroller: TemplateChild<gtk4::ScrolledWindow>,
    #[template_child]
    pub list_view: TemplateChild<gtk4::ListView>,

    /// The window's snapshot of open tabs, in strip order.
    source: RefCell<Vec<AllTabsRow>>,
    model: AllTabsRowModel,
    selection: RefCell<Option<gtk4::SingleSelection>>,
    growth: RefCell<Option<RowGrowth>>,
    retirement: RefCell<Option<RowRetirement>>,
    /// `content-changed` handlers of bound rows, by list-item address.
    pub(super) bound: RefCell<BoundTabRowHandlers>,
    pub(super) activate_callback: RefCell<Option<ActivateCallback>>,
    /// Whether the current popup ended with a row activation.
    pub(super) activated: Cell<bool>,
    announcer: AnnouncementThrottler,
    announcement: gtk_lush_settle::SettleBurst,
    announcements: Cell<u32>,
    /// While `set_rows` or `prepare_to_show` writes the entry, its `changed`
    /// does not re-filter.
    applying_query: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextAllTabsPopover {
    const NAME: &str = "LushtextAllTabsPopover";
    type Type = super::LushtextAllTabsPopover;
    type ParentType = gtk4::Popover;

    fn class_init(klass: &mut Self::Class) {
        AllTabsItem::ensure_type();
        LushtextRowText::ensure_type();
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextAllTabsPopover {
    fn constructed(&self) {
        self.parent_constructed();
        crate::ui::entry_paste_cap::cap_single_line_inserts(&*self.search_entry);
        let selection = gtk4::SingleSelection::new(Some(self.model.clone()));
        selection.set_autoselect(false);
        selection.set_can_unselect(true);
        // The highlight moves while the focus stays in the search entry: the
        // list's value text names the highlighted tab (as the palette does).
        let popover_weak = self.obj().downgrade();
        selection.connect_selected_notify(move |_| {
            if let Some(popover) = popover_weak.upgrade() {
                popover.imp().refresh_list_value_text();
            }
        });
        self.list_view.set_model(Some(&selection));
        self.selection.replace(Some(selection));
        self.list_view
            .set_factory(Some(&build_all_tabs_row_factory(&self.obj())));
        self.apply_accessibility_metadata();
        self.setup_search();
        self.setup_keys();

        let obj = self.obj();
        let popover_weak = obj.downgrade();
        self.list_view.connect_activate(move |_, position| {
            if let Some(popover) = popover_weak.upgrade() {
                popover.imp().activate_at(position);
            }
        });
        let popover_weak = obj.downgrade();
        obj.connect_closed(move |_| {
            if let Some(popover) = popover_weak.upgrade() {
                popover.imp().release_after_close();
            }
        });
    }

    fn dispose(&self) {
        self.bound.borrow_mut().clear();
        let growth = self.growth.take();
        drop(growth);
        let retirement = self.retirement.take();
        drop(retirement);
        let _ = self.announcement.clear();
    }
}

impl WidgetImpl for LushtextAllTabsPopover {}
impl PopoverImpl for LushtextAllTabsPopover {}

impl LushtextAllTabsPopover {
    fn apply_accessibility_metadata(&self) {
        accessibility::set_labelled_description(
            &*self.search_entry,
            ALL_TABS_SEARCH_LABEL,
            ALL_TABS_SEARCH_DESCRIPTION,
        );
        accessibility::set_role(&*self.list_view, gtk4::AccessibleRole::List);
        accessibility::set_labelled_description(
            &*self.list_view,
            ALL_TABS_LIST_LABEL,
            ALL_TABS_LIST_DESCRIPTION,
        );
        accessibility::set_role(&*self.empty_state, gtk4::AccessibleRole::Status);
        accessibility::set_label(&*self.empty_state, ALL_TABS_NO_MATCH_TITLE);
        self.empty_title.set_label(ALL_TABS_NO_MATCH_TITLE);
    }

    fn setup_search(&self) {
        let popover_weak = self.obj().downgrade();
        self.search_entry.connect_changed(move |_| {
            if let Some(popover) = popover_weak.upgrade()
                && !popover.imp().applying_query.get()
            {
                popover.imp().refilter(false);
                popover.imp().schedule_announcement();
            }
        });
        let popover_weak = self.obj().downgrade();
        self.search_entry.connect_activate(move |_| {
            if let Some(popover) = popover_weak.upgrade()
                && let Some(position) = popover.imp().highlighted_position()
            {
                popover.imp().activate_at(position);
            }
        });
        let popover_weak = self.obj().downgrade();
        self.search_entry.connect_stop_search(move |_| {
            if let Some(popover) = popover_weak.upgrade() {
                popover.popdown();
            }
        });
    }

    /// Up and Down move the highlight from the search entry; the shortcut
    /// that opened the popover closes it again.
    fn setup_keys(&self) {
        let entry_keys = gtk4::EventControllerKey::new();
        entry_keys.set_name(Some("all-tabs-highlight-keys"));
        let popover_weak = self.obj().downgrade();
        entry_keys.connect_key_pressed(move |_, keyval, _, _| {
            let Some(popover) = popover_weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            match keyval {
                gdk::Key::Down | gdk::Key::KP_Down => popover.imp().move_highlight(1),
                gdk::Key::Up | gdk::Key::KP_Up => popover.imp().move_highlight(-1),
                _ => glib::Propagation::Proceed,
            }
        });
        self.search_entry.add_controller(entry_keys);

        let toggle_keys = gtk4::EventControllerKey::new();
        toggle_keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let popover_weak = self.obj().downgrade();
        // The accelerator `win.toggle-all-tabs` is bound to, parsed once.
        let shortcut = gtk4::accelerator_parse(TOGGLE_ALL_TABS_ACCEL);
        toggle_keys.connect_key_pressed(move |_, keyval, _, state| {
            let Some(popover) = popover_weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if let Some((key, modifiers)) = shortcut
                && keyval.to_lower() == key.to_lower()
                && state & gtk4::accelerator_get_default_mod_mask() == modifiers
            {
                popover.popdown();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.obj().add_controller(toggle_keys);
    }

    pub(super) fn set_rows(&self, rows: Vec<AllTabsRow>) {
        self.source.replace(rows);
        self.refilter(true);
    }

    pub(super) fn prepare_to_show(&self, rows: Vec<AllTabsRow>) {
        // A retirement still removing the last popup's rows stops; this
        // publish replaces what it left.
        let retirement = self.retirement.take();
        drop(retirement);
        self.activated.set(false);
        self.source.replace(rows);
        self.applying_query.set(true);
        self.search_entry.set_text("");
        self.applying_query.set(false);
        self.list_scroller.vadjustment().set_value(0.0);
        self.refilter(false);
        self.search_entry.grab_focus();
    }

    pub(super) fn set_query(&self, query: &str) {
        self.search_entry.set_text(query);
        self.search_entry.grab_focus();
    }

    /// Rank the snapshot for the current query, publish it, and highlight
    /// the kept tab (`keep_highlight`), the selected tab (empty query) or the
    /// best match.
    fn refilter(&self, keep_highlight: bool) {
        let kept_page = keep_highlight
            .then(|| {
                self.highlighted_position()
                    .and_then(|position| self.model.row_at(position))
                    .and_then(|row| row.page.upgrade())
            })
            .flatten();
        let query = self.search_entry.text().to_string();
        let (listed, rows, selected) = rank_source_rows(&self.source.borrow(), &query);
        let empty = rows.is_empty();
        self.model.publish(rows);
        self.track_growth();
        let highlight = kept_page
            .and_then(|page| self.model.position_of(&page))
            .or_else(|| all_tabs_highlight(query.trim().is_empty(), &listed, selected));
        self.highlight(highlight);
        if empty {
            self.stack.set_visible_child(&*self.empty_state);
        } else {
            self.stack.set_visible_child(&*self.list_scroller);
        }
        accessibility::set_hidden(&*self.list_scroller, empty);
        accessibility::set_hidden(&*self.empty_state, !empty);
        self.refresh_list_value_text();
    }

    /// The list's value text: the highlighted tab's row name, then the match
    /// count. Reads the row data, never a row object.
    fn refresh_list_value_text(&self) {
        // A selection change can still notify while the popover is disposed.
        let (Some(list_view), Some(_)) = (self.list_view.try_get(), self.search_entry.try_get())
        else {
            return;
        };
        let name = self
            .highlighted_position()
            .and_then(|position| self.model.row_at(position))
            .map(|row| {
                all_tabs_row_name(
                    &row.title,
                    AllTabsRowState {
                        modified: row.modified,
                        pinned: row.pinned,
                        current: row.current,
                    },
                )
            });
        accessibility::set_value_text(
            &list_view,
            &all_tabs_list_value_text(name.as_deref(), &self.match_count_text()),
        );
    }

    /// The match count for the current query: the list's value text and the
    /// settled announcement.
    fn match_count_text(&self) -> String {
        let query = self.search_entry.text();
        let total = u32::try_from(self.source.borrow().len()).unwrap_or(u32::MAX);
        all_tabs_match_announcement(self.model.full_len(), total, query.trim().is_empty())
    }

    /// Announce the match count once the query settles, never per keystroke.
    fn schedule_announcement(&self) {
        let _handle = self.announcement.schedule(
            &*self.obj(),
            Duration::from_millis(u64::from(ALL_TABS_ANNOUNCE_DELAY_MS)),
            |popover, handle| {
                let imp = popover.imp();
                let message = imp.match_count_text();
                if imp.announcer.announce_if_allowed(
                    &*imp.search_entry,
                    AnnouncementLane::DebouncedResults,
                    ANNOUNCEMENT_KEY,
                    &message,
                ) {
                    imp.announcements
                        .set(imp.announcements.get().saturating_add(1));
                }
                handle.finish_if_current();
            },
        );
    }

    fn selection(&self) -> Option<gtk4::SingleSelection> {
        self.selection.borrow().clone()
    }

    fn highlighted_position(&self) -> Option<u32> {
        self.selection()
            .map(|selection| selection.selected())
            .filter(|position| *position != gtk4::INVALID_LIST_POSITION)
    }

    /// Highlight `position`, listing every row first when it lies past a
    /// staged publish's listed rows.
    fn highlight(&self, position: Option<u32>) {
        let Some(selection) = self.selection() else {
            return;
        };
        match position {
            Some(position) => {
                if position >= self.model.len() {
                    self.finish_growth();
                }
                selection.set_selected(position);
                self.list_view
                    .scroll_to(position, gtk4::ListScrollFlags::NONE, None);
            }
            None => selection.set_selected(gtk4::INVALID_LIST_POSITION),
        }
    }

    fn move_highlight(&self, delta: i32) -> glib::Propagation {
        self.finish_growth();
        let len = self.model.len();
        if len == 0 {
            return glib::Propagation::Stop;
        }
        let next = match self.highlighted_position() {
            None => 0,
            Some(position) if delta < 0 => position.saturating_sub(1),
            Some(position) => (position + 1).min(len - 1),
        };
        self.highlight(Some(next));
        glib::Propagation::Stop
    }

    fn activate_at(&self, position: u32) {
        let page = self
            .model
            .row_at(position)
            .and_then(|row| row.page.upgrade());
        self.activated.set(true);
        self.obj().popdown();
        if let Some(callback) = self.activate_callback.borrow().as_ref() {
            callback(page);
        }
    }

    fn track_growth(&self) {
        // A list with no frame clock (the popover never shown) has no frame
        // to spare: list it all.
        if !self.list_view.is_realized() {
            self.model.finish_growth();
        }
        let growth = (self.model.growing_rows() > 0)
            .then(|| RowGrowth::start(Box::new(self.model.clone()), &self.list_view));
        let previous = self.growth.replace(growth);
        drop(previous);
    }

    fn finish_growth(&self) {
        if self.model.growing_rows() == 0 {
            return;
        }
        let growth = self.growth.take();
        drop(growth);
        self.model.finish_growth();
    }

    /// After the popover closed: a match count still waiting for the query to
    /// settle describes a list that is gone, so it is dropped; the snapshot
    /// is released (the next popup takes a fresh one); and the realized rows
    /// retire.
    fn release_after_close(&self) {
        let _ = self.announcement.clear();
        let source = self.source.take();
        drop(source);
        self.retire_rows();
    }

    /// Retire the realized rows in slices once the list is unmapped (ledger
    /// A110), never inside the close dispatch.
    fn retire_rows(&self) {
        let growth = self.growth.take();
        drop(growth);
        if self.model.is_empty() {
            return;
        }
        let retirement = RowRetirement::start(
            Box::new(self.model.clone()),
            &self.list_view,
            Box::new(()),
            Box::new(|| {}),
        );
        self.retirement.replace(Some(retirement));
    }

    /// Whether the popover is still publishing or retiring rows: what the
    /// readiness blocker reads, from the same two facts `facts()` reports.
    pub(super) fn publishing(&self) -> bool {
        self.growth_pending() || self.retirement_pending()
    }

    /// Whether a staged first publish still grows.
    pub(super) fn growth_pending(&self) -> bool {
        self.model.growing_rows() > 0
    }

    /// Whether realized rows are still being retired.
    pub(super) fn retirement_pending(&self) -> bool {
        self.retirement
            .borrow()
            .as_ref()
            .is_some_and(RowRetirement::is_retiring)
    }

    #[cfg(feature = "test-utils")]
    pub(super) fn facts(&self) -> AllTabsPopoverFacts {
        let empty_state_visible = self
            .stack
            .try_get()
            .zip(self.empty_state.try_get())
            .is_some_and(|(stack, empty)| {
                stack.visible_child().as_ref() == Some(empty.upcast_ref::<gtk4::Widget>())
            });
        let query_len = self.search_entry.try_get().map_or(0, |entry| {
            u32::try_from(entry.text().chars().count()).unwrap_or(u32::MAX)
        });
        AllTabsPopoverFacts {
            rows: self.model.full_len(),
            listed_rows: self.model.len(),
            query_len,
            highlighted_position: self.highlighted_position(),
            growth_pending: self.growth_pending(),
            retirement_pending: self.retirement_pending(),
            empty_state_visible,
            announcements: self.announcements.get(),
            announcement_pending: self.announcement.pending(),
            row_objects: self.model.live_rows(),
        }
    }
}
