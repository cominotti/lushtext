// SPDX-License-Identifier: MIT OR Apache-2.0

//! A103: on GtkSourceView >= 5.22 a `GtkSourceMap` **skips allocating its
//! slider when the slider's rectangle is unchanged** (`gtk_source_map_allocate_slider`
//! returns early when `slider_allocated` and the area is equal), even when the
//! slider itself queued a resize. A resize of the slider marks the map for
//! reallocation too, and GTK clears the map's "a child needs allocating" mark
//! once the map's `size_allocate` returns (`gtk_widget_allocate`), so the slider
//! is left without a current allocation and drawing the map logs
//! `Gtk-WARNING: Trying to snapshot GtkSourceMapSlider … without a current
//! allocation`, until the slider's rectangle next changes (a scroll, a resize).
//!
//! A CSS change of the slider's size is such a resize: a class added to the
//! map that a provider uses to change the slider's margins logs it too
//! (recorded as a value). Controls: a frame with nothing queued and a resize
//! queued on the map alone draw cleanly.
//!
//! On GtkSourceView < 5.22 the map allocated its slider on every
//! `size_allocate`, and the same resize draws cleanly.
//!
//! LushText toggled such a class (`minimap-wide-editor-slider-offset`, a
//! one-pixel slider margin) on every minimap projection; on 5.22 the class is
//! not applied (`ui/editor_page/minimap/projection_execution.rs`).

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::observation::Stop;
use crate::probes::a99::MapFixture;
use crate::probes::text::{LogCapture, record_text_toolkit};
use crate::session::{Presented, settle};
use crate::version::catalogued_bound;
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Lines of the fixture.
pub const LINES: usize = 2_000;
/// The warning text the probe counts.
pub const SLIDER_WARNING: &str = "Trying to snapshot GtkSourceMapSlider";

/// The map's slider, its first child.
#[must_use]
pub fn slider(map: &sourceview5::Map) -> Option<gtk4::Widget> {
    let mut child = map.first_child();
    while let Some(widget) = child {
        if widget.type_().name() == "GtkSourceMapSlider" {
            return Some(widget);
        }
        child = widget.next_sibling();
    }
    None
}

/// What one round changes before a frame is drawn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Queued {
    /// Nothing.
    Nothing,
    /// A resize of the map.
    Map,
    /// A resize of the slider.
    Slider,
    /// A class on the map that a provider uses to move the slider's margins.
    SliderMarginClass,
}

/// The class and the provider rule [`Queued::SliderMarginClass`] uses.
pub const MARGIN_CLASS: &str = "a103-slider-offset";
const MARGIN_RULE: &str = ".a103-slider-offset slider { margin-top: 1px; margin-bottom: -1px; }";

/// Install the rule that moves the slider's margins while the map carries
/// [`MARGIN_CLASS`]; remove the returned provider to drop it.
#[must_use]
pub fn install_margin_rule(display: &gtk4::gdk::Display) -> gtk4::CssProvider {
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(MARGIN_RULE);
    gtk4::style_context_add_provider_for_display(
        display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    provider
}

/// Apply one round's change to a presented fixture, let frames draw, and
/// count the slider warnings they logged.
#[must_use]
pub fn queue_and_count(fixture: &MapFixture, queued: Queued) -> usize {
    let capture = LogCapture::start();
    match queued {
        Queued::Nothing => {}
        Queued::Map => fixture.map.queue_resize(),
        Queued::Slider => {
            if let Some(slider) = slider(&fixture.map) {
                slider.queue_resize();
            }
        }
        Queued::SliderMarginClass => {
            if fixture.map.has_css_class(MARGIN_CLASS) {
                fixture.map.remove_css_class(MARGIN_CLASS);
            } else {
                fixture.map.add_css_class(MARGIN_CLASS);
            }
        }
    }
    settle(LAYOUT_SETTLE);
    settle(LAYOUT_SETTLE);
    capture
        .messages()
        .iter()
        .filter(|message| message.message.contains(SLIDER_WARNING))
        .count()
}

/// A fresh, presented fixture whose map has drawn and allocated its slider,
/// with the margin rule installed on its display.
fn presented_round(recorder: &mut Recorder, queued: Queued) -> Result<usize, Stop> {
    let fixture = MapFixture::new(LINES);
    let provider = install_margin_rule(&fixture.map.display());
    let _shown = Presented::checked(
        recorder,
        &fixture.root,
        "control: the view and its map were presented",
    )?;
    settle(LAYOUT_SETTLE);
    recorder.control(
        slider(&fixture.map).is_some_and(|slider| slider.height() > 0),
        "control: the map has an allocated slider",
    )?;
    let warnings = queue_and_count(&fixture, queued);
    gtk4::style_context_remove_provider_for_display(&fixture.map.display(), &provider);
    Ok(warnings)
}

/// Probe A103.
#[must_use]
pub fn probe_a103() -> Observation {
    Recorder::run(AxiomId::new(103), |recorder| {
        record_text_toolkit(recorder);
        let nothing = presented_round(recorder, Queued::Nothing)?;
        let map = presented_round(recorder, Queued::Map)?;
        let slider_resize = presented_round(recorder, Queued::Slider)?;
        let margin_class = presented_round(recorder, Queued::SliderMarginClass)?;
        recorder.measure("warnings_nothing", nothing);
        recorder.measure("warnings_map_resize", map);
        recorder.measure("warnings_slider_resize", slider_resize);
        recorder.measure("warnings_slider_margin_class", margin_class);
        recorder.control(
            nothing == 0 && map == 0,
            "control: a frame with nothing queued, or a map resize, draws cleanly",
        )?;
        if catalogued_bound(recorder, AxiomId::new(103))?.reached() {
            recorder.axiom(
                slider_resize > 0,
                "A103 (GtkSourceView >= 5.22): a resize of the map's slider leaves it \
                 without a current allocation when the map is drawn",
            )
        } else {
            recorder.axiom(
                slider_resize == 0 && margin_class == 0,
                "A103 (GtkSourceView < 5.22): the map allocates its slider on every \
                 size_allocate, so a slider resize draws cleanly",
            )
        }
    })
}
