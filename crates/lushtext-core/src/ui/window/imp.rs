// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the main application window.
//!
//! This module owns the composite-template wiring, long-lived window state,
//! split-view persistence, and the callback glue that binds the sidebar,
//! command palette, session restore, and notifications into one shell.

use super::drafts::{DraftMutationIntent, DraftMutationOrder};
use super::drafts::{DraftRestoreTicket, PendingPreservation};
use super::geometry::execution::{
    configure_split_views, current_window_width, effective_properties_fraction,
    effective_workspace_sidebar_fraction, install_split_view_breakpoints,
    migrate_split_view_settings, restore_properties_split_view, restore_workspace_split_view,
    set_workspace_sidebar_preset, sync_properties_breakpoint, sync_properties_split_view,
    sync_secondary_surfaces, sync_split_view_widths_for_allocation,
};
use super::geometry::policy::NORMAL_MODE_MIN_HEIGHT_SP;
pub use super::geometry::policy::SecondarySurface;
use super::notes::ActiveNotesBrowser;
use super::session_restore::SessionRestoreRuntime;
use super::session_restore::policy::SessionRestoreTurnMetrics;
use crate::config::{self, keys};
use crate::model::draft::{DraftManifest, DraftManifestAuthority, PreloadedDraftRestore};
use crate::model::recent_document::RecentDocumentEntry;
use crate::model::workspace::WorkspaceScope;
use crate::services::notifications::NotificationBus;
use crate::services::palette::{
    FileIndexBuildCoordinator, FileIndexBuildStart, NoteSourceRefreshCoordinator,
    NoteSourceRefreshStart,
};
use crate::ui::accessibility;
use crate::ui::buffer_snapshot::BufferSnapshotHandle;
use crate::ui::command_palette::LushtextCommandPalette;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::markdown_preview::LushtextMarkdownPreview;
use crate::ui::open_popover::LushtextOpenPopover;
use crate::ui::properties_panel::LushtextPropertiesPanel;
use crate::ui::search_panel::LushtextSearchPanel;
use crate::ui::sidebar::LushtextSidebar;
use crate::ui::sidebar::width_preset::WorkspaceSidebarWidthPreset;
use crate::ui::status_bar::{LushtextStatusBar, MessageKind};
use glib::prelude::*;
use gtk_lush_settle::{Debounce, SettleBurst, SupersedingTimer};
use gtk_lush_widgets::ClipBin;
use gtk4::prelude::*;
use gtk4::{self, CompositeTemplate, gio, glib};
use libadwaita::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

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
pub(super) const WORKSPACE_SIDEBAR_TRANSITION_SETTLE_DELAY_MS: u64 = 260;

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

