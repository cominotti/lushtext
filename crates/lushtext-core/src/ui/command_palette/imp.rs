// SPDX-License-Identifier: GPL-3.0-or-later

//! Command palette GObject implementation: template binding, search scheduling,
//! and grouped result presentation.

use crate::model::palette::{PaletteFileEntry, PaletteNoteEntry, PaletteSearchRow, SearchMode};
use crate::services::palette::{self, FileIndex};
use crate::ui::accessibility::{self, RowAccessibility};
use crate::ui::command_palette::item::PaletteItem;
use crate::ui::command_palette::runtime::CommandPaletteSearchRequest;
use crate::ui::plain_disposal::DisposalOwned;
use glib::prelude::*;
use gtk_lush_settle::Debounce;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, gio, glib};
use std::cell::{Cell, RefCell};
use std::sync::Arc;

/// Owned transport type for search results that can cross thread boundaries.
///
/// Created on the background thread, then converted to `PaletteItem` GObjects
/// on the main thread. Only owned display data crosses the thread boundary;
/// GTK objects stay on the main thread.
fn palette_row_into_item(row: PaletteSearchRow) -> PaletteItem {
    match row {
        PaletteSearchRow::Header { label } => PaletteItem::new_header_raw(label),
        PaletteSearchRow::File {
            display_name,
            subtitle,
            file_path,
        } => PaletteItem::new_file_raw(display_name, subtitle, file_path),
        PaletteSearchRow::Command {
            display_name,
            subtitle,
            action_id,
        } => PaletteItem::new_command_raw(display_name, subtitle, action_id),
        PaletteSearchRow::Note {
            display_name,
            subtitle,
            target,
        } => PaletteItem::new_note_raw(display_name, subtitle, target),
    }
}

type ActivateCallback = Box<dyn Fn(&PaletteItem)>;
type CloseCallback = Box<dyn Fn()>;

// CompositeTemplate loads the UI layout from a compiled XML file (bundled
// as a GResource). Each #[template_child] is auto-bound to the widget with
// the matching `id` attribute in the XML.
//
// GObject methods always take &self because multiple widgets can hold
// references at once. Cell<T> for Copy types, RefCell<T> for complex types
// (Arc<FileIndex>, callbacks), and Debounce for superseding main-loop work.
/// Implementation object for the template-backed command palette widget.
#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/command-palette.ui")]
pub struct LushtextCommandPalette {
    /// Search entry that receives text, activation, Escape, and key-navigation events.
    #[template_child]
    pub search_entry: TemplateChild<gtk4::SearchEntry>,
    /// Dropdown showing the current search mode ("All", "Files", "Notes", "Commands").
    #[template_child]
    pub mode_dropdown: TemplateChild<gtk4::DropDown>,
    /// ListView that renders result rows backed by `results_store`.
    #[template_child]
    pub results_view: TemplateChild<gtk4::ListView>,
    /// "No results" message shown when a non-empty query has zero matches.
    #[template_child]
    pub no_results_label: TemplateChild<gtk4::Label>,

