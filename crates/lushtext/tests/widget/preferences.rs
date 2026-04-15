// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for LushtextPreferences dialog.

use crate::common::ensure_gtk_init;
use glib::subclass::prelude::ObjectSubclassIsExt;
use lushtext_core::config::{self, keys};
use libadwaita::prelude::*;
use lushtext_core::ui::preferences::LushtextPreferences;

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
