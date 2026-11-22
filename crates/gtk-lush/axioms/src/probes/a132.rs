// SPDX-License-Identifier: MIT OR Apache-2.0

//! A132: `AdwTabBar`'s unpinned tabs live in the first `GtkScrolledWindow`
//! of its template box that has no CSS class `pinned` (`adw-tab-bar.ui`,
//! Libadwaita 1.9 and 1.10), and a write of that scroller's hadjustment
//! `value` from outside the bar scrolls the strip: the tabs move by the
//! written delta, and the value holds across the next frames while the tab
//! box's own scroll animation is idle (`get_scroll_animation_value` returns
//! the adjustment's value then, `adw-tab-box.c`). A write while the box's
//! animation plays pauses it (`adjustment_value_changed_cb` →
//! `adw_animation_pause`), so the written value holds. The adjustment's
//! `page-increment` is 0.9 × `page-size` (`gtk_adjustment_configure` in
//! the box's allocation), and in right-to-left the adjustment's lower end
//! shows the last tab.
//!
//! LushText's tab-strip scroll buttons (`tab-strip-overflow-navigation` D3)
//! find that scroller, step its value by its own page increment, and animate
//! the write; the box's selection scroll stays the owner of the strip
//! whenever it starts one.
//!
//! **Observation, not a pass condition:** a write made in the frame in which
//! a selection-driven box animation finishes, after that animation's tick
//! and before the frame's allocation, may be reverted to the animation's
//! target by that allocation (the box's animation is `FINISHED`, which
//! `adw_animation_pause` does not change). The probe records whether the
//! write held or was reverted as `race_outcome`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;

use super::tab_strip::{
    BOX_ANIMATION, STRIP_WIDTH, TabStripFixture, WINDOW_SIZE, describe_adjustment, overflows,
    run_for, tab_widgets, wait_frames,
};
use crate::observation::Recorder;
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

/// Tabs in the probe's strip: many times the strip's width.
pub const TABS: usize = 40;
/// Frames the probe waits after a write before reading it back.
pub const HOLD_FRAMES: u32 = 3;
/// How long the probe lets a box animation run out.
const ANIMATION_SETTLE: Duration = Duration::from_millis(600);
/// Tolerance, in pixels, for a value or a position read back.
const PIXEL: f64 = 1.0;

/// The A132 fixture: `TABS` tabs in a `STRIP_WIDTH`-wide bar.
#[must_use]
pub fn fixture() -> TabStripFixture {
    TabStripFixture::new(TABS)
}

/// Write `value` into the unpinned scroller's adjustment from outside the
/// bar, as LushText's scroll step does.
pub fn write_scroll_value(fixture: &TabStripFixture, value: f64) {
    if let Some(scroller) = fixture.unpinned() {
        scroller.hadjustment().set_value(value);
    }
}

/// Select `page` and let the box's scroll animation run out; returns the
/// adjustment values the bar's frames saw while it ran.
fn select_and_trace(fixture: &TabStripFixture, page: &libadwaita::TabPage) -> Vec<f64> {
    let Some(scroller) = fixture.unpinned() else {
        return Vec::new();
    };
    let traced = scroller.hadjustment();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&seen);
    let tick = fixture.bar.add_tick_callback(move |_, _| {
        log.borrow_mut().push(traced.value());
        glib::ControlFlow::Continue
    });
    fixture.tab_view.set_selected_page(page);
    run_for(ANIMATION_SETTLE);
    let _ = wait_frames(&fixture.bar, HOLD_FRAMES);
    tick.remove();
    seen.take()
}

/// Select `page` and, on the first frame the box's animation has moved the
/// value strictly between `from` and `to`, write `value_for(current)`.
/// Returns (the value seen when writing, the written value), or `None` if
/// no frame was caught mid-animation.
fn select_and_write_while_playing(
    fixture: &TabStripFixture,
    page: &libadwaita::TabPage,
    from: f64,
    to: f64,
) -> Option<(f64, f64)> {
    let target = fixture.unpinned()?.hadjustment();
    let written = Rc::new(Cell::new(None::<(f64, f64)>));
    let record = Rc::clone(&written);
    let (low, high) = if from < to { (from, to) } else { (to, from) };
    let tick = fixture.bar.add_tick_callback(move |_, _| {
        let current = target.value();
        if record.get().is_none() && current > low + PIXEL && current < high - PIXEL {
            let value = (current / 2.0).round();
            target.set_value(value);
            record.set(Some((current, value)));
        }
        glib::ControlFlow::Continue
    });
    fixture.tab_view.set_selected_page(page);
    run_for(ANIMATION_SETTLE);
    let _ = wait_frames(&fixture.bar, HOLD_FRAMES);
    tick.remove();
    written.get()
}

