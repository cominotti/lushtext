// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A49: a concealed `slide-down` revealer whose child requests a
//! fixed width. While the child is visible the concealed revealer measures
//! it at full width (the window cannot shrink below it); hide the child and
//! the revealer measures 0 without measuring it.

mod support;

use std::process::ExitCode;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a49::CHILD_WIDTH;
use gtk_lush_axioms::fixtures::concealed_revealer;
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let (revealer, child) = concealed_revealer(gtk4::RevealerTransitionType::SlideDown);
    ui.set_fixture(&revealer);
    let target = child.clone();
    ui.add_control("Hide the child", move || target.set_visible(false));
    let target = child.clone();
    ui.add_control("Show the child", move || target.set_visible(true));
    let target = revealer.clone();
    ui.add_control("Toggle reveal-child", move || {
        target.set_reveal_child(!target.reveals_child());
    });
    ui.set_readout(move || {
        let (width, _, _, _) = revealer.measure(gtk4::Orientation::Horizontal, -1);
        format!(
            "child requests {CHILD_WIDTH} px, visible {}   concealed revealer measures {width} px \
             wide   child measured {} times",
            child.is_visible(),
            child.horizontal_measures()
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(49), build)
}
