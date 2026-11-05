// SPDX-License-Identifier: MIT OR Apache-2.0

//! A86: what a font or zoom change costs through a display `GtkCssProvider`,
//! by mechanism (`scope-font-zoom-restyle`).
//!
//! **A provider reload revalidates every widget.** After
//! `gtk_css_provider_load_from_string` on a provider added for the display,
//! even with unchanged text, the next frame revalidates the CSS of every
//! widget of every window, so its cost grows with the widgets the window
//! holds, whether or not any rule of the provider matches them. A load whose
//! rules are keyed by classes **no widget carries** changes no computed font
//! and re-lays out no text: it is the same revalidation and nothing else.
//!
//! **A class swap re-matches only where it is set.** With the zoom rules keyed
//! by a class (`.monospace.lz-N`, `textview.surface.lz-N`), swapping the class
//! on the font-following widgets themselves makes those widgets compute the
//! new font, and the next frame's cost does not grow with widgets outside the
//! set. A class on an **ancestor** (`.lz-N .monospace`) re-matches every
//! descendant of that ancestor (Adwaita's descendant selectors give the nodes
//! a parent-class dependency), and a class on the **window** is as costly as a
//! reload.
//!
//! The fixture is a 1280×800 window of three columns: 200 `.monospace` label
//! rows in a scroller, `UNRELATED_ROWS` (or no) unrelated rows (a `.toolbar`
//! box with a label and a `.dim-label`) in another, and a `GtkSourceView`
//! (word-char wrap, `monospace` off, class `tab-content-editor-surface`) with
//! 160 lines of 710 characters or empty. Each step alternates the zoom between
//! 100 % and 110 % of 11 pt and records the longest frame-painting dispatch
//! after it.
//!
//! Durations are the only observable of a revalidation (`GtkCssNode` is
//! private and `css_changed` is not bound), so two of the checks compare
//! medians, with wide margins: the reload's frame with 1,500 unrelated rows is
//! at least twice and 3 ms above the one without; the per-widget class swap
//! grows by less than half the reload's growth. Everything else is recorded.

use std::cell::Cell;
use std::time::Duration;

use gtk4::prelude::*;

use crate::observation::{Recorder, Stop};
use crate::probes::post_open::{
    DenseView, FIXED_PITCH_CONTROL, FrameCounter, REFERENCE_WINDOW, dense_text, fixed_pitch,
    median, watch_until_quiet,
};
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Unrelated rows in the populated window.
pub const UNRELATED_ROWS: usize = 1_500;
/// `.monospace` label rows that follow the font.
pub const MONO_ROWS: usize = 200;
/// The base font size, points.
pub const BASE_PT: f64 = 11.0;
/// Repetitions per mechanism; the median is recorded.
pub const REPS: usize = 5;
/// The editor surface class the zoom rules key on, as in LushText.
pub const SURFACE_CLASS: &str = "tab-content-editor-surface";

const QUIET: Duration = Duration::from_millis(60);
const BUDGET: Duration = Duration::from_secs(3);

/// How a step changes the font.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mechanism {
    /// `queue_draw` on the window: the frame floor.
    Redraw,
    /// Toggle the hover state of one unrelated row (an unrelated restyle).
    Hover,
    /// Reload the plain provider with unchanged text.
    ReloadSame,
    /// Reload the plain provider with the other zoom (LushText before A86).
    ReloadZoom,
    /// Swap the zoom class on the view and on each `.monospace` label.
    PerWidget,
    /// Swap the zoom class on the view and on the labels' scroller.
    Ancestor,
    /// Swap the zoom class on the window.
    WindowClass,
    /// Load rules keyed by classes no widget carries into a third provider.
    Unmatched,
}

impl Mechanism {
    /// Every mechanism, in the order a run measures them.
    pub const ALL: [Self; 8] = [
        Self::Redraw,
        Self::Hover,
        Self::ReloadSame,
        Self::ReloadZoom,
        Self::Unmatched,
        Self::PerWidget,
        Self::Ancestor,
        Self::WindowClass,
    ];

