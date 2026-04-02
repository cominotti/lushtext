// SPDX-License-Identifier: GPL-3.0-or-later

use crate::config::{self, keys};
use crate::ui::command_palette::LushtextCommandPalette;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::sidebar::LushtextSidebar;
use crate::ui::status_bar::{LushtextStatusBar, MessageKind};
use glib::prelude::*;
use gtk4::prelude::*;
use gtk4::{self, gio, glib, CompositeTemplate};
use libadwaita::subclass::prelude::*;

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

    pub settings: gio::Settings,
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
            settings: gio::Settings::new(config::APP_ID),
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

        // --- Persist window geometry incrementally via notify signals ---
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

        // --- Sidebar position persist on user drag ---
        {
            let settings = settings.clone();
            let window_weak = obj.downgrade();
            self.main_paned
                .connect_notify_local(Some("position"), move |paned, _| {
                    if let Some(window) = window_weak.upgrade() {
                        clamp_sidebar_position(paned, window.width(), &settings);
                    }
                });
        }

        // --- Sidebar file activation ---
        let window = obj.clone();
        self.sidebar.connect_file_activated(move |path| {
            window.open_document(path);
        });

        // --- Sidebar rename/delete notifications ---
        let window = obj.clone();
        self.sidebar
            .connect_file_renamed(move |old_path, new_path| {
                window.update_tab_path(old_path, new_path);
                let name = new_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                window
                    .imp()
                    .status_bar
                    .push_message(&format!("Renamed to {name}"), MessageKind::Info);
            });

        let window = obj.clone();
        self.sidebar.connect_file_deleted(move |path| {
            window.close_tab_for_path(path);
            window
                .imp()
                .status_bar
                .push_message("Deleted", MessageKind::Info);
        });

        let window = obj.clone();
        self.sidebar.connect_file_created(move |path| {
            window.open_document(path);
        });

        // --- Command palette callbacks ---
        let window = obj.clone();
        self.command_palette.connect_item_activated(move |item| {
            if item.is_file() {
                if let Some(path) = item.file_path() {
                    window.open_document(&path);
                }
            } else if item.is_command() {
                let action_id = item.action_id();
                if let Some(stripped) = action_id.strip_prefix("win.") {
                    gtk4::prelude::ActionGroupExt::activate_action(&window, stripped, None);
                } else if let Some(stripped) = action_id.strip_prefix("app.") {
                    if let Some(app) = window.application() {
                        gtk4::prelude::ActionGroupExt::activate_action(&app, stripped, None);
                    }
                }
            }
            window.close_command_palette();
        });

        let window = obj.clone();
        self.command_palette.connect_close_requested(move || {
            window.close_command_palette();
        });

        // --- Sidebar workspace change → rebuild file index ---
        let window = obj.clone();
        self.sidebar.connect_workspace_changed(move || {
            window.rebuild_file_index();
        });

        // --- Tab change signals ---
        let window = obj.clone();
        self.tab_view
            .connect_notify_local(Some("n-pages"), move |_, _| {
                window.update_content_stack();
            });

        let window = obj.clone();
        self.tab_view
            .connect_notify_local(Some("selected-page"), move |_, _| {
                window.refresh_status_bar();
            });

        // Start with empty state
        obj.update_content_stack();

        // Load workspaces from disk and build initial file index
        self.sidebar.load_workspaces();
        obj.rebuild_file_index();
    }
}

impl WidgetImpl for LushtextWindow {
    fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
        self.parent_size_allocate(width, height, baseline);
        // Clamp sidebar on every allocation — this is the definitive width,
        // free from the stale-value timing issues of property notifications.
        clamp_sidebar_position(&self.main_paned, width, &self.settings);
        // Keep palette at 60% window width (guarded to avoid re-layout cycles)
        if width > 0 {
            let palette_width = width * 6 / 10;
            if self.command_palette.width_request() != palette_width {
                self.command_palette.set_width_request(palette_width);
            }
        }
    }
}

impl WindowImpl for LushtextWindow {}
impl ApplicationWindowImpl for LushtextWindow {}
impl AdwApplicationWindowImpl for LushtextWindow {}

/// Clamp the sidebar pane position to at most 1/3 of the window width,
/// and persist the (possibly clamped) value to GSettings.
pub fn clamp_sidebar_position(paned: &gtk4::Paned, window_width: i32, settings: &gio::Settings) {
    if window_width <= 0 {
        return;
    }
    let max = window_width / 3;
    let current = paned.position();
    let clamped = current.min(max);
    if clamped != current {
        paned.set_position(clamped);
    }
    let final_pos = paned.position();
    if settings.int(keys::SIDEBAR_POSITION) != final_pos {
        let _ = settings.set_int(keys::SIDEBAR_POSITION, final_pos);
    }
}
