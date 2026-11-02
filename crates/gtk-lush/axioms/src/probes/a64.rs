// SPDX-License-Identifier: MIT OR Apache-2.0

//! A64: which guards keep a non-editable `GtkSourceView`'s key bindings from
//! its buffer and marks.
//!
//! - **Per-instance action enablement is not a guard.**
//!   `gtk_widget_action_set_enabled(view, "selection.select-all", FALSE)` holds
//!   only until GtkTextView recomputes its `clipboard.*` and `selection.*`
//!   enablement (`gtk_text_view_update_clipboard_actions`, run by `do_popup`
//!   and the touch selection bubble): after one `menu.popup`, activating
//!   `selection.select-all` selects again. And **Ctrl+A never consults it**:
//!   Ctrl+A is bound to the `select-all` signal, not the action.
//! - **A handler that stops a keybinding signal is a guard.** The keybinding
//!   signals are `G_SIGNAL_RUN_LAST`, so a handler that stops the emission
//!   keeps the class handler from running: `select-all` (Ctrl+A),
//!   `toggle-cursor-visible` (F7), and GtkSourceView's `change-number`
//!   (Ctrl+Shift+X; its Ctrl+Shift+A binding is shadowed by GtkTextView's
//!   own Ctrl+Shift+A, which unselects), `change-case`, `join-lines` and
//!   `move-to-matching-bracket` (Ctrl+%) leave the buffer and the marks
//!   unchanged. Without it, `change-number`, `change-case` and `join-lines`
//!   **change the buffer of a non-editable view** (their handlers never check
//!   `editable`); `change-case` and `join-lines` have no default key binding
//!   in GtkSourceView 5.20 and are reached by emission.
//! - **A capture-phase key controller that claims Shift+F10 is a guard** for
//!   the `menu.popup` binding action: no context menu opens.
//!
//! Real keys go through the private compositor (RemoteDesktop,
//! `remote_input`), so the binding lookup is the toolkit's own.
//!
//! Protected View (C2) pre-empts these bindings on entry (design D3, step 5).

use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::a59::popover_visible;
use crate::probes::remote_input::{
    BUTTON_PRIMARY, RemoteInput, centre_on_screen, fullscreen_window, keysym,
};
use crate::session::{Presented, settle, wait_until};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// The probe document: a number, a bracket pair and three lines.
pub const DOCUMENT: &str = "value 42 and (bracket) here\nsecond line\nthird line\n";
/// Offset inside `42`.
pub const NUMBER_OFFSET: i32 = 7;
/// Offset of `(`.
pub const BRACKET_OFFSET: i32 = 13;

/// The keybinding signals the probe pre-empts.
pub const PREEMPTED_SIGNALS: [&str; 6] = [
    "select-all",
    "toggle-cursor-visible",
    "change-number",
    "change-case",
    "join-lines",
    "move-to-matching-bracket",
];

/// A source view over [`DOCUMENT`] in a scrolled window.
#[must_use]
pub fn source_view() -> (gtk4::ScrolledWindow, sourceview5::View, gtk4::TextBuffer) {
    let view = sourceview5::View::new();
    let buffer = view.buffer();
    buffer.set_text(DOCUMENT);
    view.set_monospace(true);
    let scrolled = gtk4::ScrolledWindow::builder()
        .child(&view)
        .hexpand(true)
        .vexpand(true)
        .build();
    (scrolled, view, buffer)
}

/// Stop every signal in [`PREEMPTED_SIGNALS`] on `view` before its class
/// handler. Returns the handler ids.
#[must_use]
pub fn preempt_signals(view: &sourceview5::View) -> Vec<glib::SignalHandlerId> {
    PREEMPTED_SIGNALS
        .into_iter()
        .map(|name| {
            view.connect_local(name, false, move |values| {
                if let Some(Ok(object)) = values.first().map(glib::Value::get::<glib::Object>) {
                    object.stop_signal_emission_by_name(name);
                }
                None
            })
        })
        .collect()
}

