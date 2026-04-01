// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for LushtextPreferences dialog.

use crate::common::ensure_gtk_init;
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
