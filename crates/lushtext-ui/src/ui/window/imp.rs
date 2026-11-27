// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the main application window.
//!
//! This module owns the composite-template struct, `Default`, `ObjectSubclass`,
//! `constructed()` geometry hooks, `dispose()`, `WidgetImpl` (`measure` /
//! `size_allocate`), and `WindowImpl::close_request`.

use super::drafts::DraftState;
use super::editor_memory_eviction::EditorMemoryState;
use super::focus_mode::FocusModeState;
use super::geometry::execution::{
    configure_split_views, install_split_view_breakpoints, plan_split_view_restore,
    restore_properties_split_view, restore_workspace_split_view,
    sync_split_view_widths_for_allocation,
};
use super::geometry::policy::NORMAL_MODE_MIN_HEIGHT_SP;
pub use super::geometry::policy::SecondarySurface;
use super::geometry::{SecondarySurfaceState, SplitLayoutHandles};
use super::notes::ActiveNotesBrowser;
use super::recent_documents_journal::RecentDocumentsState;
use super::search_progress_execution::SearchProgressState;
use super::session_restore::SessionState;
use super::startup_data::StartupDataFlowState;
use super::tab_strip::TabManagementState;
use crate::config::keys;
use crate::ui::buffer_snapshot::BufferSnapshotHandle;
use crate::ui::command_palette::LushtextCommandPalette;
use crate::ui::construction_spans;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::info_bar::LushtextInfoBar;
use crate::ui::markdown_preview::LushtextMarkdownPreview;
use crate::ui::open_popover::LushtextOpenPopover;
use crate::ui::properties_panel::LushtextPropertiesPanel;
use crate::ui::search_panel::LushtextSearchPanel;
use crate::ui::sidebar::LushtextSidebar;
use crate::ui::status_bar::{LushtextStatusBar, MessageKind};
use gtk_lush_settle::{Debounce, SettleBurst};
use gtk_lush_widgets::ClipBin;
use gtk4::prelude::*;
use gtk4::{self, CompositeTemplate, gio, glib};
use libadwaita::subclass::prelude::*;
use lushtext_model::workspace::WorkspaceScope;
use lushtext_services::coordination::notifications::NotificationBus;
use lushtext_services::palette::{
    FileIndexBuildCoordinator, FileIndexBuildStart, NoteSourceRefreshCoordinator,
    NoteSourceRefreshStart,
};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::PathBuf;

