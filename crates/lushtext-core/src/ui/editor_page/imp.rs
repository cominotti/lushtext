// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the editor page widget.
//!
//! Each tab in the editor is an `EditorPage` containing a GtkSourceView,
//! a minimap, a search bar, and per-tab state (file path, size, eviction status).
//! GSettings bindings keep all editor pages in sync with user preferences.

use crate::config::keys;
use crate::model::bookmark::BookmarkRecord;
use crate::model::encoding::{DocumentEncodingState, FileHealthFinding, InvisibleCharactersMode};
use crate::model::formatting_overrides::FormattingOverrides;
use crate::services::file_limits::FileSizeCheck;
use crate::services::notifications::InlineActionNotification;
use crate::ui::buffer_snapshot::BufferSnapshotHandle;
use crate::ui::info_bar::LushtextInfoBar;
use crate::ui::search_bar::LushtextSearchBar;
use glib::value::ToValue;
use gtk_lush_settle::{Debounce, SettleBurst, SupersedingTimer, TimerToken};
use gtk_lush_signals::SignalBag;
use gtk_lush_viewport::{RestPause, RestState, ViewportObserver};
use gtk_lush_widgets::RenderHoldOverlay;
use gtk4::gio;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, glib};
use sourceview5::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::buffer_replacement::BufferReplacementState;
use super::minimap::{MinimapAvailability, MinimapMarker};
use super::style_scheme::apply_color_scheme_to_editor;

/// Callback for notifying the window when residency or eviction safety changes.
type MemoryChangedCallback = Box<dyn Fn()>;
type NotificationCallback = Box<dyn Fn(InlineActionNotification)>;
type LoadCompletedCallback = Box<dyn FnOnce()>;
type LoadFailedCallback = Box<dyn FnOnce(String)>;
type FileLoadedCallback = Box<dyn Fn()>;
type NotesChangedCallback = Box<dyn Fn()>;
type BookmarkActivatedCallback = Box<dyn Fn(BookmarkRecord)>;

// GTK callbacks receive shared `&self` references because several widget-tree
// owners may reference one object. State groups use `Cell` for Copy values and
// `RefCell` for runtime-checked mutation of owned values.

/// Coalesced end-of-document overscroll updates for one editor tab.
///
/// `LushtextEditorPage` is a `GtkBox` subclass, and GTK4 never invokes the
/// `size_allocate` vfunc on widgets whose class installs a layout manager, so
/// allocation hooks on the page itself are silently dead. Viewport geometry is
/// observed through the text view's scroll adjustments instead: their page
/// size tracks the editor viewport on every allocation, including each frame
/// of the sidebar show/hide animation.
#[derive(Default)]
pub struct OverscrollState {
    /// Generation counter used to collapse bursts of GTK allocations into one
    /// idle overscroll recomputation after the layout settles.
    pub update_generation: Cell<u32>,
    /// Drop-owned adjustment observers for viewport page-size and value changes.
    pub observer: RefCell<Option<ViewportObserver>>,
    /// Shared lower-edge rest state for horizontal and vertical editor axes.
    pub rest_state: RestState,
    /// Active pause that excludes transient adjustment values during reflow repair.
    pub reflow_pause: RefCell<Option<RestPause>>,
}

/// Signal handlers connected to application-global preference/theme objects.
#[derive(Default)]
pub struct PreferenceBindingState {
    /// Grouped global signal lifetimes for Settings and StyleManager.
    ///
    /// These sources outlive an editor tab, so they must be disconnected when
    /// the page is disposed to avoid stale closures retaining editor state.
    pub signals: SignalBag,
}

/// File-monitor state for external change detection.
#[derive(Default)]
pub struct MonitorState {
    /// File monitor for detecting external modifications. Created on file load,
    /// cancelled on tab close.
    pub file_monitor: RefCell<Option<gio::FileMonitor>>,
    /// Debounce for file monitor events (500ms).
    pub monitor_debounce: Debounce,
    /// File mtime (seconds since epoch) at last load or save.
    pub last_known_mtime: Cell<Option<u64>>,
}

/// Draft-recovery state scoped to one editor tab.
#[derive(Default)]
pub struct DraftState {
    /// Whether the buffer has been modified since the last draft save.
    pub draft_dirty: Cell<bool>,
    /// Monotonic counter used to reject stale autosave completions.
    ///
    /// Every user edit bumps this value. Draft autosave captures the counter with
    /// the text snapshot and clears `draft_dirty` only if the background write
    /// succeeds for the same generation.
    pub dirty_generation: Cell<u64>,
    /// Stable draft identifier for this tab across autosave cycles.
    pub draft_id: RefCell<Option<String>>,
    /// Whether this tab is currently showing draft-restored content.
    pub draft_restored: Cell<bool>,
    /// Whether automatic recovery is paused after a snapshot exceeded policy.
    ///
    /// The warning stays set until a later matching generation is accepted by
    /// both the body writer and manifest commit.
    pub automatic_recovery_limited: Cell<bool>,
}

