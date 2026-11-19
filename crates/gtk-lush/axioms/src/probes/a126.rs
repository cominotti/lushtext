// SPDX-License-Identifier: MIT OR Apache-2.0

//! A126: a key press that a capture-phase key controller **stopped**, put
//! back on the display's event queue later with `gdk_display_put_event`, is
//! **delivered again through GTK's normal route** as the same `GdkEvent`:
//! capture controllers see it again, and the focused list's class key binding
//! acts on it from the focus of that time, as it does for a fresh press.
//! Several presses put back in one dispatch arrive in the order they were
//! put, all before a `G_PRIORITY_LOW` idle added in that dispatch.
//!
//! The probe presents a list of rows in a fullscreen window, focuses and
//! selects row 2, and types Down three times with real keyboard input while a
//! capture-phase controller on the list's parent keeps each press and stops
//! it (control: neither focus nor selection moved). From a later dispatch it
//! puts the three presses back, in order, and adds a low-priority idle. It
//! records every press the controller sees afterwards (by event pointer), the
//! idle's place among them, and where the focus and the selection end.
//!
//! `gdk_display_put_event` (deprecated since 4.10, no replacement) appends a
//! new reference to the display's queue (`gdkdisplay.c`, the same body in
//! GTK 4.22.2 and 4.24.1); the backend's event source dispatches the queue
//! head at `G_PRIORITY_DEFAULT` (`gdk/wayland/gdkeventsource.c`), which skips
//! only pending events and, while a frame pauses events, unflushed ones
//! (`_gdk_event_queue_find_first`, `gdkevents.c`).
//!
//! LushText's sliced sidebar collapse (`slice-large-directory-collapse` D22)
//! holds the list-navigation keys pressed while a collapse is pending, so
//! the focus never lands on a hidden row, and puts them back when the
//! collapse has ended, so GTK applies them to the final tree as it applies
//! the keys queued behind `main`'s frozen collapse.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::remote_input::{RemoteInput, fullscreen_window};
use crate::session::{Presented, flush_events, settle, wait_until};
use crate::{AxiomId, Observation};

/// The rows of the fixture list.
pub const ROWS: u32 = 40;
/// The row the probe focuses and selects before typing.
pub const START_ROW: u32 = 2;
/// How many Down presses the probe holds and puts back.
pub const HELD_PRESSES: usize = 3;
/// The X keysym of Down.
const DOWN: u32 = 0xff54;
/// How long the probe waits for each stage.
const STAGE_BUDGET: Duration = Duration::from_secs(5);

/// One mark of the probe's timeline after the put.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mark {
    /// The controller saw a press again: its index among the held presses,
    /// or `None` for a press that was not one of them.
    Seen(Option<usize>),
    /// The low-priority idle added with the put ran.
    LowIdle,
}

/// The probe's fixture: a list of rows under a box carrying the holding
/// controller.
#[derive(Clone)]
pub struct HoldFixture {
    /// The window content: the list's parent, carrying the controller.
    pub column: gtk4::Box,
    /// The list.
    pub list: gtk4::ListView,
    /// Its selection.
    pub selection: gtk4::SingleSelection,
    /// While set, the controller keeps each press and stops it.
    pub holding: Rc<Cell<bool>>,
    /// The presses kept, in arrival order.
    pub held: Rc<RefCell<Vec<gdk::Event>>>,
    /// The presses put back, compared by pointer when they arrive again.
    pub put: Rc<RefCell<Vec<gdk::Event>>>,
    /// The timeline after the put.
    pub marks: Rc<RefCell<Vec<Mark>>>,
}

/// Build the fixture (not yet shown).
#[must_use]
pub fn hold_fixture() -> HoldFixture {
    let names: Vec<String> = (0..ROWS).map(|row| format!("row {row}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let model = gtk4::StringList::new(&names);
    let selection = gtk4::SingleSelection::new(Some(model));
    selection.set_autoselect(false);
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        if let Some(item) = item.downcast_ref::<gtk4::ListItem>() {
            item.set_child(Some(&gtk4::Label::new(None)));
        }
    });
    factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let text = item
            .item()
            .and_downcast::<gtk4::StringObject>()
            .map(|object| object.string());
        if let (Some(label), Some(text)) = (item.child().and_downcast::<gtk4::Label>(), text) {
            label.set_label(&text);
        }
    });
    let list = gtk4::ListView::new(Some(selection.clone()), Some(factory));
    let scroller = gtk4::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .build();
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    column.append(&scroller);
    let fixture = HoldFixture {
        column,
        list,
        selection,
        holding: Rc::default(),
        held: Rc::default(),
        put: Rc::default(),
        marks: Rc::default(),
    };
    let controller = gtk4::EventControllerKey::new();
    controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    {
        let fixture = fixture.clone();
        controller.connect_key_pressed(move |controller, _, _, _| {
            let Some(event) = controller.current_event() else {
                return glib::Propagation::Proceed;
            };
            if fixture.holding.get() {
                fixture.held.borrow_mut().push(event);
                return glib::Propagation::Stop;
            }
            let index = fixture
                .put
                .borrow()
                .iter()
                .position(|put| put.as_ptr() == event.as_ptr());
            fixture.marks.borrow_mut().push(Mark::Seen(index));
            glib::Propagation::Proceed
        });
    }
    fixture.column.add_controller(controller);
    fixture
}

