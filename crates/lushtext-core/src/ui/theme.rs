// SPDX-License-Identifier: GPL-3.0-or-later

//! Display-wide GTK and GtkSourceView theme setup.
//!
//! Application startup installs the static CSS, the font providers (through
//! `ui/font_zoom.rs`, which owns the zoom classes), the icon theme,
//! and bundled GtkSourceView schemes here. Editor and preview widgets also use
//! this module to resolve the active source palette for transparent document
//! surfaces.

use gtk4::gio;
use sourceview5::prelude::*;

use crate::config;

/// Resolved editor-surface colors used by tab-content transparency styling.
#[derive(Clone, Debug)]
pub(crate) struct TabContentPalette {
    /// Base editor document background from the active GtkSourceView style scheme.
    pub text_bg: gdk4::RGBA,
    /// Gutter background from the active GtkSourceView style scheme.
    pub line_numbers_bg: gdk4::RGBA,
    /// Current-line highlight background from the active GtkSourceView style scheme.
    pub current_line_bg: gdk4::RGBA,
    /// Current-line number background from the active GtkSourceView style scheme.
    pub current_line_number_bg: gdk4::RGBA,
    /// Right-margin background from the active GtkSourceView style scheme.
    pub right_margin_bg: gdk4::RGBA,
    /// Selected document-surface opacity, clamped to the supported range.
    pub opacity: f64,
}

/// Prepend the app-bundled GtkSourceView style-scheme resources so the shipped
/// default schemes remain stable even when the host system lacks or changes the
/// platform-provided `map-overlay` definitions.
pub(crate) fn register_sourceview_style_schemes() {
    let manager = sourceview5::StyleSchemeManager::default();
    let resource_path = "resource:///dev/cominotti/lushtext/gtksourceview/styles";
    if !manager
        .search_path()
        .iter()
        .any(|path| path.as_str() == resource_path)
    {
        manager.prepend_search_path(resource_path);
    }
}

/// Add the app's bundled icon resources to the display icon theme so dev runs
/// can resolve the application icon without an install step.
///
/// # Panics
///
/// Panics if no display is available, because icon-theme registration only
/// makes sense after GTK startup has created the default display.
pub(crate) fn register_app_icons() {
    let display = gdk4::Display::default().expect("display");
    let theme = gtk4::IconTheme::for_display(&display);
    if !theme
        .resource_path()
        .iter()
        .any(|path| path.as_str() == config::RESOURCE_ICON_PATH)
    {
        theme.add_resource_path(config::RESOURCE_ICON_PATH);
    }
}

