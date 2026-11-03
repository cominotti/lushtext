// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A82: load the same document into a tab page that was never
//! selected and into one that was selected once and hidden again, and read
//! how long a G_PRIORITY_DEFAULT_IDLE source waited after each load.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a82::{LINES, LowerDelivery, TabViewFixture, load_and_trace};

fn describe(name: &str, delivery: Option<LowerDelivery>) -> String {
    delivery.map_or_else(
        || format!("{name}: never"),
        |d| format!("{name}: iteration {}, {} ms", d.iteration, d.micros / 1000),
    )
}

fn build(ui: &support::SampleUi) {
    let note = gtk4::Label::new(Some(&format!(
        "Opens a probe window with an AdwTabView and loads {LINES} lines into a page never \
         selected, then into a page selected once and hidden again."
    )));
    note.set_wrap(true);
    ui.set_fixture(&note);
    let readout = Rc::new(RefCell::new(String::from("press the button")));
    {
        let readout = Rc::clone(&readout);
        ui.add_control("Load both hidden pages", move || {
            let text = TabViewFixture::standalone().map_or_else(
                || "the fixture window never realized".to_owned(),
                |fixture| {
                    let never = load_and_trace(&fixture.never_shown.1.buffer);
                    fixture.tab_view.set_selected_page(&fixture.shown_once.0);
                    while gtk4::glib::MainContext::default().iteration(false) {}
                    fixture.tab_view.set_selected_page(&fixture.front);
                    let shown = load_and_trace(&fixture.shown_once.1.buffer);
                    format!(
                        "{}\n{}",
                        describe("never selected", never),
                        describe("selected once, hidden", shown)
                    )
                },
            );
            *readout.borrow_mut() = text;
        });
    }
    ui.set_readout(move || readout.borrow().clone());
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(82), build)
}
