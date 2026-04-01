// SPDX-License-Identifier: GPL-3.0-or-later

use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::sidebar::LushtextSidebar;
use glib::prelude::*;
use gtk4::{self, glib, CompositeTemplate};
use libadwaita::subclass::prelude::*;

#[derive(Default, CompositeTemplate)]
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
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextWindow {
    const NAME: &'static str = "LushtextWindow";
    type Type = super::LushtextWindow;
    type ParentType = libadwaita::ApplicationWindow;

    fn class_init(klass: &mut Self::Class) {
        // Ensure custom widget types are registered
        LushtextSidebar::ensure_type();
        LushtextEditorPage::ensure_type();

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

        // Connect sidebar file activation to open documents
        let window = obj.clone();
        self.sidebar.connect_file_activated(move |path| {
            window.open_document(path);
        });

        // Update stack when tabs change
        let window = obj.clone();
        self.tab_view.connect_notify_local(Some("n-pages"), move |_, _| {
            window.update_content_stack();
        });

        // Start with empty state
        obj.update_content_stack();
    }
}

impl WidgetImpl for LushtextWindow {}
impl WindowImpl for LushtextWindow {}
impl ApplicationWindowImpl for LushtextWindow {}
impl AdwApplicationWindowImpl for LushtextWindow {}
