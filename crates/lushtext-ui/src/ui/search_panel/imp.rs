// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the workspace search panel.
//!
//! Handles template binding, GSettings persistence, debounced entry input,
//! construction, and disposal while delegating coordination and presentation
//! roles to sibling modules.
//!
//! # Role
//!
//! This module carries **no role**. It is a **called presentation surface** of
//! `WFR-SEARCH-REPLACE` — it projects the workflow onto widgets (subclass state,
//! template children, construction, and disposal) — so under
//! `gtk-adapter-module-boundaries` it is outside the five-name role taxonomy,
//! takes none of those names, and owns no `policy.rs` and no `evidence.rs`. Its
//! behavior obligations are unchanged. Named in that workflow's matrix row.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use gtk4::prelude::*;
use gtk4::{self, CompositeTemplate, gio, glib};
use libadwaita::subclass::prelude::*;
use lushtext_model::workspace::workspace_visibility::WorkspaceEntryVisibility;

use super::execution::{SearchCallbacks, SearchExecutionPhase, SearchRuntimeState};
use super::history::{
    SearchHistoryState, create_dropdown_heading, create_dropdown_list, create_history_popover,
};
use super::journal::JournalState;
use super::replace_admission::{ReplaceAdmissionPhase, ReplaceAdmissionState};
use super::replace_execution::ReplaceExecutionState;
use super::results::SearchNavigationState;
use super::retirement::SearchRetirementState;
use super::skipped_files::SkipReportState;
use crate::ui::accessibility::AnnouncementThrottler;

/// Delay text-entry searches long enough to batch normal typing without making
/// the workspace search panel feel sluggish.
const SEARCH_INPUT_DEBOUNCE_MS: u64 = 300;

/// Private template implementation for the workspace search panel.
///
/// Owns the template controls plus coordination state handles for streaming
/// search, saved-search dropdowns, replace preview, undo, and window callbacks.
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
    /// Per-search override for dotfile visibility, seeded from the global key.
    #[template_child]
    pub hidden_toggle: TemplateChild<gtk4::ToggleButton>,
    /// Glob filter entry for workspace search.
    #[template_child]
    pub glob_entry: TemplateChild<gtk4::Entry>,
    /// Replacement text entry used by Replace All preview.
    #[template_child]
    pub replace_entry: TemplateChild<gtk4::Entry>,
    /// Button that starts the checked Replace All operation.
    #[template_child]
    pub replace_all_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub skipped_files_button: TemplateChild<gtk4::Button>,
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
    /// Bounded detached-result retirement state.
    pub(super) retirement: SearchRetirementState,
    /// Saved-search dropdown state and restore guards.
    pub history: SearchHistoryState,
    /// Replace-preview, selection, and run state.
    pub replace_execution: ReplaceExecutionState,
    /// Replace stage-4 disposal admission state.
    pub(super) replace_admission: ReplaceAdmissionState,
    /// Last Replace All run's skip report and open dialog weak handle.
    pub(super) skipped_files: SkipReportState,
    /// Durable undo journal and transaction-gate state.
    pub journal: JournalState,
    /// Flat match navigation state.
    pub navigation: SearchNavigationState,
    /// Window-provided callback glue.
    pub callbacks: SearchCallbacks,
    /// Throttles spoken workspace-search result summaries.
    pub results_announcement_throttler: AnnouncementThrottler,
    /// The result rows the list currently has bound, keyed by list item, so a
    /// mode switch re-projects only them (`list_factory::rebind_bound_result_rows`).
    pub bound_result_items: RefCell<HashMap<usize, glib::WeakRef<gtk4::ListItem>>>,
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
            hidden_toggle: TemplateChild::default(),
            glob_entry: TemplateChild::default(),
            replace_entry: TemplateChild::default(),
            replace_all_button: TemplateChild::default(),
            skipped_files_button: TemplateChild::default(),
            undo_button: TemplateChild::default(),
            save_button: TemplateChild::default(),
            close_button: TemplateChild::default(),
            results_header_separator: TemplateChild::default(),
            results_footer_separator: TemplateChild::default(),
            footer_box: TemplateChild::default(),
            history_popover: create_history_popover(),
            dropdown_box: gtk4::Box::new(gtk4::Orientation::Vertical, 0),
            saved_header: create_dropdown_heading("Saved Searches"),
            saved_searches_list: create_dropdown_list(),
            dropdown_separator: gtk4::Separator::new(gtk4::Orientation::Horizontal),
            recent_header: create_dropdown_heading("Recent"),
            history_list: create_dropdown_list(),
            settings: crate::ui::settings_writes::shared(),
            runtime: SearchRuntimeState::default(),
            retirement: SearchRetirementState::default(),
            history: SearchHistoryState::default(),
            replace_execution: ReplaceExecutionState::default(),
            replace_admission: ReplaceAdmissionState::default(),
            skipped_files: SkipReportState::default(),
            journal: JournalState::default(),
            navigation: SearchNavigationState::default(),
            callbacks: SearchCallbacks::default(),
            results_announcement_throttler: AnnouncementThrottler::default(),
            bound_result_items: RefCell::new(HashMap::new()),
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
        let obj = self.obj();
        obj.register_application_panel();
        for entry in [
            self.search_entry.upcast_ref::<gtk4::Editable>(),
            self.glob_entry.upcast_ref(),
            self.replace_entry.upcast_ref(),
        ] {
            crate::ui::entry_paste_cap::cap_single_line_inserts(entry);
        }

        self.assemble_history_dropdown();
        self.setup_results_list();
        self.setup_search_entry();
        self.setup_toggles();
        self.setup_options();
        self.setup_history();
        self.setup_save_button();
        self.apply_accessibility_metadata();
        obj.refresh_accessibility_state();
        obj.load_persisted_undo_backup();
        self.history.constructed_complete.set(true);
    }

    fn dispose(&self) {
        // Cancel any in-flight search thread so it stops promptly after
        // the window closes, instead of running until the walker finishes.
        if let SearchExecutionPhase::Running { cancel_token, .. } = self.runtime.phase.take() {
            cancel_token.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.replace_execution.preview_capacity_wakeup.cancel();
        // A Replace All waiting for disposal capacity ends here without
        // calling back into a window being destroyed: its selection's drop
        // hands the payload to the lane, no file was written, and the
        // application's transaction is released so another window may run.
        self.replace_admission.wakeup.cancel();
        if let ReplaceAdmissionPhase::Parked(parked) = self.replace_admission.phase.take() {
            drop(parked);
        }
        self.obj().release_replace_transaction_on_dispose();
        // Unparent the programmatically-parented popover to avoid leak warnings.
        self.history_popover.unparent();
    }
}

