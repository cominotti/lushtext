// SPDX-License-Identifier: MIT OR Apache-2.0

//! A59: a capture-phase `GtkGestureClick` and `GtkGestureDrag` on a
//! `GtkTextView` (all buttons) that claim every press sequence keep the
//! view's own click and drag gestures from ever running, so none of what
//! their handlers do happens: the insert mark and the selection bound stay
//! put, the input method is not reset (the stock click handler resets it on
//! every `n_press == 1`, editable or not), the middle-click primary paste and
//! the default context menu (`do_popup`) never start, and no selection drag
//! or drag-and-drop begins.
//!
//! The fixture is the protected configuration (`editable` and
//! `cursor-visible` false) with the probe input method, so the control half
//! also shows what a non-editable view still does without the claim. Input is
//! real: the probe drives the private compositor's RemoteDesktop pointer
//! (`remote_input`). A touchscreen drag is not driven: RemoteDesktop touch
//! needs a screen-cast stream, which the private session (no PipeWire) cannot
//! create, so the "unclaimed touch drag scrolls" half of the design's P1 is
//! recorded as not pinned.
//!
//! Protected View (C2) installs this claim on entry (design D3, step 3).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::remote_input::{
    BUTTON_MIDDLE, BUTTON_PRIMARY, BUTTON_SECONDARY, RemoteInput, centre_on_screen,
    fullscreen_window,
};
use crate::probes::text::{PROBE_IM_ID, TextViewFixture, register_probe_im, take_im_requests};
use crate::session::{Presented, settle, wait_until};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Lines in the probe document.
pub const LINES: usize = 400;
/// Time between two clicks, longer than GTK's double-click time, so each
/// press is an `n_press == 1`.
const BETWEEN_CLICKS: Duration = Duration::from_millis(600);
/// Pixels the probe drag travels, in steps.
const DRAG_STEP: f64 = 15.0;
const DRAG_STEPS: u32 = 6;

/// The probe document: [`LINES`] short lines of words.
#[must_use]
pub fn document() -> String {
    (0..LINES)
        .map(|line| format!("line {line:05} alpha beta gamma delta epsilon\n"))
        .collect()
}

/// A protected-configuration view (`editable` and `cursor-visible` false)
/// over [`document`] with the insert mark at the start, using the probe input
/// method when it is registered.
#[must_use]
pub fn protected_view() -> TextViewFixture {
    let fixture = TextViewFixture::new(gtk4::WrapMode::WordChar);
    if register_probe_im() {
        fixture.view.set_im_module(Some(PROBE_IM_ID));
    }
    fixture.buffer.set_text(&document());
    fixture.buffer.place_cursor(&fixture.buffer.start_iter());
    fixture.view.set_editable(false);
    fixture.view.set_cursor_visible(false);
    fixture
}

/// What the view's own (bubble-phase) click and drag gestures did.
#[derive(Debug, Default)]
pub struct StockGestureLog {
    /// `pressed` emissions of the view's click gesture, with their button.
    pub pressed: RefCell<Vec<u32>>,
    /// `drag-begin` emissions of the view's drag gesture.
    pub drag_begins: Cell<u32>,
    /// `drag-update` emissions of the view's drag gesture.
    pub drag_updates: Cell<u32>,
    /// Button presses that reached the view at all (capture phase, logged
    /// first, never claimed): the control that input arrived.
    pub arrived: RefCell<Vec<(u32, f64, f64)>>,
}

impl StockGestureLog {
    /// Clear every count.
    pub fn clear(&self) {
        self.pressed.borrow_mut().clear();
        self.drag_begins.set(0);
        self.drag_updates.set(0);
        self.arrived.borrow_mut().clear();
    }
}

