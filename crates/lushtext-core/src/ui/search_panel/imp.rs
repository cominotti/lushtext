// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the workspace search panel.
//!
//! Handles search execution with channel-based streaming, result grouping
//! into a `GtkTreeListModel`, and debounced query processing. Uses
//! `std::thread::spawn` + `crossbeam_channel::bounded` instead of
//! `spawn_blocking_then` because search results stream incrementally.

use super::item::SearchResultItem;
use super::{SearchFileGroup, SearchMatchLocation, SearchProgressUpdate};
use crate::model::content_search::{
    ReplacePreviewOutcome, SavedSearch, SearchHistoryEntry, SearchMatch, SearchMatchId,
};
use crate::ui::accessibility;
use gtk_lush_settle::Debounce;
use gtk4::prelude::*;
use gtk4::{self, CompositeTemplate, gio, glib};
use libadwaita::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize};
use std::time::Duration;

/// Callback type for file-open events (path, line_number).
type OpenFileCallback = Box<dyn Fn(&Path, u32)>;

/// Callback type for navigation events from F4/Shift+F4 (path, line_number).
type NavigateCallback = Box<dyn Fn(&Path, u32)>;

/// Callback type for search progress events.
type ProgressCallback = Box<dyn Fn(SearchProgressUpdate)>;

/// Callback type for Replace All execution: receives checked replacements.
type ReplaceCallback = Box<dyn Fn(super::GuardedReplacements)>;

/// Callback type for Undo All: receives the backup map to restore.
type UndoCallback = Box<dyn Fn(Arc<super::GuardedReplaceUndoBackup>)>;
type MessageCallback = Box<dyn Fn(&str)>;

/// Delay text-entry searches long enough to batch normal typing without making
/// the workspace search panel feel sluggish.
const SEARCH_INPUT_DEBOUNCE_MS: u64 = 300;

/// Runtime state for one in-flight search and its grouped GTK result models.
pub struct SearchRuntimeState {
    /// Root-level model: contains one file-header row per matching file.
    pub root_store: RefCell<gio::ListStore>,
    /// Per-file child stores keyed by absolute file path.
    pub file_groups: RefCell<BTreeMap<PathBuf, SearchFileGroup>>,
    /// Plain match data in arrival order for non-rendering workflows.
    ///
    /// Preview generation must not walk GTK tree models on the action path;
    /// this Rust snapshot lets the worker handoff clone service data directly.
    pub search_matches: RefCell<Vec<SearchMatch>>,
    /// Immutable accepted generation shared by preview/apply without whole-vector clones.
    pub accepted_matches: RefCell<Option<Arc<Vec<SearchMatch>>>>,
    /// Count of zero-copy accepted-snapshot handoffs into Replace Preview.
    pub shared_snapshot_handoffs: Cell<u64>,
    /// Regression probe for any compatibility path that deep-clones accepted matches.
    pub whole_result_clones: Cell<u64>,
    /// Cancel token for the currently running worker thread, if any.
    pub cancel_token: RefCell<Option<Arc<AtomicBool>>>,
    /// Plain-Rust active-plus-latest ownership policy.
    pub flight: RefCell<super::policy::WorkspaceSearchFlight>,
    /// Direct active-group count; the single-flight invariant keeps this at zero or one.
    pub active_worker_groups: Cell<u8>,
    /// High-water active-group count for deterministic concurrency evidence.
    pub active_worker_groups_high_water: Cell<u8>,
    /// Coalesced detached-generation GTK disposal session.
    pub(super) retirement: RefCell<Option<super::retirement::SearchRetirementSession>>,
    /// Replaceable latest query held while detached-generation disposal is saturated.
    pub deferred_search: RefCell<Option<crate::model::content_search::SearchQuerySpec>>,
    /// Whether the one bounded retirement idle callback is armed.
    pub retirement_armed: Cell<bool>,
    /// Monotonic identity for detached result generations.
    pub retirement_generation: Cell<u64>,
    /// High-water rows released by any one bounded retirement turn.
    pub retirement_rows_per_slice_high_water: Cell<usize>,
    /// Test-only before/after ownership evidence from each retirement turn.
    #[cfg(feature = "test-utils")]
    pub retirement_observations: RefCell<Vec<super::retirement::SearchRetirementSliceObservation>>,
    /// High-water detached generations retained by the bounded disposer.
    pub retirement_generations_high_water: Cell<usize>,
    /// Debounce for search-entry input.
    pub search_debounce: Debounce,
    /// Debounce for glob-entry input, separate from the main query.
    pub glob_debounce: Debounce,
    /// Workspace folders to search. Updated by the window when workspaces change.
    pub workspace_folders: RefCell<Arc<[PathBuf]>>,
    /// Running total of matches in the current search.
    pub total_matches: Cell<u32>,
    /// Running total of files that currently have at least one match.
    pub total_files: Cell<u32>,
    /// Whether a search is currently active (worker thread + polling timer alive).
    pub searching: Cell<bool>,
    /// Whether the 10k result cap was reached for the current search.
    pub result_capped: Cell<bool>,
    /// Last progress count forwarded to the window for approximate completion totals.
    pub last_progress_count: Cell<usize>,
}

