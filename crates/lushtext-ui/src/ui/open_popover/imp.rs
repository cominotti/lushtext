// SPDX-License-Identifier: GPL-3.0-or-later

//! Open popover GObject implementation: template binding, filtering, signals, and keynav.

use crate::ui::accessibility;
use crate::ui::open_popover::item::OpenPopoverItem;
use crate::ui::open_popover::policy;
use crate::ui::signal_connect;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, gio, glib};
use lushtext_model::persistence::recent_document::RecentDocumentRow;
use lushtext_services::persistence::recent_documents;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;

/// GNOME Text Editor 50.1 keeps the recent-list region capped at 600px.
const RECENT_SCROLLER_MAX_HEIGHT: i32 = 600;
/// GNOME Text Editor 50.1 fixes the recent-list content width to 250px.
const RECENT_SCROLLER_CONTENT_WIDTH: i32 = 250;

/// Boxed path payload carried by `recent-activated` and `remove-requested` signals.
#[derive(Clone, Debug, glib::Boxed)]
#[boxed_type(name = "LushtextOpenPopoverPath")]
pub(super) struct BoxedPath(pub(super) PathBuf);

/// Private GObject implementation for the recent-documents popover.
///
/// GTK binds template children and stores signal-facing state here; public
/// callers use the wrapper in `mod.rs`, while this type stays on the GTK thread.
#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/open-popover.ui")]
pub struct LushtextOpenPopover {
    /// Search entry focused whenever the popover opens.
    #[template_child]
    pub search_entry: TemplateChild<gtk4::SearchEntry>,
    /// Compact button that opens the normal file chooser.
    #[template_child]
    pub chooser_button: TemplateChild<gtk4::Button>,
    /// Stack switching between empty and recent-list states.
    #[template_child]
    pub stack: TemplateChild<gtk4::Stack>,
    /// Empty/no-results state container.
    #[template_child]
    pub empty_state: TemplateChild<gtk4::Box>,
    /// Empty/no-results title so searches can distinguish no recents from no matches.
    #[template_child]
    pub empty_title: TemplateChild<gtk4::Label>,
    /// Scrolled region that owns only the recent rows.
    #[template_child]
    pub recent_scroller: TemplateChild<gtk4::ScrolledWindow>,
    /// Virtualized recent-document list.
    #[template_child]
    pub list_view: TemplateChild<gtk4::ListView>,

    /// Full recent rows after open-tab exclusion, before search filtering.
    pub source_rows: RefCell<Vec<RecentDocumentRow>>,
    /// Filtered row store watched by the ListView.
    pub rows_store: gio::ListStore,
    /// Row position used only for keyboard navigation while the list uses `NoSelection`.
    /// `Cell` lets focus/key signal handlers update it through GObject's shared
    /// `&self`; it is cleared whenever the popover opens or rows are rebound.
    pub keyboard_row_position: Cell<Option<u32>>,
}

impl Default for LushtextOpenPopover {
    fn default() -> Self {
        Self {
            search_entry: TemplateChild::default(),
            chooser_button: TemplateChild::default(),
            stack: TemplateChild::default(),
            empty_state: TemplateChild::default(),
            empty_title: TemplateChild::default(),
            recent_scroller: TemplateChild::default(),
            list_view: TemplateChild::default(),
            source_rows: RefCell::default(),
            rows_store: gio::ListStore::new::<OpenPopoverItem>(),
            keyboard_row_position: Cell::default(),
        }
    }
}

// ObjectSubclass registers the custom GtkPopover type referenced by templates.
#[glib::object_subclass]
impl ObjectSubclass for LushtextOpenPopover {
    const NAME: &str = "LushtextOpenPopover";
    type Type = super::LushtextOpenPopover;
    type ParentType = gtk4::Popover;

