// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A6: a `GtkScrollable` works in its CSS content box. Pad the
//! list 7 px top and 5 px bottom and its published page drops from 300 to
//! 288: the allocation minus the inset.

mod support;

use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::{HostedList, INSET_CLASS, InsetStyle};
use gtk4::prelude::*;

fn build(ui: &support::SampleUi) {
    let hosted = HostedList::new();
    ui.set_fixture(&hosted.host);
    let style: Rc<RefCell<Option<InsetStyle>>> = Rc::default();
    let list = hosted.list.clone();
    let installed = style.clone();
    ui.add_control("Pad the list (7 px + 5 px)", move || {
        installed
            .borrow_mut()
            .get_or_insert_with(InsetStyle::install);
        list.add_css_class(INSET_CLASS);
    });
    let list = hosted.list.clone();
    ui.add_control("Remove the padding", move || {
        list.remove_css_class(INSET_CLASS);
    });
    let list = hosted.list.clone();
    let adjustment = hosted.adjustment.clone();
    let host = hosted.host;
    ui.set_readout(move || {
        let _keep_style_alive = &style;
        format!(
            "allocated {} px   content box {} px   published page {}",
            host.child_height(),
            list.height(),
            adjustment.page_size()
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(6), build)
}
