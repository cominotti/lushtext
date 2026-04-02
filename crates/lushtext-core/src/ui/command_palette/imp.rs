// SPDX-License-Identifier: GPL-3.0-or-later

use crate::model::palette::{SearchMode, SearchResultItem};
use crate::services::palette::{self, FileIndex};
use crate::ui::command_palette::item::PaletteItem;
use glib::prelude::*;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{self, gio, glib, CompositeTemplate};
use std::cell::{Cell, RefCell};

type ActivateCallback = Box<dyn Fn(&PaletteItem)>;
type CloseCallback = Box<dyn Fn()>;

#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/command-palette.ui")]
pub struct LushtextCommandPalette {
    #[template_child]
    pub search_entry: TemplateChild<gtk4::SearchEntry>,
    #[template_child]
    pub mode_label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub results_view: TemplateChild<gtk4::ListView>,
    #[template_child]
    pub no_results_label: TemplateChild<gtk4::Label>,

    pub mode: Cell<SearchMode>,
    pub results_store: gio::ListStore,
    pub file_index: RefCell<FileIndex>,
    pub activate_callback: RefCell<Option<ActivateCallback>>,
    pub close_callback: RefCell<Option<CloseCallback>>,
}

impl Default for LushtextCommandPalette {
    fn default() -> Self {
        Self {
            search_entry: TemplateChild::default(),
            mode_label: TemplateChild::default(),
            results_view: TemplateChild::default(),
            no_results_label: TemplateChild::default(),
            mode: Cell::new(SearchMode::All),
            results_store: gio::ListStore::new::<PaletteItem>(),
            file_index: RefCell::new(FileIndex::default()),
            activate_callback: RefCell::default(),
            close_callback: RefCell::default(),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextCommandPalette {
    const NAME: &'static str = "LushtextCommandPalette";
    type Type = super::LushtextCommandPalette;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        PaletteItem::ensure_type();
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextCommandPalette {
    fn constructed(&self) {
        self.parent_constructed();

        let selection = gtk4::SingleSelection::new(Some(self.results_store.clone()));
        selection.set_autoselect(true);
        self.results_view.set_model(Some(&selection));

        self.setup_factory();
        self.setup_search();
        self.setup_key_controller();
        self.setup_list_activation();
    }
}

impl WidgetImpl for LushtextCommandPalette {}
impl BoxImpl for LushtextCommandPalette {}

impl LushtextCommandPalette {
    fn setup_factory(&self) {
        let factory = gtk4::SignalListItemFactory::new();

        factory.connect_setup(|_, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("ListItem");

            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            row.set_margin_start(8);
            row.set_margin_end(8);
            row.set_margin_top(4);
            row.set_margin_bottom(4);

            let name_label = gtk4::Label::new(None);
            name_label.set_halign(gtk4::Align::Start);
            name_label.set_hexpand(true);
            name_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);

            let subtitle_label = gtk4::Label::new(None);
            subtitle_label.set_halign(gtk4::Align::End);
            subtitle_label.add_css_class("dim-label");
            subtitle_label.add_css_class("caption");
            subtitle_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);

            row.append(&name_label);
            row.append(&subtitle_label);

            list_item.set_child(Some(&row));
        });

        factory.connect_bind(|_, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("ListItem");
            let item = list_item
                .item()
                .and_downcast::<PaletteItem>()
                .expect("PaletteItem");

            let row = list_item.child().and_downcast::<gtk4::Box>().unwrap();
            let name_label = row.first_child().and_downcast::<gtk4::Label>().unwrap();
            let subtitle_label = name_label
                .next_sibling()
                .and_downcast::<gtk4::Label>()
                .unwrap();

            name_label.set_label(&item.display_name());
            subtitle_label.set_label(&item.subtitle());
        });

        self.results_view.set_factory(Some(&factory));
    }

    fn setup_search(&self) {
        let obj = self.obj().clone();
        self.search_entry.connect_search_changed(move |entry| {
            let query = entry.text();
            obj.imp().rebuild_results(&query);
        });
    }

    fn setup_key_controller(&self) {
        let key_controller = gtk4::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);

        let obj = self.obj().clone();
        key_controller.connect_key_pressed(move |_, keyval, _, _| {
            let imp = obj.imp();
            match keyval {
                gdk4::Key::Tab | gdk4::Key::ISO_Left_Tab => {
                    let next = imp.mode.get().next();
                    imp.mode.set(next);
                    imp.mode_label.set_label(next.label());
                    let query = imp.search_entry.text();
                    imp.rebuild_results(&query);
                    glib::Propagation::Stop
                }
                gdk4::Key::Up | gdk4::Key::KP_Up => {
                    imp.move_selection(-1);
                    glib::Propagation::Stop
                }
                gdk4::Key::Down | gdk4::Key::KP_Down => {
                    imp.move_selection(1);
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });

        self.search_entry.add_controller(key_controller);

        // Enter key activates the selected item
        let obj = self.obj().clone();
        self.search_entry.connect_activate(move |_| {
            obj.imp().activate_selected();
        });

        // Escape key (stop-search signal) closes the palette
        let obj = self.obj().clone();
        self.search_entry.connect_stop_search(move |_| {
            if let Some(ref cb) = *obj.imp().close_callback.borrow() {
                cb();
            }
        });
    }

    fn setup_list_activation(&self) {
        let obj = self.obj().clone();
        self.results_view.connect_activate(move |_, position| {
            obj.imp().activate_at(position);
        });
    }

    /// Rebuild the results list from the current query and mode.
    pub fn rebuild_results(&self, query: &str) {
        let mode = self.mode.get();
        let index = self.file_index.borrow();
        let results = palette::search_all(&index, query, mode, 50);

        self.results_store.remove_all();
        for result in &results {
            let item = match &result.item {
                SearchResultItem::File(f) => PaletteItem::from_indexed_file(f),
                SearchResultItem::Command(c) => PaletteItem::from_command_def(c),
            };
            self.results_store.append(&item);
        }

        let has_results = self.results_store.n_items() > 0;
        self.no_results_label
            .set_visible(!has_results && !query.is_empty());

        // Auto-select first item
        if has_results {
            if let Some(selection) = self.selection_model() {
                selection.set_selected(0);
            }
        }
    }

    fn move_selection(&self, delta: i32) {
        let Some(selection) = self.selection_model() else {
            return;
        };
        let n = self.results_store.n_items();
        if n == 0 {
            return;
        }
        let current = selection.selected();
        let new_pos = if delta > 0 {
            (current + 1).min(n - 1)
        } else {
            current.saturating_sub(1)
        };
        selection.set_selected(new_pos);
        self.results_view
            .scroll_to(new_pos, gtk4::ListScrollFlags::NONE, None);
    }

    pub fn activate_selected(&self) {
        let Some(selection) = self.selection_model() else {
            return;
        };
        self.activate_at(selection.selected());
    }

    fn activate_at(&self, position: u32) {
        let Some(item) = self.results_store.item(position) else {
            return;
        };
        let Some(palette_item) = item.downcast_ref::<PaletteItem>() else {
            return;
        };
        if let Some(ref cb) = *self.activate_callback.borrow() {
            cb(palette_item);
        }
    }

    fn selection_model(&self) -> Option<gtk4::SingleSelection> {
        self.results_view
            .model()
            .and_then(|m| m.downcast::<gtk4::SingleSelection>().ok())
    }
}