/// Save lifecycle state for one editor tab.
#[derive(Default)]
pub struct SaveState {
    /// Whether a background save is currently writing this editor's snapshot.
    ///
    /// The modified flag stays true while this is set so close flows cannot treat
    /// an in-flight durable write as already safe.
    pub inflight: Cell<bool>,
    /// Monotonic identity for the active save and any formatting installation.
    pub generation: Cell<u64>,
    /// Lifecycle handle for a save snapshot that is yielding between chunks.
    pub snapshot: RefCell<Option<BufferSnapshotHandle>>,
}

/// One editor-scoped warning action routed through the shared inline alert buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingWarningAction {
    /// Open the line-ending chooser so mixed endings can be normalized.
    NormalizeLineEndings,
    /// Restore the buffer state that existed before a local-history restore.
    UndoLocalHistoryRestore,
}

/// Encoding, line-ending, health, and save-confirmation state for one tab.
#[derive(Default)]
pub struct DocumentMetadataState {
    /// Current open/save encoding and line-ending facts for this tab.
    pub encoding_state: Cell<DocumentEncodingState>,
    /// Whether the current on-disk representation carried a byte-order mark.
    pub has_bom: Cell<bool>,
    /// Encoding-adjacent health findings surfaced for the current content.
    pub file_health: RefCell<Vec<FileHealthFinding>>,
    /// Per-tab invisible-character visibility mode.
    pub invisible_mode: Cell<InvisibleCharactersMode>,
    /// Shared info-bar action currently routed to this editor.
    pub warning_action: Cell<Option<PendingWarningAction>>,
    /// One-shot guard that allows the next save to proceed even if the current
    /// encoding conversion is known to be lossy.
    pub allow_lossy_save_once: Cell<bool>,
    /// Monotonic request counter for async lossy-encoding analysis.
    ///
    /// Save-encoding choices can be made from a dialog while the user keeps
    /// editing or switches tabs. Capturing this counter with the request lets
    /// the window ignore stale worker results instead of showing an outdated
    /// lossy-conversion confirmation.
    pub lossy_analysis_generation: Cell<u32>,
    /// Active large-buffer capture for the newest lossy-analysis request.
    pub lossy_analysis_snapshot: RefCell<Option<BufferSnapshotHandle>>,
}

/// File-load lifecycle callbacks that need to survive repeated reloads.
#[derive(Default)]
pub struct LoadState {
    /// One-shot callback fired after the first successful file load.
    pub load_completed_callback: RefCell<Option<LoadCompletedCallback>>,
    /// One-shot callback fired after the first failed file load.
    ///
    /// Window-level open flows use this to undo provisional tab/path state only
    /// after the background load has actually failed.
    pub load_failed_callback: RefCell<Option<LoadFailedCallback>>,
    /// Recurring callbacks fired after every successful file load or reload.
    ///
    /// Notes, local history, and future tab-local workflows all need the same
    /// "a real file just finished loading" hook, so this stays fan-out friendly.
    pub file_loaded_callbacks: RefCell<Vec<FileLoadedCallback>>,
    /// Whether load installation is suppressing document-amplifying projections.
    pub projection_suspended: Cell<bool>,
    /// Current generation-bound chunked install session, if any.
    pub(crate) installation: RefCell<Option<Rc<RefCell<super::load_save::ChunkedLoadInstall>>>>,
    /// Whether signal-emitting final projection work owns the editor lifecycle.
    ///
    /// Reload requests are reduced to one latest pending intent while this is
    /// set, so synchronous GTK signals cannot start a replacement halfway
    /// through finalization or strand it behind a terminal install session.
    pub finalizing: Cell<bool>,
    /// Whether widget disposal interrupted signal-emitting finalization.
    pub dispose_during_finalization: Cell<bool>,
    /// Latest replacement request waiting for bounded cancellation cleanup.
    pub(crate) pending_load: RefCell<Option<super::load_save::PendingFileLoad>>,
    /// Whether cancellation discarded a partially installed file payload.
    ///
    /// The buffer is intentionally cleared on that path, so saving must remain
    /// blocked until a retry installs one exact payload successfully.
    pub installation_incomplete: Cell<bool>,
    /// Whether bounded cleanup must publish the user-cancelled terminal state.
    pub user_cancel_pending: Cell<bool>,
    /// Completed main-loop slices for the newest installation.
    pub installation_slice_count: Cell<u64>,
}

/// User-visible file-load lifecycle for one editor tab.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EditorLoadState {
    /// An untitled tab with no saved-file identity.
    #[default]
    Untitled,
    /// A file-backed tab whose background read has not settled yet.
    Loading,
    /// A file-backed tab whose content has loaded successfully.
    Loaded,
    /// A tab showing a failed load placeholder or recoverable user edits.
    Failed,
}