impl Default for SearchRuntimeState {
    fn default() -> Self {
        Self {
            root_store: RefCell::new(gio::ListStore::new::<SearchResultItem>()),
            file_groups: RefCell::new(BTreeMap::new()),
            search_matches: RefCell::new(Vec::new()),
            accepted_matches: RefCell::new(None),
            shared_snapshot_handoffs: Cell::new(0),
            whole_result_clones: Cell::new(0),
            cancel_token: RefCell::new(None),
            flight: RefCell::new(super::policy::WorkspaceSearchFlight::default()),
            active_worker_groups: Cell::new(0),
            active_worker_groups_high_water: Cell::new(0),
            retirement: RefCell::new(None),
            deferred_search: RefCell::new(None),
            retirement_armed: Cell::new(false),
            retirement_generation: Cell::new(0),
            retirement_rows_per_slice_high_water: Cell::new(0),
            #[cfg(feature = "test-utils")]
            retirement_observations: RefCell::new(Vec::new()),
            retirement_generations_high_water: Cell::new(0),
            search_debounce: Debounce::default(),
            glob_debounce: Debounce::default(),
            workspace_folders: RefCell::new(Arc::from([])),
            total_matches: Cell::new(0),
            total_files: Cell::new(0),
            searching: Cell::new(false),
            result_capped: Cell::new(false),
            last_progress_count: Cell::new(0),
        }
    }
}

/// Saved-search and recent-history state for the search entry popover.
#[derive(Default)]
pub struct SearchHistoryState {
    /// Guards against spurious searches during construction-time GSettings restore.
    pub constructed_complete: Cell<bool>,
    /// Guards against redundant searches while restoring one saved query spec.
    pub restoring_history: Cell<bool>,
    /// Persisted recent-search entries (most recent first, capped by the service).
    pub history_entries: RefCell<Vec<SearchHistoryEntry>>,
    /// Named saved searches shown in the popover's first section.
    pub saved_searches: RefCell<Vec<SavedSearch>>,
}

