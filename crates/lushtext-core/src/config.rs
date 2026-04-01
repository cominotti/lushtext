// SPDX-License-Identifier: GPL-3.0-or-later

//! Compile-time application constants.

pub const APP_ID: &str = "dev.cominotti.lushtext";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// GSettings key names (must match `data/dev.cominotti.lushtext.gschema.xml`).
pub mod keys {
    pub const WORD_WRAP: &str = "word-wrap";
    pub const SHOW_LINE_NUMBERS: &str = "show-line-numbers";
    pub const HIGHLIGHT_CURRENT_LINE: &str = "highlight-current-line";
    pub const TAB_WIDTH: &str = "tab-width";
    pub const INSERT_SPACES: &str = "insert-spaces-instead-of-tabs";
    pub const STYLE_SCHEME: &str = "style-scheme";
    pub const USE_SYSTEM_FONT: &str = "use-system-font";
    pub const CUSTOM_FONT: &str = "custom-font";
}