/// Deferred cursor and scroll restoration applied after async file load.
#[derive(Default)]
pub struct RestoreState {
    /// Deferred cursor line to apply after async file load completes.
    pub cursor_line: Cell<Option<u32>>,
    /// Deferred cursor column to apply after async file load completes.
    pub cursor_col: Cell<Option<u32>>,
    /// Deferred scroll-to line to apply after async file load completes.
    pub scroll_line: Cell<Option<u32>>,
}

/// Debounced persistence state shared by note-sidecar projections.
#[derive(Default)]
pub struct NotesPersistenceState {
    /// Debounce used to schedule background sidecar saves.
    pub save_debounce: Debounce,
    /// Guard preventing overlapping saves for the same note projection.
    pub save_inflight: Cell<bool>,
    /// Dirty flag set while a save is already in flight.
    pub save_dirty: Cell<bool>,
}

/// One live bookmark projected into a `GtkSourceMark`.
#[derive(Clone)]
pub struct LiveBookmark {
    /// Current persisted bookmark fields mirrored from the sidecar model.
    pub record: BookmarkRecord,
    /// Source-view mark that moves with the buffer while the file is open.
    pub mark: sourceview5::Mark,
}

/// Live bookmark projection state scoped to one editor tab.
#[derive(Default)]
pub struct BookmarkState {
    /// Current bookmark marks projected into the source buffer.
    pub entries: RefCell<Vec<LiveBookmark>>,
    /// Monotonic token used to reject stale async sidecar loads.
    ///
    /// Bookmark sidecar I/O finishes later on the GTK main loop. If the user
    /// edits bookmarks before that completion lands, the stale load must not
    /// replace the newer live projection.
    pub change_generation: Cell<u64>,
    /// Callback invoked when bookmark state changes and should be persisted.
    pub changed_callback: RefCell<Option<NotesChangedCallback>>,
    /// Window callback installed after editor construction to route gutter activation
    /// into bookmark editing UI.
    ///
    /// Stored in a `RefCell` because GObject-style methods receive `&self`, so
    /// callback wiring mutates implementation state through interior mutability.
    pub activated_callback: RefCell<Option<BookmarkActivatedCallback>>,
    /// Debounced sidecar persistence state for bookmark saves.
    pub persistence: NotesPersistenceState,
}

/// Minimap widgets, marker state, and signal lifetimes for one editor tab.
#[derive(Default)]
pub struct MinimapState {
    /// Programmatically created `GtkSourceMap` bound to the main source view.
    pub source_map: RefCell<Option<sourceview5::Map>>,
    /// Narrow drawing layer that paints semantic markers over the map edge.
    pub marker_strip: RefCell<Option<gtk4::DrawingArea>>,
    /// Last computed render model for the semantic marker strip.
    pub markers: RefCell<Vec<MinimapMarker>>,
    /// Source marks that keep modified-since-save lines aligned with later edits.
    pub modified_marks: RefCell<Vec<sourceview5::Mark>>,
    /// Current availability state for the minimap on this tab.
    pub availability: Cell<MinimapAvailability>,
    /// Cached answer for whether wrapped minimap layout would be too expensive.
    ///
    /// Estimating long-line wrapping can scan the buffer once after edits. Caching
    /// keeps resize-driven refreshes from repeatedly walking large documents.
    pub wrapped_layout_too_large: Cell<Option<bool>>,
    /// Debounce for coalescing expensive marker refresh work.
    pub refresh_debounce: Debounce,
    /// Whether a debounced minimap refresh callback is still waiting to run.
    pub refresh_pending: Cell<bool>,
    /// Reusable render-hold owner for frozen minimap pixels during width reflow.
    pub render_hold: RefCell<Option<RenderHoldOverlay>>,
    /// Settle burst while a width-reflow repair waits for a stable width.
    pub reflow_settle: SettleBurst,
    /// Whether repaired live map pixels are warming underneath the frozen cover.
    ///
    /// This is separate from `reflow_settle` so user scrolling can reveal
    /// the already-repaired live map without pretending the width burst itself is
    /// still unresolved.
    pub reflow_reveal_pending: Cell<bool>,
    /// Prevents programmatic loads and evictions from being recorded as user edits.
    pub tracking_suspended: Cell<bool>,
    /// Tracks which lines already own a modified marker for O(1) de-duplication.
    pub modified_lines_cache: RefCell<BTreeSet<u32>>,
    /// One-shot guard so the "too large for minimap" message does not spam on each edit.
    pub too_large_feedback_shown: Cell<bool>,
    /// Handler ID for the buffer's `insert-text` signal. Disconnected in dispose.
    pub buffer_signals: SignalBag,
}

