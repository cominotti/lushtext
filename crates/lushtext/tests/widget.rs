// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget tests for LushText GTK4 UI components.
//!
//! These tests require a display server. For headless environments, use
//! `mutter --headless` — see `.github/workflows/ci.yml` for the full invocation.
//! They verify widget construction, property behavior, and signal wiring.

#[path = "widget/common.rs"]
mod common;

#[path = "widget/file_tree_item.rs"]
mod file_tree_item;

#[path = "widget/search_bar.rs"]
mod search_bar;

#[path = "widget/search_panel.rs"]
mod search_panel;

#[path = "widget/status_bar.rs"]
mod status_bar;

#[path = "widget/editor_page.rs"]
mod editor_page;

#[path = "widget/sidebar.rs"]
mod sidebar;

#[path = "widget/workspace_section.rs"]
mod workspace_section;

#[path = "widget/window.rs"]
mod window;

#[path = "widget/command_palette.rs"]
mod command_palette;

#[path = "widget/markdown_preview.rs"]
mod markdown_preview;

#[path = "widget/app.rs"]
mod app;

#[path = "widget/preferences.rs"]
mod preferences;
