// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A104: a numbered list outside a scroller. Each button measures
//! the list's width for a height (none, its minimum, or zero) and shows the
//! width and how many "Trying to measure GtkListView … for height of H, but it
//! needs at least N" warnings it logged. On GTK 4.24 the zero-height measure
//! logs; before 4.24 none does.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::HostedList;
use gtk_lush_axioms::fixtures::a104::width_for_height;

fn build(ui: &support::SampleUi) {
    let hosted = HostedList::new();
    ui.set_fixture(&hosted.host);
    let last: Rc<Cell<Option<(i32, i32, usize)>>> = Rc::new(Cell::new(None));
    for (label, height) in [
        ("Width for no height", Some(-1)),
        ("Width for its minimum height", None),
        ("Width for height 0", Some(0)),
    ] {
        let (list, last) = (hosted.list.clone(), last.clone());
        ui.add_control(label, move || {
            let height =
                height.unwrap_or_else(|| list.measure(gtk4::Orientation::Vertical, list.width()).0);
            let (width, warnings) = width_for_height(&list, height);
            last.set(Some((height, width, warnings)));
        });
    }
    let list = hosted.list;
    ui.set_readout(move || {
        let mode = list.request_mode();
        match last.get() {
            Some((height, width, warnings)) => format!(
                "request mode {mode:?}\nheight {height}: width {width}, {warnings} warning(s)"
            ),
            None => format!("request mode {mode:?}"),
        }
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(104), build)
}