/// Temporary editor presentation state while the window is in Focus Mode.
#[derive(Default)]
pub struct FocusModeEditorState {
    /// Whether Focus Mode is currently shaping this editor page.
    pub active: Cell<bool>,
    /// Left margin to restore when Focus Mode exits.
    pub normal_left_margin: Cell<i32>,
    /// Right margin to restore when Focus Mode exits.
    pub normal_right_margin: Cell<i32>,
    /// Target readable-column width in characters.
    pub target_columns: Cell<u32>,
    /// Whether typewriter scrolling is enabled for source editing.
    pub typewriter_scrolling: Cell<bool>,
    /// Gentle overlay line that marks the source editor's column-zero text origin while focused.
    pub text_origin_guide: RefCell<Option<gtk4::DrawingArea>>,
    /// Buffer signal lifetimes that drive typewriter scrolling and cursor centering.
    pub buffer_signals: SignalBag,
}

/// Automatic local-history capture state scoped to one editor tab.
#[derive(Default)]
pub struct LocalHistoryState {
    /// Last clean saved text used to capture the "before edits" baseline snapshot.
    pub last_clean_text: RefCell<Option<String>>,
    /// Generation of the clean baseline currently owned by this editor/path cycle.
    pub clean_baseline_generation: Cell<u64>,
    /// One automatic retry admission for the current clean baseline generation.
    pub baseline_retry_budget: Cell<u8>,
    /// Generation counter used to cancel or replace pending periodic capture timers.
    pub periodic_generation: Cell<u32>,
    /// One physically superseding five-minute callback owned by this tab.
    pub periodic_timer: SupersedingTimer,
    /// Current timer token for readiness and deterministic lifecycle tests.
    pub periodic_timer_token: Cell<Option<TimerToken>>,
    /// Generation advanced when disposal invalidates every in-flight capture.
    pub editor_generation: Cell<u64>,
    /// Generation advanced whenever the saved-file identity changes.
    pub path_generation: Cell<u64>,
    /// Generation advanced on every buffer content change.
    pub edit_generation: Cell<u64>,
    /// Cancellation for the current chunked periodic snapshot, if any.
    pub(crate) periodic_snapshot: RefCell<Option<BufferSnapshotHandle>>,
    /// Whether this editor already has one weak baseline waiter in the global FIFO.
    pub(crate) baseline_retry_pending: Cell<bool>,
    /// Suppresses automatic capture while save or restore changes the buffer programmatically.
    pub automatic_capture_suppressed: Cell<bool>,
    /// One-shot text used by the browser's immediate undo-restore action.
    pub restore_undo_text: RefCell<Option<String>>,
    /// Buffer signal lifetimes that drive automatic local-history capture.
    pub buffer_signals: SignalBag,
    /// Deterministic policy seam for widget tests without allocating huge buffers.
    #[cfg(feature = "test-utils")]
    pub availability_override:
        Cell<Option<crate::services::local_history_service::LocalHistoryAvailability>>,
}

/// Private template implementation for one editor tab.
///
/// Owns the GtkSourceView, minimap overlay, search bar, and per-tab state that
/// must stay adjacent to the template children. GObject methods always take
/// `&self` because multiple parts of the widget tree hold references
/// simultaneously; `Cell` and `RefCell` provide the required interior
/// mutability.
// `CompositeTemplate` loads the compiled XML resource; `TemplateChild` fields
// are populated automatically from matching widget IDs during initialization.
#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/editor-page.ui")]
pub struct LushtextEditorPage {
    /// Inline notification bar rendered above the text editor surface.
    #[template_child]
    pub info_bar: TemplateChild<LushtextInfoBar>,
    /// Root overlay that hosts the editor and transient overlay children.
    #[template_child]
    pub overlay: TemplateChild<gtk4::Overlay>,
    /// Overlay layer used for the minimap and its hold/projection chrome.
    #[template_child]
    pub minimap_overlay: TemplateChild<gtk4::Overlay>,
    /// Primary GtkSourceView buffer and editing surface.
    #[template_child]
    pub source_view: TemplateChild<sourceview5::View>,
    /// Scroller that owns editor viewport geometry and adjustment signals.
    #[template_child]
    pub scrolled_window: TemplateChild<gtk4::ScrolledWindow>,
    /// Visibility container that shows or hides the in-tab find/replace bar.
    #[template_child]
    pub search_revealer: TemplateChild<gtk4::Revealer>,
    /// In-tab find/replace control bound to this editor's buffer.
    #[template_child]
    pub search_bar: TemplateChild<LushtextSearchBar>,