impl HoldFixture {
    /// Put every held press back on the display's queue, in order, and add
    /// a low-priority idle that marks the timeline.
    pub fn put_back(&self) {
        self.holding.set(false);
        let held: Vec<gdk::Event> = self.held.borrow_mut().drain(..).collect();
        let display = self.list.display();
        for event in &held {
            #[expect(
                deprecated,
                reason = "A126 pins gdk_display_put_event, which has no replacement for \
                          re-delivering a press a controller held"
            )]
            display.put_event(event);
        }
        self.put.borrow_mut().extend(held);
        let marks = Rc::clone(&self.marks);
        glib::idle_add_local_full(glib::Priority::LOW, move || {
            marks.borrow_mut().push(Mark::LowIdle);
            glib::ControlFlow::Break
        });
    }

    /// The label of the row holding the window's focus, if the focus is a
    /// row of the list.
    #[must_use]
    pub fn focused_row(&self) -> Option<String> {
        let window = self.list.root().and_downcast::<gtk4::Window>()?;
        let focus = gtk4::prelude::GtkWindowExt::focus(&window)?;
        (focus.parent().as_ref() == Some(self.list.upcast_ref::<gtk4::Widget>()))
            .then(|| focus.first_child().and_downcast::<gtk4::Label>())
            .flatten()
            .map(|label| label.label().to_string())
    }
}

/// Probe A126. See the module documentation.
#[must_use]
pub fn probe_a126() -> Observation {
    Recorder::run(AxiomId::new(126), |recorder| {
        let fixture = hold_fixture();
        let shown = Presented::shown(fullscreen_window(&fixture.column));
        settle(Duration::from_millis(300));
        recorder.control(shown.realized(), "control: the window received a size")?;
        fixture.list.scroll_to(
            START_ROW,
            gtk4::ListScrollFlags::FOCUS | gtk4::ListScrollFlags::SELECT,
            None,
        );
        let start_label = format!("row {START_ROW}");
        let focused = wait_until(STAGE_BUDGET, || {
            fixture.focused_row().as_deref() == Some(start_label.as_str())
        });
        recorder.measure("focus_before", format!("{:?}", fixture.focused_row()));
        recorder.control(
            focused && fixture.selection.selected() == START_ROW,
            "control: the start row is focused and selected",
        )?;
        let input = match RemoteInput::start() {
            Ok(input) => input,
            Err(error) => {
                recorder.measure("remote_desktop_error", error);
                return recorder
                    .control(false, "control: the compositor accepts RemoteDesktop input");
            }
        };

        fixture.holding.set(true);
        for _ in 0..HELD_PRESSES {
            if let Err(error) = input.chord(&[], DOWN) {
                recorder.measure("key_error", error);
                return recorder.control(false, "control: the presses were injected");
            }
        }
        let all_held = wait_until(STAGE_BUDGET, || fixture.held.borrow().len() == HELD_PRESSES);
        settle(Duration::from_millis(100));
        recorder.measure("held_presses", fixture.held.borrow().len());
        recorder.measure("focus_while_held", format!("{:?}", fixture.focused_row()));
        recorder.control(
            all_held
                && fixture.held.borrow().len() == HELD_PRESSES
                && fixture.selection.selected() == START_ROW
                && fixture.focused_row().as_deref() == Some(start_label.as_str()),
            "control: the capture controller held every press and nothing moved",
        )?;

        // From a later dispatch, as the sidebar does when a collapse ends.
        {
            let fixture = fixture.clone();
            glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
                fixture.put_back();
                glib::ControlFlow::Break
            });
        }
        let delivered = wait_until(STAGE_BUDGET, || {
            fixture.marks.borrow().contains(&Mark::LowIdle)
        });
        settle(Duration::from_millis(100));
        let marks = fixture.marks.borrow().clone();
        let end_row = START_ROW + u32::try_from(HELD_PRESSES).unwrap_or(0);
        let end_label = format!("row {end_row}");
        recorder.measure("timeline_after_put", format!("{marks:?}"));
        recorder.measure("focus_after", format!("{:?}", fixture.focused_row()));
        recorder.measure("selected_after", fixture.selection.selected());
        recorder.control(delivered, "control: the low-priority idle ran")?;
        let expected: Vec<Mark> = (0..HELD_PRESSES)
            .map(|index| Mark::Seen(Some(index)))
            .chain(std::iter::once(Mark::LowIdle))
            .collect();
        recorder.axiom(
            marks.iter().filter(|mark| **mark != Mark::LowIdle).count() == HELD_PRESSES
                && marks
                    .iter()
                    .filter(|mark| matches!(mark, Mark::Seen(_)))
                    .all(|mark| matches!(mark, Mark::Seen(Some(_)))),
            "A126: every press put back is delivered again, as the same event, through the \
             capture phase",
        )?;
        recorder.axiom(
            marks == expected,
            "A126: the presses put back in one dispatch arrive in the order put, all before a \
             G_PRIORITY_LOW idle added with them",
        )?;
        recorder.axiom(
            fixture.focused_row().as_deref() == Some(end_label.as_str())
                && fixture.selection.selected() == end_row,
            "A126: the list's class key binding acts on each press put back, from the focus of \
             that time, as on a fresh press",
        )?;
        drop(input);
        drop(shown);
        drop(fixture);
        flush_events();
        Ok(())
    })
}
