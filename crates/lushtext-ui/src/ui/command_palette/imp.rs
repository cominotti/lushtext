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

use crate::ui::accessibility;
use crate::ui::command_palette::PaletteEscape;
use crate::ui::command_palette::category_model::PaletteCategoryModel;
use crate::ui::command_palette::item::PaletteItem;
use crate::ui::command_palette::policy::{self, BackspaceLeaveTracker};
use crate::ui::command_palette::query_execution::PaletteQueryRequest;
use crate::ui::command_palette::row_factory::PaletteView;
use crate::ui::plain_disposal::DisposalOwned;
use gtk_lush_settle::Debounce;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, gio, glib};
use lushtext_model::workspace::palette::{
    PaletteCategory, PaletteFileEntry, PaletteNoteEntry, SearchMode,
};
use lushtext_services::palette::{self, FileIndex};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

type ActivateCallback = Rc<dyn Fn(&PaletteItem)>;
type EscapeCallback = Box<dyn Fn(PaletteEscape)>;

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
    pub(super) file_index_generation: Cell<lushtext_model::values::FileIndexGeneration>,
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
            file_index_generation: Cell::new(lushtext_model::values::FileIndexGeneration::ZERO),
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
}