/// Count what `view`'s own click and drag gestures do, and every button press
/// that reaches it. Returns `None` when the view has no bubble-phase click or
/// drag gesture (a toolkit change the probe reports as an invalid fixture).
#[must_use]
pub fn observe_stock_gestures(view: &gtk4::TextView) -> Option<Rc<StockGestureLog>> {
    let log = Rc::new(StockGestureLog::default());
    let controllers = view.observe_controllers();
    let mut click = None;
    let mut drag = None;
    for index in 0..controllers.n_items() {
        let Some(controller) = controllers.item(index) else {
            continue;
        };
        if controller
            .downcast_ref::<gtk4::EventController>()
            .is_none_or(|controller| {
                controller.propagation_phase() != gtk4::PropagationPhase::Bubble
            })
        {
            continue;
        }
        if let Some(gesture) = controller.downcast_ref::<gtk4::GestureClick>() {
            click.get_or_insert_with(|| gesture.clone());
        } else if let Some(gesture) = controller.downcast_ref::<gtk4::GestureDrag>() {
            drag.get_or_insert_with(|| gesture.clone());
        }
    }
    let click = click?;
    let drag = drag?;
    {
        let log = log.clone();
        click.connect_pressed(move |gesture, _, _, _| {
            log.pressed.borrow_mut().push(gesture.current_button());
        });
    }
    {
        let log = log.clone();
        drag.connect_drag_begin(move |_, _, _| log.drag_begins.set(log.drag_begins.get() + 1));
    }
    {
        let log = log.clone();
        drag.connect_drag_update(move |_, _, _| {
            log.drag_updates.set(log.drag_updates.get() + 1);
        });
    }
    let arrivals = gtk4::EventControllerLegacy::new();
    arrivals.set_propagation_phase(gtk4::PropagationPhase::Capture);
    {
        let log = log.clone();
        arrivals.connect_event(move |_, event| {
            if event.event_type() == gtk4::gdk::EventType::ButtonPress
                && let Some(button) = event.downcast_ref::<gtk4::gdk::ButtonEvent>()
            {
                let (x, y) = event.position().unwrap_or((-1.0, -1.0));
                log.arrived.borrow_mut().push((button.button(), x, y));
            }
            gtk4::glib::Propagation::Proceed
        });
    }
    view.add_controller(arrivals);
    Some(log)
}

/// The capture-phase claim Protected View installs: a click and a drag
/// gesture for every button that claim each sequence they see.
#[derive(Clone, Debug)]
pub struct CaptureClaim {
    click: gtk4::GestureClick,
    drag: gtk4::GestureDrag,
    /// Sequences the claim took.
    pub claimed: Rc<Cell<u32>>,
}

impl CaptureClaim {
    /// Install the claim on `view`.
    #[must_use]
    pub fn install(view: &gtk4::TextView) -> Self {
        let claimed = Rc::new(Cell::new(0));
        let click = gtk4::GestureClick::new();
        click.set_button(0);
        click.set_propagation_phase(gtk4::PropagationPhase::Capture);
        {
            let claimed = claimed.clone();
            click.connect_pressed(move |gesture, _, _, _| {
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                claimed.set(claimed.get() + 1);
            });
        }
        let drag = gtk4::GestureDrag::new();
        drag.set_button(0);
        drag.set_propagation_phase(gtk4::PropagationPhase::Capture);
        drag.connect_drag_begin(|gesture, _, _| {
            gesture.set_state(gtk4::EventSequenceState::Claimed);
        });
        view.add_controller(click.clone());
        view.add_controller(drag.clone());
        Self {
            click,
            drag,
            claimed,
        }
    }

    /// Remove the claim from `view`.
    pub fn remove(&self, view: &gtk4::TextView) {
        view.remove_controller(&self.click);
        view.remove_controller(&self.drag);
    }
}

/// The view's buffer state a press could change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MarkState {
    /// The insert mark's offset.
    pub insert: i32,
    /// The selection bound's offset.
    pub selection_bound: i32,
    /// Characters in the buffer.
    pub chars: i32,
}

