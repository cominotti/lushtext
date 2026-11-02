// SPDX-License-Identifier: MIT OR Apache-2.0

//! A70: after text is inserted below the viewport of a validated
//! `GtkTextView`, the view lays the inserted lines out in its **incremental
//! validation idle** (`GTK_TEXT_VIEW_PRIORITY_VALIDATE`,
//! `GDK_PRIORITY_REDRAW + 5`), which runs **after** the frame clock's paint
//! source and before the next frame, and which validates the **invalid lines
//! only**, in batches of 2000 px (A31): the cost of that dispatch grows with
//! the visible characters of the inserted lines, in either wrap mode, while
//! the lines already validated are not laid out again. This extends A31 to
//! inserted lines.
//!
//! Consequently, when each insert waits for the frame clock's `after-paint`
//! of a frame that follows the previous insert, **each validation dispatch
//! holds one insert's lines**; inserts made back to back in one dispatch are
//! validated together by one dispatch. The probe tells which lines a
//! dispatch validated from the horizontal adjustment, wrap off: every insert's
//! lines are a little longer than the previous insert's, so the view's width
//! steps once per validated insert.
//!
//! The validation dispatches are isolated structurally: one-shot idles at
//! priorities 124 and 126 bracket the 125 idle, and an iteration that painted
//! a frame is excluded. Their durations against the inserted characters are
//! recorded magnitudes, never verdicts; the sample's `--derive-v-slice` mode
//! bisects the largest insert whose validation dispatch fits half of
//! LushText's `B_SETTLE_US` in both wrap modes (`V_SLICE`).
//!
//! The same holds without frames: when each insert is made from a one-shot
//! idle **below** `GTK_TEXT_VIEW_PRIORITY_VALIDATE` (`G_PRIORITY_DEFAULT_IDLE`)
//! queued by the previous insert, the validation idle of that insert runs
//! first, so each validation dispatch again holds one insert's lines, and the
//! inserts follow each other at main-loop speed rather than one per frame.
//! The probe also counts GTK's `Trying to snapshot GtkGizmo … without a
//! current allocation` warning (A37) in both paced runs: the inserted lines
//! are below the viewport, which a paint never validates.
//!
//! LushText's post-open settle (`bound-post-open-settle`, D6) slices a dense
//! install at `V_SLICE` visible characters and queues each next slice from
//! such an idle (first from the frame clock's `after-paint`, review R14, until
//! the idle-paced install of an 11 MiB document took 16.7 s against 1.9 s),
//! so no validation dispatch holds more than one slice.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::glib;
use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::observation::{Recorder, Stop};
use crate::probes::post_open::{
    Bracketed, DENSE_LINE_CHARS, DenseView, FrameCounter, HALF_SETTLE_US, MAX_ITERATIONS,
    REFERENCE_WINDOW, VALIDATE_PRIORITY, bracketed, dense_text, median, timed_drain,
    timed_iteration,
};
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Lines of the validated document the inserts go below.
pub const PREFIX_LINES: usize = 80;
/// The inserted line counts the probe records the validation cost at.
pub const CURVE_LINES: [usize; 6] = [1, 4, 8, 16, 32, 64];
/// Repetitions per curve point in the probe (the derivation uses
/// [`DERIVATION_REPETITIONS`]).
pub const CURVE_REPETITIONS: usize = 3;
/// Repetitions per bisection point when deriving `V_SLICE`.
pub const DERIVATION_REPETITIONS: usize = 7;
/// The most lines the derivation tries.
pub const DERIVATION_CAP_LINES: usize = 256;
/// Inserts in the pacing comparison.
pub const PACED_INSERTS: usize = 6;
/// Lines per insert in the pacing comparison.
pub const PACED_LINES: usize = 8;
/// Characters each paced insert's lines grow by over the previous insert's.
pub const WIDTH_STEP_CHARS: usize = 6;
/// Seed of the probe documents.
pub const SEED: u64 = 55;

const PACING_BUDGET: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_millis(300);

/// A dense view presented at the reference size, holding `PREFIX_LINES`
/// validated dense lines and scrolled to the top.
pub struct ValidationFixture {
    /// The view.
    pub dense: DenseView,
    /// Counts the view's frames.
    pub frames: FrameCounter,
    _shown: Presented,
    _editor: Option<(crate::probes::a72::DisplayCss, Option<sourceview5::Map>)>,
}

