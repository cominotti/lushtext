// SPDX-License-Identifier: MIT OR Apache-2.0

//! A80: while a `GtkTextView` has lines left to validate, its incremental
//! validation idle (`GTK_TEXT_VIEW_PRIORITY_VALIDATE`, `GDK_PRIORITY_REDRAW +
//! 5`, A31, A70) is ready again after every 2000 px batch, so a source at a
//! **lower** priority — `G_PRIORITY_DEFAULT_IDLE`, where `g_idle_add` and
//! gtk-rs's `glib::idle_add_once` put it — **does not dispatch until the
//! whole buffer is validated**, however long that takes. A source **one step
//! above** it (`GDK_PRIORITY_REDRAW + 4`) dispatches before the first
//! incremental batch completed (after the on-screen part, which the view
//! validates first, at a higher priority).
//!
//! The view does not have to be on screen: a view on a **hidden page** of a
//! `GtkStack` (as a background tab of an `AdwTabView` is) validates its whole
//! buffer the same way, from the same idle, and holds the lower-priority
//! source back just as long. Progress is read from the y the view's layout
//! records for the buffer's last line (`gtk_text_view_get_line_yrange`, which
//! validates nothing), so it works for the hidden view too.
//!
//! A control runs the same view after it settled: the lower-priority source
//! then dispatches in the second iteration, right after the higher one.
//!
//! Recorded magnitudes, never verdicts: the iterations and microseconds
//! before each source dispatched, and the validated height when it did.
//!
//! gtk-lush-tasks delivers `spawn_blocking_then` completions from one
//! completion source at `GDK_PRIORITY_REDRAW + 4`, one completion per
//! dispatch (`spawn-completion-priority`); before, each completion was a
//! `G_PRIORITY_DEFAULT_IDLE` idle, and a session restoring ten 8 MiB
//! documents installed them in three batches about 11 s apart.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::glib;
use gtk4::prelude::*;

use crate::observation::{Recorder, Stop};
use crate::probes::post_open::{DenseView, MAX_ITERATIONS, VALIDATE_PRIORITY, dense_text};
use crate::session::{Presented, flush_events, settle};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Lines in the probe document.
pub const LINES: usize = 1 << 14;
/// Characters per line.
pub const LINE_CHARS: usize = 40;
/// Seed of the probe document.
pub const SEED: u64 = 80;
/// The priority one step above the validation idle.
pub const ABOVE_VALIDATE_PRIORITY: i32 = VALIDATE_PRIORITY - 1;
/// The fewest 2000 px batches (A31) the document must take for the fixture
/// to be valid.
pub const MIN_BATCHES: i32 = 16;
/// The px one validation batch adds (A31).
pub const BATCH_PX: i32 = 2000;

const BUDGET: Duration = Duration::from_secs(60);
const QUIET: Duration = Duration::from_millis(100);

/// Where the probe view lives.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Host {
    /// On screen, filling the window.
    Presented,
    /// On the hidden page of a `GtkStack` whose visible page is a label.
    HiddenPage,
}

impl Host {
    /// A short name for measurements.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Presented => "presented",
            Self::HiddenPage => "hidden",
        }
    }
}

/// When one armed source dispatched.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Delivery {
    /// Main-context iterations from arming to the one that dispatched it
    /// (1 = the first iteration).
    pub iteration: usize,
    /// Microseconds from arming to its dispatch.
    pub micros: u128,
    /// The y the layout recorded for the last line when it dispatched.
    pub validated_y: i32,
}

/// What one run observed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeliveryTrace {
    /// The `G_PRIORITY_DEFAULT_IDLE` source.
    pub default_idle: Option<Delivery>,
    /// The source at [`ABOVE_VALIDATE_PRIORITY`].
    pub above_validate: Option<Delivery>,
    /// The last line's y once nothing was left to dispatch.
    pub final_y: i32,
    /// Iterations the run took.
    pub iterations: usize,
}

/// A probe view in its host window.
pub struct DeliveryFixture {
    /// The view.
    pub view: DenseView,
    _shown: Presented,
}

impl DeliveryFixture {
    /// Present an empty view in `host`.
    ///
    /// # Errors
    ///
    /// Stops the probe when the window never received a size.
    fn new(recorder: &mut Recorder, host: Host) -> Result<Self, Stop> {
        let view = DenseView::new(gtk4::WrapMode::None);
        let shown = match host {
            Host::Presented => Presented::checked(
                recorder,
                &view.scrolled,
                "control: the presented view's window was realized",
            )?,
            Host::HiddenPage => {
                let stack = gtk4::Stack::new();
                stack.add_named(&gtk4::Label::new(Some("visible page")), Some("visible"));
                stack.add_named(&view.scrolled, Some("hidden"));
                stack.set_visible_child_name("visible");
                Presented::checked(recorder, &stack, "control: the stack's window was realized")?
            }
        };
        settle(LAYOUT_SETTLE);
        Ok(Self {
            view,
            _shown: shown,
        })
    }

