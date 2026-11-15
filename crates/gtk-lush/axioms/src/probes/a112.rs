// SPDX-License-Identifier: MIT OR Apache-2.0

//! A112: `gtk_adjustment_configure` sets the adjustment's new value **before**
//! it emits `changed`, and emits `value-changed` after it, so a `changed`
//! handler already reads the value the reconfiguration set; a plain
//! `gtk_adjustment_set_value` emits `value-changed` alone, with no `changed`.
//! A `GtkTextView` that scrolls to a mark in its laid-out document moves its
//! vertical adjustment the same way: `value-changed` to a value no `changed`
//! emission carried. A wrapped `GtkTextView` whose width changes moves its
//! value only through `configure`: when it keeps its first visible line in
//! place after the rewrap, every `value-changed` reports the value the
//! preceding `changed` already carried.
//!
//! LushText tells a scroll from a reallocation by exactly this
//! (`restore-main-ci` D2, `ui/editor_page/overscroll.rs`): during a width
//! reflow burst, a `value-changed` whose value differs from the value the
//! last `changed` carried moved the view on its own, so the burst's settle
//! keeps it instead of restoring the top anchor. The rewrap half is what
//! keeps that rule from mistaking GTK's own relayout for a scroll.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

use super::LAYOUT_SETTLE;
use super::text::TextViewFixture;
use crate::observation::Recorder;
use crate::session::{Presented, settle, wait_until};
use crate::{AxiomId, Observation};

/// Lines in the probe's document.
pub const DOCUMENT_LINES: u32 = 2_000;
/// The line the view scrolls to.
pub const TARGET_LINE: i32 = 1_000;

/// One emission an adjustment made: the signal and the value it read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Emission {
    /// `changed`, with the value the adjustment held when it was emitted.
    Changed(f64),
    /// `value-changed`, with the value the adjustment held.
    ValueChanged(f64),
}

/// Every `changed` and `value-changed` an adjustment emits, in order.
#[derive(Clone, Debug, Default)]
pub struct EmissionLog {
    /// The emissions recorded so far.
    pub emissions: Rc<RefCell<Vec<Emission>>>,
}

impl EmissionLog {
    /// Record `adjustment`'s emissions from now on.
    #[must_use]
    pub fn attach(adjustment: &gtk4::Adjustment) -> Self {
        let log = Self::default();
        let emissions = Rc::clone(&log.emissions);
        adjustment.connect_changed(move |adjustment| {
            emissions
                .borrow_mut()
                .push(Emission::Changed(adjustment.value()));
        });
        let emissions = Rc::clone(&log.emissions);
        adjustment.connect_value_changed(move |adjustment| {
            emissions
                .borrow_mut()
                .push(Emission::ValueChanged(adjustment.value()));
        });
        log
    }

    /// Take the emissions recorded so far.
    #[must_use]
    pub fn take(&self) -> Vec<Emission> {
        self.emissions.take()
    }
}

/// Whether every `value-changed` in `emissions` is to a value the preceding
/// `changed` did not carry: the scroll signature.
#[must_use]
pub fn moved_on_its_own(emissions: &[Emission], value_before: f64) -> bool {
    let mut carried = value_before;
    let mut moved = false;
    for emission in emissions {
        match *emission {
            Emission::Changed(value) => carried = value,
            Emission::ValueChanged(value) => {
                if (value - carried).abs() <= 0.5 {
                    return false;
                }
                moved = true;
            }
        }
    }
    moved
}

/// Whether every `value-changed` in `emissions` reports the value the
/// preceding `changed` carried (or `value_before`, before any `changed`): the
/// reconfiguration signature, with no move of the value's own.
#[must_use]
pub fn every_move_was_carried(emissions: &[Emission], value_before: f64) -> bool {
    let mut carried = value_before;
    emissions.iter().all(|emission| match *emission {
        Emission::Changed(value) => {
            carried = value;
            true
        }
        Emission::ValueChanged(value) => (value - carried).abs() <= 0.5,
    })
}

/// Lines in the wrapped rewrap document; each wraps several times at the
/// fixture width.
pub const WRAPPED_DOCUMENT_LINES: u32 = 800;
/// How much narrower the rewrap makes the wrapped view, in pixels.
pub const REWRAP_NARROWING: i32 = 240;

