// SPDX-License-Identifier: GPL-3.0-or-later

//! A cross-cutting module with no role (`WFR-FONT-ZOOM`): the GTK-free half of `ui/font_zoom.rs`: which zoom levels a font provider
//! carries, the class that selects one, and the provider's rule text
//! (`scope-font-zoom-restyle`, design D1).
//!
//! The rules for one zoom level are exactly the declarations the display
//! provider wrote before this change (`apply_font_css`), keyed by a class:
//!
//! - `textview.tab-content-editor-surface.<class>` and its `text` node: the
//!   family, the zoomed size and the glyph-safe line height, always;
//! - `.monospace.<class>`: the family and the zoomed size, except with the
//!   system font at 100 %, where auxiliary monospace widgets keep GTK's
//!   default font.

/// Lowest zoom the UI exposes, percent (as `ui/window/zoom.rs`).
pub const ZOOM_MIN: u32 = 50;
/// Highest zoom the UI exposes, percent.
pub const ZOOM_MAX: u32 = 400;
/// The zoom actions' step, percent: a level steps along its residue modulo
/// this value.
pub const ZOOM_STEP: u32 = 10;
/// The neutral zoom, percent.
pub const ZOOM_DEFAULT: u32 = 100;

/// Main-editor line-height multiplier that leaves room for active-line top ink.
///
/// `1.18` is the smallest value found by the live bracket smoke to preserve
/// Adwaita Mono square-bracket caps on GTK 4.22 / GtkSourceView 5.20 without
/// making ordinary editor rows look loose.
pub const EDITOR_GLYPH_LINE_HEIGHT: f64 = 1.18;

/// The base font every zoom level scales: the family, its size in points, and
/// whether it is the desktop's monospace font.
#[derive(Clone, Debug, PartialEq)]
pub struct BaseFont {
    /// The CSS font family.
    pub family: String,
    /// The unzoomed size, points (positive).
    pub points: f64,
    /// Whether the desktop font is used (`use-system-font`).
    pub system: bool,
}

/// A zoom level clamped to the range the UI exposes.
#[must_use]
pub const fn clamp_zoom(zoom: u32) -> u32 {
    if zoom < ZOOM_MIN {
        ZOOM_MIN
    } else if zoom > ZOOM_MAX {
        ZOOM_MAX
    } else {
        zoom
    }
}

/// The residue modulo `ZOOM_STEP` a level steps along, `None` on the menu grid.
#[must_use]
pub const fn lattice_of(zoom: u32) -> Option<u32> {
    match clamp_zoom(zoom) % ZOOM_STEP {
        0 => None,
        residue => Some(residue),
    }
}

/// Whether a provider carrying `lattice` (beside the grid) has rules for `zoom`.
#[must_use]
pub const fn carries(lattice: Option<u32>, zoom: u32) -> bool {
    match lattice_of(zoom) {
        None => true,
        Some(residue) => matches!(lattice, Some(carried) if carried == residue),
    }
}

/// The zoom set a provider loaded for `lattice` carries, ascending: the menu's
/// grid (50–400 in steps of 10) and, off the grid, every level of the lattice
/// within the range. At most 72 levels.
#[must_use]
pub fn zoom_set(lattice: Option<u32>) -> Vec<u32> {
    let grid = (ZOOM_MIN..=ZOOM_MAX).step_by(ZOOM_STEP as usize);
    let mut set: Vec<u32> = match lattice {
        None => grid.collect(),
        Some(residue) => grid
            .chain((ZOOM_MIN + residue..=ZOOM_MAX).step_by(ZOOM_STEP as usize))
            .collect(),
    };
    set.sort_unstable();
    set
}

/// The class that selects `zoom` in generation `generation`'s rules.
#[must_use]
pub fn zoom_class(generation: u64, zoom: u32) -> String {
    format!("lushtext-font-{generation}-zoom-{}", clamp_zoom(zoom))
}

/// The zoomed size, points, as the provider rounds it (one decimal).
fn zoomed_points(base: &BaseFont, zoom: u32) -> f64 {
    base.points * f64::from(clamp_zoom(zoom)) / 100.0
}

/// The editor surface's declarations at `zoom`.
#[must_use]
pub fn editor_declarations(base: &BaseFont, zoom: u32) -> String {
    let family = &base.family;
    let points = zoomed_points(base, zoom);
    format!(
        "font-family: \"{family}\"; font-size: {points:.1}pt; line-height: {EDITOR_GLYPH_LINE_HEIGHT:.2};"
    )
}

/// The `.monospace` widgets' declarations at `zoom`, `None` where they keep
/// GTK's default font (the system font at 100 %).
#[must_use]
pub fn monospace_declarations(base: &BaseFont, zoom: u32) -> Option<String> {
    if base.system && clamp_zoom(zoom) == ZOOM_DEFAULT {
        return None;
    }
    let family = &base.family;
    let points = zoomed_points(base, zoom);
    Some(format!(
        "font-family: \"{family}\"; font-size: {points:.1}pt;"
    ))
}

