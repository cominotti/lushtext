// SPDX-License-Identifier: GPL-3.0-or-later

//! Window-side recording of the construction spans: each stage of
//! `LushtextWindow::new` and `constructed`, the end of the constructing
//! dispatch, and the first painted frame.
//!
//! **Cross-cutting coordination with no workflow role**: it serves every
//! workflow that builds part of the window, and owns no decision. The data
//! lives in `model::construction_spans`; `ui::construction_spans` owns the
//! clock and the process-wide startup spans. The shell-geometry evidence
//! (`geometry::evidence`) projects the result.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_settle::{Debounce, SettleBurst};
use gtk4::prelude::*;
use gtk4::{TemplateChild, glib};
use lushtext_model::workspace::WorkspaceScope;
use lushtext_services::coordination::notifications::NotificationBus;

use super::LushtextWindow;
use super::drafts::DraftState;
use super::editor_memory_eviction::EditorMemoryState;
use super::focus_mode::FocusModeState;
use super::geometry::execution::{
    current_window_width, effective_properties_fraction, effective_workspace_sidebar_fraction,
    set_workspace_sidebar_preset, sync_properties_breakpoint, sync_properties_split_view,
    sync_secondary_surfaces,
};
use super::geometry::{SecondarySurfaceState, SplitLayoutHandles};
use super::recent_documents_journal::RecentDocumentsState;
use super::search_progress_execution::SearchProgressState;
use super::session_restore::SessionState;
use super::startup_data::StartupDataFlowState;
use super::tab_strip::TabManagementState;
use crate::config::keys;
use crate::ui::accessibility;
use crate::ui::construction_spans;
use crate::ui::markdown_preview::PREVIEW_DEFAULT_WIDTH_SP;
use crate::ui::settings_writes;
use crate::ui::sidebar::width_preset::WorkspaceSidebarWidthPreset;
use crate::ui::status_bar::MessageKind;

