// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the editor page widget.
//!
//! Each tab in the editor is an `EditorPage` containing a GtkSourceView,
//! a search bar, and per-tab state (file path, size, eviction status).
//! GSettings bindings keep all editor pages in sync with user preferences.

use crate::config::keys;
use crate::model::formatting_overrides::FormattingOverrides;
use crate::services::file_limits::FileSizeCheck;
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
    /// Per-file formatting overrides from EditorConfig. Empty for untitled tabs
    /// or files without a matching `.editorconfig`.
    pub formatting_overrides: Cell<FormattingOverrides>,
    /// Handler ID for the buffer's `modified-changed` signal. Disconnected in `Drop`.
    pub modified_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    /// Callback invoked when estimated buffer memory changes (load, save, evict).
    pub memory_changed_callback: RefCell<Option<MemoryChangedCallback>>,
    /// File monitor for detecting external modifications. Created on file load,
    /// cancelled on tab close.
    pub file_monitor: RefCell<Option<gio::FileMonitor>>,
    /// Generation counter for debouncing file monitor events (500ms).
    /// Incremented on each monitor event; stale timer callbacks no-op.
    pub monitor_generation: Cell<u32>,
    /// File mtime (seconds since epoch) at last load or save. Compared with
    /// current mtime to distinguish real changes from noise.
    pub last_known_mtime: Cell<Option<u64>>,
    /// Whether the buffer has been modified since the last draft save.
    /// Checked by the global autosave timer to decide which tabs need drafting.
    pub draft_dirty: Cell<bool>,
    /// Draft ID for this tab. For path-backed files this is a hash of the path;
    /// for untitled tabs it's a generated unique ID. Stable across the tab's lifetime.
    pub draft_id: RefCell<Option<String>>,
    /// Whether this tab is currently showing draft-restored content.
    pub draft_restored: Cell<bool>,
    /// One-shot callback fired after the first successful file load.
    /// Used by the window to defer draft recovery until file content is
    /// ready, avoiding the race where `load_file_async` overwrites draft
    /// content. Consumed on first call (FnOnce).
    pub load_completed_callback: RefCell<Option<Box<dyn FnOnce()>>>,
    /// Deferred cursor line to apply after async file load completes.
    pub restore_cursor_line: Cell<Option<u32>>,
    /// Deferred cursor column to apply after async file load completes.
    pub restore_cursor_col: Cell<Option<u32>>,
    /// Deferred scroll-to line to apply after async file load completes.
    pub restore_scroll_line: Cell<Option<u32>>,
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
            dark_handler_id: RefCell::new(None),
            word_wrap_handler_id: RefCell::new(None),
            style_scheme_handler_id: RefCell::new(None),
            tab_width_handler_id: RefCell::new(None),
            insert_spaces_handler_id: RefCell::new(None),
            formatting_overrides: Cell::new(FormattingOverrides::default()),
            modified_handler_id: RefCell::new(None),
            memory_changed_callback: RefCell::default(),
            file_monitor: RefCell::new(None),
            monitor_generation: Cell::new(0),
            last_known_mtime: Cell::new(None),
            draft_dirty: Cell::new(false),
            draft_id: RefCell::new(None),
            draft_restored: Cell::new(false),
            load_completed_callback: RefCell::default(),
            restore_cursor_line: Cell::new(None),
            restore_cursor_col: Cell::new(None),
            restore_scroll_line: Cell::new(None),
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
        if let Some(handler_id) = self.modified_handler_id.take() {
            let buffer = self
                .source_view
                .buffer()
                .downcast::<sourceview5::Buffer>()
                .expect("GtkSourceView buffer");
            buffer.disconnect(handler_id);
        }
        // Cancel file monitor to stop receiving events for this tab.
        if let Some(monitor) = self.file_monitor.take() {
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
            (keys::TAB_WIDTH, &self.tab_width_handler_id),
            (keys::INSERT_SPACES, &self.insert_spaces_handler_id),
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
        self.word_wrap_handler_id.replace(Some(id));

        apply_color_scheme(&buffer, settings);
        {
            let buf = buffer.clone();
            let s = settings.clone();
            let id = settings.connect_changed(Some(keys::STYLE_SCHEME), move |_, _| {
                apply_color_scheme(&buf, &s);
            });
            self.style_scheme_handler_id.replace(Some(id));
        }
        {
            let buf = buffer.downgrade();
            let s = settings.clone();
            let style_manager = libadwaita::StyleManager::default();
            let handler_id = style_manager.connect_dark_notify(move |_| {
                if let Some(buf) = buf.upgrade() {
                    apply_color_scheme(&buf, &s);
                }
            });
            self.dark_handler_id.replace(Some(handler_id));
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
        if let Some(handler_id) = self.dark_handler_id.take() {
            libadwaita::StyleManager::default().disconnect(handler_id);
        }
        if let Some(handler_id) = self.word_wrap_handler_id.take() {
            self.settings.disconnect(handler_id);
        }
        if let Some(handler_id) = self.style_scheme_handler_id.take() {
            self.settings.disconnect(handler_id);
        }
        if let Some(handler_id) = self.tab_width_handler_id.take() {
            self.settings.disconnect(handler_id);
        }
        if let Some(handler_id) = self.insert_spaces_handler_id.take() {
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