/// A word-wrapped text view holding [`WRAPPED_DOCUMENT_LINES`] long lines.
#[must_use]
pub fn wrapped_document_view() -> TextViewFixture {
    let fixture = TextViewFixture::new(gtk4::WrapMode::Word);
    let text: String = (0..WRAPPED_DOCUMENT_LINES)
        .map(|line| format!("{line} {}\n", "lorem ipsum ".repeat(40)))
        .collect();
    fixture.buffer.set_text(&text);
    fixture
}

/// Narrow `fixture` by [`REWRAP_NARROWING`] and widen it back, settling
/// after each, so the view rewraps twice under a kept first line.
pub fn rewrap(fixture: &TextViewFixture) {
    fixture.scrolled.set_margin_end(REWRAP_NARROWING);
    settle(LAYOUT_SETTLE * 3);
    fixture.scrolled.set_margin_end(0);
    settle(LAYOUT_SETTLE * 3);
}

/// A text view holding [`DOCUMENT_LINES`] short lines, unwrapped.
#[must_use]
pub fn document_view() -> TextViewFixture {
    let fixture = TextViewFixture::new(gtk4::WrapMode::None);
    let text: String = (0..DOCUMENT_LINES)
        .map(|line| format!("line {line}\n"))
        .collect();
    fixture.buffer.set_text(&text);
    fixture
}

/// Scroll `fixture` to [`TARGET_LINE`] through the insert mark.
pub fn scroll_to_target(fixture: &TextViewFixture) {
    let iter = fixture
        .buffer
        .iter_at_line(TARGET_LINE)
        .unwrap_or_else(|| fixture.buffer.end_iter());
    fixture.buffer.place_cursor(&iter);
    fixture
        .view
        .scroll_to_mark(&fixture.buffer.get_insert(), 0.0, true, 0.0, 0.5);
}