    /// A short label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Redraw => "redraw",
            Self::Hover => "hover",
            Self::ReloadSame => "reload same",
            Self::ReloadZoom => "reload zoom",
            Self::PerWidget => "per-widget class",
            Self::Ancestor => "ancestor class",
            Self::WindowClass => "window class",
            Self::Unmatched => "unmatched load",
        }
    }

    const fn class_prefix(self) -> Option<&'static str> {
        match self {
            Self::PerWidget => Some("lzl"),
            Self::Ancestor => Some("lza"),
            Self::WindowClass => Some("lzw"),
            _ => None,
        }
    }
}

fn points(zoom: u32) -> f64 {
    BASE_PT * f64::from(zoom) / 100.0
}

/// The rules LushText's font provider wrote before A86, for one zoom.
#[must_use]
pub fn plain_rules(zoom: u32) -> String {
    let pt = points(zoom);
    format!(
        ".monospace {{ font-family: \"Monospace\"; font-size: {pt:.1}pt; }}\n\
         textview.{SURFACE_CLASS}, textview.{SURFACE_CLASS} text {{ font-family: \"Monospace\"; \
         font-size: {pt:.1}pt; line-height: 1.18; }}\n"
    )
}

/// The class-keyed rules of every mechanism, 50–400 % in steps of 10, under
/// class `prefix-N` (`unmatched` uses a prefix no widget ever carries).
#[must_use]
pub fn class_rules(prefixes: &[&str]) -> String {
    let mut css = String::new();
    for zoom in (50..=400).step_by(10) {
        let pt = points(zoom);
        for prefix in prefixes {
            let font = format!("font-family: \"Monospace\"; font-size: {pt:.1}pt;");
            let rules = match *prefix {
                "lzl" | "unused" => format!(
                    ".monospace.{prefix}-{zoom} {{ {font} }}\n\
                     textview.{SURFACE_CLASS}.{prefix}-{zoom}, \
                     textview.{SURFACE_CLASS}.{prefix}-{zoom} text {{ {font} line-height: 1.18; }}\n"
                ),
                _ => format!(
                    ".{prefix}-{zoom} .monospace {{ {font} }}\n\
                     .{prefix}-{zoom} textview.{SURFACE_CLASS}, \
                     .{prefix}-{zoom} textview.{SURFACE_CLASS} text, \
                     textview.{SURFACE_CLASS}.{prefix}-{zoom}, \
                     textview.{SURFACE_CLASS}.{prefix}-{zoom} text {{ {font} line-height: 1.18; }}\n"
                ),
            };
            css.push_str(&rules);
        }
    }
    css
}

fn rows(count: usize, mono: bool, labels: &mut Vec<gtk4::Label>) -> gtk4::Box {
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    for row in 0..count {
        let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        line.add_css_class("toolbar");
        let label = gtk4::Label::new(Some(&format!("Row {row}")));
        if mono {
            label.add_css_class("monospace");
            labels.push(label.clone());
        }
        line.append(&label);
        let detail = gtk4::Label::new(Some("detail"));
        detail.add_css_class("dim-label");
        line.append(&detail);
        column.append(&line);
    }
    column
}

/// The presented window, its providers, and the widgets that follow the font.
pub struct FontScopeFixture {
    display: gtk4::gdk::Display,
    plain: gtk4::CssProvider,
    classes: gtk4::CssProvider,
    unmatched: gtk4::CssProvider,
    /// The editor-like view.
    pub dense: DenseView,
    /// The `.monospace` labels.
    pub labels: Vec<gtk4::Label>,
    mono_scroller: gtk4::ScrolledWindow,
    frames: FrameCounter,
    zoom: Cell<u32>,
    unmatched_flip: Cell<bool>,
    mode: Cell<Option<Mechanism>>,
    hover_row: Option<gtk4::Widget>,
    shown: Presented,
}

