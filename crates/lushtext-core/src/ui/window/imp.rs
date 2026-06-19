// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the main application window.
//!
//! This module owns the composite-template wiring, long-lived window state,
//! split-view persistence, and the callback glue that binds the sidebar,
//! command palette, session restore, and notifications into one shell.

use super::notes::ActiveNotesBrowser;
use crate::config::{self, keys};
use crate::model::draft::{DraftManifest, PreloadedDraftRestore};
use crate::model::recent_document::RecentDocumentEntry;
use crate::model::workspace::WorkspaceScope;
use crate::services::notifications::NotificationBus;
use crate::ui::accessibility;
use crate::ui::command_palette::LushtextCommandPalette;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::markdown_preview::LushtextMarkdownPreview;
use crate::ui::open_popover::LushtextOpenPopover;
use crate::ui::properties_panel::LushtextPropertiesPanel;
use crate::ui::search_panel::LushtextSearchPanel;
use crate::ui::sidebar::{LushtextSidebar, WorkspaceSidebarWidthPreset};
use crate::ui::status_bar::{LushtextStatusBar, MessageKind};
use glib::prelude::*;
use gtk_lush_settle::{Debounce, SettleBurst, SupersedingTimer};
use gtk_lush_widgets::ClipBin;
use gtk4::prelude::*;
use gtk4::{self, CompositeTemplate, gio, glib};
use libadwaita::prelude::AdwApplicationWindowExt;
use libadwaita::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Tiny non-zero floor used only before the first real split-view sync.
const WORKSPACE_SIDEBAR_MIN_WIDTH_SP: f64 = 1.0;
/// Properties sidebar minimum width in scale-independent pixels.
///
/// This matches `resources/ui/properties-panel.ui`; keeping the shell budget in
/// sync with the rendered widget avoids one-pixel GTK allocation warnings near
/// the adaptive breakpoint.
const PROPERTIES_SIDEBAR_MIN_WIDTH_SP: f64 = 280.0;
/// Target total-window width for the visible right properties pane.
const FIXED_PROPERTIES_SIDEBAR_FRACTION: f64 = 0.25;
/// Minimum center-column width that keeps restored-document inline alerts stable
/// once their titles and actions are allowed to wrap on narrow windows.
const MIN_EDITOR_CONTENT_WIDTH_SP: f64 = 620.0;
/// Extra width budget for split separators, padding, and rounding noise that
/// the raw `25% / 50% / 25%` fractions do not capture near the breakpoint.
const DUAL_PANE_LAYOUT_OVERHEAD_SP: f64 = 32.0;
/// Minimum normal-mode height that preserves persistent chrome and an editor.
///
/// The prior layout could be allocated around 200px tall, which clipped the
/// status bar and later let the compact document-properties sheet exceed the
/// available window height. This floor keeps the header, tab strip, status bar,
/// bottom sheet, and a small editor viewport inside GTK's legal allocation budget.
const NORMAL_MODE_MIN_HEIGHT_SP: i32 = 360;
/// Collapse the left workspace pane on narrower windows.
///
/// This numeric value feeds both pure layout math and the Libadwaita breakpoint
/// condition string so the declarative and imperative paths cannot drift.
const WORKSPACE_BREAKPOINT_MAX_WIDTH_SP: i32 = 860;
/// GNOME Text Editor switches the header Open control to an icon at 400sp.
const OPEN_BUTTON_BREAKPOINT_MAX_WIDTH_SP: i32 = 400;
/// Wide document-properties presentation in the multi-layout view.
const PROPERTIES_LAYOUT_PANE: &str = "pane";
/// Compact document-properties presentation in the multi-layout view.
const PROPERTIES_LAYOUT_SHEET: &str = "sheet";
/// Normal preview presentation: editor content with optional end preview pane.
pub(super) const PREVIEW_LAYOUT_EDITOR: &str = "editor";
/// Focused preview presentation: Markdown preview fills the editor content area.
pub(super) const PREVIEW_LAYOUT_PREVIEW: &str = "preview";
/// Tiny non-zero floor used only before the first real preview-width sync.
pub(super) const PREVIEW_MIN_WIDTH_SP: f64 = 1.0;
/// Fallback side-by-side preview width for invalid legacy settings.
pub(super) const PREVIEW_DEFAULT_WIDTH_SP: i32 = 300;
/// Maximum share of the editor content that side-by-side preview may consume.
pub(super) const PREVIEW_MAX_WIDTH_FRACTION: f64 = 1.0 / 3.0;
/// Short delay for Adwaita layout and embedded preview children to settle.
pub(super) const PREVIEW_SETTLE_DELAY_MS: u64 = 16;
/// Delay before final secondary-surface reconciliation after a sidebar toggle.
///
/// Adwaita drives `OverlaySplitView:show-sidebar` with its own animated
/// transition. Holding unrelated breakpoint/presentation changes for about one
/// transition budget prevents those changes from forcing the sidebar to the
/// endpoint in the same visible frame.
const WORKSPACE_SIDEBAR_TRANSITION_SETTLE_DELAY_MS: u64 = 260;

/// Secondary surfaces that can compete for the compact-width slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecondarySurface {
    /// The left workspace sidebar.
    Workspace,
    /// The document-properties surface.
    DocumentProperties,
}

/// Adaptive presentation currently used for document properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertiesPresentation {
    /// Properties render as the right sidebar of the inner split view.
    Pane,
    /// Properties render as the sheet of the compact bottom sheet.
    Sheet,
}

impl PropertiesPresentation {
    fn layout_name(self) -> &'static str {
        match self {
            Self::Pane => PROPERTIES_LAYOUT_PANE,
            Self::Sheet => PROPERTIES_LAYOUT_SHEET,
        }
    }

    fn from_layout_name(name: Option<&str>) -> Self {
        match name {
            Some(PROPERTIES_LAYOUT_SHEET) => Self::Sheet,
            _ => Self::Pane,
        }
    }
}

// GObject subclass methods receive `&self` while GTK owns the instance, so
// window state uses `Cell`/`RefCell` for single-threaded interior mutability.
/// Requested-versus-rendered visibility state for compact secondary-surface arbitration.
#[derive(Default)]
pub struct SecondarySurfaceState {
    /// Whether the user last explicitly left the workspace sidebar open.
    pub workspace_requested_visible: Cell<bool>,
    /// Whether the user last explicitly left document properties open.
    pub properties_requested_visible: Cell<bool>,
    /// Which secondary surface currently owns the compact-width slot, if any.
    pub compact_surface: Cell<Option<SecondarySurface>>,
}

/// Stable inputs for the main shell's adaptive geometry decision.
///
/// Keeping this as a plain Rust value makes the breakpoint behavior unit
/// testable without constructing GTK widgets or depending on transient rendered
/// sidebar visibility.
#[derive(Clone, Copy, Debug)]
struct AdaptiveShellInputs {
    /// Current allocated or restored window width in scale-independent pixels.
    window_width: i32,
    /// Workspace width preset selected by the user.
    workspace_preset: WorkspaceSidebarWidthPreset,
    /// Whether the user last requested the workspace sidebar open.
    workspace_requested_visible: bool,
    /// Whether the user last requested document properties open.
    properties_requested_visible: bool,
    /// Which surface was explicitly chosen for the compact slot, if any.
    compact_surface: Option<SecondarySurface>,
    /// Focus Mode suppresses secondary surfaces while preserving requests.
    focus_mode_active: bool,
}

/// Derived shell geometry for one stable set of inputs.
#[derive(Clone, Copy, Debug, PartialEq)]
struct AdaptiveShellLayout {
    /// Document-properties breakpoint threshold for the current intent.
    properties_breakpoint_max_width: i32,
    /// Whether the workspace would consume side-by-side width in this layout.
    workspace_consumes_width: bool,
    /// Resolved document-properties presentation.
    properties_presentation: PropertiesPresentation,
    /// Compact surface that should render for this pass.
    compact_surface: Option<SecondarySurface>,
    /// Whether the workspace sidebar should be rendered now.
    render_workspace: bool,
    /// Whether document properties should be rendered now.
    render_properties: bool,
}

/// Editor-memory accounting shared by the eviction helpers.
#[derive(Default)]
pub struct EditorMemoryState {
    /// Running total of estimated buffer memory across all open tabs.
    pub total: Cell<u64>,
    /// Per-editor estimates keyed by `editor.as_ptr() as usize`.
    pub by_editor: RefCell<HashMap<usize, u64>>,
}