    /// Absolute path of the file being edited. `None` for untitled tabs.
    pub file_path: RefCell<Option<PathBuf>>,
    /// Canonical file identity resolved by the background load path.
    ///
    /// Duplicate-tab reconciliation uses this after load completion so the GTK
    /// thread never has to call `Path::canonicalize()` while opening files.
    pub canonical_file_path: RefCell<Option<PathBuf>>,
    /// On-disk file size in bytes, populated after async load completes.
    /// Used for memory estimation and status bar display.
    pub file_size: Cell<Option<u64>>,
    /// Explicit file-load lifecycle state for duplicate ownership decisions.
    pub load_state: Cell<EditorLoadState>,
    /// Whether the newest accepted load attempt failed while content remained visible.
    ///
    /// A failed reload may preserve the prior buffer and `Loaded` state, so
    /// eviction safety cannot be inferred from `load_state` alone.
    pub latest_load_failed: Cell<bool>,
    /// Feature gate classification based on file size (syntax, undo thresholds).
    pub size_check: Cell<FileSizeCheck>,
    /// Whether this tab's buffer was evicted to free memory. Evicted tabs
    /// reload from disk when re-focused.
    pub evicted: Cell<bool>,
    /// Window-wide least-recently-used generation assigned on load or activation.
    pub memory_access_generation: Cell<u64>,
    /// Page-local generation advanced whenever residency or eligibility changes.
    pub memory_policy_generation: Cell<u64>,
    /// Synthetic estimate used by deterministic budget widget tests.
    #[cfg(feature = "test-utils")]
    pub memory_estimate_override: Cell<Option<u64>>,
    /// Cooperative cancellation token for the current background file load.
    /// A fresh `Arc<AtomicBool>` per load prevents a newer request from
    /// uncancelling an older background worker.
    pub cancel_token: RefCell<Arc<AtomicBool>>,
    /// Monotonic identity for file loads so stale background completions cannot
    /// apply after a newer open or reopen starts for the same editor tab.
    pub load_generation: Cell<u64>,
    /// Last style-scheme ID actually applied to this buffer.
    pub applied_style_scheme_id: RefCell<Option<String>>,
    /// Current document-surface opacity for the main editor text area.
    pub document_surface_opacity: Cell<f64>,
    /// Application-wide GSettings instance for editor preference bindings.
    pub settings: gio::Settings,
    /// Grouped signal-handler IDs for application-global settings/theme wiring.
    pub preference_bindings: PreferenceBindingState,
    /// Per-file formatting overrides from EditorConfig. Empty for untitled tabs
    /// or files without a matching `.editorconfig`.
    pub formatting_overrides: Cell<FormattingOverrides>,
    /// Buffer signals wired by the owning window for tab title, draft, and preview state.
    pub document_buffer_signals: SignalBag,
    /// Editor-local buffer signals for user edit actions.
    pub editing_buffer_signals: SignalBag,
    /// Page-to-window callback invoked when residency or eviction safety changes.
    ///
    /// The `RefCell` permits replacing the single GTK-main-thread observer
    /// through the wrapper's shared `&self` API.
    pub memory_changed_callback: RefCell<Option<MemoryChangedCallback>>,
    /// Callback invoked when the editor needs to surface an inline notification.
    pub notification_callback: RefCell<Option<NotificationCallback>>,
    /// External file-monitor state.
    pub monitor: MonitorState,
    /// Draft lifecycle state.
    pub draft: DraftState,
    /// Background save lifecycle state.
    pub save: SaveState,
    /// Per-document encoding, line-ending, and health metadata.
    pub document_metadata: DocumentMetadataState,
    /// File-load lifecycle callbacks.
    pub load: LoadState,
    /// Single active/latest whole-buffer mutation owner shared by workflows.
    pub replacement: BufferReplacementState,
    /// Deferred cursor/scroll restoration state.
    pub restore: RestoreState,
    /// Dynamic editor overscroll scheduling state.
    pub overscroll: OverscrollState,
    /// Live bookmark mark projection and persistence state.
    pub bookmarks: BookmarkState,
    /// Automatic local-history capture lifecycle state.
    pub local_history: LocalHistoryState,
    /// Minimap widget state, marker projections, and refresh bookkeeping.
    pub minimap: MinimapState,
    /// Focus Mode presentation state scoped to this tab.
    pub focus_mode: FocusModeEditorState,
}