    fn class_init(klass: &mut Self::Class) {
        // Register row-item GObject before the list model stores those objects.
        OpenPopoverItem::ensure_type();
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextOpenPopover {
    fn signals() -> &'static [glib::subclass::Signal] {
        use std::sync::OnceLock;
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("open-file-requested").build(),
                glib::subclass::Signal::builder("recent-activated")
                    .param_types([BoxedPath::static_type()])
                    .build(),
                glib::subclass::Signal::builder("remove-requested")
                    .param_types([BoxedPath::static_type()])
                    .build(),
                glib::subclass::Signal::builder("dismissed-from-keyboard").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        crate::ui::entry_paste_cap::cap_single_line_inserts(&*self.search_entry);

        // GNOME Text Editor uses no persistent row selection in this popover.
        // Keep activation and focus navigation, but avoid an accent-colored
        // selected row that would make the Open menu feel unlike GNOME's.
        let selection = gtk4::NoSelection::new(Some(self.rows_store.clone()));
        self.list_view.set_model(Some(&selection));
        self.recent_scroller
            .set_min_content_width(RECENT_SCROLLER_CONTENT_WIDTH);
        self.recent_scroller
            .set_max_content_width(RECENT_SCROLLER_CONTENT_WIDTH);
        self.recent_scroller
            .set_max_content_height(RECENT_SCROLLER_MAX_HEIGHT);

        self.list_view
            .set_factory(Some(&super::item::build_recent_row_factory(&self.obj())));
        self.setup_search();
        self.setup_keyboard();
        self.setup_activation();
        self.setup_open_button();
        self.apply_accessibility_metadata();
        self.refresh_filter();

        let obj_weak = self.obj().downgrade();
        self.obj().connect_show(move |_| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.prepare_to_show();
            }
        });
    }
}

impl WidgetImpl for LushtextOpenPopover {}
impl PopoverImpl for LushtextOpenPopover {}

impl LushtextOpenPopover {
    fn apply_accessibility_metadata(&self) {
        accessibility::set_labelled_description(
            &*self.search_entry,
            "Recent documents search",
            "Filter recently opened documents",
        );
        accessibility::set_labelled_description(
            &*self.chooser_button,
            "Open another file",
            "Open the normal file chooser",
        );
        accessibility::set_role(&*self.list_view, gtk4::AccessibleRole::List);
        accessibility::set_labelled_description(
            &*self.list_view,
            "Recent documents",
            "Recently opened files",
        );
        accessibility::set_labelled_description(
            &*self.recent_scroller,
            "Scrollable recent documents",
            "Shows recent documents within a capped scrolling region",
        );
        accessibility::set_role(&*self.empty_state, gtk4::AccessibleRole::Status);
        accessibility::set_labelled_description(
            &*self.empty_state,
            "No recent documents",
            "No recently opened documents are available",
        );
    }