impl Default for super::imp::LushtextWindow {
    fn default() -> Self {
        Self {
            header_bar: TemplateChild::default(),
            title_widget: TemplateChild::default(),
            new_tab_button: TemplateChild::default(),
            open_menu_button: TemplateChild::default(),
            open_button_stack: TemplateChild::default(),
            open_popover: TemplateChild::default(),
            document_properties_toggle_button: TemplateChild::default(),
            tab_bar: TemplateChild::default(),
            window_overlay: TemplateChild::default(),
            workspace_split_view: TemplateChild::default(),
            properties_layout_view: TemplateChild::default(),
            properties_bottom_sheet: TemplateChild::default(),
            properties_split_view: TemplateChild::default(),
            tab_view: TemplateChild::default(),
            content_stack: TemplateChild::default(),
            sidebar: TemplateChild::default(),
            properties_panel: TemplateChild::default(),
            status_bar: TemplateChild::default(),
            palette_revealer: TemplateChild::default(),
            command_palette: TemplateChild::default(),
            focus_mode_revealer: TemplateChild::default(),
            focus_mode_affordance: TemplateChild::default(),
            leave_focus_mode_button: TemplateChild::default(),
            notes_menu_button: TemplateChild::default(),
            primary_menu_button: TemplateChild::default(),
            preview_layout_view: TemplateChild::default(),
            preview_split_view: TemplateChild::default(),
            editor_box: TemplateChild::default(),
            markdown_preview: TemplateChild::default(),
            preview_document_alert: TemplateChild::default(),
            content_box: TemplateChild::default(),
            search_panel_revealer: TemplateChild::default(),
            search_panel: TemplateChild::default(),
            settings: crate::ui::settings_writes::shared(),
            construction: RefCell::default(),
            composition: super::composition::CompositionState::default(),
            #[cfg(feature = "responsiveness-probes")]
            layout_probe_spans: Cell::new(0),
            secondary_surfaces: SecondarySurfaceState::default(),
            preview_visible: Cell::new(false),
            preview_mode: Cell::new(false),
            preferred_preview_width: Cell::new(PREVIEW_DEFAULT_WIDTH_SP),
            preview_sidebar_show_deferred: Cell::new(false),
            preview_transition_settle: SettleBurst::default(),
            preview_render_debounce: Debounce::default(),
            index_rebuild_debounce: Debounce::default(),
            file_index_builds: RefCell::default(),
            file_index_admission: RefCell::default(),
            file_index_capacity_wakeup: crate::ui::plain_disposal::DisposalCapacityWakeup::default(
            ),
            command_palette_notes_refresh_debounce: Debounce::default(),
            command_palette_note_refreshes: RefCell::default(),
            command_palette_note_admission: RefCell::default(),
            command_palette_note_capacity_wakeup:
                crate::ui::plain_disposal::DisposalCapacityWakeup::default(),
            command_palette_note_capture: std::rc::Rc::default(),
            saved_focus: RefCell::new(None),
            transient_child_escape_handled: Cell::new(false),
            user_input_since_map: Cell::new(false),
            opened_document_handoff_pending: Cell::new(false),
            open_paths: RefCell::new(HashSet::new()),
            tab_projection_refresh_defer_depth: Cell::new(0),
            editor_memory: EditorMemoryState::default(),
            disposing: Cell::new(false),
            session: SessionState::default(),
            focus_mode: FocusModeState::default(),
            drafts: DraftState::default(),
            startup_data_flow: StartupDataFlowState::default(),
            recent_documents: RecentDocumentsState::default(),
            tab_management: TabManagementState::default(),
            tab_overflow: super::tab_overflow::TabOverflowState::default(),
            active_notes_browser: RefCell::new(None),
            notes_row_retirements: RefCell::new(Vec::new()),
            note_save_snapshots: RefCell::new(Vec::new()),
            search_saved_focus: RefCell::new(None),
            notification_bus: NotificationBus::default(),
            notification_sweep_source_id: RefCell::new(None),
            notification_projection: super::notifications::NotificationProjectionState::default(),
            search_progress: SearchProgressState::default(),
            workspace_scope: RefCell::new(WorkspaceScope::All),
            split_layout: SplitLayoutHandles::default(),
            split_width_synced_for_width: Cell::new(0),
        }
    }
}

/// Runs the stages of `LushtextWindow::new` one after another, recording each.
pub(in crate::ui::window) struct ConstructionStages<'a> {
    window: &'a LushtextWindow,
    cursor_us: i64,
}

impl<'a> ConstructionStages<'a> {
    /// Start recording stages for `window` from now.
    pub(in crate::ui::window) fn new(window: &'a LushtextWindow) -> Self {
        Self {
            window,
            cursor_us: construction_spans::now_us(),
        }
    }

    /// Run one named stage and record how long it took.
    pub(in crate::ui::window) fn run(&mut self, name: &'static str, stage: impl FnOnce()) {
        stage();
        self.cursor_us = self.window.record_construction_stage(name, self.cursor_us);
    }
}

impl LushtextWindow {
    /// Whether no other LushText window of this window's application exists.
    pub(in crate::ui::window) fn is_last_lushtext_window(&self) -> bool {
        self.application().is_none_or(|app| {
            !app.windows().iter().any(|other| {
                other != self.upcast_ref::<gtk4::Window>() && other.is::<LushtextWindow>()
            })
        })
    }

    /// Record the stage that ran from `start_us` until now, and return now so
    /// the next stage can start there.
    pub(crate) fn record_construction_stage(&self, name: &'static str, start_us: i64) -> i64 {
        let end_us = construction_spans::now_us();
        self.record_stage_span(name, start_us, end_us);
        end_us
    }

    /// Record one stage span and forward it to the evidence stream.
    fn record_stage_span(&self, name: &'static str, start_us: i64, end_us: i64) {
        self.imp()
            .construction
            .borrow_mut()
            .record_stage(name, start_us, end_us);
        construction_spans::emit(name, start_us, end_us);
    }

