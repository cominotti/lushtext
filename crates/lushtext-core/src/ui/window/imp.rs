// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the main application window.
//!
//! Handles window geometry persistence via GSettings, sidebar clamping,
//! tab lifecycle signals, and command palette integration.

use crate::config::{self, keys};
use crate::model::draft::DraftManifest;
use crate::ui::command_palette::LushtextCommandPalette;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::markdown_preview::LushtextMarkdownPreview;
use crate::ui::search_panel::LushtextSearchPanel;
use crate::ui::sidebar::LushtextSidebar;
use crate::ui::status_bar::{LushtextStatusBar, MessageKind};
use glib::prelude::*;
use gtk4::prelude::*;
use gtk4::{self, CompositeTemplate, gio, glib};
use libadwaita::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

// CompositeTemplate loads the UI layout from a compiled XML file (bundled
// as a GResource at build time). Each #[template_child] field is auto-bound
// to the widget with the matching `id` attribute in the XML.
//
// GObject methods always take &self because multiple widgets can hold
// references to the same window at once. To store mutable state, we use
// Cell<T> for Copy types (generation counters, positions) and RefCell<T>
// for complex types (HashSet, HashMap). Cell has no borrow overhead;
// RefCell panics on overlapping borrows.
#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/window.ui")]
pub struct LushtextWindow {
    #[template_child]
    pub header_bar: TemplateChild<libadwaita::HeaderBar>,
    #[template_child]
    pub title_widget: TemplateChild<libadwaita::WindowTitle>,
    #[template_child]
    pub tab_bar: TemplateChild<libadwaita::TabBar>,
    #[template_child]
    pub tab_view: TemplateChild<libadwaita::TabView>,
    #[template_child]
    pub content_stack: TemplateChild<gtk4::Stack>,
    #[template_child]
    pub main_paned: TemplateChild<gtk4::Paned>,
    #[template_child]
    pub sidebar: TemplateChild<LushtextSidebar>,
    #[template_child]
    pub status_bar: TemplateChild<LushtextStatusBar>,
    #[template_child]
    pub palette_revealer: TemplateChild<gtk4::Revealer>,
    #[template_child]
    pub command_palette: TemplateChild<LushtextCommandPalette>,
    #[template_child]
    pub primary_menu_button: TemplateChild<gtk4::MenuButton>,
    #[template_child]
    pub preview_paned: TemplateChild<gtk4::Paned>,
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