/// Search-progress lease state used by the status-bar heartbeat flow.
#[derive(Default)]
pub struct SearchProgressState {
    /// Periodic lease renewal for active search progress notifications.
    pub heartbeat_source_id: RefCell<Option<glib::SourceId>>,
    /// Superseding delay before progress is allowed to appear in the status bar.
    pub visibility_timer: SupersedingTimer,
    /// Whether search progress is allowed to render after the initial delay.
    pub visible: Cell<bool>,
}

/// Session-persistence state for the main window shell.
#[derive(Default)]
pub struct SessionState {
    /// Debounce for session saves (500ms) and ordered-save freshness.
    pub save_debounce: Debounce,
    /// Guard flag while restoring session state from disk.
    pub restoring: Cell<bool>,
    /// Whether the newest attempted session save failed and still needs retry.
    pub save_failed: Cell<bool>,
    /// Generation of the newest failed session save.
    pub failed_generation: Cell<u32>,
    /// Last failure detail kept for close-flow warnings and widget tests.
    pub failure_detail: RefCell<Option<String>>,
    /// Whether draft/session close-safety work is already running.
    pub close_safety_inflight: Cell<bool>,
    /// One-shot bypass for the final close after async safety work succeeds.
    pub close_safety_bypass: Cell<bool>,
}

/// Reversible shell state owned by Focus Mode.
#[derive(Default)]
pub struct FocusModeState {
    /// Whether Focus Mode is currently active for this window.
    pub active: Cell<bool>,
    /// Whether the window was already fullscreen when Focus Mode was entered.
    pub was_fullscreen_on_entry: Cell<bool>,
    /// Whether side-by-side Markdown preview should be restored on exit.
    pub restore_side_by_side_preview: Cell<bool>,
    /// Whether the user changed preview state while focused.
    pub preview_changed_while_focused: Cell<bool>,
    /// Superseding timer for delayed affordance hiding.
    pub affordance_timer: SupersedingTimer,
}

/// Draft lifecycle state owned by the main window shell.
#[derive(Default)]
pub struct DraftState {
    /// Source ID for the global autosave timer. Removed on dispose.
    pub autosave_source_id: RefCell<Option<glib::SourceId>>,
    /// Superseding one-shot for the first dirty draft after a clean cycle.
    pub first_dirty_autosave_timer: SupersedingTimer,
    /// In-memory draft manifest kept in sync with disk.
    pub manifest: RefCell<DraftManifest>,
    /// Draft restore outcomes preloaded during session restore and consumed once.
    pub preloaded: RefCell<HashMap<String, PreloadedDraftRestore>>,
    /// Monotonic counter for generating unique IDs for untitled tab drafts.
    pub next_tab_id: Cell<u64>,
    /// Whether a draft autosave batch is currently writing draft files/manifest state.
    pub autosave_inflight: Cell<bool>,
    /// Whether another autosave pass is needed after the in-flight batch finishes.
    pub autosave_pending: Cell<bool>,
    /// Draft IDs explicitly discarded during an in-progress close flow.
    /// These must not be re-written by `flush_dirty_drafts()` right before the
    /// window is destroyed.
    pub close_discard_ids: RefCell<HashSet<String>>,
}

/// Startup data-flow gate state owned by the window shell.
#[derive(Default)]
pub struct StartupDataFlowState {
    /// Whether format preflight and any required user decision have resolved.
    pub completed: Cell<bool>,
    /// Whether a preflight task is already running for this window.
    pub running: Cell<bool>,
    /// External activation paths queued while startup metadata consumers are paused.
    pub pending_activation_paths: RefCell<Vec<PathBuf>>,
}

/// Recent-document state owned by the window and projected into the Open popover.
#[derive(Default)]
pub struct RecentDocumentsState {
    /// Newest-first persisted recent-document entries.
    pub entries: RefCell<Vec<RecentDocumentEntry>>,
    /// Whether startup recent-document loading is still in flight.
    pub loading: Cell<bool>,
    /// Paths removed while the startup load was in flight.
    pub removed_while_loading: RefCell<Vec<PathBuf>>,
    /// Monotonic version advanced by in-memory user mutations.
    pub generation: Cell<u64>,
    /// Whether the popover projection should be rebuilt before the next popup.
    pub rows_dirty: Cell<bool>,
    /// Debounce for coalescing bursts of recent-document persistence writes.
    pub save_debounce: Debounce,
    /// Whether a recent-document save is currently running on a worker.
    pub save_inflight: Cell<bool>,
    /// Whether another save should run after the in-flight save completes.
    pub save_pending: Cell<bool>,
    /// Widget tests may seed recents before the async startup load returns.
    #[cfg(feature = "test-utils")]
    pub test_seeded: Cell<bool>,
}

/// Tab-strip menu and close-authorization state owned by the window shell.
pub struct TabManagementState {
    /// Shared `GMenu` model reused for the Adwaita tab context menu.
    pub context_menu: gio::Menu,
    /// The tab page whose context menu is currently being prepared or shown.
    pub target_page: RefCell<Option<glib::WeakRef<libadwaita::TabPage>>>,
    /// Pages already confirmed through the combined bulk-close dialog.
    ///
    /// The `connect_close_page` signal checks this set so a bulk close can
    /// reuse the existing close machinery without spawning one dialog per tab.
    pub preconfirmed_close_pages: RefCell<HashSet<usize>>,
    /// Tracks which tab pages already have pinned-state signal wiring attached.
    ///
    /// Pages can be created during session restore before the explicit tab
    /// workflow setup runs, so this guard prevents duplicate signal hookups.
    pub configured_pages: RefCell<HashSet<usize>>,
}

impl Default for TabManagementState {
    fn default() -> Self {
        Self {
            context_menu: gio::Menu::new(),
            target_page: RefCell::new(None),
            preconfirmed_close_pages: RefCell::new(HashSet::new()),
            configured_pages: RefCell::new(HashSet::new()),
        }
    }
}

// `CompositeTemplate` loads `window.ui` from the compiled GResource, and each
// `TemplateChild` below is bound by the matching template ID.
#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/window.ui")]
pub struct LushtextWindow {
    #[template_child]
    pub header_bar: TemplateChild<libadwaita::HeaderBar>,
    #[template_child]
    pub title_widget: TemplateChild<libadwaita::WindowTitle>,
    #[template_child]
    pub new_tab_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub open_menu_button: TemplateChild<gtk4::MenuButton>,
    #[template_child]
    pub open_button_stack: TemplateChild<gtk4::Stack>,
    #[template_child]
    pub open_popover: TemplateChild<LushtextOpenPopover>,
    #[template_child]
    pub document_properties_toggle_button: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub tab_bar: TemplateChild<libadwaita::TabBar>,
    #[template_child]
    pub window_overlay: TemplateChild<gtk4::Overlay>,
    #[template_child]
    pub workspace_split_view: TemplateChild<libadwaita::OverlaySplitView>,
    #[template_child]
    pub properties_layout_view: TemplateChild<libadwaita::MultiLayoutView>,
    #[template_child]
    pub properties_bottom_sheet: TemplateChild<libadwaita::BottomSheet>,
    #[template_child]
    pub properties_split_view: TemplateChild<libadwaita::OverlaySplitView>,
    #[template_child]
    pub tab_view: TemplateChild<libadwaita::TabView>,
    #[template_child]
    pub content_stack: TemplateChild<gtk4::Stack>,
    #[template_child]
    pub sidebar: TemplateChild<LushtextSidebar>,
    #[template_child]
    pub properties_panel: TemplateChild<LushtextPropertiesPanel>,
    #[template_child]
    pub status_bar: TemplateChild<LushtextStatusBar>,
    #[template_child]
    pub palette_revealer: TemplateChild<gtk4::Revealer>,
    #[template_child]
    pub command_palette: TemplateChild<LushtextCommandPalette>,
    #[template_child]
    pub focus_mode_revealer: TemplateChild<gtk4::Revealer>,
    #[template_child]
    pub focus_mode_affordance: TemplateChild<gtk4::Box>,
    #[template_child]
    pub leave_focus_mode_button: TemplateChild<gtk4::Button>,
    /// Dedicated secondary menu for bookmark and note workflows.
    #[template_child]
    pub notes_menu_button: TemplateChild<gtk4::MenuButton>,
    #[template_child]
    pub primary_menu_button: TemplateChild<gtk4::MenuButton>,
    #[template_child]
    pub preview_layout_view: TemplateChild<libadwaita::MultiLayoutView>,
    #[template_child]
    pub preview_split_view: TemplateChild<libadwaita::OverlaySplitView>,
    #[template_child]
    pub editor_box: TemplateChild<gtk4::Box>,
    #[template_child]
    pub markdown_preview: TemplateChild<LushtextMarkdownPreview>,
    #[template_child]
    pub content_box: TemplateChild<gtk4::Box>,
    #[template_child]
    pub search_panel_revealer: TemplateChild<gtk4::Revealer>,
    #[template_child]
    pub search_panel: TemplateChild<LushtextSearchPanel>,