    /// Begin this window's construction record at `started_us` (the start of
    /// `LushtextWindow::new`, before the template was instantiated), and watch
    /// for the end of the constructing dispatch and the first painted frame.
    pub(in crate::ui::window) fn begin_construction_record(&self, started_us: i64) {
        let template_end_us = {
            let mut construction = self.imp().construction.borrow_mut();
            construction.begin(started_us);
            construction.stages().first().map(|stage| stage.start_us)
        };
        // `constructed` recorded its stages while the object was being built;
        // the time before its first stage is template instantiation.
        if let Some(template_end_us) = template_end_us {
            self.record_stage_span("template", started_us, template_end_us);
        }

        // GLib dispatches a high-priority idle on the first iteration after
        // the dispatch that is building this window, before default-priority
        // work: that is when construction returned control to the main loop.
        let window_weak = self.downgrade();
        glib::idle_add_local_full(glib::Priority::HIGH, move || {
            if let Some(window) = window_weak.upgrade() {
                let end_us = construction_spans::now_us();
                window
                    .imp()
                    .construction
                    .borrow_mut()
                    .finish_construction(end_us);
                construction_spans::emit("window_construction", started_us, end_us);
            }
            glib::ControlFlow::Break
        });

        let window_weak = self.downgrade();
        self.connect_realize(move |_| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            window.watch_first_frame(started_us);
        });
    }

    /// Record the first `after-paint` of this window's frame clock painted
    /// while the window is mapped. A frame that runs between realize and map
    /// is never presented (ledger A51), so it is not the first frame: neither
    /// the evidence nor the settings writer's gate may rest on it.
    fn watch_first_frame(&self, started_us: i64) {
        if self.imp().construction.borrow().first_frame_painted() {
            return;
        }
        let Some(clock) = self.frame_clock() else {
            return;
        };
        let handler = Rc::new(gtk_lush_signals::SignalSlot::new());
        let handler_slot = Rc::clone(&handler);
        let window_weak = self.downgrade();
        let id = clock.connect_after_paint(move |_| {
            let window = window_weak.upgrade();
            if window.as_ref().is_some_and(|window| !window.is_mapped()) {
                return;
            }
            handler_slot.clear();
            let Some(window) = window else {
                return;
            };
            let painted_us = construction_spans::now_us();
            window
                .imp()
                .construction
                .borrow_mut()
                .record_first_frame(painted_us, construction_spans::largest_dispatch_so_far_us());
            construction_spans::emit("first_frame", started_us, painted_us);
            // The settings writer persists nothing before the first frame
            // painted on the mapped window.
            settings_writes::first_frame_presented();
        });
        handler.replace(&clock, id);
    }
}