impl FontScopeFixture {
    fn new(recorder: &mut Recorder, unrelated: usize, text: bool) -> Result<Self, Stop> {
        let display = recorder.require(gtk4::gdk::Display::default(), "control: display")?;
        let priority = gtk4::STYLE_PROVIDER_PRIORITY_USER;
        let plain = gtk4::CssProvider::new();
        plain.load_from_string(&plain_rules(100));
        let classes = gtk4::CssProvider::new();
        classes.load_from_string(&class_rules(&["lzl", "lza", "lzw"]));
        let unmatched = gtk4::CssProvider::new();
        for provider in [&plain, &classes, &unmatched] {
            gtk4::style_context_add_provider_for_display(&display, provider, priority);
        }
        let mut labels = Vec::new();
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let mono_scroller = gtk4::ScrolledWindow::builder()
            .child(&rows(MONO_ROWS, true, &mut labels))
            .width_request(300)
            .vexpand(true)
            .build();
        let unrelated_rows = rows(unrelated, false, &mut Vec::new());
        let hover_row = unrelated_rows.first_child();
        let rest = gtk4::ScrolledWindow::builder()
            .child(&unrelated_rows)
            .width_request(200)
            .vexpand(true)
            .build();
        let dense = DenseView::new(gtk4::WrapMode::WordChar);
        dense.view.set_monospace(false);
        dense.view.add_css_class(SURFACE_CLASS);
        if text {
            dense.append(&dense_text(160, 710, 7));
        }
        root.append(&mono_scroller);
        root.append(&rest);
        root.append(&dense.scrolled);
        let shown = Presented::checked_sized(
            recorder,
            &root,
            REFERENCE_WINDOW,
            "control: the fixture window was presented",
        )?;
        settle(LAYOUT_SETTLE);
        let frames = recorder.require(FrameCounter::watch(&root), "control: frame clock")?;
        let fixture = Self {
            display,
            plain,
            classes,
            unmatched,
            dense,
            labels,
            mono_scroller,
            frames,
            zoom: Cell::new(100),
            unmatched_flip: Cell::new(false),
            mode: Cell::new(None),
            hover_row,
            shown,
        };
        fixture.quiet();
        Ok(fixture)
    }

    /// The fixture for a sample; `None` when the window never realized.
    #[must_use]
    pub fn standalone(unrelated: usize, text: bool) -> Option<Self> {
        Self::new(&mut Recorder::detached(), unrelated, text).ok()
    }

    fn quiet(&self) {
        let _ = watch_until_quiet(Some(&self.frames), QUIET, BUDGET, || 0.0);
    }

    fn class_targets(&self, mechanism: Mechanism) -> Vec<gtk4::Widget> {
        match mechanism {
            Mechanism::PerWidget => std::iter::once(self.dense.view.clone().upcast())
                .chain(self.labels.iter().map(|label| label.clone().upcast()))
                .collect(),
            Mechanism::Ancestor => vec![
                self.dense.view.clone().upcast(),
                self.mono_scroller.clone().upcast(),
            ],
            Mechanism::WindowClass => vec![self.shown.window().clone().upcast()],
            _ => Vec::new(),
        }
    }

    fn set_class(&self, mechanism: Mechanism, from: Option<u32>, to: Option<u32>) {
        let Some(prefix) = mechanism.class_prefix() else {
            return;
        };
        for widget in self.class_targets(mechanism) {
            if let Some(from) = from {
                widget.remove_css_class(&format!("{prefix}-{from}"));
            }
            if let Some(to) = to {
                widget.add_css_class(&format!("{prefix}-{to}"));
            }
        }
    }

    /// Put the fixture in `mechanism`'s starting state (zoom 100, the class
    /// set where the mechanism sets it, no other class) and let it settle.
    pub fn enter(&self, mechanism: Mechanism) {
        if let Some(previous) = self.mode.get() {
            self.set_class(previous, Some(self.zoom.get()), None);
        }
        self.plain.load_from_string(&plain_rules(100));
        self.zoom.set(100);
        self.set_class(mechanism, None, Some(100));
        self.mode.set(Some(mechanism));
        self.quiet();
    }