/// Replace-preview and undo state for the search panel.
#[derive(Default)]
pub struct SearchPreviewState {
    /// Whether the results list currently renders preview rows with checkboxes.
    pub preview_mode: Cell<bool>,
    /// In-memory before/after file snapshots after a successful Replace All.
    pub undo_backup: RefCell<Option<Arc<super::GuardedReplaceUndoBackup>>>,
    /// Generation counter invalidating stale backup loads and deletes.
    pub undo_backup_generation: Arc<AtomicU32>,
    /// One paced wakeup while an on-disk undo journal awaits disposal capacity.
    pub(super) undo_capacity_wakeup: crate::ui::plain_disposal::DisposalCapacityWakeup,
    /// Whether one Replace All apply or undo transaction owns journal mutation.
    pub replace_transaction_pending: Cell<bool>,
    /// Reserved generation awaiting handoff from preview selection to file apply.
    pub replace_transaction_generation: Cell<Option<u32>>,
    /// Generation counter invalidating stale async preview generation results.
    pub preview_generation: Cell<u32>,
    /// Whether pure replacement preview construction is currently running.
    pub preview_pending: Cell<bool>,
    /// Whether this panel currently owns one active preview worker.
    pub preview_worker_running: Cell<bool>,
    /// Cancellation token for the active worker; superseding generations set it immediately.
    pub preview_cancel_token: RefCell<Option<Arc<AtomicBool>>>,
    /// Latest superseding request retained while the single worker is active.
    pub(super) queued_preview_request: RefCell<Option<super::replace::ReplacePreviewRequest>>,
    /// One paced capacity wakeup for the latest preview request.
    pub(super) preview_capacity_wakeup: crate::ui::plain_disposal::DisposalCapacityWakeup,
    /// Accepted bounded preview plus its dense generation-scoped identity map.
    pub preview_outcome:
        RefCell<Option<crate::ui::plain_disposal::DisposalOwned<ReplacePreviewOutcome>>>,
    /// Stable identities of generated preview rows the user chose to apply.
    pub checked_match_ids: RefCell<HashSet<SearchMatchId>>,
    /// Number of document-sized preview outcomes handed to worker retirement.
    pub preview_retirement_jobs: Cell<u64>,
    /// Superseded preview payloads still awaiting final off-main destruction.
    pub preview_retirement_pending: Arc<AtomicUsize>,
    /// Number of checked-row partitions executed on the worker lane.
    pub preview_selection_jobs: Cell<u64>,
}

/// Match-navigation state used by F4 / Shift+F4.
#[derive(Default)]
pub struct SearchNavigationState {
    /// Flat navigation index in match arrival order.
    pub match_positions: RefCell<Vec<SearchMatchLocation>>,
    /// Visible tree rows for each navigation target, captured when rows are appended.
    pub match_rows: RefCell<Vec<Option<gtk4::TreeListRow>>>,
    /// File-header rows keyed by absolute file path for direct child-row lookup.
    pub file_rows: RefCell<BTreeMap<PathBuf, gtk4::TreeListRow>>,
    /// Current position in `match_positions` for wraparound navigation.
    pub current_match_index: Cell<Option<usize>>,
}

/// Callback glue owned by the search panel but provided by the window shell.
#[derive(Default)]
pub struct SearchCallbacks {
    /// Called when the user activates a match result: (file_path, line_number).
    pub open_file_callback: RefCell<Option<OpenFileCallback>>,
    /// Called when the user presses Escape or the close button.
    pub close_requested_callback: RefCell<Option<Box<dyn Fn()>>>,
    /// Called when F4 / Shift+F4 navigates to a match.
    pub navigate_callback: RefCell<Option<NavigateCallback>>,
    /// Called on search progress and completion.
    pub progress_callback: RefCell<Option<ProgressCallback>>,
    /// Called when "Confirm Replace" is clicked with checked replacements.
    pub replace_callback: RefCell<Option<ReplaceCallback>>,
    /// Called when "Undo" is clicked with the backup to restore.
    pub undo_callback: RefCell<Option<UndoCallback>>,
    /// Called when the panel needs to surface a short status message.
    pub message_callback: RefCell<Option<MessageCallback>>,
}

