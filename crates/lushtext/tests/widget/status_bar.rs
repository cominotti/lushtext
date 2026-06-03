// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextStatusBar widget.

use crate::common::ensure_gtk_init;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::services::notifications::StatusMessage;
use lushtext_core::ui::status_bar::{LushtextStatusBar, MessageKind};

// --- Construction ---

#[test]
fn test_new() {
    ensure_gtk_init();
    let _bar = LushtextStatusBar::new();
}

#[test]
fn test_default_equals_new() {
    ensure_gtk_init();
    let _bar: LushtextStatusBar = LushtextStatusBar::default();
}

#[test]
fn test_initially_no_message() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert_eq!(bar.imp().message_label.label().as_str(), "");
}

#[test]
fn test_status_controls_expose_accessibility_roles() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();

    assert_eq!(
        bar.imp().sidebar_toggle_button.accessible_role(),
        gtk4::AccessibleRole::ToggleButton
    );
    assert_eq!(
        bar.imp().metadata_box.accessible_role(),
        gtk4::AccessibleRole::Group
    );
    assert_eq!(
        bar.imp().line_ending_button.accessible_role(),
        gtk4::AccessibleRole::Button
    );
    assert_eq!(
        bar.imp().encoding_button.accessible_role(),
        gtk4::AccessibleRole::Button
    );
}

fn status_message(text: &str, severity: MessageKind) -> StatusMessage {
    StatusMessage {
        text: text.to_string(),
        severity,
    }
}

// --- Message rendering ---

#[test]
fn test_render_message_sets_label() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.render_message(Some(&status_message("File saved", MessageKind::Info)));
    assert_eq!(bar.imp().message_label.label().as_str(), "File saved");
}

#[test]
fn test_render_message_info_has_css_class() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.render_message(Some(&status_message("Loaded", MessageKind::Info)));
    assert!(bar.imp().message_label.has_css_class("status-info"));
}

#[test]
fn test_render_message_warning_has_css_class() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.render_message(Some(&status_message("Caution", MessageKind::Warning)));
    assert!(bar.imp().message_label.has_css_class("status-warning"));
}

#[test]
fn test_render_message_error_has_css_class() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.render_message(Some(&status_message(
        "Permission denied",
        MessageKind::Error,
    )));
    assert!(bar.imp().message_label.has_css_class("status-error"));
}

#[test]
fn test_render_message_replaces_text() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.render_message(Some(&status_message("first", MessageKind::Info)));
    bar.render_message(Some(&status_message("second", MessageKind::Warning)));
    assert_eq!(bar.imp().message_label.label().as_str(), "second");
}

#[test]
fn test_render_message_replaces_css_class() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.render_message(Some(&status_message("first", MessageKind::Warning)));
    bar.render_message(Some(&status_message("second", MessageKind::Error)));
    assert!(bar.imp().message_label.has_css_class("status-error"));
    assert!(!bar.imp().message_label.has_css_class("status-warning"));
}

// --- Message clearing ---

#[test]
fn test_render_none_empties_label() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.render_message(Some(&status_message("test", MessageKind::Info)));
    bar.render_message(None);
    assert_eq!(bar.imp().message_label.label().as_str(), "");
}

#[test]
fn test_render_none_removes_css_classes() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.render_message(Some(&status_message("test", MessageKind::Error)));
    bar.render_message(None);
    assert!(!bar.imp().message_label.has_css_class("status-error"));
    assert!(!bar.imp().message_label.has_css_class("status-warning"));
    assert!(!bar.imp().message_label.has_css_class("status-info"));
}

// --- Metadata visibility ---

#[test]
fn test_metadata_visible_by_default() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert!(bar.imp().metadata_box.is_visible());
}

#[test]
fn test_set_metadata_hidden() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.set_metadata_visible(false);
    assert!(!bar.imp().metadata_box.is_visible());
}

#[test]
fn test_set_metadata_visible_again() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.set_metadata_visible(false);
    bar.set_metadata_visible(true);
    assert!(bar.imp().metadata_box.is_visible());
}

// --- Metadata controls ---

#[test]
fn test_encoding_button_shows_utf8() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert_eq!(bar.imp().encoding_button.label().as_deref(), Some("UTF-8"));
}

#[test]
fn test_line_ending_button_shows_lf() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert_eq!(bar.imp().line_ending_button.label().as_deref(), Some("LF"));
}

#[test]
fn test_editorconfig_separator_hidden_by_default() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert!(!bar.imp().editorconfig_separator.is_visible());
}

#[test]
fn test_set_editorconfig_active_shows_badge_and_separator() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.set_editorconfig_active(true);
    assert!(bar.imp().editorconfig_label.is_visible());
    assert!(bar.imp().editorconfig_separator.is_visible());
}

#[test]
fn test_set_editorconfig_inactive_hides_badge_and_separator() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.set_editorconfig_active(true);
    bar.set_editorconfig_active(false);
    assert!(!bar.imp().editorconfig_label.is_visible());
    assert!(!bar.imp().editorconfig_separator.is_visible());
}

// --- Sidebar toggle button ---

#[test]
fn test_sidebar_toggle_button_exists() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    let _button = &bar.imp().sidebar_toggle_button;
}

#[test]
fn test_sidebar_toggle_button_icon() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert_eq!(
        bar.imp()
            .sidebar_toggle_button
            .icon_name()
            .expect("expected operation to succeed")
            .as_str(),
        "sidebar-show-symbolic"
    );
}

#[test]
fn test_sidebar_toggle_button_is_flat() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert!(bar.imp().sidebar_toggle_button.has_css_class("flat"));
}

#[test]
fn test_sidebar_toggle_button_has_tooltip() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    let tooltip = bar
        .imp()
        .sidebar_toggle_button
        .tooltip_text()
        .expect("expected operation to succeed");
    assert!(tooltip.contains("Sidebar"));
}

#[test]
fn test_sidebar_toggle_button_action_name() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert_eq!(
        bar.imp()
            .sidebar_toggle_button
            .action_name()
            .expect("expected operation to succeed")
            .as_str(),
        "win.toggle-sidebar"
    );
}
