// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the editor page widget.
//!
//! Each tab in the editor is an `EditorPage` containing a GtkSourceView,
//! a minimap, a search bar, and per-tab state (file path, size, eviction status).
//! GSettings bindings keep all editor pages in sync with user preferences.

use crate::config::keys;
use crate::model::annotation::{AnnotationId, AnnotationRecord};
use crate::model::bookmark::BookmarkRecord;
use crate::model::encoding::{DocumentEncodingState, FileHealthFinding, InvisibleCharactersMode};
use crate::model::formatting_overrides::FormattingOverrides;
use crate::services::file_limits::FileSizeCheck;
use crate::services::notifications::InlineActionNotification;
use crate::ui::info_bar::LushtextInfoBar;
use crate::ui::search_bar::LushtextSearchBar;
use gtk4::gio;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, glib};
use sourceview5::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::minimap::{MinimapAvailability, MinimapMarker};

/// Callback for notifying the window when this editor's estimated buffer
/// memory changes. The `u64` argument is the new estimated byte count.
type MemoryChangedCallback = Box<dyn Fn(u64)>;
type NotificationCallback = Box<dyn Fn(InlineActionNotification)>;
type LoadCompletedCallback = Box<dyn FnOnce()>;
type FileLoadedCallback = Box<dyn Fn()>;
type NotesChangedCallback = Box<dyn Fn()>;

/// Coalesced end-of-document overscroll updates for one editor tab.
#[derive(Default)]
pub struct OverscrollState {
    /// Generation counter used to collapse bursts of GTK allocations into one
    /// idle overscroll recomputation after the layout settles.
    pub update_generation: Cell<u32>,
}

/// Signal handlers connected to application-global preference/theme objects.
#[derive(Default)]
pub struct PreferenceBindingState {
    /// Handler ID for `StyleManager::connect_dark_notify`. Disconnected in `Drop`
    /// to prevent stale closures keeping the buffer alive after tab close.
    pub dark_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    /// Handler ID for GSettings `word-wrap` change. Disconnected in `Drop`.
    pub word_wrap_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    /// Handler ID for GSettings `style-scheme` change. Disconnected in `Drop`.
    pub style_scheme_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    /// Handler ID for GSettings `tab-content-opacity` change. Disconnected in `Drop`.
    pub tab_content_opacity_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    /// Handler ID for GSettings `tab-width` change. Disconnected in `Drop`.
    pub tab_width_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    /// Handler ID for GSettings `insert-spaces` change. Disconnected in `Drop`.
    pub insert_spaces_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    /// Handler ID for GSettings `annotation-highlights-visible`. Disconnected in `Drop`.
    pub annotation_visibility_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    /// Handler ID for GSettings `show-minimap`. Disconnected in `Drop`.
    pub show_minimap_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    /// Handler ID for GSettings `minimap-width`. Disconnected in `Drop`.
    pub minimap_width_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    /// Handler ID for GSettings `focus-mode-target-columns`. Disconnected in `Drop`.
    pub focus_mode_columns_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    /// Handler ID for GSettings `focus-mode-typewriter-scrolling`. Disconnected in `Drop`.
    pub focus_mode_typewriter_handler_id: RefCell<Option<glib::SignalHandlerId>>,
}

/// File-monitor state for external change detection.
#[derive(Default)]
pub struct MonitorState {
    /// File monitor for detecting external modifications. Created on file load,
    /// cancelled on tab close.
    pub file_monitor: RefCell<Option<gio::FileMonitor>>,
    /// Generation counter for debouncing file monitor events (500ms).
    pub monitor_generation: Cell<u32>,
    /// File mtime (seconds since epoch) at last load or save.
    pub last_known_mtime: Cell<Option<u64>>,
}

/// Draft-recovery state scoped to one editor tab.
#[derive(Default)]
pub struct DraftState {
    /// Whether the buffer has been modified since the last draft save.
    pub draft_dirty: Cell<bool>,
    /// Stable draft identifier for this tab across autosave cycles.
    pub draft_id: RefCell<Option<String>>,
    /// Whether this tab is currently showing draft-restored content.
    pub draft_restored: Cell<bool>,
}

/// One editor-scoped warning action routed through the shared info bar buttons.
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
}