    /// Application-wide settings for geometry, sidebar layout, and editor behavior.
    pub settings: gio::Settings,
    /// Requested-versus-rendered state for the workspace sidebar and document properties.
    pub secondary_surfaces: SecondarySurfaceState,
    /// Whether the side-by-side preview pane is currently visible.
    pub preview_visible: Cell<bool>,
    /// Whether the preview-only mode (Alt+P) is active (editor hidden, preview full-width).
    pub preview_mode: Cell<bool>,
    /// Legacy preferred side-by-side preview width from `preview-pane-position`.
    pub preferred_preview_width: Cell<i32>,
    /// Settle burst while preview layout switching or embedded widget repair is pending.
    pub preview_transition_settle: SettleBurst,
    /// Settle burst while Adwaita's workspace sidebar transition is in flight.
    pub workspace_sidebar_transition_settle: SettleBurst,
    /// Debounce for preview renders (300ms).
    pub preview_render_debounce: Debounce,
    /// Debounce for command-palette file index rebuilds (300ms).
    pub index_rebuild_debounce: Debounce,
    /// Debounce for command-palette note source refreshes after bursty note edits.
    pub command_palette_notes_refresh_debounce: Debounce,
    /// Generation for command-palette note source loads.
    ///
    /// Sidecar scans run off the main thread; this token prevents an older
    /// load from replacing note rows after scope or note state has changed.
    pub command_palette_notes_generation: Cell<u32>,
    /// Focus widget saved before the command palette steals focus.
    pub saved_focus: RefCell<Option<glib::WeakRef<gtk4::Widget>>>,
    /// One-tick latch for Escape already handled by a child command-palette entry.
    ///
    /// If GTK lets the same key event continue to the window bubble controller
    /// after `stop-search`, the shell consumes that event without closing the
    /// next surface underneath the palette.
    pub transient_child_escape_handled: Cell<bool>,
    /// Set of file paths with open tabs, for O(1) duplicate detection in `open_document`.
    pub open_paths: RefCell<HashSet<PathBuf>>,
    /// Depth counter for tab storms that should rebuild derived projections once.
    pub tab_projection_refresh_defer_depth: Cell<u32>,
    /// Editor-memory accounting used by the eviction helpers.
    pub editor_memory: EditorMemoryState,
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
    /// Weak handle for browser-navigation actions while Browse Notes is visible.
    pub(super) active_notes_browser: RefCell<Option<ActiveNotesBrowser>>,
    /// Focus widget saved before the search panel steals focus.
    pub search_saved_focus: RefCell<Option<glib::WeakRef<gtk4::Widget>>>,
    /// Window-scoped notification bus + store.
    pub notification_bus: NotificationBus,
    /// Periodic sweep for expiring transient and progress notifications.
    pub notification_sweep_source_id: RefCell<Option<glib::SourceId>>,
    /// Search-progress lease state used by the status-bar notification flow.
    pub search_progress: SearchProgressState,
    /// Shared app-wide workspace scope mirrored from the sidebar selector.
    pub workspace_scope: RefCell<WorkspaceScope>,
    /// Stored so the properties breakpoint condition can track the selected
    /// workspace preset and whether the left pane currently consumes width.
    pub properties_breakpoint: RefCell<Option<libadwaita::Breakpoint>>,
    /// Guards split-width synchronization against reentrant allocations caused
    /// by programmatic `OverlaySplitView` fraction updates.
    pub split_width_syncing: Cell<bool>,
    /// Last window width whose split-view constraints were synced from allocation.
    ///
    /// `size_allocate()` runs for animation frames as well as real resizes, so
    /// this keeps the handler to cheap width-change work instead of repeating
    /// GSettings and breakpoint churn while Adwaita is animating panes.
    pub split_width_synced_for_width: Cell<i32>,
    /// Last parsed properties breakpoint width installed on the Adwaita breakpoint.
    pub properties_breakpoint_max_width: Cell<i32>,
}

impl Default for LushtextWindow {
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
            content_box: TemplateChild::default(),
            search_panel_revealer: TemplateChild::default(),
            search_panel: TemplateChild::default(),
            settings: gio::Settings::new(config::APP_ID),
            secondary_surfaces: SecondarySurfaceState::default(),
            preview_visible: Cell::new(false),
            preview_mode: Cell::new(false),
            preferred_preview_width: Cell::new(PREVIEW_DEFAULT_WIDTH_SP),
            preview_transition_settle: SettleBurst::default(),
            workspace_sidebar_transition_settle: SettleBurst::default(),
            preview_render_debounce: Debounce::default(),
            index_rebuild_debounce: Debounce::default(),
            command_palette_notes_refresh_debounce: Debounce::default(),
            command_palette_notes_generation: Cell::new(0),
            saved_focus: RefCell::new(None),
            transient_child_escape_handled: Cell::new(false),
            open_paths: RefCell::new(HashSet::new()),
            tab_projection_refresh_defer_depth: Cell::new(0),
            editor_memory: EditorMemoryState::default(),
            session: SessionState::default(),
            focus_mode: FocusModeState::default(),
            drafts: DraftState::default(),
            startup_data_flow: StartupDataFlowState::default(),
            recent_documents: RecentDocumentsState::default(),
            tab_management: TabManagementState::default(),
            active_notes_browser: RefCell::new(None),
            search_saved_focus: RefCell::new(None),
            notification_bus: NotificationBus::default(),
            notification_sweep_source_id: RefCell::new(None),
            search_progress: SearchProgressState::default(),
            workspace_scope: RefCell::new(WorkspaceScope::All),
            properties_breakpoint: RefCell::new(None),
            split_width_syncing: Cell::new(false),
            split_width_synced_for_width: Cell::new(0),
            properties_breakpoint_max_width: Cell::new(0),
        }
    }
}

// `ObjectSubclass` registers this Rust type with GLib's runtime type system so
// GTK can construct it from templates, properties, and signal dispatch.
#[glib::object_subclass]
impl ObjectSubclass for LushtextWindow {
    const NAME: &'static str = "LushtextWindow";
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
        let settings = &self.settings;

        self.open_button_stack.set_visible_child_name("wide");
        self.apply_accessibility_metadata();
        obj.setup_open_popover_callbacks();

        let w = settings.int(keys::WINDOW_WIDTH);
        let h = settings.int(keys::WINDOW_HEIGHT);
        obj.set_height_request(NORMAL_MODE_MIN_HEIGHT_SP);
        obj.set_default_size(w, h);
        if settings.boolean(keys::WINDOW_MAXIMIZED) {
            obj.maximize();
        }

        configure_split_views(
            &self.workspace_split_view,
            &self.properties_layout_view,
            &self.properties_split_view,
            &self.properties_bottom_sheet,
            &self.preview_layout_view,
            &self.preview_split_view,
        );
        migrate_split_view_settings(settings, w);
        install_split_view_breakpoints(&obj);
        restore_workspace_split_view(&obj);
        restore_properties_split_view(&obj);

        // The legacy preview-pane-position key now stores a preferred
        // side-by-side preview width. Preview still starts hidden; target-state
        // actions apply this width when the pane is explicitly requested.
        let preferred_preview_width = settings.int(keys::PREVIEW_PANE_POSITION);
        self.preferred_preview_width.set(preferred_preview_width);
        obj.sync_preview_width_constraints(w);
        obj.apply_preview_shell_state();

