// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the bottom status bar widget.
//!
//! The widget keeps the left/right pane toggles fixed while the metadata
//! cluster can collapse from separate document-format buttons into one grouped
//! entry point on narrow allocations.

use std::cell::Cell;

use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, glib};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/status-bar.ui")]
pub struct LushtextStatusBar {
    /// Fixed left toggle for the workspace sidebar.
    #[template_child]
    pub sidebar_toggle_button: TemplateChild<gtk4::ToggleButton>,
    /// Fixed right toggle for the properties pane.
    #[template_child]
    pub properties_toggle_button: TemplateChild<gtk4::ToggleButton>,
    /// Status-message area that stretches across the middle of the bar.
    #[template_child]
    pub message_label: TemplateChild<gtk4::Label>,
    /// Container for the document metadata cluster. Hidden when no tab is active.
    #[template_child]
    pub metadata_box: TemplateChild<gtk4::Box>,
    /// Badge indicating when EditorConfig overrides are active.
    #[template_child]
    pub editorconfig_label: TemplateChild<gtk4::Label>,
    /// Normal-width group for the separate line-ending, encoding, and issue buttons.
    #[template_child]
    pub document_format_controls_box: TemplateChild<gtk4::Box>,
    /// Line-ending entry point for the active document.
    #[template_child]
    pub line_ending_button: TemplateChild<gtk4::Button>,
    /// Encoding entry point for the active document.
    #[template_child]
    pub encoding_button: TemplateChild<gtk4::Button>,
    /// Optional separator ahead of the issue button.
    #[template_child]
    pub health_separator: TemplateChild<gtk4::Separator>,
    /// File-health entry point when the active document has findings.
    #[template_child]
    pub health_button: TemplateChild<gtk4::Button>,
    /// Grouped narrow-width entry point for encoding, line endings, and issues.
    #[template_child]
    pub document_format_button: TemplateChild<gtk4::Button>,
    /// File-size label for the active document.
    #[template_child]
    pub file_size_label: TemplateChild<gtk4::Label>,
    /// Whether the compact grouped format button currently replaces the
    /// separate status-bar controls.
    pub format_controls_compact: Cell<bool>,
    /// Whether the active document currently exposes any file-health findings.
    pub health_visible: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextStatusBar {
    const NAME: &'static str = "LushtextStatusBar";
    type Type = super::LushtextStatusBar;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextStatusBar {}
impl WidgetImpl for LushtextStatusBar {
    fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
        self.parent_size_allocate(width, height, baseline);

        // Collapse the format controls before they start competing with the
        // message label on narrower window widths.
        self.obj().update_format_controls_compact(width);
    }
}
impl BoxImpl for LushtextStatusBar {}