/// What the window's construction cost, and what the settings writer did.
///
/// Integers only (the automation snapshot projects it as `window_construction`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WindowConstructionEvidence {
    /// GTK's own startup (`parent_startup`), reported, not budgeted.
    pub startup_toolkit_us: Option<u64>,
    /// LushText's startup work.
    pub startup_lushtext_us: Option<u64>,
    /// From `LushtextWindow::new` until control returned to the main loop.
    pub window_construction_us: Option<u64>,
    /// From `LushtextWindow::new` until the window's first frame painted while
    /// it was mapped (never a frame on the realized, unmapped window, A51).
    pub first_frame_us: Option<u64>,
    /// The largest main-loop dispatch before the first frame, when the
    /// measurement build's watchdog ran; `None` otherwise, never `Some(0)`.
    pub largest_dispatch_before_first_frame_us: Option<u64>,
    /// Composition stages this window schedules (`composition/`): W when its
    /// request ran it, then C, R and P (R and P in one dispatch), and B1, B2
    /// and B3 (in one dispatch after the first frame), always (a rail the
    /// restored layout hides gets its content revealed into the hidden rail).
    pub stages_total: u32,
    /// Scheduled composition stages that have not run.
    pub stages_pending: u32,
    /// Composition stages an operation forced ahead of, or without, a schedule.
    pub forced_stages: u32,
    /// From the window request (or the construction) to the first painted
    /// frame, the shell with its final chrome.
    pub shell_first_frame_us: Option<u64>,
    /// From the window request (or the construction) until composition
    /// completed; `None` until it did.
    pub composition_complete_us: Option<u64>,
    /// From the window request (or the construction) to the first frame
    /// painted after the activation's file opens ran; `None` when the request
    /// named no files.
    pub activation_shown_us: Option<u64>,
    /// The largest main-loop dispatch until composition completed, when the
    /// measurement build's watchdog ran; `None` otherwise, never `Some(0)`.
    pub largest_composition_dispatch_us: Option<u64>,
    /// Backend writes the settings writer made before the first frame.
    pub settings_writes_before_first_frame: u64,
    /// Keys whose newest value the writer has not persisted yet.
    pub settings_pending: u64,
    /// Values the writer has flushed.
    pub settings_flushed: u64,
    /// Backend writes (flushes) the writer has made.
    pub settings_flushes: u64,
    /// Writes the writer dropped as equal to the current or persisted value.
    pub settings_dropped_equal: u64,
}

impl LushtextWindow {
    /// Read the window-construction part of the shell-geometry evidence
    /// (`geometry::evidence`), which the automation snapshot also projects.
    pub(crate) fn window_construction_evidence(&self) -> WindowConstructionEvidence {
        let startup = construction_spans::startup_spans();
        let writer = settings_writes::evidence();
        let composition = self.composition_evidence();
        let construction = self.imp().construction.borrow();
        WindowConstructionEvidence {
            startup_toolkit_us: startup.toolkit_us,
            startup_lushtext_us: startup.lushtext_us,
            window_construction_us: construction.window_construction_us(),
            first_frame_us: construction.first_frame_us(),
            largest_dispatch_before_first_frame_us: construction
                .largest_dispatch_before_first_frame_us(),
            stages_total: composition.stages_total,
            stages_pending: composition.stages_pending,
            forced_stages: composition.forced_stages,
            shell_first_frame_us: composition.shell_first_frame_us,
            composition_complete_us: composition.composition_complete_us,
            activation_shown_us: composition.activation_shown_us,
            largest_composition_dispatch_us: composition.largest_composition_dispatch_us,
            settings_writes_before_first_frame: writer.writes_before_first_frame,
            settings_pending: writer.pending,
            settings_flushed: writer.flushed,
            settings_flushes: writer.flushes,
            settings_dropped_equal: writer.dropped_equal,
        }
    }

    /// Assign stable labels to compact shell controls whose visible content is
    /// mostly symbolic. Assistive technology reads these labels through GTK's
    /// accessibility layer, and the smoke lane uses them as durable anchors.
    pub(super) fn apply_accessibility_metadata(&self) {
        let imp = self.imp();
        accessibility::set_label(&*imp.new_tab_button, "New file");
        accessibility::set_key_shortcuts(&*imp.new_tab_button, "<Control>n");
        accessibility::set_labelled_description(
            &*imp.open_menu_button,
            "Open recent documents",
            "Search recent documents or open the file chooser",
        );
        accessibility::set_key_shortcuts(&*imp.open_menu_button, "<Control>k");
        accessibility::set_has_popup(&*imp.open_menu_button, true);
        accessibility::set_controls(
            &*imp.open_menu_button,
            &[imp.open_popover.upcast_ref::<gtk4::Accessible>()],
        );
        accessibility::set_labelled_description(
            &*imp.document_properties_toggle_button,
            "Toggle document properties",
            "Show or hide metadata and formatting controls for the active document",
        );
        accessibility::set_key_shortcuts(&*imp.document_properties_toggle_button, "F9");
        accessibility::set_pressed(&*imp.document_properties_toggle_button, false);
        accessibility::set_controls(
            &*imp.document_properties_toggle_button,
            &[imp.properties_panel.upcast_ref::<gtk4::Accessible>()],
        );
        accessibility::set_label(&*imp.primary_menu_button, "Main menu");
        accessibility::set_has_popup(&*imp.primary_menu_button, true);
        accessibility::set_label(&*imp.notes_menu_button, "Notes menu");
        accessibility::set_has_popup(&*imp.notes_menu_button, true);
        accessibility::set_role(&*imp.tab_bar, gtk4::AccessibleRole::TabList);
        accessibility::set_labelled_description(
            &*imp.tab_bar,
            "Open document tabs",
            "Switch between open documents",
        );
        accessibility::set_labelled_description(
            &*imp.tab_view,
            "Editor tab content",
            "Content for the selected document tab",
        );
        accessibility::set_labelled_description(
            &*imp.focus_mode_affordance,
            "Focus mode controls",
            "Shows that focus mode is active",
        );
        accessibility::set_hidden(&*imp.focus_mode_affordance, true);
        accessibility::set_label(&*imp.leave_focus_mode_button, "Leave focus mode");
        accessibility::set_key_shortcuts(&*imp.leave_focus_mode_button, "Escape");
    }

