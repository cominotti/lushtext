// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the editor page widget.
//!
//! Each tab in the editor is an `EditorPage` containing a GtkSourceView,
//! a search bar, and per-tab state (file path, size, eviction status).
//! GSettings bindings keep all editor pages in sync with user preferences.

use crate::config::keys;
use crate::model::annotation::{AnnotationId, AnnotationRecord};
use crate::model::bookmark::BookmarkRecord;
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
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// Callback for notifying the window when this editor's estimated buffer
/// memory changes. The `u64` argument is the new estimated byte count.
type MemoryChangedCallback = Box<dyn Fn(u64)>;
type NotificationCallback = Box<dyn Fn(InlineActionNotification)>;
type LoadCompletedCallback = Box<dyn FnOnce()>;
type FileLoadedCallback = Box<dyn Fn()>;
type NotesChangedCallback = Box<dyn Fn()>;

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
    /// Handler ID for GSettings `tab-width` change. Disconnected in `Drop`.
    pub tab_width_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    /// Handler ID for GSettings `insert-spaces` change. Disconnected in `Drop`.
    pub insert_spaces_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    /// Handler ID for GSettings `annotation-highlights-visible`. Disconnected in `Drop`.
    pub annotation_visibility_handler_id: RefCell<Option<glib::SignalHandlerId>>,
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

/// File-load lifecycle callbacks that need to survive repeated reloads.
#[derive(Default)]
pub struct LoadState {
    /// One-shot callback fired after the first successful file load.
    pub load_completed_callback: RefCell<Option<LoadCompletedCallback>>,
    /// Recurring callback fired after every successful file load or reload.
    pub file_loaded_callback: RefCell<Option<FileLoadedCallback>>,
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
}

/// Live annotation projection state scoped to one editor tab.
#[derive(Default)]
pub struct AnnotationState {
    /// Current annotation range anchors projected into the buffer.
    pub entries: RefCell<Vec<LiveAnnotation>>,
    /// Callback invoked when annotation state changes and should be persisted.
    pub changed_callback: RefCell<Option<NotesChangedCallback>>,
    /// Pending annotation ID that should reopen once the file load completes.
    pub pending_focus_id: RefCell<Option<AnnotationId>>,
    /// Whether annotations have been loaded for the current file content.
    pub loaded: Cell<bool>,
    /// Debounced sidecar persistence state for annotation saves.
    pub persistence: NotesPersistenceState,
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
    /// File-load lifecycle callbacks.
    pub load: LoadState,
    /// Deferred cursor/scroll restoration state.
    pub restore: RestoreState,
    /// Live bookmark mark projection and persistence state.
    pub bookmarks: BookmarkState,
    /// Live annotation range projection and persistence state.
    pub annotations: AnnotationState,
}

impl Default for LushtextEditorPage {
    fn default() -> Self {
        Self {
            info_bar: TemplateChild::default(),
            source_view: TemplateChild::default(),
            scrolled_window: TemplateChild::default(),
            search_revealer: TemplateChild::default(),
            search_bar: TemplateChild::default(),
            file_path: RefCell::default(),
            file_size: Cell::default(),
            size_check: Cell::new(FileSizeCheck::Normal),
            evicted: Cell::new(false),
            cancel_token: Arc::new(AtomicBool::new(false)),
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
            load: LoadState::default(),
            restore: RestoreState::default(),
            bookmarks: BookmarkState::default(),
            annotations: AnnotationState::default(),
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
        let id = settings.connect_changed(Some(keys::WORD_WRAP), move |s, _| {
            apply_word_wrap(&view, s);
        });
        self.preference_bindings
            .word_wrap_handler_id
            .replace(Some(id));

        apply_color_scheme(&buffer, settings);
        {
            let buf = buffer.clone();
            let s = settings.clone();
            let id = settings.connect_changed(Some(keys::STYLE_SCHEME), move |_, _| {
                apply_color_scheme(&buf, &s);
            });
            self.preference_bindings
                .style_scheme_handler_id
                .replace(Some(id));
        }
        {
            let buf = buffer.downgrade();
            let editor_weak = self.obj().downgrade();
            let s = settings.clone();
            let style_manager = libadwaita::StyleManager::default();
            let handler_id = style_manager.connect_dark_notify(move |_| {
                if let Some(buf) = buf.upgrade() {
                    apply_color_scheme(&buf, &s);
                }
                if let Some(editor) = editor_weak.upgrade() {
                    editor.refresh_annotation_highlights();
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

        self.obj().setup_bookmark_projection();
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
    }
}

impl WidgetImpl for LushtextEditorPage {}
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

fn apply_color_scheme(buffer: &sourceview5::Buffer, settings: &gio::Settings) {
    let base_id = settings.string(keys::STYLE_SCHEME);
    let style_manager = libadwaita::StyleManager::default();
    let scheme_manager = sourceview5::StyleSchemeManager::default();

    let scheme = if style_manager.is_dark() {
        let dark_id = format!("{base_id}-dark");
        scheme_manager
            .scheme(&dark_id)
            .or_else(|| scheme_manager.scheme(&base_id))
    } else {
        scheme_manager.scheme(&base_id)
    };

    if let Some(scheme) = scheme {
        buffer.set_style_scheme(Some(&scheme));
    }
}
