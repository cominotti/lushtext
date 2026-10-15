// SPDX-License-Identifier: MIT OR Apache-2.0

//! A37: a change that invalidates `GtkTextView` lines **inside a frame**
//! (input is delivered in the frame clock's flush-events phase; a tick
//! callback runs in its update phase) leaves the view's validation idle
//! (`first_validate_callback`, priority `GTK_PRIORITY_RESIZE - 2`) pending at
//! paint time, and unless the view itself is allocated in that frame's layout
//! phase, `gtk_text_view_paint` flushes the validation **from inside the
//! snapshot**. A line that grew past the viewport then widens the horizontal
//! adjustment there, after the scrollbar was allocated for the frame, and GTK
//! logs `Trying to snapshot GtkGizmo … without a current allocation` for the
//! scrollbar's trough when it snapshots the scrollbar next. An edit and a tag
//! change both count, and so does a change of a view property that changes
//! the default text style (a margin, the tabs, wrapping, line spacing, the
//! indent, justification, editability): its setter invalidates every line and
//! queues no resize, allocation or redraw, so the validation is flushed by the
//! first paint that redraws the view (anything does: a state-flag change, a
//! gutter, the selection). (Not probed: a change made just before a frame that
//! input started behaves the same, because GDK runs such a frame from a source
//! of priority `G_PRIORITY_DEFAULT + 1`, ahead of the validation idle — and
//! every frame of a layout animation starts that way, because GTK synthesizes
//! a motion event after each relayout. The responsiveness lane reproduced it on
//! find, where GtkSourceView's search scan changes tags from an idle and the
//! search bar's slide shrinks the viewport, after which LushText's overscroll
//! idle sets the view's bottom margin.)
//!
//! Queueing the view's allocation before the paint — at once for an edit, a
//! caret move or a default-style property change, and from the frame's
//! `before-paint` or after its layout handlers for a tag change — moves the
//! flush into the layout phase (`gtk_text_view_size_allocate` flushes it),
//! and no adjustment changes inside the paint.
//!
//! The probe changes the view from a tick callback, which runs in the same
//! frame as its layout and paint, and counts the scroll-adjustment emissions
//! during the paint phase: from a one-pixel `GtkDrawingArea` snapshotted before
//! the view (its draw function runs inside the snapshot) to the frame clock's
//! `after-paint`.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{LogCapture, ParagraphKind, paragraph};
use crate::session::{Presented, settle, wait_until};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Characters on the one line of the probe document, far wider than the
/// probe window.
pub const LINE_CHARS: usize = 500;
/// Frames that each change the view once, in each run of the probe.
pub const FRAMES: i32 = 12;
/// Characters a widening tag covers more in each frame.
pub const TAG_STEP: i32 = 10;
/// Pixels the view's left margin grows in each frame.
pub const MARGIN_STEP: i32 = 10;
/// `GtkTextView` properties whose setters change the default text style:
/// each invalidates every line (`gtk_text_layout_default_style_changed`) and
/// queues no resize, allocation or redraw. `notify` follows a real change
/// only, except for `tabs`, which notifies on every call. LushText's
/// `ui/editor_page/frame_validation.rs` keeps the same list.
pub const DEFAULT_STYLE_PROPERTIES: [&str; 12] = [
    "wrap-mode",
    "editable",
    "pixels-above-lines",
    "pixels-below-lines",
    "pixels-inside-wrap",
    "justification",
    "left-margin",
    "right-margin",
    "top-margin",
    "bottom-margin",
    "indent",
    "tabs",
];
/// Seed of the probe paragraph.
pub const SEED: u64 = 37;
/// The warning GTK logs for a widget snapshotted while it awaits allocation.
pub const SNAPSHOT_WARNING: &str = "without a current allocation";

/// How long a probe waits for its changing frames.
const FRAMES_BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// How a probe frame changes the view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameChange {
    /// Type one character at the cursor and scroll it on screen, as a key
    /// press does.
    Type,
    /// Widen the line: a scaled tag covers [`TAG_STEP`] more characters.
    Tag,
    /// Widen the line: the view's left margin grows [`MARGIN_STEP`] pixels,
    /// and the view is redrawn (the setter queues no redraw; in an
    /// application something else redraws the view sooner or later).
    Margin,
}