/// File-load lifecycle callbacks that need to survive repeated reloads.
#[derive(Default)]
pub struct LoadState {
    /// One-shot callback fired after the first successful file load.
    pub load_completed_callback: RefCell<Option<LoadCompletedCallback>>,
    /// Recurring callbacks fired after every successful file load or reload.
    ///
    /// Notes, local history, and future tab-local workflows all need the same
    /// "a real file just finished loading" hook, so this stays fan-out friendly.
    pub file_loaded_callbacks: RefCell<Vec<FileLoadedCallback>>,
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

/// Debounced persistence state shared by bookmark and annotation projections.
#[derive(Default)]
pub struct NotesPersistenceState {
    /// Generation counter used to debounce background sidecar saves.
    pub save_generation: Cell<u32>,
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
    /// Callback invoked when bookmark state changes and should be persisted.
    pub changed_callback: RefCell<Option<NotesChangedCallback>>,
    /// Debounced sidecar persistence state for bookmark saves.
    pub persistence: NotesPersistenceState,
}

/// One live annotation projected into range anchors and a text tag.
#[derive(Clone)]
pub struct LiveAnnotation {
    /// Current persisted annotation fields mirrored from the sidecar model.
    pub record: AnnotationRecord,
    /// Start anchor with right gravity so inserts at the boundary land before the range.
    pub start_mark: gtk4::TextMark,
    /// Exclusive end anchor with left gravity so inserts at the boundary stay outside.
    pub end_mark: gtk4::TextMark,
    /// Stable tag name used to keep the highlight tied to this annotation ID.
    pub tag_name: String,
    /// Native GtkSourceView end-of-line annotation shown beside the source text.
    pub source_annotation: sourceview5::Annotation,
}

/// Live annotation projection state scoped to one editor tab.
#[derive(Default)]
pub struct AnnotationState {
    /// Current annotation range anchors projected into the buffer.
    pub entries: RefCell<Vec<LiveAnnotation>>,
    /// GtkSourceView provider that renders native end-of-line annotations.
    pub source_provider: RefCell<Option<sourceview5::AnnotationProvider>>,
    /// Callback invoked when annotation state changes and should be persisted.
    pub changed_callback: RefCell<Option<NotesChangedCallback>>,
    /// Pending annotation ID that should reopen once the file load completes.
    pub pending_focus_id: RefCell<Option<AnnotationId>>,
    /// Whether annotations have been loaded for the current file content.
    pub loaded: Cell<bool>,
    /// Debounced sidecar persistence state for annotation saves.
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
    /// Generation counter for coalescing expensive marker refresh work.
    pub refresh_generation: Cell<u32>,
    /// Prevents programmatic loads and evictions from being recorded as user edits.
    pub tracking_suspended: Cell<bool>,
    /// Tracks which lines already own a modified marker for O(1) de-duplication.
    pub modified_lines_cache: RefCell<BTreeSet<u32>>,
    /// One-shot guard so the "too large for minimap" message does not spam on each edit.
    pub too_large_feedback_shown: Cell<bool>,
    /// Handler ID for the buffer's `insert-text` signal. Disconnected in dispose.
    pub insert_text_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    /// Handler ID for the buffer's `delete-range` signal. Disconnected in dispose.
    pub delete_range_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    /// Handler ID for the buffer's `modified-changed` signal used by minimap state. Disconnected in dispose.
    pub modified_changed_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    /// Handler ID for the buffer's `changed` signal used by minimap refresh. Disconnected in dispose.
    pub changed_handler_id: RefCell<Option<glib::SignalHandlerId>>,
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
    /// Handler ID for cursor movement through the insert mark.
    pub mark_set_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    /// Handler ID for inserted/deleted text changes.
    pub changed_handler_id: RefCell<Option<glib::SignalHandlerId>>,
}

/// Automatic local-history capture state scoped to one editor tab.
#[derive(Default)]
pub struct LocalHistoryState {
    /// Last clean saved text used to capture the "before edits" baseline snapshot.
    pub last_clean_text: RefCell<Option<String>>,
    /// Generation counter used to cancel or replace pending periodic capture timers.
    pub periodic_generation: Cell<u32>,
    /// Suppresses automatic capture while save or restore changes the buffer programmatically.
    pub automatic_capture_suppressed: Cell<bool>,
    /// One-shot text used by the browser's immediate undo-restore action.
    pub restore_undo_text: RefCell<Option<String>>,
    /// Handler ID for the buffer's `modified-changed` signal used by local history. Disconnected in dispose.
    pub modified_changed_handler_id: RefCell<Option<glib::SignalHandlerId>>,
}

// CompositeTemplate loads the UI layout from a compiled XML file (bundled
// as a GResource at build time). Each #[template_child] field is auto-bound
// to the widget with the matching `id` attribute in the XML.
//
// GObject methods always take &self because multiple parts of the widget tree
// hold references simultaneously. Cell<T> for Copy types (file_size, eviction
// flag), RefCell<T> for complex types (file_path, handler IDs). Cell has no
// runtime borrow overhead; RefCell panics on overlapping borrows.
#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/editor-page.ui")]
pub struct LushtextEditorPage {
    #[template_child]
    pub info_bar: TemplateChild<LushtextInfoBar>,
    #[template_child]
    pub overlay: TemplateChild<gtk4::Overlay>,
    #[template_child]
    pub minimap_overlay: TemplateChild<gtk4::Overlay>,
    #[template_child]
    pub source_view: TemplateChild<sourceview5::View>,
    #[template_child]
    pub scrolled_window: TemplateChild<gtk4::ScrolledWindow>,
    #[template_child]
    pub search_revealer: TemplateChild<gtk4::Revealer>,
    #[template_child]
    pub search_bar: TemplateChild<LushtextSearchBar>,

