// SPDX-License-Identifier: MIT OR Apache-2.0

//! A45: a widget unparented, and so unrealized, while a frame-clock tick
//! callback is armed **never runs that callback** in a later frame, and its
//! later disposal logs nothing. Unrealizing disconnects the widget from the
//! frame clock. For a `GtkSourceMap` whose update tick a scroll of its view
//! has just armed, unparenting the map and dropping it therefore logs no
//! `CRITICAL`, unlike clearing its view in place (ledger A32).
//!
//! The control is a sibling armed in the same turn and kept in the tree: it
//! does tick.
//!
//! The document scale policy (C1) retires the minimap by unparenting and
//! dropping it, then builds a fresh map, instead of clearing a live map's
//! view (`ui/editor_page/minimap/retirement.rs`).

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::a32::MappedView;
use crate::probes::text::{CapturedLog, LogCapture};
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Settle rounds after the unparent, each one layout settle long.
pub const FRAMES_WAITED: u32 = 5;

/// Two drawing areas in a box, each counting its tick-callback invocations.
#[derive(Clone, Debug)]
pub struct TickPair {
    /// The box holding both.
    pub container: gtk4::Box,
    /// The widget that is unparented while armed.
    pub retired: gtk4::DrawingArea,
    /// The sibling that stays in the tree.
    pub kept: gtk4::DrawingArea,
    /// Ticks the retired widget ran.
    pub retired_ticks: Rc<Cell<u32>>,
    /// Ticks the kept widget ran.
    pub kept_ticks: Rc<Cell<u32>>,
}

impl TickPair {
    /// Two unarmed widgets side by side.
    #[must_use]
    pub fn new() -> Self {
        let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let retired = gtk4::DrawingArea::new();
        let kept = gtk4::DrawingArea::new();
        for area in [&retired, &kept] {
            area.set_size_request(40, 40);
            container.append(area);
        }
        Self {
            container,
            retired,
            kept,
            retired_ticks: Rc::new(Cell::new(0)),
            kept_ticks: Rc::new(Cell::new(0)),
        }
    }

    /// Arm a tick callback on both widgets.
    pub fn arm(&self) {
        for (area, counter) in [
            (&self.retired, &self.retired_ticks),
            (&self.kept, &self.kept_ticks),
        ] {
            let counter = counter.clone();
            area.add_tick_callback(move |_, _| {
                counter.set(counter.get() + 1);
                glib::ControlFlow::Continue
            });
        }
    }

    /// Unparent the retired widget.
    pub fn unparent_retired(&self) {
        if self.retired.parent().is_some() {
            self.container.remove(&self.retired);
        }
    }
}

impl Default for TickPair {
    fn default() -> Self {
        Self::new()
    }
}

fn warnings(messages: &[CapturedLog]) -> Vec<String> {
    messages.iter().map(ToString::to_string).collect()
}

/// Probe A45.
#[must_use]
pub fn probe_a45() -> Observation {
    Recorder::run(AxiomId::new(45), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let pair = TickPair::new();
        let shown = Presented::checked(
            recorder,
            &pair.container,
            "control: the two tick widgets were presented",
        )?;
        let capture = LogCapture::start();
        recorder.control(
            pair.retired.is_realized() && pair.retired.is_mapped(),
            "control: the widget to retire is realized and mapped",
        )?;
        pair.arm();
        pair.unparent_retired();
        for _ in 0..FRAMES_WAITED {
            settle(LAYOUT_SETTLE);
        }
        recorder.measure("kept_ticks", pair.kept_ticks.get());
        recorder.measure("retired_ticks", pair.retired_ticks.get());
        recorder.measure("retired_realized", pair.retired.is_realized());
        recorder.control(
            pair.kept_ticks.get() > 0,
            "control: the armed sibling kept in the tree ticks",
        )?;
        recorder.axiom(
            pair.retired_ticks.get() == 0,
            "a widget unparented while its tick callback is armed never runs it",
        )?;
        let weak = pair.retired.downgrade();
        let TickPair { retired, .. } = pair;
        drop(retired);
        settle(LAYOUT_SETTLE);
        let after_plain = warnings(&capture.messages());
        recorder.measure("retired_disposed", weak.upgrade().is_none());
        recorder.measure("plain_dispose_messages", super::join_entries(&after_plain));
        recorder.axiom(
            after_plain.is_empty(),
            "disposing the unparented widget logs nothing",
        )?;
        drop(shown);

        let fixture = recorder.require(
            MappedView::new(),
            "control: GtkSourceView has an html language",
        )?;
        let _map_shown = Presented::checked(
            recorder,
            &fixture.container,
            "control: the view and its map were presented",
        )?;
        let MappedView {
            container,
            source,
            map,
        } = fixture;
        let before = source.scrolled.vadjustment().value();
        let adjustment = source.scrolled.vadjustment();
        adjustment.set_value(adjustment.value() + crate::probes::a32::SCROLL_STEP);
        recorder.measure("view_scrolled_by", adjustment.value() - before);
        recorder.control(
            adjustment.value() > before && map.is_realized(),
            "control: the view scrolled (arming the map's update tick) with the map realized",
        )?;
        container.remove(&map);
        let map_weak = map.downgrade();
        drop(map);
        for _ in 0..FRAMES_WAITED {
            settle(LAYOUT_SETTLE);
            adjustment.set_value(adjustment.value() + crate::probes::a32::SCROLL_STEP);
        }
        let after_map = warnings(&capture.messages());
        drop(capture);
        recorder.measure("map_disposed", map_weak.upgrade().is_none());
        recorder.measure("map_retire_messages", super::join_entries(&after_map));
        recorder.axiom(
            after_map.is_empty(),
            "unparenting and dropping a GtkSourceMap with its update tick armed logs no CRITICAL",
        )
    })
}