    /// Current search mode filter (All, Files, Notes, or Commands).
    pub mode: Cell<SearchMode>,
    /// GObject observable list watched by the results ListView.
    ///
    /// Items are `PaletteItem` GObjects, and batch replacement uses `splice()`
    /// so GTK receives one `items-changed` notification.
    pub results_store: gio::ListStore,
    /// Shared file index for fuzzy search. `Arc` allows cloning to background
    /// threads without copying the index.
    pub(super) file_index: RefCell<Arc<DisposalOwned<FileIndex>>>,
    /// Open file-backed tabs supplied by the window shell.
    pub open_tabs: RefCell<Arc<[PaletteFileEntry]>>,
    /// Cached note rows supplied by the window shell after sidecar loading.
    pub(super) note_entries: RefCell<Arc<DisposalOwned<Box<[PaletteNoteEntry]>>>>,
    /// Label for the workspace-indexed file group.
    pub workspace_group_label: RefCell<String>,
    /// Guard used while programmatically syncing the mode dropdown.
    pub syncing_mode_selector: Cell<bool>,
    /// Whether a background fuzzy search is currently pending.
    pub searching: Cell<bool>,
    /// Whether normal source updates may enqueue visible palette work.
    pub palette_open: Cell<bool>,
    /// One-active/one-latest query coordinator shared by direct and debounced entry points.
    pub(super) search_runtime:
        RefCell<palette::PaletteSearchCoordinator<CommandPaletteSearchRequest>>,
    /// Number of workers that cooperatively observed a superseding cancellation.
    pub observed_search_cancellations: Cell<usize>,
    /// Candidate progress retained from the most recent cancelled worker.
    pub last_cancelled_search_examined: Cell<usize>,
    /// Callback invoked when the user activates a result (Enter or click).
    pub activate_callback: RefCell<Option<ActivateCallback>>,
    /// Callback invoked when the palette should close (Escape key).
    pub close_callback: RefCell<Option<CloseCallback>>,
    /// Debounce for non-empty text queries; the runtime coordinator owns freshness.
    pub search_debounce: Debounce,
    /// Queue of incremental index mutations waiting to be flushed.
    pub(super) pending_index_updates: RefCell<Vec<super::FileIndexUpdate>>,
    /// Exact conservative bytes owned by the pending mutation queue.
    pub(super) pending_index_update_bytes: Cell<u64>,
    /// Whether bounded queue overflow requires a filesystem rebuild.
    pub(super) index_update_rebuild_pending: Cell<bool>,
    /// Serializes index clone/mutation workers so results cannot overwrite out of order.
    pub(super) index_update_worker_running: Cell<bool>,
    /// Invalidates a worker result when a full index replacement wins meanwhile.
    pub(super) file_index_generation: Cell<u64>,
    /// Debounce for coalescing index update flushes (75ms).
    pub(super) index_update_debounce: Debounce,
    /// One paced retry after replacement admission reports memory pressure.
    pub(super) index_update_capacity_wakeup: crate::ui::plain_disposal::DisposalCapacityWakeup,
}

impl Default for LushtextCommandPalette {
    fn default() -> Self {
        Self {
            search_entry: TemplateChild::default(),
            mode_dropdown: TemplateChild::default(),
            results_view: TemplateChild::default(),
            no_results_label: TemplateChild::default(),
            mode: Cell::new(SearchMode::All),
            results_store: gio::ListStore::new::<PaletteItem>(),
            file_index: RefCell::new(Arc::new(DisposalOwned::small_unreserved(
                FileIndex::default(),
            ))),
            open_tabs: RefCell::new(Arc::from(Vec::<PaletteFileEntry>::new())),
            note_entries: RefCell::new(Arc::new(DisposalOwned::small_unreserved(
                Vec::<PaletteNoteEntry>::new().into_boxed_slice(),
            ))),
            workspace_group_label: RefCell::new("All Workspaces".to_string()),
            syncing_mode_selector: Cell::new(false),
            searching: Cell::new(false),
            palette_open: Cell::new(false),
            search_runtime: RefCell::default(),
            observed_search_cancellations: Cell::new(0),
            last_cancelled_search_examined: Cell::new(0),
            activate_callback: RefCell::default(),
            close_callback: RefCell::default(),
            search_debounce: Debounce::default(),
            pending_index_updates: RefCell::default(),
            pending_index_update_bytes: Cell::new(0),
            index_update_rebuild_pending: Cell::new(false),
            index_update_worker_running: Cell::new(false),
            file_index_generation: Cell::new(0),
            index_update_debounce: Debounce::default(),
            index_update_capacity_wakeup:
                crate::ui::plain_disposal::DisposalCapacityWakeup::default(),
        }
    }
}

// ObjectSubclass registers this struct with GLib's runtime type system.
// NAME must match the `class` attribute in the UI template XML.
#[glib::object_subclass]
impl ObjectSubclass for LushtextCommandPalette {
    const NAME: &'static str = "LushtextCommandPalette";
    type Type = super::LushtextCommandPalette;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        // Register PaletteItem with GLib before template/class setup needs it;
        // custom GObject types must be known before GTK can store them.
        PaletteItem::ensure_type();
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextCommandPalette {
    fn constructed(&self) {
        // ObjectImpl hosts GObject lifecycle callbacks. constructed() runs
        // after template children are initialized, so models and signals are
        // safe to wire here.
        self.parent_constructed();

        let selection = gtk4::SingleSelection::new(Some(self.results_store.clone()));
        selection.set_autoselect(true);
        let obj_weak = self.obj().downgrade();
        selection.connect_selected_notify(move |_| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.imp().refresh_accessibility_state();
            }
        });
        self.results_view.set_model(Some(&selection));