/// A wrap-off `GtkSourceView` with line numbers holding one
/// [`LINE_CHARS`]-character line, in a `GtkScrolledWindow` whose scrollbars
/// are always shown (no overlay), below a one-pixel paint marker.
#[derive(Clone, Debug)]
pub struct LongLineView {
    /// The box holding the marker and the scrolled window, to present.
    pub container: gtk4::Box,
    /// The scrolled window.
    pub scrolled: gtk4::ScrolledWindow,
    /// The view under observation.
    pub view: sourceview5::View,
    /// The view's buffer, with no language.
    pub buffer: sourceview5::Buffer,
    /// The scaled tag [`FrameChange::Tag`] applies.
    pub widening: gtk4::TextTag,
    /// The paint marker, snapshotted before the view.
    pub marker: gtk4::DrawingArea,
    tagged: Rc<Cell<i32>>,
}

impl LongLineView {
    /// The fixture.
    #[must_use]
    pub fn new() -> Self {
        let buffer = sourceview5::Buffer::new(None);
        buffer.set_text(&paragraph(ParagraphKind::Ascii, LINE_CHARS, SEED));
        buffer.place_cursor(&buffer.start_iter());
        let widening = gtk4::TextTag::builder().scale(1.6).build();
        buffer.tag_table().add(&widening);
        let view = sourceview5::View::with_buffer(&buffer);
        view.set_wrap_mode(gtk4::WrapMode::None);
        view.set_monospace(true);
        view.set_show_line_numbers(true);
        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&view)
            .overlay_scrolling(false)
            .hexpand(true)
            .vexpand(true)
            .build();
        let marker = gtk4::DrawingArea::builder()
            .content_width(1)
            .content_height(1)
            .build();
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        container.append(&marker);
        container.append(&scrolled);
        Self {
            container,
            scrolled,
            view,
            buffer,
            widening,
            marker,
            tagged: Rc::new(Cell::new(0)),
        }
    }

    /// The horizontal adjustment's `upper` and `page-size`.
    #[must_use]
    pub fn horizontal_extent(&self) -> (f64, f64) {
        let adjustment = self.scrolled.hadjustment();
        (adjustment.upper(), adjustment.page_size())
    }

    /// Change the view once per frame for `frames` frames, from a tick
    /// callback (the frame's update phase). The returned counter reaches
    /// `frames` when done.
    #[must_use]
    pub fn change_in_frames(&self, change: FrameChange, frames: i32) -> Rc<Cell<i32>> {
        let done = Rc::new(Cell::new(0));
        let counter = Rc::clone(&done);
        let fixture = self.clone();
        self.view.add_tick_callback(move |view, _| {
            if counter.get() >= frames {
                return gtk4::glib::ControlFlow::Break;
            }
            match change {
                FrameChange::Type => {
                    fixture.buffer.insert_interactive_at_cursor("x", true);
                    view.scroll_mark_onscreen(&fixture.buffer.get_insert());
                }
                FrameChange::Tag => {
                    fixture.tagged.set(fixture.tagged.get() + TAG_STEP);
                    let end = fixture.buffer.iter_at_offset(fixture.tagged.get());
                    fixture
                        .buffer
                        .apply_tag(&fixture.widening, &fixture.buffer.start_iter(), &end);
                }
                FrameChange::Margin => {
                    view.set_left_margin(view.left_margin() + MARGIN_STEP);
                    view.queue_draw();
                }
            }
            counter.set(counter.get() + 1);
            gtk4::glib::ControlFlow::Continue
        });
        done
    }

    /// The workaround: queue the view's allocation at once on an edit, a
    /// caret move or a change of a [`DEFAULT_STYLE_PROPERTIES`] property, and
    /// from the frame's `before-paint` or after its layout handlers when a
    /// tag changed, so the frame's layout phase flushes the pending
    /// validation. Needs the view realized (its frame clock).
    #[must_use]
    pub fn queue_allocation_before_paint(&self) -> bool {
        let Some(clock) = self.view.frame_clock() else {
            return false;
        };
        let view = self.view.downgrade();
        self.buffer.connect_changed(move |_| {
            if let Some(view) = view.upgrade() {
                view.queue_allocate();
            }
        });
        let view = self.view.downgrade();
        self.buffer
            .connect_notify_local(Some("cursor-position"), move |_, _| {
                if let Some(view) = view.upgrade() {
                    view.queue_allocate();
                }
            });
        for property in DEFAULT_STYLE_PROPERTIES {
            self.view
                .connect_notify_local(Some(property), |view, _| view.queue_allocate());
        }
        let tags_changed = Rc::new(Cell::new(false));
        for signal in ["apply-tag", "remove-tag"] {
            let tags_changed = Rc::clone(&tags_changed);
            self.buffer.connect_local(signal, false, move |_| {
                tags_changed.set(true);
                None
            });
        }
        {
            // A tag whose properties change size invalidates every range it
            // covers, like re-applying it.
            let tags_changed = Rc::clone(&tags_changed);
            self.buffer
                .tag_table()
                .connect_tag_changed(move |_, _, size_changed| {
                    if size_changed {
                        tags_changed.set(true);
                    }
                });
        }
        let consume = {
            let view = self.view.downgrade();
            move || {
                if tags_changed.replace(false)
                    && let Some(view) = view.upgrade()
                {
                    view.queue_allocate();
                }
            }
        };
        let at_before_paint = consume.clone();
        clock.connect_before_paint(move |_| at_before_paint());
        clock.connect_closure(
            "layout",
            true,
            gtk4::glib::closure_local!(move |_: gtk4::gdk::FrameClock| consume()),
        );
        true
    }
}