/// A capture-phase key controller that claims Shift+F10 and Menu, the
/// `menu.popup` bindings.
#[must_use]
pub fn menu_key_claim() -> gtk4::EventControllerKey {
    let controller = gtk4::EventControllerKey::new();
    controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    controller.connect_key_pressed(|_, keyval, _, state| {
        let shift_f10 =
            keyval == gtk4::gdk::Key::F10 && state.contains(gtk4::gdk::ModifierType::SHIFT_MASK);
        if shift_f10 || keyval == gtk4::gdk::Key::Menu {
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    controller
}

/// The text, insert offset, selection-bound offset and `cursor-visible`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewState {
    /// The buffer text.
    pub text: String,
    /// The insert mark's offset.
    pub insert: i32,
    /// The selection bound's offset.
    pub bound: i32,
    /// The view's `cursor-visible`.
    pub cursor_visible: bool,
}

/// `view`'s [`ViewState`].
#[must_use]
pub fn view_state(view: &sourceview5::View) -> ViewState {
    let buffer = view.buffer();
    let (start, end) = buffer.bounds();
    ViewState {
        text: buffer.text(&start, &end, true).to_string(),
        insert: buffer.iter_at_mark(&buffer.get_insert()).offset(),
        bound: buffer.iter_at_mark(&buffer.selection_bound()).offset(),
        cursor_visible: view.is_cursor_visible(),
    }
}

/// Reset `view` to [`DOCUMENT`], non-editable, cursor visible, with the
/// cursor at `offset` (and the selection bound at `bound`).
pub fn reset(view: &sourceview5::View, offset: i32, bound: i32) {
    let buffer = view.buffer();
    buffer.set_text(DOCUMENT);
    buffer.select_range(
        &buffer.iter_at_offset(offset),
        &buffer.iter_at_offset(bound),
    );
    view.set_cursor_visible(true);
    view.set_editable(false);
    settle(LAYOUT_SETTLE);
}

/// One emission of each pre-emptable signal, from its own starting state.
/// Returns `(signal, before, after)` per signal.
fn emit_each(view: &sourceview5::View) -> Vec<(&'static str, ViewState, ViewState)> {
    let mut seen = Vec::new();
    let mut run = |name: &'static str, offset: i32, bound: i32, emit: &dyn Fn()| {
        reset(view, offset, bound);
        let before = view_state(view);
        emit();
        settle(LAYOUT_SETTLE);
        seen.push((name, before, view_state(view)));
    };
    run("select-all", 0, 0, &|| {
        view.emit_by_name::<()>("select-all", &[&true]);
    });
    run("toggle-cursor-visible", 0, 0, &|| {
        view.emit_by_name::<()>("toggle-cursor-visible", &[]);
    });
    run("change-number", NUMBER_OFFSET, NUMBER_OFFSET, &|| {
        view.emit_by_name::<()>("change-number", &[&1i32]);
    });
    run("change-case", 0, 5, &|| {
        view.emit_by_name::<()>("change-case", &[&sourceview5::ChangeCaseType::Upper]);
    });
    run("join-lines", 0, 40, &|| {
        view.emit_by_name::<()>("join-lines", &[]);
    });
    run(
        "move-to-matching-bracket",
        BRACKET_OFFSET,
        BRACKET_OFFSET,
        &|| {
            view.emit_by_name::<()>("move-to-matching-bracket", &[&false]);
        },
    );
    seen
}

fn describe(seen: &[(&'static str, ViewState, ViewState)]) -> String {
    super::join_entries(seen.iter().map(|(name, before, after)| {
        format!(
            "{name}: changed {} (insert {}->{}, bound {}->{}, cursor-visible {}->{})",
            before.text != after.text,
            before.insert,
            after.insert,
            before.bound,
            after.bound,
            before.cursor_visible,
            after.cursor_visible
        )
    }))
}

/// The real-key half: Ctrl+A, F7, Ctrl+Shift+X and Shift+F10 on the focused
/// view. Returns `(key, before, after, menu shown)` per key.
fn press_each(
    input: &RemoteInput,
    view: &sourceview5::View,
) -> Result<Vec<(&'static str, ViewState, ViewState, bool)>, String> {
    let keys: [(&'static str, &[u32], u32, i32); 4] = [
        ("Ctrl+A", &[keysym::CONTROL_L], keysym::A, 0),
        ("F7", &[], keysym::F7, 0),
        (
            "Ctrl+Shift+X",
            &[keysym::CONTROL_L, keysym::SHIFT_L],
            keysym::UPPER_X,
            NUMBER_OFFSET,
        ),
        ("Shift+F10", &[keysym::SHIFT_L], keysym::F10, 0),
    ];
    let mut seen = Vec::new();
    for (name, modifiers, key, offset) in keys {
        reset(view, offset, offset);
        let before = view_state(view);
        input.chord(modifiers, key)?;
        let menu = wait_until(LAYOUT_SETTLE, || popover_visible(view));
        settle(LAYOUT_SETTLE);
        seen.push((name, before, view_state(view), menu));
        close_menu(view);
    }
    Ok(seen)
}

fn close_menu(view: &sourceview5::View) {
    let mut child = view.first_child();
    while let Some(widget) = child {
        if let Some(popover) = widget.downcast_ref::<gtk4::Popover>() {
            popover.popdown();
        }
        child = widget.next_sibling();
    }
    settle(LAYOUT_SETTLE);
}

fn describe_keys(seen: &[(&'static str, ViewState, ViewState, bool)]) -> String {
    super::join_entries(seen.iter().map(|(name, before, after, menu)| {
        format!(
            "{name}: changed {} (insert {}->{}, bound {}->{}, cursor-visible {}->{}), menu {menu}",
            before.text != after.text,
            before.insert,
            after.insert,
            before.bound,
            after.bound,
            before.cursor_visible,
            after.cursor_visible
        )
    }))
}

fn unchanged<T>(seen: &[(&'static str, ViewState, ViewState, T)]) -> bool {
    seen.iter().all(|(_, before, after, _)| before == after)
}

/// Probe A64.
#[must_use]
pub fn probe_a64() -> Observation {
    Recorder::run(AxiomId::new(64), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let (scrolled, view, buffer) = source_view();
        let shown = Presented::checked_window(
            recorder,
            fullscreen_window(&scrolled),
            "control: the source view was presented",
        )?;
        let window = shown.window().clone();
        recorder.control(
            wait_until(Duration::from_secs(5), || window.is_fullscreen()),
            "control: the window is fullscreen",
        )?;
        view.set_editable(false);
        settle(LAYOUT_SETTLE);

        // Per-instance action enablement.
        reset(&view, 0, 0);
        view.action_set_enabled("selection.select-all", false);
        let _ = view.activate_action("selection.select-all", None);
        settle(LAYOUT_SETTLE);
        let disabled = view_state(&view);
        let _ = view.activate_action("menu.popup", None);
        settle(LAYOUT_SETTLE);
        let popup_shown = popover_visible(&view);
        close_menu(&view);
        let _ = view.activate_action("selection.select-all", None);
        settle(LAYOUT_SETTLE);
        let recomputed = view_state(&view);
        recorder.measure("select_all_action_disabled", format!("{disabled:?}"));
        recorder.measure("menu_popup_shown", popup_shown);
        recorder.measure("select_all_action_after_popup", format!("{recomputed:?}"));
        recorder.control(
            disabled.insert == disabled.bound && popup_shown,
            "control: a disabled selection.select-all does nothing, and menu.popup opens the menu",
        )?;
        recorder.axiom(
            recomputed.insert != recomputed.bound,
            "after menu.popup, GtkTextView has re-enabled selection.select-all: per-instance \
             enablement is not a guard",
        )?;

        // Keybinding signals, by emission.
        let stock = emit_each(&view);
        recorder.measure("stock_emissions", describe(&stock));
        let handlers = preempt_signals(&view);
        let stopped = emit_each(&view);
        recorder.measure("preempted_emissions", describe(&stopped));
        recorder.control(
            stock.iter().all(|(_, before, after)| before != after),
            "control: on a non-editable view every one of these signals changes the buffer, a \
             mark or cursor-visible",
        )?;
        recorder.measure(
            "stock_buffer_changes_on_non_editable",
            super::join_entries(
                stock
                    .iter()
                    .filter(|(_, before, after)| before.text != after.text)
                    .map(|(name, _, _)| *name),
            ),
        );

        // Real keys.
        let input = match RemoteInput::start() {
            Ok(input) => input,
            Err(error) => {
                recorder.measure("remote_desktop_error", error);
                return recorder
                    .control(false, "control: the compositor accepts RemoteDesktop input");
            }
        };
        let target = recorder.require(
            centre_on_screen(&view),
            "control: the view's centre maps to the window",
        )?;
        for handler in handlers {
            view.disconnect(handler);
        }
        let focused = input
            .move_to(target.0, target.1)
            .and_then(|()| input.click(BUTTON_PRIMARY));
        if let Err(error) = focused {
            recorder.measure("input_error", error);
            return recorder.control(false, "control: every injected event was accepted");
        }
        settle(LAYOUT_SETTLE);
        recorder.control(
            wait_until(Duration::from_secs(2), || view.has_focus()),
            "control: a real press focused the view",
        )?;
        view.action_set_enabled("selection.select-all", false);
        let keys_seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let key_log = gtk4::EventControllerKey::new();
        key_log.set_propagation_phase(gtk4::PropagationPhase::Capture);
        {
            let keys_seen = keys_seen.clone();
            key_log.connect_key_pressed(move |_, keyval, _, state| {
                keys_seen
                    .borrow_mut()
                    .push(format!("{}+{state:?}", keyval.name().unwrap_or_default()));
                glib::Propagation::Proceed
            });
        }
        view.add_controller(key_log.clone());
        let stock_keys = press_each(&input, &view);
        view.remove_controller(&key_log);
        recorder.measure("keys_seen", super::join_entries(keys_seen.borrow().iter()));
        let stock_keys = match stock_keys {
            Ok(seen) => seen,
            Err(error) => {
                recorder.measure("input_error", error);
                return recorder.control(false, "control: every injected event was accepted");
            }
        };
        recorder.measure("stock_keys", describe_keys(&stock_keys));
        let handlers = preempt_signals(&view);
        let claim = menu_key_claim();
        view.add_controller(claim.clone());
        let preempted_keys = press_each(&input, &view);
        view.remove_controller(&claim);
        for handler in handlers {
            view.disconnect(handler);
        }
        let preempted_keys = match preempted_keys {
            Ok(seen) => seen,
            Err(error) => {
                recorder.measure("input_error", error);
                return recorder.control(false, "control: every injected event was accepted");
            }
        };
        recorder.measure("preempted_keys", describe_keys(&preempted_keys));
        drop(buffer);

        recorder.control(
            stock_keys.iter().all(|(name, before, after, menu)| {
                before != after || (*name == "Shift+F10" && *menu)
            }),
            "control: without pre-emption each real key reaches its binding (Ctrl+A selects \
             although selection.select-all is disabled; Ctrl+Shift+X changes the number of a \
             non-editable buffer)",
        )?;
        recorder.axiom(
            stopped.iter().all(|(_, before, after)| before == after),
            "a handler that stops the emission keeps every one of these signals from its \
             default handler",
        )?;
        recorder.axiom(
            unchanged(&preempted_keys) && preempted_keys.iter().all(|(_, _, _, menu)| !menu),
            "with the signals stopped and Shift+F10 claimed in the capture phase, no real key \
             changes the buffer, a mark or cursor-visible, or opens the menu",
        )
    })
}
