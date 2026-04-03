// SPDX-License-Identifier: GPL-3.0-or-later

use crate::model::palette::{SearchMode, SearchResultItem};
use crate::services::palette::{self, FileIndex};
use crate::ui::command_palette::item::PaletteItem;
use glib::prelude::*;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{self, gio, glib, CompositeTemplate};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::Arc;

/// Owned transport type for search results that can cross thread boundaries.
/// Created on the background thread, converted to `PaletteItem` GObjects
/// on the main thread. At max=50 results, total clone cost is ~15KB — negligible.
enum SearchHit {
    File {
        display_name: String,
        subtitle: String,
        file_path: PathBuf,
    },
    Command {
        display_name: String,
        subtitle: String,
        action_id: String,
    },
}

impl SearchHit {
    fn from_file(f: &crate::model::palette::IndexedFile) -> Self {
        Self::File {
            display_name: f.name.clone(),
            subtitle: f.relative_display(),
            file_path: f.path.clone(),
        }
    }

    fn from_command(c: &crate::model::palette::CommandDef) -> Self {
        Self::Command {
            display_name: c.label.to_string(),
            subtitle: c.display_subtitle(),
            action_id: c.id.to_string(),
        }
    }
}

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
    pub file_index: RefCell<Arc<FileIndex>>,
    pub activate_callback: RefCell<Option<ActivateCallback>>,
    pub close_callback: RefCell<Option<CloseCallback>>,
    pub search_generation: Cell<u32>,
    pub(super) pending_index_updates: RefCell<Vec<super::FileIndexUpdate>>,
    pub(super) index_update_generation: Cell<u32>,
    pub(super) index_update_inflight: Cell<bool>,
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
            file_index: RefCell::new(Arc::new(FileIndex::default())),
            activate_callback: RefCell::default(),
            close_callback: RefCell::default(),
            search_generation: Cell::new(0),
            pending_index_updates: RefCell::default(),
            index_update_generation: Cell::new(0),
            index_update_inflight: Cell::new(false),
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
        let obj_weak = self.obj().downgrade();
        self.search_entry.connect_search_changed(move |entry| {
            let Some(obj) = obj_weak.upgrade() else {
                return;
            };
            let imp = obj.imp();
            let gen = imp.search_generation.get().wrapping_add(1);
            imp.search_generation.set(gen);

            let query = entry.text().to_string();

            // Empty queries bypass debounce for instant clear (expected UX).
            if query.is_empty() {
                imp.rebuild_results_owned(query);
                return;
            }

            let obj_weak = obj.downgrade();
            glib::timeout_add_local_once(std::time::Duration::from_millis(150), move || {
                let Some(obj) = obj_weak.upgrade() else {
                    return;
                };
                if obj.imp().search_generation.get() != gen {
                    return; // superseded by newer keystroke
                }
                obj.imp().rebuild_results_owned(query);
            });
        });
    }

    fn setup_key_controller(&self) {
        let key_controller = gtk4::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);

        let obj_weak = self.obj().downgrade();
        key_controller.connect_key_pressed(move |_, keyval, _, _| {
            let Some(obj) = obj_weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
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
        let obj_weak = self.obj().downgrade();
        self.search_entry.connect_activate(move |_| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.imp().activate_selected();
            }
        });

        // Escape key (stop-search signal) closes the palette
        let obj_weak = self.obj().downgrade();
        self.search_entry.connect_stop_search(move |_| {
            if let Some(obj) = obj_weak.upgrade() {
                if let Some(ref cb) = *obj.imp().close_callback.borrow() {
                    cb();
                }
            }
        });
    }

    fn setup_list_activation(&self) {
        let obj_weak = self.obj().downgrade();
        self.results_view.connect_activate(move |_, position| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.imp().activate_at(position);
            }
        });
    }

    /// Rebuild the results list from the current query and mode.
    /// Runs the SIMD fuzzy search on a background thread to keep the main
    /// thread under the 16ms frame budget, even at 100k indexed files.
    /// Uses `splice` to emit a single `items-changed` signal for the batch.
    ///
    /// Increments `search_generation` to supersede any pending debounced
    /// search from `setup_search`. Direct callers (e.g., `set_file_index`,
    /// `open`, Tab mode-switch) rely on this to cancel stale timers.
    pub fn rebuild_results(&self, query: &str) {
        self.rebuild_results_owned(query.to_string());
    }

    pub fn rebuild_results_owned(&self, query: String) {
        let gen = self.search_generation.get().wrapping_add(1);
        self.search_generation.set(gen);

        let mode = self.mode.get();
        let index = Arc::clone(&self.file_index.borrow());

        crate::services::async_task::spawn_blocking_then(
            self.obj().clone(),
            move || {
                let results = palette::search_all(&index, &query, mode, 50);
                let hits: Vec<SearchHit> = results
                    .iter()
                    .map(|r| match &r.item {
                        SearchResultItem::File(f) => SearchHit::from_file(f),
                        SearchResultItem::Command(c) => SearchHit::from_command(c),
                    })
                    .collect();
                (hits, query)
            },
            move |obj, (hits, query)| {
                let imp = obj.imp();
                if imp.search_generation.get() != gen {
                    return; // superseded by a newer search
                }

                let items: Vec<PaletteItem> = hits
                    .into_iter()
                    .map(|hit| match hit {
                        SearchHit::File {
                            display_name,
                            subtitle,
                            file_path,
                        } => PaletteItem::new_file_raw(display_name, subtitle, file_path),
                        SearchHit::Command {
                            display_name,
                            subtitle,
                            action_id,
                        } => PaletteItem::new_command_raw(display_name, subtitle, action_id),
                    })
                    .collect();

                let old_count = imp.results_store.n_items();
                imp.results_store.splice(0, old_count, &items);

                let has_results = !items.is_empty();
                imp.no_results_label
                    .set_visible(!has_results && !query.is_empty());

                if has_results {
                    if let Some(selection) = imp.selection_model() {
                        selection.set_selected(0);
                    }
                }
            },
        );
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
