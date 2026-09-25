// SPDX-License-Identifier: MIT OR Apache-2.0

//! A17: how `AdwOverlaySplitView` sizes its sidebar from
//! `sidebar-width-fraction`, `min-sidebar-width`, and `max-sidebar-width`
//! (in `sidebar-width-unit`, sp by default), and whether toggling
//! `show-sidebar` or `collapsed` changes the toplevel window's width.
//!
//! The split view is a window's whole content, so its width is the window's.
//! Toolkit animations are off for the probe, so `show-sidebar` reaches its
//! end state in the frame it changes.

use gtk4::prelude::*;

use super::LAYOUT_SETTLE;
use super::adaptive::SettingsOverride;
use crate::observation::Recorder;
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

/// The window, and so the split view, width.
pub const WINDOW_WIDTH: i32 = 1_000;
/// The window height.
pub(crate) const WINDOW_HEIGHT: i32 = 500;
/// A text scale the unit steps run under, in permille.
pub const SCALED_TEXT: i32 = 1_500;
/// A sidebar minimum request larger than the fraction's width.
pub const SIDEBAR_REQUEST: i32 = 350;

/// The A17 fixture: an `AdwOverlaySplitView` with a label on each side.
#[derive(Clone, Debug)]
pub struct SplitViewFixture {
    /// The split view under observation.
    pub split: libadwaita::OverlaySplitView,
    /// Its sidebar child.
    pub sidebar: gtk4::Label,
    /// Its content child.
    pub content: gtk4::Label,
}

impl SplitViewFixture {
    /// Build the fixture with the split view's default sizing.
    #[must_use]
    pub fn new() -> Self {
        let sidebar = gtk4::Label::new(Some("sidebar"));
        let content = gtk4::Label::new(Some("content"));
        let split = libadwaita::OverlaySplitView::builder()
            .sidebar(&sidebar)
            .content(&content)
            .build();
        Self {
            split,
            sidebar,
            content,
        }
    }

    /// Set the split view's sizing: fraction, minimum, and maximum.
    pub fn size_sidebar(&self, fraction: f64, min: f64, max: f64) {
        self.split.set_sidebar_width_fraction(fraction);
        self.split.set_min_sidebar_width(min);
        self.split.set_max_sidebar_width(max);
    }
}

impl Default for SplitViewFixture {
    fn default() -> Self {
        Self::new()
    }
}

/// The widths one sizing step reads back after settling.
fn settled_widths(fixture: &SplitViewFixture) -> (i32, i32) {
    settle(LAYOUT_SETTLE);
    (fixture.sidebar.width(), fixture.content.width())
}

