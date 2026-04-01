// SPDX-License-Identifier: GPL-3.0-or-later

use crate::ui::search_bar::LushtextSearchBar;
use gtk4::subclass::prelude::*;
use gtk4::{self, glib, CompositeTemplate};
use sourceview5::prelude::*;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;

#[derive(Default, CompositeTemplate)]
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

        apply_color_scheme(&buffer);

        let buffer_for_signal = buffer.clone();
        libadwaita::StyleManager::default().connect_dark_notify(move |_| {
            apply_color_scheme(&buffer_for_signal);
        });

        // Close button and Escape key hide the search bar
        let revealer = self.search_revealer.clone();
        self.search_bar.connect_close(move || {
            revealer.set_reveal_child(false);
        });
    }
}

impl WidgetImpl for LushtextEditorPage {}
impl BoxImpl for LushtextEditorPage {}

fn apply_color_scheme(buffer: &sourceview5::Buffer) {
    let style_manager = libadwaita::StyleManager::default();
    let scheme_id = if style_manager.is_dark() {
        "Adwaita-dark"
    } else {
        "Adwaita"
    };
    let scheme_manager = sourceview5::StyleSchemeManager::default();
    if let Some(scheme) = scheme_manager.scheme(scheme_id) {
        buffer.set_style_scheme(Some(&scheme));
    }
}