        {
            let settings = settings.clone();
            // GObject property notifications fire on the main thread here; the
            // `_local` variant lets the closure capture GTK objects that are not
            // `Send`.
            obj.connect_notify_local(Some("default-width"), move |window, _| {
                if !window.is_maximized() {
                    let (w, _) = window.default_size();
                    let _ = settings.set_int(keys::WINDOW_WIDTH, w);
                }
            });
        }
        {
            let settings = settings.clone();
            obj.connect_notify_local(Some("default-height"), move |window, _| {
                if !window.is_maximized() {
                    let (_, h) = window.default_size();
                    let _ = settings.set_int(keys::WINDOW_HEIGHT, h);
                }
            });
        }
        {
            let settings = settings.clone();
            obj.connect_notify_local(Some("maximized"), move |window, _| {
                let _ = settings.set_boolean(keys::WINDOW_MAXIMIZED, window.is_maximized());
            });
        }

        {
            let window_weak = obj.downgrade();
            settings.connect_changed(Some(keys::USE_EDITORCONFIG), move |s, _| {
                if let Some(window) = window_weak.upgrade() {
                    window.on_use_editorconfig_changed(s.boolean(keys::USE_EDITORCONFIG));
                }
            });
        }

        {
            let window_weak = obj.downgrade();
            settings.connect_changed(Some(keys::PREVIEW_PANE_POSITION), move |s, _| {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                window
                    .imp()
                    .preferred_preview_width
                    .set(s.int(keys::PREVIEW_PANE_POSITION));
                window.sync_preview_width_constraints(current_window_width(&window));
                window.queue_preview_layout_settle();
            });
        }