/// LushText's editor font size, pt (`ui/theme.rs`: the system monospace
/// font, `Monospace 11` by default, at zoom 100 %).
pub const EDITOR_FONT_PT: f64 = 11.0;
/// LushText's editor line height (`ui/theme.rs`, `EDITOR_GLYPH_LINE_HEIGHT`).
pub const EDITOR_LINE_HEIGHT: f64 = 1.18;
/// The CSS class the editor-like fixture's view carries.
pub const EDITOR_CLASS: &str = "gtk-lush-axioms-a70-editor";

/// How the derivation's view is dressed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewStyle {
    /// A bare monospace source view (the probe's own fixture).
    Bare,
    /// Like LushText's editor: its font CSS (`Monospace` 11 pt, line height
    /// 1.18), line numbers and the current-line highlight.
    Editor,
    /// [`ViewStyle::Editor`] plus a `GtkSourceMap` of the buffer beside it,
    /// like LushText's minimap.
    EditorWithMap,
}

impl ValidationFixture {
    fn new(recorder: &mut Recorder, wrap: gtk4::WrapMode) -> Result<Self, Stop> {
        Self::styled(recorder, wrap, ViewStyle::Bare)
    }

    fn styled(
        recorder: &mut Recorder,
        wrap: gtk4::WrapMode,
        style: ViewStyle,
    ) -> Result<Self, Stop> {
        let dense = DenseView::new(wrap);
        dense
            .buffer
            .set_text(&dense_text(PREFIX_LINES, DENSE_LINE_CHARS, SEED));
        dense.buffer.place_cursor(&dense.buffer.start_iter());
        let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        container.append(&dense.scrolled);
        let editor = if style == ViewStyle::Bare {
            None
        } else {
            let css = recorder.require(
                crate::probes::a72::DisplayCss::install(&format!(
                    "textview.{EDITOR_CLASS}, textview.{EDITOR_CLASS} text {{ font-family: \
                     \"Monospace\"; font-size: {EDITOR_FONT_PT:.1}pt; line-height: \
                     {EDITOR_LINE_HEIGHT:.2}; }}"
                )),
                "control: a default display exists",
            )?;
            dense.view.add_css_class(EDITOR_CLASS);
            dense.view.set_show_line_numbers(true);
            dense.view.set_highlight_current_line(true);
            let map = (style == ViewStyle::EditorWithMap).then(|| {
                let map = sourceview5::Map::new();
                map.set_view(&dense.view);
                container.append(&map);
                map
            });
            Some((css, map))
        };
        let shown = Presented::checked_sized(
            recorder,
            &container,
            REFERENCE_WINDOW,
            "control: the dense view was presented",
        )?;
        settle(LAYOUT_SETTLE);
        recorder.control(
            crate::probes::post_open::fixed_pitch(&dense.view, None),
            crate::probes::post_open::FIXED_PITCH_CONTROL,
        )?;
        let frames = recorder.require(
            FrameCounter::watch(&dense.view),
            "control: the view has a frame clock",
        )?;
        Ok(Self {
            dense,
            frames,
            _shown: shown,
            _editor: editor,
        })
    }

    /// [`Self::standalone`] dressed as `style`.
    #[must_use]
    pub fn standalone_styled(wrap: gtk4::WrapMode, style: ViewStyle) -> Option<Self> {
        let mut recorder = Recorder::detached();
        Self::styled(&mut recorder, wrap, style).ok()
    }

    /// Build the fixture outside a probe (the sample's derivation). `None`
    /// when it never realized.
    #[must_use]
    pub fn standalone(wrap: gtk4::WrapMode) -> Option<Self> {
        let mut recorder = Recorder::detached();
        Self::new(&mut recorder, wrap).ok()
    }