/// Private template implementation for the workspace search panel.
///
/// Owns the template controls plus Rust state for streaming search, saved-search
/// dropdowns, replace preview, undo, and window callbacks.
#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/search-panel.ui")]
pub struct LushtextSearchPanel {
    /// Search input that drives debounced workspace search.
    #[template_child]
    pub search_entry: TemplateChild<gtk4::SearchEntry>,
    /// Hierarchical results list backed by a TreeListModel.
    #[template_child]
    pub results_list: TemplateChild<gtk4::ListView>,
    /// Revealer for empty, progress, and error feedback.
    #[template_child]
    pub results_feedback_revealer: TemplateChild<gtk4::Revealer>,
    /// Revealer for the actual grouped results body.
    #[template_child]
    pub results_body_revealer: TemplateChild<gtk4::Revealer>,
    /// Scrolled container whose height is clamped by the window layout.
    #[template_child]
    pub results_scroll: TemplateChild<gtk4::ScrolledWindow>,
    /// Summary label for match counts and capped-result warnings.
    #[template_child]
    pub count_label: TemplateChild<gtk4::Label>,
    /// Error label shown when search or replace cannot proceed.
    #[template_child]
    pub error_label: TemplateChild<gtk4::Label>,
    /// Toggle for case-sensitive search.
    #[template_child]
    pub case_toggle: TemplateChild<gtk4::ToggleButton>,
    /// Toggle for regex search mode.
    #[template_child]
    pub regex_toggle: TemplateChild<gtk4::ToggleButton>,
    /// Toggle for whole-word matching.
    #[template_child]
    pub word_toggle: TemplateChild<gtk4::ToggleButton>,
    /// Toggle that reveals advanced search options.
    #[template_child]
    pub more_toggle: TemplateChild<gtk4::ToggleButton>,
    /// Revealer containing glob and gitignore options.
    #[template_child]
    pub options_revealer: TemplateChild<gtk4::Revealer>,
    /// Toggle controlling whether gitignore rules are honored.
    #[template_child]
    pub gitignore_toggle: TemplateChild<gtk4::ToggleButton>,
    /// Glob filter entry for workspace search.
    #[template_child]
    pub glob_entry: TemplateChild<gtk4::Entry>,
    /// Replacement text entry used by Replace All preview.
    #[template_child]
    pub replace_entry: TemplateChild<gtk4::Entry>,
    /// Button that starts the checked Replace All operation.
    #[template_child]
    pub replace_all_button: TemplateChild<gtk4::Button>,
    /// Button that restores the latest Replace All undo backup.
    #[template_child]
    pub undo_button: TemplateChild<gtk4::Button>,
    /// Button that saves the current search.
    #[template_child]
    pub save_button: TemplateChild<gtk4::Button>,
    /// Button that hides the search panel.
    #[template_child]
    pub close_button: TemplateChild<gtk4::Button>,
    /// Separator between result summary chrome and result rows.
    #[template_child]
    pub results_header_separator: TemplateChild<gtk4::Separator>,
    /// Separator above the replace/undo footer.
    #[template_child]
    pub results_footer_separator: TemplateChild<gtk4::Separator>,
    /// Footer containing replace, undo, and saved-search controls.
    #[template_child]
    pub footer_box: TemplateChild<gtk4::Box>,

    /// Dropdown popover for saved searches + recent history. Parented to search_entry.
    pub history_popover: gtk4::Popover,
    /// Container box inside the popover holding both sections.
    pub dropdown_box: gtk4::Box,
    /// "Saved Searches" section header label.
    pub saved_header: gtk4::Label,
    /// List box for saved search entries.
    pub saved_searches_list: gtk4::ListBox,
    /// Separator between saved searches and recent history sections.
    pub dropdown_separator: gtk4::Separator,
    /// "Recent" section header label.
    pub recent_header: gtk4::Label,
    /// List box for recent search history entries.
    pub history_list: gtk4::ListBox,

    /// GSettings instance for search toggle persistence.
    pub settings: gio::Settings,
    /// Search execution, grouped result models, and debounce counters.
    pub runtime: SearchRuntimeState,
    /// Saved-search dropdown state and restore guards.
    pub history: SearchHistoryState,
    /// Replace-preview and undo state.
    pub preview: SearchPreviewState,
    /// Flat match navigation state.
    pub navigation: SearchNavigationState,
    /// Window-provided callback glue.
    pub callbacks: SearchCallbacks,
    /// Throttles spoken workspace-search result summaries.
    pub results_announcement_throttler: accessibility::AnnouncementThrottler,
}