/// Select `page`, then, from a tick callback added after the box's
/// animation started (so it runs after the animation's own tick), write
/// `value` in the first frame at least [`BOX_ANIMATION`] after the
/// selection. Returns the value read back once everything settled.
fn select_and_write_at_finish(
    fixture: &TabStripFixture,
    page: &libadwaita::TabPage,
    value: f64,
) -> Option<f64> {
    let adjustment = fixture.unpinned()?.hadjustment();
    let start = fixture.bar.frame_clock()?.frame_time();
    let duration = i64::try_from(BOX_ANIMATION.as_micros()).ok()?;
    fixture.tab_view.set_selected_page(page);
    let done = Rc::new(Cell::new(false));
    let flag = Rc::clone(&done);
    let target = adjustment.clone();
    let tick = fixture.bar.add_tick_callback(move |_, clock| {
        if !flag.get() && clock.frame_time() - start >= duration {
            target.set_value(value);
            flag.set(true);
        }
        glib::ControlFlow::Continue
    });
    run_for(ANIMATION_SETTLE);
    let _ = wait_frames(&fixture.bar, HOLD_FRAMES);
    tick.remove();
    done.get().then(|| adjustment.value())
}

/// Probe A132.
#[must_use]
pub fn probe_a132() -> Observation {
    Recorder::run(AxiomId::new(132), |recorder| {
        let fixture = fixture();
        let _shown = Presented::checked_sized(
            recorder,
            &fixture.root,
            WINDOW_SIZE,
            "control: the tab strip's window was realized",
        )?;
        settle(super::LAYOUT_SETTLE);
        recorder.measure("bar_height", fixture.bar.height());

        // Template shape.
        let pinned = fixture.pinned();
        let unpinned = fixture.unpinned();
        recorder.measure("pinned_scroller_found", pinned.is_some());
        recorder.measure("unpinned_scroller_found", unpinned.is_some());
        let unpinned = recorder.require(
            unpinned,
            "control: a GtkScrolledWindow without CSS class pinned is under the bar within the \
             template's depth",
        )?;
        let box_type = unpinned.child().map_or_else(
            || "none".to_owned(),
            |child| child.type_().name().to_owned(),
        );
        recorder.measure("unpinned_child_type", &box_type);
        let tabs = tab_widgets(&unpinned);
        recorder.measure("unpinned_tabs", tabs.len());
        recorder.control(
            pinned.is_some_and(|pinned| pinned != unpinned)
                && box_type == "AdwTabBox"
                && tabs.len() == TABS,
            "control: the bar's template holds a pinned scroller and an unpinned one whose \
             AdwTabBox shows every tab, each AdwTab naming its page",
        )?;
        let adjustment = unpinned.hadjustment();
        // Each page attach scrolled the strip to its tab: bring it back to the
        // start, where the selected first tab is in view, and let it idle.
        recorder.measure("after_attach", describe_adjustment(&adjustment));
        write_scroll_value(&fixture, 0.0);
        run_for(ANIMATION_SETTLE);
        recorder.measure("initial", describe_adjustment(&adjustment));
        recorder.measure("strip_width", STRIP_WIDTH);
        recorder.control(
            overflows(&adjustment),
            "control: the unpinned tabs overflow the strip",
        )?;

        // Page increment.
        let increment = adjustment.page_increment();
        let page_size = adjustment.page_size();
        recorder.axiom(
            (increment - 0.9 * page_size).abs() < 0.01,
            "the unpinned adjustment's page-increment is 0.9 x its page-size",
        )?;

        // An outside write while the box is idle.
        let pages = fixture.pages();
        let first = recorder.require(pages.first().cloned(), "control: the first page exists")?;
        let last = recorder.require(pages.last().cloned(), "control: the last page exists")?;
        let before = recorder.require(
            TabStripFixture::tab_span(&unpinned, &first),
            "control: the first tab has bounds",
        )?;
        let delta = increment.min(adjustment.upper() - page_size - adjustment.value());
        let written = adjustment.value() + delta;
        write_scroll_value(&fixture, written);
        let framed = wait_frames(&fixture.bar, HOLD_FRAMES);
        let value_after_frames = adjustment.value();
        settle(super::LAYOUT_SETTLE);
        let value_settled = adjustment.value();
        let after = recorder.require(
            TabStripFixture::tab_span(&unpinned, &first),
            "control: the first tab still has bounds",
        )?;
        recorder.measure("write_delta", delta);
        recorder.measure("value_after_frames", value_after_frames);
        recorder.measure("value_settled", value_settled);
        recorder.measure(
            "first_tab_x_before_after",
            format!("{} {}", before.0, after.0),
        );
        recorder.control(
            framed,
            "control: the bar's frame clock ticked after the write",
        )?;
        recorder.axiom(
            (value_after_frames - written).abs() < PIXEL
                && (value_settled - written).abs() < PIXEL
                && (before.0 - after.0 - delta).abs() <= PIXEL,
            "an outside write of the unpinned hadjustment moves the tabs by the written delta \
             and holds across the next frames while the box is idle",
        )?;

        // Control: selecting the last page plays a box animation.
        let start_value = adjustment.value();
        let trace = select_and_trace(&fixture, &last);
        let target = adjustment.value();
        let between = trace
            .iter()
            .filter(|value| {
                let (low, high) = if start_value < target {
                    (start_value, target)
                } else {
                    (target, start_value)
                };
                **value > low + PIXEL && **value < high - PIXEL
            })
            .count();
        recorder.measure(
            "selection_scroll_from_to",
            format!("{start_value} {target}"),
        );
        recorder.measure("selection_frames_between", between);
        recorder.measure(
            "selection_trace",
            super::join_entries(trace.iter().take(24)),
        );
        recorder.measure(
            "gtk_enable_animations",
            gtk4::Settings::default().is_some_and(|settings| settings.is_gtk_enable_animations()),
        );
        recorder.control(
            between >= 1 && (target - start_value).abs() > PIXEL,
            "control: selecting an out-of-view page plays the box's scroll animation over \
             several frames",
        )?;

        // Back to the first page at the start, without an animation.
        write_scroll_value(&fixture, 0.0);
        settle(super::LAYOUT_SETTLE);
        fixture.tab_view.set_selected_page(&first);
        run_for(ANIMATION_SETTLE);
        recorder.measure("reset_value", adjustment.value());
        recorder.control(
            adjustment.value().abs() < PIXEL,
            "control: the strip is back at its start with the first page selected",
        )?;

        // A write while the box's animation plays pauses it.
        let paused = select_and_write_while_playing(&fixture, &last, 0.0, target);
        let final_value = adjustment.value();
        recorder.measure(
            "playing_write_seen_written",
            paused.map_or_else(
                || "none".to_owned(),
                |(seen, value)| format!("{seen} {value}"),
            ),
        );
        recorder.measure("playing_write_final", final_value);
        let (_, written_while_playing) = recorder.require(
            paused,
            "control: a frame was caught while the box's animation was playing",
        )?;
        recorder.axiom(
            (final_value - written_while_playing).abs() < PIXEL
                && (final_value - target).abs() > PIXEL,
            "a write while the box's scroll animation plays pauses it: the written value holds",
        )?;

        // Observation: a write in the frame the box animation finishes.
        write_scroll_value(&fixture, 0.0);
        settle(super::LAYOUT_SETTLE);
        fixture.tab_view.set_selected_page(&first);
        run_for(ANIMATION_SETTLE);
        let race_value = (target / 2.0).round();
        let race = select_and_write_at_finish(&fixture, &last, race_value);
        let outcome = match race {
            Some(value) if (value - race_value).abs() < PIXEL => "held",
            Some(value) if (value - target).abs() < PIXEL => "reverted",
            Some(_) => "other",
            None => "not-written",
        };
        recorder.measure("race_written_read", format!("{race_value} {race:?}"));
        recorder.measure("race_outcome", outcome);

        // Right-to-left: the lower end shows the last tab.
        // The interface direction, as a right-to-left locale sets it: a
        // widget whose own direction is unset reads the default direction.
        let default_direction = gtk4::Widget::default_direction();
        gtk4::Widget::set_default_direction(gtk4::TextDirection::Rtl);
        settle(super::LAYOUT_SETTLE);
        write_scroll_value(&fixture, adjustment.lower());
        let _ = wait_frames(&fixture.bar, HOLD_FRAMES);
        settle(super::LAYOUT_SETTLE);
        let rtl_value = adjustment.value();
        let last_shown = TabStripFixture::tab_overlaps(&unpinned, &last);
        let first_shown = TabStripFixture::tab_overlaps(&unpinned, &first);
        recorder.measure("rtl_value", rtl_value);
        recorder.measure(
            "rtl_lower_shows_last_first",
            format!("{last_shown} {first_shown}"),
        );
        gtk4::Widget::set_default_direction(default_direction);
        settle(super::LAYOUT_SETTLE);
        recorder.axiom(
            (rtl_value - adjustment.lower()).abs() < PIXEL && last_shown && !first_shown,
            "in right-to-left the unpinned adjustment's lower end shows the last tab",
        )?;
        Ok(())
    })
}
