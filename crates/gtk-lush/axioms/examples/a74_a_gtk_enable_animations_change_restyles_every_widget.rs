// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A74: a change of `gtk-enable-animations` restyles every widget.
//! Toggle the setting, or `gtk-cursor-blink` for comparison, and read the
//! longest frame after each.

mod support;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a74::rows;
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let content = rows(1_600);
    ui.set_fixture(&content);
    let lines = Rc::new(RefCell::new(Vec::<String>::new()));
    for (label, animations) in [
        ("Toggle gtk-enable-animations", true),
        ("Toggle gtk-cursor-blink", false),
    ] {
        let lines = lines.clone();
        let content = content.clone();
        ui.add_control(label, move || {
            let Some(settings) = gtk4::Settings::default() else {
                return;
            };
            if animations {
                settings.set_gtk_enable_animations(!settings.is_gtk_enable_animations());
            } else {
                settings.set_gtk_cursor_blink(!settings.is_gtk_cursor_blink());
            }
            let Some(clock) = content.frame_clock() else {
                return;
            };
            let started = Instant::now();
            let lines = lines.clone();
            let handler = Rc::new(RefCell::new(None));
            let slot = handler.clone();
            let clock_for_disconnect = clock.clone();
            handler.replace(Some(clock.connect_after_paint(move |_| {
                lines.borrow_mut().push(format!(
                    "{label}: next frame painted {} us after the change",
                    started.elapsed().as_micros()
                ));
                if let Some(id) = slot.take() {
                    clock_for_disconnect.disconnect(id);
                }
            })));
            content.queue_draw();
        });
    }
    ui.set_readout(move || lines.borrow().join("\n"));
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(74), build)
}
