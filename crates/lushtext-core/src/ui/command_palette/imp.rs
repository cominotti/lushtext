// SPDX-License-Identifier: GPL-3.0-or-later

//! Command palette GObject implementation: template binding, search scheduling,
//! grouped result presentation, and the category view
//! (`drill-down-palette-categories`): entering and leaving it, its navigation
//! without a walk, and its accessible projection.
//!
//! # Role
//!
//! This module carries **no role**. It is a **called presentation surface** of
//! `WFR-COMMAND-PALETTE` — it projects the workflow onto widgets (subclass state, template children, the list factory, and accessible-state projection) — so under
//! `gtk-adapter-module-boundaries` it is outside the five-name role taxonomy,
//! takes none of those names, and owns no `policy.rs` and no `evidence.rs`. Its
//! behavior obligations are unchanged. Named in that workflow's matrix row.

use crate::model::palette::{
    PaletteCategory, PaletteFileEntry, PaletteNoteEntry, PaletteSearchRow, SearchMode,
};
use crate::services::palette::{self, FileIndex};
use crate::ui::accessibility;
use crate::ui::command_palette::PaletteEscape;
use crate::ui::command_palette::category_model::PaletteCategoryModel;
use crate::ui::command_palette::item::PaletteItem;
use crate::ui::command_palette::policy::{self, BackspaceLeaveTracker, GroupedRowKind};
use crate::ui::command_palette::query_execution::PaletteQueryRequest;
use crate::ui::command_palette::row_factory::PaletteView;
use crate::ui::plain_disposal::DisposalOwned;
use gtk_lush_settle::Debounce;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::Arc;