impl Default for LushtextSearchPanel {
    fn default() -> Self {
        Self {
            search_entry: TemplateChild::default(),
            results_list: TemplateChild::default(),
            results_feedback_revealer: TemplateChild::default(),
            results_body_revealer: TemplateChild::default(),
            results_scroll: TemplateChild::default(),
            count_label: TemplateChild::default(),
            error_label: TemplateChild::default(),
            case_toggle: TemplateChild::default(),
            regex_toggle: TemplateChild::default(),
            word_toggle: TemplateChild::default(),
            more_toggle: TemplateChild::default(),
            options_revealer: TemplateChild::default(),
            gitignore_toggle: TemplateChild::default(),
            glob_entry: TemplateChild::default(),
            replace_entry: TemplateChild::default(),
            replace_all_button: TemplateChild::default(),
            undo_button: TemplateChild::default(),
            save_button: TemplateChild::default(),
            close_button: TemplateChild::default(),
            results_header_separator: TemplateChild::default(),
            results_footer_separator: TemplateChild::default(),
            footer_box: TemplateChild::default(),
            history_popover: {
                let popover = gtk4::Popover::new();
                popover.set_autohide(true);
                popover.set_has_arrow(false);
                popover
            },
            dropdown_box: gtk4::Box::new(gtk4::Orientation::Vertical, 0),
            saved_header: {
                let label = gtk4::Label::new(Some("Saved Searches"));
                label.add_css_class("heading");
                label.set_halign(gtk4::Align::Start);
                label.set_margin_start(8);
                label.set_margin_top(6);
                label.set_margin_bottom(4);
                label
            },
            saved_searches_list: {
                let list = gtk4::ListBox::new();
                list.set_selection_mode(gtk4::SelectionMode::Single);
                list.set_activate_on_single_click(true);
                list
            },
            dropdown_separator: gtk4::Separator::new(gtk4::Orientation::Horizontal),
            recent_header: {
                let label = gtk4::Label::new(Some("Recent"));
                label.add_css_class("heading");
                label.set_halign(gtk4::Align::Start);
                label.set_margin_start(8);
                label.set_margin_top(6);
                label.set_margin_bottom(4);
                label
            },
            history_list: {
                let list = gtk4::ListBox::new();
                list.set_selection_mode(gtk4::SelectionMode::Single);
                list.set_activate_on_single_click(true);
                list
            },
            settings: gio::Settings::new(crate::config::APP_ID),
            runtime: SearchRuntimeState::default(),
            history: SearchHistoryState::default(),
            preview: SearchPreviewState::default(),
            navigation: SearchNavigationState::default(),
            callbacks: SearchCallbacks::default(),
            results_announcement_throttler: accessibility::AnnouncementThrottler::default(),
        }
    }
}