    /// Connect GSettings notifications, adaptive split listeners, sidebar signals,
    /// command palette handlers, and tab view callbacks during construction.
    pub(super) fn wire_window_signals(&self) {
        let imp = self.imp();
        let settings = &imp.settings;

        // GObject property notifications fire on the main thread here; the
        // `_local` variant lets the closures capture GTK objects that are not
        // `Send`. The values go to the coalescing settings writer, which keeps
        // them in memory and persists the latest once per persistence window,
        // so a resize burst never writes the backend per notification.
        self.connect_notify_local(Some("default-width"), move |window, _| {
            if !window.is_maximized() {
                let (w, _) = window.default_size();
                settings_writes::set_int(keys::WINDOW_WIDTH, w);
            }
        });
        self.connect_notify_local(Some("default-height"), move |window, _| {
            if !window.is_maximized() {
                let (_, h) = window.default_size();
                settings_writes::set_int(keys::WINDOW_HEIGHT, h);
            }
        });
        self.connect_notify_local(Some("maximized"), move |window, _| {
            settings_writes::set_boolean(keys::WINDOW_MAXIMIZED, window.is_maximized());
        });

        {
            // Returning to the window is the attention moment that matters:
            // external changes happen while the application is *not* focused,
            // by construction, so this trigger is correlated with the cause
            // rather than merely with elapsed time. The throttle and the
            // yield-to-user-work guard live in `window/attention_refresh.rs`.
            self.connect_notify_local(Some("is-active"), |window, _| {
                if gtk4::prelude::GtkWindowExt::is_active(window) {
                    let _admission = window.refresh_workspace_surfaces_on_attention(
                        crate::ui::window::AttentionSurfaces::IndexAndTree,
                    );
                }
            });
        }

        {
            // Both visibility keys invalidate the palette file index; the sidebar
            // owns its own subscription for the tree.
            // The window and its settings object share one lifetime, so the
            // handler ids need no bag.
            let window_weak = self.downgrade();
            let _ =
                crate::ui::workspace_visibility::connect_visibility_changed(settings, move || {
                    if let Some(window) = window_weak.upgrade() {
                        window.rebuild_file_index();
                    }
                });
        }

        settings_writes::connect_changed_for(self, keys::USE_EDITORCONFIG, |window, s| {
            window.on_use_editorconfig_changed(s.boolean(keys::USE_EDITORCONFIG));
        });

        settings_writes::connect_changed_for(self, keys::PREVIEW_PANE_POSITION, |window, s| {
            window
                .imp()
                .preferred_preview_width
                .set(lushtext_model::values::Sp::new(
                    s.int(keys::PREVIEW_PANE_POSITION),
                ));
            window.sync_preview_width_constraints(current_window_width(window));
            window.queue_preview_layout_settle();
        });

        settings_writes::connect_changed_for(
            self,
            keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION,
            |window, s| {
                let preset = WorkspaceSidebarWidthPreset::from_fraction(
                    s.double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION),
                );
                set_workspace_sidebar_preset(window, preset);
            },
        );