    /// Absolute path of the file being edited. `None` for untitled tabs.
    pub file_path: RefCell<Option<PathBuf>>,
    /// On-disk file size in bytes, populated after async load completes.
    /// Used for memory estimation and status bar display.
    pub file_size: Cell<Option<u64>>,
    /// Feature gate classification based on file size (syntax, undo thresholds).
    pub size_check: Cell<FileSizeCheck>,
    /// Whether this tab's buffer was evicted to free memory. Evicted tabs
    /// reload from disk when re-focused.
    pub evicted: Cell<bool>,
    /// Cooperative cancellation token for background file loads. `Arc<AtomicBool>`
    /// is Send+Sync, allowing the background thread to check it.
    pub cancel_token: Arc<AtomicBool>,
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
    /// Handler ID for the buffer's `modified-changed` signal. Disconnected in dispose.
    pub modified_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    /// Handler ID for the buffer's `changed` signal (preview refresh). Disconnected in dispose.
    pub buffer_changed_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    /// Handler ID for the buffer's `end-user-action` signal. Disconnected in dispose.
    pub end_user_action_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    /// Callback invoked when estimated buffer memory changes (load, save, evict).
    pub memory_changed_callback: RefCell<Option<MemoryChangedCallback>>,
    /// Callback invoked when the editor needs to surface an inline notification.
    pub notification_callback: RefCell<Option<NotificationCallback>>,
    /// External file-monitor state.
    pub monitor: MonitorState,
    /// Draft lifecycle state.
    pub draft: DraftState,
    /// Per-document encoding, line-ending, and health metadata.
    pub document_metadata: DocumentMetadataState,
    /// File-load lifecycle callbacks.
    pub load: LoadState,
    /// Deferred cursor/scroll restoration state.
    pub restore: RestoreState,
    /// Dynamic editor overscroll scheduling state.
    pub overscroll: OverscrollState,
    /// Live bookmark mark projection and persistence state.
    pub bookmarks: BookmarkState,
    /// Live annotation range projection and persistence state.
    pub annotations: AnnotationState,
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
            file_size: Cell::default(),
            size_check: Cell::new(FileSizeCheck::Normal),
            evicted: Cell::new(false),
            cancel_token: Arc::new(AtomicBool::new(false)),
            applied_style_scheme_id: RefCell::new(None),
            document_surface_opacity: Cell::new(1.0),
            settings: gio::Settings::new(crate::config::APP_ID),
            preference_bindings: PreferenceBindingState::default(),
            formatting_overrides: Cell::new(FormattingOverrides::default()),
            modified_handler_id: RefCell::new(None),
            buffer_changed_handler_id: RefCell::new(None),
            end_user_action_handler_id: RefCell::new(None),
            memory_changed_callback: RefCell::default(),
            notification_callback: RefCell::default(),
            monitor: MonitorState::default(),
            draft: DraftState::default(),
            document_metadata: DocumentMetadataState::default(),
            load: LoadState::default(),
            restore: RestoreState::default(),
            overscroll: OverscrollState::default(),
            bookmarks: BookmarkState::default(),
            annotations: AnnotationState::default(),
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
        let buffer = self
            .source_view
            .buffer()
            .downcast::<sourceview5::Buffer>()
            .expect("GtkSourceView buffer");
        if let Some(handler_id) = self.modified_handler_id.take() {
            buffer.disconnect(handler_id);
        }
        if let Some(handler_id) = self.buffer_changed_handler_id.take() {
            buffer.disconnect(handler_id);
        }
        if let Some(handler_id) = self.end_user_action_handler_id.take() {
            buffer.disconnect(handler_id);
        }
        if let Some(handler_id) = self.local_history.modified_changed_handler_id.take() {
            buffer.disconnect(handler_id);
        }
        if let Some(handler_id) = self.minimap.insert_text_handler_id.take() {
            buffer.disconnect(handler_id);
        }
        if let Some(handler_id) = self.minimap.delete_range_handler_id.take() {
            buffer.disconnect(handler_id);
        }
        if let Some(handler_id) = self.minimap.modified_changed_handler_id.take() {
            buffer.disconnect(handler_id);
        }
        if let Some(handler_id) = self.minimap.changed_handler_id.take() {
            buffer.disconnect(handler_id);
        }
        if let Some(handler_id) = self.focus_mode.mark_set_handler_id.take() {
            buffer.disconnect(handler_id);
        }
        if let Some(handler_id) = self.focus_mode.changed_handler_id.take() {
            buffer.disconnect(handler_id);
        }
        if let Some(provider) = self.annotations.source_provider.take() {
            self.source_view.annotations().remove_provider(&provider);
        }
        self.minimap.source_map.borrow_mut().take();
        self.minimap.marker_strip.borrow_mut().take();
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
        for (key, handler_field) in [
            (
                keys::TAB_WIDTH,
                &self.preference_bindings.tab_width_handler_id,
            ),
            (
                keys::INSERT_SPACES,
                &self.preference_bindings.insert_spaces_handler_id,
            ),
        ] {
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
            handler_field.replace(Some(id));
        }

