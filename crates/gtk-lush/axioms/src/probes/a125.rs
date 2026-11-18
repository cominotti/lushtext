// SPDX-License-Identifier: MIT OR Apache-2.0

//! A125: a one-shot `G_PRIORITY_HIGH_IDLE` source runs **before the next
//! frame clock dispatch**, even when that frame is already due, whether it is
//! armed in a frame's `after-paint`, in a dispatch that runs while the next
//! frame is due, or in its own dispatch (it re-arms once). Read inside the
//! source, `gdk_frame_clock_get_frame_counter` equals the counter of the frame
//! just painted, and it advances only when the next frame starts.
//!
//! The probe presents a window whose widget holds a tick callback, so the
//! clock updates every frame. Case 1 arms the source in one frame's
//! `after-paint`; the source busy-waits 4 ms and re-arms itself once (case 3).
//! Case 2 queues a redraw from a default-priority timeout that then
//! busy-waits 20 ms, so the next frame is due before it returns, and arms the
//! source there. Each case records the order of the sources and the next
//! frame's `before-paint`, the counters read, and (recorded, not asserted)
//! how much later the next frame began than the clock's own interval.
//!
//! LushText's sliced sidebar collapse (`slice-large-directory-collapse` D25)
//! runs each bound unit from such a source armed by the input, by each
//! frame's `after-paint` and by its own dispatch, and reads its one unit per
//! frame interval token from the counter there, outside a frame dispatch.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::glib;
use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::session::{Presented, flush_events, settle};
use crate::{AxiomId, Observation};

/// How long the after-paint source works, as a bound collapse step might.
pub const SOURCE_WORK: Duration = Duration::from_millis(4);
/// How long the busy default-priority dispatch of case 2 runs: more than one
/// 60 Hz frame, so the next frame is due before it returns.
pub const BUSY_DISPATCH: Duration = Duration::from_millis(20);
/// How long the probe waits for each case to finish.
const CASE_BUDGET: Duration = Duration::from_secs(2);

/// One event of the probe's timeline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    /// A frame's `before-paint`, with the clock's counter.
    BeforePaint(i64),
    /// A frame's `after-paint`, with the clock's counter.
    AfterPaint(i64),
    /// A source ran: its case, its dispatch number (0, then 1 for the
    /// re-armed one), and the counter it read.
    Source(u8, u8, i64),
}

/// The probe's fixture: a window column with a ticking widget.
#[derive(Clone)]
pub struct ClockFixture {
    /// The window content.
    pub column: gtk4::Box,
    /// The widget that ticks and is redrawn.
    pub area: gtk4::DrawingArea,
    /// The timeline, with each event's time.
    pub events: Rc<RefCell<Vec<(Event, Instant)>>>,
    /// Which case arms from the next `after-paint` (case 1), 0 for none.
    pub arm_after_paint: Rc<Cell<u8>>,
}

/// Build the fixture (not yet shown).
#[must_use]
pub fn clock_fixture() -> ClockFixture {
    let area = gtk4::DrawingArea::new();
    area.set_content_width(200);
    area.set_content_height(200);
    area.set_vexpand(true);
    area.add_tick_callback(|_, _| glib::ControlFlow::Continue);
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    column.append(&area);
    ClockFixture {
        column,
        area,
        events: Rc::default(),
        arm_after_paint: Rc::default(),
    }
}

fn busy(duration: Duration) {
    let started = Instant::now();
    while started.elapsed() < duration {
        std::hint::spin_loop();
    }
}

impl ClockFixture {
    fn record(&self, event: Event) {
        self.events.borrow_mut().push((event, Instant::now()));
    }

    /// Arm the one-shot source of `case`; its first dispatch works `work`
    /// and re-arms once when `rearm` is set.
    pub fn arm(&self, case: u8, dispatch: u8, work: Duration, rearm: bool) {
        let fixture = self.clone();
        glib::idle_add_local_full(glib::Priority::HIGH_IDLE, move || {
            let counter = fixture
                .area
                .frame_clock()
                .map_or(-1, |clock| clock.frame_counter());
            fixture.record(Event::Source(case, dispatch, counter));
            busy(work);
            if rearm {
                fixture.arm(case, dispatch + 1, Duration::ZERO, false);
            }
            glib::ControlFlow::Break
        });
    }