        {
            let window_weak = self.downgrade();
            imp.workspace_split_view.connect_notify_local(
                Some("sidebar-width-fraction"),
                move |split, _| {
                    let Some(window) = window_weak.upgrade() else {
                        return;
                    };
                    let width = current_window_width(&window);
                    let fixed = effective_workspace_sidebar_fraction(&window, width);
                    if (fixed - split.sidebar_width_fraction()).abs() > f64::EPSILON {
                        split.set_sidebar_width_fraction(fixed);
                        return;
                    }
                    if window
                        .imp()
                        .split_layout
                        .workspace_sidebar_transition_settle
                        .pending()
                    {
                        return;
                    }
                    sync_properties_breakpoint(&window);
                    sync_properties_split_view(&window, width);
                },
            );
        }

        {
            let window_weak = self.downgrade();
            imp.workspace_split_view
                .connect_notify_local(Some("collapsed"), move |_split, _| {
                    let Some(window) = window_weak.upgrade() else {
                        return;
                    };
                    if window
                        .imp()
                        .split_layout
                        .workspace_sidebar_transition_settle
                        .pending()
                    {
                        return;
                    }
                    let width = current_window_width(&window);
                    sync_properties_breakpoint(&window);
                    sync_properties_split_view(&window, width);
                });
        }

        {
            let window_weak = self.downgrade();
            imp.workspace_split_view.connect_notify_local(
                Some("show-sidebar"),
                move |_split, _| {
                    let Some(window) = window_weak.upgrade() else {
                        return;
                    };
                    if window
                        .imp()
                        .split_layout
                        .workspace_sidebar_transition_settle
                        .pending()
                    {
                        return;
                    }
                    let width = current_window_width(&window);
                    sync_properties_breakpoint(&window);
                    sync_properties_split_view(&window, width);
                },
            );
        }

        {
            let window_weak = self.downgrade();
            imp.properties_split_view.connect_notify_local(
                Some("sidebar-width-fraction"),
                move |split, _| {
                    let Some(window) = window_weak.upgrade() else {
                        return;
                    };
                    let width = current_window_width(&window);
                    let fixed = effective_properties_fraction(&window, width);
                    if (fixed - split.sidebar_width_fraction()).abs() > f64::EPSILON {
                        split.set_sidebar_width_fraction(fixed);
                    }
                },
            );
        }

        {
            let window_weak = self.downgrade();
            imp.properties_layout_view.connect_notify_local(
                Some("layout-name"),
                move |_layout_view, _| {
                    let Some(window) = window_weak.upgrade() else {
                        return;
                    };
                    sync_secondary_surfaces(&window);
                    if window.rendered_document_properties_visible() {
                        window.restore_focus_after_breakpoint_collapse();
                    }
                },
            );
        }

        let window_weak = self.downgrade();
        imp.sidebar.connect_file_activated(move |path| {
            if let Some(window) = window_weak.upgrade() {
                window.open_document(path);
            }
        });

        let window_weak = self.downgrade();
        imp.sidebar.connect_local_history_requested(move |path| {
            if let Some(window) = window_weak.upgrade() {
                window.show_local_history_for_path(path);
            }
        });

        let window_weak = self.downgrade();
        imp.sidebar.connect_document_note_requested(move |path| {
            if let Some(window) = window_weak.upgrade() {
                window.open_document_note_for_path(path);
            }
        });

        let window_weak = self.downgrade();
        imp.sidebar.connect_file_renamed(move |old_path, new_path| {
            if let Some(window) = window_weak.upgrade() {
                window.handle_sidebar_file_renamed(old_path, new_path);
            }
        });

