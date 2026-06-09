// SPDX-License-Identifier: GPL-3.0-or-later

//! Command palette GObject implementation: template binding, search scheduling,
//! and grouped result presentation.

use crate::model::palette::{PaletteFileEntry, SearchMode, SearchResultItem};
use crate::services::palette::{self, FileIndex};
use crate::ui::command_palette::item::PaletteItem;
use glib::prelude::*;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

/// Owned transport type for search results that can cross thread boundaries.
///
/// Created on the background thread, then converted to `PaletteItem` GObjects
/// on the main thread. Only owned display data crosses the thread boundary;
/// GTK objects stay on the main thread.
enum SearchHit {
    Header {
        label: String,
    },
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
    /// Create a presentation-only source header for grouped result sections.
    fn header(label: impl Into<String>) -> Self {
        Self::Header {
            label: label.into(),
        }
    }

    /// Convert an open file-backed tab entry into the same row shape as indexed files.
    fn from_open_file(f: &PaletteFileEntry) -> Self {
        Self::File {
            display_name: f.display_name.clone(),
            subtitle: f.subtitle.clone(),
            file_path: f.path.clone(),
        }
    }

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

    /// Convert the background-thread hit into a `PaletteItem` for the GTK list model.
    fn into_item(self) -> PaletteItem {
        match self {
            Self::Header { label } => PaletteItem::new_header_raw(label),
            Self::File {
                display_name,
                subtitle,
                file_path,
            } => PaletteItem::new_file_raw(display_name, subtitle, file_path),
            Self::Command {
                display_name,
                subtitle,
                action_id,
            } => PaletteItem::new_command_raw(display_name, subtitle, action_id),
        }
    }
}

type ActivateCallback = Box<dyn Fn(&PaletteItem)>;
type CloseCallback = Box<dyn Fn()>;

// CompositeTemplate loads the UI layout from a compiled XML file (bundled
// as a GResource). Each #[template_child] is auto-bound to the widget with
// the matching `id` attribute in the XML.
//
// GObject methods always take &self because multiple widgets can hold
// references at once. Cell<T> for Copy types (SearchMode, generation counters),
// RefCell<T> for complex types (Arc<FileIndex>, callbacks).
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
    pub file_index: RefCell<Arc<FileIndex>>,
    /// Open file-backed tabs supplied by the window shell.
    pub open_tabs: RefCell<Vec<PaletteFileEntry>>,
    /// Label for the workspace-indexed file group.
    pub workspace_group_label: RefCell<String>,
    /// Guard used while programmatically syncing the mode dropdown.
    pub syncing_mode_selector: Cell<bool>,
    /// Callback invoked when the user activates a result (Enter or click).
    pub activate_callback: RefCell<Option<ActivateCallback>>,
    /// Callback invoked when the palette should close (Escape key).
    pub close_callback: RefCell<Option<CloseCallback>>,
    /// Generation counter for debouncing search queries (150ms). Incremented on
    /// each keystroke; stale timer callbacks compare to detect superseded searches.
    pub search_generation: Cell<u32>,
    /// Queue of incremental index mutations waiting to be flushed.
    pub(super) pending_index_updates: RefCell<Vec<super::FileIndexUpdate>>,
    /// Generation counter for debouncing index update flushes (75ms).
    pub(super) index_update_generation: Cell<u32>,
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
            file_index: RefCell::new(Arc::new(FileIndex::default())),
            open_tabs: RefCell::default(),
            workspace_group_label: RefCell::new("All Workspaces".to_string()),
            syncing_mode_selector: Cell::new(false),
            activate_callback: RefCell::default(),
            close_callback: RefCell::default(),
            search_generation: Cell::new(0),
            pending_index_updates: RefCell::default(),
            index_update_generation: Cell::new(0),
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
        self.results_view.set_model(Some(&selection));

        self.setup_factory();
        self.setup_mode_selector();
        self.setup_search();
        self.setup_key_controller();
        self.setup_list_activation();
        self.apply_accessibility_metadata();
    }
}

impl WidgetImpl for LushtextCommandPalette {}
impl BoxImpl for LushtextCommandPalette {}

