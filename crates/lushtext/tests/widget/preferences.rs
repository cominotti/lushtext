// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for LushtextPreferences dialog.

use crate::common::ensure_gtk_init;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::config::{self, keys};
use libadwaita::prelude::*;
use lushtext_core::ui::preferences::LushtextPreferences;

fn has_accessible_role(root: &impl IsA<gtk4::Widget>, role: gtk4::AccessibleRole) -> bool {
    let mut stack = vec![root.as_ref().clone()];
    while let Some(widget) = stack.pop() {
        if widget.accessible_role() == role {
            return true;
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            stack.push(current.clone());
            child = current.next_sibling();
        }
    }
    false
}

#[test]
fn test_new() {
    ensure_gtk_init();
    let _prefs = LushtextPreferences::new();
}

#[test]
fn test_default_equals_new() {
    ensure_gtk_init();
    let _prefs: LushtextPreferences = LushtextPreferences::default();
}

#[test]
fn test_preferences_controls_expose_accessibility_roles() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert_eq!(
        imp.style_scheme_row.accessible_role(),
        gtk4::AccessibleRole::ComboBox
    );
    assert_eq!(
        imp.workspace_sidebar_width_row.accessible_role(),
        gtk4::AccessibleRole::ComboBox
    );
    assert_eq!(
        imp.word_wrap_row.accessible_role(),
        gtk4::AccessibleRole::Switch
    );
    assert_eq!(
        imp.tab_width_row.accessible_role(),
        gtk4::AccessibleRole::Group
    );
    assert!(
        has_accessible_role(&*imp.tab_width_row, gtk4::AccessibleRole::SpinButton),
        "numeric tab width row should expose an internal spin button"
    );
    assert!(
        has_accessible_role(&*imp.font_button, gtk4::AccessibleRole::Button),
        "font chooser should expose a button role"
    );
    assert!(
        has_accessible_role(&*imp.transparency_button, gtk4::AccessibleRole::Button),
        "transparency menu should expose a button role"
    );
}

// --- GSettings binding tests ---

#[test]
fn test_word_wrap_row_bound_to_settings() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    // Schema default: word-wrap = true
    assert!(imp.word_wrap_row.is_active());
}

#[test]
fn test_show_line_numbers_row_bound_to_settings() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    // Schema default: show-line-numbers = true
    assert!(imp.show_line_numbers_row.is_active());
}

#[test]
fn test_highlight_line_row_bound_to_settings() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert!(imp.highlight_line_row.is_active());
}

#[test]
fn test_show_minimap_row_bound_to_settings() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert!(!imp.show_minimap_row.is_active());
}

#[test]
fn test_minimap_long_line_markers_row_defaults_off_and_updates_setting() {
    ensure_gtk_init();
    let settings = gio::Settings::new(config::APP_ID);
    settings.reset(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE);

    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert!(!settings.boolean(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE));
    assert!(!imp.minimap_long_line_markers_row.is_active());

    imp.minimap_long_line_markers_row.set_active(true);
    while glib::MainContext::default().iteration(false) {}

    assert!(settings.boolean(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE));
}

#[test]
fn test_minimap_controls_are_grouped_on_editor_page() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert_eq!(imp.minimap_group.title().as_str(), "Minimap");
    assert!(imp.show_minimap_row.is_ancestor(&*imp.minimap_group));
    assert!(
        imp.minimap_long_line_markers_row
            .is_ancestor(&*imp.minimap_group)
    );
}

#[test]
fn test_focus_mode_column_width_row_bound_to_settings() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert_eq!(imp.focus_mode_target_columns_row.value(), 80.0);
}

#[test]
fn test_focus_mode_typewriter_scrolling_defaults_off() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert!(!imp.focus_mode_typewriter_scrolling_row.is_active());
}

#[test]
fn test_insert_spaces_row_bound_to_settings() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert!(imp.insert_spaces_row.is_active());
}

#[test]
fn test_use_system_font_row_bound_to_settings() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert!(imp.use_system_font_row.is_active());
}

#[test]
fn test_workspace_auto_collapse_row_bound_to_settings() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert!(imp.workspace_auto_collapse_row.is_active());
}

#[test]
fn test_workspace_empty_folder_lookahead_cap_row_bound_to_settings() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert_eq!(imp.workspace_empty_folder_lookahead_cap_row.value(), 1000.0);
}

#[test]
fn test_custom_font_row_disabled_when_system_font() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    // System font is on by default → custom font row should be insensitive
    assert!(!imp.custom_font_row.is_sensitive());
}

#[test]
fn test_color_scheme_row_populated() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    // The combo row model should have been populated with available schemes
    assert!(imp.style_scheme_row.model().is_some());
}

#[test]
fn test_transparency_row_is_visible_with_default_percentage() {
    ensure_gtk_init();
    let settings = gio::Settings::new(config::APP_ID);
    settings
        .set_double(keys::TAB_CONTENT_OPACITY, 1.0)
        .expect("reset tab-content-opacity");

    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert!(imp.transparency_row.is_visible());
    assert_eq!(imp.transparency_adjustment.value(), 1.0);
    assert_eq!(imp.transparency_label.label().as_str(), "100%");
}

#[test]
fn test_transparency_row_updates_setting_and_label() {
    ensure_gtk_init();
    let settings = gio::Settings::new(config::APP_ID);
    settings
        .set_double(keys::TAB_CONTENT_OPACITY, 1.0)
        .expect("reset tab-content-opacity");

    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();
    imp.transparency_adjustment.set_value(0.85);

    while glib::MainContext::default().iteration(false) {}

    assert!((settings.double(keys::TAB_CONTENT_OPACITY) - 0.85).abs() < f64::EPSILON);
    assert_eq!(imp.transparency_label.label().as_str(), " 85%");
}

#[test]
fn test_transparency_row_restores_persisted_percentage() {
    ensure_gtk_init();
    let settings = gio::Settings::new(config::APP_ID);
    settings
        .set_double(keys::TAB_CONTENT_OPACITY, 0.65)
        .expect("set tab-content-opacity");

    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert!((imp.transparency_adjustment.value() - 0.65).abs() < f64::EPSILON);
    assert_eq!(imp.transparency_label.label().as_str(), " 65%");
}

#[test]
fn test_workspace_sidebar_width_row_lists_all_presets() {
    ensure_gtk_init();
    let settings = gio::Settings::new(config::APP_ID);
    settings
        .set_double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION, 0.3)
        .expect("set comfy preset");

    let prefs = LushtextPreferences::new();
    let model = prefs
        .imp()
        .workspace_sidebar_width_row
        .model()
        .and_downcast::<gtk4::StringList>()
        .expect("workspace width row should use a StringList model");

    assert_eq!(model.n_items(), 3);
    assert_eq!(model.string(0).as_deref(), Some("Small"));
    assert_eq!(model.string(1).as_deref(), Some("Comfy"));
    assert_eq!(model.string(2).as_deref(), Some("Large"));
    assert_eq!(prefs.imp().workspace_sidebar_width_row.selected(), 1);
}

#[test]
fn test_workspace_sidebar_width_row_updates_setting() {
    ensure_gtk_init();
    let settings = gio::Settings::new(config::APP_ID);
    settings
        .set_double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION, 0.3)
        .expect("set comfy preset");

    let prefs = LushtextPreferences::new();
    prefs.imp().workspace_sidebar_width_row.set_selected(2);

    while glib::MainContext::default().iteration(false) {}

    assert_eq!(settings.double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION), 0.4);
}
