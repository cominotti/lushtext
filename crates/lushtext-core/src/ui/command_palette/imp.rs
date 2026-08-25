// SPDX-License-Identifier: GPL-3.0-or-later

//! Command palette GObject implementation: template binding, search scheduling,
//! and grouped result presentation.

use crate::model::palette::{PaletteFileEntry, PaletteNoteEntry, PaletteSearchRow, SearchMode};
use crate::services::palette::{self, FileIndex};
use crate::ui::accessibility::{self, RowAccessibility};
use crate::ui::command_palette::item::PaletteItem;
use crate::ui::command_palette::policy;
use crate::ui::command_palette::query_execution::PaletteQueryRequest;
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
    ///
    /// This coordinator owns the query generation and exposes `is_current`,
    /// which is what makes it the query seam's value object: no separate ticket
    /// type is needed on this side of the workflow.
    pub(super) search_flight: RefCell<palette::PaletteSearchCoordinator<PaletteQueryRequest>>,
    /// Number of workers that cooperatively observed a superseding cancellation.
    ///
    /// Test-gated: no production path reads it, so a default-feature build
    /// compiles no storage for it.
    #[cfg(feature = "test-utils")]
    pub(super) observed_search_cancellations: Cell<usize>,
    /// Candidate progress retained from the most recent cancelled worker.
    ///
    /// Test-gated for the same reason as `observed_search_cancellations`.
    #[cfg(feature = "test-utils")]
    pub(super) last_cancelled_search_examined: Cell<usize>,
    /// Callback invoked when the user activates a result (Enter or click).
    pub activate_callback: RefCell<Option<ActivateCallback>>,
    /// Callback invoked when the palette should close (Escape key).
    pub close_callback: RefCell<Option<CloseCallback>>,
    /// Debounce for non-empty text queries; the runtime coordinator owns freshness.
    pub search_debounce: Debounce,
    /// Queue of incremental index mutations waiting to be flushed.
    pub(super) pending_index_updates: RefCell<Vec<policy::FileIndexUpdate>>,
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
            search_flight: RefCell::default(),
            #[cfg(feature = "test-utils")]
            observed_search_cancellations: Cell::new(0),
            #[cfg(feature = "test-utils")]
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
    const NAME: &str = "LushtextCommandPalette";
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
            imp.restart_query();
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
                obj.start_query_flight(query);
                return;
            }

            // Inversion: control resumes in this callback, which re-enters
            // query stage 1 with the query text captured at keystroke time.
            imp.search_debounce.schedule(
                &obj,
                std::time::Duration::from_millis(policy::SEARCH_DEBOUNCE_MS),
                move |obj, _| {
                    obj.start_query_flight(query);
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
                    imp.restart_query();
                    glib::Propagation::Stop
                }
                gdk4::Key::ISO_Left_Tab => {
                    imp.set_mode(imp.mode.get().previous());
                    imp.restart_query();
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

    /// Restart the query flight from the currently visible entry text.
    ///
    /// Query stage 1 for every adapter-owned entry point: the mode dropdown,
    /// Tab and Shift+Tab cycling, and source installation.
    pub(super) fn restart_query(&self) {
        self.obj()
            .start_query_flight(self.search_entry.text().to_string());
    }

    /// Query stage 4's widget mutation: replace the whole visible model with one splice.
    ///
    /// `query_execution::settle_query_flight` owns stage 4 and calls this once it
    /// has proved the completion is still current.
    pub(super) fn publish_search_rows(&self, rows: Vec<PaletteSearchRow>, query: &str) {
        let items: Vec<PaletteItem> = rows.into_iter().map(palette_row_into_item).collect();

        // One splice keeps GTK projection work independent of the match count.
        let old_count = self.results_store.n_items();
        self.results_store.splice(0, old_count, &items);

        let has_results = items.iter().any(PaletteItem::is_activatable);
        self.no_results_label
            .set_visible(policy::no_results_visible(has_results, query.is_empty()));

        if let Some(first_result) = self.first_activatable_position()
            && let Some(selection) = self.selection_model()
        {
            selection.set_selected(first_result);
        }
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

    /// Project the visible model into the activatable-flag sequence policy reads.
    fn activatable_flags(&self) -> Vec<bool> {
        (0..self.results_store.n_items())
            .map(|position| {
                self.results_store
                    .item(position)
                    .and_downcast_ref::<PaletteItem>()
                    .is_some_and(PaletteItem::is_activatable)
            })
            .collect()
    }

    /// Find the first row that can actually be opened or executed.
    fn first_activatable_position(&self) -> Option<u32> {
        policy::first_activatable(&self.activatable_flags())
    }

    /// Move keyboard selection across result rows while skipping source headers.
    fn next_activatable_position(&self, current: u32, delta: i32) -> Option<u32> {
        policy::next_activatable(&self.activatable_flags(), current, delta)
    }

    fn selection_model(&self) -> Option<gtk4::SingleSelection> {
        self.results_view
            .model()
            .and_then(|m| m.downcast::<gtk4::SingleSelection>().ok())
    }

    /// Project live palette search state into accessible states and values.
    pub(super) fn refresh_accessibility_state(&self) {
        let activatable = self.activatable_flags();
        let result_count = activatable.iter().filter(|flag| **flag).count();
        let searching = self.searching.get();
        // The label's own visibility already encodes the non-empty-query half of
        // the no-results decision, set by `publish_search_rows`; this only adds
        // the live "and nothing is activatable, and we are not still searching"
        // half that AT-SPI hiding needs.
        let status_visible = self.no_results_label.is_visible() && result_count == 0 && !searching;

        accessibility::set_busy(&*self.search_entry, searching);
        accessibility::set_busy(&*self.results_view, searching);
        accessibility::set_hidden(&*self.no_results_label, !status_visible);
        accessibility::set_hidden(&*self.results_view, status_visible);

        let selected_name = self
            .selection_model()
            .and_then(|selection| self.results_store.item(selection.selected()))
            .and_downcast::<PaletteItem>()
            .filter(PaletteItem::is_activatable)
            .map(|item| item.display_name());
        let value_text =
            policy::accessible_value_text(selected_name.as_deref(), searching, result_count);
        accessibility::set_value_text(&*self.results_view, &value_text);
        accessibility::set_value_text(&*self.no_results_label, &policy::result_count_text(0));
    }

    /// Replace the open file-backed tab source and repaint if the palette is open.
    pub(super) fn install_open_tabs(&self, open_tabs: Vec<PaletteFileEntry>) {
        *self.open_tabs.borrow_mut() = Arc::from(open_tabs);
        self.restart_query_if_open();
    }

    /// Replace the cached note rows and repaint if a note-showing mode is active.
    pub(super) fn install_note_entries(
        &self,
        note_entries: DisposalOwned<Box<[PaletteNoteEntry]>>,
    ) {
        let note_entries = note_entries.into_retained_current();
        let previous =
            std::mem::replace(&mut *self.note_entries.borrow_mut(), Arc::new(note_entries));
        drop(previous);
        if self.palette_open.get() && matches!(self.mode.get(), SearchMode::All | SearchMode::Notes)
        {
            self.restart_query();
        }
    }

    /// Replace the workspace group label and repaint if it actually changed.
    pub(super) fn install_workspace_group_label(&self, label: String) {
        if *self.workspace_group_label.borrow() == label {
            return;
        }
        *self.workspace_group_label.borrow_mut() = label;
        self.restart_query_if_open();
    }

    /// Replace every window-owned source in one turn.
    pub(super) fn install_sources(
        &self,
        open_tabs: Vec<PaletteFileEntry>,
        workspace_group_label: &str,
    ) {
        *self.open_tabs.borrow_mut() = Arc::from(open_tabs);
        *self.workspace_group_label.borrow_mut() = workspace_group_label.to_string();
        self.restart_query_if_open();
    }

    /// Restart the query only while the palette is showing results.
    pub(super) fn restart_query_if_open(&self) {
        if self.palette_open.get() {
            self.restart_query();
        }
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