    /// Connect the clock's `before-paint` and `after-paint` recorders.
    fn connect(&self) -> Option<(gtk4::gdk::FrameClock, Vec<glib::SignalHandlerId>)> {
        let clock = self.area.frame_clock()?;
        let before = {
            let fixture = self.clone();
            clock.connect_before_paint(move |clock| {
                fixture.record(Event::BeforePaint(clock.frame_counter()));
            })
        };
        let after = {
            let fixture = self.clone();
            clock.connect_after_paint(move |clock| {
                fixture.record(Event::AfterPaint(clock.frame_counter()));
                let case = fixture.arm_after_paint.replace(0);
                if case != 0 {
                    fixture.arm(case, 0, SOURCE_WORK, true);
                }
            })
        };
        Some((clock, vec![before, after]))
    }

    /// Dispatch until `done` holds or the budget runs out.
    fn run_until(&self, mut done: impl FnMut(&[(Event, Instant)]) -> bool) -> bool {
        let context = glib::MainContext::default();
        let deadline = Instant::now() + CASE_BUDGET;
        while Instant::now() < deadline {
            if done(&self.events.borrow()) {
                return true;
            }
            if !context.iteration(false) {
                std::thread::sleep(Duration::from_micros(300));
            }
        }
        false
    }
}

/// What one case showed.
#[derive(Clone, Debug, Default)]
pub struct CaseReading {
    /// The counter of the frame painted last before the case's first source.
    pub painted: Option<i64>,
    /// The counters the case's sources read, in dispatch order.
    pub source_counters: Vec<i64>,
    /// The counter of the first `before-paint` after the case's last source.
    pub next_frame: Option<i64>,
    /// Whether every source of the case ran before that `before-paint`.
    pub sources_first: bool,
    /// The sources that ran.
    pub sources: usize,
    /// The gap from the painted frame's `before-paint` to the next one, and
    /// the clock's typical gap before the case, microseconds.
    pub frame_gap_us: Option<(u64, u64)>,
}

fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

/// Read case `case` from the timeline.
#[must_use]
pub fn read_case(events: &[(Event, Instant)], case: u8, sources: usize) -> CaseReading {
    let positions: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, (event, _))| matches!(event, Event::Source(c, _, _) if *c == case))
        .map(|(index, _)| index)
        .collect();
    let (Some(&first), Some(&last)) = (positions.first(), positions.last()) else {
        return CaseReading::default();
    };
    let painted = events[..first]
        .iter()
        .rev()
        .find_map(|(event, _)| match event {
            Event::AfterPaint(counter) => Some(*counter),
            _ => None,
        });
    let source_counters = positions
        .iter()
        .filter_map(|index| match events[*index].0 {
            Event::Source(_, _, counter) => Some(counter),
            _ => None,
        })
        .collect();
    let next = events[last..].iter().find_map(|(event, at)| match event {
        Event::BeforePaint(counter) => Some((*counter, *at)),
        _ => None,
    });
    // The next frame's before-paint after the case's first source, and no
    // before-paint between the first and the last source.
    let between = events[first..=last]
        .iter()
        .any(|(event, _)| matches!(event, Event::BeforePaint(_)));
    let befores: Vec<Instant> = events[..first]
        .iter()
        .filter_map(|(event, at)| matches!(event, Event::BeforePaint(_)).then_some(*at))
        .collect();
    let typical = befores
        .windows(2)
        .rev()
        .take(5)
        .map(|pair| micros(pair[1].saturating_duration_since(pair[0])))
        .min();
    let frame_gap_us = match (befores.last(), next, typical) {
        (Some(previous), Some((_, at)), Some(typical)) => {
            Some((micros(at.saturating_duration_since(*previous)), typical))
        }
        _ => None,
    };
    CaseReading {
        painted,
        source_counters,
        next_frame: next.map(|(counter, _)| counter),
        sources_first: next.is_some() && !between,
        sources: positions.len().min(sources),
        frame_gap_us,
    }
}