impl WidgetImpl for LushtextSearchPanel {}
impl BoxImpl for LushtextSearchPanel {}

impl LushtextSearchPanel {
    /// Build the excluded-name rule for one search from the persisted keys.
    ///
    /// Only the excluded names come from settings here; the dotfile axis is the
    /// panel's per-search toggle, carried in `ContentSearchOptions::hidden`.
    pub(super) fn current_entry_visibility(&self) -> WorkspaceEntryVisibility {
        crate::ui::workspace_visibility::workspace_entry_visibility(&self.settings)
    }

    /// Bind toggle buttons to GSettings for persistence and wire immediate
    /// re-search on toggle change. Must be called AFTER `setup_search_entry`
    /// and BEFORE `constructed_complete` is set to `true`.
    fn setup_toggles(&self) {
        use crate::config::keys;

        self.settings
            .bind(keys::SEARCH_CASE_SENSITIVE, &*self.case_toggle, "active")
            .build();
        self.settings
            .bind(keys::SEARCH_REGEX, &*self.regex_toggle, "active")
            .build();
        self.settings
            .bind(keys::SEARCH_WHOLE_WORD, &*self.word_toggle, "active")
            .build();

        for toggle in [
            &*self.case_toggle,
            &*self.regex_toggle,
            &*self.word_toggle,
            &*self.gitignore_toggle,
            &*self.hidden_toggle,
        ] {
            let panel_weak = self.obj().downgrade();
            toggle.connect_notify_local(Some("active"), move |_, _| {
                let Some(panel) = panel_weak.upgrade() else {
                    return;
                };
                let imp = panel.imp();
                if !imp.history.constructed_complete.get() || imp.history.restoring_history.get() {
                    return;
                }
                let spec = panel.current_query_spec();
                if !spec.query.is_empty() {
                    panel.start_search(&spec);
                }
            });
        }
    }

    /// Wire the "More" button to the options revealer, bind gitignore toggle
    /// to GSettings, wire replace/undo buttons, and set up glob entry debounce.
    fn setup_options(&self) {
        use crate::config::keys;

        self.settings
            .bind(
                keys::SEARCH_PANEL_OPTIONS_EXPANDED,
                &*self.more_toggle,
                "active",
            )
            .build();
        self.settings
            .bind(keys::SEARCH_GITIGNORE, &*self.gitignore_toggle, "active")
            .build();
        self.more_toggle
            .bind_property("active", &*self.options_revealer, "reveal-child")
            .sync_create()
            .build();

        let panel_weak = self.obj().downgrade();
        self.obj().connect_map(move |_| {
            if let Some(panel) = panel_weak.upgrade() {
                panel.seed_hidden_toggle();
            }
        });
        crate::ui::settings_writes::connect_changed_for(
            &*self.obj(),
            crate::config::keys::WORKSPACE_SHOW_HIDDEN_FILES,
            |panel, _| {
                if panel.is_mapped() && !panel.is_searching() {
                    panel.seed_hidden_toggle();
                }
            },
        );

        self.obj().setup_replace_controls();

        let panel_weak = self.obj().downgrade();
        self.glob_entry.connect_changed(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            panel.invalidate_replace_preview_request();
            let imp = panel.imp();
            if imp.history.restoring_history.get() {
                return;
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

    /// Set up the search entry with debounced search triggering.
    fn setup_search_entry(&self) {
        let panel_weak = self.obj().downgrade();
        self.search_entry.connect_search_changed(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            let imp = panel.imp();

            if imp.history_popover.is_visible() {
                imp.history_popover.popdown();
            }
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

        let panel_weak = self.obj().downgrade();
        self.search_entry.connect_stop_search(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            let cb = panel
                .imp()
                .callbacks
                .close_requested_callback
                .borrow()
                .clone();
            if let Some(cb) = cb {
                cb();
            }
        });

        let panel_weak = self.obj().downgrade();
        self.close_button.connect_clicked(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            let cb = panel
                .imp()
                .callbacks
                .close_requested_callback
                .borrow()
                .clone();
            if let Some(cb) = cb {
                cb();
            }
        });
    }
}
