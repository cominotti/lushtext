// SPDX-License-Identifier: GPL-3.0-or-later

//! When the user last pressed a key or button, scrolled, or touched a window
//! (`bound-post-open-settle`, design D2, review R6).
//!
//! **Cross-cutting, no role.** Its consumers are the deferred language
//! attach of `WFR-DOCUMENT-LOAD`, which waits for an input-quiet interval
//! before running the language-definition parse that cannot leave the GTK
//! thread (ledger A67), the settings writer's flush, and every paced loop of
//! the post-open settle ([`paced_idle`]: the dense install's slices, the
//! paced replacement's turns, the restyle's reveals), which yield to the
//! frame that shows the latest input. Only discrete user input counts: key
//! presses, button presses, scroll events and touch begins. Pointer motion,
//! crossing events, releases and touch updates do not, because they can
//! arrive every frame for as long as the pointer moves.
//!
//! Each window installs one capture-phase legacy controller ([`install`]),
//! and records its frame clock's `after-paint`; the time is process-wide,
//! because a parse blocks every window.
//!
//! **Typing wins over pacing.** A paced loop keeps an idle ready for as long
//! as it runs, and each of its dispatches (a slice, then GtkTextView's
//! validation of it) can take most of `B_SETTLE`. A key that arrived during
//! one used to wait for it, and then the frame showing the key could wait
//! for the next: the type-during-settle lane measured key-to-frame up to
//! 42-63 ms against 20-39 ms on main. While a counted input has not been
//! followed by a painted frame of its window, [`paced_idle`] holds the loop's
//! next dispatch until that frame has painted, at most
//! [`INPUT_FRAME_WAIT_US`] after the input (a hidden window paints nothing).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use gtk4::{gdk, glib};

/// How long after a counted input a paced loop waits, at most, for the frame
/// that shows it: 128 ms (a power of two in milliseconds, kind: wall-clock
/// interval, µs), about fifteen 120 Hz frames, past which the window is taken
/// not to be painting (hidden or minimized) and the loop resumes.
pub const INPUT_FRAME_WAIT_US: i64 = 128_000;

/// How often a held paced loop looks again for that frame: 1 ms (kind:
/// polling interval), small against the 8.3 ms reference frame.
pub const INPUT_FRAME_POLL: Duration = Duration::from_millis(1);

thread_local! {
    static LAST_INPUT_US: Cell<Option<i64>> = const { Cell::new(None) };
    /// The window the last counted input went to.
    static LAST_INPUT_WINDOW: RefCell<Option<glib::WeakRef<gtk4::Window>>> =
        const { RefCell::new(None) };
    /// The last `after-paint` of that window's frame clock after the input.
    static LAST_INPUT_PAINT_US: Cell<Option<i64>> = const { Cell::new(None) };
}

fn record_input(window: Option<&gtk4::Window>) {
    LAST_INPUT_US.with(|last| last.set(Some(glib::monotonic_time())));
    LAST_INPUT_WINDOW.with(|last| last.replace(window.map(ObjectExt::downgrade)));
}

fn record_paint(window: &gtk4::Window) {
    let is_input_window = LAST_INPUT_WINDOW.with(|last| {
        last.borrow()
            .as_ref()
            .and_then(glib::WeakRef::upgrade)
            .is_some_and(|input| &input == window)
    });
    if is_input_window {
        LAST_INPUT_PAINT_US.with(|last| last.set(Some(glib::monotonic_time())));
    }
}

/// Whether an event of `event_type` counts as user input for input-quiet.
#[must_use]
pub fn counts_as_input(event_type: gdk::EventType) -> bool {
    matches!(
        event_type,
        gdk::EventType::KeyPress
            | gdk::EventType::ButtonPress
            | gdk::EventType::Scroll
            | gdk::EventType::TouchBegin
    )
}

/// Record user input on `window` from its capture phase, before any child
/// handles it.
pub fn install(window: &(impl IsA<gtk4::Window> + IsA<gtk4::Widget>)) {
    let controller = gtk4::EventControllerLegacy::new();
    controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    controller.connect_event(|controller, event| {
        if counts_as_input(event.event_type()) {
            let window = controller
                .widget()
                .and_then(|widget| widget.downcast::<gtk4::Window>().ok());
            record_input(window.as_ref());
        }
        glib::Propagation::Proceed
    });
    window.add_controller(controller);
    // The frame clock exists from realize on; a window is realized once.
    let watch = |widget: &gtk4::Widget| {
        let (Some(clock), Ok(window)) = (
            widget.frame_clock(),
            widget.clone().downcast::<gtk4::Window>(),
        ) else {
            return;
        };
        let window = window.downgrade();
        clock.connect_after_paint(move |_| {
            if let Some(window) = window.upgrade() {
                record_paint(&window);
            }
        });
    };
    let widget = window.upcast_ref::<gtk4::Widget>();
    if widget.is_realized() {
        watch(widget);
    } else {
        widget.connect_realize(move |widget| watch(widget));
    }
}