impl LushtextCommandPalette {
    /// Give the palette's compact controls durable accessible names for screen
    /// readers and AT-SPI smoke assertions.
    fn apply_accessibility_metadata(&self) {
        self.search_entry.update_property(&[
            gtk4::accessible::Property::Label("Command palette query"),
            gtk4::accessible::Property::Description(
                "Search open tabs, workspace files, notes, and commands",
            ),
        ]);
        self.mode_dropdown.update_property(&[
            gtk4::accessible::Property::Label("Command palette mode"),
            gtk4::accessible::Property::Description("Choose which result category to search"),
        ]);
        self.results_view.update_property(&[
            gtk4::accessible::Property::Label("Command palette results"),
            gtk4::accessible::Property::Description("Matching files, notes, and commands"),
        ]);
        self.no_results_label
            .update_property(&[gtk4::accessible::Property::Label(
                "Command palette no results",
            )]);
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

        factory.connect_bind(|_, list_item| {
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
            let generation = imp.search_generation.get().wrapping_add(1);
            imp.search_generation.set(generation);

            let query = entry.text().to_string();

            // Empty queries bypass debounce so default results update
            // immediately when the query is cleared.
            if query.is_empty() {
                imp.rebuild_results_owned(query);
                return;
            }

            let obj_weak = obj.downgrade();
            // Schedule debounce work on GTK's main loop. The `_local` variant
            // is safe for this non-Send closure because it runs on the main
            // thread after the delay.
            glib::timeout_add_local_once(std::time::Duration::from_millis(150), move || {
                let Some(obj) = obj_weak.upgrade() else {
                    return;
                };
                if obj.imp().search_generation.get() != generation {
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
    /// Increments `search_generation` to supersede any pending debounced
    /// search from `setup_search`. Direct callers (e.g., `set_file_index`,
    /// `open`, Tab mode-switch) rely on this to cancel stale timers.
    pub fn rebuild_results(&self, query: &str) {
        self.rebuild_results_owned(query.to_string());
    }

    /// Rebuild results from an owned query snapshot.
    ///
    /// Fuzzy matching runs on a background thread, then the main-thread
    /// completion applies results only if its generation is still current.
    pub fn rebuild_results_owned(&self, query: String) {
        let generation = self.search_generation.get().wrapping_add(1);
        self.search_generation.set(generation);

        let mode = self.mode.get();
        let index = Arc::clone(&self.file_index.borrow());
        let open_tabs = self.open_tabs.borrow().clone();
        let workspace_group_label = self.workspace_group_label.borrow().clone();

        crate::services::async_task::spawn_blocking_then(
            self.obj().clone(),
            move || {
                let hits = grouped_hits(
                    &index,
                    &open_tabs,
                    &workspace_group_label,
                    &query,
                    mode,
                    MAX_RESULTS_PER_SOURCE,
                );
                (hits, query)
            },
            move |obj, (hits, query)| {
                let imp = obj.imp();
                if imp.search_generation.get() != generation {
                    return; // superseded by a newer search
                }

                let items: Vec<PaletteItem> = hits.into_iter().map(SearchHit::into_item).collect();

                // splice() replaces items in a single operation (one items-changed
                // signal) instead of N append/remove calls (N relayout passes).
                let old_count = imp.results_store.n_items();
                imp.results_store.splice(0, old_count, &items);

                let has_results = items.iter().any(PaletteItem::is_activatable);
                imp.no_results_label
                    .set_visible(!has_results && !query.is_empty());

                if let Some(first_result) = imp.first_activatable_position()
                    && let Some(selection) = imp.selection_model()
                {
                    selection.set_selected(first_result);
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
        let Some(new_pos) = self.next_activatable_position(current, delta) else {
            return;
        };
        selection.set_selected(new_pos);
        self.results_view
            .scroll_to(new_pos, gtk4::ListScrollFlags::NONE, None);
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
}

/// Assemble GTK-ready rows from service-owned palette policy.
///
/// The UI controls presentation order and headers here, while command
/// membership and Notes section rules stay in `services::palette`.
fn grouped_hits(
    index: &FileIndex,
    open_tabs: &[PaletteFileEntry],
    workspace_group_label: &str,
    query: &str,
    mode: SearchMode,
    max_per_source: usize,
) -> Vec<SearchHit> {
    let mut hits = Vec::new();
    let mut seen_file_paths = HashSet::new();

    match mode {
        SearchMode::Files => {
            append_file_groups(
                &mut hits,
                &mut seen_file_paths,
                index,
                open_tabs,
                workspace_group_label,
                query,
                max_per_source,
            );
        }
        SearchMode::Notes => {
            append_note_command_sections(&mut hits, query, max_per_source);
        }
        SearchMode::Commands => {
            append_command_group(&mut hits, query, max_per_source);
        }
        SearchMode::All => {
            append_file_groups(
                &mut hits,
                &mut seen_file_paths,
                index,
                open_tabs,
                workspace_group_label,
                query,
                max_per_source,
            );
            append_note_command_group(&mut hits, query, max_per_source);
            append_non_note_command_group(&mut hits, query, max_per_source);
        }
    }

    hits
}

/// Append file-oriented groups and remember open-tab paths for de-duplication.
fn append_file_groups(
    hits: &mut Vec<SearchHit>,
    seen_file_paths: &mut HashSet<PathBuf>,
    index: &FileIndex,
    open_tabs: &[PaletteFileEntry],
    workspace_group_label: &str,
    query: &str,
    max_per_source: usize,
) {
    let open_file_hits = palette::search_open_files(open_tabs, query, max_per_source);
    let open_file_hits: Vec<_> = open_file_hits
        .into_iter()
        .filter_map(|result| match result.item {
            SearchResultItem::OpenFile(file) => {
                seen_file_paths.insert(file.path.clone());
                Some(SearchHit::from_open_file(file))
            }
            SearchResultItem::File(_) | SearchResultItem::Command(_) => None,
        })
        .collect();
    append_group(hits, "Open Tabs", open_file_hits);

    let workspace_hits: Vec<_> = index
        .search(query, max_per_source)
        .into_iter()
        .filter_map(|result| match result.item {
            SearchResultItem::File(file) if !seen_file_paths.contains(&file.path) => {
                seen_file_paths.insert(file.path.clone());
                Some(SearchHit::from_file(file))
            }
            SearchResultItem::File(_)
            | SearchResultItem::OpenFile(_)
            | SearchResultItem::Command(_) => None,
        })
        .collect();
    append_group(hits, workspace_group_label, workspace_hits);
}

/// Append all command results for dedicated `Commands` mode.
fn append_command_group(hits: &mut Vec<SearchHit>, query: &str, max: usize) {
    let command_hits = command_hits_from_results(palette::search_commands(query, max));
    hits.extend(command_hits);
}

/// Append note workflow commands as one source group in mixed `All` mode.
fn append_note_command_group(hits: &mut Vec<SearchHit>, query: &str, max: usize) {
    let command_hits = command_hits_from_results(palette::search_note_commands(query, max));
    append_group(hits, "Notes", command_hits);
}

/// Append non-note commands in mixed `All` mode to avoid duplicate note rows.
fn append_non_note_command_group(hits: &mut Vec<SearchHit>, query: &str, max: usize) {
    let command_hits = command_hits_from_results(palette::search_non_note_commands(query, max));
    append_group(hits, "Commands", command_hits);
}

/// Append Notes mode groups in the workflow-oriented section order.
fn append_note_command_sections(hits: &mut Vec<SearchHit>, query: &str, max: usize) {
    for section in palette::NoteCommandSection::ALL {
        let command_hits = command_hits_from_results(palette::search_note_commands_for_section(
            section, query, max,
        ));
        append_group(hits, section.label(), command_hits);
    }
}

fn command_hits_from_results(
    results: Vec<crate::model::palette::ScoredResult<'static>>,
) -> Vec<SearchHit> {
    results
        .into_iter()
        .filter_map(|result| match result.item {
            SearchResultItem::Command(command) => Some(SearchHit::from_command(command)),
            SearchResultItem::OpenFile(_) | SearchResultItem::File(_) => None,
        })
        .collect()
}

/// Add a section only when that source has matching activatable rows.
fn append_group(hits: &mut Vec<SearchHit>, label: &str, group_hits: Vec<SearchHit>) {
    if group_hits.is_empty() {
        return;
    }
    hits.push(SearchHit::header(label));
    hits.extend(group_hits);
}

/// Maximum fuzzy matches to show from any one source group.
///
/// The palette already caps visible results at a small, scannable list; keeping
/// the same cap per source prevents one group from monopolizing mixed results
/// while staying cheap for list-model replacement and keyboard navigation.
const MAX_RESULTS_PER_SOURCE: usize = 50;
