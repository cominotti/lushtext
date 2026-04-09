// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the main application window.
//!
//! Handles window geometry persistence via GSettings, sidebar clamping,
//! tab lifecycle signals, and command palette integration.

use crate::config::{self, keys};
use crate::model::draft::DraftManifest;
use crate::services::notifications::NotificationBus;
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
    pub sidebar_revealer: TemplateChild<gtk4::Revealer>,
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
    /// Cached paned separator/handle overhead (pixels), refreshed from the
    /// current realized layout budget as `paned_min - sidebar_min - content_min`.
    /// Used in `clamp_sidebar_position` to replace the former hardcoded 16px buffer.
    pub handle_overhead: Cell<i32>,
    /// Window-scoped notification bus + store.
    pub notification_bus: NotificationBus,
    /// Periodic sweep for expiring transient/progress notifications.
    pub notification_sweep_source_id: RefCell<Option<glib::SourceId>>,
    /// Periodic lease renewal for active search progress notifications.
    pub search_progress_heartbeat_source_id: RefCell<Option<glib::SourceId>>,
    /// Generation counter for delayed search progress display.
    pub search_progress_generation: Cell<u32>,
    /// Whether search progress is allowed to render after the initial delay.
    pub search_progress_visible: Cell<bool>,
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
            sidebar_revealer: TemplateChild::default(),
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
            handle_overhead: Cell::new(16),
            notification_bus: NotificationBus::default(),
            notification_sweep_source_id: RefCell::new(None),
            search_progress_heartbeat_source_id: RefCell::new(None),
            search_progress_generation: Cell::new(0),
            search_progress_visible: Cell::new(false),
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

        // --- Compute paned handle overhead and pre-clamp sidebar position ---
        //
        // GTK4's layout cycle runs measure() BEFORE size_allocate(). During
        // measure(), GtkPaned distributes width based on its current `position`,
        // which may be stale from a previous frame or GSettings restore. If the
        // position leaves less than content_box's actual minimum width,
        // GTK warns "Trying to measure GtkBox for width of X, but needs Y".
        //
        // Defense: pre-clamp the restored position against the restored window
        // width so the first layout pass has a valid position. The original
        // unclamped value is preserved in saved_sidebar_pos for the show
        // animation to use when the window is wider.
        let saved_pos = settings.int(keys::SIDEBAR_POSITION);
        update_sidebar_measurements(&obj, &self.content_box);

        // Pre-clamp against the restored default width (best proxy for the
        // first frame). The clamp only reduces, never grows — so if the WM
        // opens at a wider width, the position is still valid.
        let clamped_pos = clamp_sidebar_visible_position(&obj, &self.content_box, w, saved_pos);
        self.main_paned.set_position(clamped_pos);
        self.last_sidebar_pos.set(clamped_pos);
        self.pending_sidebar_pos.set(clamped_pos);
        // Preserve the original unclamped position for show-animation target.
        // When the window is wider than the GSettings width, animate_sidebar
        // will use this value (which clamp_sidebar_position will then validate
        // against the actual allocated width).
        self.saved_sidebar_pos
            .set(saved_pos.max(SIDEBAR_COLLAPSED_POSITION));

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

        // Refresh the live layout budget once the widgets are mapped. GtkPaned's
        // realized handle width can differ from the pre-map measurement by 1px,
        // which is enough to reintroduce the `GtkBox ... needs at least ...`
        // warning during the first toggle on some themes/layouts.
        {
            let window_weak = obj.downgrade();
            obj.connect_map(move |_| {
                if let Some(window) = window_weak.upgrade() {
                    refresh_sidebar_layout_budget(&window);
                }
            });
        }

        // --- Restore sidebar visibility ---
        let sidebar_vis = settings.boolean(keys::SIDEBAR_VISIBLE);
        self.sidebar_visible.set(sidebar_vis);
        self.sidebar_revealer.set_transition_duration(0);
        self.sidebar_revealer.set_visible(sidebar_vis);
        self.sidebar_revealer.set_reveal_child(sidebar_vis);
        if !sidebar_vis {
            // Mirror the post-hide runtime state on startup so the first
            // toggle-on animates from the same collapsed endpoint instead of
            // popping in at the already-restored width.
            self.main_paned.set_position(SIDEBAR_COLLAPSED_POSITION);
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
                            &window.imp().content_box,
                            if paned.width() > 0 {
                                paned.width()
                            } else {
                                window.width()
                            },
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
            glib::Propagation::Stop // Always inhibit — close_page_finish decides
        });

        let window_weak = obj.downgrade();
        self.tab_view.connect_page_detached(move |_, page, _| {
            if let Some(window) = window_weak.upgrade() {
                if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() {
                    if let Some(ref path) = editor.file_path() {
                        window.imp().open_paths.borrow_mut().remove(path.as_path());
                    }
                    window.dismiss_editor_notifications(editor);
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
        if let Some(source_id) = self.notification_sweep_source_id.take() {
            source_id.remove();
        }
        if let Some(source_id) = self.search_progress_heartbeat_source_id.take() {
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
        clamp_sidebar_position(&self.obj(), &self.main_paned, &self.content_box, width);
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

        if modified.is_empty() {
            // No unsaved changes — flush drafts, save session, close.
            self.search_panel.close();
            window.flush_dirty_drafts();
            window.save_session_sync();
            return self.parent_close_request();
        }

        // Show save-changes dialog and inhibit close until the user responds.
        let window_for_close = window.clone();
        window.show_save_changes_dialog(modified, move |confirmed| {
            if confirmed {
                window_for_close.imp().search_panel.close();
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

pub const SIDEBAR_COLLAPSED_POSITION: i32 = 0;

fn sidebar_max_position(
    window: &super::LushtextWindow,
    content_box: &gtk4::Box,
    window_width: i32,
) -> Option<i32> {
    if window_width <= 0 {
        return None;
    }
    let imp = window.imp();
    if imp.main_paned.width() > 0 {
        let allocated_width = imp.main_paned.width();
        let paned_max = imp.main_paned.max_position();
        if paned_max > 0 {
            return Some((allocated_width / 3).min(paned_max).max(0));
        }
    }
    // Query the end child's actual minimum width so the sidebar never squeezes it
    // below that floor. Re-measure the paned handle budget from the live layout
    // instead of trusting construction-time values; restored workspaces can
    // change the realized geometry by 1px, which is enough to trigger GTK's
    // width warning if we keep a stale handle budget.
    let content_min = measure_content_box_min(content_box);
    let handle_overhead = measure_sidebar_handle_overhead(window, content_min);
    let stack_floor = window_width - content_min - handle_overhead;
    Some((window_width / 3).min(stack_floor).max(0))
}

fn measure_content_box_min(content_box: &gtk4::Box) -> i32 {
    let (content_min, _, _, _) = content_box.measure(gtk4::Orientation::Horizontal, -1);
    content_min
}

fn measure_sidebar_handle_overhead(window: &super::LushtextWindow, content_min: i32) -> i32 {
    let imp = window.imp();
    let (paned_min, _, _, _) = imp.main_paned.measure(gtk4::Orientation::Horizontal, -1);
    let (sidebar_min, _, _, _) = imp
        .sidebar_revealer
        .measure(gtk4::Orientation::Horizontal, -1);
    let handle_overhead = (paned_min - sidebar_min - content_min).max(1);
    imp.handle_overhead.set(handle_overhead);
    handle_overhead
}

fn update_sidebar_measurements(window: &super::LushtextWindow, content_box: &gtk4::Box) -> i32 {
    let content_min = measure_content_box_min(content_box);
    if content_min > 0 && content_box.width_request() != content_min {
        content_box.set_width_request(content_min);
    }
    measure_sidebar_handle_overhead(window, content_min)
}

fn sidebar_affects_layout(imp: &LushtextWindow) -> bool {
    imp.sidebar_visible.get() || imp.sidebar_revealer.property::<bool>("visible")
}

pub(super) fn refresh_sidebar_layout_budget(window: &super::LushtextWindow) {
    let imp = window.imp();
    update_sidebar_measurements(window, &imp.content_box);
    // Keep clamping active while the revealer is still participating in layout.
    // The toggle action flips `sidebar_visible` before the hide animation starts,
    // so the cache alone is not a reliable indicator that the sidebar is fully
    // offstage yet.
    if !sidebar_affects_layout(imp) {
        return;
    }
    let budget_width = if imp.main_paned.width() > 0 {
        imp.main_paned.width()
    } else {
        window.width()
    };
    let clamped = clamp_sidebar_visible_position(
        window,
        &imp.content_box,
        budget_width,
        imp.main_paned.position(),
    );
    if clamped != imp.main_paned.position() {
        imp.main_paned.set_position(clamped);
    }
}

/// Clamp a desired *visible* sidebar position before it is written into
/// `GtkPaned`. This is safe for animation targets and animation ticks.
pub fn clamp_sidebar_visible_position(
    window: &super::LushtextWindow,
    content_box: &gtk4::Box,
    window_width: i32,
    desired: i32,
) -> i32 {
    match sidebar_max_position(window, content_box, window_width) {
        Some(max) => desired.min(max).max(SIDEBAR_COLLAPSED_POSITION),
        None => desired.max(SIDEBAR_COLLAPSED_POSITION),
    }
}

/// Clamp the sidebar pane position to at most 1/3 of the window width
/// and ensure the end child (content stack) keeps at least its minimum width.
/// Uses a generation-counter debounce so resize-time clamping stays immediate
/// while D-Bus-backed persistence only happens once resizing settles.
pub fn clamp_sidebar_position(
    window: &super::LushtextWindow,
    paned: &gtk4::Paned,
    content_box: &gtk4::Box,
    window_width: i32,
) {
    let imp = window.imp();
    if !sidebar_affects_layout(imp) {
        return;
    }
    let Some(max) = sidebar_max_position(window, content_box, window_width) else {
        return;
    };
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