/// Load the application CSS and set up the font customization provider.
/// Must be called after GTK is initialized (i.e. during startup).
pub(crate) fn load_css() {
    let display = gdk4::Display::default().expect("display");

    let css_provider = gtk4::CssProvider::new();
    css_provider.load_from_resource("/dev/cominotti/lushtext/style/style.css");
    gtk4::style_context_add_provider_for_display(
        &display,
        &css_provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // Font rules (`scope-font-zoom-restyle`, ledger A86): two display
    // providers at user priority, loaded once per base font with a rule set
    // per zoom level keyed by a class; a zoom step swaps that class on the
    // registered font-following widgets (`ui/font_zoom.rs`) instead of
    // reloading, because any reload revalidates every widget of every window.
    // The main editor surface always gets the glyph-safe line height, while
    // auxiliary `.monospace` widgets keep GTK's defaults with the system font
    // at 100 %. The editor keeps GtkTextView `monospace` disabled to avoid
    // active-line glyph clipping in GTK 4.22 / GtkSourceView 5.20.
    let settings = crate::ui::settings_writes::shared();
    let first_install = !crate::ui::font_zoom::is_installed();
    crate::ui::font_zoom::install(&display);
    apply_font(&settings);
    if first_install {
        for key in [
            config::keys::USE_SYSTEM_FONT,
            config::keys::CUSTOM_FONT,
            config::keys::ZOOM_LEVEL,
        ] {
            settings.connect_changed(Some(key), move |settings, _| apply_font(settings));
        }
        if let Some(iface) = desktop_interface_settings() {
            let s = settings.clone();
            let iface_keepalive = iface.clone();
            iface.connect_changed(Some("monospace-font-name"), move |_, _| {
                // Keep the desktop Settings object alive for the app lifetime;
                // the font providers are display-wide, so this signal is too.
                let _keepalive = &iface_keepalive;
                if s.boolean(config::keys::USE_SYSTEM_FONT) {
                    apply_font(&s);
                }
            });
        }
    }

    // Tab-content transparency uses one display-wide provider because the
    // setting is global and the CSS only becomes meaningful once widgets add
    // the specific surface classes from their templates.
    let transparency_provider = gtk4::CssProvider::new();
    gtk4::style_context_add_provider_for_display(
        &display,
        &transparency_provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
    );
    apply_tab_content_transparency_css(&transparency_provider, &settings);
    for key in [
        config::keys::STYLE_SCHEME,
        config::keys::TAB_CONTENT_OPACITY,
    ] {
        let p = transparency_provider.clone();
        let s = settings.clone();
        settings.connect_changed(Some(key), move |_, _| {
            apply_tab_content_transparency_css(&p, &s);
        });
    }
    {
        let p = transparency_provider;
        let s = settings;
        libadwaita::StyleManager::default().connect_dark_notify(move |_| {
            apply_tab_content_transparency_css(&p, &s);
        });
    }
}

/// Resolve the base font and zoom from GSettings and hand them to the font
/// zoom registry: a class swap on a known zoom, a provider load on a new base
/// font or zoom lattice (`ui/font_zoom.rs`).
fn apply_font(settings: &gio::Settings) {
    crate::ui::font_zoom::apply(
        &base_font(settings),
        settings.uint(config::keys::ZOOM_LEVEL),
    );
}

/// The base font: the desktop monospace font, or the user's custom font.
fn base_font(settings: &gio::Settings) -> crate::ui::font_zoom::policy::BaseFont {
    let use_system = settings.boolean(config::keys::USE_SYSTEM_FONT);
    // Guard against non-GNOME desktops where the schema may not exist
    // (gio::Settings::new aborts if the schema is missing).
    let desc = if use_system {
        desktop_interface_settings().map_or_else(
            || pango::FontDescription::from_string("Monospace 11"),
            |iface| pango::FontDescription::from_string(&iface.string("monospace-font-name")),
        )
    } else {
        pango::FontDescription::from_string(&settings.string(config::keys::CUSTOM_FONT))
    };
    let family = desc.family().unwrap_or_else(|| "Monospace".into());
    // Pango stores font sizes in 1/1024 pt (PANGO_SCALE); divide to get CSS-compatible points.
    let raw = f64::from(desc.size()) / f64::from(pango::SCALE);
    crate::ui::font_zoom::policy::BaseFont {
        family: family.to_string(),
        points: if raw > 0.0 { raw } else { 11.0 },
        system: use_system,
    }
}

/// Return GNOME's interface settings when the schema exists on this desktop.
fn desktop_interface_settings() -> Option<gio::Settings> {
    let source = gio::SettingsSchemaSource::default()?;
    if source.lookup("org.gnome.desktop.interface", true).is_some() {
        Some(gio::Settings::new("org.gnome.desktop.interface"))
    } else {
        None
    }
}

/// Resolve the active GtkSourceView style scheme after light or dark selection.
#[must_use]
pub(crate) fn active_sourceview_scheme(
    settings: &gio::Settings,
) -> Option<sourceview5::StyleScheme> {
    let base_id = settings.string(config::keys::STYLE_SCHEME);
    let style_manager = libadwaita::StyleManager::default();
    let scheme_manager = sourceview5::StyleSchemeManager::default();

    if style_manager.is_dark() {
        let dark_id = format!("{base_id}-dark");
        scheme_manager
            .scheme(&dark_id)
            .or_else(|| scheme_manager.scheme(&base_id))
            .or_else(|| scheme_manager.scheme("Adwaita-dark"))
            .or_else(|| scheme_manager.scheme("Adwaita"))
    } else {
        scheme_manager
            .scheme(&base_id)
            .or_else(|| scheme_manager.scheme("Adwaita"))
    }
}

/// Collect the current document-surface palette from the active style scheme.
#[must_use]
pub(crate) fn resolve_tab_content_palette(settings: &gio::Settings) -> TabContentPalette {
    let dark = libadwaita::StyleManager::default().is_dark();
    let fallback_text = if dark { "#141414" } else { "#FAFAFA" };
    let fallback_line_numbers = if dark { "#1e1e1e" } else { "#F6F5F4" };
    let scheme = active_sourceview_scheme(settings);
    let opacity = settings
        .double(config::keys::TAB_CONTENT_OPACITY)
        .clamp(0.0, 1.0);

    let text_bg = scheme
        .as_ref()
        .and_then(|scheme| style_background_rgba(scheme, "text"))
        .unwrap_or_else(|| parse_css_rgba(fallback_text));
    let line_numbers_bg = scheme
        .as_ref()
        .and_then(|scheme| style_background_rgba(scheme, "line-numbers"))
        .unwrap_or_else(|| parse_css_rgba(fallback_line_numbers));
    let current_line_bg = scheme
        .as_ref()
        .and_then(|scheme| style_background_rgba(scheme, "current-line"))
        .unwrap_or(text_bg);
    let current_line_number_bg = scheme
        .as_ref()
        .and_then(|scheme| style_background_rgba(scheme, "current-line-number"))
        .unwrap_or(current_line_bg);
    let right_margin_bg = scheme
        .as_ref()
        .and_then(|scheme| style_background_rgba(scheme, "right-margin"))
        .unwrap_or(text_bg);

    TabContentPalette {
        text_bg,
        line_numbers_bg,
        current_line_bg,
        current_line_number_bg,
        right_margin_bg,
        opacity,
    }
}

/// Render one RGBA color for CSS strings while keeping the original RGB triplet.
#[must_use]
pub(crate) fn css_rgba_with_alpha(color: &gdk4::RGBA, alpha: f64) -> String {
    format!(
        "rgba({:.0}, {:.0}, {:.0}, {:.3})",
        color.red() * 255.0,
        color.green() * 255.0,
        color.blue() * 255.0,
        alpha.clamp(0.0, 1.0)
    )
}

/// Render one RGBA color for GtkSourceView style-scheme XML.
#[must_use]
pub(crate) fn sourceview_rgba_with_alpha(color: &gdk4::RGBA, alpha: f64) -> String {
    format!(
        "#rgba({:.0},{:.0},{:.0},{:.3})",
        color.red() * 255.0,
        color.green() * 255.0,
        color.blue() * 255.0,
        alpha.clamp(0.0, 1.0)
    )
}

/// Extract one style background from the active GtkSourceView style scheme.
fn style_background_rgba(scheme: &sourceview5::StyleScheme, style_id: &str) -> Option<gdk4::RGBA> {
    let style = scheme.style(style_id)?;
    let spec = style.background().or_else(|| style.line_background())?;
    Some(parse_css_rgba(spec.as_str()))
}

/// Parse one CSS color specification into a concrete RGBA value.
fn parse_css_rgba(spec: &str) -> gdk4::RGBA {
    gdk4::RGBA::parse(spec)
        .unwrap_or_else(|_| gdk4::RGBA::parse("#000000").expect("hardcoded color parses"))
}

/// Keep document surfaces translucent while the shell chrome stays explicitly opaque.
fn apply_tab_content_transparency_css(provider: &gtk4::CssProvider, settings: &gio::Settings) {
    let palette = resolve_tab_content_palette(settings);
    let document_bg = css_rgba_with_alpha(&palette.text_bg, palette.opacity);
    let gutter_bg = css_rgba_with_alpha(&palette.line_numbers_bg, palette.opacity);
    let preview_bg = document_bg.clone();
    let minimap_bg = css_rgba_with_alpha(&palette.text_bg, 1.0);

    let css = format!(
        r#"
window {{
  background-color: transparent;
}}

.header-chrome-opaque {{
  background-color: @headerbar_bg_color;
}}

.side-rail-chrome-opaque {{
  background-color: @headerbar_bg_color;
}}

.shell-chrome-opaque,
.empty-state-opaque,
.preview-placeholder-opaque,
.search-bar-container {{
  background-color: @window_bg_color;
}}

textview.tab-content-editor-surface,
textview.tab-content-editor-surface text,
textview.tab-content-editor-surface border.top,
textview.tab-content-editor-surface border.right,
textview.tab-content-editor-surface border.bottom {{
  background-color: {document_bg};
}}

textview.tab-content-editor-surface border.left {{
  background-color: {gutter_bg};
}}

textview.tab-content-preview-surface,
textview.tab-content-preview-surface text,
textview.tab-content-preview-surface border.top,
textview.tab-content-preview-surface border.right,
textview.tab-content-preview-surface border.bottom,
textview.tab-content-preview-surface border.left {{
  background-color: {preview_bg};
}}

.minimap-shell,
.minimap-shell textview.GtkSourceMap,
.minimap-view,
.minimap-view text,
.minimap-view border.top,
.minimap-view border.right,
.minimap-view border.bottom,
.minimap-view border.left {{
  background-color: {minimap_bg};
}}
"#
    );
    provider.load_from_string(&css);
}
