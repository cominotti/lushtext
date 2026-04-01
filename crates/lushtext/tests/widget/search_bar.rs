// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextSearchBar widget.

use crate::common::ensure_gtk_init;
use gtk4::prelude::*;
use lushtext_core::ui::search_bar::LushtextSearchBar;
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn test_new() {
    ensure_gtk_init();
    let _bar = LushtextSearchBar::new();
}

#[test]
fn test_default_equals_new() {
    ensure_gtk_init();
    let _bar: LushtextSearchBar = Default::default();
}

// --- Match count display ---

#[test]
fn test_set_match_count_zero_shows_no_results() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();
    bar.set_match_count(0, 0);
}

#[test]
fn test_set_match_count_nonzero() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();
    bar.set_match_count(3, 10);
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
    // Start with results, then clear, then add again
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
fn test_connect_close_fires_on_escape() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();

    let closed = Rc::new(Cell::new(false));
    let closed_clone = closed.clone();
    bar.connect_close(move || closed_clone.set(true));

    // Emit the stop-search signal (triggered by Escape key)
    bar.search_entry().emit_stop_search();
    assert!(closed.get());
}