    /// Append `lines` dense lines and time the validation dispatches that
    /// follow; then remove them again and let the view settle. Returns the
    /// longest frameless validation dispatch, µs, and how many there were.
    #[must_use]
    pub fn validation_after_insert(&self, lines: usize) -> (u128, usize) {
        let buffer = &self.dense.buffer;
        let before = buffer.end_iter().offset();
        self.dense
            .append(&dense_text(lines, DENSE_LINE_CHARS, SEED + 1));
        let observed = bracketed(VALIDATE_PRIORITY, Some(&self.frames), || 0.0);
        let frameless = observed.frameless();
        let longest = frameless
            .iter()
            .map(|(d, _)| d.micros)
            .max()
            .unwrap_or_default();
        let (mut start, mut end) = (buffer.iter_at_offset(before), buffer.end_iter());
        buffer.delete(&mut start, &mut end);
        let _ = timed_drain(MAX_ITERATIONS, None);
        settle(Duration::from_millis(30));
        (longest, frameless.len())
    }

    /// The median over `repetitions` of [`Self::validation_after_insert`]'s
    /// longest dispatch.
    #[must_use]
    pub fn median_validation_us(&self, lines: usize, repetitions: usize) -> u128 {
        let samples: Vec<u128> = (0..repetitions)
            .map(|_| self.validation_after_insert(lines).0)
            .collect();
        median(&samples)
    }

    /// The longest dispatch of each of `repetitions` inserts of `lines`.
    #[must_use]
    pub fn validation_samples_us(&self, lines: usize, repetitions: usize) -> Vec<u128> {
        (0..repetitions)
            .map(|_| self.validation_after_insert(lines).0)
            .collect()
    }
}

/// The text of paced insert `index`: [`PACED_LINES`] lines of
/// `DENSE_LINE_CHARS + (index + 1) * WIDTH_STEP_CHARS` characters.
#[must_use]
pub fn paced_insert_text(index: usize) -> String {
    dense_text(
        PACED_LINES,
        DENSE_LINE_CHARS + (index + 1) * WIDTH_STEP_CHARS,
        SEED + 2 + index as u64,
    )
}

/// How the view's width stepped while [`PACED_INSERTS`] inserts were made.
#[derive(Clone, Debug, Default)]
pub struct PacingRun {
    /// The horizontal `upper` after each dispatch that widened it, with
    /// whether that dispatch painted a frame.
    pub widenings: Vec<(f64, bool)>,
    /// Inserts made.
    pub inserts: usize,
    /// Wall time from the first insert to the last widening, ms.
    pub elapsed_ms: u128,
    /// GTK snapshot-allocation warnings (A37) logged during the run.
    pub snapshot_warnings: usize,
}

/// How the pacing run spaces its inserts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pacing {
    /// All inserts back to back in one dispatch.
    BackToBack,
    /// Each insert from the `after-paint` of a frame after the previous one.
    AfterPaint,
    /// Each insert from a one-shot `G_PRIORITY_DEFAULT_IDLE` idle (below
    /// `GTK_TEXT_VIEW_PRIORITY_VALIDATE`) queued by the previous insert.
    IdleBelowValidate,
}

/// Queue insert `index` and, after it, the next one, each from a one-shot
/// idle below the validation idle's priority.
fn queue_idle_insert(dense: DenseView, inserted: Rc<Cell<usize>>) {
    glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
        let index = inserted.get();
        if index < PACED_INSERTS {
            dense.append(&paced_insert_text(index));
            inserted.set(index + 1);
            queue_idle_insert(dense.clone(), Rc::clone(&inserted));
        }
        glib::ControlFlow::Break
    });
}