/// Probe A125. See the module documentation.
#[must_use]
pub fn probe_a125() -> Observation {
    Recorder::run(AxiomId::new(125), |recorder| {
        let fixture = clock_fixture();
        let shown = Presented::shown(
            libadwaita::Window::builder()
                .default_width(400)
                .default_height(400)
                .content(&fixture.column)
                .build(),
        );
        settle(Duration::from_millis(200));
        recorder.control(shown.realized(), "control: the window received a size")?;
        let (clock, handlers) = recorder.require(fixture.connect(), "control: a frame clock")?;
        let frames_run = fixture.run_until(|events| {
            events
                .iter()
                .filter(|(event, _)| matches!(event, Event::BeforePaint(_)))
                .count()
                >= 6
        });
        recorder.control(frames_run, "control: the ticking widget gets frames")?;

        // Case 1 (and 3): armed in after-paint, re-arming once.
        fixture.arm_after_paint.set(1);
        let after_paint_done = fixture.run_until(|events| {
            let last = events
                .iter()
                .rposition(|(event, _)| matches!(event, Event::Source(1, 1, _)));
            last.is_some_and(|last| {
                events[last..]
                    .iter()
                    .any(|(event, _)| matches!(event, Event::BeforePaint(_)))
            })
        });
        recorder.control(after_paint_done, "control: the after-paint case ran")?;

        // Case 2: armed in a busy dispatch while the next frame is due.
        {
            let fixture_for_timeout = fixture.clone();
            glib::timeout_add_local_full(Duration::ZERO, glib::Priority::DEFAULT, move || {
                fixture_for_timeout.area.queue_draw();
                busy(BUSY_DISPATCH);
                fixture_for_timeout.arm(2, 0, Duration::ZERO, false);
                glib::ControlFlow::Break
            });
        }
        let busy_done = fixture.run_until(|events| {
            let last = events
                .iter()
                .rposition(|(event, _)| matches!(event, Event::Source(2, _, _)));
            last.is_some_and(|last| {
                events[last..]
                    .iter()
                    .any(|(event, _)| matches!(event, Event::BeforePaint(_)))
            })
        });
        recorder.control(busy_done, "control: the busy-dispatch case ran")?;
        for handler in handlers {
            clock.disconnect(handler);
        }
        let events = fixture.events.borrow().clone();
        let after_paint = read_case(&events, 1, 2);
        let due = read_case(&events, 2, 1);
        recorder.measure("after_paint_case", format!("{after_paint:?}"));
        recorder.measure("busy_dispatch_case", format!("{due:?}"));
        recorder.control(
            after_paint.sources == 2 && due.sources == 1,
            "control: every source of both cases ran",
        )?;
        recorder.axiom(
            after_paint.sources_first && due.sources_first,
            "A125: a HIGH_IDLE one-shot armed in after-paint, re-armed from its own dispatch, or \
             armed in a dispatch while the next frame is due runs before that frame's \
             before-paint",
        )?;
        recorder.axiom(
            after_paint.painted.is_some()
                && after_paint
                    .source_counters
                    .iter()
                    .all(|counter| Some(*counter) == after_paint.painted)
                && due.painted.is_some()
                && due
                    .source_counters
                    .iter()
                    .all(|counter| Some(*counter) == due.painted),
            "A125: read in the source, the frame counter equals the counter of the frame just \
             painted",
        )?;
        recorder.axiom(
            after_paint
                .next_frame
                .zip(after_paint.painted)
                .is_some_and(|(next, painted)| next > painted)
                && due
                    .next_frame
                    .zip(due.painted)
                    .is_some_and(|(next, painted)| next > painted),
            "A125: the counter advances only when the next frame starts",
        )?;
        drop(shown);
        drop(fixture);
        flush_events();
        Ok(())
    })
}