    /// Apply one step of the current mechanism and return the longest
    /// frame-painting dispatch after it, µs.
    pub fn step(&self) -> u128 {
        let mechanism = self.mode.get().unwrap_or(Mechanism::Redraw);
        let next = if self.zoom.get() == 100 { 110 } else { 100 };
        match mechanism {
            Mechanism::Redraw => self.shown.window().queue_draw(),
            Mechanism::Hover => {
                if let Some(row) = &self.hover_row {
                    if row.state_flags().contains(gtk4::StateFlags::PRELIGHT) {
                        row.unset_state_flags(gtk4::StateFlags::PRELIGHT);
                    } else {
                        row.set_state_flags(gtk4::StateFlags::PRELIGHT, false);
                    }
                }
            }
            Mechanism::ReloadSame => self.plain.load_from_string(&plain_rules(self.zoom.get())),
            Mechanism::ReloadZoom => {
                self.plain.load_from_string(&plain_rules(next));
                self.zoom.set(next);
            }
            Mechanism::Unmatched => {
                let flip = !self.unmatched_flip.get();
                self.unmatched_flip.set(flip);
                // Different text each time, keyed by a class nobody carries.
                let prefixes: &[&str] = if flip {
                    &["unused"]
                } else {
                    &["unused", "lzx"]
                };
                self.unmatched.load_from_string(&class_rules(prefixes));
            }
            Mechanism::PerWidget | Mechanism::Ancestor | Mechanism::WindowClass => {
                self.set_class(mechanism, Some(self.zoom.get()), Some(next));
                self.zoom.set(next);
            }
        }
        watch_until_quiet(Some(&self.frames), QUIET, BUDGET, || 0.0)
            .iter()
            .filter(|watched| watched.dispatch.frames > 0)
            .map(|watched| watched.dispatch.micros)
            .max()
            .unwrap_or_default()
    }

    /// Replace the class-keyed provider's rules (task 3.4 of
    /// `scope-font-zoom-restyle`: an unrelated restyle with and without a
    /// large zoom rule set).
    pub fn set_class_rules(&self, text: &str) {
        self.classes.load_from_string(text);
        self.quiet();
    }

    /// The current zoom, percent.
    #[must_use]
    pub fn zoom(&self) -> u32 {
        self.zoom.get()
    }

    /// The view's Pango font size, Pango units.
    #[must_use]
    pub fn view_size(&self) -> i32 {
        pango_size(self.dense.view.upcast_ref())
    }

    /// The first label's Pango font size, Pango units.
    #[must_use]
    pub fn label_size(&self) -> i32 {
        self.labels
            .first()
            .map_or(0, |label| pango_size(label.upcast_ref()))
    }

    /// The view's adjustments' `upper`, a proxy for its laid-out extent.
    #[must_use]
    pub fn uppers(&self) -> (f64, f64) {
        self.dense.uppers()
    }

    /// The median of `reps` steps of `mechanism`, entered first.
    #[must_use]
    pub fn median_of(&self, mechanism: Mechanism, reps: usize) -> u128 {
        self.enter(mechanism);
        let costs: Vec<u128> = (0..reps).map(|_| self.step()).collect();
        median(&costs)
    }
}

impl Drop for FontScopeFixture {
    fn drop(&mut self) {
        for provider in [&self.plain, &self.classes, &self.unmatched] {
            gtk4::style_context_remove_provider_for_display(&self.display, provider);
        }
    }
}

fn pango_size(widget: &gtk4::Widget) -> i32 {
    widget
        .pango_context()
        .font_description()
        .map_or(0, |description| description.size())
}

/// Medians of every mechanism in one fixture, µs, in [`Mechanism::ALL`] order.
#[must_use]
pub fn measure_all(fixture: &FontScopeFixture, reps: usize) -> Vec<(Mechanism, u128)> {
    Mechanism::ALL
        .iter()
        .map(|mechanism| (*mechanism, fixture.median_of(*mechanism, reps)))
        .collect()
}