impl Default for LongLineView {
    fn default() -> Self {
        Self::new()
    }
}

/// Counts the scroll-adjustment emissions (`changed` and `value-changed`) a
/// fixture's view makes inside a frame's paint phase: after its marker drew
/// (the marker is redrawn at every `before-paint`, so it draws in every frame)
/// and before the frame clock's `after-paint`. Disconnects everything when
/// dropped.
pub struct PaintPhaseChanges {
    in_paint: Rc<Cell<bool>>,
    changes: Rc<Cell<u32>>,
    clock: gtk4::gdk::FrameClock,
    clock_handlers: Vec<gtk4::glib::SignalHandlerId>,
    adjustment_handlers: Vec<(gtk4::Adjustment, gtk4::glib::SignalHandlerId)>,
}

impl std::fmt::Debug for PaintPhaseChanges {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PaintPhaseChanges")
            .field("changes", &self.changes.get())
            .finish_non_exhaustive()
    }
}

impl PaintPhaseChanges {
    /// Start counting for `fixture`, or `None` before it is realized (no
    /// frame clock yet).
    #[must_use]
    pub fn watch(fixture: &LongLineView) -> Option<Self> {
        let clock = fixture.view.frame_clock()?;
        let in_paint = Rc::new(Cell::new(false));
        let changes = Rc::new(Cell::new(0));
        let entering = Rc::clone(&in_paint);
        fixture
            .marker
            .set_draw_func(move |_, _, _, _| entering.set(true));
        let marker = fixture.marker.clone();
        let leaving = Rc::clone(&in_paint);
        let clock_handlers = vec![
            clock.connect_before_paint(move |_| marker.queue_draw()),
            clock.connect_after_paint(move |_| leaving.set(false)),
        ];
        let mut adjustment_handlers = Vec::new();
        for adjustment in [fixture.view.hadjustment(), fixture.view.vadjustment()]
            .into_iter()
            .flatten()
        {
            let (changed_in_paint, changed) = (Rc::clone(&in_paint), Rc::clone(&changes));
            let changed_id = adjustment.connect_changed(move |_| {
                if changed_in_paint.get() {
                    changed.set(changed.get() + 1);
                }
            });
            let (moved_in_paint, moved) = (Rc::clone(&in_paint), Rc::clone(&changes));
            let moved_id = adjustment.connect_value_changed(move |_| {
                if moved_in_paint.get() {
                    moved.set(moved.get() + 1);
                }
            });
            adjustment_handlers.push((adjustment.clone(), changed_id));
            adjustment_handlers.push((adjustment, moved_id));
        }
        Some(Self {
            in_paint,
            changes,
            clock,
            clock_handlers,
            adjustment_handlers,
        })
    }

    /// Emissions counted so far.
    #[must_use]
    pub fn count(&self) -> u32 {
        self.changes.get()
    }
}

impl Drop for PaintPhaseChanges {
    fn drop(&mut self) {
        for handler in self.clock_handlers.drain(..) {
            self.clock.disconnect(handler);
        }
        for (adjustment, handler) in self.adjustment_handlers.drain(..) {
            adjustment.disconnect(handler);
        }
        self.in_paint.set(false);
    }
}

/// What one run of changing frames observed.
#[derive(Clone, Copy, Debug)]
struct FramesRun {
    frames: i32,
    upper_before: f64,
    upper_after: f64,
    in_paint: u32,
    snapshot_warnings: usize,
}

