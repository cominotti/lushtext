// SPDX-License-Identifier: GPL-3.0-or-later

//! Open popover GObject implementation: template binding, filtering, and keynav.

use crate::model::recent_document::RecentDocumentRow;
use crate::services::recent_documents;
use crate::ui::open_popover::item::OpenPopoverItem;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, gio, glib};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;

/// GNOME Text Editor 50.1 keeps the recent-list region capped at 600px.
const RECENT_SCROLLER_MAX_HEIGHT: i32 = 600;
/// GNOME Text Editor 50.1 fixes the recent-list content width to 250px.
const RECENT_SCROLLER_CONTENT_WIDTH: i32 = 250;

type OpenFileCallback = Box<dyn Fn()>;
type OpenRecentCallback = Box<dyn Fn(PathBuf)>;
type RemoveRecentCallback = Box<dyn Fn(PathBuf)>;
type DismissCallback = Box<dyn Fn()>;

/// Row subtree shared by production factory setup and test snapshots.
///
/// Building the GNOME-shaped row in one place keeps the visible widget tree and
/// tests that assert parity with GNOME Text Editor's source layout pinned to
/// the same constants.
struct RecentRowWidgets {
    grid: gtk4::Grid,
    #[cfg_attr(
        not(feature = "test-utils"),
        expect(
            dead_code,
            reason = "production uses the widget through the grid; test snapshots inspect it directly"
        )
    )]
    marker_stack: gtk4::Stack,
    #[cfg_attr(
        not(feature = "test-utils"),
        expect(
            dead_code,
            reason = "production uses the widget through the grid; test snapshots inspect it directly"
        )
    )]
    title: gtk4::Inscription,
    #[cfg_attr(
        not(feature = "test-utils"),
        expect(
            dead_code,
            reason = "production uses the widget through the grid; test snapshots inspect it directly"
        )
    )]
    subtitle: gtk4::Inscription,
    #[cfg_attr(
        not(feature = "test-utils"),
        expect(
            dead_code,
            reason = "production uses the widget through the grid; test snapshots inspect it directly"
        )
    )]
    age: gtk4::Inscription,
    remove: gtk4::Button,
}

/// Internal grid attachment tuple captured before conversion to the public row snapshot.
#[cfg(feature = "test-utils")]
pub(super) struct RecentRowChildLayout {
    /// GtkGrid column occupied by the child.
    pub column: i32,
    /// GtkGrid row occupied by the child.
    pub row: i32,
    /// Number of GtkGrid columns spanned by the child.
    pub column_span: i32,
    /// Number of GtkGrid rows spanned by the child.
    pub row_span: i32,
}

