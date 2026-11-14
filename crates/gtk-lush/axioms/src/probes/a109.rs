// SPDX-License-Identifier: MIT OR Apache-2.0

//! A109: constructing a `GtkInscription` costs several times constructing a
//! `GtkLabel`: `gtk_inscription_init` creates its Pango layout at once
//! (`gtk_widget_create_pango_layout`), which computes a style for a CSS node
//! that has no parent yet, a style the cache cannot share. A list factory
//! that builds its rows from inscriptions pays it for every realized row
//! (about 205, A1) inside the dispatch that creates them: 15 ms more than
//! labels in the stage-1 measurement of `lighten-notes-browser-rows`.
//!
//! That change rejected `GtkInscription` for its row text and built
//! `LushtextRowText`, whose layout is created on its first snapshot; the
//! upstream report is drafted in LushText's `docs/upstream-reports.md`.

use std::time::Instant;

use gtk4::prelude::*;

use super::row_lists::median;
use crate::observation::Recorder;
use crate::session::flush_events;
use crate::{AxiomId, Observation};

/// Widgets constructed per round: the realized band (A1).
pub const WIDGETS: u32 = 205;
/// Rounds; the median is kept.
pub const ROUNDS: u32 = 7;
/// The factor that reads as "several times".
pub const SEVERAL: u64 = 2;

/// Microseconds to construct `WIDGETS` widgets with `build`, median of
/// `ROUNDS`; the widgets are dropped outside the timing.
#[must_use]
pub fn construct_us(build: impl Fn(u32) -> gtk4::Widget) -> u64 {
    let rounds = (0..ROUNDS)
        .map(|_| {
            let mut kept = Vec::with_capacity(WIDGETS as usize);
            let started = Instant::now();
            for index in 0..WIDGETS {
                kept.push(build(index));
            }
            let micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
            drop(kept);
            flush_events();
            micros
        })
        .collect();
    median(rounds)
}

/// A label with row text.
#[must_use]
pub fn label(index: u32) -> gtk4::Widget {
    gtk4::Label::new(Some(&format!("c{index:03}.rs:{index}"))).upcast()
}

/// An inscription with the same text.
#[must_use]
pub fn inscription(index: u32) -> gtk4::Widget {
    gtk4::Inscription::new(Some(&format!("c{index:03}.rs:{index}"))).upcast()
}

/// Probe A109. See the module documentation.
#[must_use]
pub fn probe_a109() -> Observation {
    Recorder::run(AxiomId::new(109), |recorder| {
        // Warm both types (class init, the font map) before timing.
        let _ = construct_us(label);
        let _ = construct_us(inscription);
        let labels = construct_us(label);
        let inscriptions = construct_us(inscription);
        recorder.measure("widgets", WIDGETS);
        recorder.measure("labels_us", labels);
        recorder.measure("inscriptions_us", inscriptions);
        recorder.control(
            labels > 0,
            "control: constructing labels took measurable time",
        )?;
        recorder.axiom(
            inscriptions >= labels * SEVERAL,
            "A109: constructing a GtkInscription costs several times constructing a GtkLabel",
        )
    })
}
