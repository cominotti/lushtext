// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the main application window.
//!
//! This module owns the composite-template wiring, long-lived window state,
//! split-view persistence, and the callback glue that binds the sidebar,
//! command palette, session restore, and notifications into one shell.

use crate::config::{self, keys};
use crate::model::draft::{DraftManifest, PreloadedDraftRestore};
use crate::services::notifications::NotificationBus;
use crate::ui::command_palette::LushtextCommandPalette;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::markdown_preview::LushtextMarkdownPreview;
use crate::ui::properties_panel::LushtextPropertiesPanel;
use crate::ui::search_panel::LushtextSearchPanel;
use crate::ui::sidebar::{LushtextSidebar, WorkspaceSidebarWidthPreset};
use crate::ui::status_bar::{LushtextStatusBar, MessageKind};
use glib::prelude::*;
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
const PROPERTIES_SIDEBAR_MIN_WIDTH_SP: f64 = 260.0;
/// Target total-window width for the visible right properties pane.
const FIXED_PROPERTIES_SIDEBAR_FRACTION: f64 = 0.25;
/// Minimum center-column width that keeps restored-document info bars stable
/// once their titles and actions are allowed to wrap on narrow windows.
const MIN_EDITOR_CONTENT_WIDTH_SP: f64 = 620.0;
/// Extra width budget for split separators, padding, and rounding noise that
/// the raw `25% / 50% / 25%` fractions do not capture near the breakpoint.
const DUAL_PANE_LAYOUT_OVERHEAD_SP: f64 = 32.0;
/// Collapse the left workspace pane on narrower windows.
const WORKSPACE_BREAKPOINT_MAX_WIDTH_SP: &str = "max-width: 860sp";

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
    /// Generation counter for delayed search-progress display.
    pub generation: Cell<u32>,
    /// Whether search progress is allowed to render after the initial delay.
    pub visible: Cell<bool>,
}

/// Session-persistence state for the main window shell.
#[derive(Default)]
pub struct SessionState {
    /// Generation counter for debouncing session saves (500ms).
    pub save_generation: Cell<u32>,
    /// Guard flag while restoring session state from disk.
    pub restoring: Cell<bool>,
}

/// Draft lifecycle state owned by the main window shell.
#[derive(Default)]
pub struct DraftState {
    /// Source ID for the global autosave timer. Removed on dispose.
    pub autosave_source_id: RefCell<Option<glib::SourceId>>,
    /// In-memory draft manifest kept in sync with disk.
    pub manifest: RefCell<DraftManifest>,
    /// Draft restore outcomes preloaded during session restore and consumed once.
    pub preloaded: RefCell<HashMap<String, PreloadedDraftRestore>>,
    /// Monotonic counter for generating unique IDs for untitled tab drafts.
    pub next_tab_id: Cell<u64>,
    /// Draft IDs explicitly discarded during an in-progress close flow.
    /// These must not be re-written by `flush_dirty_drafts()` right before the
    /// window is destroyed.
    pub close_discard_ids: RefCell<HashSet<String>>,
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
    pub workspace_split_view: TemplateChild<libadwaita::OverlaySplitView>,
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

