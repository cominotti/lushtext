// SPDX-License-Identifier: GPL-3.0-or-later

//! Open popover GObject implementation: template binding, filtering, and keynav.

use crate::model::recent_document::RecentDocumentRow;
use crate::services::recent_documents;
use crate::ui::open_popover::item::OpenPopoverItem;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, gio, glib};
use std::cell::RefCell;
use std::path::PathBuf;

const RECENT_VISIBLE_ROWS: i32 = 10;
const RECENT_ROW_HEIGHT: i32 = 54;

type OpenFileCallback = Box<dyn Fn()>;
type OpenRecentCallback = Box<dyn Fn(PathBuf)>;
type RemoveRecentCallback = Box<dyn Fn(PathBuf)>;
type DismissCallback = Box<dyn Fn()>;

// CompositeTemplate loads the XML generated from `open-popover.blp`; template
// children below are owned by GTK and bound during instance initialization.
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

        let selection = gtk4::SingleSelection::new(Some(self.rows_store.clone()));
        selection.set_autoselect(true);
        self.list_view.set_model(Some(&selection));
        self.recent_scroller
            .set_max_content_height(RECENT_VISIBLE_ROWS * RECENT_ROW_HEIGHT);

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
            gtk4::accessible::Property::Description("Shows ten recent documents before scrolling"),
        ]);
        self.empty_state
            .update_property(&[gtk4::accessible::Property::Label("No recent documents")]);
    }

    fn setup_factory(&self) {
        let factory = gtk4::SignalListItemFactory::new();
        let obj_weak = self.obj().downgrade();

        factory.connect_setup(move |_, list_item| {
            // SignalListItemFactory creates a small reusable row subtree; GTK
            // later binds different OpenPopoverItem objects into the same row.
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("ListItem");
            let grid = gtk4::Grid::builder()
                .row_spacing(3)
                .column_spacing(6)
                .margin_top(3)
                .margin_bottom(3)
                .margin_start(0)
                .margin_end(6)
                .height_request(48)
                .build();

            let title = gtk4::Label::new(None);
            title.set_halign(gtk4::Align::Start);
            title.set_hexpand(true);
            title.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
            grid.attach(&title, 0, 0, 2, 1);

            let subtitle = gtk4::Label::new(None);
            subtitle.set_halign(gtk4::Align::Start);
            subtitle.set_hexpand(true);
            subtitle.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            subtitle.add_css_class("caption");
            subtitle.add_css_class("dim-label");
            grid.attach(&subtitle, 0, 1, 1, 1);

            let age = gtk4::Label::new(None);
            age.set_halign(gtk4::Align::End);
            age.set_valign(gtk4::Align::Center);
            age.add_css_class("caption");
            age.add_css_class("dim-label");
            grid.attach(&age, 1, 1, 1, 1);

            let remove = gtk4::Button::builder()
                .icon_name("window-close-symbolic")
                .tooltip_text("Remove")
                .halign(gtk4::Align::End)
                .valign(gtk4::Align::Center)
                .build();
            remove.add_css_class("flat");
            remove.add_css_class("circular");
            remove.update_property(&[gtk4::accessible::Property::Label("Remove recent document")]);
            let list_item_weak = list_item.downgrade();
            let obj_weak = obj_weak.clone();
            remove.connect_clicked(move |_| {
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
            grid.attach(&remove, 2, 0, 1, 2);

            list_item.set_child(Some(&grid));
        });

        factory.connect_bind(|_, list_item| {
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
                .first_child()
                .and_downcast::<gtk4::Label>()
                .expect("recent row title");
            let subtitle = title
                .next_sibling()
                .and_downcast::<gtk4::Label>()
                .expect("recent row subtitle");
            let age = subtitle
                .next_sibling()
                .and_downcast::<gtk4::Label>()
                .expect("recent row age");

            title.set_label(&item.title());
            subtitle.set_label(&item.subtitle());
            match item.age_label() {
                Some(label) if !label.is_empty() => {
                    age.set_label(&label);
                    age.set_visible(true);
                }
                _ => age.set_visible(false),
            }
            grid.update_property(&[gtk4::accessible::Property::Label(&format!(
                "Open recent document {}",
                item.title()
            ))]);
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
        list_keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let obj_weak = self.obj().downgrade();
        list_keys.connect_key_pressed(move |_, keyval, _, _| {
            let Some(obj) = obj_weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            match keyval {
                gdk4::Key::Up | gdk4::Key::KP_Up if obj.imp().selected_position() == Some(0) => {
                    obj.imp().search_entry.grab_focus();
                    glib::Propagation::Stop
                }
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

    pub fn set_source_rows(&self, rows: Vec<RecentDocumentRow>) {
        self.source_rows.replace(rows);
        self.refresh_filter();
    }

    pub fn prepare_to_show(&self) {
        self.search_entry.set_text("");
        self.recent_scroller.vadjustment().set_value(0.0);
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
        if let Some(selection) = self.selection_model() {
            selection.set_selected(0);
        }
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
        if let Some(selection) = self.selection_model() {
            selection.set_selected(0);
        }
        self.list_view.grab_focus();
        self.list_view.scroll_to(
            0,
            gtk4::ListScrollFlags::FOCUS | gtk4::ListScrollFlags::SELECT,
            None,
        );
    }

    fn selected_position(&self) -> Option<u32> {
        let selected = self.selection_model()?.selected();
        (selected != gtk4::INVALID_LIST_POSITION).then_some(selected)
    }

    fn selection_model(&self) -> Option<gtk4::SingleSelection> {
        self.list_view
            .model()
            .and_downcast::<gtk4::SingleSelection>()
    }
}