    fn setup_search(&self) {
        let obj_weak = self.obj().downgrade();
        self.search_entry.connect_changed(move |_| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.imp().refresh_filter();
            }
        });

        let obj_weak = self.obj().downgrade();
        self.search_entry.connect_activate(move |_| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.imp().activate_first();
            }
        });

        let obj_weak = self.obj().downgrade();
        self.search_entry.connect_stop_search(move |_| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.imp().dismiss_from_keyboard();
            }
        });
    }

    fn setup_keyboard(&self) {
        let search_keys = gtk4::EventControllerKey::new();
        search_keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let obj_weak = self.obj().downgrade();
        search_keys.connect_key_pressed(move |_, keyval, _, _| {
            let Some(obj) = obj_weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            match keyval {
                gdk4::Key::Down | gdk4::Key::KP_Down => {
                    obj.imp().focus_first_row();
                    glib::Propagation::Stop
                }
                gdk4::Key::Escape => {
                    obj.imp().dismiss_from_keyboard();
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        self.search_entry.add_controller(search_keys);

        let list_keys = gtk4::EventControllerKey::new();
        // Bubble-phase list keys are the fallback when focus stays on the
        // ListView itself; focused row children are handled earlier in capture.
        list_keys.set_propagation_phase(gtk4::PropagationPhase::Bubble);
        let obj_weak = self.obj().downgrade();
        list_keys.connect_key_pressed(move |_, keyval, _, _| {
            let Some(obj) = obj_weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            match keyval {
                gdk4::Key::Up | gdk4::Key::KP_Up => obj.imp().focus_previous_row_or_search(),
                gdk4::Key::Down | gdk4::Key::KP_Down => obj.imp().focus_next_row(),
                gdk4::Key::Escape => {
                    obj.imp().dismiss_from_keyboard();
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        self.list_view.add_controller(list_keys);
    }

    fn setup_activation(&self) {
        let obj_weak = self.obj().downgrade();
        self.list_view.connect_activate(move |_, position| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.imp().activate_at(position);
            }
        });
    }

    fn setup_open_button(&self) {
        let obj_weak = self.obj().downgrade();
        self.chooser_button.connect_clicked(move |_| {
            let Some(obj) = obj_weak.upgrade() else {
                return;
            };
            obj.popdown();
            signal_connect::emit_void(&obj, "open-file-requested", &[]);
        });
    }

    /// Replace the unfiltered recent rows and immediately rebuild the filtered list.
    ///
    /// Runs on the GTK thread and only mutates in-memory row state.
    pub fn set_source_rows(&self, rows: Vec<RecentDocumentRow>) {
        self.source_rows.replace(rows);
        self.refresh_filter();
    }

    /// Reset search, scroll, and keyboard-navigation state before the popover opens.
    ///
    /// The visible side effect is moving focus back to the search entry.
    pub fn prepare_to_show(&self) {
        self.search_entry.set_text("");
        self.recent_scroller.vadjustment().set_value(0.0);
        self.keyboard_row_position.set(None);
        self.refresh_filter();
        if self.rows_store.n_items() > 0 {
            self.list_view
                .scroll_to(0, gtk4::ListScrollFlags::NONE, None);
        }
        self.search_entry.grab_focus();
    }

    /// Programmatically update the same search entry users type into.
    pub fn set_search_text(&self, query: &str) {
        self.search_entry.set_text(query);
        self.search_entry.grab_focus();
    }

    fn refresh_filter(&self) {
        let query = self.search_entry.text().to_string();
        let rows = recent_documents::search_rows(&self.source_rows.borrow(), &query);
        let items: Vec<OpenPopoverItem> = rows.into_iter().map(OpenPopoverItem::from_row).collect();
        // `splice` swaps the visible model in one notification burst, avoiding
        // transient empty rows while the popover is open and filtered.
        self.rows_store.splice(0, self.rows_store.n_items(), &items);
        // Filtering rebuilds visible row positions, so any synthetic keynav
        // position from the old model would be stale.
        self.keyboard_row_position.set(None);
        if items.is_empty() {
            // Which of the two empty states the user is in is a policy decision:
            // an empty history and an over-filtered list look the same on screen
            // but say different things, so the choice is made in one pure place.
            let copy = policy::empty_state_copy(&query);
            self.empty_title.set_label(copy.title);
            accessibility::set_labelled_description(
                &*self.empty_state,
                copy.accessible_name,
                copy.accessible_description,
            );
            self.stack.set_visible_child(&*self.empty_state);
        } else {
            self.stack.set_visible_child(&*self.recent_scroller);
        }
        self.refresh_accessibility_state(query.as_str(), items.len());
    }

    fn refresh_accessibility_state(&self, query: &str, visible_count: usize) {
        let list_visible = visible_count > 0;
        accessibility::set_hidden(&*self.list_view, !list_visible);
        accessibility::set_hidden(&*self.recent_scroller, !list_visible);
        accessibility::set_hidden(&*self.empty_state, list_visible);

        let value_text = policy::list_value_text(query, visible_count);
        accessibility::set_value_text(&*self.list_view, &value_text);
        accessibility::set_value_text(&*self.empty_state, &value_text);
    }

    fn activate_first(&self) {
        if self.rows_store.n_items() > 0 {
            self.activate_at(0);
        }
    }

    fn activate_at(&self, position: u32) {
        let Some(item) = self
            .rows_store
            .item(position)
            .and_downcast::<OpenPopoverItem>()
        else {
            return;
        };
        self.search_entry.set_text("");
        self.obj().popdown();
        let boxed = BoxedPath(item.path());
        signal_connect::emit_void(&*self.obj(), "recent-activated", &[&boxed]);
    }

    pub(super) fn remove_recent(&self, path: PathBuf) {
        let boxed = BoxedPath(path);
        signal_connect::emit_void(&*self.obj(), "remove-requested", &[&boxed]);
    }

    fn dismiss_from_keyboard(&self) {
        self.obj().popdown();
        signal_connect::emit_void(&*self.obj(), "dismissed-from-keyboard", &[]);
    }

    fn focus_first_row(&self) {
        if self.rows_store.n_items() == 0 {
            return;
        }
        self.keyboard_row_position.set(Some(0));
        self.list_view.grab_focus();
        self.list_view.scroll_to(
            0,
            gtk4::ListScrollFlags::FOCUS | gtk4::ListScrollFlags::SELECT,
            None,
        );
    }

    /// Keep the synthetic keyboard row position aligned when focus lands inside a row.
    ///
    /// This replaces selection state because the list intentionally uses `NoSelection`.
    pub(super) fn sync_keyboard_position_from_focus(&self, list_item: &gtk4::ListItem) {
        let position = list_item.position();
        // GTK can report focus on a recycled ListItem while its position is
        // invalid or outside the current filtered store, so only sync visible rows.
        if position != gtk4::INVALID_LIST_POSITION && position < self.rows_store.n_items() {
            self.keyboard_row_position.set(Some(position));
        }
    }

    /// Handle arrow/Escape keys from a focused row child before the ListView fallback.
    ///
    /// Returns `Stop` only for keys consumed by the navigation contract.
    pub(super) fn handle_focused_row_key(
        &self,
        list_item: &gtk4::ListItem,
        keyval: gdk4::Key,
    ) -> glib::Propagation {
        self.sync_keyboard_position_from_focus(list_item);
        match keyval {
            gdk4::Key::Up | gdk4::Key::KP_Up => self.focus_previous_row_or_search(),
            gdk4::Key::Down | gdk4::Key::KP_Down => self.focus_next_row(),
            gdk4::Key::Escape => {
                self.dismiss_from_keyboard();
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        }
    }

    /// Move keyboard navigation to the previous visible row, or back to search.
    fn focus_previous_row_or_search(&self) -> glib::Propagation {
        let position = self.keyboard_row_position.get().unwrap_or(0);
        if position == 0 {
            self.keyboard_row_position.set(None);
            self.search_entry.grab_focus();
            return glib::Propagation::Stop;
        }
        let previous = position - 1;
        self.keyboard_row_position.set(Some(previous));
        self.list_view
            .scroll_to(previous, gtk4::ListScrollFlags::FOCUS, None);
        glib::Propagation::Stop
    }

    /// Move keyboard navigation to the next visible row without creating selection state.
    fn focus_next_row(&self) -> glib::Propagation {
        let row_count = self.rows_store.n_items();
        if row_count == 0 {
            return glib::Propagation::Proceed;
        }
        let next = self
            .keyboard_row_position
            .get()
            .map_or(0, |position| (position + 1).min(row_count - 1));
        self.keyboard_row_position.set(Some(next));
        self.list_view
            .scroll_to(next, gtk4::ListScrollFlags::FOCUS, None);
        glib::Propagation::Stop
    }
}