        // Manual mapping: bool → WrapMode requires connect_changed instead of bind().
        // Store the handler ID so we can disconnect in Drop.
        apply_word_wrap(&self.source_view, settings);
        let view = self.source_view.clone();
        let editor_weak: glib::WeakRef<super::LushtextEditorPage> = self.obj().downgrade();
        let id = settings.connect_changed(Some(keys::WORD_WRAP), move |s, _| {
            apply_word_wrap(&view, s);
            if let Some(editor) = editor_weak.upgrade() {
                editor.schedule_minimap_refresh();
            }
        });
        self.preference_bindings
            .word_wrap_handler_id
            .replace(Some(id));

        *self.applied_style_scheme_id.borrow_mut() = apply_color_scheme(&buffer, settings);
        self.document_surface_opacity
            .set(settings.double(keys::TAB_CONTENT_OPACITY).clamp(0.0, 1.0));
        {
            let buf = buffer.clone();
            let s = settings.clone();
            let editor_weak = self.obj().downgrade();
            let id = settings.connect_changed(Some(keys::STYLE_SCHEME), move |_, _| {
                let applied = apply_color_scheme(&buf, &s);
                if let Some(editor) = editor_weak.upgrade() {
                    *editor.imp().applied_style_scheme_id.borrow_mut() = applied;
                }
            });
            self.preference_bindings
                .style_scheme_handler_id
                .replace(Some(id));
        }
        {
            let buf = buffer.clone();
            let s = settings.clone();
            let editor_weak = self.obj().downgrade();
            let id = settings.connect_changed(Some(keys::TAB_CONTENT_OPACITY), move |_, _| {
                let applied = apply_color_scheme(&buf, &s);
                if let Some(editor) = editor_weak.upgrade() {
                    editor
                        .imp()
                        .document_surface_opacity
                        .set(s.double(keys::TAB_CONTENT_OPACITY).clamp(0.0, 1.0));
                    *editor.imp().applied_style_scheme_id.borrow_mut() = applied;
                    editor.queue_minimap_draw();
                }
            });
            self.preference_bindings
                .tab_content_opacity_handler_id
                .replace(Some(id));
        }
        {
            let buf = buffer.downgrade();
            let editor_weak = self.obj().downgrade();
            let s = settings.clone();
            let style_manager = libadwaita::StyleManager::default();
            let handler_id = style_manager.connect_dark_notify(move |_| {
                if let Some(buf) = buf.upgrade() {
                    let applied = apply_color_scheme(&buf, &s);
                    if let Some(editor) = editor_weak.upgrade() {
                        *editor.imp().applied_style_scheme_id.borrow_mut() = applied;
                    }
                }
                if let Some(editor) = editor_weak.upgrade() {
                    editor
                        .imp()
                        .document_surface_opacity
                        .set(s.double(keys::TAB_CONTENT_OPACITY).clamp(0.0, 1.0));
                    editor.refresh_annotation_highlights();
                    editor.queue_minimap_draw();
                }
            });
            self.preference_bindings
                .dark_handler_id
                .replace(Some(handler_id));
        }