/// Whether a counted input is still waiting for the frame that shows it: no
/// `after-paint` of its window since, and less than [`INPUT_FRAME_WAIT_US`]
/// ago.
#[must_use]
pub fn input_awaits_frame() -> bool {
    let Some(input) = last_input_us() else {
        return false;
    };
    let painted = LAST_INPUT_PAINT_US
        .with(Cell::get)
        .is_some_and(|paint| paint >= input);
    !painted && glib::monotonic_time() - input < INPUT_FRAME_WAIT_US
}

/// Run `step` once from an idle at `priority`, the way every paced loop of
/// the post-open settle queues its next dispatch, except that while
/// [`input_awaits_frame`] the idle waits for that frame instead: it polls
/// every [`INPUT_FRAME_POLL`] at the same priority, then queues the idle
/// again. `store` receives each source the wait queues, so the caller always
/// holds the live one to remove.
pub fn paced_idle(
    priority: glib::Priority,
    step: impl FnOnce() + 'static,
    store: impl Fn(glib::SourceId) + 'static,
) {
    let step: PacedStep = Rc::new(Cell::new(Some(Box::new(step))));
    let store: Rc<dyn Fn(glib::SourceId)> = Rc::new(store);
    queue_paced_idle(priority, step, &store);
}

/// [`paced_idle`] at `priority`, aged: if it has not run `age` after it was
/// queued (higher-priority work kept it waiting that long), its step is queued
/// once more as a [`paced_idle`] at `aged_priority`, from a timeout at that
/// priority, and runs from whichever dispatches first. `store` always receives
/// the source that holds the step, so removing the stored source cancels it:
/// the aging timeout holds the step only weakly, and a source whose step was
/// taken does nothing (and stores nothing) when it dispatches.
pub fn paced_idle_aging(
    priority: glib::Priority,
    aged_priority: glib::Priority,
    age: std::time::Duration,
    step: impl FnOnce() + 'static,
    store: impl Fn(glib::SourceId) + 'static,
) {
    // The aging timeout is removed when the step runs or is dropped (the
    // caller removed its source): the guard travels with the step.
    let aging: Rc<Cell<Option<glib::SourceId>>> = Rc::default();
    let guard = AgingTimeoutGuard(Rc::clone(&aging));
    let step: PacedStep = Rc::new(Cell::new(Some(Box::new(move || {
        drop(guard);
        step();
    }))));
    let store: Rc<dyn Fn(glib::SourceId)> = Rc::new(store);
    let weak_step = Rc::downgrade(&step);
    let aging_store = Rc::clone(&store);
    queue_paced_idle(priority, step, &store);
    let fired = Rc::clone(&aging);
    let timeout = glib::timeout_add_local_full(age, aged_priority, move || {
        // This source ends now: the guard must not remove it again.
        fired.take();
        if let Some(waiting) = weak_step.upgrade()
            && let Some(step) = waiting.take()
        {
            let aged: PacedStep = Rc::new(Cell::new(Some(step)));
            queue_paced_idle(aged_priority, aged, &aging_store);
        }
        glib::ControlFlow::Break
    });
    aging.set(Some(timeout));
}

/// Removes a `paced_idle_aging` timeout that has not fired yet.
struct AgingTimeoutGuard(Rc<Cell<Option<glib::SourceId>>>);

impl Drop for AgingTimeoutGuard {
    fn drop(&mut self) {
        if let Some(timeout) = self.0.take() {
            timeout.remove();
        }
    }
}

/// The one step a [`paced_idle`] runs, taken when it runs.
type PacedStep = Rc<Cell<Option<Box<dyn FnOnce()>>>>;

fn queue_paced_idle(priority: glib::Priority, step: PacedStep, store: &Rc<dyn Fn(glib::SourceId)>) {
    let id = {
        let store = Rc::clone(store);
        glib::idle_add_local_full(priority, move || {
            if !step_pending(&step) {
                // Taken by an aged copy (`paced_idle_aging`), which the caller
                // holds instead.
                return glib::ControlFlow::Break;
            }
            if input_awaits_frame() {
                let (step, poll_store) = (Rc::clone(&step), Rc::clone(&store));
                let id = glib::timeout_add_local_full(INPUT_FRAME_POLL, priority, move || {
                    if !step_pending(&step) {
                        return glib::ControlFlow::Break;
                    }
                    if input_awaits_frame() {
                        return glib::ControlFlow::Continue;
                    }
                    queue_paced_idle(priority, Rc::clone(&step), &poll_store);
                    glib::ControlFlow::Break
                });
                store(id);
            } else if let Some(step) = step.take() {
                step();
            }
            glib::ControlFlow::Break
        })
    };
    store(id);
}

fn step_pending(step: &PacedStep) -> bool {
    let taken = step.take();
    let pending = taken.is_some();
    step.set(taken);
    pending
}

/// The monotonic time of the last counted input on any window, or `None`.
#[must_use]
pub fn last_input_us() -> Option<i64> {
    LAST_INPUT_US.with(Cell::get)
}

/// Record input now, as a counted event would (widget tests drive the attach
/// deadline with it, since synthetic key events bypass the capture phase).
#[cfg(feature = "test-utils")]
pub fn simulate_user_input() {
    record_input(None);
}