    /// Application-wide GSettings for window geometry and sidebar position.
    pub settings: gio::Settings,
    /// Cached sidebar visibility for the `clamp_sidebar_position` hot path.
    /// Avoids a GObject property lookup (~60Hz during resize).
    pub sidebar_visible: Cell<bool>,
    /// Sidebar position saved before hide animation, restored on show.
    pub saved_sidebar_pos: Cell<i32>,
    /// Currently running sidebar show/hide animation, if any.
    /// Paused on rapid toggle so the new animation can start from the current position.
    pub sidebar_animation: RefCell<Option<libadwaita::TimedAnimation>>,
    /// Whether the side-by-side preview pane is currently visible.
    pub preview_visible: Cell<bool>,
    /// Whether the preview-only mode (Alt+P) is active (editor hidden, preview full-width).
    pub preview_mode: Cell<bool>,
    /// Preview pane position saved before hide animation, restored on show.
    pub saved_preview_pos: Cell<i32>,
    /// Currently running preview show/hide animation, if any.
    pub preview_animation: RefCell<Option<libadwaita::TimedAnimation>>,
    /// Generation counter for debouncing preview renders (300ms).
    pub preview_render_generation: Cell<u32>,
    /// Last preview pane position persisted to GSettings.
    pub last_preview_pos: Cell<i32>,
    /// Preview pane position pending GSettings persistence.
    pub pending_preview_pos: Cell<i32>,
    /// Generation counter for debouncing preview position GSettings writes (200ms).
    pub preview_persist_generation: Cell<u32>,
    /// Generation counter for debouncing file index rebuilds (300ms).
    /// Incremented on each workspace change; stale timer callbacks no-op.
    pub index_rebuild_generation: Cell<u32>,
    /// Focus widget saved before the command palette steals focus.
    /// `WeakRef` avoids preventing widget finalization if the tab closes
    /// while the palette is open. Consumed by `restore_saved_focus()`.
    pub saved_focus: RefCell<Option<glib::WeakRef<gtk4::Widget>>>,
    /// Last sidebar position persisted to GSettings. Compared against pending
    /// to skip redundant D-Bus writes during rapid resize events.
    pub last_sidebar_pos: Cell<i32>,
    /// Sidebar position that will be persisted after the debounce settles.
    pub pending_sidebar_pos: Cell<i32>,
    /// Generation counter for debouncing sidebar position GSettings writes (200ms).
    pub sidebar_persist_generation: Cell<u32>,
    /// Set of file paths with open tabs, for O(1) duplicate detection in `open_document`.
    pub open_paths: RefCell<HashSet<PathBuf>>,
    /// Running total of estimated buffer memory across all tabs (bytes).
    /// Compared against `BUFFER_MEMORY_BUDGET` to trigger eviction.
    pub buffer_memory_total: Cell<u64>,
    /// Per-editor estimated buffer memory, keyed by `editor.as_ptr() as usize`
    /// for stable identity without preventing widget finalization.
    pub buffer_memory_by_editor: RefCell<HashMap<usize, u64>>,
    /// Source ID for the global 5-second autosave timer. Removed on dispose.
    pub autosave_source_id: RefCell<Option<glib::SourceId>>,
    /// In-memory copy of the draft manifest, kept in sync with disk.
    pub draft_manifest: RefCell<DraftManifest>,
    /// Draft content pre-loaded during session restore (draft_id → text).
    /// Populated in `load_session_and_drafts`, consumed by `check_draft_on_open`
    /// and `check_draft_by_id` to avoid a per-tab background thread hop.
    pub preloaded_drafts: RefCell<HashMap<String, String>>,
    /// Monotonic counter for generating unique IDs for untitled tab drafts.
    pub next_tab_id: Cell<u64>,
    /// Generation counter for debouncing session saves (500ms).
    pub session_save_generation: Cell<u32>,
    /// Guard flag: true while restoring a session from disk.
    /// Prevents `save_session_debounced` from firing during restore.
    pub restoring_session: Cell<bool>,
    /// Focus widget saved before the search panel steals focus.
    /// Separate from `saved_focus` (command palette) so both overlays
    /// can independently save/restore focus.
    pub search_saved_focus: RefCell<Option<glib::WeakRef<gtk4::Widget>>>,
}

impl Default for LushtextWindow {
    fn default() -> Self {
        Self {
            header_bar: TemplateChild::default(),
            title_widget: TemplateChild::default(),
            tab_bar: TemplateChild::default(),
            tab_view: TemplateChild::default(),
            content_stack: TemplateChild::default(),
            main_paned: TemplateChild::default(),
            sidebar: TemplateChild::default(),
            status_bar: TemplateChild::default(),
            palette_revealer: TemplateChild::default(),
            command_palette: TemplateChild::default(),
            primary_menu_button: TemplateChild::default(),
            preview_paned: TemplateChild::default(),
            editor_box: TemplateChild::default(),
            markdown_preview: TemplateChild::default(),
            content_box: TemplateChild::default(),
            search_panel_revealer: TemplateChild::default(),
            search_panel: TemplateChild::default(),
            settings: gio::Settings::new(config::APP_ID),
            sidebar_visible: Cell::new(true),
            saved_sidebar_pos: Cell::new(0),
            sidebar_animation: RefCell::new(None),
            preview_visible: Cell::new(false),
            preview_mode: Cell::new(false),
            saved_preview_pos: Cell::new(0),
            preview_animation: RefCell::new(None),
            preview_render_generation: Cell::new(0),
            last_preview_pos: Cell::new(-1),
            pending_preview_pos: Cell::new(-1),
            preview_persist_generation: Cell::new(0),
            index_rebuild_generation: Cell::new(0),
            saved_focus: RefCell::new(None),
            last_sidebar_pos: Cell::new(-1),
            pending_sidebar_pos: Cell::new(-1),
            sidebar_persist_generation: Cell::new(0),
            open_paths: RefCell::new(HashSet::new()),
            buffer_memory_total: Cell::new(0),
            buffer_memory_by_editor: RefCell::new(HashMap::new()),
            autosave_source_id: RefCell::new(None),
            draft_manifest: RefCell::new(DraftManifest::default()),
            preloaded_drafts: RefCell::new(HashMap::new()),
            next_tab_id: Cell::new(0),
            session_save_generation: Cell::new(0),
            restoring_session: Cell::new(false),
            search_saved_focus: RefCell::new(None),
        }
    }
}