/// `fixture`'s [`MarkState`].
#[must_use]
pub fn mark_state(fixture: &TextViewFixture) -> MarkState {
    let buffer = &fixture.buffer;
    MarkState {
        insert: buffer.iter_at_mark(&buffer.get_insert()).offset(),
        selection_bound: buffer.iter_at_mark(&buffer.selection_bound()).offset(),
        chars: buffer.char_count(),
    }
}

/// Whether the view shows a popover (its default context menu).
#[must_use]
pub fn popover_visible(view: &impl IsA<gtk4::Widget>) -> bool {
    let mut child = view.first_child();
    while let Some(widget) = child {
        if widget.is::<gtk4::Popover>() && widget.is_visible() {
            return true;
        }
        child = widget.next_sibling();
    }
    false
}

fn close_popovers(view: &gtk4::TextView) {
    let mut child = view.first_child();
    while let Some(widget) = child {
        if let Some(popover) = widget.downcast_ref::<gtk4::Popover>() {
            popover.popdown();
        }
        child = widget.next_sibling();
    }
}

/// What one round of presses did.
#[derive(Debug)]
struct Round {
    after_primary: MarkState,
    reset_requests: usize,
    menu_shown: bool,
    after_middle: MarkState,
    after_drag: MarkState,
    stock_pressed: Vec<u32>,
    stock_drag_begins: u32,
    stock_drag_updates: u32,
    arrived: Vec<(u32, f64, f64)>,
}

fn press_round(
    input: &RemoteInput,
    fixture: &TextViewFixture,
    log: &StockGestureLog,
    (x, y): (f64, f64),
) -> Result<Round, String> {
    fixture.buffer.place_cursor(&fixture.buffer.start_iter());
    settle(LAYOUT_SETTLE);
    let _ = take_im_requests();
    log.clear();

    input.move_to(x, y)?;
    input.click(BUTTON_PRIMARY)?;
    settle(BETWEEN_CLICKS);
    let after_primary = mark_state(fixture);
    let reset_requests = take_im_requests()
        .iter()
        .filter(|request| request.trigger == "reset")
        .count();

    input.click(BUTTON_SECONDARY)?;
    let menu_shown = wait_until(LAYOUT_SETTLE, || popover_visible(&fixture.view));
    close_popovers(&fixture.view);
    settle(BETWEEN_CLICKS);

    input.move_to(x, y + 40.0)?;
    input.click(BUTTON_MIDDLE)?;
    settle(BETWEEN_CLICKS);
    let after_middle = mark_state(fixture);

    fixture.buffer.place_cursor(&fixture.buffer.start_iter());
    settle(LAYOUT_SETTLE);
    input.move_to(x - 60.0, y - 60.0)?;
    input.button(BUTTON_PRIMARY, true)?;
    for _ in 0..DRAG_STEPS {
        input.move_by(DRAG_STEP, DRAG_STEP)?;
    }
    input.button(BUTTON_PRIMARY, false)?;
    settle(BETWEEN_CLICKS);
    let after_drag = mark_state(fixture);

    Ok(Round {
        after_primary,
        reset_requests,
        menu_shown,
        after_middle,
        after_drag,
        stock_pressed: log.pressed.borrow().clone(),
        stock_drag_begins: log.drag_begins.get(),
        stock_drag_updates: log.drag_updates.get(),
        arrived: log.arrived.borrow().clone(),
    })
}

fn describe_round(round: &Round) -> String {
    format!(
        "primary {:?}; im resets {}; menu {}; middle {:?}; drag {:?}; stock pressed {:?}, \
         drag-begin {}, drag-update {}; arrived {:?}",
        round.after_primary,
        round.reset_requests,
        round.menu_shown,
        round.after_middle,
        round.after_drag,
        round.stock_pressed,
        round.stock_drag_begins,
        round.stock_drag_updates,
        round.arrived,
    )
}

