// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for LushtextApplication.

use crate::common::ensure_gtk_init;
use gio::prelude::*;
use lushtext_core::app::LushtextApplication;
use lushtext_core::config;

#[test]
fn test_new() {
    ensure_gtk_init();
    let _app = LushtextApplication::new();
}

#[test]
fn test_app_id() {
    ensure_gtk_init();
    let app = LushtextApplication::new();
    assert_eq!(app.application_id().unwrap().as_str(), config::APP_ID);
}

#[test]
fn test_handles_open_flag() {
    ensure_gtk_init();
    let app = LushtextApplication::new();
    assert!(app.flags().contains(gio::ApplicationFlags::HANDLES_OPEN));
}

#[test]
fn test_default_equals_new() {
    ensure_gtk_init();
    let _app: LushtextApplication = LushtextApplication::default();
}