/// Make the paced inserts as `pacing` says, and record each dispatch that
/// widened the view.
#[must_use]
pub fn pacing_run(fixture: &ValidationFixture, pacing: Pacing) -> PacingRun {
    let capture = crate::probes::text::LogCapture::start();
    let started = Instant::now();
    let dense = fixture.dense.clone();
    let inserted = Rc::new(Cell::new(0usize));
    let handler = Rc::new(RefCell::new(None));
    if pacing == Pacing::IdleBelowValidate {
        queue_idle_insert(dense.clone(), Rc::clone(&inserted));
    } else if pacing == Pacing::AfterPaint {
        // Every insert, the first included, is made from an `after-paint`:
        // a frame that was already due when an insert was made would run
        // before that insert's validation idle, and its `after-paint` would
        // add the next insert to the same invalid region.
        dense.view.queue_draw();
        if let Some(clock) = dense.view.frame_clock() {
            let (dense, inserted) = (dense.clone(), Rc::clone(&inserted));
            handler.replace(Some((
                clock.clone(),
                clock.connect_after_paint(move |_| {
                    let index = inserted.get();
                    if index < PACED_INSERTS {
                        dense.append(&paced_insert_text(index));
                        inserted.set(index + 1);
                        dense.view.queue_draw();
                    }
                }),
            )));
        }
    } else {
        let (dense, inserted) = (dense.clone(), Rc::clone(&inserted));
        glib::idle_add_local_full(glib::Priority::DEFAULT, move || {
            for index in 0..PACED_INSERTS {
                dense.append(&paced_insert_text(index));
            }
            inserted.set(PACED_INSERTS);
            glib::ControlFlow::Break
        });
    }
    let mut run = PacingRun::default();
    let deadline = Instant::now() + PACING_BUDGET;
    let mut last_change = Instant::now();
    let mut width = dense.uppers().0;
    while Instant::now() < deadline {
        match timed_iteration(Some(&fixture.frames)) {
            Some(dispatch) => {
                let now = dense.uppers().0;
                if now > width {
                    run.widenings.push((now, dispatch.frames > 0));
                    width = now;
                    last_change = Instant::now();
                    run.elapsed_ms = started.elapsed().as_millis();
                }
            }
            None => std::thread::sleep(Duration::from_millis(1)),
        }
        if inserted.get() >= PACED_INSERTS && last_change.elapsed() > QUIET {
            break;
        }
    }
    if let Some((clock, id)) = handler.take() {
        clock.disconnect(id);
    }
    run.inserts = inserted.get();
    run.snapshot_warnings = capture
        .messages()
        .iter()
        .filter(|message| {
            message
                .message
                .contains(crate::probes::a37::SNAPSHOT_WARNING)
        })
        .count();
    run
}

/// Validation cost (median of `repetitions`, longest frameless dispatch) for
/// each of `lines`, in `wrap`.
fn curve(
    fixture: &ValidationFixture,
    lines: &[usize],
    repetitions: usize,
) -> Vec<(usize, u128, usize)> {
    lines
        .iter()
        .map(|&count| {
            let (_, dispatches) = fixture.validation_after_insert(count);
            (
                count,
                fixture.median_validation_us(count, repetitions),
                dispatches,
            )
        })
        .collect()
}

fn describe_curve(points: &[(usize, u128, usize)]) -> String {
    super::join_entries(points.iter().map(|(lines, micros, dispatches)| {
        format!(
            "{} chars: {micros} us in {dispatches} dispatch(es)",
            lines * DENSE_LINE_CHARS
        )
    }))
}

/// The largest line count in `1..=cap` at which **every** one of
/// `repetitions` inserts keeps its longest validation dispatch within
/// [`HALF_SETTLE_US`], by bisection, with every point tried (recorded with its
/// worst repetition); `cap` when even it fits, 0 when one line does not.
#[must_use]
pub fn bisect_lines(
    fixture: &ValidationFixture,
    cap: usize,
    repetitions: usize,
    tried: &mut Vec<(usize, u128)>,
) -> usize {
    let mut fits = |lines: usize| {
        let micros = fixture
            .validation_samples_us(lines, repetitions)
            .into_iter()
            .max()
            .unwrap_or(u128::MAX);
        tried.push((lines, micros));
        micros <= HALF_SETTLE_US
    };
    if fits(cap) {
        return cap;
    }
    if !fits(1) {
        return 0;
    }
    let (mut low, mut high) = (1usize, cap);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if fits(middle) {
            low = middle;
        } else {
            high = middle;
        }
    }
    low
}