    /// Application-wide settings for geometry, sidebar layout, and editor behavior.
    pub settings: gio::Settings,
    /// Cached workspace sidebar visibility for action-state synchronization and tests.
    pub sidebar_visible: Cell<bool>,
    /// Cached properties sidebar visibility for action-state synchronization and tests.
    pub properties_sidebar_visible: Cell<bool>,
    /// Whether the side-by-side preview pane is currently visible.
    pub preview_visible: Cell<bool>,
    /// Whether the preview-only mode (Alt+P) is active (editor hidden, preview full-width).
    pub preview_mode: Cell<bool>,
    /// Preview pane position saved before hide animation, restored on show.
    pub saved_preview_pos: Cell<i32>,
    /// Currently running preview show/hide animation, if any.
    pub preview_animation: RefCell<Option<libadwaita::TimedAnimation>>,
    /// True while a programmatic preview animation is moving the paned divider.
    pub preview_animation_active: Cell<bool>,
    /// Generation counter for debouncing preview renders (300ms).
    pub preview_render_generation: Cell<u32>,
    /// Last preview pane position persisted to GSettings.
    pub last_preview_pos: Cell<i32>,
    /// Preview pane position pending GSettings persistence.
    pub pending_preview_pos: Cell<i32>,
    /// Generation counter for debouncing preview position GSettings writes (200ms).
    pub preview_persist_generation: Cell<u32>,
    /// Generation counter for debouncing file index rebuilds (300ms).
    pub index_rebuild_generation: Cell<u32>,
    /// Focus widget saved before the command palette steals focus.
    pub saved_focus: RefCell<Option<glib::WeakRef<gtk4::Widget>>>,
    /// Set of file paths with open tabs, for O(1) duplicate detection in `open_document`.
    pub open_paths: RefCell<HashSet<PathBuf>>,
    /// Editor-memory accounting used by the eviction helpers.
    pub editor_memory: EditorMemoryState,
    /// Session save/restore state.
    pub session: SessionState,
    /// Draft persistence and autosave state.
    pub drafts: DraftState,
    /// Tab-menu targeting, pinned-page wiring, and bulk-close authorization.
    pub tab_management: TabManagementState,
    /// Focus widget saved before the search panel steals focus.
    pub search_saved_focus: RefCell<Option<glib::WeakRef<gtk4::Widget>>>,
    /// Window-scoped notification bus + store.
    pub notification_bus: NotificationBus,
    /// Periodic sweep for expiring transient and progress notifications.
    pub notification_sweep_source_id: RefCell<Option<glib::SourceId>>,
    /// Search-progress lease state used by the status-bar notification flow.
    pub search_progress: SearchProgressState,
    /// Stored so the properties breakpoint condition can track the selected
    /// workspace preset and whether the left pane currently consumes width.
    pub properties_breakpoint: RefCell<Option<libadwaita::Breakpoint>>,
}

