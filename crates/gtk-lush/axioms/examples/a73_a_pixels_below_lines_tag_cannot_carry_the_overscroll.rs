// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A73: a pixels-below-lines tag as end-of-document overscroll.
//! Tag the last paragraph (empty when the text ends in a newline) or the one
//! before it, set the view's bottom margin, and watch the current-line
//! highlight, the gutter and the map.

mod support;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::a73::{ENDS_IN_NEWLINE, OVERSCROLL_PX};
use gtk_lush_axioms::fixtures::post_open::DenseView;
use gtk4::prelude::*;
use sourceview5::prelude::*;

fn build(ui: &support::SampleUi) {
    let dense = DenseView::new(gtk4::WrapMode::Word);
    dense.view.set_show_line_numbers(true);
    dense.view.set_highlight_current_line(true);
    dense.buffer.set_text(ENDS_IN_NEWLINE);
    let map = sourceview5::Map::new();
    map.set_view(&dense.view);
    let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    container.append(&dense.scrolled);
    container.append(&map);
    ui.set_fixture(&container);
    let tag = gtk4::TextTag::builder()
        .pixels_below_lines(OVERSCROLL_PX)
        .build();
    dense.buffer.tag_table().add(&tag);
    for line in [2, 1] {
        let (buffer, tag) = (dense.buffer.clone(), tag.clone());
        ui.add_control(&format!("Tag line {line}"), move || {
            let start = buffer
                .iter_at_line(line)
                .unwrap_or_else(|| buffer.end_iter());
            let end = buffer
                .iter_at_line(line + 1)
                .unwrap_or_else(|| buffer.end_iter());
            buffer.apply_tag(&tag, &start, &end);
        });
    }
    {
        let dense = dense.clone();
        ui.add_control("Toggle the bottom margin", move || {
            let margin = if dense.view.bottom_margin() == 0 {
                OVERSCROLL_PX
            } else {
                0
            };
            dense.view.set_bottom_margin(margin);
        });
    }
    ui.set_readout(move || {
        let last = dense.buffer.end_iter();
        let (y, height) = dense.view.line_yrange(&last);
        format!(
            "last line y {y} h {height}   view upper {:.0}   map upper {:.0}   map bottom margin {}",
            dense.uppers().1,
            map.vadjustment().map_or(0.0, |adjustment| adjustment.upper()),
            map.bottom_margin()
        )
    });
}

fn main() -> std::process::ExitCode {
    support::run(AxiomId::new(73), build)
}