        self.setup_factory();
        self.setup_mode_selector();
        self.setup_search();
        self.setup_key_controller();
        self.setup_list_activation();
        self.apply_accessibility_metadata();
    }

    fn dispose(&self) {
        self.index_update_capacity_wakeup.cancel();
    }
}

impl WidgetImpl for LushtextCommandPalette {}
impl BoxImpl for LushtextCommandPalette {}

impl LushtextCommandPalette {
    /// Give the palette's compact controls durable accessible names for screen
    /// readers and AT-SPI smoke assertions.
    fn apply_accessibility_metadata(&self) {
        accessibility::set_labelled_description(
            &*self.search_entry,
            "Command palette query",
            "Search open tabs, workspace files, notes, and commands",
        );
        accessibility::set_labelled_description(
            &*self.mode_dropdown,
            "Command palette mode",
            "Choose which result category to search",
        );
        accessibility::set_value_text(&*self.mode_dropdown, self.mode.get().label());
        accessibility::set_role(&*self.results_view, gtk4::AccessibleRole::List);
        accessibility::set_labelled_description(
            &*self.results_view,
            "Command palette results",
            "Matching files, notes, and commands",
        );
        accessibility::set_role(&*self.no_results_label, gtk4::AccessibleRole::Status);
        accessibility::set_labelled_description(
            &*self.no_results_label,
            "Command palette no results",
            "No matching files, notes, or commands",
        );
        self.refresh_accessibility_state();
    }

    fn setup_factory(&self) {
        let factory = gtk4::SignalListItemFactory::new();

        factory.connect_setup(|_, list_item| {
            // SignalListItemFactory uses GObject signals to set up and bind
            // recycled rows. GTK passes rows as generic Objects, so handlers
            // downcast before attaching or updating row widgets.
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

            let section_separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
            section_separator.set_hexpand(true);
            section_separator.set_visible(false);

            let subtitle_label = gtk4::Label::new(None);
            subtitle_label.set_halign(gtk4::Align::End);
            subtitle_label.add_css_class("dim-label");
            subtitle_label.add_css_class("caption");
            subtitle_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);

            row.append(&name_label);
            row.append(&section_separator);
            row.append(&subtitle_label);

            list_item.set_child(Some(&row));
        });

