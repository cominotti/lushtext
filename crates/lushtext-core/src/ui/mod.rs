// SPDX-License-Identifier: GPL-3.0-or-later

//! GTK4/Libadwaita UI widgets.
//!
//! Each widget follows the two-module GObject pattern: `mod.rs` (public wrapper
//! type + API) and `imp.rs` (private struct + trait implementations).

pub(crate) mod buffer_snapshot;
pub mod command_palette;
pub mod editor_page;
pub mod info_bar;
pub mod markdown_preview;
pub mod preferences;
pub mod properties_panel;
pub mod search_bar;
pub mod search_panel;
pub mod shrinkable_bin;
pub mod sidebar;
pub mod status_bar;
pub mod window;