/// Probe A17. See the module documentation.
#[must_use]
pub fn probe_a17() -> Observation {
    Recorder::run(AxiomId::new(17), |recorder| {
        let settings = SettingsOverride::new();
        settings.disable_animations();
        settings.set_text_scale(1_000);
        let fixture = SplitViewFixture::new();
        recorder.measure("default_fraction", fixture.split.sidebar_width_fraction());
        recorder.measure("default_min", fixture.split.min_sidebar_width());
        recorder.measure("default_max", fixture.split.max_sidebar_width());
        recorder.measure(
            "default_unit",
            format!("{:?}", fixture.split.sidebar_width_unit()),
        );
        let shown = Presented::checked_sized(
            recorder,
            &fixture.split,
            (WINDOW_WIDTH, WINDOW_HEIGHT),
            "control: the fixture window realizes",
        )?;
        let window = shown.window();
        settle(LAYOUT_SETTLE);
        recorder.measure("window_width", window.width());
        recorder.measure("split_width", fixture.split.width());
        recorder.control(
            window.width() == WINDOW_WIDTH && fixture.split.width() == WINDOW_WIDTH,
            "control: the split view is as wide as the window",
        )?;

        let (default_sidebar, default_content) = settled_widths(&fixture);
        recorder.measure("default_sidebar", default_sidebar);
        recorder.measure("default_content", default_content);

        fixture.size_sidebar(0.25, 300.0, 400.0);
        let (min_wins, _) = settled_widths(&fixture);
        recorder.measure("fraction_0_25_min_300_max_400", min_wins);

        fixture.size_sidebar(0.25, 100.0, 200.0);
        let (max_wins, _) = settled_widths(&fixture);
        recorder.measure("fraction_0_25_min_100_max_200", max_wins);

        fixture.size_sidebar(0.3, 100.0, 500.0);
        let (fraction_wins, fraction_content) = settled_widths(&fixture);
        recorder.measure("fraction_0_3_min_100_max_500", fraction_wins);
        recorder.measure("fraction_0_3_content", fraction_content);

        fixture.size_sidebar(0.25, 280.0, 280.0);
        let (pinned, _) = settled_widths(&fixture);
        recorder.measure("fraction_0_25_min_max_280", pinned);

        // Units: the same sp minimum under a larger text scale, then in px.
        settings.set_text_scale(SCALED_TEXT);
        fixture.size_sidebar(0.25, 300.0, 400.0);
        fixture.split.queue_resize();
        let (scaled_min, _) = settled_widths(&fixture);
        recorder.measure("scaled_1500_min_300sp", scaled_min);
        fixture.size_sidebar(0.5, 100.0, 200.0);
        let (scaled_max, _) = settled_widths(&fixture);
        recorder.measure("scaled_1500_fraction_0_5_max_200sp", scaled_max);
        fixture
            .split
            .set_sidebar_width_unit(libadwaita::LengthUnit::Px);
        fixture.size_sidebar(0.25, 300.0, 400.0);
        let (px_min, _) = settled_widths(&fixture);
        recorder.measure("scaled_1500_min_300px", px_min);
        fixture
            .split
            .set_sidebar_width_unit(libadwaita::LengthUnit::Sp);
        settings.set_text_scale(1_000);

        // The sidebar's own minimum against a smaller fraction width.
        fixture.size_sidebar(0.25, 100.0, 280.0);
        fixture.sidebar.set_width_request(SIDEBAR_REQUEST);
        let (child_minimum, _) = settled_widths(&fixture);
        recorder.measure("sidebar_request_350_fraction_250", child_minimum);
        fixture.sidebar.set_width_request(-1);
        fixture.size_sidebar(0.25, 180.0, 280.0);
        let (rest_sidebar, rest_content) = settled_widths(&fixture);

        // Toggling show-sidebar and collapsed, and the toplevel's width.
        let (split_min_shown, _, _, _) = fixture.split.measure(gtk4::Orientation::Horizontal, -1);
        fixture.split.set_show_sidebar(false);
        let (_, hidden_content) = settled_widths(&fixture);
        let window_hidden = window.width();
        let (split_min_hidden, _, _, _) = fixture.split.measure(gtk4::Orientation::Horizontal, -1);
        fixture.split.set_show_sidebar(true);
        settle(LAYOUT_SETTLE);
        fixture.split.set_collapsed(true);
        let (collapsed_sidebar, collapsed_content) = settled_widths(&fixture);
        let window_collapsed = window.width();
        let (split_min_collapsed, _, _, _) =
            fixture.split.measure(gtk4::Orientation::Horizontal, -1);
        fixture.split.set_collapsed(false);
        let (_, uncollapsed_content) = settled_widths(&fixture);
        let window_restored = window.width();
        recorder.measure("rest_sidebar", rest_sidebar);
        recorder.measure("rest_content", rest_content);
        recorder.measure("split_min_shown", split_min_shown);
        recorder.measure("hidden_content", hidden_content);
        recorder.measure("window_hidden", window_hidden);
        recorder.measure("split_min_hidden", split_min_hidden);
        recorder.measure("collapsed_sidebar", collapsed_sidebar);
        recorder.measure("collapsed_content", collapsed_content);
        recorder.measure("window_collapsed", window_collapsed);
        recorder.measure("split_min_collapsed", split_min_collapsed);
        recorder.measure("uncollapsed_content", uncollapsed_content);
        recorder.measure("window_restored", window_restored);

        recorder.axiom(
            default_sidebar == 250 && default_content == 750,
            "A17: by default the sidebar is fraction × width (0.25 × 1000 = 250) and the content \
             the rest",
        )?;
        recorder.axiom(
            min_wins == 300 && max_wins == 200 && fraction_wins == 300 && pinned == 280,
            "A17: the sidebar width is fraction × width clamped to [min, max]",
        )?;
        recorder.axiom(
            scaled_min == 450 && scaled_max == 300 && px_min == 300,
            "A17: min and max are in sidebar-width-unit, sp by default, so they scale with the text",
        )?;
        recorder.axiom(
            child_minimum == SIDEBAR_REQUEST,
            "A17: the sidebar's own minimum width wins over the fraction",
        )?;
        recorder.axiom(
            window_hidden == WINDOW_WIDTH
                && window_collapsed == WINDOW_WIDTH
                && window_restored == WINDOW_WIDTH,
            "A17: toggling show-sidebar or collapsed does not change the toplevel's width",
        )
    })
}