/// Private template implementation for the main application window.
///
/// Owns the mounted window surfaces, adaptive split views, tab view, transient
/// controls, and long-lived workflow state that coordinates editor tabs with
/// the sidebar, search, preview, and status bar.
// `CompositeTemplate` loads the compiled window resource; `TemplateChild`
// fields are populated from matching IDs during GObject initialization.
#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/window.ui")]
pub struct LushtextWindow {
    /// Top header bar that hosts primary window controls.
    #[template_child]
    pub header_bar: TemplateChild<libadwaita::HeaderBar>,
    /// Window title widget updated from the active document/workspace state.
    #[template_child]
    pub title_widget: TemplateChild<libadwaita::WindowTitle>,
    /// Header button that opens a new editor tab.
    #[template_child]
    pub new_tab_button: TemplateChild<gtk4::Button>,
    /// Menu button that hosts the recent Open popover.
    #[template_child]
    pub open_menu_button: TemplateChild<gtk4::MenuButton>,
    /// Stack that swaps open-button presentation for adaptive states.
    #[template_child]
    pub open_button_stack: TemplateChild<gtk4::Stack>,
    /// Searchable recent-document popover mounted from the header.
    #[template_child]
    pub open_popover: TemplateChild<LushtextOpenPopover>,
    /// Header toggle controlling document properties visibility.
    #[template_child]
    pub document_properties_toggle_button: TemplateChild<gtk4::ToggleButton>,
    /// Adwaita tab bar bound to the editor TabView.
    #[template_child]
    pub tab_bar: TemplateChild<libadwaita::TabBar>,
    /// Overlay used for transient surfaces above the main shell.
    #[template_child]
    pub window_overlay: TemplateChild<gtk4::Overlay>,
    /// Adaptive left workspace split view.
    #[template_child]
    pub workspace_split_view: TemplateChild<libadwaita::OverlaySplitView>,
    /// Layout view that switches document properties between side and bottom slots.
    #[template_child]
    pub properties_layout_view: TemplateChild<libadwaita::MultiLayoutView>,
    /// Compact document-properties bottom sheet.
    #[template_child]
    pub properties_bottom_sheet: TemplateChild<libadwaita::BottomSheet>,
    /// Wide-layout document-properties split view.
    #[template_child]
    pub properties_split_view: TemplateChild<libadwaita::OverlaySplitView>,
    /// Shared tab model for all editor and preview pages.
    #[template_child]
    pub tab_view: TemplateChild<libadwaita::TabView>,
    /// Stack that swaps editor content with window empty states.
    #[template_child]
    pub content_stack: TemplateChild<gtk4::Stack>,
    /// Workspace sidebar widget mounted in the left split.
    #[template_child]
    pub sidebar: TemplateChild<LushtextSidebar>,
    /// Document metadata and formatting panel.
    #[template_child]
    pub properties_panel: TemplateChild<LushtextPropertiesPanel>,
    /// Bottom status bar with messages and document metadata.
    #[template_child]
    pub status_bar: TemplateChild<LushtextStatusBar>,
    /// Revealer for the command palette overlay.
    #[template_child]
    pub palette_revealer: TemplateChild<gtk4::Revealer>,
    /// Command palette widget for commands, files, and notes.
    #[template_child]
    pub command_palette: TemplateChild<LushtextCommandPalette>,
    /// Revealer that presents Focus Mode chrome.
    #[template_child]
    pub focus_mode_revealer: TemplateChild<gtk4::Revealer>,
    /// Focus Mode affordance container.
    #[template_child]
    pub focus_mode_affordance: TemplateChild<gtk4::Box>,
    /// Button that exits Focus Mode.
    #[template_child]
    pub leave_focus_mode_button: TemplateChild<gtk4::Button>,
    /// Dedicated secondary menu for bookmark and note workflows.
    #[template_child]
    pub notes_menu_button: TemplateChild<gtk4::MenuButton>,
    /// Primary application menu button.
    #[template_child]
    pub primary_menu_button: TemplateChild<gtk4::MenuButton>,
    /// Layout view that switches Markdown preview between side and compact slots.
    #[template_child]
    pub preview_layout_view: TemplateChild<libadwaita::MultiLayoutView>,
    /// Wide-layout Markdown preview split view.
    #[template_child]
    pub preview_split_view: TemplateChild<libadwaita::OverlaySplitView>,
    /// Box containing the active editor stack and inline chrome.
    #[template_child]
    pub editor_box: TemplateChild<gtk4::Box>,
    /// Read-only Markdown preview paired with the active editor.
    #[template_child]
    pub markdown_preview: TemplateChild<LushtextMarkdownPreview>,
    /// The selected tab's document alert, shown above the preview only in
    /// preview-only mode, where the editor's own bar is hidden with the editor.
    /// Projected by the notification workflow; never hosts state of its own.
    #[template_child]
    pub preview_document_alert: TemplateChild<LushtextInfoBar>,
    /// Central shell containing editor, preview, and search panel surfaces.
    #[template_child]
    pub content_box: TemplateChild<gtk4::Box>,
    /// Revealer for the workspace-wide search panel.
    #[template_child]
    pub search_panel_revealer: TemplateChild<gtk4::Revealer>,
    /// Workspace-wide search and replace panel.
    #[template_child]
    pub search_panel: TemplateChild<LushtextSearchPanel>,

