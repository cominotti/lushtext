// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget tests for LushText GTK4 UI components.
//!
//! These tests require a display server (or `xvfb-run` for headless execution).
//! They verify widget construction, property behavior, and signal wiring.
//!
//! Run with: `make test-widget` or `xvfb-run make test-widget`

#[path = "widget/common.rs"]
mod common;

#[path = "widget/file_tree_item.rs"]
mod file_tree_item;

#[path = "widget/search_bar.rs"]
mod search_bar;

#[path = "widget/status_bar.rs"]
mod status_bar;

#[path = "widget/editor_page.rs"]
mod editor_page;

#[path = "widget/sidebar.rs"]
mod sidebar;

#[path = "widget/window.rs"]
mod window;

#[path = "widget/app.rs"]
mod app;

#[path = "widget/preferences.rs"]
mod preferences;
