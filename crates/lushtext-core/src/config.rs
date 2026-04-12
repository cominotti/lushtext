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
    pub const USE_EDITORCONFIG: &str = "use-editorconfig";
    pub const COLOR_SCHEME: &str = "color-scheme";
    pub const STYLE_SCHEME: &str = "style-scheme";
    pub const USE_SYSTEM_FONT: &str = "use-system-font";
    pub const CUSTOM_FONT: &str = "custom-font";
    pub const ZOOM_LEVEL: &str = "zoom-level";

    // Window state
    pub const WINDOW_WIDTH: &str = "window-width";
    pub const WINDOW_HEIGHT: &str = "window-height";
    pub const WINDOW_MAXIMIZED: &str = "window-maximized";
    pub const SPLIT_VIEW_LAYOUT_MIGRATED: &str = "split-view-layout-migrated";
    pub const WORKSPACE_SIDEBAR_VISIBLE: &str = "workspace-sidebar-visible";
    pub const WORKSPACE_SIDEBAR_WIDTH_FRACTION: &str = "workspace-sidebar-width-fraction";
    pub const WORKSPACE_AUTO_COLLAPSE: &str = "workspace-auto-collapse";
    pub const PROPERTIES_SIDEBAR_VISIBLE: &str = "properties-sidebar-visible";
    pub const PROPERTIES_SIDEBAR_WIDTH_FRACTION: &str = "properties-sidebar-width-fraction";
    pub const SIDEBAR_POSITION: &str = "sidebar-position";
    pub const SIDEBAR_VISIBLE: &str = "sidebar-visible";
    pub const PREVIEW_PANE_POSITION: &str = "preview-pane-position";
    pub const PREVIEW_PANE_VISIBLE: &str = "preview-pane-visible";
    pub const SEARCH_PANEL_VISIBLE: &str = "search-panel-visible";
    pub const SEARCH_CASE_SENSITIVE: &str = "search-case-sensitive";
    pub const SEARCH_REGEX: &str = "search-regex";
    pub const SEARCH_WHOLE_WORD: &str = "search-whole-word";
    pub const SEARCH_PANEL_OPTIONS_EXPANDED: &str = "search-panel-options-expanded";
    pub const SEARCH_GITIGNORE: &str = "search-gitignore";
}
