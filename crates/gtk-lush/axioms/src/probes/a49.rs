// SPDX-License-Identifier: MIT OR Apache-2.0

//! A49: a **concealed** vertical-slide `GtkRevealer` (`slide-up` or
//! `slide-down`, `reveal-child` false) still measures a **visible** child
//! **horizontally at full scale** — `gtk_revealer_measure` scales only the
//! slide orientation by the reveal progress — and returns 0 at once for an
//! **invisible** child, without measuring it (`gtkrevealer.c`,
//! `get_child_size_scale`).
//!
//! So a concealed search panel or command palette costs the window's
//! horizontal measure its whole width while its child is visible. LushText
//! keeps a concealed revealer's child invisible and makes it visible as
//! `reveal-child` turns true (`first-frame-window-shell`, design D2).

use gtk4::prelude::*;

use super::composition::CountingWidget;
use crate::observation::Recorder;
use crate::session::Presented;
use crate::{AxiomId, Observation};

/// The width the revealer's child requests.
pub const CHILD_WIDTH: i32 = 300;

/// A concealed revealer of `transition` around a counting child.
#[must_use]
pub fn concealed_revealer(
    transition: gtk4::RevealerTransitionType,
) -> (gtk4::Revealer, CountingWidget) {
    let child = CountingWidget::new(CHILD_WIDTH);
    let revealer = gtk4::Revealer::builder()
        .transition_type(transition)
        .reveal_child(false)
        .child(&child)
        .build();
    (revealer, child)
}

/// Probe A49.
#[must_use]
pub fn probe_a49() -> Observation {
    Recorder::run(AxiomId::new(49), |recorder| {
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let mut cases = Vec::new();
        for (keys, transition) in [
            (
                [
                    "slide_up_visible_child_width",
                    "slide_up_visible_child_height",
                    "slide_up_invisible_child_width",
                ],
                gtk4::RevealerTransitionType::SlideUp,
            ),
            (
                [
                    "slide_down_visible_child_width",
                    "slide_down_visible_child_height",
                    "slide_down_invisible_child_width",
                ],
                gtk4::RevealerTransitionType::SlideDown,
            ),
        ] {
            let (revealer, child) = concealed_revealer(transition);
            content.append(&revealer);
            cases.push((keys, revealer, child));
        }
        let _shown =
            Presented::checked(recorder, &content, "control: the fixture window realizes")?;
        for ([width_key, height_key, invisible_key], revealer, child) in &cases {
            recorder.control(
                !revealer.is_child_revealed(),
                "control: the revealer is concealed",
            )?;
            // GTK caches a widget's measurement; drop the first frame's so
            // the count says whether the concealed revealer measures again.
            child.queue_resize();
            let before = child.horizontal_measures();
            let (visible_minimum, _, _, _) = revealer.measure(gtk4::Orientation::Horizontal, -1);
            let (visible_height, _, _, _) = revealer.measure(gtk4::Orientation::Vertical, -1);
            let measured_visible = child.horizontal_measures() > before;
            child.set_visible(false);
            let before = child.measures();
            let (invisible_minimum, _, _, _) = revealer.measure(gtk4::Orientation::Horizontal, -1);
            let measured_invisible = child.measures() > before;
            recorder.measure(width_key, visible_minimum);
            recorder.measure(height_key, visible_height);
            recorder.measure(invisible_key, invisible_minimum);
            recorder.axiom(
                measured_visible && visible_minimum >= CHILD_WIDTH && visible_height == 0,
                "A49: a concealed vertical-slide revealer measures a visible child horizontally at full scale",
            )?;
            recorder.axiom(
                !measured_invisible && invisible_minimum == 0,
                "A49: it returns 0 for an invisible child without measuring it",
            )?;
        }
        Ok(())
    })
}