impl Default for LushtextEditorPage {
    fn default() -> Self {
        Self {
            info_bar: TemplateChild::default(),
            overlay: TemplateChild::default(),
            minimap_overlay: TemplateChild::default(),
            source_view: TemplateChild::default(),
            scrolled_window: TemplateChild::default(),
            search_revealer: TemplateChild::default(),
            search_bar: TemplateChild::default(),
            file_path: RefCell::default(),
            canonical_file_path: RefCell::default(),
            file_size: Cell::default(),
            load_state: Cell::new(EditorLoadState::Untitled),
            latest_load_failed: Cell::new(false),
            size_check: Cell::new(FileSizeCheck::Normal),
            evicted: Cell::new(false),
            memory_access_generation: Cell::new(0),
            memory_policy_generation: Cell::new(0),
            #[cfg(feature = "test-utils")]
            memory_estimate_override: Cell::new(None),
            cancel_token: RefCell::new(Arc::new(AtomicBool::new(false))),
            load_generation: Cell::new(0),
            applied_style_scheme_id: RefCell::new(None),
            document_surface_opacity: Cell::new(1.0),
            settings: gio::Settings::new(crate::config::APP_ID),
            preference_bindings: PreferenceBindingState::default(),
            formatting_overrides: Cell::new(FormattingOverrides::default()),
            document_buffer_signals: SignalBag::new(),
            editing_buffer_signals: SignalBag::new(),
            memory_changed_callback: RefCell::default(),
            notification_callback: RefCell::default(),
            monitor: MonitorState::default(),
            draft: DraftState::default(),
            save: SaveState::default(),
            document_metadata: DocumentMetadataState::default(),
            load: LoadState::default(),
            replacement: BufferReplacementState::default(),
            restore: RestoreState::default(),
            overscroll: OverscrollState::default(),
            bookmarks: BookmarkState::default(),
            local_history: LocalHistoryState::default(),
            minimap: MinimapState::default(),
            focus_mode: FocusModeEditorState::default(),
        }
    }
}