    /// Application-wide settings for geometry, sidebar layout, and editor behavior.
    pub settings: gio::Settings,
    /// This window's startup-to-first-frame spans (`ui::construction_spans`).
    pub construction: RefCell<lushtext_model::budgets::construction_spans::WindowConstructionSpans>,
    /// Staged composition: which stages ran and what they hold (`composition/`).
    pub(crate) composition: super::composition::CompositionState,
    /// Measure and allocation spans recorded so far by the measurement build.
    #[cfg(feature = "responsiveness-probes")]
    pub(super) layout_probe_spans: Cell<u32>,
    /// Requested-versus-rendered state for the workspace sidebar and document properties.
    pub secondary_surfaces: SecondarySurfaceState,
    /// Whether the side-by-side preview pane is currently visible.
    pub preview_visible: Cell<bool>,
    /// Whether the preview-only mode (Alt+P) is active (editor hidden, preview full-width).
    pub preview_mode: Cell<bool>,
    /// Legacy preferred side-by-side preview width from `preview-pane-position`.
    pub preferred_preview_width: Cell<lushtext_model::values::Sp>,
    /// Whether showing the side-by-side sidebar waits for its first allocation.
    ///
    /// See `LushtextWindow::show_preview_sidebar_once_allocated`.
    pub preview_sidebar_show_deferred: Cell<bool>,
    /// Settle burst while preview layout switching or embedded widget repair is pending.
    pub preview_transition_settle: SettleBurst,
    /// Debounce for preview renders (300ms).
    pub preview_render_debounce: Debounce,
    /// Debounce for command-palette file index rebuilds (300ms).
    pub index_rebuild_debounce: Debounce,
    /// One-active/one-latest ownership for command-palette index traversal.
    pub file_index_builds: RefCell<FileIndexBuildCoordinator>,
    /// Active build whose compact request is waiting for disposal replacement capacity.
    pub file_index_admission: RefCell<Option<FileIndexBuildStart>>,
    /// One paced capacity wakeup for a deferred file-index traversal.
    pub(super) file_index_capacity_wakeup: crate::ui::plain_disposal::DisposalCapacityWakeup,
    /// Debounce for command-palette note source refreshes after bursty note edits.
    pub command_palette_notes_refresh_debounce: Debounce,
    /// One-active/one-latest ownership for bounded palette note-source loads.
    pub command_palette_note_refreshes: RefCell<NoteSourceRefreshCoordinator>,
    /// Active note refresh waiting to reserve replacement ownership before sidecar I/O.
    pub command_palette_note_admission: RefCell<Option<NoteSourceRefreshStart>>,
    /// One paced capacity wakeup for the command-palette note source.
    pub(super) command_palette_note_capacity_wakeup:
        crate::ui::plain_disposal::DisposalCapacityWakeup,
    /// The palette's sliced open-editor bookmark capture, which precedes each
    /// note-source request (`drill-down-palette-categories` D7).
    pub(crate) command_palette_note_capture: std::rc::Rc<super::notes::OpenEditorCaptureOwner>,
    /// Focus widget saved before the command palette steals focus.
    pub saved_focus: RefCell<Option<glib::WeakRef<gtk4::Widget>>>,
    /// One-tick latch for Escape already handled by a child command-palette entry.
    ///
    /// If GTK lets the same key event continue to the window bubble controller
    /// after `stop-search`, the shell consumes that event without closing the
    /// next surface underneath the palette.
    pub transient_child_escape_handled: Cell<bool>,
    /// Whether the window has seen a key or button press since it was last
    /// mapped: then the opened-document handoff keeps the user's focus
    /// (`editor_focus::setup_opened_document_focus`).
    pub user_input_since_map: Cell<bool>,
    /// An opened-document focus handoff was requested and has not finished
    /// (`editor_focus`): the selected editor's inline alert waits for it.
    pub opened_document_handoff_pending: Cell<bool>,
    /// Set of file paths with open tabs, for O(1) duplicate detection in `open_document`.
    pub open_paths: RefCell<HashSet<PathBuf>>,
    /// Depth counter for tab storms that should rebuild derived projections once.
    pub tab_projection_refresh_defer_depth: Cell<u32>,
    /// Editor-memory accounting used by the eviction helpers.
    pub editor_memory: EditorMemoryState,
    /// Whether GObject disposal has begun and template callbacks must stay inert.
    pub disposing: Cell<bool>,
    /// Session save/restore state.
    pub session: SessionState,
    /// Focus Mode reversible shell state.
    pub focus_mode: FocusModeState,
    /// Draft persistence and autosave state.
    pub drafts: DraftState,
    /// Format preflight state that gates startup metadata consumers.
    pub startup_data_flow: StartupDataFlowState,
    /// App-owned recent documents backing the Open popover.
    pub recent_documents: RecentDocumentsState,
    /// Tab-menu targeting, pinned-page wiring, and bulk-close authorization.
    pub tab_management: TabManagementState,
    /// `WFR-TAB-OVERFLOW-NAVIGATION`'s production state.
    pub(crate) tab_overflow: super::tab_overflow::TabOverflowState,
    /// Weak handle for browser-navigation actions while Browse Notes is visible.
    pub(super) active_notes_browser: RefCell<Option<ActiveNotesBrowser>>,
    /// Closed notes browsers whose realized rows are still being retired in
    /// slices (`lighten-notes-browser-rows` D4); each holds its dialog until
    /// its list is empty. Dropped with the window, which releases the rest.
    pub(super) notes_row_retirements: RefCell<Vec<crate::ui::list_retirement::RowRetirement>>,
    /// Active large note-save snapshots, owned until completion or window disposal.
    pub(super) note_save_snapshots: RefCell<Vec<BufferSnapshotHandle>>,
    /// Focus widget saved before the search panel steals focus.
    pub search_saved_focus: RefCell<Option<glib::WeakRef<gtk4::Widget>>>,
    /// Window-scoped notification bus + store.
    pub notification_bus: NotificationBus,
    /// Periodic sweep for expiring transient and progress notifications.
    pub notification_sweep_source_id: RefCell<Option<glib::SourceId>>,
    /// Announcement ledger and counts of the notification projection pass.
    pub(crate) notification_projection: super::notifications::NotificationProjectionState,
    /// Search-progress lease state used by the status-bar notification flow.
    pub search_progress: SearchProgressState,
    /// Shared app-wide workspace scope mirrored from the sidebar selector.
    pub workspace_scope: RefCell<WorkspaceScope>,
    /// Split-view breakpoint and width-sync coordination handles.
    pub(super) split_layout: SplitLayoutHandles,
    /// Last window width whose split-view constraints were synced from allocation.
    ///
    /// `size_allocate()` runs for animation frames as well as real resizes, so
    /// this keeps the handler to cheap width-change work instead of repeating
    /// GSettings and breakpoint churn while Adwaita is animating panes.
    pub split_width_synced_for_width: Cell<i32>,
}