        {
            let window_weak = obj.downgrade();
            settings.connect_changed(Some(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION), move |s, _| {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                let preset = WorkspaceSidebarWidthPreset::from_fraction(
                    s.double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION),
                );
                set_workspace_sidebar_preset(&window, preset);
            });
        }

        {
            let window_weak = obj.downgrade();
            self.workspace_split_view.connect_notify_local(
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
                    if window.imp().workspace_sidebar_transition_settle.pending() {
                        return;
                    }
                    sync_properties_breakpoint(&window);
                    sync_properties_split_view(&window, width);
                },
            );
        }

        {
            let window_weak = obj.downgrade();
            self.workspace_split_view
                .connect_notify_local(Some("collapsed"), move |_split, _| {
                    let Some(window) = window_weak.upgrade() else {
                        return;
                    };
                    if window.imp().workspace_sidebar_transition_settle.pending() {
                        return;
                    }
                    let width = current_window_width(&window);
                    sync_properties_breakpoint(&window);
                    sync_properties_split_view(&window, width);
                });
        }

        {
            let window_weak = obj.downgrade();
            self.workspace_split_view.connect_notify_local(
                Some("show-sidebar"),
                move |_split, _| {
                    let Some(window) = window_weak.upgrade() else {
                        return;
                    };
                    if window.imp().workspace_sidebar_transition_settle.pending() {
                        return;
                    }
                    let width = current_window_width(&window);
                    sync_properties_breakpoint(&window);
                    sync_properties_split_view(&window, width);
                },
            );
        }

        {
            let window_weak = obj.downgrade();
            self.properties_split_view.connect_notify_local(
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
            let window_weak = obj.downgrade();
            self.properties_layout_view.connect_notify_local(
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

        let window_weak = obj.downgrade();
        self.sidebar.connect_file_activated(move |path| {
            if let Some(window) = window_weak.upgrade() {
                window.open_document(path);
            }
        });

        let window_weak = obj.downgrade();
        self.sidebar.connect_local_history_requested(move |path| {
            if let Some(window) = window_weak.upgrade() {
                window.show_local_history_for_path(path);
            }
        });

        let window_weak = obj.downgrade();
        self.sidebar.connect_document_note_requested(move |path| {
            if let Some(window) = window_weak.upgrade() {
                window.open_document_note_for_path(path);
            }
        });

        let window_weak = obj.downgrade();
        self.sidebar
            .connect_file_renamed(move |old_path, new_path| {
                if let Some(window) = window_weak.upgrade() {
                    window.update_tab_path(old_path, new_path);
                    window.migrate_note_sidecars_after_rename(old_path, new_path);
                    window.migrate_local_history_after_rename(old_path, new_path);
                    window
                        .imp()
                        .command_palette
                        .update_index_file_renamed(old_path, new_path);
                    let name = new_path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    window.publish_status_message(&format!("Renamed to {name}"), MessageKind::Info);
                }
            });

        let window_weak = obj.downgrade();
        self.sidebar.connect_file_deleted(move |path| {
            if let Some(window) = window_weak.upgrade() {
                window.close_tab_for_path(path);
                window.imp().command_palette.update_index_file_deleted(path);
                window.publish_status_message("Deleted", MessageKind::Info);
            }
        });

        let window_weak = obj.downgrade();
        self.sidebar.connect_file_created(move |path| {
            if let Some(window) = window_weak.upgrade() {
                window.open_document(path);
                window.imp().command_palette.update_index_file_created(path);
            }
        });

        let window_weak = obj.downgrade();
        self.sidebar.connect_message(move |text, severity| {
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

        let window_weak = obj.downgrade();
        self.sidebar
            .connect_folder_note_requested(move |workspace_id| {
                if let Some(window) = window_weak.upgrade() {
                    window.open_folder_note_for_id(&workspace_id);
                }
            });

        let window_weak = obj.downgrade();
        self.sidebar
            .connect_folder_note_for_folder_requested(move |workspace_id, folder| {
                if let Some(window) = window_weak.upgrade() {
                    window.open_folder_note_for_workspace_folder(&workspace_id, &folder);
                }
            });

        let window_weak = obj.downgrade();
        self.sidebar.connect_workspace_structure_changed(move || {
            if let Some(window) = window_weak.upgrade() {
                window.refresh_workspace_scope_consumers();
            }
        });

        let window_weak = obj.downgrade();
        self.sidebar.connect_workspace_scope_changed(move |scope| {
            if let Some(window) = window_weak.upgrade() {
                window.set_workspace_scope(scope);
            }
        });

        let window_weak = obj.downgrade();
        self.command_palette.connect_item_activated(move |item| {
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

        let window_weak = obj.downgrade();
        self.command_palette.connect_close_requested(move || {
            if let Some(window) = window_weak.upgrade() {
                window.mark_child_transient_escape_handled();
                window.close_command_palette();
            }
        });

        let window_weak = obj.downgrade();
        self.tab_view
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

        let window_weak = obj.downgrade();
        self.tab_view
            .connect_notify_local(Some("selected-page"), move |_, _| {
                if let Some(window) = window_weak.upgrade() {
                    window.refresh_status_bar();
                    window.refresh_sidebar_file_row_states();
                    window.refresh_open_popover_rows();
                    window.reload_if_evicted();
                    window.maybe_evict_background_tabs();
                    window.save_session_debounced();
                    window.refresh_preview();
                    window.apply_focus_mode_to_editors();
                    if let Some(editor) = window.active_editor() {
                        editor.refresh_minimap();
                    }
                }
            });

        let window_weak = obj.downgrade();
        self.tab_view.connect_close_page(move |tab_view, page| {
            if let Some(window) = window_weak.upgrade()
                && window.consume_preconfirmed_tab_close(page)
            {
                tab_view.close_page_finish(page, true);
                return glib::Propagation::Stop;
            }

            let child = page.child();
            let Some(editor) = child.downcast_ref::<LushtextEditorPage>() else {
                tab_view.close_page_finish(page, true);
                return glib::Propagation::Stop;
            };
            if !editor.is_modified() {
                tab_view.close_page_finish(page, true);
                return glib::Propagation::Stop;
            }
            let Some(window) = window_weak.upgrade() else {
                tab_view.close_page_finish(page, false);
                return glib::Propagation::Stop;
            };
            let tab_view = tab_view.clone();
            let page = page.clone();
            let page_for_finish = page.clone();
            window.confirm_close_tab(&page, editor, move |confirmed| {
                tab_view.close_page_finish(&page_for_finish, confirmed);
            });
            glib::Propagation::Stop
        });

        let window_weak = obj.downgrade();
        self.tab_view.connect_page_detached(move |_, page, _| {
            if let Some(window) = window_weak.upgrade() {
                window.forget_tab_page(page);
                if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                    if let Some(ref path) = editor.file_path() {
                        let mut paths = window.imp().open_paths.borrow_mut();
                        paths.remove(path.as_path());
                        paths.remove(&super::documents::open_path_key(path));
                        if let Some(canonical_path) = editor.canonical_file_path() {
                            paths.remove(&canonical_path);
                        }
                    }
                    window.dismiss_editor_notifications(editor);
                    window.untrack_editor_memory(editor);
                    editor.cancel_load();
                    editor.stop_file_monitor();
                }
                if !window.tab_projection_refresh_deferred() {
                    window.refresh_tab_model_projections();
                }
                window.save_session_debounced();
            }
        });

        obj.update_content_stack();
    }

    fn dispose(&self) {
        if let Some(source_id) = self.drafts.autosave_source_id.take() {
            source_id.remove();
        }
        if let Some(source_id) = self.notification_sweep_source_id.take() {
            source_id.remove();
        }
        if let Some(source_id) = self.search_progress.heartbeat_source_id.take() {
            source_id.remove();
        }
    }
}

impl LushtextWindow {
    /// Assign stable labels to compact shell controls whose visible content is
    /// mostly symbolic. Assistive technology reads these labels through GTK's
    /// accessibility layer, and the smoke lane uses them as durable anchors.
    fn apply_accessibility_metadata(&self) {
        accessibility::set_label(&*self.new_tab_button, "New file");
        accessibility::set_key_shortcuts(&*self.new_tab_button, "<Control>n");
        accessibility::set_labelled_description(
            &*self.open_menu_button,
            "Open recent documents",
            "Search recent documents or open the file chooser",
        );
        accessibility::set_key_shortcuts(&*self.open_menu_button, "<Control>k");
        accessibility::set_has_popup(&*self.open_menu_button, true);
        accessibility::set_controls(
            &*self.open_menu_button,
            &[self.open_popover.upcast_ref::<gtk4::Accessible>()],
        );
        accessibility::set_labelled_description(
            &*self.document_properties_toggle_button,
            "Toggle document properties",
            "Show or hide metadata and formatting controls for the active document",
        );
        accessibility::set_key_shortcuts(&*self.document_properties_toggle_button, "F9");
        accessibility::set_pressed(&*self.document_properties_toggle_button, false);
        accessibility::set_controls(
            &*self.document_properties_toggle_button,
            &[self.properties_panel.upcast_ref::<gtk4::Accessible>()],
        );
        accessibility::set_label(&*self.primary_menu_button, "Main menu");
        accessibility::set_has_popup(&*self.primary_menu_button, true);
        accessibility::set_label(&*self.notes_menu_button, "Notes menu");
        accessibility::set_has_popup(&*self.notes_menu_button, true);
        accessibility::set_role(&*self.tab_bar, gtk4::AccessibleRole::TabList);
        accessibility::set_labelled_description(
            &*self.tab_bar,
            "Open document tabs",
            "Switch between open documents",
        );
        accessibility::set_labelled_description(
            &*self.tab_view,
            "Editor tab content",
            "Content for the selected document tab",
        );
        accessibility::set_labelled_description(
            &*self.focus_mode_affordance,
            "Focus mode controls",
            "Shows that focus mode is active",
        );
        accessibility::set_hidden(&*self.focus_mode_affordance, true);
        accessibility::set_label(&*self.leave_focus_mode_button, "Leave focus mode");
        accessibility::set_key_shortcuts(&*self.leave_focus_mode_button, "Escape");
    }
}

impl WidgetImpl for LushtextWindow {
    fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
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
        if self.session.close_safety_bypass.replace(false) {
            return self.parent_close_request();
        }
        if self.session.close_safety_inflight.get() {
            window.publish_status_message("Finishing close safety checks…", MessageKind::Info);
            return glib::Propagation::Stop;
        }
        window.clear_close_discard_drafts();
        if window.has_saving_editors() {
            window.publish_save_in_progress_warning();
            return glib::Propagation::Stop;
        }
        let modified = window.modified_editors();

        if modified.is_empty() {
            begin_async_close_safety(&window);
            return glib::Propagation::Stop;
        }

        let window_for_close = window.clone();
        window.show_save_changes_dialog(&modified, move |confirmed| {
            if confirmed {
                begin_async_close_safety(&window_for_close);
            }
        });
        glib::Propagation::Stop
    }
}

impl ApplicationWindowImpl for LushtextWindow {}
impl AdwApplicationWindowImpl for LushtextWindow {}

fn begin_async_close_safety(window: &super::LushtextWindow) {
    // Keep the close transaction single-flight: duplicate close requests report
    // progress while the background draft flush and ordered session save finish.
    if window.imp().session.close_safety_inflight.get() {
        window.publish_status_message("Finishing close safety checks…", MessageKind::Info);
        return;
    }
    window.imp().session.close_safety_inflight.set(true);
    window.imp().search_panel.close();
    let window_for_draft = window.clone();
    window.flush_dirty_drafts_async(move |draft_result| match draft_result {
        Ok(()) => {
            let window_for_session = window_for_draft.clone();
            let window_for_destroy = window_for_draft;
            window_for_session.save_session_for_close_async(move || {
                window_for_destroy
                    .imp()
                    .session
                    .close_safety_inflight
                    .set(false);
                window_for_destroy
                    .imp()
                    .session
                    .close_safety_bypass
                    .set(true);
                window_for_destroy.destroy();
            });
        }
        Err(error) => {
            window_for_draft
                .imp()
                .session
                .close_safety_inflight
                .set(false);
            window_for_draft
                .publish_status_message(&format!("Draft save failed: {error}"), MessageKind::Error);
        }
    });
}

fn configure_split_views(
    workspace_split_view: &libadwaita::OverlaySplitView,
    properties_layout_view: &libadwaita::MultiLayoutView,
    properties_split_view: &libadwaita::OverlaySplitView,
    properties_bottom_sheet: &libadwaita::BottomSheet,
    preview_layout_view: &libadwaita::MultiLayoutView,
    preview_split_view: &libadwaita::OverlaySplitView,
) {
    workspace_split_view.set_sidebar_position(gtk4::PackType::Start);
    workspace_split_view.set_sidebar_width_unit(libadwaita::LengthUnit::Sp);
    workspace_split_view.set_min_sidebar_width(WORKSPACE_SIDEBAR_MIN_WIDTH_SP);
    workspace_split_view.set_max_sidebar_width(WORKSPACE_SIDEBAR_MIN_WIDTH_SP);
    workspace_split_view.set_pin_sidebar(true);
    workspace_split_view.set_enable_show_gesture(false);
    workspace_split_view.set_enable_hide_gesture(false);

    properties_layout_view.set_layout_name(PropertiesPresentation::Pane.layout_name());

    properties_split_view.set_sidebar_position(gtk4::PackType::End);
    properties_split_view.set_sidebar_width_unit(libadwaita::LengthUnit::Sp);
    properties_split_view.set_min_sidebar_width(PROPERTIES_SIDEBAR_MIN_WIDTH_SP);
    properties_split_view.set_pin_sidebar(true);
    properties_split_view.set_enable_show_gesture(false);
    properties_split_view.set_enable_hide_gesture(false);

    preview_layout_view.set_layout_name(PREVIEW_LAYOUT_EDITOR);
    preview_split_view.set_sidebar_position(gtk4::PackType::End);
    preview_split_view.set_sidebar_width_unit(libadwaita::LengthUnit::Sp);
    preview_split_view.set_min_sidebar_width(PREVIEW_MIN_WIDTH_SP);
    preview_split_view.set_max_sidebar_width(PREVIEW_MIN_WIDTH_SP);
    preview_split_view.set_pin_sidebar(true);
    preview_split_view.set_enable_show_gesture(false);
    preview_split_view.set_enable_hide_gesture(false);
    preview_split_view.set_show_sidebar(false);

    // The compact presentation is driven only by the same window action that
    // owns the wide pane. Disabling swipe open/close keeps that requested
    // visibility state deterministic.
    properties_bottom_sheet.set_can_open(false);
    properties_bottom_sheet.set_can_close(false);
    properties_bottom_sheet.set_full_width(true);
    properties_bottom_sheet.set_modal(false);
}

fn migrate_split_view_settings(settings: &gio::Settings, restored_width: i32) {
    if settings.boolean(keys::SPLIT_VIEW_LAYOUT_MIGRATED) {
        return;
    }

    let width = restored_width.max(1);
    let legacy_visible = if settings.user_value(keys::SIDEBAR_VISIBLE).is_some() {
        settings.boolean(keys::SIDEBAR_VISIBLE)
    } else {
        true
    };
    let workspace_fraction = WorkspaceSidebarWidthPreset::DEFAULT.fraction();
    let properties_fraction = desired_properties_fraction(width);

    let _ = settings.set_boolean(keys::WORKSPACE_SIDEBAR_VISIBLE, legacy_visible);
    let _ = settings.set_double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION, workspace_fraction);
    let _ = settings.set_boolean(keys::PROPERTIES_SIDEBAR_VISIBLE, false);
    let _ = settings.set_double(keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION, properties_fraction);
    let _ = settings.set_boolean(keys::SPLIT_VIEW_LAYOUT_MIGRATED, true);
}

fn restore_workspace_split_view(window: &super::LushtextWindow) {
    let width = current_window_width(window);
    let visible = window
        .imp()
        .settings
        .boolean(keys::WORKSPACE_SIDEBAR_VISIBLE);
    let preset = workspace_sidebar_preset(window);
    let fraction = preset.effective_fraction(width);
    sync_workspace_sidebar_width_constraints(window, width);
    window.imp().split_width_synced_for_width.set(width);
    window
        .imp()
        .workspace_split_view
        .set_sidebar_width_fraction(fraction);
    let _ = window
        .imp()
        .settings
        .set_double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION, preset.fraction());
    window
        .imp()
        .secondary_surfaces
        .workspace_requested_visible
        .set(visible);
}

fn restore_properties_split_view(window: &super::LushtextWindow) {
    let width = current_window_width(window);
    let visible = window
        .imp()
        .settings
        .boolean(keys::PROPERTIES_SIDEBAR_VISIBLE);
    let fraction = effective_properties_fraction(window, width);
    window
        .imp()
        .properties_split_view
        .set_sidebar_width_fraction(fraction);
    let _ = window.imp().settings.set_double(
        keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION,
        desired_properties_fraction(width),
    );
    window
        .imp()
        .secondary_surfaces
        .properties_requested_visible
        .set(visible);
    sync_properties_breakpoint(window);
    sync_secondary_surfaces(window);
}

fn install_split_view_breakpoints(window: &super::LushtextWindow) {
    let properties_max_width = properties_breakpoint_max_width_for_window(window);
    window
        .imp()
        .properties_breakpoint_max_width
        .set(properties_max_width);
    let properties_bp = libadwaita::Breakpoint::new(
        libadwaita::BreakpointCondition::parse(&properties_breakpoint_condition(
            properties_max_width,
        ))
        .expect("valid properties breakpoint condition"),
    );
    properties_bp.add_setter(
        window
            .imp()
            .properties_layout_view
            .upcast_ref::<glib::Object>(),
        "layout-name",
        Some(&PropertiesPresentation::Sheet.layout_name().to_value()),
    );
    window
        .imp()
        .properties_breakpoint
        .replace(Some(properties_bp.clone()));
    window.add_breakpoint(properties_bp);

    let workspace_bp = libadwaita::Breakpoint::new(
        libadwaita::BreakpointCondition::parse(&workspace_breakpoint_condition())
            .expect("valid workspace breakpoint condition"),
    );
    workspace_bp.add_setter(
        window
            .imp()
            .workspace_split_view
            .upcast_ref::<glib::Object>(),
        "collapsed",
        Some(&true.to_value()),
    );
    window.add_breakpoint(workspace_bp);

    let open_button_bp = libadwaita::Breakpoint::new(
        libadwaita::BreakpointCondition::parse(&properties_breakpoint_condition(
            OPEN_BUTTON_BREAKPOINT_MAX_WIDTH_SP,
        ))
        .expect("valid Open button breakpoint condition"),
    );
    open_button_bp.add_setter(
        window.imp().open_button_stack.upcast_ref::<glib::Object>(),
        "visible-child-name",
        Some(&"narrow".to_value()),
    );
    window.add_breakpoint(open_button_bp);
}

fn properties_breakpoint_condition(max_width_sp: i32) -> String {
    format!("max-width: {max_width_sp}sp")
}

fn workspace_breakpoint_condition() -> String {
    properties_breakpoint_condition(WORKSPACE_BREAKPOINT_MAX_WIDTH_SP)
}

/// Build the properties-pane breakpoint from the minimum center width instead
/// of a magic number so the shell explains *why* it collapses earlier.
fn properties_breakpoint_max_width_for_window(window: &super::LushtextWindow) -> i32 {
    derive_adaptive_shell_layout(adaptive_shell_inputs(window)).properties_breakpoint_max_width
}

/// Compute the total window width below which the properties pane should
/// overlay instead of consuming layout width in the quarter-width shell.
#[expect(
    clippy::cast_possible_truncation,
    reason = "Stored window geometry is clamped to GTK window dimensions before converting to i32"
)]
fn properties_breakpoint_max_width_sp(workspace_width_sp: f64) -> i32 {
    let center_target = MIN_EDITOR_CONTENT_WIDTH_SP + DUAL_PANE_LAYOUT_OVERHEAD_SP;
    let fraction_guard = dual_sidebar_window_width_for_center(center_target, workspace_width_sp);
    let min_width_guard = center_target + workspace_width_sp + PROPERTIES_SIDEBAR_MIN_WIDTH_SP;
    fraction_guard.max(min_width_guard).ceil() as i32
}