/// Editor-memory accounting shared by the eviction helpers.
#[derive(Default)]
pub struct EditorMemoryState {
    /// Constant-work per-editor residency records and saturating aggregates.
    pub ledger: RefCell<crate::model::editor_memory::EditorResidencyLedger>,
    /// Previously selected editor, retained weakly for O(1) eligibility refresh.
    pub active_editor: RefCell<Option<glib::WeakRef<LushtextEditorPage>>>,
    /// Exceptional lifecycle state requiring a current full reconciliation.
    pub accounting_uncertain: Cell<bool>,
    /// Whether a next-main-loop aggregate evaluation is already queued.
    pub evaluation_armed: Cell<bool>,
    /// Guard that prevents overlapping aggregate evaluations.
    pub evaluation_running: Cell<bool>,
    /// Narrow guard for signals emitted synchronously by `EditorPage::evict`.
    pub applying_eviction: Cell<bool>,
    /// Window-wide generation source used for deterministic least-recent use.
    pub next_access_generation: Cell<u64>,
    /// Number of aggregate evaluations, retained for coalescing assertions.
    pub evaluation_count: Cell<u64>,
    /// Full tab scans performed only for enforcement or reconciliation.
    pub full_scan_count: Cell<u64>,
    /// Stable result of the most recently completed aggregate policy pass.
    pub last_outcome: Cell<crate::model::editor_memory::EditorMemoryBudgetOutcome>,
    /// Idle dispatches used to prove that candidate application stays bounded.
    #[cfg(feature = "test-utils")]
    pub eviction_dispatch_count: Cell<u64>,
    /// One-shot race injector run after planning and before candidate rechecks.
    #[cfg(feature = "test-utils")]
    pub before_eviction_hook: RefCell<Option<Box<dyn FnOnce()>>>,
    /// One-shot between-turn transition injector for stale-plan tests.
    #[cfg(feature = "test-utils")]
    pub after_eviction_hook: RefCell<Option<Box<dyn FnOnce()>>>,
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
    /// Cooperative cancellation for startup manifest/session recovery work.
    pub restore_cancel: RefCell<Option<Arc<AtomicBool>>>,
    /// One paced admission wakeup before startup recovery I/O begins.
    pub(super) restore_capacity_wakeup: crate::ui::plain_disposal::ProgressDisposalCapacityWakeup,
    /// Active bounded GTK session-restore generation, if any.
    pub(super) restore_runtime: RefCell<Option<SessionRestoreRuntime>>,
    /// Monotonic identity allocated before each bounded restore generation.
    pub(super) next_restore_generation: Cell<u64>,
    /// Last generation's terminal or cancellation counters, retained after the
    /// runtime that owned them is taken.
    ///
    /// A **last-restore outcome record**, not a cached evidence surface: the
    /// surface projects this field rather than reading a cache of itself.
    pub(super) last_restore_outcome: Cell<Option<SessionRestoreTurnMetrics>>,
    /// Aggregate tab-derived projection rebuilds, retained for boundedness proof.
    pub(super) tab_projection_publications: Cell<u64>,
    /// User/CLI tab-selection intent accepted since window construction.
    pub(super) selection_generation: Cell<u64>,
    /// Suppresses generation changes for restore-owned transient selections.
    pub(super) applying_restore_selection: Cell<bool>,
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
    /// Monotonic identity source for sequential multi-editor close saves.
    pub next_close_save_identity: Cell<u64>,
    /// Current close-save batch allowed to admit and publish completion.
    pub active_close_save_identity: Cell<Option<u64>>,
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
    /// Whether the first-dirty timer currently has a callback armed.
    ///
    /// `SupersedingTimer` exposes no armed query, and the draft evidence surface
    /// owes an honest answer, so the workflow tracks it beside the timer it
    /// describes — the same shape `orphan_cleanup_timer_pending` already uses.
    pub(super) first_dirty_autosave_pending: Cell<bool>,
    /// Superseding startup/follow-up timer for bounded orphan cleanup.
    pub orphan_cleanup_timer: SupersedingTimer,
    /// Whether one orphan-cleanup inspect/execute worker is active.
    pub orphan_cleanup_inflight: Cell<bool>,
    /// Latest manifest offset requested while an older cleanup worker was active.
    pub orphan_cleanup_pending_offset: Cell<Option<usize>>,
    /// Consecutive retryable cleanup failures used for bounded backoff.
    pub orphan_cleanup_failure_streak: Cell<u32>,
    /// Whether the owned orphan-cleanup timer currently has a callback armed.
    pub orphan_cleanup_timer_pending: Cell<bool>,
    /// Number of orphan-cleanup workers started by this window.
    ///
    /// Always compiled for the same reason as the retained-body counters: the
    /// draft evidence surface reports them, and the invariant the high-water mark
    /// proves — never two concurrent cleanup passes, either of which could delete
    /// a body the other had just re-validated — is a production invariant.
    pub(super) orphan_cleanup_workers_started: Cell<usize>,
    /// Peak simultaneous orphan-cleanup workers observed by this window.
    pub(super) orphan_cleanup_workers_high_water: Cell<usize>,
    /// In-memory draft manifest kept in sync with disk.
    pub manifest: RefCell<DraftManifest>,
    /// Completeness and durable replacement authority for the in-memory manifest.
    pub manifest_authority: Cell<DraftManifestAuthority>,
    /// Draft restore outcomes preloaded during session restore and consumed once.
    pub preloaded:
        RefCell<crate::ui::plain_disposal::DisposalOwned<HashMap<String, PreloadedDraftRestore>>>,
    /// Whether a draft autosave batch is currently writing draft files/manifest state.
    pub autosave_inflight: Cell<bool>,
    /// Whether another autosave pass is needed after the in-flight batch finishes.
    pub autosave_pending: Cell<bool>,
    /// Main-thread intent allocator shared by autosave and deletion workflows.
    pub(super) mutation_order: RefCell<DraftMutationOrder>,
    /// Whether one autosave batch or delete command currently owns mutation execution.
    pub(super) mutation_inflight: Cell<bool>,
    /// Compact deletes waiting behind an earlier draft mutation.
    pub(super) pending_deletes: RefCell<VecDeque<DraftMutationIntent>>,
    /// IDs represented in `pending_deletes`, keeping common admission O(1).
    pub(super) pending_delete_ids: RefCell<HashSet<String>>,
    /// Current deletion intents whose durable manifest removal may outlive a
    /// failed body deletion and must remain explicit on a later retry.
    pub(super) delete_tombstones: RefCell<HashMap<String, DraftMutationIntent>>,
    /// Stale file-backed drafts whose queued delete must first preserve the
    /// body (set-aside copy, plus local history when it accepts it). A failed
    /// preservation leaves the body and its persisted entry in place.
    pub(super) stale_preservations: RefCell<HashMap<String, PendingPreservation>>,
    /// Draft ids whose recovery body is queued or being read for restore,
    /// with how many restore tickets for each are still outstanding.
    ///
    /// Autosave must not write a body for such an id: the new buffer would
    /// overwrite the recovery body before restore could apply or preserve it.
    pub(super) restore_pending_ids: RefCell<HashMap<String, usize>>,
    /// Cancellation token for the current autosave buffer copy, if any.
    pub(crate) autosave_snapshot: RefCell<Option<BufferSnapshotHandle>>,
    /// Cancellation token for the current close-time buffer copy, if any.
    pub(crate) close_snapshot: RefCell<Option<BufferSnapshotHandle>>,
    /// Draft IDs explicitly discarded during an in-progress close flow.
    /// These must not be re-written by `flush_dirty_drafts()` right before the
    /// window is destroyed.
    pub close_discard_ids: RefCell<HashSet<String>>,
    /// Serialized non-preloaded recovery reads, including startup budget skips.
    pub(super) lazy_restore_queue: RefCell<VecDeque<DraftRestoreTicket>>,
    /// Whether one lazy draft body is currently crossing the worker boundary.
    pub(super) lazy_restore_inflight: Cell<bool>,
    /// One paced progress-lane wakeup retained while a lazy ticket stays compact.
    pub(super) lazy_restore_capacity_wakeup:
        crate::ui::plain_disposal::ProgressDisposalCapacityWakeup,
    /// Number of asynchronous draft resolutions not yet delivered to GTK.
    pub(super) restore_inflight_count: Cell<usize>,
    /// Number of complete autosave bodies currently held across a worker handoff.
    ///
    /// Always compiled, not test-gated: the draft evidence surface reports it, and
    /// an evidence surface must be readable in a production build. Two `usize`
    /// cells per window, and the invariant they prove — the pipeline never holds
    /// more than one complete document-sized body — is a production invariant.
    pub(super) retained_complete_bodies: Cell<usize>,
    /// Peak complete-body count observed by this window.
    pub(super) max_retained_complete_bodies: Cell<usize>,
}