impl FramesRun {
    fn widened(&self) -> bool {
        self.frames == FRAMES && self.upper_after > self.upper_before
    }
}

/// Change `fixture` in [`FRAMES`] frames and observe.
fn frames_run(fixture: &LongLineView, change: FrameChange) -> Option<FramesRun> {
    let capture = LogCapture::start();
    let changes = PaintPhaseChanges::watch(fixture)?;
    let upper_before = fixture.horizontal_extent().0;
    let done = fixture.change_in_frames(change, FRAMES);
    wait_until(FRAMES_BUDGET, || done.get() >= FRAMES);
    settle(LAYOUT_SETTLE);
    let in_paint = changes.count();
    drop(changes);
    let snapshot_warnings = capture
        .messages()
        .iter()
        .filter(|message| message.message.contains(SNAPSHOT_WARNING))
        .count();
    drop(capture);
    Some(FramesRun {
        frames: done.get(),
        upper_before,
        upper_after: fixture.horizontal_extent().0,
        in_paint,
        snapshot_warnings,
    })
}

fn record_run(recorder: &mut Recorder, keys: &'static [&'static str; 3], run: FramesRun) {
    recorder.measure(
        keys[0],
        format!("{} -> {}", run.upper_before, run.upper_after),
    );
    recorder.measure(keys[1], run.in_paint);
    recorder.measure(keys[2], run.snapshot_warnings);
}

/// Probe A37.
#[must_use]
pub fn probe_a37() -> Observation {
    Recorder::run(AxiomId::new(37), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let fixture = LongLineView::new();
        let _shown = Presented::checked(
            recorder,
            &fixture.container,
            "control: the view was presented",
        )?;
        let (upper, page) = fixture.horizontal_extent();
        recorder.measure("line_upper_and_page", format!("{upper} / {page}"));
        recorder.control(
            upper > page + 1.0,
            "control: the line is wider than the viewport",
        )?;

        let mut runs = Vec::new();
        for (change, keys) in [
            (
                FrameChange::Type,
                &["typed_upper", "typed_in_paint", "typed_snapshot_warnings"],
            ),
            (
                FrameChange::Tag,
                &[
                    "tagged_upper",
                    "tagged_in_paint",
                    "tagged_snapshot_warnings",
                ],
            ),
            (
                FrameChange::Margin,
                &[
                    "margin_upper",
                    "margin_in_paint",
                    "margin_snapshot_warnings",
                ],
            ),
        ] {
            let run = recorder.require(
                frames_run(&fixture, change),
                "control: the view has a frame clock",
            )?;
            record_run(recorder, keys, run);
            runs.push(run);
        }
        let installed = fixture.queue_allocation_before_paint();
        recorder.control(installed, "control: the workaround found the frame clock")?;
        for (change, keys) in [
            (
                FrameChange::Type,
                &[
                    "allocated_typed_upper",
                    "allocated_typed_in_paint",
                    "allocated_typed_snapshot_warnings",
                ],
            ),
            (
                FrameChange::Tag,
                &[
                    "allocated_tagged_upper",
                    "allocated_tagged_in_paint",
                    "allocated_tagged_snapshot_warnings",
                ],
            ),
            (
                FrameChange::Margin,
                &[
                    "allocated_margin_upper",
                    "allocated_margin_in_paint",
                    "allocated_margin_snapshot_warnings",
                ],
            ),
        ] {
            let run = recorder.require(
                frames_run(&fixture, change),
                "control: the view still has a frame clock",
            )?;
            record_run(recorder, keys, run);
            runs.push(run);
        }
        recorder.control(
            runs.iter().all(FramesRun::widened),
            "control: every frame changed the view and every run widened the line",
        )?;

        recorder.axiom(
            runs[0].in_paint > 0,
            "typing inside a frame changes the scroll adjustments inside that frame's paint",
        )?;
        recorder.axiom(
            runs[1].in_paint > 0,
            "a tag change inside a frame changes the scroll adjustments inside that frame's paint",
        )?;
        recorder.axiom(
            runs[2].in_paint > 0,
            "a default-style property change inside a frame (a margin) changes the scroll \
             adjustments inside the paint of a frame that redraws the view",
        )?;
        recorder.axiom(
            runs[3..].iter().all(|run| run.in_paint == 0),
            "queueing the view's allocation before the paint moves every adjustment change into \
             layout",
        )
    })
}
