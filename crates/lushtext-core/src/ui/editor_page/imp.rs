// SPDX-License-Identifier: GPL-3.0-or-later

use crate::config::keys;
use crate::services::file_limits::FileSizeCheck;
use crate::ui::search_bar::LushtextSearchBar;
use gtk4::gio;
use gtk4::subclass::prelude::*;
use gtk4::{self, glib, CompositeTemplate};
use sourceview5::prelude::*;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/editor-page.ui")]
pub struct LushtextEditorPage {
    #[template_child]
    pub source_view: TemplateChild<sourceview5::View>,
    #[template_child]
    pub scrolled_window: TemplateChild<gtk4::ScrolledWindow>,
    #[template_child]
    pub search_revealer: TemplateChild<gtk4::Revealer>,
    #[template_child]
    pub search_bar: TemplateChild<LushtextSearchBar>,

    pub file_path: RefCell<Option<PathBuf>>,
    pub file_size: Cell<Option<u64>>,
    pub size_check: Cell<FileSizeCheck>,
    pub evicted: Cell<bool>,
    pub cancel_token: Arc<AtomicBool>,
    pub settings: gio::Settings,
    pub dark_handler_id: RefCell<Option<glib::SignalHandlerId>>,
}

impl Default for LushtextEditorPage {
    fn default() -> Self {
        Self {
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
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextEditorPage {
    const NAME: &'static str = "LushtextEditorPage";
    type Type = super::LushtextEditorPage;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        LushtextSearchBar::ensure_type();
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextEditorPage {
    fn constructed(&self) {
        self.parent_constructed();

        let buffer = self
            .source_view
            .buffer()
            .downcast::<sourceview5::Buffer>()
            .expect("GtkSourceView buffer");
        buffer.set_highlight_syntax(true);

        let settings = &self.settings;

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
        settings
            .bind(keys::TAB_WIDTH, &*self.source_view, "tab-width")
            .flags(gio::SettingsBindFlags::GET)
            .build();
        settings
            .bind(
                keys::INSERT_SPACES,
                &*self.source_view,
                "insert-spaces-instead-of-tabs",
            )
            .flags(gio::SettingsBindFlags::GET)
            .build();

        // bool → WrapMode: no direct settings binding
        apply_word_wrap(&self.source_view, settings);
        let view = self.source_view.clone();
        settings.connect_changed(Some(keys::WORD_WRAP), move |s, _| {
            apply_word_wrap(&view, s);
        });

        apply_color_scheme(&buffer, settings);
        {
            let buf = buffer.clone();
            let s = settings.clone();
            settings.connect_changed(Some(keys::STYLE_SCHEME), move |_, _| {
                apply_color_scheme(&buf, &s);
            });
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

        let revealer = self.search_revealer.clone();
        let source_view = self.source_view.clone();
        self.search_bar.connect_close(move || {
            revealer.set_reveal_child(false);
            source_view.grab_focus();
        });
    }
}

impl WidgetImpl for LushtextEditorPage {}
impl BoxImpl for LushtextEditorPage {}

impl Drop for LushtextEditorPage {
    fn drop(&mut self) {
        if let Some(handler_id) = self.dark_handler_id.take() {
            libadwaita::StyleManager::default().disconnect(handler_id);
        }
    }
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