/// Probe A70.
#[must_use]
pub fn probe_a70() -> Observation {
    Recorder::run(AxiomId::new(70), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let wrapped = ValidationFixture::new(recorder, gtk4::WrapMode::Word)?;
        let vupper_before = wrapped.dense.uppers().1;
        wrapped
            .dense
            .append(&dense_text(4, DENSE_LINE_CHARS, SEED + 1));
        let vupper_inserted = wrapped.dense.uppers().1;
        let vupper = wrapped.dense.scrolled.vadjustment();
        let observed: Bracketed =
            bracketed(VALIDATE_PRIORITY, Some(&wrapped.frames), || vupper.upper());
        let grown_inside = observed
            .frameless()
            .iter()
            .any(|(_, upper)| *upper > vupper_inserted);
        recorder.measure(
            "wrap_on_upper",
            format!(
                "{vupper_before} -> {vupper_inserted} at insert -> {} validated",
                observed.frameless().last().map_or(0.0, |(_, u)| *u)
            ),
        );
        recorder.control(observed.closed, "control: both priority markers dispatched")?;
        recorder.axiom(
            grown_inside,
            "the inserted lines are laid out by a dispatch at GTK_TEXT_VIEW_PRIORITY_VALIDATE \
             (between idles at 124 and 126), not inside a frame",
        )?;
        let on_curve = curve(&wrapped, &CURVE_LINES, CURVE_REPETITIONS);
        recorder.measure("validation_wrap_on", describe_curve(&on_curve));
        drop(wrapped);

        let unwrapped = ValidationFixture::new(recorder, gtk4::WrapMode::None)?;
        let off_curve = curve(&unwrapped, &CURVE_LINES, CURVE_REPETITIONS);
        recorder.measure("validation_wrap_off", describe_curve(&off_curve));
        recorder.control(
            on_curve
                .iter()
                .chain(&off_curve)
                .all(|(_, micros, d)| *micros > 0 && *d > 0),
            "control: every insert was followed by a validation dispatch",
        )?;

        let base = unwrapped.dense.uppers().0;
        let batched = pacing_run(&unwrapped, Pacing::BackToBack);
        recorder.measure(
            "back_to_back_widenings",
            super::join_entries(batched.widenings.iter().map(|(w, frame)| {
                format!("{}{}", w - base, if *frame { " (in a frame)" } else { "" })
            })),
        );
        let after_batch = unwrapped.dense.uppers().0;
        drop(unwrapped);
        let fresh = ValidationFixture::new(recorder, gtk4::WrapMode::None)?;
        let paced = pacing_run(&fresh, Pacing::AfterPaint);
        drop(fresh);
        let idle_fixture = ValidationFixture::new(recorder, gtk4::WrapMode::None)?;
        let idle = pacing_run(&idle_fixture, Pacing::IdleBelowValidate);
        recorder.measure(
            "idle_paced_widenings",
            super::join_entries(idle.widenings.iter().map(|(w, frame)| {
                format!("{}{}", w - base, if *frame { " (in a frame)" } else { "" })
            })),
        );
        recorder.measure(
            "pacing_elapsed_ms",
            format!(
                "after-paint {} ms, idle below validate {} ms, for {PACED_INSERTS} inserts",
                paced.elapsed_ms, idle.elapsed_ms
            ),
        );
        recorder.measure(
            "snapshot_warnings",
            format!(
                "after-paint {}, idle below validate {}",
                paced.snapshot_warnings, idle.snapshot_warnings
            ),
        );
        recorder.measure(
            "paced_widenings",
            super::join_entries(paced.widenings.iter().map(|(w, frame)| {
                format!("{}{}", w - base, if *frame { " (in a frame)" } else { "" })
            })),
        );
        recorder.control(
            batched.inserts == PACED_INSERTS
                && paced.inserts == PACED_INSERTS
                && idle.inserts == PACED_INSERTS
                && (idle_fixture.dense.uppers().0 - after_batch).abs() < 1.0,
            "control: both runs made every insert and reached the same width",
        )?;
        recorder.axiom(
            batched.widenings.len() == 1,
            "inserts made back to back are validated together by one dispatch",
        )?;
        recorder.axiom(
            paced.widenings.len() == PACED_INSERTS,
            "when each insert waits for the after-paint of the frame after the previous one, \
             each validation dispatch holds one insert's lines",
        )?;
        recorder.axiom(
            idle.widenings.len() == PACED_INSERTS && idle.widenings.iter().all(|(_, frame)| !frame),
            "when each insert waits for an idle below GTK_TEXT_VIEW_PRIORITY_VALIDATE queued by \
             the previous one, each validation dispatch, outside any frame, holds one insert's \
             lines",
        )?;
        recorder.axiom(
            paced.snapshot_warnings == 0 && idle.snapshot_warnings == 0,
            "neither pacing logs the A37 snapshot-allocation warning for inserts below the \
             viewport",
        )
    })
}
