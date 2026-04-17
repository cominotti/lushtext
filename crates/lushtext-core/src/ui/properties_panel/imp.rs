// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the properties panel widget.
//!
//! The widget keeps slow, inspectable document details in one place and owns
//! the dynamic file-health rows that are rebuilt as the active document changes.

use gtk4::{self, CompositeTemplate, gio, glib};
use libadwaita::subclass::prelude::*;
use std::cell::RefCell;

#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/properties-panel.ui")]
pub struct LushtextPropertiesPanel {
    /// Full path for the active document, or an empty-state message when no
    /// file-backed editor is selected.
    #[template_child]
    pub location_row: TemplateChild<libadwaita::ActionRow>,
    /// On-disk size populated after async file load finishes.
    #[template_child]
    pub file_size_row: TemplateChild<libadwaita::ActionRow>,
    /// Buffer statistics that stay available even for untitled documents.
    #[template_child]
    pub statistics_row: TemplateChild<libadwaita::ActionRow>,
    /// Whether formatting is coming from raw preferences or an EditorConfig
    /// override for the active file.
    #[template_child]
    pub formatting_source_row: TemplateChild<libadwaita::ActionRow>,
    /// Group containing the summary row plus any dynamic finding rows.
    #[template_child]
    pub health_group: TemplateChild<libadwaita::PreferencesGroup>,
    /// Stable summary row that explains the current file-health state.
    #[template_child]
    pub health_summary_row: TemplateChild<libadwaita::ActionRow>,
    /// Launch point for the dedicated health dialog when findings exist.
    #[template_child]
    pub health_review_button: TemplateChild<gtk4::Button>,
    /// Shared application settings used to explain whether Preferences or
    /// EditorConfig currently drive the document formatting behavior.
    pub settings: gio::Settings,
    /// Extra rows currently mounted under the health group.
    pub health_detail_rows: RefCell<Vec<libadwaita::ActionRow>>,
}

impl Default for LushtextPropertiesPanel {
    fn default() -> Self {
        Self {
            location_row: TemplateChild::default(),
            file_size_row: TemplateChild::default(),
            statistics_row: TemplateChild::default(),
            formatting_source_row: TemplateChild::default(),
            health_group: TemplateChild::default(),
            health_summary_row: TemplateChild::default(),
            health_review_button: TemplateChild::default(),
            settings: gio::Settings::new(crate::config::APP_ID),
            health_detail_rows: RefCell::new(Vec::new()),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextPropertiesPanel {
    const NAME: &'static str = "LushtextPropertiesPanel";
    type Type = super::LushtextPropertiesPanel;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextPropertiesPanel {}

impl WidgetImpl for LushtextPropertiesPanel {}
impl BoxImpl for LushtextPropertiesPanel {}
