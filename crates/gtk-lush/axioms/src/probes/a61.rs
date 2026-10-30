// SPDX-License-Identifier: MIT OR Apache-2.0

//! A61 (extends A34): a `move-cursor` handler that scrolls the view itself
//! and stops the emission keeps the insert mark and the selection bound
//! where they are **also under `gtk-keynav-use-caret`** (caret browsing),
//! where A34 shows the stock handler moves the insert mark even with
//! `cursor-visible` false. `move-cursor` is `G_SIGNAL_RUN_LAST`, so a handler
//! connected before the class handler runs can stop it.
//!
//! The probe emits `move-cursor` for each step kind the navigation bindings
//! use (with and without `extend_selection`), then presses the real Page Down
//! and Right keys (RemoteDesktop, `remote_input`) so the binding path is the
//! toolkit's own.
//!
//! Protected View (C2) connects this intercept on entry (design D3, step 2).

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::a34::{caret_and_scroll, cursorless_view};
use crate::probes::remote_input::{
    BUTTON_PRIMARY, RemoteInput, centre_on_screen, fullscreen_window, keysym,
};
use crate::probes::text::TextViewFixture;
use crate::session::{Presented, settle, wait_until};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// The step kinds the probe emits, as navigation bindings do.
pub const STEPS: [gtk4::MovementStep; 5] = [
    gtk4::MovementStep::Pages,
    gtk4::MovementStep::DisplayLines,
    gtk4::MovementStep::VisualPositions,
    gtk4::MovementStep::Words,
    gtk4::MovementStep::BufferEnds,
];

/// Connect the intercept: scroll by one page per step count and stop the
/// emission. Returns the handler id and a count of intercepted emissions.
#[must_use]
pub fn connect_intercept(
    fixture: &TextViewFixture,
) -> (gtk4::glib::SignalHandlerId, Rc<Cell<u32>>) {
    let intercepted = Rc::new(Cell::new(0u32));
    let adjustment = fixture.scrolled.vadjustment();
    let counted = intercepted.clone();
    let handler = fixture
        .view
        .connect_move_cursor(move |view, _step, count, _extend| {
            let page = adjustment.page_increment().max(1.0);
            adjustment.set_value(adjustment.value() + page * f64::from(count.signum()));
            counted.set(counted.get() + 1);
            view.stop_signal_emission_by_name("move-cursor");
        });
    (handler, intercepted)
}

/// The insert mark's and the selection bound's offsets.
#[must_use]
pub fn marks(fixture: &TextViewFixture) -> (i32, i32) {
    let buffer = &fixture.buffer;
    (
        buffer.iter_at_mark(&buffer.get_insert()).offset(),
        buffer.iter_at_mark(&buffer.selection_bound()).offset(),
    )
}

/// Restores `gtk-keynav-use-caret` when dropped.
struct CaretBrowsing {
    settings: gtk4::Settings,
    before: bool,
}

impl Drop for CaretBrowsing {
    fn drop(&mut self) {
        self.settings.set_gtk_keynav_use_caret(self.before);
        settle(LAYOUT_SETTLE);
    }
}