impl DraftState {
    /// Cancel owned orphan-cleanup scheduling during window teardown.
    pub(super) fn dispose_orphan_cleanup(&self) {
        self.orphan_cleanup_timer_pending.set(false);
        self.orphan_cleanup_pending_offset.set(None);
        let _ = self.orphan_cleanup_timer.invalidate();
    }
}

/// Startup data-flow gate state owned by the window shell.
#[derive(Default)]
pub struct StartupDataFlowState {
    /// Whether format preflight and any required user decision have resolved.
    pub completed: Cell<bool>,
    /// Whether a preflight task is already running for this window.
    pub running: Cell<bool>,
    /// External activation paths queued while startup metadata consumers are paused.
    ///
    /// Bounded by `MAX_PENDING_ACTIVATION_OPENS`; see
    /// `startup_data::queue_activation_open_if_startup_pending` for why the
    /// overflow is dropped rather than opened immediately.
    pub pending_activation_paths: RefCell<Vec<PathBuf>>,
    /// How many activation opens were refused because the queue was full.
    pub dropped_activation_opens: Cell<usize>,
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
    /// A weak handle prevents menu state from retaining a detached tab.
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
    /// Requested-versus-rendered state for the workspace sidebar and document properties.
    pub secondary_surfaces: SecondarySurfaceState,
    /// Whether the side-by-side preview pane is currently visible.
    pub preview_visible: Cell<bool>,
    /// Whether the preview-only mode (Alt+P) is active (editor hidden, preview full-width).
    pub preview_mode: Cell<bool>,
    /// Legacy preferred side-by-side preview width from `preview-pane-position`.
    pub preferred_preview_width: Cell<i32>,
    /// Whether showing the side-by-side sidebar waits for its first allocation.
    ///
    /// See `LushtextWindow::show_preview_sidebar_once_allocated`.
    pub preview_sidebar_show_deferred: Cell<bool>,
    /// Settle burst while preview layout switching or embedded widget repair is pending.
    pub preview_transition_settle: SettleBurst,
    /// Settle burst while Adwaita's workspace sidebar transition is in flight.
    pub workspace_sidebar_transition_settle: SettleBurst,
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
    /// Weak handle for browser-navigation actions while Browse Notes is visible.
    pub(super) active_notes_browser: RefCell<Option<ActiveNotesBrowser>>,
    /// Active large note-save snapshots, owned until completion or window disposal.
    pub(super) note_save_snapshots: RefCell<Vec<BufferSnapshotHandle>>,
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
            preview_sidebar_show_deferred: Cell::new(false),
            preview_transition_settle: SettleBurst::default(),
            workspace_sidebar_transition_settle: SettleBurst::default(),
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
            saved_focus: RefCell::new(None),
            transient_child_escape_handled: Cell::new(false),
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
            active_notes_browser: RefCell::new(None),
            note_save_snapshots: RefCell::new(Vec::new()),
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
            // Returning to the window is the attention moment that matters:
            // external changes happen while the application is *not* focused,
            // by construction, so this trigger is correlated with the cause
            // rather than merely with elapsed time. The throttle and the
            // yield-to-user-work guard live in `window/attention_refresh.rs`.
            obj.connect_notify_local(Some("is-active"), |window, _| {
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
            let window_weak = obj.downgrade();
            let _ =
                crate::ui::workspace_visibility::connect_visibility_changed(settings, move || {
                    if let Some(window) = window_weak.upgrade() {
                        window.rebuild_file_index();
                    }
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
                    window.handle_sidebar_file_renamed(old_path, new_path);
                }
            });

        let window_weak = obj.downgrade();
        self.sidebar.connect_file_deleted(move |path| {
            if let Some(window) = window_weak.upgrade() {
                window.handle_sidebar_file_deleted(path);
            }
        });

        let window_weak = obj.downgrade();
        self.sidebar.connect_file_created(move |path| {
            if let Some(window) = window_weak.upgrade() {
                window.handle_sidebar_file_created(path);
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
                    let session = &window.imp().session;
                    if !session.applying_restore_selection.get() {
                        session
                            .selection_generation
                            .set(session.selection_generation.get().wrapping_add(1));
                    }
                    if window.tab_projection_refresh_deferred() {
                        return;
                    }
                    window.refresh_selected_tab_model_projections();
                }
            });

        let window_weak = obj.downgrade();
        self.tab_view.connect_close_page(move |tab_view, page| {
            let window = window_weak.upgrade();
            super::LushtextWindow::handle_tab_close_request(window.as_ref(), tab_view, page)
        });

        let window_weak = obj.downgrade();
        self.tab_view.connect_page_detached(move |_, page, _| {
            if let Some(window) = window_weak.upgrade()
                && !window.imp().disposing.get()
            {
                window.handle_tab_detached(page);
            }
        });

        obj.update_content_stack();
    }

