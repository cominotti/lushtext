// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the properties panel widget.
//!
//! The widget keeps slow, inspectable document details in one place and owns
//! the dynamic file-health rows that are rebuilt as the active document changes.

use gtk4::{self, CompositeTemplate, gio, glib};
use libadwaita::subclass::prelude::*;
use std::cell::RefCell;

use crate::ui::accessibility;

/// Private template implementation for the document properties panel.
///
/// Binds the static properties-panel rows to instance data, while dynamic
/// file-health rows remain ordinary Rust-owned children.
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
// Register the Rust implementation as a `GtkBox` subclass so the public
// wrapper can participate in templates and Libadwaita layout containers.
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

impl ObjectImpl for LushtextPropertiesPanel {
    fn constructed(&self) {
        self.parent_constructed();
        self.apply_accessibility_metadata();
    }
}

impl LushtextPropertiesPanel {
    /// Give the document-inspection rows stable accessible identities.
    ///
    /// AdwActionRow exposes visible titles, but the panel's important state is
    /// usually in subtitles that change when the active editor changes. The
    /// helper-backed metadata keeps those values available to AT and tests.
    fn apply_accessibility_metadata(&self) {
        accessibility::set_role(&*self.obj(), gtk4::AccessibleRole::Group);
        accessibility::set_labelled_description(
            &*self.obj(),
            "Document properties",
            "Document metadata, formatting source, and file-health details",
        );
        accessibility::set_labelled_description(
            &*self.location_row,
            "Document location",
            "Path or untitled state for the active document",
        );
        accessibility::set_labelled_description(
            &*self.file_size_row,
            "Document file size",
            "On-disk size for the active saved document",
        );
        accessibility::set_labelled_description(
            &*self.statistics_row,
            "Document statistics",
            "Line and character counts for the active document",
        );
        accessibility::set_labelled_description(
            &*self.formatting_source_row,
            "Formatting source",
            "Whether Preferences or EditorConfig currently controls formatting",
        );
        accessibility::set_role(&*self.health_group, gtk4::AccessibleRole::Group);
        accessibility::set_labelled_description(
            &*self.health_group,
            "File health",
            "Encoding, line-ending, and file-health findings for the active document",
        );
        accessibility::set_labelled_description(
            &*self.health_summary_row,
            "File health summary",
            "Summary of file-health findings for the active document",
        );
        accessibility::set_labelled_description(
            &*self.health_review_button,
            "Review file health findings",
            "Open detailed file-health findings for the active document",
        );
        accessibility::set_hidden(&*self.health_review_button, true);
        accessibility::set_disabled(&*self.health_review_button, true);
    }
}

impl WidgetImpl for LushtextPropertiesPanel {}
impl BoxImpl for LushtextPropertiesPanel {}