#[glib::object_subclass]
// Register the search panel as a GtkBox subclass so the window can mount it as
// ordinary chrome while this implementation owns its template children.
impl ObjectSubclass for LushtextSearchPanel {
    const NAME: &str = "LushtextSearchPanel";
    type Type = super::LushtextSearchPanel;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextSearchPanel {
    fn constructed(&self) {
        self.parent_constructed();

        // Assemble dropdown popover: sections → box → scrolled window → popover → parent to entry.
        self.dropdown_box.append(&self.saved_header);
        self.dropdown_box.append(&self.saved_searches_list);
        self.dropdown_box.append(&self.dropdown_separator);
        self.dropdown_box.append(&self.recent_header);
        self.dropdown_box.append(&self.history_list);

        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_max_content_height(300);
        scroll.set_propagate_natural_height(true);
        scroll.set_child(Some(&self.dropdown_box));
        self.history_popover.set_child(Some(&scroll));
        self.history_popover.set_parent(&*self.search_entry);

        self.setup_results_list();
        self.setup_search_entry();
        self.setup_toggles();
        self.setup_options();
        self.setup_history();
        self.setup_save_button();
        self.apply_accessibility_metadata();
        self.obj().refresh_accessibility_state();
        self.obj().load_persisted_undo_backup();
        self.history.constructed_complete.set(true);
    }

    fn dispose(&self) {
        // Cancel any in-flight search thread so it stops promptly after
        // the window closes, instead of running until the walker finishes.
        if let Some(cancel) = self.runtime.cancel_token.take() {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(retired) = self.preview.undo_backup.take() {
            drop(retired);
        }
        self.preview.preview_capacity_wakeup.cancel();
        self.preview.undo_capacity_wakeup.cancel();
        // Unparent the programmatically-parented popover to avoid leak warnings.
        self.history_popover.unparent();
    }
}

impl WidgetImpl for LushtextSearchPanel {}
impl BoxImpl for LushtextSearchPanel {}

impl LushtextSearchPanel {
    /// Bind toggle buttons to GSettings for persistence and wire immediate
    /// re-search on toggle change. Must be called AFTER `setup_search_entry`
    /// and BEFORE `constructed_complete` is set to `true`.
    fn setup_toggles(&self) {
        use crate::config::keys;

        // Two-way GSettings bindings: persist toggle state across sessions.
        // These fire notify::active with the restored value.
        self.settings
            .bind(keys::SEARCH_CASE_SENSITIVE, &*self.case_toggle, "active")
            .build();
        self.settings
            .bind(keys::SEARCH_REGEX, &*self.regex_toggle, "active")
            .build();
        self.settings
            .bind(keys::SEARCH_WHOLE_WORD, &*self.word_toggle, "active")
            .build();

        // Immediate re-search when a user flips a toggle. Connect after
        // GSettings binding and keep a restore guard so startup/history replay
        // notifications do not accidentally launch a workspace search.
        for toggle in [&*self.case_toggle, &*self.regex_toggle, &*self.word_toggle] {
            let panel_weak = self.obj().downgrade();
            toggle.connect_notify_local(Some("active"), move |_, _| {
                let Some(panel) = panel_weak.upgrade() else {
                    return;
                };
                let imp = panel.imp();
                if !imp.history.constructed_complete.get() || imp.history.restoring_history.get() {
                    return; // GSettings restore or history restore — skip.
                }
                let spec = panel.current_query_spec();
                if !spec.query.is_empty() {
                    panel.start_search(&spec);
                }
            });
        }
    }

    /// Wire the "More" button to the options revealer, bind gitignore toggle
    /// to GSettings, and set up glob entry debounce. Must be called AFTER
    /// `setup_toggles` and BEFORE `constructed_complete` is set to `true`.
    fn setup_options(&self) {
        use crate::config::keys;

        // 1. GSettings bind: restores persisted expanded state to more_toggle.
        self.settings
            .bind(
                keys::SEARCH_PANEL_OPTIONS_EXPANDED,
                &*self.more_toggle,
                "active",
            )
            .build();

        // 2. GSettings bind: restores persisted gitignore state.
        self.settings
            .bind(keys::SEARCH_GITIGNORE, &*self.gitignore_toggle, "active")
            .build();

        // 3. Propagate more_toggle.active → options_revealer.reveal_child.
        // sync_create ensures the revealer reflects the restored GSettings state.
        self.more_toggle
            .bind_property("active", &*self.options_revealer, "reveal-child")
            .sync_create()
            .build();

        // 4. Immediate re-search when gitignore toggle changes (same as case/regex/word).
        let panel_weak = self.obj().downgrade();
        self.gitignore_toggle
            .connect_notify_local(Some("active"), move |_, _| {
                let Some(panel) = panel_weak.upgrade() else {
                    return;
                };
                let imp = panel.imp();
                if !imp.history.constructed_complete.get() || imp.history.restoring_history.get() {
                    return; // GSettings restore or history restore — skip.
                }
                let spec = panel.current_query_spec();
                if !spec.query.is_empty() {
                    panel.start_search(&spec);
                }
            });

        // 5. Replace All / Confirm Replace button.
        let panel_weak = self.obj().downgrade();
        self.replace_all_button.connect_clicked(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            if panel.imp().preview.preview_mode.get() {
                panel.activate_confirm_replacements();
            } else {
                panel.activate_replace_preview();
            }
        });

        // 6. Undo button.
        let panel_weak = self.obj().downgrade();
        self.undo_button.connect_clicked(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            panel.activate_undo_replacements();
        });

        // 7. Replace entry: update button sensitivity on text change.
        let panel_weak = self.obj().downgrade();
        self.replace_entry.connect_changed(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            panel.invalidate_replace_preview_request();
            panel.update_replace_button_sensitivity();
        });