        {
            let editor_weak = self.obj().downgrade();
            let id =
                settings.connect_changed(Some(keys::ANNOTATION_HIGHLIGHTS_VISIBLE), move |s, _| {
                    if let Some(editor) = editor_weak.upgrade() {
                        editor.set_annotation_highlights_visible(
                            s.boolean(keys::ANNOTATION_HIGHLIGHTS_VISIBLE),
                        );
                    }
                });
            self.preference_bindings
                .annotation_visibility_handler_id
                .replace(Some(id));
        }
        {
            let editor_weak = self.obj().downgrade();
            let id = settings.connect_changed(Some(keys::SHOW_MINIMAP), move |_, _| {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.refresh_minimap();
                }
            });
            self.preference_bindings
                .show_minimap_handler_id
                .replace(Some(id));
        }
        {
            let editor_weak = self.obj().downgrade();
            let id = settings.connect_changed(Some(keys::MINIMAP_WIDTH), move |_, _| {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.apply_minimap_width_from_settings();
                    editor.schedule_minimap_refresh();
                }
            });
            self.preference_bindings
                .minimap_width_handler_id
                .replace(Some(id));
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
            self.preference_bindings
                .focus_mode_columns_handler_id
                .replace(Some(id));
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
            self.preference_bindings
                .focus_mode_typewriter_handler_id
                .replace(Some(id));
        }

        self.obj().setup_bookmark_projection();
        self.obj().setup_native_annotation_projection();
        self.obj().setup_local_history_context_menu();
        self.obj().setup_local_history_tracking();
        self.obj().set_annotation_highlights_visible(
            settings.boolean(keys::ANNOTATION_HIGHLIGHTS_VISIBLE),
        );

        {
            let editor_weak = self.obj().downgrade();
            let handler_id = buffer.connect_end_user_action(move |_| {
                if let Some(editor) = editor_weak.upgrade() {
                    if editor.reconcile_bookmarks_after_edit() {
                        editor.emit_bookmarks_changed();
                    }
                    if editor.reconcile_annotations_after_edit() {
                        editor.emit_annotations_changed();
                    }
                    editor.schedule_minimap_refresh();
                }
            });
            self.end_user_action_handler_id.replace(Some(handler_id));
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
        self.obj().setup_focus_mode_text_origin_guide();
        self.obj().setup_focus_mode_presentation();
        self.obj().apply_invisible_characters_mode();
    }
}

impl WidgetImpl for LushtextEditorPage {
    fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
        self.parent_size_allocate(width, height, baseline);

        // Recompute the EOF overscroll after GTK has allocated the text view so
        // `visible_rect()` reflects the real viewport height for this frame.
        self.obj().schedule_dynamic_overscroll_update();
        self.obj().refresh_focus_mode_readable_column();
        self.obj().queue_focus_mode_text_origin_guide_draw();
    }
}
impl BoxImpl for LushtextEditorPage {}

// Disconnect signal handlers from application-global objects (Settings,
// StyleManager) that outlive individual EditorPage instances. These don't
// access template children, so Rust's Drop is safe for them.
impl Drop for LushtextEditorPage {
    fn drop(&mut self) {
        if let Some(handler_id) = self.preference_bindings.dark_handler_id.take() {
            libadwaita::StyleManager::default().disconnect(handler_id);
        }
        if let Some(handler_id) = self.preference_bindings.word_wrap_handler_id.take() {
            self.settings.disconnect(handler_id);
        }
        if let Some(handler_id) = self.preference_bindings.style_scheme_handler_id.take() {
            self.settings.disconnect(handler_id);
        }
        if let Some(handler_id) = self
            .preference_bindings
            .tab_content_opacity_handler_id
            .take()
        {
            self.settings.disconnect(handler_id);
        }
        if let Some(handler_id) = self.preference_bindings.tab_width_handler_id.take() {
            self.settings.disconnect(handler_id);
        }
        if let Some(handler_id) = self.preference_bindings.insert_spaces_handler_id.take() {
            self.settings.disconnect(handler_id);
        }
        if let Some(handler_id) = self
            .preference_bindings
            .annotation_visibility_handler_id
            .take()
        {
            self.settings.disconnect(handler_id);
        }
        if let Some(handler_id) = self.preference_bindings.show_minimap_handler_id.take() {
            self.settings.disconnect(handler_id);
        }
        if let Some(handler_id) = self.preference_bindings.minimap_width_handler_id.take() {
            self.settings.disconnect(handler_id);
        }
        if let Some(handler_id) = self
            .preference_bindings
            .focus_mode_columns_handler_id
            .take()
        {
            self.settings.disconnect(handler_id);
        }
        if let Some(handler_id) = self
            .preference_bindings
            .focus_mode_typewriter_handler_id
            .take()
        {
            self.settings.disconnect(handler_id);
        }
    }
}

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