// ObjectSubclass registers this struct with GLib's runtime type system.
// NAME must match the `class` attribute in the UI template XML.
#[glib::object_subclass]
impl ObjectSubclass for LushtextEditorPage {
    const NAME: &'static str = "LushtextEditorPage";
    type Type = super::LushtextEditorPage;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        // Register child widget types BEFORE parsing the template.
        LushtextInfoBar::ensure_type();
        LushtextSearchBar::ensure_type();
        sourceview5::View::ensure_type();
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextEditorPage {
    // Disconnect the buffer's modified-changed handler here rather than in
    // Rust's Drop. GTK4's dispose() runs BEFORE Drop and clears all template
    // children — accessing `self.source_view` in Drop panics because the
    // TemplateChild's OnceCell is already empty.
    fn dispose(&self) {
        super::save_runtime::cancel_for_editor(&self.obj());
        self.obj().cancel_buffer_replacement_for_dispose();
        self.obj().dispose_load_resources();
        if let Some(snapshot) = self.save.snapshot.take() {
            snapshot.dispose();
        }
        if let Some(snapshot) = self.document_metadata.lossy_analysis_snapshot.take() {
            snapshot.dispose();
        }
        if let Some(snapshot) = self.local_history.periodic_snapshot.take() {
            snapshot.dispose();
        }
        let _ = self.local_history.periodic_timer.invalidate();
        self.local_history.periodic_timer_token.set(None);
        self.local_history
            .editor_generation
            .set(self.local_history.editor_generation.get().wrapping_add(1));
        self.preference_bindings.signals.clear();
        self.document_buffer_signals.clear();
        self.editing_buffer_signals.clear();
        self.local_history.buffer_signals.clear();
        self.minimap.buffer_signals.clear();
        self.focus_mode.buffer_signals.clear();
        self.overscroll.observer.borrow_mut().take();
        self.overscroll.reflow_pause.borrow_mut().take();
        self.minimap.source_map.borrow_mut().take();
        self.minimap.marker_strip.borrow_mut().take();
        self.minimap.render_hold.borrow_mut().take();
        self.focus_mode.text_origin_guide.borrow_mut().take();
        self.minimap.modified_marks.borrow_mut().clear();
        self.minimap.modified_lines_cache.borrow_mut().clear();
        self.minimap.markers.borrow_mut().clear();
        // Cancel file monitor to stop receiving events for this tab.
        if let Some(monitor) = self.monitor.file_monitor.take() {
            monitor.cancel();
        }
    }

    fn constructed(&self) {
        self.parent_constructed();

        // GtkSourceView's View.buffer() returns a generic GtkTextBuffer.
        // Downcast to sourceview5::Buffer for syntax highlighting methods.
        let buffer = self
            .source_view
            .buffer()
            .downcast::<sourceview5::Buffer>()
            .expect("GtkSourceView buffer");
        buffer.set_highlight_syntax(true);

        let settings = &self.settings;
        let invisible_mode = crate::model::encoding::InvisibleCharactersMode::from_id(
            settings.string(keys::INVISIBLE_CHARACTERS_MODE).as_str(),
        )
        .unwrap_or_default();
        self.document_metadata.invisible_mode.set(invisible_mode);

        // GSettings bind() creates a live sync between the settings key and
        // the widget property. GET flag = one-way: setting changes update the
        // widget, but widget changes don't write back to settings.
        // Two-way binding (DEFAULT flag) is used in the preferences dialog.
        settings
            .bind(
                keys::SHOW_LINE_NUMBERS,
                &*self.source_view,
                "show-line-numbers",
            )
            .flags(gio::SettingsBindFlags::GET)
            .build();
        settings
            .bind(
                keys::BOOKMARK_GUTTER_VISIBLE,
                &*self.source_view,
                "show-line-marks",
            )
            .flags(gio::SettingsBindFlags::GET)
            .build();
        settings
            .bind(
                keys::HIGHLIGHT_CURRENT_LINE,
                &*self.source_view,
                "highlight-current-line",
            )
            .flags(gio::SettingsBindFlags::GET)
            .build();
        // Formatting settings (tab-width, insert-spaces, indent-width) use
        // manual handlers instead of Settings::bind(GET) so that EditorConfig
        // overrides can take priority. The handler reads the current overrides
        // and only falls back to GSettings when no override is active.
        apply_formatting_settings(&self.source_view, settings, FormattingOverrides::default());
        for key in [keys::TAB_WIDTH, keys::INSERT_SPACES] {
            let editor_weak = self.obj().downgrade();
            let id = settings.connect_changed(Some(key), move |_, _| {
                if let Some(editor) = editor_weak.upgrade() {
                    let overrides = editor.imp().formatting_overrides.get();
                    apply_formatting_settings(
                        &editor.imp().source_view,
                        &editor.imp().settings,
                        overrides,
                    );
                }
            });
            self.preference_bindings.signals.track(settings, id);
        }

        // Keep this one-way from settings to the view. `mapping` converts the
        // stored boolean Variant into the GtkWrapMode property value; the
        // separate handler below only refreshes minimap geometry.
        settings
            .bind(keys::WORD_WRAP, &*self.source_view, "wrap-mode")
            .get_only()
            .mapping(|variant, _| {
                let enabled = variant.get::<bool>()?;
                Some(word_wrap_mode(enabled).to_value())
            })
            .build();
        let editor_weak: glib::WeakRef<super::LushtextEditorPage> = self.obj().downgrade();
        let id = settings.connect_changed(Some(keys::WORD_WRAP), move |_, _| {
            if let Some(editor) = editor_weak.upgrade() {
                editor.sync_minimap_wrap_mode();
                editor.schedule_minimap_refresh();
            }
        });
        self.preference_bindings.signals.track(settings, id);

        apply_color_scheme_to_editor(&self.obj());
        self.document_surface_opacity
            .set(settings.double(keys::TAB_CONTENT_OPACITY).clamp(0.0, 1.0));
        {
            let editor_weak = self.obj().downgrade();
            let id = settings.connect_changed(Some(keys::STYLE_SCHEME), move |_, _| {
                if let Some(editor) = editor_weak.upgrade() {
                    apply_color_scheme_to_editor(&editor);
                }
            });
            self.preference_bindings.signals.track(settings, id);
        }
        {
            let editor_weak = self.obj().downgrade();
            let id = settings.connect_changed(Some(keys::TAB_CONTENT_OPACITY), move |_, _| {
                if let Some(editor) = editor_weak.upgrade() {
                    apply_color_scheme_to_editor(&editor);
                }
            });
            self.preference_bindings.signals.track(settings, id);
        }
        {
            let editor_weak = self.obj().downgrade();
            let style_manager = libadwaita::StyleManager::default();
            let handler_id = style_manager.connect_dark_notify(move |_| {
                if let Some(editor) = editor_weak.upgrade() {
                    apply_color_scheme_to_editor(&editor);
                }
            });
            self.preference_bindings
                .signals
                .track(&style_manager, handler_id);
        }

        {
            let editor_weak = self.obj().downgrade();
            let id = settings.connect_changed(Some(keys::SHOW_MINIMAP), move |_, _| {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.schedule_minimap_refresh();
                }
            });
            self.preference_bindings.signals.track(settings, id);
        }
        {
            let editor_weak = self.obj().downgrade();
            let id = settings.connect_changed(
                Some(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE),
                move |_, _| {
                    if let Some(editor) = editor_weak.upgrade() {
                        editor.schedule_minimap_refresh();
                    }
                },
            );
            self.preference_bindings.signals.track(settings, id);
        }
        {
            // Keep the overlay width one-way from settings. The mapping clamps
            // the stored integer for `width-request`; the handler below only
            // schedules marker and geometry redraw work.
            settings
                .bind(keys::MINIMAP_WIDTH, &*self.minimap_overlay, "width-request")
                .get_only()
                .mapping(|variant, _| {
                    let width = variant.get::<i32>()?.clamp(64, 160);
                    Some(width.to_value())
                })
                .build();
            let editor_weak = self.obj().downgrade();
            let id = settings.connect_changed(Some(keys::MINIMAP_WIDTH), move |_, _| {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.schedule_minimap_refresh();
                }
            });
            self.preference_bindings.signals.track(settings, id);
        }
        {
            let editor_weak = self.obj().downgrade();
            let id =
                settings.connect_changed(Some(keys::FOCUS_MODE_TARGET_COLUMNS), move |s, _| {
                    if let Some(editor) = editor_weak.upgrade() {
                        editor
                            .set_focus_mode_target_columns(s.uint(keys::FOCUS_MODE_TARGET_COLUMNS));
                    }
                });
            self.preference_bindings.signals.track(settings, id);
        }
        {
            let editor_weak = self.obj().downgrade();
            let id = settings.connect_changed(
                Some(keys::FOCUS_MODE_TYPEWRITER_SCROLLING),
                move |s, _| {
                    if let Some(editor) = editor_weak.upgrade() {
                        editor.set_focus_mode_typewriter_scrolling(
                            s.boolean(keys::FOCUS_MODE_TYPEWRITER_SCROLLING),
                        );
                    }
                },
            );
            self.preference_bindings.signals.track(settings, id);
        }

