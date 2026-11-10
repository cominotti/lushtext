// SPDX-License-Identifier: MIT OR Apache-2.0

//! A32 (a defect axiom): detaching a `GtkSourceMap` from its view
//! (`gtk_source_map_set_view(NULL)`, here through the `view` property) while
//! the map's update tick is pending makes that tick run against no view, and
//! GTK logs `CRITICAL`s (`gtk_scrollable_get_vadjustment` and
//! `g_object_get` on a null object). A scroll of the view queues the tick, so
//! scrolling and detaching in the same main-loop turn is enough.
//!
//! The axiom is **version-qualified** (`adopt-gnome-51` D3). On GtkSourceView
//! < 5.22 "holds" means the defect is present (5.20, the GNOME 50 floor, has
//! it). Upstream fixed it in 5.22.0 (commit 8d679ccd cancels the tick in
//! `disconnect_view`), so on >= 5.22 "holds" means the same detach logs **no**
//! CRITICAL, and a regression back to the defect fails the probe. The document
//! scale policy (C1) still retires the minimap instead of detaching it (A45):
//! the floor needs it, and a retired map is released rather than kept parented
//! without a view (`adopt-gnome-51` D5).

use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{CapturedLog, LogCapture, SourceViewFixture};
use crate::session::{Presented, settle};
use crate::version::catalogued_bound;
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Lines in the probe document.
pub const LINES: usize = 3_000;
/// How far each scroll moves the view, in pixels.
pub const SCROLL_STEP: f64 = 400.0;

/// A source view and a `GtkSourceMap` attached to it, side by side.
#[derive(Clone, Debug)]
pub struct MappedView {
    /// The box holding both.
    pub container: gtk4::Box,
    /// The source view and its buffer.
    pub source: SourceViewFixture,
    /// The map.
    pub map: sourceview5::Map,
}

impl MappedView {
    /// The fixture, or `None` without an html language.
    #[must_use]
    pub fn new() -> Option<Self> {
        let source = SourceViewFixture::html(gtk4::WrapMode::None)?;
        let text: String = (0..LINES)
            .map(|line| format!("<p class=\"l{line}\">line {line}</p>\n"))
            .collect();
        source.buffer.set_text(&text);
        let map = sourceview5::Map::new();
        map.set_view(&source.view);
        let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        container.append(&source.scrolled);
        container.append(&map);
        Some(Self {
            container,
            source,
            map,
        })
    }

    /// Scroll the view down by [`SCROLL_STEP`].
    pub fn scroll(&self) {
        let adjustment = self.source.scrolled.vadjustment();
        adjustment.set_value(adjustment.value() + SCROLL_STEP);
    }

    /// Detach the map from its view.
    pub fn detach(&self) {
        self.map.set_property("view", None::<&sourceview5::View>);
    }

    /// Detach as [`detach`](Self::detach) does, first unmapping the map
    /// (`set_child_visible(false)` keeps it realized, so its armed tick still
    /// runs and the defect still logs its CRITICALs), so the viewless map is
    /// never drawn: drawing it intermittently printed pixman's
    /// "Invalid rectangle passed *** BUG ***", a second symptom of the same
    /// defect that bypasses GLib logging and that the lane fails on.
    pub fn detach_unmapped(&self) {
        self.map.set_child_visible(false);
        self.detach();
    }
}

fn criticals(messages: &[CapturedLog]) -> Vec<String> {
    messages
        .iter()
        .filter(|message| message.level == "critical")
        .map(ToString::to_string)
        .collect()
}

/// Probe A32.
#[must_use]
pub fn probe_a32() -> Observation {
    Recorder::run(AxiomId::new(32), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let fixture = recorder.require(
            MappedView::new(),
            "control: GtkSourceView has an html language",
        )?;
        let _shown = Presented::checked(
            recorder,
            &fixture.container,
            "control: the view and its map were presented",
        )?;
        let capture = LogCapture::start();
        fixture.scroll();
        settle(LAYOUT_SETTLE);
        let attached = criticals(&capture.messages());
        recorder.measure("criticals_while_attached", super::join_entries(&attached));
        recorder.control(
            attached.is_empty() && fixture.source.scrolled.vadjustment().value() > 0.0,
            "control: scrolling with the map attached scrolls and logs nothing",
        )?;

        fixture.scroll();
        fixture.detach_unmapped();
        settle(LAYOUT_SETTLE);
        let detached = criticals(&capture.messages());
        drop(capture);
        recorder.measure("map_view_after_detach", fixture.map.view().is_none());
        recorder.measure("criticals_after_detach", super::join_entries(&detached));
        recorder.control(
            fixture.map.view().is_none(),
            "control: the map was detached",
        )?;
        if catalogued_bound(recorder, AxiomId::new(32))?.reached() {
            recorder.axiom(
                detached.is_empty(),
                "A32 (GtkSourceView >= 5.22): a detach with the update tick pending logs no \
                 CRITICAL (the defect is fixed)",
            )
        } else {
            recorder.axiom(
                !detached.is_empty(),
                "A32 (GtkSourceView < 5.22): a detach with the update tick pending logs a \
                 CRITICAL (the defect is present)",
            )
        }
    })
}
