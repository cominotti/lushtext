// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A72: a dense document restyled through a lowest-priority tag
//! (family, relative scale) or through view CSS. Toggle the tag's zoom, the
//! view's CSS zoom, and the hidden paragraphs, and watch the view and its map.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a72::{
    DisplayCss, NEW_ZOOM, ZOOM, document, document_tag, style_css,
};
use gtk_lush_axioms::fixtures::post_open::DenseView;
use gtk4::prelude::*;
use sourceview5::prelude::*;

fn build(ui: &support::SampleUi) {
    let dense = DenseView::new(gtk4::WrapMode::Word);
    dense.view.set_show_line_numbers(true);
    dense.buffer.set_text(&document());
    let map = sourceview5::Map::new();
    map.set_view(&dense.view);
    let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    container.append(&dense.scrolled);
    container.append(&map);
    ui.set_fixture(&container);
    let tag = document_tag(&dense.buffer, ZOOM, None);
    let css = Rc::new(DisplayCss::install(&style_css(ZOOM, None)));
    let zoomed = Cell::new(false);
    {
        let tag = tag.clone();
        ui.add_control("Toggle the tag's zoom", move || {
            zoomed.set(!zoomed.get());
            tag.set_scale(if zoomed.get() { NEW_ZOOM } else { ZOOM });
        });
    }
    {
        let dense = dense.clone();
        ui.add_control("Move the view under the zoom CSS", move || {
            let _ = &css;
            dense.view.add_css_class("gtk-lush-axioms-a72-property");
        });
    }
    {
        let dense = dense.clone();
        ui.add_control("Toggle wrapping", move || {
            let wrap = if dense.view.wrap_mode() == gtk4::WrapMode::None {
                gtk4::WrapMode::Word
            } else {
                gtk4::WrapMode::None
            };
            dense.view.set_wrap_mode(wrap);
        });
    }
    ui.set_readout(move || {
        let (width, height) = dense.uppers();
        format!(
            "tag scale {:.2}   content {width:.0} x {height:.0} px   map {:.0} px",
            tag.scale(),
            map.vadjustment()
                .map_or(0.0, |adjustment| adjustment.upper())
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(72), build)
}