// ObjectSubclass registers this struct with GLib's runtime type system.
// NAME must match the `class` attribute in the UI template XML.
// ParentType sets which Adwaita/GTK widget we extend.
#[glib::object_subclass]
impl ObjectSubclass for LushtextWindow {
    const NAME: &'static str = "LushtextWindow";
    type Type = super::LushtextWindow;
    type ParentType = libadwaita::ApplicationWindow;

    fn class_init(klass: &mut Self::Class) {
        // Register custom widget types BEFORE the template is parsed.
        // GTK needs to know about these types when it encounters them in
        // the UI XML — without ensure_type(), template parsing fails.
        LushtextSidebar::ensure_type();
        LushtextEditorPage::ensure_type();
        LushtextStatusBar::ensure_type();
        LushtextCommandPalette::ensure_type();
        LushtextMarkdownPreview::ensure_type();
        LushtextSearchPanel::ensure_type();

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

        // --- Restore window geometry from GSettings ---
        let w = settings.int(keys::WINDOW_WIDTH);
        let h = settings.int(keys::WINDOW_HEIGHT);
        obj.set_default_size(w, h);
        if settings.boolean(keys::WINDOW_MAXIMIZED) {
            obj.maximize();
        }

        // --- Restore sidebar position ---
        let saved_pos = settings.int(keys::SIDEBAR_POSITION);
        self.main_paned.set_position(saved_pos);
        self.last_sidebar_pos.set(saved_pos);
        self.pending_sidebar_pos.set(saved_pos);

        // --- Persist window geometry incrementally via notify signals ---
        // connect_notify_local (not connect_notify) because the closure captures
        // GTK widgets that are not thread-safe. The _local variant guarantees
        // main-thread execution only.
        // (Sidebar clamping is handled in size_allocate, not here.)
        {
            let settings = settings.clone();
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

        // --- Restore sidebar visibility ---
        let sidebar_vis = settings.boolean(keys::SIDEBAR_VISIBLE);
        self.sidebar_visible.set(sidebar_vis);
        if !sidebar_vis {
            // Save the restored position so the first show animation can slide to it.
            // set_visible(false) makes the paned ignore the start child entirely,
            // giving all space to the editor. Position value stays as-is but has
            // no visual effect while the sidebar is invisible.
            self.saved_sidebar_pos.set(self.main_paned.position());
            self.sidebar.set_visible(false);
        }

        // --- Restore preview pane position ---
        // Preview starts hidden (default), but we restore the saved position
        // so the first show animation can slide to it.
        let saved_preview_pos = settings.int(keys::PREVIEW_PANE_POSITION);
        self.saved_preview_pos.set(saved_preview_pos);
        self.last_preview_pos.set(saved_preview_pos);
        self.pending_preview_pos.set(saved_preview_pos);

        // --- Preview pane position clamp on user drag ---
        {
            let window_weak = obj.downgrade();
            self.preview_paned
                .connect_notify_local(Some("position"), move |_paned, _| {
                    if let Some(window) = window_weak.upgrade() {
                        window.clamp_preview_position(window.width());
                    }
                });
        }

        // --- EditorConfig toggle ---
        {
            let window_weak = obj.downgrade();
            settings.connect_changed(Some(keys::USE_EDITORCONFIG), move |s, _| {
                if let Some(window) = window_weak.upgrade() {
                    window.on_use_editorconfig_changed(s.boolean(keys::USE_EDITORCONFIG));
                }
            });
        }

        // --- Sidebar position persist on user drag ---
        {
            let window_weak = obj.downgrade();
            self.main_paned
                .connect_notify_local(Some("position"), move |paned, _| {
                    if let Some(window) = window_weak.upgrade() {
                        clamp_sidebar_position(
                            &window,
                            paned,
                            &window.imp().content_stack,
                            window.width(),
                        );
                    }
                });
        }

        // --- Sidebar file activation ---
        let window_weak = obj.downgrade();
        self.sidebar.connect_file_activated(move |path| {
            if let Some(window) = window_weak.upgrade() {
                window.open_document(path);
            }
        });

        // --- Sidebar rename/delete notifications ---
        let window_weak = obj.downgrade();
        self.sidebar
            .connect_file_renamed(move |old_path, new_path| {
                if let Some(window) = window_weak.upgrade() {
                    window.update_tab_path(old_path, new_path);
                    window
                        .imp()
                        .command_palette
                        .update_index_file_renamed(old_path, new_path);
                    let name = new_path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    window
                        .imp()
                        .status_bar
                        .push_message(&format!("Renamed to {name}"), MessageKind::Info);
                }
            });

        let window_weak = obj.downgrade();
        self.sidebar.connect_file_deleted(move |path| {
            if let Some(window) = window_weak.upgrade() {
                window.close_tab_for_path(path);
                window.imp().command_palette.update_index_file_deleted(path);
                window
                    .imp()
                    .status_bar
                    .push_message("Deleted", MessageKind::Info);
            }
        });

        let window_weak = obj.downgrade();
        self.sidebar.connect_file_created(move |path| {
            if let Some(window) = window_weak.upgrade() {
                window.open_document(path);
                window.imp().command_palette.update_index_file_created(path);
            }
        });

        // --- Command palette callbacks ---
        let window_weak = obj.downgrade();
        self.command_palette.connect_item_activated(move |item| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            if item.is_file() {
                if let Some(path) = item.file_path() {
                    window.open_document(&path);
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
                window.close_command_palette();
            }
        });

        // --- Sidebar workspace change → rebuild file index ---
        // NOTE: The callback is registered in search::setup_search_panel() which
        // also needs to forward workspace roots to the search panel. Since the
        // sidebar uses a single-slot callback, both operations are combined there.
        // Do NOT register a separate callback here — it would be overwritten.

        // --- Tab change signals ---
        let window_weak = obj.downgrade();
        self.tab_view
            .connect_notify_local(Some("n-pages"), move |_, _| {
                if let Some(window) = window_weak.upgrade() {
                    window.update_content_stack();
                }
            });

        let window_weak = obj.downgrade();
        self.tab_view
            .connect_notify_local(Some("selected-page"), move |_, _| {
                if let Some(window) = window_weak.upgrade() {
                    window.refresh_status_bar();
                    window.reload_if_evicted();
                    window.maybe_evict_background_tabs();
                    window.save_session_debounced();
                    window.refresh_preview();
                }
            });

        // --- Tab close confirmation ---
        // Intercept tab close to show "Save Changes?" dialog for modified tabs.
        // Returning true inhibits the close; close_page_finish() is called
        // after the user confirms or cancels.
        let window_weak = obj.downgrade();
        self.tab_view.connect_close_page(move |tab_view, page| {
            let child = page.child();
            let Some(editor) = child.downcast_ref::<LushtextEditorPage>() else {
                tab_view.close_page_finish(page, true);
                return glib::Propagation::Stop;
            };
            if !editor.is_modified() {
                tab_view.close_page_finish(page, true);
                return glib::Propagation::Stop;
            }
            // Show save-changes dialog; close_page_finish is called in the callback.
            if let Some(window) = window_weak.upgrade() {
                let tab_view_weak = tab_view.downgrade();
                let page_weak = page.downgrade();
                window.confirm_close_tab(page, editor, move |confirmed| {
                    if let Some(tab_view) = tab_view_weak.upgrade()
                        && let Some(page) = page_weak.upgrade()
                    {
                        tab_view.close_page_finish(&page, confirmed);
                    }
                });
            }
            glib::Propagation::Stop // Always inhibit — close_page_finish decides
        });

        let window_weak = obj.downgrade();
        self.tab_view.connect_page_detached(move |_, page, _| {
            if let Some(window) = window_weak.upgrade() {
                if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                    if let Some(ref path) = editor.file_path() {
                        window.imp().open_paths.borrow_mut().remove(path.as_path());
                    }
                    window.untrack_editor_memory(editor);
                    editor.cancel_load();
                    editor.stop_file_monitor();
                }
                window.update_content_stack();
                window.refresh_status_bar();
                window.save_session_debounced();
            }
        });

        // Start with empty state
        obj.update_content_stack();

        // Load workspaces from disk asynchronously; the completion callback
        // triggers notify_workspace_changed which rebuilds the file index.
        self.sidebar.load_workspaces();

        // Load draft manifest + session, then restore tabs. Combined so the
        // manifest is ready before restored tabs call check_draft_on_open.
        obj.load_session_and_drafts();
        obj.start_autosave_timer();
    }