        let bind_obj_weak = self.obj().downgrade();
        factory.connect_bind(move |_, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("ListItem");
            let item = list_item
                .item()
                .and_downcast::<PaletteItem>()
                .expect("PaletteItem");

            let row = list_item
                .child()
                .and_downcast::<gtk4::Box>()
                .expect("palette row child should be the layout box");
            let name_label = row
                .first_child()
                .and_downcast::<gtk4::Label>()
                .expect("palette row should start with the title label");
            let section_separator = name_label
                .next_sibling()
                .and_downcast::<gtk4::Separator>()
                .expect("palette row title should be followed by the section separator");
            let subtitle_label = section_separator
                .next_sibling()
                .and_downcast::<gtk4::Label>()
                .expect("palette row separator should be followed by the subtitle label");

            list_item.set_activatable(item.is_activatable());
            list_item.set_selectable(item.is_activatable());
            name_label.set_label(&item.display_name());
            if item.is_header() {
                row.add_css_class("command-palette-section-row");
                name_label.add_css_class("command-palette-section-header");
                name_label.set_hexpand(false);
                name_label.set_ellipsize(gtk4::pango::EllipsizeMode::None);
                section_separator.set_visible(true);
                subtitle_label.set_visible(false);
            } else {
                row.remove_css_class("command-palette-section-row");
                name_label.remove_css_class("command-palette-section-header");
                name_label.set_hexpand(true);
                name_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
                section_separator.set_visible(false);
                subtitle_label.set_visible(true);
                subtitle_label.set_label(&item.subtitle());
            }

            let position = i32::try_from(list_item.position()).unwrap_or(i32::MAX - 1) + 1;
            let set_size = bind_obj_weak.upgrade().map_or(position, |obj| {
                i32::try_from(obj.imp().results_store.n_items()).unwrap_or(i32::MAX)
            });
            let selected = bind_obj_weak
                .upgrade()
                .and_then(|obj| obj.imp().selection_model())
                .is_some_and(|selection| selection.selected() == list_item.position());
            apply_palette_row_accessibility(&row, &item, selected, position, set_size);
        });

        factory.connect_unbind(|_, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("ListItem");
            if let Some(row) = list_item.child().and_downcast::<gtk4::Box>() {
                accessibility::clear_row_accessibility(&row);
            }
        });

        self.results_view.set_factory(Some(&factory));
    }

    /// Wire the dropdown to the same mode state that Tab cycling uses.
    fn setup_mode_selector(&self) {
        let model = gtk4::StringList::new(SearchMode::labels());
        self.mode_dropdown.set_model(Some(&model));
        self.mode_dropdown.set_selected(self.mode.get().position());

        let obj_weak = self.obj().downgrade();
        self.mode_dropdown.connect_selected_notify(move |dropdown| {
            let Some(obj) = obj_weak.upgrade() else {
                return;
            };
            let imp = obj.imp();
            if imp.syncing_mode_selector.get() {
                return;
            }
            imp.set_mode(SearchMode::from_position(dropdown.selected()));
            let query = imp.search_entry.text();
            imp.rebuild_results(&query);
            imp.search_entry.grab_focus();
        });
    }

    fn setup_search(&self) {
        let obj_weak = self.obj().downgrade();
        self.search_entry.connect_search_changed(move |entry| {
            let Some(obj) = obj_weak.upgrade() else {
                return;
            };
            let imp = obj.imp();
            if !imp.palette_open.get() {
                return;
            }
            let query = entry.text().to_string();

            // Empty queries bypass debounce so default results update
            // immediately when the query is cleared.
            if query.is_empty() {
                imp.rebuild_results_owned(query);
                return;
            }

            imp.search_debounce.schedule(
                &obj,
                std::time::Duration::from_millis(150),
                move |obj, _| {
                    obj.imp().rebuild_results_owned(query);
                },
            );
        });
    }

    fn setup_key_controller(&self) {
        let key_controller = gtk4::EventControllerKey::new();
        // Capture phase lets palette navigation consume Return/Escape/arrow
        // keys before embedded list widgets can turn them into unrelated focus moves.
        key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);

        let obj_weak = self.obj().downgrade();
        key_controller.connect_key_pressed(move |_, keyval, _, _| {
            let Some(obj) = obj_weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let imp = obj.imp();
            match keyval {
                gdk4::Key::Tab => {
                    imp.set_mode(imp.mode.get().next());
                    let query = imp.search_entry.text();
                    imp.rebuild_results(&query);
                    glib::Propagation::Stop
                }
                gdk4::Key::ISO_Left_Tab => {
                    imp.set_mode(imp.mode.get().previous());
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

        let obj_weak = self.obj().downgrade();
        self.search_entry.connect_activate(move |_| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.imp().activate_selected();
            }
        });

        let obj_weak = self.obj().downgrade();
        self.search_entry.connect_stop_search(move |_| {
            if let Some(obj) = obj_weak.upgrade()
                && let Some(ref cb) = *obj.imp().close_callback.borrow()
            {
                cb();
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
    /// Advances `search_debounce` to supersede any pending debounced search from
    /// `setup_search`. Direct callers (e.g., `set_file_index`, `open`, Tab
    /// mode-switch) rely on this to cancel stale timers.
    pub fn rebuild_results(&self, query: &str) {
        self.rebuild_results_owned(query.to_string());
    }

    /// Rebuild results from an owned query snapshot.
    ///
    /// Direct and debounced callers replace the same latest request. At most
    /// one worker owns source snapshots while one compact request waits.
    pub fn rebuild_results_owned(&self, query: String) {
        let _ = self.search_debounce.advance();
        let request = CommandPaletteSearchRequest {
            query: Arc::from(query),
            mode: self.mode.get(),
            index: Arc::clone(&self.file_index.borrow()),
            open_tabs: Arc::clone(&self.open_tabs.borrow()),
            note_entries: Arc::clone(&self.note_entries.borrow()),
            workspace_group_label: Arc::from(self.workspace_group_label.borrow().as_str()),
        };
        let start = self.search_runtime.borrow_mut().submit(request);
        if let Some(start) = start {
            self.spawn_search(start);
        }
        self.refresh_searching_state();
    }

    fn spawn_search(&self, start: palette::PaletteSearchStart<CommandPaletteSearchRequest>) {
        let generation = start.generation;
        let cancellation = start.cancellation;
        let request = start.request;
        gtk_lush_tasks::spawn_blocking_then(
            self.obj().clone(),
            move || {
                let outcome =
                    super::runtime::execute_search(&request, &cancellation, MAX_RESULTS_PER_SOURCE);
                (outcome, request.query)
            },
            move |obj, (outcome, query)| {
                let imp = obj.imp();
                let (is_current, next) = {
                    let mut runtime = imp.search_runtime.borrow_mut();
                    let is_current = runtime.is_current(generation);
                    let next = runtime.finish(generation);
                    (is_current, next)
                };

                match outcome {
                    palette::PaletteSearchOutcome::Complete { value, .. } if is_current => {
                        imp.apply_search_rows(value, &query);
                    }
                    palette::PaletteSearchOutcome::Cancelled { metrics } => {
                        imp.observed_search_cancellations
                            .set(imp.observed_search_cancellations.get().saturating_add(1));
                        imp.last_cancelled_search_examined
                            .set(metrics.candidates_examined);
                    }
                    palette::PaletteSearchOutcome::Complete { .. } => {}
                }

                if let Some(next) = next {
                    imp.spawn_search(next);
                }
                imp.refresh_searching_state();
            },
        );
    }

    fn apply_search_rows(&self, rows: Vec<PaletteSearchRow>, query: &str) {
        let items: Vec<PaletteItem> = rows.into_iter().map(palette_row_into_item).collect();

        // One splice keeps GTK projection work independent of the match count.
        let old_count = self.results_store.n_items();
        self.results_store.splice(0, old_count, &items);

        let has_results = items.iter().any(PaletteItem::is_activatable);
        self.no_results_label
            .set_visible(!has_results && !query.is_empty());

        if let Some(first_result) = self.first_activatable_position()
            && let Some(selection) = self.selection_model()
        {
            selection.set_selected(first_result);
        }
        self.refresh_accessibility_state();
    }

    pub(super) fn refresh_searching_state(&self) {
        let searching = self.palette_open.get() && self.search_runtime.borrow().has_work();
        self.searching.set(searching);
        self.refresh_accessibility_state();
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
        let Some(new_pos) = self.next_activatable_position(current, delta) else {
            return;
        };
        selection.set_selected(new_pos);
        self.results_view
            .scroll_to(new_pos, gtk4::ListScrollFlags::NONE, None);
        self.refresh_accessibility_state();
    }

    /// Activate the currently selected actionable row.
    ///
    /// Presentation-only source headers are ignored before forwarding the row
    /// to the registered activation callback.
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
        if !palette_item.is_activatable() {
            return;
        }
        if let Some(ref cb) = *self.activate_callback.borrow() {
            cb(palette_item);
        }
    }

    /// Update the active search mode and sync all dependent widgets.
    pub fn set_mode(&self, mode: SearchMode) {
        self.mode.set(mode);
        self.syncing_mode_selector.set(true);
        self.mode_dropdown.set_selected(mode.position());
        self.syncing_mode_selector.set(false);
        self.search_entry
            .set_placeholder_text(Some(mode.placeholder()));
        accessibility::set_value_text(&*self.mode_dropdown, mode.label());
        self.refresh_accessibility_state();
    }

    /// Find the first row that can actually be opened or executed.
    fn first_activatable_position(&self) -> Option<u32> {
        (0..self.results_store.n_items()).find(|position| self.position_is_activatable(*position))
    }

    /// Move keyboard selection across result rows while skipping source headers.
    fn next_activatable_position(&self, current: u32, delta: i32) -> Option<u32> {
        let n = self.results_store.n_items();
        if n == 0 {
            return None;
        }
        if current >= n {
            return self.first_activatable_position();
        }

        if delta > 0 {
            let mut position = current.saturating_add(1);
            while position < n {
                if self.position_is_activatable(position) {
                    return Some(position);
                }
                position = position.saturating_add(1);
            }
        } else {
            let mut position = current.saturating_sub(1);
            loop {
                if self.position_is_activatable(position) {
                    return Some(position);
                }
                if position == 0 {
                    break;
                }
                position = position.saturating_sub(1);
            }
        }

        Some(current).filter(|position| self.position_is_activatable(*position))
    }

    /// Check whether a row should receive keyboard focus and activation.
    fn position_is_activatable(&self, position: u32) -> bool {
        self.results_store
            .item(position)
            .and_downcast_ref::<PaletteItem>()
            .is_some_and(PaletteItem::is_activatable)
    }

    fn selection_model(&self) -> Option<gtk4::SingleSelection> {
        self.results_view
            .model()
            .and_then(|m| m.downcast::<gtk4::SingleSelection>().ok())
    }

    /// Project live palette search state into accessible states and values.
    pub(super) fn refresh_accessibility_state(&self) {
        let has_results = (0..self.results_store.n_items()).any(|position| {
            self.results_store
                .item(position)
                .and_downcast_ref::<PaletteItem>()
                .is_some_and(PaletteItem::is_activatable)
        });
        let no_results_visible =
            self.no_results_label.is_visible() && !has_results && !self.searching.get();
        let result_count = (0..self.results_store.n_items())
            .filter(|position| {
                self.results_store
                    .item(*position)
                    .and_downcast_ref::<PaletteItem>()
                    .is_some_and(PaletteItem::is_activatable)
            })
            .count();

        accessibility::set_busy(&*self.search_entry, self.searching.get());
        accessibility::set_busy(&*self.results_view, self.searching.get());
        accessibility::set_hidden(&*self.no_results_label, !no_results_visible);
        accessibility::set_hidden(&*self.results_view, no_results_visible);

        let selected_text = self
            .selection_model()
            .and_then(|selection| self.results_store.item(selection.selected()))
            .and_downcast::<PaletteItem>()
            .filter(PaletteItem::is_activatable)
            .map(|item| format!("Selected {}", item.display_name()));
        let value_text = if let Some(selected_text) = selected_text {
            selected_text
        } else if self.searching.get() {
            "Searching command palette".to_string()
        } else {
            match result_count {
                0 => "No command palette results".to_string(),
                1 => "1 command palette result".to_string(),
                count => format!("{count} command palette results"),
            }
        };
        accessibility::set_value_text(&*self.results_view, &value_text);
        accessibility::set_value_text(&*self.no_results_label, "No command palette results");
    }
}

pub(super) fn apply_palette_row_accessibility(
    row: &gtk4::Box,
    item: &PaletteItem,
    selected: bool,
    position: i32,
    set_size: i32,
) {
    let label = if item.is_header() {
        format!("{} section", item.display_name())
    } else if item.is_file() {
        format!("Open file {}", item.display_name())
    } else if item.is_note() {
        format!("Open note {}", item.display_name())
    } else {
        format!("Run command {}", item.display_name())
    };
    let description = if item.is_header() {
        "Command palette result group".to_string()
    } else if let Some(path) = item.file_path() {
        format!("{}; {}", item.subtitle(), path.display())
    } else {
        item.subtitle()
    };
    accessibility::apply_row_accessibility(
        row,
        RowAccessibility::new(&label)
            .description(&description)
            .selected(selected)
            .position(position, set_size),
    );
}

/// Maximum fuzzy matches to show from any one source group.
///
/// The palette already caps visible results at a small, scannable list; keeping
/// the same cap per source prevents one group from monopolizing mixed results
/// while staying cheap for list-model replacement and keyboard navigation.
const MAX_RESULTS_PER_SOURCE: usize = 50;
