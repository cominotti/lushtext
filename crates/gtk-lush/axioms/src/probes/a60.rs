// SPDX-License-Identifier: MIT OR Apache-2.0

//! A60: a non-editable `GtkTextView` keeps its input method out of focus and
//! key handling. `set_editable(false)` sets the view's key controller's
//! `im-context` to none, so key presses never reach the input method's
//! `filter_keypress`; and a later focus-in does not call
//! `gtk_im_context_focus_in` (gtktextview.c `gtk_text_view_focus_in` calls it
//! only while editable). An editable view does both.
//!
//! The probe watches its own `GtkIMContext` (the probe input method selected
//! through the view's `im-module`), activates the window and moves keyboard
//! focus with a real pointer press on a neighbouring button (RemoteDesktop,
//! `remote_input`), and presses a real key.
//!
//! Protected View (C2) relies on it: a non-editable view hands no paragraph
//! to the input method on focus or typing (A30), without switching the
//! process's input-method module.

use std::time::Duration;

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::remote_input::{
    BUTTON_PRIMARY, RemoteInput, centre_on_screen, fullscreen_window, keysym,
};
use crate::probes::text::{
    ImRequest, PROBE_IM_ID, TextViewFixture, register_probe_im, take_im_requests,
};
use crate::session::{Presented, settle, wait_until};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// A view using the probe input method beside a button that can take focus,
/// both in one vertical box. `None` when the probe input method cannot be
/// registered.
#[must_use]
pub fn focus_pair() -> Option<(gtk4::Box, gtk4::Button, TextViewFixture)> {
    if !register_probe_im() {
        return None;
    }
    let fixture = TextViewFixture::new(gtk4::WrapMode::WordChar);
    fixture.view.set_im_module(Some(PROBE_IM_ID));
    fixture
        .buffer
        .set_text("a short line\nanother short line\n");
    let button = gtk4::Button::with_label("focus elsewhere");
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.append(&button);
    content.append(&fixture.scrolled);
    Some((content, button, fixture))
}

/// The view's key controller (the one GtkTextView creates at init).
#[must_use]
pub fn key_controller(view: &gtk4::TextView) -> Option<gtk4::EventControllerKey> {
    let controllers = view.observe_controllers();
    (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .find_map(|controller| controller.downcast::<gtk4::EventControllerKey>().ok())
}

/// Whether the view's key controller carries an input-method context.
#[must_use]
pub fn key_controller_has_im(view: &gtk4::TextView) -> Option<bool> {
    key_controller(view).map(|controller| controller.im_context().is_some())
}

fn triggers(requests: &[ImRequest]) -> String {
    super::join_entries(requests.iter().map(|request| request.trigger))
}

fn count(requests: &[ImRequest], trigger: &str) -> usize {
    requests
        .iter()
        .filter(|request| request.trigger == trigger)
        .count()
}

/// Focus the button, then the view, then press Right: what the input method
/// saw on the focus-in and on the key.
fn focus_and_key(
    input: &RemoteInput,
    button: &gtk4::Button,
    fixture: &TextViewFixture,
) -> Result<(Vec<ImRequest>, Vec<ImRequest>), String> {
    button.grab_focus();
    settle(LAYOUT_SETTLE);
    let _ = take_im_requests();
    fixture.view.grab_focus();
    settle(LAYOUT_SETTLE);
    let focus = take_im_requests();
    input.key(keysym::RIGHT, true)?;
    input.key(keysym::RIGHT, false)?;
    settle(LAYOUT_SETTLE);
    let key = take_im_requests();
    Ok((focus, key))
}

/// Probe A60.
#[must_use]
pub fn probe_a60() -> Observation {
    Recorder::run(AxiomId::new(60), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let (content, button, fixture) = recorder.require(
            focus_pair(),
            "control: the probe input method is registered on gtk-im-module",
        )?;
        let shown = Presented::checked_window(
            recorder,
            fullscreen_window(&content),
            "control: the text view was presented",
        )?;
        let window = shown.window().clone();
        recorder.control(
            wait_until(Duration::from_secs(5), || window.is_fullscreen()),
            "control: the window is fullscreen",
        )?;
        settle(LAYOUT_SETTLE);
        let input = match RemoteInput::start() {
            Ok(input) => input,
            Err(error) => {
                recorder.measure("remote_desktop_error", error);
                return recorder
                    .control(false, "control: the compositor accepts RemoteDesktop input");
            }
        };
        let target = recorder.require(
            centre_on_screen(&button),
            "control: the button's centre maps to the window",
        )?;
        let activated = input
            .move_to(target.0, target.1)
            .and_then(|()| input.click(BUTTON_PRIMARY));
        if let Err(error) = activated {
            recorder.measure("input_error", error);
            return recorder.control(false, "control: every injected event was accepted");
        }
        let active = wait_until(Duration::from_secs(2), || window.is_active());
        recorder.measure("window_is_active", active);
        recorder.control(active, "control: a real press activated the window")?;

        let editable_im = key_controller_has_im(&fixture.view);
        recorder.measure("editable_key_controller_has_im", format!("{editable_im:?}"));
        let editable = focus_and_key(&input, &button, &fixture);
        let (editable_focus, editable_key) = match editable {
            Ok(seen) => seen,
            Err(error) => {
                recorder.measure("input_error", error);
                return recorder.control(false, "control: every injected event was accepted");
            }
        };
        recorder.measure("editable_focus_in", triggers(&editable_focus));
        recorder.measure("editable_key", triggers(&editable_key));
        recorder.control(
            editable_im == Some(true),
            "control: an editable view's key controller carries the input method",
        )?;
        recorder.control(
            count(&editable_focus, "focus-in") > 0,
            "control: an editable view focuses the input method in on focus-in",
        )?;
        recorder.control(
            count(&editable_key, "filter-keypress") > 0,
            "control: an editable view filters a key press through the input method",
        )?;

        fixture.view.set_editable(false);
        settle(LAYOUT_SETTLE);
        let protected_im = key_controller_has_im(&fixture.view);
        recorder.measure(
            "non_editable_key_controller_has_im",
            format!("{protected_im:?}"),
        );
        let protected = focus_and_key(&input, &button, &fixture);
        let (protected_focus, protected_key) = match protected {
            Ok(seen) => seen,
            Err(error) => {
                recorder.measure("input_error", error);
                return recorder.control(false, "control: every injected event was accepted");
            }
        };
        recorder.measure("non_editable_focus_in", triggers(&protected_focus));
        recorder.measure("non_editable_key", triggers(&protected_key));
        recorder.control(
            fixture.view.has_focus(),
            "control: the non-editable view took keyboard focus",
        )?;
        recorder.axiom(
            protected_im == Some(false),
            "a non-editable view's key controller carries no input-method context",
        )?;
        recorder.axiom(
            count(&protected_focus, "focus-in") == 0,
            "a non-editable view does not focus the input method in on focus-in",
        )?;
        recorder.axiom(
            count(&protected_key, "filter-keypress") == 0,
            "a key press on a non-editable view never reaches the input method",
        )
    })
}
