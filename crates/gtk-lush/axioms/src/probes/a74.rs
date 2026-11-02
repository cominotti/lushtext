// SPDX-License-Identifier: MIT OR Apache-2.0

//! A74: a change of `GtkSettings:gtk-enable-animations` restyles **every
//! widget** of a libadwaita application. Libadwaita's style manager follows
//! the setting (`enable_animations_cb`) by reloading a style provider of its
//! own, so the next frame revalidates the CSS of every widget of every window,
//! and its cost grows with the number of widgets; a frame after a redraw with
//! no style change, or after a change of a setting GTK does not style from
//! (`gtk-cursor-blink`), does not.
//!
//! The probe presents a window of [`SMALL_ROWS`] and then [`LARGE_ROWS`]
//! labelled rows, and records the longest frame-painting dispatch after a
//! plain redraw, after toggling `gtk-cursor-blink` (the control) and after
//! toggling `gtk-enable-animations`, each the median of [`REPETITIONS`].
//!
//! `bound-post-open-settle`'s independent review (M4a) named this restyle as
//! the cause of the whole-tree revalidation about 200 ms after a document
//! opens in the responsiveness lane. The lane ruled it out: the measurement
//! build recorded no `notify` of any `GtkSettings` or `AdwStyleManager`
//! property in that window (2026-11-02). `perf` places that revalidation in
//! the frame clock's flush phase, under `gdk_surface_ensure_motion`: the
//! motion GDK synthesizes after the lane's focus click moved the pointer into
//! the window, whose `:hover` change restyles the widgets under the pointer.
//! The axiom stays pinned because it holds and a desktop that toggles
//! animations restyles every libadwaita window.

use std::time::Duration;

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::post_open::{FrameCounter, REFERENCE_WINDOW, median, watch_until_quiet};
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Rows in the smaller window.
pub const SMALL_ROWS: usize = 200;
/// Rows in the larger window.
pub const LARGE_ROWS: usize = 1_600;
/// Repetitions per measurement (the median is recorded).
pub const REPETITIONS: usize = 5;
/// How long a measurement waits for the main context to go quiet.
const QUIET: Duration = Duration::from_millis(60);
/// The most a measurement waits.
const BUDGET: Duration = Duration::from_secs(3);

/// Longest frame-painting dispatch after each kind of change, µs.
#[derive(Clone, Copy, Debug, Default)]
pub struct RestyleCosts {
    /// After `queue_draw` alone.
    pub redraw: u128,
    /// After toggling `gtk-cursor-blink`.
    pub cursor_blink: u128,
    /// After toggling `gtk-enable-animations`.
    pub enable_animations: u128,
}

/// A scrolled column of `rows` rows, each a box with two labels.
#[must_use]
pub fn rows(count: usize) -> gtk4::ScrolledWindow {
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    for row in 0..count {
        let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        line.add_css_class("toolbar");
        line.append(&gtk4::Label::new(Some(&format!("Row {row}"))));
        let detail = gtk4::Label::new(Some("detail"));
        detail.add_css_class("dim-label");
        line.append(&detail);
        column.append(&line);
    }
    gtk4::ScrolledWindow::builder()
        .child(&column)
        .hexpand(true)
        .vexpand(true)
        .build()
}

fn longest_frame(frames: &FrameCounter, change: impl FnOnce()) -> u128 {
    change();
    watch_until_quiet(Some(frames), QUIET, BUDGET, || 0.0)
        .iter()
        .filter(|watched| watched.dispatch.frames > 0)
        .map(|watched| watched.dispatch.micros)
        .max()
        .unwrap_or_default()
}

/// Measure the three costs on a window of `count` rows.
pub fn measure(
    recorder: &mut Recorder,
    count: usize,
) -> Result<RestyleCosts, crate::observation::Stop> {
    let content = rows(count);
    let shown = Presented::checked_sized(
        recorder,
        &content,
        REFERENCE_WINDOW,
        "control: the rows were presented",
    )?;
    settle(LAYOUT_SETTLE);
    let frames = recorder.require(
        FrameCounter::watch(&content),
        "control: the window has a frame clock",
    )?;
    let settings = recorder.require(
        gtk4::Settings::default(),
        "control: the default display has settings",
    )?;
    let mut costs = (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..REPETITIONS {
        costs
            .0
            .push(longest_frame(&frames, || shown.window().queue_draw()));
        costs.1.push(longest_frame(&frames, || {
            settings.set_gtk_cursor_blink(!settings.is_gtk_cursor_blink());
            shown.window().queue_draw();
        }));
        costs.2.push(longest_frame(&frames, || {
            settings.set_gtk_enable_animations(!settings.is_gtk_enable_animations());
            shown.window().queue_draw();
        }));
    }
    Ok(RestyleCosts {
        redraw: median(&costs.0),
        cursor_blink: median(&costs.1),
        enable_animations: median(&costs.2),
    })
}

fn describe(costs: RestyleCosts) -> String {
    format!(
        "redraw {} us, gtk-cursor-blink {} us, gtk-enable-animations {} us",
        costs.redraw, costs.cursor_blink, costs.enable_animations
    )
}

/// Probe A74.
#[must_use]
pub fn probe_a74() -> Observation {
    Recorder::run(AxiomId::new(74), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let small = measure(recorder, SMALL_ROWS)?;
        recorder.measure(
            "small_window",
            format!("{SMALL_ROWS} rows: {}", describe(small)),
        );
        let large = measure(recorder, LARGE_ROWS)?;
        recorder.measure(
            "large_window",
            format!("{LARGE_ROWS} rows: {}", describe(large)),
        );
        recorder.control(
            small.redraw > 0 && large.redraw > 0,
            "control: every redraw painted a frame",
        )?;
        recorder.axiom(
            large.enable_animations > 2 * large.redraw.max(large.cursor_blink),
            "after gtk-enable-animations changes, the next frame costs more than twice a redraw \
             or a gtk-cursor-blink change",
        )?;
        recorder.axiom(
            large.enable_animations > 2 * small.enable_animations,
            "that frame's cost grows with the number of widgets (every widget is restyled)",
        )
    })
}
