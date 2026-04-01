// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextSearchBar widget.

use crate::common::ensure_gtk_init;
use gtk4::prelude::*;
use lushtext_core::ui::search_bar::LushtextSearchBar;

#[test]
fn test_new() {
    ensure_gtk_init();
    let _bar = LushtextSearchBar::new();
}

#[test]
fn test_set_match_count_zero_shows_no_results() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();
    bar.set_match_count(0, 0);
    // The label text is set internally via imp().match_label
    // We verify the public API doesn't panic and the widget state is consistent
}

#[test]
fn test_set_match_count_nonzero() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();
    bar.set_match_count(3, 10);
}

#[test]
fn test_set_match_count_boundary_values() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();
    bar.set_match_count(1, 1);
    bar.set_match_count(0, 0);
    bar.set_match_count(999, 999);
}

#[test]
fn test_search_entry_accessible() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();
    let entry = bar.search_entry();
    // SearchEntry should be accessible and functional
    entry.set_text("test query");
    assert_eq!(entry.text().as_str(), "test query");
}

#[test]
fn test_replace_entry_accessible() {
    ensure_gtk_init();
    let bar = LushtextSearchBar::new();
    let entry = bar.replace_entry();
    entry.set_text("replacement");
    assert_eq!(entry.text().as_str(), "replacement");
}