    fn dispose(&self) {
        // Cancel the autosave timer to stop ticking after window close.
        if let Some(source_id) = self.autosave_source_id.take() {
            source_id.remove();
        }
    }
}

impl WidgetImpl for LushtextWindow {
    // NOTE: No measure() override here — intentional.
    //
    // clamp_sidebar_position mutates paned.set_position(), which is a
    // side effect. GTK calls measure() speculatively with various for_size
    // values, including the *minimum* window width (640px). Calling clamp
    // from measure permanently ratchets the sidebar position down to the
    // minimum-width constraint (~209px), and size_allocate at the real width
    // (1200px) cannot restore it because the clamp only reduces, never grows.

    fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
        // Clamp sidebar BEFORE the parent allocates — this is the definitive
        // width, free from stale-value timing issues. Running before
        // parent_size_allocate ensures the paned position is already correct
        // when GTK measures the content stack, preventing "needs at least N"
        // measurement warnings.
        clamp_sidebar_position(&self.obj(), &self.main_paned, &self.content_stack, width);
        // Clamp preview pane symmetrically (max 1/3 of window from right).
        self.obj().clamp_preview_position(width);
        // Clamp search panel results height (max 1/3 of window height).
        if height > 0 {
            self.search_panel.clamp_results_height(height / 3);
        }
        self.parent_size_allocate(width, height, baseline);
        // Keep palette at 60% window width for readability.
        // Guarded with width_request comparison to avoid triggering a
        // re-layout on every allocation.
        if width > 0 {
            let palette_width = width * 6 / 10;
            if self.command_palette.width_request() != palette_width {
                self.command_palette.set_width_request(palette_width);
            }
        }
    }
}