/// Probe A59.
#[must_use]
pub fn probe_a59() -> Observation {
    Recorder::run(AxiomId::new(59), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        recorder.control(
            register_probe_im(),
            "control: the probe input method is registered on gtk-im-module",
        )?;
        let fixture = protected_view();
        let log = recorder.require(
            observe_stock_gestures(&fixture.view),
            "control: the view has its own bubble-phase click and drag gestures",
        )?;
        let shown = Presented::checked_window(
            recorder,
            fullscreen_window(&fixture.scrolled),
            "control: the text view was presented",
        )?;
        let window = shown.window().clone();
        recorder.control(
            wait_until(Duration::from_secs(5), || window.is_fullscreen()),
            "control: the window is fullscreen, so its coordinates are screen coordinates",
        )?;
        settle(LAYOUT_SETTLE);
        let centre = recorder.require(
            centre_on_screen(&fixture.view),
            "control: the view's centre maps to the window",
        )?;
        recorder.measure("view_centre", format!("{centre:?}"));
        let input = match RemoteInput::start() {
            Ok(input) => input,
            Err(error) => {
                recorder.measure("remote_desktop_error", error);
                return recorder
                    .control(false, "control: the compositor accepts RemoteDesktop input");
            }
        };
        let start = mark_state(&fixture);
        recorder.measure("start", format!("{start:?}"));

        let stock = press_round(&input, &fixture, &log, centre);
        let stock = match stock {
            Ok(round) => round,
            Err(error) => {
                recorder.measure("input_error", error);
                return recorder.control(false, "control: every injected event was accepted");
            }
        };
        recorder.measure("without_claim", describe_round(&stock));
        recorder.control(
            stock.arrived.len() >= 4,
            "control: every press reached the view",
        )?;
        recorder.control(
            stock.after_primary.insert != start.insert,
            "control: without the claim, a primary click moves the insert mark of a non-editable view",
        )?;
        recorder.control(
            stock.reset_requests > 0,
            "control: without the claim, the click resets the input method",
        )?;
        recorder.control(
            stock.menu_shown,
            "control: without the claim, a secondary click opens the default context menu",
        )?;
        recorder.control(
            stock.stock_pressed.contains(&2),
            "control: without the claim, a middle press reaches the view's click handler (the \
             primary-paste site)",
        )?;
        recorder.control(
            stock.stock_drag_updates > 0
                && stock.after_drag.insert != stock.after_drag.selection_bound,
            "control: without the claim, a primary drag runs the view's drag gesture and selects",
        )?;

        let claim = CaptureClaim::install(&fixture.view);
        let claimed = press_round(&input, &fixture, &log, centre);
        claim.remove(&fixture.view);
        let claimed = match claimed {
            Ok(round) => round,
            Err(error) => {
                recorder.measure("input_error", error);
                return recorder.control(false, "control: every injected event was accepted");
            }
        };
        recorder.measure("with_claim", describe_round(&claimed));
        recorder.measure("sequences_claimed", claim.claimed.get());
        recorder.measure(
            "touchscreen_drag",
            "not driven: RemoteDesktop touch needs a screen-cast stream the private session \
             (no PipeWire) cannot create",
        );
        recorder.control(
            claimed.arrived.len() >= 4 && claim.claimed.get() >= 3,
            "control: with the claim, every press still reached the view and was claimed",
        )?;
        recorder.axiom(
            claimed.stock_pressed.is_empty()
                && claimed.stock_drag_begins == 0
                && claimed.stock_drag_updates == 0,
            "a capture-phase claim keeps the view's own click and drag gestures from running",
        )?;
        recorder.axiom(
            claimed.after_primary == start
                && claimed.after_middle == start
                && claimed.after_drag == start,
            "with the claim, no press, middle press or drag moves the insert mark or the \
             selection bound or changes the buffer",
        )?;
        recorder.axiom(
            claimed.reset_requests == 0,
            "with the claim, a press does not reset the input method",
        )?;
        recorder.axiom(
            !claimed.menu_shown,
            "with the claim, a secondary press opens no context menu",
        )
    })
}