        // 8. Glob entry: 300ms debounce.
        let panel_weak = self.obj().downgrade();
        self.glob_entry.connect_changed(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            panel.invalidate_replace_preview_request();
            let imp = panel.imp();
            if imp.history.restoring_history.get() {
                return; // History restore — skip debounce.
            }

            imp.runtime.glob_debounce.schedule(
                &panel,
                Duration::from_millis(SEARCH_INPUT_DEBOUNCE_MS),
                move |panel, _| {
                    let spec = panel.current_query_spec();
                    if !spec.query.is_empty() {
                        panel.start_search(&spec);
                    }
                },
            );
        });
    }

    /// Give the workspace-search controls explicit names that screen readers
    /// and AT-SPI smoke helpers can target without relying on widget order.
    fn apply_accessibility_metadata(&self) {
        accessibility::set_labelled_description(
            &*self.search_entry,
            "Workspace search query",
            "Search across workspace files",
        );
        accessibility::set_has_popup(&*self.search_entry, true);
        accessibility::set_role(&*self.results_list, gtk4::AccessibleRole::List);
        accessibility::set_labelled_description(
            &*self.results_list,
            "Workspace search results",
            "Matching files and lines",
        );
        accessibility::set_role(&*self.count_label, gtk4::AccessibleRole::Status);
        accessibility::set_labelled_description(
            &*self.count_label,
            "Workspace search result count",
            "Current workspace search status and result total",
        );
        accessibility::set_role(&*self.error_label, gtk4::AccessibleRole::Alert);
        accessibility::set_labelled_description(
            &*self.error_label,
            "Workspace search error",
            "Problem reported by the current workspace search",
        );

        for (toggle, label) in [
            (&*self.case_toggle, "Match case"),
            (&*self.regex_toggle, "Use regular expression"),
            (&*self.word_toggle, "Match whole words"),
            (&*self.more_toggle, "Search options"),
            (&*self.gitignore_toggle, "Respect gitignore"),
        ] {
            accessibility::set_label(toggle, label);
            accessibility::set_pressed(toggle, toggle.is_active());
            toggle.connect_active_notify(|toggle| {
                accessibility::set_pressed(toggle, toggle.is_active());
            });
        }
        accessibility::set_controls(
            &*self.more_toggle,
            &[self.options_revealer.upcast_ref::<gtk4::Accessible>()],
        );

        accessibility::set_labelled_description(
            &*self.glob_entry,
            "File glob filter",
            "Limit workspace search to matching paths",
        );
        accessibility::set_labelled_description(
            &*self.replace_entry,
            "Workspace replacement text",
            "Replacement text for workspace matches",
        );
        accessibility::set_label(&*self.replace_all_button, "Replace all matches");
        accessibility::set_label(&*self.undo_button, "Undo replacements");
        accessibility::set_label(&*self.save_button, "Save search");
        accessibility::set_label(&*self.close_button, "Close workspace search");
        accessibility::set_key_shortcuts(&*self.close_button, "Escape");
        accessibility::set_role(&self.saved_searches_list, gtk4::AccessibleRole::List);
        accessibility::set_label(&self.saved_searches_list, "Saved workspace searches");
        accessibility::set_role(&self.history_list, gtk4::AccessibleRole::List);
        accessibility::set_label(&self.history_list, "Recent workspace searches");
    }

    /// Set up the search entry with debounced search triggering.
    fn setup_search_entry(&self) {
        let panel_weak = self.obj().downgrade();
        self.search_entry.connect_search_changed(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            let imp = panel.imp();

            // Dismiss the history dropdown when the user edits the query.
            if imp.history_popover.is_visible() {
                imp.history_popover.popdown();
            }

            // Suppress search during history restore (guard pattern).
            if imp.history.restoring_history.get() {
                return;
            }

            panel.invalidate_replace_preview_request();

            let spec = panel.current_query_spec();
            if spec.query.is_empty() {
                let _ = imp.runtime.search_debounce.invalidate();
                panel.start_search(&spec);
                return;
            }

            imp.runtime.search_debounce.schedule(
                &panel,
                Duration::from_millis(SEARCH_INPUT_DEBOUNCE_MS),
                move |panel, _| {
                    let spec = panel.current_query_spec();
                    panel.start_search(&spec);
                },
            );
        });

        // Escape key: signal close request.
        let panel_weak = self.obj().downgrade();
        self.search_entry.connect_stop_search(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            if let Some(ref cb) = *panel.imp().callbacks.close_requested_callback.borrow() {
                cb();
            }
        });

        // Close button: same as Escape.
        let panel_weak = self.obj().downgrade();
        self.close_button.connect_clicked(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            if let Some(ref cb) = *panel.imp().callbacks.close_requested_callback.borrow() {
                cb();
            }
        });
    }

    /// Wire focus → popover show and row-activated → restore history entry.
    fn setup_history(&self) {
        // Show history dropdown when search_entry gains focus.
        let panel_weak = self.obj().downgrade();
        self.search_entry
            .connect_notify_local(Some("has-focus"), move |entry, _| {
                let Some(panel) = panel_weak.upgrade() else {
                    return;
                };
                let imp = panel.imp();
                // Only show on focus-in (not focus-out), when entries exist,
                // and not during preview mode.
                let has_entries = !imp.history.history_entries.borrow().is_empty()
                    || !imp.history.saved_searches.borrow().is_empty();
                if entry.has_focus() && has_entries && !imp.preview.preview_mode.get() {
                    panel.populate_dropdown();
                    imp.history_popover.popup();
                }
            });

        // Row activated in history list → restore state and trigger search.
        let panel_weak = self.obj().downgrade();
        self.history_list.connect_row_activated(move |_, row| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            #[expect(
                clippy::cast_sign_loss,
                reason = "GtkListBoxRow indices are non-negative when a row exists"
            )]
            let idx = row.index() as usize;
            let entry = {
                let entries = panel.imp().history.history_entries.borrow();
                entries.get(idx).cloned()
            };
            if let Some(entry) = entry {
                panel.restore_from_history(&entry);
            }
        });

        // Row activated in saved searches list → restore state and trigger search.
        let panel_weak = self.obj().downgrade();
        self.saved_searches_list
            .connect_row_activated(move |_, row| {
                let Some(panel) = panel_weak.upgrade() else {
                    return;
                };
                #[expect(
                    clippy::cast_sign_loss,
                    reason = "GtkListBoxRow indices are non-negative when a row exists"
                )]
                let idx = row.index() as usize;
                let entry = {
                    let entries = panel.imp().history.saved_searches.borrow();
                    entries.get(idx).cloned()
                };
                if let Some(entry) = entry {
                    panel.restore_from_saved_search(&entry);
                }
            });
    }

    /// Wire the save button to open the save search dialog.
    fn setup_save_button(&self) {
        let panel_weak = self.obj().downgrade();
        self.save_button.connect_clicked(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            panel.show_save_search_dialog();
        });
    }
}

/// Compute a display-friendly relative path for a result file.
/// Strips the workspace folder prefix for readability.
pub fn make_display_path(path: &Path, workspace_folders: &[PathBuf]) -> String {
    for folder in workspace_folders {
        if let Ok(relative) = path.strip_prefix(folder) {
            return relative.display().to_string();
        }
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_path_uses_first_covering_workspace_folder() {
        let workspace_folder = PathBuf::from("/repo");
        let nested_folder = PathBuf::from("/repo/src");
        let path = Path::new("/repo/src/main.rs");

        assert_eq!(
            make_display_path(path, &[workspace_folder, nested_folder.clone()]),
            Path::new("src/main.rs").display().to_string()
        );
        assert_eq!(
            make_display_path(path, &[nested_folder]),
            Path::new("main.rs").display().to_string()
        );
    }
}