        let window_weak = self.downgrade();
        imp.sidebar.connect_file_deleted(move |path| {
            if let Some(window) = window_weak.upgrade() {
                window.handle_sidebar_file_deleted(path);
            }
        });

        let window_weak = self.downgrade();
        imp.sidebar.connect_file_created(move |path| {
            if let Some(window) = window_weak.upgrade() {
                window.handle_sidebar_file_created(path);
            }
        });

        let window_weak = self.downgrade();
        imp.sidebar.connect_message(move |text, severity| {
            if let Some(window) = window_weak.upgrade() {
                window.publish_status_message(text, severity);
                if matches!(severity, MessageKind::Info) {
                    window.announce_workflow_update(
                        accessibility::AnnouncementLane::StatusUpdate,
                        &format!("sidebar:{text}"),
                        text,
                    );
                }
            }
        });

        let window_weak = self.downgrade();
        imp.sidebar
            .connect_folder_note_requested(move |workspace_id| {
                if let Some(window) = window_weak.upgrade() {
                    window.open_folder_note_for_id(&workspace_id);
                }
            });

        let window_weak = self.downgrade();
        imp.sidebar
            .connect_folder_note_for_folder_requested(move |workspace_id, folder| {
                if let Some(window) = window_weak.upgrade() {
                    window.open_folder_note_for_workspace_folder(&workspace_id, &folder);
                }
            });

        let window_weak = self.downgrade();
        imp.sidebar.connect_workspace_structure_changed(move || {
            if let Some(window) = window_weak.upgrade() {
                window.refresh_workspace_scope_consumers();
            }
        });

        let window_weak = self.downgrade();
        imp.sidebar.connect_workspace_scope_changed(move |scope| {
            if let Some(window) = window_weak.upgrade() {
                window.set_workspace_scope(scope);
            }
        });