fn current_window_width(window: &super::LushtextWindow) -> i32 {
    if window.width() > 0 {
        window.width()
    } else {
        let (w, _) = window.default_size();
        w.max(1)
    }
}

fn workspace_sidebar_preset(window: &super::LushtextWindow) -> WorkspaceSidebarWidthPreset {
    WorkspaceSidebarWidthPreset::from_fraction(
        window
            .imp()
            .settings
            .double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION),
    )
}

/// Convert a desired center width plus a fixed workspace pane width into the
/// total window width needed to keep the right pane at its quarter-width target.
fn dual_sidebar_window_width_for_center(center_width_sp: f64, workspace_width_sp: f64) -> f64 {
    (center_width_sp + workspace_width_sp)
        / (1.0 - FIXED_PROPERTIES_SIDEBAR_FRACTION).max(f64::EPSILON)
}

fn adaptive_shell_inputs(window: &super::LushtextWindow) -> AdaptiveShellInputs {
    adaptive_shell_inputs_for_width(window, current_window_width(window))
}

fn adaptive_shell_inputs_for_width(
    window: &super::LushtextWindow,
    window_width: i32,
) -> AdaptiveShellInputs {
    let imp = window.imp();
    AdaptiveShellInputs {
        window_width,
        workspace_preset: workspace_sidebar_preset(window),
        workspace_requested_visible: imp.secondary_surfaces.workspace_requested_visible.get(),
        properties_requested_visible: imp.secondary_surfaces.properties_requested_visible.get(),
        compact_surface: imp.secondary_surfaces.compact_surface.get(),
        focus_mode_active: imp.focus_mode.active.get(),
    }
}

fn workspace_consumes_width_for_intent(input: AdaptiveShellInputs) -> bool {
    !input.focus_mode_active
        && input.workspace_requested_visible
        && input.window_width > WORKSPACE_BREAKPOINT_MAX_WIDTH_SP
}

fn preferred_compact_surface_for_intent(input: AdaptiveShellInputs) -> Option<SecondarySurface> {
    if let Some(surface) = input.compact_surface
        && secondary_surface_requested_for_intent(input, surface)
    {
        return Some(surface);
    }

    if input.properties_requested_visible {
        Some(SecondarySurface::DocumentProperties)
    } else {
        None
    }
}

fn secondary_surface_requested_for_intent(
    input: AdaptiveShellInputs,
    surface: SecondarySurface,
) -> bool {
    match surface {
        SecondarySurface::Workspace => input.workspace_requested_visible,
        SecondarySurface::DocumentProperties => input.properties_requested_visible,
    }
}

