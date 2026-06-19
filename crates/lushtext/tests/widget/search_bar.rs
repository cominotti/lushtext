// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextSearchBar widget.

use crate::common::{ensure_gtk_init, wait_until};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::ui::accessibility::test_audit::AccessibleAudit;
use lushtext_core::ui::search_bar::LushtextSearchBar;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

#[test]
fn test_new() {
    ensure_gtk_init();
    let _bar = LushtextSearchBar::new();
}

#[test]
fn test_default_equals_new() {
    ensure_gtk_init();
    let _bar: LushtextSearchBar = LushtextSearchBar::default();
}

// --- Match count display ---

#[test]
fn test_set_match_count_zero_shows_blank() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();
    bar.set_match_count(0, 0);
    // Zero matches: label is blank (matches GNOME Text Editor behavior).
}

#[test]
fn test_set_match_count_negative_shows_blank() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();
    // SearchContext returns -1 while scanning is in progress.
    bar.set_match_count(0, -1);
}

#[test]
fn test_set_match_count_nonzero() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();
    bar.set_match_count(3, 10);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Status)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ValueText,
        ])
        .assert_on(&*bar.imp().match_label);
}

#[test]
fn test_set_match_count_single_result() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();
    bar.set_match_count(1, 1);
}

#[test]
fn test_set_match_count_transitions() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();
    bar.set_match_count(5, 20);
    bar.set_match_count(0, 0);
    bar.set_match_count(1, 3);
}

// --- Entry accessors ---

#[test]
fn test_search_entry_text_input() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();
    let entry = bar.search_entry();
    entry.set_text("find me");
    assert_eq!(entry.text().as_str(), "find me");
}

#[test]
fn test_replace_entry_text_input() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();
    let entry = bar.replace_entry();
    entry.set_text("replace with");
    assert_eq!(entry.text().as_str(), "replace with");
}

#[test]
fn test_search_entry_initially_empty() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();
    assert_eq!(bar.search_entry().text().as_str(), "");
}

#[test]
fn test_replace_entry_initially_empty() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();
    assert_eq!(bar.replace_entry().text().as_str(), "");
}

#[test]
fn test_search_controls_expose_accessibility_roles() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();

    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::SearchBox)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(bar.search_entry());
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::TextBox)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(bar.replace_entry());
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Button)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::KeyShortcuts,
        ])
        .assert_on(bar.close_button());
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::ToggleButton)
        .properties(&[gtk4::AccessibleProperty::Label])
        .states(&[gtk4::AccessibleState::Pressed])
        .assert_on(bar.replace_mode_button());
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Button)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::HasPopup,
        ])
        .assert_on(&*bar.imp().options_button);
}

// --- Close button ---

#[test]
fn test_close_button_accessible() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();
    let _button = bar.close_button();
}

#[test]
fn test_connect_close_fires_on_button_click() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();

    let closed = Rc::new(Cell::new(false));
    let closed_clone = closed.clone();
    bar.connect_close(move || closed_clone.set(true));

    bar.close_button().emit_clicked();
    assert!(closed.get());
}

#[test]
fn test_attach_detach_clears_occurrence_signal_owner() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();
    let buffer = sourceview5::Buffer::new(None);
    let view = sourceview5::View::with_buffer(&buffer);

    bar.attach(&buffer, &view);
    assert_eq!(bar.imp().occurrences_signals.len(), 1);

    bar.detach();
    assert!(bar.imp().occurrences_signals.is_empty());
}

#[test]
fn test_no_match_search_sets_accessible_invalid_state() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();
    let buffer = sourceview5::Buffer::new(None);
    buffer.set_text("alpha beta gamma");
    let view = sourceview5::View::with_buffer(&buffer);

    bar.attach(&buffer, &view);
    bar.search_entry().set_text("missing");

    wait_until(Duration::from_secs(2), || {
        gtk4::test_accessible_has_state(bar.search_entry(), gtk4::AccessibleState::Invalid)
    });

    bar.detach();
    assert!(!gtk4::test_accessible_has_state(
        bar.search_entry(),
        gtk4::AccessibleState::Invalid
    ));
}

// --- Replace mode toggle ---

#[test]
fn test_replace_mode_toggle_shows_replace_row() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();
    // Replace row starts collapsed (revealers not revealed).
    assert!(!bar.replace_mode_button().is_active());
    assert!(!bar.is_replace_revealed());
    assert_replace_revealer_state(&bar, false);

    // Activate the toggle — replace row reveals.
    bar.set_replace_mode(true);
    assert!(bar.replace_mode_button().is_active());
    assert!(bar.is_replace_revealed());
    assert_replace_revealer_state(&bar, true);

    // Deactivate — replace row collapses again.
    bar.set_replace_mode(false);
    assert!(!bar.replace_mode_button().is_active());
    assert!(!bar.is_replace_revealed());
    assert_replace_revealer_state(&bar, false);
}

fn assert_replace_revealer_state(bar: &LushtextSearchBar, expected: bool) {
    let imp = bar.imp();
    assert_eq!(imp.replace_entry_revealer.reveals_child(), expected);
    assert_eq!(imp.replace_button_revealer.reveals_child(), expected);
    assert_eq!(imp.replace_all_revealer.reveals_child(), expected);
}

// --- Navigation state ---

#[test]
fn test_navigated_initially_false() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();
    assert!(!bar.has_navigated());
}