        self.obj().setup_bookmark_projection();
        self.obj().setup_local_history_context_menu();
        self.obj().setup_local_history_tracking();

        {
            let editor_weak = self.obj().downgrade();
            let handler_id = buffer.connect_end_user_action(move |_| {
                if let Some(editor) = editor_weak.upgrade() {
                    if editor.reconcile_bookmarks_after_edit() {
                        editor.emit_bookmarks_changed();
                    }
                    editor.schedule_minimap_refresh();
                }
            });
            self.editing_buffer_signals.track(&buffer, handler_id);
        }

        // Text changes update residency, while modified-state changes alter
        // eviction safety independently. The window coalesces both at idle.
        {
            let editor_weak = self.obj().downgrade();
            let handler_id = buffer.connect_changed(move |_| {
                if let Some(editor) = editor_weak.upgrade() {
                    if editor.load_projection_suspended() {
                        return;
                    }
                    editor.notify_memory_policy_changed();
                }
            });
            self.editing_buffer_signals.track(&buffer, handler_id);
        }

        {
            let editor_weak = self.obj().downgrade();
            let handler_id = buffer.connect_modified_changed(move |_| {
                if let Some(editor) = editor_weak.upgrade() {
                    if editor.load_projection_suspended() {
                        return;
                    }
                    editor.notify_memory_policy_changed();
                }
            });
            self.editing_buffer_signals.track(&buffer, handler_id);
        }

        // Search bar close: hide_search restores cursor and detaches the
        // SearchContext. The close handler fires on both the close button
        // and Escape key press in the search entry.
        let editor_weak = self.obj().downgrade();
        self.search_bar.connect_close(move || {
            if let Some(editor) = editor_weak.upgrade() {
                editor.hide_search();
            }
        });

        {
            let editor_weak = self.obj().downgrade();
            self.source_view.connect_map(move |_| {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.schedule_dynamic_overscroll_update();
                }
            });
        }

        self.obj().setup_minimap();
        self.obj().setup_allocation_reflow_observers();
        self.obj().setup_focus_mode_text_origin_guide();
        self.obj().setup_focus_mode_presentation();
        self.obj().apply_invisible_characters_mode();
        self.obj().refresh_accessibility_metadata();
    }
}

// No `size_allocate` override here on purpose: GTK4 skips that vfunc for
// widgets whose class installs a layout manager, and `GtkBox` uses
// `GtkBoxLayout`, so such an override would be silently dead code. Passive
// viewport geometry changes are observed through the text view's scroll
// adjustments in `overscroll.rs` instead.
impl WidgetImpl for LushtextEditorPage {}
impl BoxImpl for LushtextEditorPage {}

/// Apply formatting settings to the source view, resolving EditorConfig
/// overrides against GSettings fallbacks. Called on initial construction,
/// GSettings change, and EditorConfig resolution.
pub(super) fn apply_formatting_settings(
    view: &sourceview5::View,
    settings: &gio::Settings,
    overrides: FormattingOverrides,
) {
    let tab_width = overrides
        .tab_width
        .unwrap_or_else(|| settings.uint(keys::TAB_WIDTH));
    view.set_tab_width(tab_width);

    let insert_spaces = overrides
        .insert_spaces
        .unwrap_or_else(|| settings.boolean(keys::INSERT_SPACES));
    view.set_insert_spaces_instead_of_tabs(insert_spaces);

    // indent-width has no GSettings key — only settable via EditorConfig.
    // GtkSourceView default: -1 means "inherit from tab-width".
    let indent_width = overrides.indent_width.unwrap_or(-1);
    view.set_indent_width(indent_width);
}

fn word_wrap_mode(enabled: bool) -> gtk4::WrapMode {
    if enabled {
        gtk4::WrapMode::Word
    } else {
        gtk4::WrapMode::None
    }
}