fn derive_adaptive_shell_layout(input: AdaptiveShellInputs) -> AdaptiveShellLayout {
    let workspace_consumes_width = workspace_consumes_width_for_intent(input);
    let workspace_width_sp = if workspace_consumes_width {
        input.workspace_preset.clamped_width_sp(input.window_width)
    } else {
        0.0
    };
    let properties_breakpoint_max_width = properties_breakpoint_max_width_sp(workspace_width_sp);
    let properties_presentation = if input.window_width <= properties_breakpoint_max_width {
        PropertiesPresentation::Sheet
    } else {
        PropertiesPresentation::Pane
    };
    let compact = properties_presentation == PropertiesPresentation::Sheet;
    let workspace_collapsed = input.window_width <= WORKSPACE_BREAKPOINT_MAX_WIDTH_SP;
    let compact_surface = if compact {
        preferred_compact_surface_for_intent(input)
    } else {
        None
    };

    let render_workspace = if input.focus_mode_active {
        false
    } else if workspace_collapsed {
        compact_surface == Some(SecondarySurface::Workspace) && input.workspace_requested_visible
    } else if compact {
        !(compact_surface == Some(SecondarySurface::DocumentProperties)
            && input.properties_requested_visible)
            && input.workspace_requested_visible
    } else {
        input.workspace_requested_visible
    };
    let render_properties = if input.focus_mode_active {
        false
    } else if compact {
        compact_surface == Some(SecondarySurface::DocumentProperties)
            && input.properties_requested_visible
    } else {
        input.properties_requested_visible
    };

    AdaptiveShellLayout {
        properties_breakpoint_max_width,
        workspace_consumes_width,
        properties_presentation,
        compact_surface,
        render_workspace,
        render_properties,
    }
}

fn effective_workspace_sidebar_width_sp(window: &super::LushtextWindow, window_width: i32) -> f64 {
    workspace_sidebar_preset(window).clamped_width_sp(window_width)
}

fn effective_workspace_sidebar_fraction(window: &super::LushtextWindow, window_width: i32) -> f64 {
    workspace_sidebar_preset(window).effective_fraction(window_width)
}

fn sync_workspace_sidebar_width_constraints(window: &super::LushtextWindow, window_width: i32) {
    let target_width = effective_workspace_sidebar_width_sp(window, window_width);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "The sidebar target width is derived from the current split width and remains within i32 paned coordinates"
    )]
    let target_width_request = target_width.round() as i32;
    let split = &window.imp().workspace_split_view;
    if (split.min_sidebar_width() - target_width).abs() > f64::EPSILON {
        split.set_min_sidebar_width(target_width);
    }
    if (split.max_sidebar_width() - target_width).abs() > f64::EPSILON {
        split.set_max_sidebar_width(target_width);
    }
    if window.imp().sidebar.width_request() != target_width_request {
        window.imp().sidebar.set_width_request(target_width_request);
    }
}

fn desired_properties_fraction(window_width: i32) -> f64 {
    fixed_fraction(
        window_width,
        PROPERTIES_SIDEBAR_MIN_WIDTH_SP,
        FIXED_PROPERTIES_SIDEBAR_FRACTION,
    )
}

fn effective_properties_fraction(window: &super::LushtextWindow, window_width: i32) -> f64 {
    let total_fraction = desired_properties_fraction(window_width);
    if derive_adaptive_shell_layout(adaptive_shell_inputs_for_width(window, window_width))
        .workspace_consumes_width
    {
        let total_width = f64::from(window_width.max(1));
        let workspace_width = effective_workspace_sidebar_width_sp(window, window_width);
        let remaining_fraction = (1.0 - workspace_width / total_width).max(f64::EPSILON);
        let inner_width = (total_width - workspace_width).max(1.0);
        let lower = (PROPERTIES_SIDEBAR_MIN_WIDTH_SP / inner_width).min(1.0);
        (total_fraction / remaining_fraction).max(lower).min(1.0)
    } else {
        total_fraction
    }
}

fn properties_presentation(window: &super::LushtextWindow) -> PropertiesPresentation {
    let layout_name = window.imp().properties_layout_view.layout_name();
    PropertiesPresentation::from_layout_name(layout_name.as_deref())
}

fn properties_surface_is_compact(window: &super::LushtextWindow) -> bool {
    properties_presentation(window) == PropertiesPresentation::Sheet
}

fn focus_is_within(window: &super::LushtextWindow, folder: &gtk4::Widget) -> bool {
    let mut focus = gtk4::prelude::GtkWindowExt::focus(window);
    while let Some(widget) = focus {
        if widget.as_ptr() == folder.as_ptr() {
            return true;
        }
        focus = widget.parent();
    }
    false
}

fn set_workspace_sidebar_preset(
    window: &super::LushtextWindow,
    preset: WorkspaceSidebarWidthPreset,
) {
    let fraction = preset.fraction();
    if (window
        .imp()
        .settings
        .double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION)
        - fraction)
        .abs()
        > f64::EPSILON
    {
        let _ = window
            .imp()
            .settings
            .set_double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION, fraction);
    }
    sync_split_view_widths(window, current_window_width(window));
}

fn sync_properties_breakpoint(window: &super::LushtextWindow) {
    let max_width =
        derive_adaptive_shell_layout(adaptive_shell_inputs(window)).properties_breakpoint_max_width;
    if window.imp().properties_breakpoint_max_width.get() == max_width {
        return;
    }
    let condition =
        libadwaita::BreakpointCondition::parse(&properties_breakpoint_condition(max_width))
            .expect("valid properties breakpoint condition");
    if let Some(breakpoint) = window.imp().properties_breakpoint.borrow().as_ref() {
        breakpoint.set_condition(Some(&condition));
    }
    window.imp().properties_breakpoint_max_width.set(max_width);
}

fn sync_secondary_surfaces(window: &super::LushtextWindow) {
    let imp = window.imp();
    let layout = derive_adaptive_shell_layout(adaptive_shell_inputs(window));
    let compact = layout.properties_presentation == PropertiesPresentation::Sheet;
    let was_workspace_visible = imp.workspace_split_view.shows_sidebar();
    let was_properties_visible = window.rendered_document_properties_visible();
    let focus_in_workspace = focus_is_within(window, imp.sidebar.upcast_ref::<gtk4::Widget>());
    let focus_in_properties =
        focus_is_within(window, imp.properties_panel.upcast_ref::<gtk4::Widget>());

    if properties_presentation(window) != layout.properties_presentation {
        imp.properties_layout_view
            .set_layout_name(layout.properties_presentation.layout_name());
    }

    if !compact {
        imp.secondary_surfaces.compact_surface.set(None);
    } else if imp.secondary_surfaces.compact_surface.get() != layout.compact_surface {
        imp.secondary_surfaces
            .compact_surface
            .set(layout.compact_surface);
    }

    if imp.workspace_split_view.shows_sidebar() != layout.render_workspace {
        imp.workspace_split_view
            .set_show_sidebar(layout.render_workspace);
    }

    if compact {
        if imp.properties_split_view.shows_sidebar() {
            imp.properties_split_view.set_show_sidebar(false);
        }
        if imp.properties_bottom_sheet.is_open() != layout.render_properties {
            imp.properties_bottom_sheet
                .set_open(layout.render_properties);
        }
    } else {
        if imp.properties_bottom_sheet.is_open() {
            imp.properties_bottom_sheet.set_open(false);
        }
        if imp.properties_split_view.shows_sidebar() != layout.render_properties {
            imp.properties_split_view
                .set_show_sidebar(layout.render_properties);
        }
    }

    window.sync_secondary_surface_action_states();

    if (was_workspace_visible && !layout.render_workspace && focus_in_workspace)
        || (was_properties_visible
            && !layout.render_properties
            && (focus_in_properties || window.active_editor().is_none()))
    {
        window.restore_focus_after_secondary_pane_close();
    }
}

fn sync_properties_split_view(window: &super::LushtextWindow, window_width: i32) {
    let expected = effective_properties_fraction(window, window_width);
    if (window.imp().properties_split_view.sidebar_width_fraction() - expected).abs() > f64::EPSILON
    {
        window
            .imp()
            .properties_split_view
            .set_sidebar_width_fraction(expected);
    }
}

fn sync_split_view_widths_for_allocation(window: &super::LushtextWindow, window_width: i32) {
    if window.imp().split_width_synced_for_width.get() == window_width {
        return;
    }
    sync_split_view_widths(window, window_width);
}

