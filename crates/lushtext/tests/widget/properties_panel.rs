// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextPropertiesPanel widget.

use crate::common::ensure_gtk_init;
use glib::subclass::prelude::ObjectSubclassIsExt;
use lushtext_core::ui::accessibility::test_audit::AccessibleAudit;
use lushtext_core::ui::properties_panel::LushtextPropertiesPanel;

#[test]
fn test_properties_panel_exposes_document_metadata_accessibility() {
    ensure_gtk_init();
    let panel = LushtextPropertiesPanel::new();
    let imp = panel.imp();

    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Group)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&panel);
    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&*imp.location_row);
    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&*imp.statistics_row);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Group)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&*imp.health_group);
}

#[test]
fn test_properties_panel_empty_state_refreshes_accessible_values() {
    ensure_gtk_init();
    let panel = LushtextPropertiesPanel::new();
    panel.set_active_editor(None);
    let imp = panel.imp();

    for row in [
        &*imp.location_row,
        &*imp.file_size_row,
        &*imp.statistics_row,
        &*imp.formatting_source_row,
        &*imp.health_summary_row,
    ] {
        AccessibleAudit::new()
            .properties(&[gtk4::AccessibleProperty::ValueText])
            .assert_on(row);
    }
    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .states(&[
            gtk4::AccessibleState::Hidden,
            gtk4::AccessibleState::Disabled,
        ])
        .assert_on(&*imp.health_review_button);
}