    /// Present an empty view in `host` for a sample; `None` when the window
    /// never realized.
    #[must_use]
    pub fn standalone(host: Host) -> Option<Self> {
        Self::new(&mut Recorder::detached(), host).ok()
    }

    /// The y the layout records for the buffer's last line.
    #[must_use]
    pub fn validated_y(&self) -> i32 {
        self.view.view.line_yrange(&self.view.buffer.end_iter()).0
    }

    /// Optionally load the probe document, then arm one source at
    /// `G_PRIORITY_DEFAULT_IDLE` and one at [`ABOVE_VALIDATE_PRIORITY`] and
    /// iterate the default main context (sleeping a millisecond when nothing
    /// is ready, so frames arrive) until both dispatched and nothing changed
    /// for a while.
    #[must_use]
    pub fn trace(&self, load: bool) -> DeliveryTrace {
        if load {
            self.view
                .buffer
                .set_text(&dense_text(LINES, LINE_CHARS, SEED));
        }
        let started = Instant::now();
        let iteration = Rc::new(Cell::new(0usize));
        let default_idle = Rc::new(Cell::new(None::<Delivery>));
        let above_validate = Rc::new(Cell::new(None::<Delivery>));
        for (priority, slot) in [
            (glib::Priority::DEFAULT_IDLE, Rc::clone(&default_idle)),
            (
                glib::Priority::from(ABOVE_VALIDATE_PRIORITY),
                Rc::clone(&above_validate),
            ),
        ] {
            let iteration = Rc::clone(&iteration);
            let view = self.view.view.clone();
            glib::idle_add_local_full(priority, move || {
                slot.set(Some(Delivery {
                    iteration: iteration.get(),
                    micros: started.elapsed().as_micros(),
                    validated_y: view.line_yrange(&view.buffer().end_iter()).0,
                }));
                glib::ControlFlow::Break
            });
        }
        let context = glib::MainContext::default();
        let mut last_change = Instant::now();
        let mut y = self.validated_y();
        while started.elapsed() < BUDGET && iteration.get() < MAX_ITERATIONS {
            iteration.set(iteration.get() + 1);
            if context.iteration(false) {
                let now = self.validated_y();
                if now != y {
                    y = now;
                    last_change = Instant::now();
                }
            } else {
                iteration.set(iteration.get() - 1);
                if default_idle.get().is_some()
                    && above_validate.get().is_some()
                    && last_change.elapsed() >= QUIET
                {
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        DeliveryTrace {
            default_idle: default_idle.get(),
            above_validate: above_validate.get(),
            final_y: self.validated_y(),
            iterations: iteration.get(),
        }
    }
}

fn record(recorder: &mut Recorder, key: &'static str, trace: &DeliveryTrace) {
    let describe = |delivery: Option<Delivery>| {
        delivery.map_or_else(
            || "never".to_owned(),
            |d| {
                format!(
                    "iteration {} after {} us at y {}",
                    d.iteration, d.micros, d.validated_y
                )
            },
        )
    };
    recorder.measure(
        key,
        format!(
            "default idle {}; above validate {}; final y {}; iterations {}",
            describe(trace.default_idle),
            describe(trace.above_validate),
            trace.final_y,
            trace.iterations
        ),
    );
}

fn check_host(recorder: &mut Recorder, host: Host) -> Result<(), Stop> {
    let fixture = DeliveryFixture::new(recorder, host)?;
    let settled = fixture.trace(false);
    let (settled_key, loaded_key) = match host {
        Host::Presented => ("presented_settled", "presented_loaded"),
        Host::HiddenPage => ("hidden_settled", "hidden_loaded"),
    };
    record(recorder, settled_key, &settled);
    recorder.control(
        settled.default_idle.is_some_and(|d| d.iteration <= 2),
        "control: with nothing to validate, a G_PRIORITY_DEFAULT_IDLE source dispatches in the \
         iteration after the higher one",
    )?;
    let loaded = fixture.trace(true);
    record(recorder, loaded_key, &loaded);
    recorder.control(
        loaded.final_y >= MIN_BATCHES * BATCH_PX,
        "control: the view validated the document over at least MIN_BATCHES 2000 px batches",
    )?;
    let (Some(default_idle), Some(above)) = (loaded.default_idle, loaded.above_validate) else {
        return recorder.control(false, "control: both armed sources dispatched");
    };
    recorder.axiom(
        above.validated_y < BATCH_PX,
        "a source one step above the validation idle dispatches before the first 2000 px \
         incremental validation batch completed",
    )?;
    recorder.axiom(
        default_idle.validated_y == loaded.final_y,
        "a G_PRIORITY_DEFAULT_IDLE source does not dispatch until the view validated the whole \
         document",
    )?;
    drop(fixture);
    flush_events();
    Ok(())
}

/// Probe A80.
#[must_use]
pub fn probe_a80() -> Observation {
    Recorder::run(AxiomId::new(80), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        recorder.measure(
            "document",
            format!("{LINES} lines of {LINE_CHARS} characters"),
        );
        check_host(recorder, Host::Presented)?;
        check_host(recorder, Host::HiddenPage)
    })
}
