// SPDX-License-Identifier: GPL-3.0-or-later

use crate::ui::search_bar::LushtextSearchBar;
use gtk4::subclass::prelude::*;
use gtk4::{self, glib, CompositeTemplate};
use sourceview5::prelude::*;
use std::cell::RefCell;
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

    /// The file path this editor page is associated with.
    pub file_path: RefCell<Option<PathBuf>>,
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextEditorPage {
    const NAME: &'static str = "LushtextEditorPage";
    type Type = super::LushtextEditorPage;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        // Ensure child widget types are registered before template parsing
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

        // Set up the source buffer with syntax highlighting enabled
        let buffer = self
            .source_view
            .buffer()
            .downcast::<sourceview5::Buffer>()
            .expect("GtkSourceView buffer");
        buffer.set_highlight_syntax(true);

        // Apply default style scheme
        let scheme_manager = sourceview5::StyleSchemeManager::default();
        if let Some(scheme) = scheme_manager.scheme("Adwaita") {
            buffer.set_style_scheme(Some(&scheme));
        }
    }
}

impl WidgetImpl for LushtextEditorPage {}
impl BoxImpl for LushtextEditorPage {}
