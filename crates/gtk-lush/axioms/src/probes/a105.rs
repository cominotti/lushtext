// SPDX-License-Identifier: MIT OR Apache-2.0

//! A105: the frame time of a frame (`GdkFrameTimings::frame_time`) is read
//! when the frame starts **only up to GTK 4.22**. GTK 4.24 reworked the frame
//! clock (4.23.x `!10035`, `!10108`, `!10141` "frameclock: Redo predicted
//! presentation time", `!10361`); its frame time is aligned to the refresh
//! grid, up to about one refresh interval **before or after** the moment the
//! frame began. A frame dispatched right after an input can therefore carry a
//! frame time earlier than that input.
//!
//! The probe queues a redraw at times off the refresh grid, stamps when each
//! frame began (`before-paint`, monotonic), and reads that frame's frame time.
//! On GTK < 4.24 the frame time lies within [`START_TOLERANCE_US`] of the
//! begin stamp; on GTK >= 4.24 some frame times lie farther from it. Recorded
//! as values: the extreme offsets, and how many frame times precede the
//! request or follow the presentation.
//!
//! LushText's responsiveness probe therefore records when each frame began
//! (`began_us`), and the lane driver attributes input to frames by it
//! (`frame_start_us`); attributing by frame time charged a key to the next
//! key's frame (`adopt-gnome-51` L1).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::session::{Presented, settle};
use crate::version::catalogued_bound;
use crate::{AxiomId, Observation};

/// Redraws the probe requests.
pub const REQUESTS: u32 = 24;
/// How far a frame time read at the frame's start may sit from the probe's
/// `before-paint` stamp of the same frame.
pub const START_TOLERANCE_US: i64 = 1_000;
/// The wait between requests, off a 60 or 120 Hz grid.
pub const REQUEST_SPACING: Duration = Duration::from_millis(37);

/// One redraw request and the frame that drew it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DrawnRequest {
    /// When the redraw was queued (monotonic, µs).
    pub requested_us: i64,
    /// When the frame that drew it began (`before-paint`, monotonic, µs).
    pub began_us: i64,
    /// The frame's frame time (µs).
    pub frame_time_us: i64,
    /// The frame's presentation time, 0 when not reported (µs).
    pub presentation_us: i64,
}

/// A drawing area whose frames are stamped at `before-paint`.
#[derive(Clone, Debug)]
pub struct StampedArea {
    /// The area to present and redraw.
    pub area: gtk4::DrawingArea,
    began: Rc<Cell<Option<(i64, i64)>>>,
    drawn: Rc<RefCell<Vec<(i64, i64, i64)>>>,
}

impl StampedArea {
    /// An area 200 px square.
    #[must_use]
    pub fn new() -> Self {
        let area = gtk4::DrawingArea::new();
        area.set_content_width(200);
        area.set_content_height(200);
        area.set_draw_func(|_, cr, _, _| {
            cr.set_source_rgb(0.2, 0.4, 0.8);
            let _ = cr.paint();
        });
        Self {
            area,
            began: Rc::new(Cell::new(None)),
            drawn: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Stamp the area's frame clock; call once the area is realized.
    pub fn hook(&self) {
        let Some(clock) = self.area.frame_clock() else {
            return;
        };
        let began = self.began.clone();
        clock.connect_before_paint(move |clock| {
            began.set(Some((glib::monotonic_time(), clock.frame_counter())));
        });
        let (began, drawn) = (self.began.clone(), self.drawn.clone());
        clock.connect_after_paint(move |_| {
            if let Some((at, counter)) = began.take() {
                drawn.borrow_mut().push((at, counter, 0));
            }
        });
    }

    /// Request one redraw, let it draw, and return the request with its frame.
    #[must_use]
    pub fn request(&self) -> Option<DrawnRequest> {
        let before = self.drawn.borrow().len();
        let requested_us = glib::monotonic_time();
        self.area.queue_draw();
        settle(REQUEST_SPACING);
        let drawn = self.drawn.borrow();
        let &(began_us, counter, _) = drawn.get(before)?;
        let clock = self.area.frame_clock()?;
        let timings = clock.timings(counter)?;
        Some(DrawnRequest {
            requested_us,
            began_us,
            frame_time_us: timings.frame_time(),
            presentation_us: timings.presentation_time(),
        })
    }
}

impl Default for StampedArea {
    fn default() -> Self {
        Self::new()
    }
}

/// Probe A105.
#[must_use]
pub fn probe_a105() -> Observation {
    Recorder::run(AxiomId::new(105), |recorder| {
        let stamped = StampedArea::new();
        let _shown = Presented::checked(
            recorder,
            &stamped.area,
            "control: the drawing area was presented",
        )?;
        stamped.hook();
        settle(REQUEST_SPACING);
        let drawn: Vec<DrawnRequest> = (0..REQUESTS).filter_map(|_| stamped.request()).collect();
        recorder.measure("requests_drawn", drawn.len());
        recorder.control(
            drawn.len() * 2 >= REQUESTS as usize
                && drawn
                    .iter()
                    .all(|request| request.began_us >= request.requested_us),
            "control: most requests were drawn by a frame that began after the request",
        )?;
        let before_request = drawn
            .iter()
            .filter(|request| request.frame_time_us < request.requested_us)
            .count();
        let after_presentation = drawn
            .iter()
            .filter(|request| {
                request.presentation_us > 0 && request.frame_time_us > request.presentation_us
            })
            .count();
        let offsets: Vec<i64> = drawn
            .iter()
            .map(|request| request.frame_time_us - request.began_us)
            .collect();
        let away_from_start = offsets
            .iter()
            .filter(|offset| offset.abs() > START_TOLERANCE_US)
            .count();
        recorder.measure("frame_time_away_from_begin", away_from_start);
        recorder.measure("frame_time_before_request", before_request);
        recorder.measure("frame_time_after_presentation", after_presentation);
        recorder.measure(
            "frame_time_minus_began_min_us",
            offsets.iter().min().copied().unwrap_or_default(),
        );
        recorder.measure(
            "frame_time_minus_began_max_us",
            offsets.iter().max().copied().unwrap_or_default(),
        );
        if catalogued_bound(recorder, AxiomId::new(105))?.reached() {
            recorder.axiom(
                away_from_start > 0,
                "A105 (GTK >= 4.24): some frame times lie more than a millisecond from when \
                 their frame began (grid-aligned)",
            )
        } else {
            recorder.axiom(
                away_from_start == 0 && before_request == 0,
                "A105 (GTK < 4.24): every frame time is within a millisecond of when its frame \
                 began, so at or after the request it draws",
            )
        }
    })
}
