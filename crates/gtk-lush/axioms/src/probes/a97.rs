// SPDX-License-Identifier: MIT OR Apache-2.0

//! A97: a `GtkSearchEntry` emits `stop-search` on Escape **whether or not it
//! holds text**, and keeps its text; a capture-phase `GtkEventControllerKey`
//! on the entry sees a Backspace press **before** the entry handles it (the
//! text it reads is the text before the deletion), sees it while the entry is
//! empty, and sees the key's **release**. Holding Backspace past the
//! repeat delay delivers **several presses with no release between them**
//! (Wayland key repeat is synthesized by the client), then one release.
//!
//! Real keys go through the private compositor (RemoteDesktop,
//! `remote_input`). LushText's command palette (`drill-down-palette-categories`
//! D5) routes Escape in its entry through `stop-search`, and leaves a category
//! view on Backspace only once a release was seen on the empty query, so the
//! auto-repeat of the Backspace that emptied it does not leave.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::remote_input::{RemoteInput, fullscreen_window};
use crate::session::{Presented, settle, wait_until};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// `Escape`.
pub const ESCAPE: u32 = 0xff1b;
/// `BackSpace`.
pub const BACKSPACE: u32 = 0xff08;
/// How long the probe holds Backspace: past GTK's default 500 ms repeat delay.
pub const HOLD: Duration = Duration::from_millis(1_200);

/// One key event the capture controller saw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seen {
    /// A press of `key`, with the entry's text when the controller saw it.
    Press(gtk4::gdk::Key, String),
    /// A release of `key`.
    Release(gtk4::gdk::Key),
}

/// The entry, its event log and its `stop-search` count.
#[derive(Clone)]
pub struct EntryFixture {
    /// The search entry under test.
    pub entry: gtk4::SearchEntry,
    /// Key events the capture-phase controller saw, in order.
    pub seen: Rc<RefCell<Vec<Seen>>>,
    /// `stop-search` emissions.
    pub stops: Rc<RefCell<u32>>,
}

/// A search entry with a capture-phase key controller that logs and never
/// stops events, and a `stop-search` counter.
#[must_use]
pub fn entry_fixture() -> EntryFixture {
    let entry = gtk4::SearchEntry::new();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let stops = Rc::new(RefCell::new(0u32));
    let controller = gtk4::EventControllerKey::new();
    controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    {
        let seen = Rc::clone(&seen);
        let entry_weak = entry.downgrade();
        controller.connect_key_pressed(move |_, key, _, _| {
            let text = entry_weak
                .upgrade()
                .map(|entry| entry.text().to_string())
                .unwrap_or_default();
            seen.borrow_mut().push(Seen::Press(key, text));
            glib::Propagation::Proceed
        });
    }
    {
        let seen = Rc::clone(&seen);
        controller.connect_key_released(move |_, key, _, _| {
            seen.borrow_mut().push(Seen::Release(key));
        });
    }
    entry.add_controller(controller);
    {
        let stops = Rc::clone(&stops);
        entry.connect_stop_search(move |_| *stops.borrow_mut() += 1);
    }
    EntryFixture { entry, seen, stops }
}

/// Presses of `key` and releases of `key` in `seen`, and whether every press
/// came before the first release.
#[must_use]
pub fn presses_then_release(seen: &[Seen], key: gtk4::gdk::Key) -> (usize, usize, bool) {
    let presses = seen
        .iter()
        .filter(|event| matches!(event, Seen::Press(k, _) if *k == key))
        .count();
    let releases = seen
        .iter()
        .filter(|event| matches!(event, Seen::Release(k) if *k == key))
        .count();
    let first_release = seen
        .iter()
        .position(|event| matches!(event, Seen::Release(k) if *k == key));
    let last_press = seen
        .iter()
        .rposition(|event| matches!(event, Seen::Press(k, _) if *k == key));
    let ordered = match (last_press, first_release) {
        (Some(press), Some(release)) => press < release,
        _ => false,
    };
    (presses, releases, ordered)
}

