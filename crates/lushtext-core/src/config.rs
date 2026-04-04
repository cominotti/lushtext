// SPDX-License-Identifier: GPL-3.0-or-later

//! Compile-time application constants.

pub const APP_ID: &str = "dev.cominotti.lushtext";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Install-time data directory set by Meson via `cargo.sh`.
/// `Some` for installed/Flatpak builds, `None` for dev builds.
pub const PKGDATADIR: Option<&str> = option_env!("LUSHTEXT_PKGDATADIR");

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

    // Window state
    pub const WINDOW_WIDTH: &str = "window-width";
    pub const WINDOW_HEIGHT: &str = "window-height";
    pub const WINDOW_MAXIMIZED: &str = "window-maximized";
    pub const SIDEBAR_POSITION: &str = "sidebar-position";
    pub const SIDEBAR_VISIBLE: &str = "sidebar-visible";
}
