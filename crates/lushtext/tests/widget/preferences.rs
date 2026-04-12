// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for LushtextPreferences dialog.

use crate::common::ensure_gtk_init;
use glib::subclass::prelude::ObjectSubclassIsExt;
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
    let _prefs: LushtextPreferences = Default::default();
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