/// Probe A61.
#[must_use]
pub fn probe_a61() -> Observation {
    Recorder::run(AxiomId::new(61), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let settings = recorder.require(gtk4::Settings::default(), "control: GTK has settings")?;
        let fixture = cursorless_view();
        let shown = Presented::checked_window(
            recorder,
            fullscreen_window(&fixture.scrolled),
            "control: the text view was presented",
        )?;
        let window = shown.window().clone();
        recorder.control(
            wait_until(Duration::from_secs(5), || window.is_fullscreen()),
            "control: the window is fullscreen",
        )?;
        let caret_before = settings.is_gtk_keynav_use_caret();
        recorder.measure("keynav_use_caret_before", caret_before);
        let browsing = CaretBrowsing {
            before: caret_before,
            settings: settings.clone(),
        };
        settings.set_gtk_keynav_use_caret(true);
        fixture.view.grab_focus();
        settle(LAYOUT_SETTLE);

        let before = caret_and_scroll(&fixture);
        fixture
            .view
            .emit_move_cursor(gtk4::MovementStep::Pages, 1, false);
        settle(LAYOUT_SETTLE);
        let stock = caret_and_scroll(&fixture);
        recorder.measure(
            "caret_browsing_stock_page_down",
            format!("{before:?} -> {stock:?}"),
        );
        recorder.control(
            stock.0 != before.0,
            "control: under caret browsing the stock move-cursor moves the insert mark (A34)",
        )?;

        fixture.buffer.place_cursor(&fixture.buffer.start_iter());
        fixture.scrolled.vadjustment().set_value(0.0);
        settle(LAYOUT_SETTLE);
        let (handler, intercepted) = connect_intercept(&fixture);
        let start = marks(&fixture);
        let mut emitted = Vec::new();
        for step in STEPS {
            for extend in [false, true] {
                fixture.view.emit_move_cursor(step, 1, extend);
                settle(LAYOUT_SETTLE);
                emitted.push(format!("{step:?}/{extend}:{:?}", marks(&fixture)));
            }
        }
        let after_emitted = marks(&fixture);
        let scrolled_to = fixture.scrolled.vadjustment().value();
        recorder.measure("intercepted_emissions", super::join_entries(&emitted));
        recorder.measure("scroll_after_emissions", scrolled_to);

        let input = match RemoteInput::start() {
            Ok(input) => input,
            Err(error) => {
                recorder.measure("remote_desktop_error", error);
                return recorder
                    .control(false, "control: the compositor accepts RemoteDesktop input");
            }
        };
        // The press that activates the window lands on the view, so it goes
        // through the stock click handler: reset the marks afterwards.
        let target = recorder.require(
            centre_on_screen(&fixture.view),
            "control: the view's centre maps to the window",
        )?;
        let keys = input
            .move_to(target.0, target.1)
            .and_then(|()| input.click(BUTTON_PRIMARY))
            .and_then(|()| {
                settle(LAYOUT_SETTLE);
                fixture.buffer.place_cursor(&fixture.buffer.start_iter());
                settle(LAYOUT_SETTLE);
                let counted = intercepted.get();
                input.chord(&[], keysym::PAGE_DOWN)?;
                input.chord(&[], keysym::RIGHT)?;
                input.chord(&[keysym::SHIFT_L], keysym::RIGHT)?;
                settle(LAYOUT_SETTLE);
                Ok(intercepted.get() - counted)
            });
        let key_emissions = match keys {
            Ok(count) => count,
            Err(error) => {
                recorder.measure("input_error", error);
                return recorder.control(false, "control: every injected event was accepted");
            }
        };
        let after_keys = marks(&fixture);
        fixture.view.disconnect(handler);
        recorder.measure("key_emissions_intercepted", key_emissions);
        recorder.measure("marks_after_keys", format!("{after_keys:?}"));
        recorder.measure(
            "keynav_use_caret_during",
            settings.is_gtk_keynav_use_caret(),
        );
        drop(browsing);
        recorder.measure(
            "keynav_use_caret_restored",
            settings.is_gtk_keynav_use_caret() == caret_before,
        );

        recorder.control(
            intercepted.get() >= u32::try_from(emitted.len()).unwrap_or(u32::MAX),
            "control: the intercept saw every emission",
        )?;
        recorder.control(
            key_emissions >= 3,
            "control: the real navigation keys reached move-cursor",
        )?;
        recorder.axiom(
            after_emitted == start && scrolled_to > 0.0,
            "with the intercept stopping move-cursor under caret browsing, no step moves the \
             insert mark or the selection bound, and the view scrolls",
        )?;
        recorder.axiom(
            after_keys == (0, 0),
            "with the intercept, the real Page Down, Right and Shift+Right keys move no mark",
        )
    })
}