/// Probe A97. See the module documentation.
#[must_use]
pub fn probe_a97() -> Observation {
    Recorder::run(AxiomId::new(97), |recorder| {
        let fixture = entry_fixture();
        let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        column.append(&fixture.entry);
        let shown = Presented::checked_window(
            recorder,
            fullscreen_window(&column),
            "control: the entry was presented",
        )?;
        let window = shown.window().clone();
        recorder.control(
            wait_until(Duration::from_secs(5), || window.is_fullscreen()),
            "control: the window is fullscreen",
        )?;
        fixture.entry.grab_focus();
        settle(LAYOUT_SETTLE);
        let input = match RemoteInput::start() {
            Ok(input) => input,
            Err(error) => {
                recorder.measure("remote_desktop_error", error);
                return recorder
                    .control(false, "control: the compositor accepts RemoteDesktop input");
            }
        };
        recorder.control(
            fixture.entry.has_focus() || fixture.entry.focus_child().is_some(),
            "control: the entry holds the keyboard focus",
        )?;

        // Escape with text, then on an empty entry.
        fixture.entry.set_text("rs");
        settle(LAYOUT_SETTLE);
        let tap = |keysym: u32| -> Result<(), String> {
            input.key(keysym, true)?;
            input.key(keysym, false)?;
            settle(LAYOUT_SETTLE);
            Ok(())
        };
        if let Err(error) = tap(ESCAPE) {
            recorder.measure("input_error", error);
            return recorder.control(false, "control: Escape was injected");
        }
        let stops_with_text = *fixture.stops.borrow();
        let text_after_escape = fixture.entry.text().to_string();
        fixture.entry.set_text("");
        settle(LAYOUT_SETTLE);
        let _ = tap(ESCAPE);
        let stops_empty = *fixture.stops.borrow() - stops_with_text;
        recorder.measure("stop_search_with_text", stops_with_text);
        recorder.measure("text_after_escape", &text_after_escape);
        recorder.measure("stop_search_empty", stops_empty);

        // Backspace on "x": the controller reads the text before the delete.
        fixture.seen.borrow_mut().clear();
        fixture.entry.set_text("x");
        fixture.entry.set_position(-1);
        settle(LAYOUT_SETTLE);
        let _ = tap(BACKSPACE);
        let first = fixture.seen.borrow().clone();
        let text_seen_at_press = first.iter().find_map(|event| match event {
            Seen::Press(key, text) if *key == gtk4::gdk::Key::BackSpace => Some(text.clone()),
            _ => None,
        });
        recorder.measure("text_seen_at_backspace", format!("{text_seen_at_press:?}"));
        recorder.measure("text_after_backspace", fixture.entry.text());
        let release_seen = first.contains(&Seen::Release(gtk4::gdk::Key::BackSpace));

        // Hold Backspace on the empty entry: repeats without releases.
        fixture.seen.borrow_mut().clear();
        if let Err(error) = input.key(BACKSPACE, true) {
            recorder.measure("input_error", error);
            return recorder.control(false, "control: Backspace was injected");
        }
        settle(HOLD);
        let _ = input.key(BACKSPACE, false);
        settle(LAYOUT_SETTLE);
        let held = fixture.seen.borrow().clone();
        let (presses, releases, ordered) = presses_then_release(&held, gtk4::gdk::Key::BackSpace);
        let empty_at_press = held.iter().all(|event| match event {
            Seen::Press(_, text) => text.is_empty(),
            Seen::Release(_) => true,
        });
        recorder.measure("held_presses", presses);
        recorder.measure("held_releases", releases);
        recorder.measure("held_presses_before_release", ordered);

        recorder.axiom(
            stops_with_text == 1
                && stops_empty == 1
                && text_after_escape == "rs"
                && text_seen_at_press.as_deref() == Some("x")
                && release_seen
                && presses >= 2
                && releases == 1
                && ordered
                && empty_at_press,
            "A97: stop-search fires on Escape with or without text and keeps it; a capture \
             controller sees Backspace before the entry, on an empty entry, its release, and \
             a held key's repeats as presses without releases",
        )
    })
}