/// The provider text of generation `generation` for `base` and the zoom set of
/// `lattice`.
#[must_use]
pub fn provider_text(generation: u64, base: &BaseFont, lattice: Option<u32>) -> String {
    let mut css = String::new();
    for zoom in zoom_set(lattice) {
        let class = zoom_class(generation, zoom);
        let editor = editor_declarations(base, zoom);
        css.push_str(&format!(
            "textview.tab-content-editor-surface.{class}, textview.tab-content-editor-surface.{class} text {{ {editor} }}\n"
        ));
        if let Some(monospace) = monospace_declarations(base, zoom) {
            css.push_str(&format!(".monospace.{class} {{ {monospace} }}\n"));
        }
    }
    css
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `apply_font_css` before `scope-font-zoom-restyle`, transcribed.
    fn legacy_css(base: &BaseFont, zoom: u32) -> String {
        let zoom = zoom.clamp(50, 400);
        let family = &base.family;
        let zoomed_pt = base.points * f64::from(zoom) / 100.0;
        let editor_css = format!(
            "textview.tab-content-editor-surface, textview.tab-content-editor-surface text {{ font-family: \"{family}\"; font-size: {zoomed_pt:.1}pt; line-height: {EDITOR_GLYPH_LINE_HEIGHT:.2}; }}"
        );
        if base.system && zoom == 100 {
            editor_css
        } else {
            format!(
                ".monospace {{ font-family: \"{family}\"; font-size: {zoomed_pt:.1}pt; }}\n{editor_css}"
            )
        }
    }

    /// The declarations of the legacy rule for `selector`, if it has one.
    fn legacy_declarations(css: &str, selector: &str) -> Option<String> {
        css.lines()
            .find(|line| line.starts_with(selector))
            .and_then(|line| {
                let open = line.find('{')?;
                let close = line.rfind('}')?;
                Some(line[open + 1..close].trim().to_owned())
            })
    }

    fn bases() -> Vec<BaseFont> {
        vec![
            BaseFont {
                family: "Monospace".into(),
                points: 11.0,
                system: true,
            },
            BaseFont {
                family: "Adwaita Mono".into(),
                points: 11.0,
                system: false,
            },
            BaseFont {
                family: "Source Code Pro".into(),
                points: 13.5,
                system: false,
            },
        ]
    }

    #[test]
    fn every_zoom_class_reproduces_the_legacy_declarations() {
        for base in bases() {
            for lattice in [None, Some(5), Some(3)] {
                for zoom in zoom_set(lattice) {
                    let legacy = legacy_css(&base, zoom);
                    assert_eq!(
                        legacy_declarations(&legacy, "textview.tab-content-editor-surface"),
                        Some(editor_declarations(&base, zoom)),
                        "{base:?} {zoom}"
                    );
                    assert_eq!(
                        legacy_declarations(&legacy, ".monospace"),
                        monospace_declarations(&base, zoom),
                        "{base:?} {zoom}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_system_font_at_100_keeps_gtk_monospace_defaults() {
        let system = &bases()[0];
        let text = provider_text(1, system, None);
        assert!(!text.contains(".monospace.lushtext-font-1-zoom-100 "));
        assert!(text.contains(".monospace.lushtext-font-1-zoom-110 "));
        assert!(text.contains("textview.tab-content-editor-surface.lushtext-font-1-zoom-100,"));
        let custom = provider_text(1, &bases()[1], None);
        assert!(custom.contains(".monospace.lushtext-font-1-zoom-100 "));
    }

    #[test]
    fn the_grid_carries_the_menu_levels_only() {
        let grid = zoom_set(None);
        assert_eq!(grid.len(), 36);
        assert_eq!(grid.first(), Some(&50));
        assert_eq!(grid.last(), Some(&400));
        assert!(grid.iter().all(|zoom| zoom % 10 == 0));
    }

    #[test]
    fn an_off_grid_level_adds_its_whole_lattice() {
        assert_eq!(lattice_of(55), Some(5));
        let set = zoom_set(Some(5));
        assert_eq!(set.len(), 71);
        assert!(set.len() <= 72);
        for zoom in [55, 65, 75, 395, 400, 50] {
            assert!(set.contains(&zoom), "{zoom}");
        }
        // 55 -> 65 -> 75 needs no reload: the lattice carries every step.
        assert!([55, 65, 75].iter().all(|zoom| carries(Some(5), *zoom)));
        assert!(!carries(None, 55));
        assert!(!carries(Some(3), 55));
        assert!(carries(Some(5), 100));
        // Stepping from 395 clamps onto the grid.
        assert!(carries(Some(5), 400));
        let widest = zoom_set(Some(1));
        assert_eq!(widest.len(), 71);
    }

    #[test]
    fn clamping_matches_the_legacy_range() {
        assert_eq!(clamp_zoom(10), 50);
        assert_eq!(clamp_zoom(900), 400);
        assert_eq!(zoom_class(3, 900), "lushtext-font-3-zoom-400");
        assert_eq!(lattice_of(5), None, "below the range clamps to 50");
    }
}