fn sync_split_view_widths(window: &super::LushtextWindow, window_width: i32) {
    if window.imp().split_width_syncing.replace(true) {
        return;
    }

    let workspace_fraction = effective_workspace_sidebar_fraction(window, window_width);
    sync_workspace_sidebar_width_constraints(window, window_width);
    if (window.imp().workspace_split_view.sidebar_width_fraction() - workspace_fraction).abs()
        > f64::EPSILON
    {
        window
            .imp()
            .workspace_split_view
            .set_sidebar_width_fraction(workspace_fraction);
    }
    window.sync_preview_width_constraints(window_width);
    if !window.imp().workspace_sidebar_transition_settle.pending() {
        sync_properties_breakpoint(window);
        sync_properties_split_view(window, window_width);
        sync_secondary_surfaces(window);
    }

    window.imp().split_width_synced_for_width.set(window_width);
    window.imp().split_width_syncing.set(false);
}

impl super::LushtextWindow {
    /// Return whether the workspace sidebar is requested in user state.
    pub(super) fn workspace_sidebar_requested_visible(&self) -> bool {
        self.imp()
            .secondary_surfaces
            .workspace_requested_visible
            .get()
    }

    /// Return whether document properties are requested in user state.
    pub(super) fn document_properties_requested_visible(&self) -> bool {
        self.imp()
            .secondary_surfaces
            .properties_requested_visible
            .get()
    }

    /// Return whether the workspace sidebar is currently rendered on screen.
    pub(super) fn rendered_workspace_sidebar_visible(&self) -> bool {
        self.imp().workspace_split_view.shows_sidebar()
    }

    /// Return whether document properties are currently rendered on screen.
    pub(super) fn rendered_document_properties_visible(&self) -> bool {
        if self.document_properties_uses_bottom_sheet() {
            self.imp().properties_bottom_sheet.is_open()
        } else {
            self.imp().properties_split_view.shows_sidebar()
        }
    }

    /// Return whether document properties currently use the compact sheet presentation.
    pub(super) fn document_properties_uses_bottom_sheet(&self) -> bool {
        properties_surface_is_compact(self)
    }

    /// Recompute the adaptive properties host after any explicit visibility change.
    pub(super) fn sync_secondary_surface_layout(&self) {
        if self.imp().workspace_sidebar_transition_settle.pending() {
            return;
        }
        self.sync_secondary_surface_layout_now();
    }

    /// Start the Adwaita workspace-sidebar transition before reconciling other surfaces.
    pub(super) fn start_workspace_sidebar_transition(&self) {
        let width = current_window_width(self);
        let layout = derive_adaptive_shell_layout(adaptive_shell_inputs_for_width(self, width));
        sync_workspace_sidebar_width_constraints(self, width);
        let sidebar_will_change =
            self.imp().workspace_split_view.shows_sidebar() != layout.render_workspace;
        if sidebar_will_change {
            self.imp().workspace_sidebar_transition_settle.schedule(
                self,
                std::time::Duration::from_millis(WORKSPACE_SIDEBAR_TRANSITION_SETTLE_DELAY_MS),
                |window, handle| {
                    window.sync_secondary_surface_layout_now();
                    handle.finish_if_current();
                },
            );
            self.imp()
                .workspace_split_view
                .set_show_sidebar(layout.render_workspace);
        }
        self.sync_secondary_surface_action_states();

        if !sidebar_will_change {
            self.sync_secondary_surface_layout_now();
            let _ = self.imp().workspace_sidebar_transition_settle.clear();
        }
    }

    /// Return whether the sidebar transition is still blocking geometry readiness.
    pub(crate) fn workspace_sidebar_transition_pending(&self) -> bool {
        self.imp().workspace_sidebar_transition_settle.pending()
    }

    /// Test seam exposing whether sidebar animation coordination blocks readiness.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn workspace_sidebar_transition_pending_for_test(&self) -> bool {
        self.workspace_sidebar_transition_pending()
    }

    /// Recompute the adaptive properties host after any explicit visibility change.
    fn sync_secondary_surface_layout_now(&self) {
        let width = current_window_width(self);
        sync_properties_breakpoint(self);
        sync_properties_split_view(self, width);
        sync_secondary_surfaces(self);
    }
}

fn fixed_fraction(window_width: i32, min_width_sp: f64, target_fraction: f64) -> f64 {
    let width = f64::from(window_width.max(1));
    let lower = (min_width_sp / width).min(1.0);
    target_fraction.max(lower).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::{
        AdaptiveShellInputs, DUAL_PANE_LAYOUT_OVERHEAD_SP, MIN_EDITOR_CONTENT_WIDTH_SP,
        PropertiesPresentation, SecondarySurface, WorkspaceSidebarWidthPreset,
        derive_adaptive_shell_layout, dual_sidebar_window_width_for_center,
        properties_breakpoint_max_width_sp,
    };

    #[test]
    fn properties_breakpoint_width_accounts_for_workspace_preset() {
        assert_eq!(
            properties_breakpoint_max_width_sp(WorkspaceSidebarWidthPreset::Comfy.max_width_sp()),
            1350
        );
        assert_eq!(properties_breakpoint_max_width_sp(0.0), 932);
        assert_eq!(
            properties_breakpoint_max_width_sp(WorkspaceSidebarWidthPreset::Small.max_width_sp()),
            1243
        );
        assert_eq!(
            properties_breakpoint_max_width_sp(WorkspaceSidebarWidthPreset::Large.max_width_sp()),
            1456
        );
    }

    #[test]
    fn adaptive_layout_budgets_requested_workspace_even_when_compact_suppresses_it() {
        let layout = derive_adaptive_shell_layout(AdaptiveShellInputs {
            window_width: 1200,
            workspace_preset: WorkspaceSidebarWidthPreset::Comfy,
            workspace_requested_visible: true,
            properties_requested_visible: true,
            compact_surface: None,
            focus_mode_active: false,
        });

        assert_eq!(layout.properties_breakpoint_max_width, 1350);
        assert_eq!(
            layout.properties_presentation,
            PropertiesPresentation::Sheet
        );
        assert_eq!(
            layout.compact_surface,
            Some(SecondarySurface::DocumentProperties)
        );
        assert!(!layout.render_workspace);
        assert!(layout.render_properties);
    }

    #[test]
    fn adaptive_layout_does_not_open_workspace_overlay_for_passive_compact_shrink() {
        let layout = derive_adaptive_shell_layout(AdaptiveShellInputs {
            window_width: 837,
            workspace_preset: WorkspaceSidebarWidthPreset::Comfy,
            workspace_requested_visible: true,
            properties_requested_visible: false,
            compact_surface: None,
            focus_mode_active: false,
        });

        assert_eq!(
            layout.properties_presentation,
            PropertiesPresentation::Sheet
        );
        assert_eq!(layout.compact_surface, None);
        assert!(!layout.render_workspace);
        assert!(!layout.render_properties);
    }

    #[test]
    fn adaptive_layout_keeps_explicit_compact_workspace_overlay() {
        let layout = derive_adaptive_shell_layout(AdaptiveShellInputs {
            window_width: 837,
            workspace_preset: WorkspaceSidebarWidthPreset::Comfy,
            workspace_requested_visible: true,
            properties_requested_visible: false,
            compact_surface: Some(SecondarySurface::Workspace),
            focus_mode_active: false,
        });

        assert_eq!(layout.compact_surface, Some(SecondarySurface::Workspace));
        assert!(layout.render_workspace);
        assert!(!layout.render_properties);
    }

    #[test]
    fn dual_sidebar_width_helper_preserves_requested_center_space() {
        let center_target = MIN_EDITOR_CONTENT_WIDTH_SP + DUAL_PANE_LAYOUT_OVERHEAD_SP;
        let total_width = dual_sidebar_window_width_for_center(
            center_target,
            WorkspaceSidebarWidthPreset::Large.max_width_sp(),
        );
        assert!(
            (total_width * 0.75
                - WorkspaceSidebarWidthPreset::Large.max_width_sp()
                - center_target)
                .abs()
                < 0.001
        );
    }

    #[test]
    fn workspace_sidebar_target_width_clamps_for_representative_window_sizes() {
        assert_eq!(
            WorkspaceSidebarWidthPreset::Small.clamped_width_sp(900),
            220.0
        );
        assert_eq!(
            WorkspaceSidebarWidthPreset::Comfy.clamped_width_sp(1200),
            360.0
        );
        assert_eq!(
            WorkspaceSidebarWidthPreset::Large.clamped_width_sp(1400),
            440.0
        );
        assert_eq!(
            WorkspaceSidebarWidthPreset::Comfy.clamped_width_sp(2000),
            360.0
        );
    }
}