        let window_weak = self.downgrade();
        imp.command_palette.connect_item_activated(move |item| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            if item.is_file() {
                if let Some(path) = item.file_path() {
                    window.open_document(&path);
                }
            } else if item.is_note() {
                if let Some(target) = item.note_target() {
                    window.activate_palette_note_target(&target);
                }
            } else if item.is_command() {
                let action_id = item.action_id();
                if let Some(stripped) = action_id.strip_prefix("win.") {
                    gtk4::prelude::ActionGroupExt::activate_action(&window, stripped, None);
                } else if let Some(stripped) = action_id.strip_prefix("app.")
                    && let Some(app) = window.application()
                {
                    gtk4::prelude::ActionGroupExt::activate_action(&app, stripped, None);
                }
            }
            window.close_command_palette();
        });

        let window_weak = self.downgrade();
        imp.command_palette.connect_escape_consumed(move |outcome| {
            if let Some(window) = window_weak.upgrade() {
                // Latch every Escape the palette consumed, a category leave as
                // much as a close: the window's Bubble controller sees the same
                // key press after `stop-search`, and would otherwise give up one
                // more level (`drill-down-palette-categories` D5).
                window.mark_child_transient_escape_handled();
                if outcome == crate::ui::command_palette::PaletteEscape::CloseRequested {
                    window.close_command_palette();
                }
            }
        });

        let window_weak = self.downgrade();
        imp.tab_view
            .connect_notify_local(Some("n-pages"), move |_, _| {
                if let Some(window) = window_weak.upgrade() {
                    if window.tab_projection_refresh_deferred() {
                        return;
                    }
                    window.update_content_stack();
                    window.reconcile_open_paths_from_tabs();
                    window.refresh_sidebar_file_row_states();
                    window.refresh_open_popover_rows();
                }
            });

        let window_weak = self.downgrade();
        imp.tab_view
            .connect_notify_local(Some("selected-page"), move |_, _| {
                if let Some(window) = window_weak.upgrade() {
                    // The tab-overflow reveal re-announces an unchanged
                    // selection so the strip scrolls to it: nothing to project.
                    if window.tab_overflow_reveals_selection() {
                        return;
                    }
                    let session = &window.imp().session;
                    if !session.applying_restore_selection.get() {
                        session
                            .selection_generation
                            .set(session.selection_generation.get().next());
                    }
                    window.follow_selection_in_unmapped_window();
                    if window.tab_projection_refresh_deferred() {
                        return;
                    }
                    window.refresh_selected_tab_model_projections();
                }
            });

        let window_weak = self.downgrade();
        imp.tab_view.connect_close_page(move |tab_view, page| {
            let window = window_weak.upgrade();
            LushtextWindow::handle_tab_close_request(window.as_ref(), tab_view, page)
        });

        let window_weak = self.downgrade();
        imp.tab_view.connect_page_attached(move |_, page, _| {
            if let Some(window) = window_weak.upgrade()
                && !window.imp().disposing.get()
            {
                window.handle_tab_attached(page);
            }
        });

        let window_weak = self.downgrade();
        imp.tab_view.connect_page_detached(move |_, page, _| {
            if let Some(window) = window_weak.upgrade()
                && !window.imp().disposing.get()
            {
                window.handle_tab_detached(page);
            }
        });
    }

    /// Tear down non-template workflow timers, async operations, and journal
    /// memberships on window disposal.
    pub(super) fn dispose_window_workflows(&self) {
        let imp = self.imp();
        // A retirement holds its browser state, which holds this window:
        // release it here, outside the borrow (the rest of its rows go now).
        let retirements = imp.notes_row_retirements.take();
        drop(retirements);
        // Normally already done when the window left its application
        // (`window-removed`); idempotent either way.
        self.tear_down_composition();
        imp.session.cancel_startup_read_for_dispose();
        imp.drafts.lazy_restore_capacity_wakeup.cancel();
        self.cancel_session_restore_for_dispose();
        imp.file_index_builds.borrow_mut().invalidate();
        imp.file_index_admission.borrow_mut().take();
        imp.file_index_capacity_wakeup.cancel();
        imp.command_palette_note_refreshes.borrow_mut().invalidate();
        imp.command_palette_note_admission.borrow_mut().take();
        imp.command_palette_note_capacity_wakeup.cancel();
        imp.command_palette_note_capture.stop();
        imp.drafts.dispose_timers();
        // Release the journal lane and this window's draft claims so the
        // application's other windows are not left waiting on a window that
        // no longer exists.
        self.leave_process_draft_journal();
        // Chunked snapshots have later GTK slices queued. Cancel before the
        // window's workflow state is torn down so none can resume after dispose.
        imp.drafts.dispose_snapshots();
        for snapshot in imp.note_save_snapshots.take() {
            snapshot.dispose();
        }
        if let Some(source_id) = imp.notification_sweep_source_id.take() {
            source_id.remove();
        }
        imp.search_progress.dispose();
    }

    /// Run pre-close guards and launch async close safety or confirm prompt.
    pub(super) fn handle_window_close_request(&self) -> glib::Propagation {
        self.clear_close_discard_drafts();
        if self.has_saving_editors() {
            self.publish_save_in_progress_warning();
            return glib::Propagation::Stop;
        }
        if self.imp().search_panel.replace_run_in_flight() {
            self.request_close_during_replace_run();
            return glib::Propagation::Stop;
        }
        // Held typed text is settled into its document (or a new tab) before
        // the unsaved documents are collected; a close is refused while a
        // workflow still owns a buffer that has text held for it.
        if !self.settle_held_text_for_close(&self.all_editors()) {
            return glib::Propagation::Stop;
        }
        let modified = self.modified_editors();

        if modified.is_empty() {
            self.begin_async_close_safety();
            return glib::Propagation::Stop;
        }

        let window_for_close = self.clone();
        self.show_save_changes_dialog(&modified, move |confirmed| {
            if confirmed {
                window_for_close.begin_async_close_safety();
            }
        });
        glib::Propagation::Stop
    }
}
