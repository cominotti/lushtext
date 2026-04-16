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

// --- File size ---

#[test]
fn test_set_file_size_some() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.set_file_size(Some(2_500));
    assert_eq!(bar.imp().file_size_label.label().as_str(), "2.5 KB");
}

#[test]
fn test_set_file_size_bytes() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.set_file_size(Some(512));
    assert_eq!(bar.imp().file_size_label.label().as_str(), "512 B");
}

#[test]
fn test_set_file_size_mb() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.set_file_size(Some(2_000_000));
    assert_eq!(bar.imp().file_size_label.label().as_str(), "2.0 MB");
}

#[test]
fn test_set_file_size_none_clears_label() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.set_file_size(Some(1_000));
    bar.set_file_size(None);
    assert_eq!(bar.imp().file_size_label.label().as_str(), "");
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
fn test_health_button_hidden_by_default() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert!(!bar.imp().health_button.is_visible());
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
    let tooltip = bar.imp().sidebar_toggle_button.tooltip_text().expect("expected operation to succeed");
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

// --- Properties toggle button ---

#[test]
fn test_properties_toggle_button_exists() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    let _button = &bar.imp().properties_toggle_button;
}

#[test]
fn test_properties_toggle_button_icon() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert_eq!(
        bar.imp()
            .properties_toggle_button
            .icon_name()
            .expect("expected operation to succeed")
            .as_str(),
        "sidebar-show-right-symbolic"
    );
}

#[test]
fn test_properties_toggle_button_is_flat() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert!(bar.imp().properties_toggle_button.has_css_class("flat"));
}

#[test]
fn test_properties_toggle_button_has_tooltip() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    let tooltip = bar.imp().properties_toggle_button.tooltip_text().expect("expected operation to succeed");
    assert!(tooltip.contains("Properties"));
}

#[test]
fn test_properties_toggle_button_action_name() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert_eq!(
        bar.imp()
            .properties_toggle_button
            .action_name()
            .expect("expected operation to succeed")
            .as_str(),
        "win.toggle-properties"
    );
}

#[test]
fn test_properties_toggle_button_is_rightmost_child() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    let last = bar
        .last_child()
        .and_downcast::<gtk4::ToggleButton>()
        .expect("rightmost child is a toggle button");
    assert_eq!(last.as_ptr(), bar.imp().properties_toggle_button.as_ptr());
}