impl Default for LushtextWindow {
    fn default() -> Self {
        Self {
            header_bar: TemplateChild::default(),
            title_widget: TemplateChild::default(),
            tab_bar: TemplateChild::default(),
            workspace_split_view: TemplateChild::default(),
            properties_split_view: TemplateChild::default(),
            tab_view: TemplateChild::default(),
            content_stack: TemplateChild::default(),
            sidebar: TemplateChild::default(),
            properties_panel: TemplateChild::default(),
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
            properties_sidebar_visible: Cell::new(false),
            preview_visible: Cell::new(false),
            preview_mode: Cell::new(false),
            saved_preview_pos: Cell::new(0),
            preview_animation: RefCell::new(None),
            preview_animation_active: Cell::new(false),
            preview_render_generation: Cell::new(0),
            last_preview_pos: Cell::new(-1),
            pending_preview_pos: Cell::new(-1),
            preview_persist_generation: Cell::new(0),
            index_rebuild_generation: Cell::new(0),
            saved_focus: RefCell::new(None),
            open_paths: RefCell::new(HashSet::new()),
            editor_memory: EditorMemoryState::default(),
            session: SessionState::default(),
            drafts: DraftState::default(),
            tab_management: TabManagementState::default(),
            search_saved_focus: RefCell::new(None),
            notification_bus: NotificationBus::default(),
            notification_sweep_source_id: RefCell::new(None),
            search_progress: SearchProgressState::default(),
            properties_breakpoint: RefCell::new(None),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextWindow {
    const NAME: &'static str = "LushtextWindow";
    type Type = super::LushtextWindow;
    type ParentType = libadwaita::ApplicationWindow;

    fn class_init(klass: &mut Self::Class) {
        LushtextSidebar::ensure_type();
        LushtextEditorPage::ensure_type();
        LushtextStatusBar::ensure_type();
        LushtextCommandPalette::ensure_type();
        LushtextMarkdownPreview::ensure_type();
        LushtextPropertiesPanel::ensure_type();
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

        let w = settings.int(keys::WINDOW_WIDTH);
        let h = settings.int(keys::WINDOW_HEIGHT);
        obj.set_default_size(w, h);
        if settings.boolean(keys::WINDOW_MAXIMIZED) {
            obj.maximize();
        }

        configure_split_views(&self.workspace_split_view, &self.properties_split_view);
        migrate_split_view_settings(settings, w);
        install_split_view_breakpoints(&obj);
        restore_workspace_split_view(&obj);
        restore_properties_split_view(&obj);

        // Restore preview pane position even though the pane starts hidden so
        // the first reveal animation still targets the user's preferred width.
        let saved_preview_pos = settings.int(keys::PREVIEW_PANE_POSITION);
        self.saved_preview_pos.set(saved_preview_pos);
        self.last_preview_pos.set(saved_preview_pos);
        self.pending_preview_pos.set(saved_preview_pos);

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

        {
            let window_weak = obj.downgrade();
            self.preview_paned
                .connect_notify_local(Some("position"), move |_paned, _| {
                    if let Some(window) = window_weak.upgrade() {
                        if window.imp().preview_animation_active.get() {
                            return;
                        }
                        window.clamp_preview_position(window.width());
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
                        return;
                    }
                    let _ = window.imp().settings.set_double(
                        keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION,
                        desired_properties_fraction(width),
                    );
                },
            );
        }

        {
            let window_weak = obj.downgrade();
            self.properties_split_view
                .connect_notify_local(Some("collapsed"), move |split, _| {
                    let Some(window) = window_weak.upgrade() else {
                        return;
                    };
                    if split.is_collapsed() && split.shows_sidebar() {
                        window.restore_focus_after_breakpoint_collapse();
                    }
                });
        }

        let window_weak = obj.downgrade();
        self.sidebar.connect_file_activated(move |path| {
            if let Some(window) = window_weak.upgrade() {
                window.open_document(path);
            }
        });

        let window_weak = obj.downgrade();
        self.sidebar
            .connect_file_renamed(move |old_path, new_path| {
                if let Some(window) = window_weak.upgrade() {
                    window.update_tab_path(old_path, new_path);
                    window.migrate_note_sidecars_after_rename(old_path, new_path);
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

        obj.update_content_stack();
        self.sidebar.load_workspaces();
        obj.load_session_and_drafts();
        obj.start_autosave_timer();
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

impl WidgetImpl for LushtextWindow {
    fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
        self.obj().clamp_preview_position(width);
        if height > 0 {
            self.search_panel.clamp_results_height(height / 3);
        }
        self.parent_size_allocate(width, height, baseline);
        if width > 0 {
            let palette_width = width * 6 / 10;
            if self.command_palette.width_request() != palette_width {
                self.command_palette.set_width_request(palette_width);
            }
            sync_split_view_widths(&self.obj(), width);
        }
    }
}

impl WindowImpl for LushtextWindow {
    fn close_request(&self) -> glib::Propagation {
        let window = self.obj().clone();
        window.clear_close_discard_drafts();
        let modified = window.modified_editors();

        if modified.is_empty() {
            self.search_panel.close();
            window.flush_dirty_drafts();
            window.save_session_sync();
            return self.parent_close_request();
        }

        let window_for_close = window.clone();
        window.show_save_changes_dialog(&modified, move |confirmed| {
            if confirmed {
                window_for_close.imp().search_panel.close();
                window_for_close.flush_dirty_drafts();
                window_for_close.save_session_sync();
                window_for_close.destroy();
            }
        });
        glib::Propagation::Stop
    }
}

impl ApplicationWindowImpl for LushtextWindow {}
impl AdwApplicationWindowImpl for LushtextWindow {}

fn configure_split_views(
    workspace_split_view: &libadwaita::OverlaySplitView,
    properties_split_view: &libadwaita::OverlaySplitView,
) {
    workspace_split_view.set_sidebar_position(gtk4::PackType::Start);
    workspace_split_view.set_sidebar_width_unit(libadwaita::LengthUnit::Sp);
    workspace_split_view.set_min_sidebar_width(WORKSPACE_SIDEBAR_MIN_WIDTH_SP);
    workspace_split_view.set_max_sidebar_width(WORKSPACE_SIDEBAR_MIN_WIDTH_SP);
    workspace_split_view.set_pin_sidebar(true);
    workspace_split_view.set_enable_show_gesture(false);
    workspace_split_view.set_enable_hide_gesture(false);

    properties_split_view.set_sidebar_position(gtk4::PackType::End);
    properties_split_view.set_sidebar_width_unit(libadwaita::LengthUnit::Sp);
    properties_split_view.set_min_sidebar_width(PROPERTIES_SIDEBAR_MIN_WIDTH_SP);
    properties_split_view.set_pin_sidebar(true);
    properties_split_view.set_enable_show_gesture(false);
    properties_split_view.set_enable_hide_gesture(false);
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
    window
        .imp()
        .workspace_split_view
        .set_sidebar_width_fraction(fraction);
    let _ = window
        .imp()
        .settings
        .set_double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION, preset.fraction());
    window.imp().workspace_split_view.set_show_sidebar(visible);
    window.imp().sidebar_visible.set(visible);
    sync_properties_breakpoint(window);
    sync_properties_split_view(window, width);
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
    window.imp().properties_split_view.set_show_sidebar(visible);
    window.imp().properties_sidebar_visible.set(visible);
}

fn install_split_view_breakpoints(window: &super::LushtextWindow) {
    let properties_bp = libadwaita::Breakpoint::new(
        libadwaita::BreakpointCondition::parse(&properties_breakpoint_condition(window))
            .expect("valid properties breakpoint condition"),
    );
    properties_bp.add_setter(
        window
            .imp()
            .properties_split_view
            .upcast_ref::<glib::Object>(),
        "collapsed",
        Some(&true.to_value()),
    );
    window
        .imp()
        .properties_breakpoint
        .replace(Some(properties_bp.clone()));
    window.add_breakpoint(properties_bp);

    let workspace_bp = libadwaita::Breakpoint::new(
        libadwaita::BreakpointCondition::parse(WORKSPACE_BREAKPOINT_MAX_WIDTH_SP)
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
}

/// Build the properties-pane breakpoint from the minimum center width instead
/// of a magic number so the shell explains *why* it collapses earlier.
fn properties_breakpoint_condition(window: &super::LushtextWindow) -> String {
    format!(
        "max-width: {}sp",
        properties_breakpoint_max_width_sp(properties_breakpoint_workspace_width_sp(window))
    )
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

fn desired_workspace_hint_fraction(window: &super::LushtextWindow) -> f64 {
    workspace_sidebar_preset(window).fraction()
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
    if workspace_sidebar_consumes_width(window) {
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

fn properties_breakpoint_workspace_width_sp(window: &super::LushtextWindow) -> f64 {
    if workspace_sidebar_consumes_width(window) {
        effective_workspace_sidebar_width_sp(window, current_window_width(window))
    } else {
        0.0
    }
}

fn workspace_sidebar_consumes_width(window: &super::LushtextWindow) -> bool {
    let split = &window.imp().workspace_split_view;
    split.shows_sidebar() && !split.is_collapsed()
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
    let condition =
        libadwaita::BreakpointCondition::parse(&properties_breakpoint_condition(window))
            .expect("valid properties breakpoint condition");
    if let Some(breakpoint) = window.imp().properties_breakpoint.borrow().as_ref() {
        breakpoint.set_condition(Some(&condition));
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
    let _ = window.imp().settings.set_double(
        keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION,
        desired_properties_fraction(window_width),
    );
}

fn sync_split_view_widths(window: &super::LushtextWindow, window_width: i32) {
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
    let _ = window.imp().settings.set_double(
        keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION,
        desired_workspace_hint_fraction(window),
    );
    sync_properties_breakpoint(window);
    sync_properties_split_view(window, window_width);
}

fn fixed_fraction(window_width: i32, min_width_sp: f64, target_fraction: f64) -> f64 {
    let width = f64::from(window_width.max(1));
    let lower = (min_width_sp / width).min(1.0);
    target_fraction.max(lower).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::{
        DUAL_PANE_LAYOUT_OVERHEAD_SP, MIN_EDITOR_CONTENT_WIDTH_SP, WorkspaceSidebarWidthPreset,
        dual_sidebar_window_width_for_center, properties_breakpoint_max_width_sp,
    };

    #[test]
    fn properties_breakpoint_width_accounts_for_workspace_preset() {
        assert_eq!(
            properties_breakpoint_max_width_sp(WorkspaceSidebarWidthPreset::Comfy.max_width_sp()),
            1350
        );
        assert_eq!(properties_breakpoint_max_width_sp(0.0), 912);
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