fn apply_word_wrap(view: &sourceview5::View, settings: &gio::Settings) {
    let mode = if settings.boolean(keys::WORD_WRAP) {
        gtk4::WrapMode::Word
    } else {
        gtk4::WrapMode::None
    };
    view.set_wrap_mode(mode);
}

fn apply_color_scheme(buffer: &sourceview5::Buffer, settings: &gio::Settings) -> Option<String> {
    let scheme_manager = sourceview5::StyleSchemeManager::default();
    let base_scheme = crate::active_sourceview_scheme(settings)?;
    let opacity = settings.double(keys::TAB_CONTENT_OPACITY).clamp(0.0, 1.0);

    if opacity >= 1.0 - f64::EPSILON {
        let applied_id = base_scheme.id().to_string();
        buffer.set_style_scheme(Some(&base_scheme));
        return Some(applied_id);
    }

    let derived_id = ensure_transparency_style_scheme(&base_scheme, settings);
    let scheme = scheme_manager
        .scheme(&derived_id)
        .unwrap_or_else(|| base_scheme.clone());
    let applied_id = scheme.id().to_string();
    buffer.set_style_scheme(Some(&scheme));
    Some(applied_id)
}

/// Create or reuse an opacity-aware child scheme derived from the active base scheme.
fn ensure_transparency_style_scheme(
    base_scheme: &sourceview5::StyleScheme,
    settings: &gio::Settings,
) -> String {
    let palette = crate::resolve_tab_content_palette(settings);
    let manager = sourceview5::StyleSchemeManager::default();
    let base_id = base_scheme.id().to_string();
    let sanitized_base = sanitize_style_scheme_component(&base_id);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "tab-content opacity is clamped to 0..1 before converting to a 0..100 scheme suffix"
    )]
    let opacity_percent = (palette.opacity * 100.0).round() as u32;
    let derived_id = format!("lushtext-opacity-{sanitized_base}-{opacity_percent}");

    if manager.scheme(&derived_id).is_some() {
        return derived_id;
    }

    let scheme_dir = crate::services::json_store::data_dir().join("style-schemes");
    let file_path = scheme_dir.join(format!("{derived_id}.xml"));
    if !file_path.exists() {
        let text_bg = crate::sourceview_rgba_with_alpha(&palette.text_bg, palette.opacity);
        let line_numbers_bg =
            crate::sourceview_rgba_with_alpha(&palette.line_numbers_bg, palette.opacity);
        let current_line_bg =
            crate::sourceview_rgba_with_alpha(&palette.current_line_bg, palette.opacity);
        let current_line_number_bg =
            crate::sourceview_rgba_with_alpha(&palette.current_line_number_bg, palette.opacity);
        let right_margin_bg =
            crate::sourceview_rgba_with_alpha(&palette.right_margin_bg, palette.opacity);
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<style-scheme id="{derived_id}" _name="LushText Transparency" version="1.0" parent-scheme="{base_id}">
  <author>LushText</author>
  <_description>Opacity-aware derived scheme for LushText tab content</_description>
  <style name="text" background="{text_bg}"/>
  <style name="background-pattern" background="{text_bg}"/>
  <style name="current-line" background="{current_line_bg}"/>
  <style name="line-numbers" background="{line_numbers_bg}"/>
  <style name="current-line-number" background="{current_line_number_bg}"/>
  <style name="right-margin" background="{right_margin_bg}"/>
</style-scheme>
"#
        );
        let _ = std::fs::create_dir_all(&scheme_dir);
        let _ = std::fs::write(&file_path, xml);
    }

    let scheme_dir_str = scheme_dir.to_string_lossy();
    if !manager
        .search_path()
        .iter()
        .any(|path| path.as_str() == scheme_dir_str.as_ref())
    {
        manager.prepend_search_path(&scheme_dir_str);
    }
    manager.force_rescan();

    derived_id
}

/// Replace punctuation in runtime-generated style-scheme IDs so the file name stays stable.
fn sanitize_style_scheme_component(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}
