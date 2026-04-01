// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextStatusBar widget.

use crate::common::ensure_gtk_init;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
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
    let _bar: LushtextStatusBar = Default::default();
}

#[test]
fn test_initially_no_message() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert_eq!(bar.imp().message_label.label().as_str(), "");
}

// --- Message posting ---

#[test]
fn test_push_message_sets_label() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.push_message("File saved", MessageKind::Info);
    assert_eq!(bar.imp().message_label.label().as_str(), "File saved");
}

#[test]
fn test_push_message_info_has_css_class() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.push_message("Loaded", MessageKind::Info);
    assert!(bar.imp().message_label.has_css_class("status-info"));
}

#[test]
fn test_push_message_warning_has_css_class() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.push_message("Caution", MessageKind::Warning);
    assert!(bar.imp().message_label.has_css_class("status-warning"));
}

#[test]
fn test_push_message_error_has_css_class() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.push_message("Permission denied", MessageKind::Error);
    assert!(bar.imp().message_label.has_css_class("status-error"));
}

#[test]
fn test_push_message_replaces_text() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.push_message("first", MessageKind::Info);
    bar.push_message("second", MessageKind::Warning);
    assert_eq!(bar.imp().message_label.label().as_str(), "second");
}

#[test]
fn test_push_message_replaces_css_class() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.push_message("first", MessageKind::Warning);
    bar.push_message("second", MessageKind::Error);
    assert!(bar.imp().message_label.has_css_class("status-error"));
    assert!(!bar.imp().message_label.has_css_class("status-warning"));
}

// --- Message clearing ---

#[test]
fn test_clear_message_empties_label() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.push_message("test", MessageKind::Info);
    bar.clear_message();
    assert_eq!(bar.imp().message_label.label().as_str(), "");
}

#[test]
fn test_clear_message_removes_css_classes() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.push_message("test", MessageKind::Error);
    bar.clear_message();
    assert!(!bar.imp().message_label.has_css_class("status-error"));
    assert!(!bar.imp().message_label.has_css_class("status-warning"));
    assert!(!bar.imp().message_label.has_css_class("status-info"));
}

// --- Generation counter ---

#[test]
fn test_generation_increments_on_push() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    let gen_before = bar.imp().message_generation.get();
    bar.push_message("msg", MessageKind::Info);
    assert_eq!(bar.imp().message_generation.get(), gen_before + 1);
}

#[test]
fn test_multiple_pushes_increment_generation() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.push_message("first", MessageKind::Info);
    bar.push_message("second", MessageKind::Warning);
    bar.push_message("third", MessageKind::Error);
    assert_eq!(bar.imp().message_generation.get(), 3);
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

// --- Encoding label ---

#[test]
fn test_encoding_label_shows_utf8() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert_eq!(bar.imp().encoding_label.label().as_str(), "UTF-8");
}