/// Format medians as `label value` pairs.
#[must_use]
pub fn describe(medians: &[(Mechanism, u128)]) -> String {
    medians
        .iter()
        .map(|(mechanism, micros)| format!("{} {micros}", mechanism.label()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn cost(medians: &[(Mechanism, u128)], mechanism: Mechanism) -> u128 {
    medians
        .iter()
        .find(|(m, _)| *m == mechanism)
        .map_or(0, |(_, micros)| *micros)
}

/// Run the probe.
#[must_use]
pub fn probe_a86() -> Observation {
    Recorder::run(AxiomId::new(86), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        // Semantics on the small window with text first.
        let small = FontScopeFixture::new(recorder, 0, true)?;
        recorder.control(fixed_pitch(&small.dense.view, None), FIXED_PITCH_CONTROL)?;
        small.enter(Mechanism::PerWidget);
        let (view_100, label_100) = (small.view_size(), small.label_size());
        let _ = small.step();
        let (view_110, label_110) = (small.view_size(), small.label_size());
        recorder.measure(
            "per_widget_sizes",
            format!("view {view_100} -> {view_110}, label {label_100} -> {label_110}"),
        );
        recorder.control(view_100 > 0 && label_100 > 0, "control: fonts resolved")?;
        recorder.axiom(
            view_110 > view_100 && label_110 > label_100,
            "a per-widget class swap applies the zoomed size to the view and the labels",
        )?;
        small.enter(Mechanism::Unmatched);
        let before = (small.view_size(), small.uppers());
        let _ = small.step();
        let after = (small.view_size(), small.uppers());
        recorder.measure(
            "unmatched_load",
            format!(
                "view size {} -> {}, uppers {:?} -> {:?}",
                before.0, after.0, before.1, after.1
            ),
        );
        recorder.axiom(
            before == after,
            "a load whose rules match no widget changes no font size and no layout extent",
        )?;
        drop(small);

        // An unrelated restyle (a redraw, a hover change) with no class rules
        // and with a rule set the size of LushText's largest (72 levels).
        {
            let populated = FontScopeFixture::new(recorder, UNRELATED_ROWS, true)?;
            let mut cells = Vec::new();
            for (label, rules) in [
                ("none", String::new()),
                ("72 levels", class_rules(&["lzl", "unused"])),
                ("none again", String::new()),
            ] {
                populated.set_class_rules(&rules);
                let redraw = populated.median_of(Mechanism::Redraw, REPS);
                let hover = populated.median_of(Mechanism::Hover, REPS);
                cells.push(format!("{label}: redraw {redraw}, hover {hover}"));
            }
            recorder.measure("unrelated_restyle_us_by_rule_set", cells.join("; "));
        }

        let mut table = Vec::new();
        for (unrelated, text) in [
            (0, true),
            (UNRELATED_ROWS, true),
            (0, false),
            (UNRELATED_ROWS, false),
        ] {
            let fixture = FontScopeFixture::new(recorder, unrelated, text)?;
            let medians = measure_all(&fixture, REPS);
            table.push(((unrelated, text), medians));
        }
        for ((unrelated, text), medians) in &table {
            recorder.measure(
                match (unrelated, text) {
                    (0, true) => "medians_us_text_0",
                    (_, true) => "medians_us_text_1500",
                    (0, false) => "medians_us_empty_0",
                    _ => "medians_us_empty_1500",
                },
                describe(medians),
            );
        }
        let text_small = &table[0].1;
        let text_large = &table[1].1;
        let reload = (
            cost(text_small, Mechanism::ReloadSame),
            cost(text_large, Mechanism::ReloadSame),
        );
        let swap = (
            cost(text_small, Mechanism::PerWidget),
            cost(text_large, Mechanism::PerWidget),
        );
        let reload_growth = reload.1.saturating_sub(reload.0);
        let swap_growth = swap.1.saturating_sub(swap.0);
        recorder.measure(
            "growth_us",
            format!("reload same {reload_growth}, per-widget class {swap_growth}"),
        );
        recorder.axiom(
            reload.1 >= reload.0.saturating_mul(2) && reload_growth >= 3_000,
            "a reload with unchanged text grows with the unrelated widgets",
        )?;
        recorder.axiom(
            swap_growth.saturating_mul(2) < reload_growth,
            "a per-widget class swap does not grow with the unrelated widgets",
        )?;
        recorder.axiom(
            cost(text_small, Mechanism::Unmatched) < cost(text_small, Mechanism::ReloadZoom),
            "an unmatched load costs less than a reload that re-lays out the text",
        )
    })
}