/// Internal test-only snapshot of the GNOME-shaped recent-row skeleton.
///
/// Tests capture this before row data is bound so source-parity checks do not
/// depend on GTK's recycled `ListView` rows being visible.
#[cfg(feature = "test-utils")]
pub(super) struct RecentRowLayoutSnapshot {
    /// Top margin on the row grid.
    pub grid_margin_top: i32,
    /// Bottom margin on the row grid.
    pub grid_margin_bottom: i32,
    /// Start margin on the row grid.
    pub grid_margin_start: i32,
    /// End margin on the row grid.
    pub grid_margin_end: i32,
    /// Vertical spacing between title and subtitle rows.
    pub grid_row_spacing: u32,
    /// Horizontal spacing between marker, text, age, and remove columns.
    pub grid_column_spacing: u32,
    /// Height request; GNOME's row does not force a fixed row height.
    pub grid_height_request: i32,
    /// Whether the leading marker/spacer stack is horizontally homogeneous.
    pub marker_hhomogeneous: bool,
    /// Whether the leading marker/spacer stack is vertically homogeneous.
    pub marker_vhomogeneous: bool,
    /// Grid position for the leading marker/spacer stack.
    pub marker_layout: RecentRowChildLayout,
    /// Title overflow mode.
    pub title_overflow: gtk4::InscriptionOverflow,
    /// Title x alignment.
    pub title_xalign: f32,
    /// Whether title can take remaining horizontal room.
    pub title_hexpand: bool,
    /// Grid position for the title.
    pub title_layout: RecentRowChildLayout,
    /// Subtitle overflow mode.
    pub subtitle_overflow: gtk4::InscriptionOverflow,
    /// Subtitle minimum character width.
    pub subtitle_min_chars: u32,
    /// Subtitle natural character width.
    pub subtitle_nat_chars: u32,
    /// Subtitle minimum line count.
    pub subtitle_min_lines: u32,
    /// Subtitle natural line count.
    pub subtitle_nat_lines: u32,
    /// Whether subtitle carries GNOME's caption class.
    pub subtitle_has_caption: bool,
    /// Whether subtitle carries GNOME's dim-label class.
    pub subtitle_has_dim_label: bool,
    /// Grid position for the subtitle.
    pub subtitle_layout: RecentRowChildLayout,
    /// Whether the optional age inscription is visible before binding row data.
    pub age_visible: bool,
    /// Age horizontal alignment.
    pub age_halign: gtk4::Align,
    /// Age vertical alignment.
    pub age_valign: gtk4::Align,
    /// Whether age carries GNOME's caption class.
    pub age_has_caption: bool,
    /// Whether age carries GNOME's dim-label class.
    pub age_has_dim_label: bool,
    /// Grid position for the age inscription.
    pub age_layout: RecentRowChildLayout,
    /// Icon used by the remove button.
    pub remove_icon_name: Option<String>,
    /// Tooltip used by the remove button.
    pub remove_tooltip: Option<String>,
    /// Remove button horizontal alignment.
    pub remove_halign: gtk4::Align,
    /// Remove button vertical alignment.
    pub remove_valign: gtk4::Align,
    /// Whether remove carries GNOME's flat class.
    pub remove_has_flat: bool,
    /// Whether remove carries GNOME's circular class.
    pub remove_has_circular: bool,
    /// Grid position for the remove button.
    pub remove_layout: RecentRowChildLayout,
}

/// Private GObject implementation for the Open popover.
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
    /// Callback for the popover's file chooser button.
    pub open_file_callback: RefCell<Option<OpenFileCallback>>,
    /// Callback for recent row activation.
    pub open_recent_callback: RefCell<Option<OpenRecentCallback>>,
    /// Callback for the compact row remove button.
    pub remove_recent_callback: RefCell<Option<RemoveRecentCallback>>,
    /// Callback after Escape/cancel dismissal so the window can restore focus.
    pub dismiss_callback: RefCell<Option<DismissCallback>>,
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
            open_file_callback: RefCell::default(),
            open_recent_callback: RefCell::default(),
            remove_recent_callback: RefCell::default(),
            dismiss_callback: RefCell::default(),
        }
    }
}