impl WindowImpl for LushtextWindow {
    fn close_request(&self) -> glib::Propagation {
        let window = self.obj().clone();
        let modified = window.modified_editors();

        // Clear undo backup on exit (AC #10: undo backup lifecycle).
        self.search_panel.close();

        if modified.is_empty() {
            // No unsaved changes — flush drafts, save session, close.
            window.flush_dirty_drafts();
            window.save_session_sync();
            return self.parent_close_request();
        }

        // Show save-changes dialog and inhibit close until the user responds.
        let window_for_close = window.clone();
        window.show_save_changes_dialog(modified, move |confirmed| {
            if confirmed {
                window_for_close.flush_dirty_drafts();
                window_for_close.save_session_sync();
                window_for_close.destroy();
            }
            // If !confirmed (cancel), the window stays open.
        });
        glib::Propagation::Stop
    }
}
impl ApplicationWindowImpl for LushtextWindow {}
impl AdwApplicationWindowImpl for LushtextWindow {}

/// Clamp the sidebar pane position to at most 1/3 of the window width
/// and ensure the end child (content stack) keeps at least its minimum width.
/// Uses a generation-counter debounce so resize-time clamping stays immediate
/// while D-Bus-backed persistence only happens once resizing settles.
pub fn clamp_sidebar_position(
    window: &super::LushtextWindow,
    paned: &gtk4::Paned,
    content_stack: &gtk4::Stack,
    window_width: i32,
) {
    if window_width <= 0 {
        return;
    }
    if !window.imp().sidebar_visible.get() {
        return;
    }
    let imp = window.imp();
    // Query the stack's minimum width so the sidebar never squeezes it
    // below that floor. 16px buffer covers the GtkPaned handle/separator
    // (Adwaita CSS sets it to 1px, but we leave margin for theme variance).
    let (stack_min, _, _, _) = content_stack.measure(gtk4::Orientation::Horizontal, -1);
    let stack_floor = window_width - stack_min - 16;
    let max = (window_width / 3).min(stack_floor);
    let current = paned.position();
    let clamped = current.min(max).max(0);
    if clamped != current {
        paned.set_position(clamped);
    }
    let final_pos = paned.position();
    imp.pending_sidebar_pos.set(final_pos);
    if imp.last_sidebar_pos.get() == final_pos {
        return;
    }

    let generation = imp.sidebar_persist_generation.get().wrapping_add(1);
    imp.sidebar_persist_generation.set(generation);

    let window_weak = window.downgrade();
    // 200ms debounce: coalesces resize events into one GSettings write.
    // size_allocate fires every frame (~60Hz) during resize, and each
    // set_int triggers a D-Bus round-trip.
    glib::timeout_add_local_once(std::time::Duration::from_millis(200), move || {
        let Some(window) = window_weak.upgrade() else {
            return;
        };
        let imp = window.imp();
        if imp.sidebar_persist_generation.get() != generation {
            return;
        }

        let final_pos = imp.pending_sidebar_pos.get();
        if imp.last_sidebar_pos.get() == final_pos {
            return;
        }
        if imp
            .settings
            .set_int(keys::SIDEBAR_POSITION, final_pos)
            .is_ok()
        {
            imp.last_sidebar_pos.set(final_pos);
        }
    });
}