    fn dispose(&self) {
        self.disposing.set(true);
        if let Some(cancel) = self.session.restore_cancel.take() {
            cancel.store(true, Ordering::Release);
        }
        self.session.restore_capacity_wakeup.cancel();
        self.drafts.lazy_restore_capacity_wakeup.cancel();
        self.obj().cancel_session_restore_for_dispose();
        self.file_index_builds.borrow_mut().invalidate();
        self.file_index_admission.borrow_mut().take();
        self.file_index_capacity_wakeup.cancel();
        self.command_palette_note_refreshes
            .borrow_mut()
            .invalidate();
        self.command_palette_note_admission.borrow_mut().take();
        self.command_palette_note_capacity_wakeup.cancel();
        if let Some(source_id) = self.drafts.autosave_source_id.take() {
            source_id.remove();
        }
        // The first-dirty timer needs the same explicit teardown its
        // orphan-cleanup sibling gets below: `dispose()` runs before `Drop`, so
        // relying on the timer's own drop would leave a window in which it can
        // still fire against torn-down workflow state. Clearing the flag in the
        // same place keeps the evidence surface honest for a disposed window,
        // which would otherwise keep reporting `first_dirty_timer_pending: true`.
        self.drafts.first_dirty_autosave_pending.set(false);
        let _ = self.drafts.first_dirty_autosave_timer.invalidate();
        self.drafts.dispose_orphan_cleanup();
        // Chunked snapshots have later GTK slices queued. Cancel before the
        // window's workflow state is torn down so none can resume after dispose.
        if let Some(snapshot) = self.drafts.autosave_snapshot.take() {
            snapshot.dispose();
        }
        if let Some(snapshot) = self.drafts.close_snapshot.take() {
            snapshot.dispose();
        }
        for snapshot in self.note_save_snapshots.take() {
            snapshot.dispose();
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
            window.begin_async_close_safety();
            return glib::Propagation::Stop;
        }

        let window_for_close = window.clone();
        window.show_save_changes_dialog(&modified, move |confirmed| {
            if confirmed {
                window_for_close.begin_async_close_safety();
            }
        });
        glib::Propagation::Stop
    }
}

impl ApplicationWindowImpl for LushtextWindow {}
impl AdwApplicationWindowImpl for LushtextWindow {}