// ObjectSubclass registers the custom GtkPopover type referenced by templates.
#[glib::object_subclass]
impl ObjectSubclass for LushtextOpenPopover {
    const NAME: &'static str = "LushtextOpenPopover";
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
    fn constructed(&self) {
        self.parent_constructed();

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

        self.setup_factory();
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
    /// Build a test-only snapshot of the row skeleton before recycled rows bind data.
    #[cfg(feature = "test-utils")]
    pub(super) fn row_layout_snapshot_for_test() -> RecentRowLayoutSnapshot {
        let row = build_recent_row_widgets();
        RecentRowLayoutSnapshot {
            grid_margin_top: row.grid.margin_top(),
            grid_margin_bottom: row.grid.margin_bottom(),
            grid_margin_start: row.grid.margin_start(),
            grid_margin_end: row.grid.margin_end(),
            grid_row_spacing: row.grid.row_spacing(),
            grid_column_spacing: row.grid.column_spacing(),
            grid_height_request: row.grid.height_request(),
            marker_hhomogeneous: row.marker_stack.is_hhomogeneous(),
            marker_vhomogeneous: row.marker_stack.is_vhomogeneous(),
            marker_layout: child_layout(&row.grid, &row.marker_stack),
            title_overflow: row.title.text_overflow(),
            title_xalign: row.title.xalign(),
            title_hexpand: row.title.hexpands(),
            title_layout: child_layout(&row.grid, &row.title),
            subtitle_overflow: row.subtitle.text_overflow(),
            subtitle_min_chars: row.subtitle.min_chars(),
            subtitle_nat_chars: row.subtitle.nat_chars(),
            subtitle_min_lines: row.subtitle.min_lines(),
            subtitle_nat_lines: row.subtitle.nat_lines(),
            subtitle_has_caption: row.subtitle.has_css_class("caption"),
            subtitle_has_dim_label: row.subtitle.has_css_class("dim-label"),
            subtitle_layout: child_layout(&row.grid, &row.subtitle),
            age_visible: row.age.property::<bool>("visible"),
            age_halign: row.age.halign(),
            age_valign: row.age.valign(),
            age_has_caption: row.age.has_css_class("caption"),
            age_has_dim_label: row.age.has_css_class("dim-label"),
            age_layout: child_layout(&row.grid, &row.age),
            remove_icon_name: row.remove.icon_name().map(Into::into),
            remove_tooltip: row.remove.tooltip_text().map(Into::into),
            remove_halign: row.remove.halign(),
            remove_valign: row.remove.valign(),
            remove_has_flat: row.remove.has_css_class("flat"),
            remove_has_circular: row.remove.has_css_class("circular"),
            remove_layout: child_layout(&row.grid, &row.remove),
        }
    }

    fn apply_accessibility_metadata(&self) {
        self.search_entry.update_property(&[
            gtk4::accessible::Property::Label("Recent documents search"),
            gtk4::accessible::Property::Description("Filter recently opened documents"),
        ]);
        self.chooser_button.update_property(&[
            gtk4::accessible::Property::Label("Open another file"),
            gtk4::accessible::Property::Description("Open the normal file chooser"),
        ]);
        self.list_view
            .set_accessible_role(gtk4::AccessibleRole::List);
        self.list_view.update_property(&[
            gtk4::accessible::Property::Label("Recent documents"),
            gtk4::accessible::Property::Description("Recently opened files"),
        ]);
        self.recent_scroller.update_property(&[
            gtk4::accessible::Property::Label("Scrollable recent documents"),
            gtk4::accessible::Property::Description(
                "Shows recent documents within a capped scrolling region",
            ),
        ]);
        self.empty_state
            .update_property(&[gtk4::accessible::Property::Label("No recent documents")]);
    }

    /// Build the recycled `ListView` row factory and wire row focus, keys, removal, and path tooltips.
    ///
    /// `unbind` clears row-level tooltips so GTK row recycling cannot leak a
    /// previous document path into later filtered results.
    fn setup_factory(&self) {
        let factory = gtk4::SignalListItemFactory::new();
        let obj_weak = self.obj().downgrade();

        factory.connect_setup(move |_, list_item| {
            // SignalListItemFactory creates a small reusable row subtree; GTK
            // later binds different OpenPopoverItem objects into the same row.
            // Factory callbacks receive a generic GObject; downcast_ref narrows
            // it to the ListItem type GTK promises for list factories.
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("ListItem");
            // The grid subtree owns focus so keynav still works when focus
            // lands on row children such as the remove button.
            list_item.set_focusable(false);
            let row = build_recent_row_widgets();
            let focus = gtk4::EventControllerFocus::new();
            let list_item_weak = list_item.downgrade();
            let obj_weak_for_focus = obj_weak.clone();
            focus.connect_enter(move |_| {
                let Some(obj) = obj_weak_for_focus.upgrade() else {
                    return;
                };
                let Some(list_item) = list_item_weak.upgrade() else {
                    return;
                };
                obj.imp().sync_keyboard_position_from_focus(&list_item);
            });
            row.grid.add_controller(focus);

            let row_keys = gtk4::EventControllerKey::new();
            // Capture row-child keys before button/list defaults so arrows keep
            // driving the no-selection row navigation contract.
            row_keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
            let list_item_weak = list_item.downgrade();
            let obj_weak_for_keys = obj_weak.clone();
            row_keys.connect_key_pressed(move |_, keyval, _, _| {
                let Some(obj) = obj_weak_for_keys.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                let Some(list_item) = list_item_weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                obj.imp().handle_focused_row_key(&list_item, keyval)
            });
            row.grid.add_controller(row_keys);

            row.remove
                .update_property(&[gtk4::accessible::Property::Label("Remove recent document")]);
            let list_item_weak = list_item.downgrade();
            let obj_weak = obj_weak.clone();
            row.remove.connect_clicked(move |_| {
                let Some(obj) = obj_weak.upgrade() else {
                    return;
                };
                let Some(list_item) = list_item_weak.upgrade() else {
                    return;
                };
                let Some(item) = list_item.item().and_downcast::<OpenPopoverItem>() else {
                    return;
                };
                obj.imp().remove_recent(item.path());
            });

            list_item.set_child(Some(&row.grid));
        });

        factory.connect_bind(|_, list_item| {
            // bind runs whenever GTK reuses a ListItem for a new OpenPopoverItem.
            // Refresh every item-specific row property here, including tooltips.
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("ListItem");
            let item = list_item
                .item()
                .and_downcast::<OpenPopoverItem>()
                .expect("OpenPopoverItem");
            let grid = list_item
                .child()
                .and_downcast::<gtk4::Grid>()
                .expect("recent row grid");
            let title = grid
                .child_at(1, 0)
                .and_downcast::<gtk4::Inscription>()
                .expect("recent row title");
            let subtitle = grid
                .child_at(1, 1)
                .and_downcast::<gtk4::Inscription>()
                .expect("recent row subtitle");
            let age = grid
                .child_at(2, 1)
                .and_downcast::<gtk4::Inscription>()
                .expect("recent row age");

            let path_tooltip = item.path().display().to_string();
            set_recent_row_non_action_tooltip(&grid, Some(&path_tooltip));
            title.set_text(Some(&item.title()));
            subtitle.set_text(Some(&item.subtitle()));
            match item.age_label() {
                Some(label) if !label.is_empty() => {
                    age.set_text(Some(&label));
                    age.set_visible(true);
                }
                _ => age.set_visible(false),
            }
            grid.update_property(&[gtk4::accessible::Property::Label(&format!(
                "Open recent document {}",
                item.title()
            ))]);
        });

        factory.connect_unbind(|_, list_item| {
            // unbind clears item-specific state before GTK recycles the row for
            // a later filter result.
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("ListItem");
            if let Some(grid) = list_item.child().and_downcast::<gtk4::Grid>() {
                set_recent_row_non_action_tooltip(&grid, None);
            }
        });

        self.list_view.set_factory(Some(&factory));
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
            if let Some(ref cb) = *obj.imp().open_file_callback.borrow() {
                cb();
            }
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

    fn refresh_filter(&self) {
        let query = self.search_entry.text().to_string();
        let rows = recent_documents::search_rows(&self.source_rows.borrow(), &query);
        let items: Vec<OpenPopoverItem> = rows.into_iter().map(OpenPopoverItem::from_row).collect();
        self.rows_store.splice(0, self.rows_store.n_items(), &items);
        // Filtering rebuilds visible row positions, so any synthetic keynav
        // position from the old model would be stale.
        self.keyboard_row_position.set(None);
        if items.is_empty() {
            if query.trim().is_empty() {
                self.empty_title.set_label("No Recent Documents");
                self.empty_state
                    .update_property(&[gtk4::accessible::Property::Label("No recent documents")]);
            } else {
                self.empty_title.set_label("No Matching Documents");
                self.empty_state
                    .update_property(&[gtk4::accessible::Property::Label(
                        "No matching recent documents",
                    )]);
            }
            self.stack.set_visible_child(&*self.empty_state);
        } else {
            self.stack.set_visible_child(&*self.recent_scroller);
        }
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
        if let Some(ref cb) = *self.open_recent_callback.borrow() {
            cb(item.path());
        }
    }

    fn remove_recent(&self, path: PathBuf) {
        if let Some(ref cb) = *self.remove_recent_callback.borrow() {
            cb(path);
        }
    }

    fn dismiss_from_keyboard(&self) {
        self.obj().popdown();
        if let Some(ref cb) = *self.dismiss_callback.borrow() {
            cb();
        }
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
    fn sync_keyboard_position_from_focus(&self, list_item: &gtk4::ListItem) {
        let position = list_item.position();
        // GTK can report focus on a recycled ListItem while its position is
        // invalid or outside the current filtered store, so only sync visible rows.
        if position != gtk4::INVALID_LIST_POSITION && position < self.rows_store.n_items() {
            self.keyboard_row_position.set(Some(position));
        }
    }

    /// Handle arrow/Escape keys from a focused row child before the ListView fallback.
    ///
    /// Returns `Stop` only for keys consumed by the popover navigation contract.
    fn handle_focused_row_key(
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

    #[cfg(feature = "test-utils")]
    /// Report whether the recent list is backed by `NoSelection` for GNOME-style rows.
    pub(super) fn recent_list_uses_no_selection_for_test(&self) -> bool {
        self.list_view
            .model()
            .and_downcast::<gtk4::NoSelection>()
            .is_some()
    }

    #[cfg(feature = "test-utils")]
    /// Expose the synthetic keyboard-navigation row position for widget tests.
    pub(super) fn keyboard_row_position_for_test(&self) -> Option<u32> {
        self.keyboard_row_position.get()
    }
}

/// Build the GNOME Text Editor recent-row skeleton.
///
/// The marker stack and hidden age cell are present even though LushText does
/// not currently show modified markers or ages for every row; source parity
/// depends on preserving those columns so text and close buttons align.
fn build_recent_row_widgets() -> RecentRowWidgets {
    let grid = gtk4::Grid::builder()
        .row_spacing(3)
        .column_spacing(6)
        .focusable(true)
        .margin_top(3)
        .margin_bottom(3)
        .margin_start(0)
        .margin_end(6)
        .build();

    let marker_stack = gtk4::Stack::builder()
        .hhomogeneous(true)
        .vhomogeneous(true)
        .build();
    let empty = gtk4::Label::new(None);
    let is_modified = gtk4::Label::builder()
        .label("•")
        .halign(gtk4::Align::Center)
        .valign(gtk4::Align::Baseline)
        .build();
    marker_stack.add_child(&empty);
    marker_stack.add_child(&is_modified);
    grid.attach(&marker_stack, 0, 0, 1, 1);

    let title = gtk4::Inscription::builder()
        .xalign(0.0)
        .text_overflow(gtk4::InscriptionOverflow::EllipsizeMiddle)
        .hexpand(true)
        .build();
    grid.attach(&title, 1, 0, 2, 1);

    let subtitle = gtk4::Inscription::builder()
        .text_overflow(gtk4::InscriptionOverflow::EllipsizeEnd)
        .min_chars(25)
        .nat_chars(25)
        .hexpand(true)
        .min_lines(1)
        .nat_lines(1)
        .xalign(0.0)
        .build();
    subtitle.add_css_class("caption");
    subtitle.add_css_class("dim-label");
    grid.attach(&subtitle, 1, 1, 1, 1);

    let age = gtk4::Inscription::builder()
        .halign(gtk4::Align::End)
        .valign(gtk4::Align::Center)
        .visible(false)
        .build();
    age.add_css_class("caption");
    age.add_css_class("dim-label");
    grid.attach(&age, 2, 1, 1, 1);

    let remove = gtk4::Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text("Remove")
        .halign(gtk4::Align::End)
        .valign(gtk4::Align::Center)
        .build();
    remove.add_css_class("flat");
    remove.add_css_class("circular");
    grid.attach(&remove, 3, 0, 1, 2);

    RecentRowWidgets {
        grid,
        marker_stack,
        title,
        subtitle,
        age,
        remove,
    }
}

/// Apply the document-path tooltip only to row surfaces that open the document.
///
/// The remove button keeps its own action tooltip, while GTK's recycled list
/// rows get refreshed or cleared every time their bound item changes.
fn set_recent_row_non_action_tooltip(grid: &gtk4::Grid, tooltip: Option<&str>) {
    grid.set_tooltip_text(tooltip);
    // These coordinates mirror build_recent_row_widgets and deliberately skip
    // the remove button at column 3 so it keeps the action tooltip.
    for (column, row) in [(0, 0), (1, 0), (1, 1), (2, 1)] {
        if let Some(child) = grid.child_at(column, row) {
            child.set_tooltip_text(tooltip);
        }
    }
}

#[cfg(feature = "test-utils")]
fn child_layout(grid: &gtk4::Grid, child: &impl IsA<gtk4::Widget>) -> RecentRowChildLayout {
    let (column, row, column_span, row_span) = grid.query_child(child);
    RecentRowChildLayout {
        column,
        row,
        column_span,
        row_span,
    }
}