// `ObjectSubclass` registers this Rust type with GLib's runtime type system so
// GTK can construct it from templates, properties, and signal dispatch.
#[glib::object_subclass]
impl ObjectSubclass for LushtextWindow {
    const NAME: &str = "LushtextWindow";
    type Type = super::LushtextWindow;
    type ParentType = libadwaita::ApplicationWindow;

    fn class_init(klass: &mut Self::Class) {
        // Custom child widgets must be registered before `bind_template()`
        // parses `window.ui`, otherwise template construction cannot resolve
        // their type names.
        LushtextSidebar::ensure_type();
        LushtextEditorPage::ensure_type();
        LushtextStatusBar::ensure_type();
        LushtextCommandPalette::ensure_type();
        LushtextOpenPopover::ensure_type();
        LushtextMarkdownPreview::ensure_type();
        LushtextInfoBar::ensure_type();
        LushtextPropertiesPanel::ensure_type();
        LushtextSearchPanel::ensure_type();
        ClipBin::ensure_type();

        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextWindow {
    fn constructed(&self) {
        self.parent_constructed();

        let obj = self.obj();
        // The deferred language attach waits for input-quiet on any window.
        crate::ui::input_quiet::install(&*obj);
        let settings = &self.settings;
        let stage_start = construction_spans::now_us();

        self.open_button_stack.set_visible_child_name("wide");
        obj.apply_accessibility_metadata();
        obj.setup_open_popover_callbacks();

        let w = settings.int(keys::WINDOW_WIDTH);
        let h = settings.int(keys::WINDOW_HEIGHT);
        obj.set_height_request(NORMAL_MODE_MIN_HEIGHT_SP.as_sp());
        obj.set_default_size(w, h);
        if settings.boolean(keys::WINDOW_MAXIMIZED) {
            obj.maximize();
        }
        let stage_start = obj.record_construction_stage("size_restore", stage_start);

        configure_split_views(
            &self.workspace_split_view,
            &self.properties_layout_view,
            &self.properties_split_view,
            &self.properties_bottom_sheet,
            &self.preview_layout_view,
            &self.preview_split_view,
        );
        let stage_start = obj.record_construction_stage("configure_split_views", stage_start);
        // The restore's writes go to the coalescing settings writer, which
        // keeps them in memory (so the breakpoints below read the planned
        // preset) and persists them after the first presented frame: window
        // construction writes nothing to the GSettings backend.
        let restore_plan = plan_split_view_restore(&obj);
        let stage_start = obj.record_construction_stage("plan_split_view_restore", stage_start);
        install_split_view_breakpoints(&obj);
        let stage_start = obj.record_construction_stage("install_breakpoints", stage_start);
        restore_workspace_split_view(&obj, &restore_plan);
        let stage_start = obj.record_construction_stage("restore_workspace", stage_start);
        restore_properties_split_view(&obj, &restore_plan);
        let stage_start = obj.record_construction_stage("restore_properties", stage_start);

        // The legacy preview-pane-position key now stores a preferred
        // side-by-side preview width. Preview still starts hidden; target-state
        // actions apply this width when the pane is explicitly requested.
        let preferred_preview_width =
            lushtext_model::values::Sp::new(settings.int(keys::PREVIEW_PANE_POSITION));
        self.preferred_preview_width.set(preferred_preview_width);
        obj.sync_preview_width_constraints(w);
        obj.wire_preview_document_alert();
        obj.apply_preview_shell_state();
        let stage_start = obj.record_construction_stage("preview_restore", stage_start);

        obj.wire_window_signals();
        obj.install_tab_overflow();
        obj.update_content_stack();
        obj.record_construction_stage("constructed_wiring", stage_start);
    }

    fn dispose(&self) {
        self.disposing.set(true);
        self.obj().dispose_tab_overflow();
        self.obj().dispose_window_workflows();
    }
}

impl WidgetImpl for LushtextWindow {
    /// Measured only in the `responsiveness-probes` build, which records the
    /// window's first measures as spans (`first-frame-window-shell`, D7); the
    /// shipped window keeps GTK's own `measure`.
    #[cfg(feature = "responsiveness-probes")]
    fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
        let start_us = construction_spans::now_us();
        let measured = self.parent_measure(orientation, for_size);
        let name = if orientation == gtk4::Orientation::Horizontal {
            "window_measure_h"
        } else {
            "window_measure_v"
        };
        self.record_layout_probe_span(name, start_us);
        measured
    }

    fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
        #[cfg(feature = "responsiveness-probes")]
        let start_us = construction_spans::now_us();
        self.allocate_shell(width, height, baseline);
        #[cfg(feature = "responsiveness-probes")]
        self.record_layout_probe_span("window_size_allocate", start_us);
    }
}