fn format_emissions(emissions: &[Emission]) -> String {
    emissions
        .iter()
        .map(|emission| match emission {
            Emission::Changed(value) => format!("changed({value:.1})"),
            Emission::ValueChanged(value) => format!("value-changed({value:.1})"),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Probe A112.
#[must_use]
pub fn probe_a112() -> Observation {
    Recorder::run(AxiomId::new(112), |recorder| {
        // A free adjustment: a reconfiguration that moves bounds and value.
        let adjustment = gtk4::Adjustment::new(0.0, 0.0, 1_000.0, 1.0, 10.0, 100.0);
        let log = EmissionLog::attach(&adjustment);
        adjustment.configure(400.0, 0.0, 2_000.0, 1.0, 10.0, 100.0);
        let configured = log.take();
        recorder.measure("configure", format_emissions(&configured));
        recorder.axiom(
            configured == [Emission::Changed(400.0), Emission::ValueChanged(400.0)],
            "configure emits changed with the new value in place, then value-changed",
        )?;
        adjustment.set_value(700.0);
        let set = log.take();
        recorder.measure("set_value", format_emissions(&set));
        recorder.axiom(
            set == [Emission::ValueChanged(700.0)],
            "set_value emits value-changed alone",
        )?;

        // A laid-out text view scrolling to a mark.
        let fixture = document_view();
        let _shown = Presented::checked(
            recorder,
            &fixture.scrolled,
            "control: the text view was presented",
        )?;
        let vadjustment = fixture.scrolled.vadjustment();
        let laid_out = wait_until(std::time::Duration::from_secs(5), || {
            vadjustment.upper() > vadjustment.page_size() * 4.0 && vadjustment.page_size() > 0.0
        });
        settle(LAYOUT_SETTLE);
        recorder.measure("upper", vadjustment.upper());
        recorder.measure("page_size", vadjustment.page_size());
        recorder.control(
            laid_out && vadjustment.value() < 0.5,
            "control: the view is laid out, taller than its page, at the top",
        )?;
        let value_before = vadjustment.value();
        let view_log = EmissionLog::attach(&vadjustment);
        scroll_to_target(&fixture);
        let scrolled = wait_until(std::time::Duration::from_secs(5), || {
            vadjustment.value() > vadjustment.page_size()
        });
        settle(LAYOUT_SETTLE);
        let emissions = view_log.take();
        recorder.measure("scroll_to_mark", format_emissions(&emissions));
        recorder.measure("value_after", vadjustment.value());
        recorder.control(scrolled, "control: scroll_to_mark moved the view")?;
        recorder.axiom(
            moved_on_its_own(&emissions, value_before),
            "scroll_to_mark moves the vertical adjustment to a value no changed emission carried",
        )?;
        drop(_shown);

        // A wrapped view rewrapped by a width change, at the top and in the
        // middle of its document (`restore-main-ci` stage 4).
        let fixture = wrapped_document_view();
        let _shown = Presented::checked(
            recorder,
            &fixture.scrolled,
            "control: the wrapped text view was presented",
        )?;
        let vadjustment = fixture.scrolled.vadjustment();
        let laid_out = wait_until(std::time::Duration::from_secs(10), || {
            vadjustment.upper() > vadjustment.page_size() * 8.0 && vadjustment.page_size() > 0.0
        });
        settle(LAYOUT_SETTLE * 3);
        recorder.control(
            laid_out && vadjustment.value() < 0.5,
            "control: the wrapped view is laid out, much taller than its page, at the top",
        )?;
        let view_log = EmissionLog::attach(&vadjustment);
        let upper_before = vadjustment.upper();
        fixture.scrolled.set_margin_end(REWRAP_NARROWING);
        let rewrapped = wait_until(std::time::Duration::from_secs(5), || {
            (vadjustment.upper() - upper_before).abs() > 1.0
        });
        settle(LAYOUT_SETTLE * 3);
        fixture.scrolled.set_margin_end(0);
        settle(LAYOUT_SETTLE * 3);
        let at_top = view_log.take();
        recorder.measure("rewrap_at_top", at_top.len());
        recorder.measure("rewrap_at_top_value_after", vadjustment.value());
        recorder.control(rewrapped, "control: narrowing the view rewrapped it")?;
        recorder.axiom(
            every_move_was_carried(&at_top, 0.0) && vadjustment.value() < 0.5,
            "a rewrap at the top moves the value only through configure, and leaves it at the top",
        )?;

        vadjustment.set_value((vadjustment.upper() - vadjustment.page_size()) / 2.0);
        settle(LAYOUT_SETTLE * 3);
        let value_before = vadjustment.value();
        let _ = view_log.take();
        rewrap(&fixture);
        let mid = view_log.take();
        let moves = mid
            .iter()
            .filter(|emission| matches!(emission, Emission::ValueChanged(_)))
            .count();
        recorder.measure("rewrap_mid_emissions", mid.len());
        recorder.measure("rewrap_mid_value_moves", moves);
        recorder.control(
            moves > 0,
            "control: a mid-document rewrap moved the value to keep the first visible line",
        )?;
        recorder.axiom(
            every_move_was_carried(&mid, value_before),
            "a mid-document rewrap moves the value only through configure: every value-changed \
             reports the value the preceding changed carried",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{Emission, every_move_was_carried, moved_on_its_own};

    #[test]
    fn a_value_carried_by_changed_is_not_a_scroll() {
        assert!(!moved_on_its_own(
            &[Emission::Changed(40.0), Emission::ValueChanged(40.0)],
            0.0
        ));
    }

    #[test]
    fn a_value_no_changed_carried_is_a_scroll() {
        assert!(moved_on_its_own(
            &[
                Emission::Changed(0.0),
                Emission::ValueChanged(4_000.0),
                Emission::ValueChanged(12_000.0)
            ],
            0.0
        ));
        assert!(!moved_on_its_own(&[], 0.0));
    }

    #[test]
    fn a_reconfiguration_carries_every_move() {
        assert!(every_move_was_carried(
            &[
                Emission::Changed(0.0),
                Emission::Changed(40.0),
                Emission::ValueChanged(40.0)
            ],
            0.0
        ));
        assert!(every_move_was_carried(&[], 7.0));
        assert!(!every_move_was_carried(
            &[Emission::Changed(40.0), Emission::ValueChanged(90.0)],
            0.0
        ));
    }
}