/// Owned transport type for search results that can cross thread boundaries.
///
/// Created on the background thread, then converted to `PaletteItem` GObjects
/// on the main thread. Only owned display data crosses the thread boundary;
/// GTK objects stay on the main thread.
fn palette_row_into_item(
    row: PaletteSearchRow,
    mode: SearchMode,
    workspace_group_label: &str,
) -> PaletteItem {
    match row {
        PaletteSearchRow::More { category, hidden } => PaletteItem::new_more(
            category,
            hidden,
            category.label(mode, workspace_group_label),
        ),
        PaletteSearchRow::Header { label, .. } => PaletteItem::new_header_raw(label),
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
type EscapeCallback = Box<dyn Fn(PaletteEscape)>;

/// The category of each grouped row: a header's own category, carried down
/// to the rows that follow it (review finding R3: matching header labels
/// confused a workspace named like an absent group). Rows before any header
/// (Commands mode) are commands.
fn grouped_row_categories(
    rows: &[PaletteSearchRow],
    mode: SearchMode,
) -> Vec<Option<PaletteCategory>> {
    let mut current = (mode == SearchMode::Commands).then_some(PaletteCategory::Commands);
    rows.iter()
        .map(|row| match row {
            PaletteSearchRow::Header { category, .. } => {
                current = Some(*category);
                current
            }
            PaletteSearchRow::More { category, .. } => Some(*category),
            _ => current,
        })
        .collect()
}

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
    /// Scroller of the grouped results, hidden while a category view is shown.
    #[template_child]
    pub results_scroll: TemplateChild<gtk4::ScrolledWindow>,
    /// The category view's bar: back button, title and notice.
    #[template_child]
    pub category_bar: TemplateChild<gtk4::Box>,
    /// "Back to all results".
    #[template_child]
    pub category_back_button: TemplateChild<gtk4::Button>,
    /// "{category} · {N} results".
    #[template_child]
    pub category_title_label: TemplateChild<gtk4::Label>,
    /// The budget or note-source notice, when one applies.
    #[template_child]
    pub category_notice_label: TemplateChild<gtk4::Label>,
    /// Scroller of the category view.
    #[template_child]
    pub category_scroll: TemplateChild<gtk4::ScrolledWindow>,
    /// The category view's list over `category_model`.
    #[template_child]
    pub category_view: TemplateChild<gtk4::ListView>,

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
    /// Callback invoked for every Escape the palette consumes: a category
    /// leave or a close request (`drill-down-palette-categories` D5).
    pub escape_callback: RefCell<Option<EscapeCallback>>,
    /// The category view shown, `None` for the grouped view.
    pub category: Cell<Option<PaletteCategory>>,
    /// The category view's lazy model.
    pub category_model: PaletteCategoryModel,
    /// The category view's rows being retired in slices after the view was
    /// left (`lighten-notes-browser-rows` D4); entering a category again
    /// cancels it and publishes over the rows left.
    pub(super) category_retirement: RefCell<Option<crate::ui::list_retirement::RowRetirement>>,
    /// A category view's staged first publish growing one step per frame
    /// (`lighten-notes-browser-rows` D5), while it grows.
    pub(super) category_growth: RefCell<Option<crate::ui::list_growth::RowGrowth>>,
    /// Matches of the shown category, listed or not.
    pub category_matching: Cell<usize>,
    /// Whether the row limit cut the shown category.
    pub category_limited: Cell<bool>,
    /// The category just left, whose "More" row the next grouped publish
    /// selects.
    pub(super) leaving_category: Cell<Option<PaletteCategory>>,
    /// When a Backspace press leaves the category view.
    pub(super) backspace: Cell<BackspaceLeaveTracker>,
    /// The notice last announced in the category view.
    pub(super) announced_notice: RefCell<Option<String>>,
    /// Category-view entry announcements raised (evidence).
    pub(super) category_entry_announcements: Cell<u64>,
    /// Category-view notice announcements raised (evidence).
    pub(super) category_notice_announcements: Cell<u64>,
    /// The palette's note source: rows held, rows found, and whether found is
    /// a floor, when a byte ceiling cut it.
    pub(super) note_source_cut: Cell<Option<(usize, usize, bool)>>,
    /// The query of the category view's last published ranking, so a
    /// same-query republish keeps the selection (`None` after entering).
    pub(super) category_published_query: RefCell<Option<String>>,
    /// The `content-changed` handler each bound row holds, by list item, so a
    /// retargeted category row rebinds in place.
    pub(super) bound_rows: RefCell<HashMap<gtk4::ListItem, (PaletteItem, glib::SignalHandlerId)>>,
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
            results_scroll: TemplateChild::default(),
            category_bar: TemplateChild::default(),
            category_back_button: TemplateChild::default(),
            category_title_label: TemplateChild::default(),
            category_notice_label: TemplateChild::default(),
            category_scroll: TemplateChild::default(),
            category_view: TemplateChild::default(),
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
            escape_callback: RefCell::default(),
            category: Cell::new(None),
            category_model: PaletteCategoryModel::default(),
            category_retirement: RefCell::new(None),
            category_growth: RefCell::new(None),
            category_matching: Cell::new(0),
            category_limited: Cell::new(false),
            leaving_category: Cell::new(None),
            backspace: Cell::new(BackspaceLeaveTracker::default()),
            announced_notice: RefCell::new(None),
            category_entry_announcements: Cell::new(0),
            category_notice_announcements: Cell::new(0),
            note_source_cut: Cell::new(None),
            category_published_query: RefCell::new(None),
            bound_rows: RefCell::default(),
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
        crate::ui::entry_paste_cap::cap_single_line_inserts(&*self.search_entry);

        let selection = gtk4::SingleSelection::new(Some(self.results_store.clone()));
        selection.set_autoselect(true);
        let obj_weak = self.obj().downgrade();
        selection.connect_selected_notify(move |_| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.imp().refresh_accessibility_state();
            }
        });
        self.results_view.set_model(Some(&selection));

        let category_selection = gtk4::SingleSelection::new(Some(self.category_model.clone()));
        category_selection.set_autoselect(false);
        let obj_weak = self.obj().downgrade();
        category_selection.connect_selected_notify(move |_| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.imp().refresh_accessibility_state();
            }
        });
        self.category_view.set_model(Some(&category_selection));
        let obj_weak = self.obj().downgrade();
        self.category_back_button.connect_clicked(move |_| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.imp().leave_category();
                obj.imp().search_entry.grab_focus();
            }
        });

        self.setup_factory(PaletteView::Grouped);
        self.setup_factory(PaletteView::Category);
        self.setup_mode_selector();
        self.setup_search();
        self.setup_key_controller();
        self.setup_list_activation();
        self.apply_accessibility_metadata();
    }

    fn dispose(&self) {
        self.index_update_capacity_wakeup.cancel();
        self.cancel_category_clear();
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
        accessibility::set_role(&*self.category_view, gtk4::AccessibleRole::List);
        accessibility::set_labelled_description(
            &*self.category_back_button,
            "Back to all results",
            "Return to the grouped command palette results",
        );
        accessibility::set_role(&*self.no_results_label, gtk4::AccessibleRole::Status);
        accessibility::set_labelled_description(
            &*self.no_results_label,
            "Command palette no results",
            "No matching files, notes, or commands",
        );
        self.refresh_accessibility_state();
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
                gdk4::Key::Page_Up | gdk4::Key::KP_Page_Up => {
                    imp.move_selection(-policy::RESULT_PAGE_ROWS.cast_signed());
                    glib::Propagation::Stop
                }
                gdk4::Key::Page_Down | gdk4::Key::KP_Page_Down => {
                    imp.move_selection(policy::RESULT_PAGE_ROWS.cast_signed());
                    glib::Propagation::Stop
                }
                gdk4::Key::BackSpace
                    if imp.category.get().is_some()
                        && imp
                            .backspace
                            .get()
                            .backspace_pressed(imp.search_entry.text().is_empty()) =>
                {
                    imp.leave_category();
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

        let obj_weak = self.obj().downgrade();
        key_controller.connect_key_released(move |_, _, _, _| {
            if let Some(obj) = obj_weak.upgrade() {
                let imp = obj.imp();
                let mut tracker = imp.backspace.get();
                tracker.key_released(imp.search_entry.text().is_empty());
                imp.backspace.set(tracker);
            }
        });

        self.search_entry.add_controller(key_controller);

        let obj_weak = self.obj().downgrade();
        self.search_entry.connect_changed(move |entry| {
            if let Some(obj) = obj_weak.upgrade() {
                let imp = obj.imp();
                let mut tracker = imp.backspace.get();
                tracker.query_changed(entry.text().is_empty());
                imp.backspace.set(tracker);
            }
        });

        let obj_weak = self.obj().downgrade();
        self.search_entry.connect_activate(move |_| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.imp().activate_selected();
            }
        });

        let obj_weak = self.obj().downgrade();
        self.search_entry.connect_stop_search(move |_| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.imp().dismiss_one_level();
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
        let obj_weak = self.obj().downgrade();
        self.category_view.connect_activate(move |_, position| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.imp().activate_category_at(position);
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
        let mode = self.mode.get();
        let workspace_group_label = self.workspace_group_label.borrow().clone();
        let categories = grouped_row_categories(&rows, mode);
        let items: Vec<PaletteItem> = rows
            .into_iter()
            .zip(categories)
            .map(|(row, category)| {
                let item = palette_row_into_item(row, mode, &workspace_group_label);
                item.set_category(category);
                item
            })
            .collect();

        // One splice keeps GTK projection work independent of the match count.
        let old_count = self.results_store.n_items();
        self.results_store.splice(0, old_count, &items);

        let has_results = items.iter().any(PaletteItem::is_result);
        self.no_results_label
            .set_visible(policy::no_results_visible(has_results, query.is_empty()));

        let selected = match self.leaving_category.take() {
            Some(category) => policy::leave_selection(&grouped_row_kinds(&items), category),
            None => self.first_activatable_position(),
        };
        if let Some(position) = selected
            && let Some(selection) = self.selection_model()
        {
            selection.set_selected(position);
            self.results_view
                .scroll_to(position, gtk4::ListScrollFlags::NONE, None);
        }
        self.refresh_accessibility_state();
    }

    /// Move the selection one row (`delta` ±1) or one page in the visible
    /// view: headers skipped in the grouped view, an O(1) clamp in the
    /// category view.
    fn move_selection(&self, delta: i32) {
        if self.category.get().is_some() {
            let Some(selection) = self.category_selection() else {
                return;
            };
            // A key lands where it would on the full ranking: a staged
            // first publish still growing lists the rest first (D5).
            self.finish_category_growth();
            let current = Some(selection.selected()).filter(|p| *p != gtk4::INVALID_LIST_POSITION);
            let Some(position) =
                policy::next_category_row(current, self.category_model.len(), delta)
            else {
                return;
            };
            selection.set_selected(position);
            self.category_view
                .scroll_to(position, gtk4::ListScrollFlags::NONE, None);
            self.refresh_accessibility_state();
            return;
        }
        let Some(selection) = self.selection_model() else {
            return;
        };
        let n = self.results_store.n_items();
        if n == 0 {
            return;
        }
        let current = selection.selected();
        let flags = self.activatable_flags();
        let new_pos = if delta.unsigned_abs() > 1 {
            policy::page_activatable(&flags, current, delta)
        } else {
            policy::next_activatable(&flags, current, delta)
        };
        let Some(new_pos) = new_pos else {
            return;
        };
        selection.set_selected(new_pos);
        self.results_view
            .scroll_to(new_pos, gtk4::ListScrollFlags::NONE, None);
        self.refresh_accessibility_state();
    }

    /// Activate the currently selected actionable row of the visible view.
    ///
    /// Presentation-only source headers are ignored before forwarding the row
    /// to the registered activation callback.
    pub fn activate_selected(&self) {
        if self.category.get().is_some() {
            if let Some(selection) = self.category_selection() {
                self.activate_category_at(selection.selected());
            }
            return;
        }
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
        self.activate_item(palette_item);
    }

    fn activate_category_at(&self, position: u32) {
        if position >= self.category_model.len() {
            return;
        }
        let Some(item) = self
            .category_model
            .item(position)
            .and_downcast::<PaletteItem>()
        else {
            return;
        };
        self.activate_item(&item);
    }

    /// A "More" row opens its category view inside the palette and never
    /// reaches the window; any other activatable row goes to the callback.
    fn activate_item(&self, item: &PaletteItem) {
        if item.is_more() {
            if let Some(category) = item.category() {
                let _ = self.enter_category(category);
            }
            return;
        }
        if !item.is_activatable() {
            return;
        }
        if let Some(ref cb) = *self.activate_callback.borrow() {
            cb(item);
        }
    }

    /// Update the active search mode and sync all dependent widgets.
    pub fn set_mode(&self, mode: SearchMode) {
        // A mode change leaves a category view; the caller restarts the
        // query in the new mode.
        let _ = self.leave_category_view();
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

    pub(super) fn selection_model(&self) -> Option<gtk4::SingleSelection> {
        self.results_view
            .model()
            .and_then(|m| m.downcast::<gtk4::SingleSelection>().ok())
    }

    /// The selected position of the visible view, when one is selected.
    /// Read through `try_get`, so the evidence surface stays honest on a
    /// disposed widget.
    pub(super) fn visible_selected_position(&self) -> Option<u32> {
        let view = if self.category.get().is_some() {
            self.category_view.try_get()
        } else {
            self.results_view.try_get()
        };
        view.and_then(|view| view.model())
            .and_then(|model| model.downcast::<gtk4::SingleSelection>().ok())
            .map(|selection| selection.selected())
            .filter(|position| *position != gtk4::INVALID_LIST_POSITION)
    }

    pub(super) fn category_selection(&self) -> Option<gtk4::SingleSelection> {
        self.category_view
            .model()
            .and_then(|m| m.downcast::<gtk4::SingleSelection>().ok())
    }

    /// Project live palette search state into accessible states and values.
    pub(super) fn refresh_accessibility_state(&self) {
        let searching = self.searching.get();
        let in_category = self.category.get().is_some();
        let (result_count, selected_name) = if in_category {
            // The model's length and the selection's own item: no walk.
            let selected = self
                .category_selection()
                .and_then(|selection| selection.selected_item())
                .and_downcast::<PaletteItem>()
                .map(|item| item.display_name());
            (
                usize::try_from(self.category_model.full_len()).unwrap_or(usize::MAX),
                selected,
            )
        } else {
            let results = (0..self.results_store.n_items())
                .filter(|position| {
                    self.results_store
                        .item(*position)
                        .and_downcast_ref::<PaletteItem>()
                        .is_some_and(PaletteItem::is_result)
                })
                .count();
            let selected = self
                .selection_model()
                .and_then(|selection| self.results_store.item(selection.selected()))
                .and_downcast::<PaletteItem>()
                .filter(PaletteItem::is_activatable)
                .map(|item| item.display_name());
            (results, selected)
        };
        // The label's own visibility already encodes the non-empty-query half of
        // the no-results decision, set by the publish; this only adds the live
        // "and nothing is listed, and we are not still searching" half that
        // AT-SPI hiding needs.
        let status_visible = self.no_results_label.is_visible() && result_count == 0 && !searching;
        let visible_list: &gtk4::ListView = if in_category {
            &self.category_view
        } else {
            &self.results_view
        };

        accessibility::set_busy(&*self.search_entry, searching);
        accessibility::set_busy(visible_list, searching);
        accessibility::set_hidden(&*self.no_results_label, !status_visible);
        accessibility::set_hidden(visible_list, status_visible);

        let value_text =
            policy::accessible_value_text(selected_name.as_deref(), searching, result_count);
        accessibility::set_value_text(visible_list, &value_text);
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

/// The kind of each grouped row, for `policy::leave_selection`.
fn grouped_row_kinds(items: &[PaletteItem]) -> Vec<GroupedRowKind> {
    items
        .iter()
        .map(
            |item| match (item.is_header(), item.is_more(), item.category()) {
                (false, true, Some(category)) => GroupedRowKind::More(category),
                (false, false, Some(category)) => GroupedRowKind::Result(category),
                _ => GroupedRowKind::Header,
            },
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::grouped_row_categories;
    use crate::model::palette::{PaletteCategory, PaletteSearchRow, SearchMode};
    use std::path::PathBuf;

    fn file(name: &str) -> PaletteSearchRow {
        PaletteSearchRow::File {
            display_name: name.to_string(),
            subtitle: String::new(),
            file_path: PathBuf::from(name),
        }
    }

    /// Review finding R3: a workspace whose label equals another group's
    /// ("Open Tabs") is not mistaken for that group when the other group is
    /// absent; the grouped search knows each header's category.
    #[test]
    fn a_workspace_labelled_like_an_absent_group_keeps_its_category() {
        let rows = vec![
            PaletteSearchRow::Header {
                label: "Open Tabs".to_string(),
                category: PaletteCategory::WorkspaceFiles,
            },
            file("a.rs"),
            PaletteSearchRow::More {
                category: PaletteCategory::WorkspaceFiles,
                hidden: 3,
            },
        ];
        assert_eq!(
            grouped_row_categories(&rows, SearchMode::Files),
            vec![Some(PaletteCategory::WorkspaceFiles); 3]
        );
    }
}