impl LushtextWindow {
    /// Record one of the window's first measure or allocation spans in the
    /// measurement build's evidence stream; later ones are not recorded, so a
    /// resize or an animation does not flood the bounded channel.
    #[cfg(feature = "responsiveness-probes")]
    fn record_layout_probe_span(&self, name: &'static str, start_us: i64) {
        /// How many measure and allocation spans one window records.
        const LAYOUT_PROBE_SPANS: u32 = 16;
        let recorded = self.layout_probe_spans.get();
        if recorded < LAYOUT_PROBE_SPANS {
            self.layout_probe_spans.set(recorded + 1);
            construction_spans::emit(name, start_us, construction_spans::now_us());
        }
    }

    /// The window's allocation: the shell's own clamps around GTK's.
    fn allocate_shell(&self, width: i32, height: i32, baseline: i32) {
        if height > 0 {
            self.search_panel.clamp_results_height(height / 3);
        }
        self.parent_size_allocate(width, height, baseline);
        self.obj().sync_preview_width_constraints(width);
        if width > 0 {
            let palette_width = width * 6 / 10;
            if self.command_palette.width_request() != palette_width {
                self.command_palette.set_width_request(palette_width);
            }
            sync_split_view_widths_for_allocation(&self.obj(), width);
        }
    }
}

impl WindowImpl for LushtextWindow {
    fn close_request(&self) -> glib::Propagation {
        let window = self.obj().clone();
        // GTK asks for a synchronous close decision. We stop the first request,
        // finish draft/session persistence asynchronously, then re-enter the
        // normal close path with this bypass flag once it is safe to destroy.
        if self.session.take_close_safety_bypass() {
            return self.parent_close_request();
        }
        if self.session.is_close_safety_inflight() {
            window.publish_status_message("Finishing close safety checks…", MessageKind::Info);
            return glib::Propagation::Stop;
        }
        window.handle_window_close_request()
    }
}

impl ApplicationWindowImpl for LushtextWindow {}
impl AdwApplicationWindowImpl for LushtextWindow {}
