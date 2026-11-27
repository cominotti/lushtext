// SPDX-License-Identifier: GPL-3.0-or-later

//! Window-side recording of the construction spans: each stage of
//! `LushtextWindow::new` and `constructed`, the end of the constructing
//! dispatch, and the first painted frame.
//!
//! **Cross-cutting coordination with no workflow role**: it serves every
//! workflow that builds part of the window, and owns no decision. The data
//! lives in `model::construction_spans`; `ui::construction_spans` owns the
//! clock and the process-wide startup spans. The shell-geometry evidence
//! (`geometry::evidence`) projects the result.

use std::cell::RefCell;
use std::rc::Rc;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use super::LushtextWindow;
use crate::ui::construction_spans;
use crate::ui::settings_writes;

/// Runs the stages of `LushtextWindow::new` one after another, recording each.
pub(in crate::ui::window) struct ConstructionStages<'a> {
    window: &'a LushtextWindow,
    cursor_us: i64,
}

impl<'a> ConstructionStages<'a> {
    /// Start recording stages for `window` from now.
    pub(in crate::ui::window) fn new(window: &'a LushtextWindow) -> Self {
        Self {
            window,
            cursor_us: construction_spans::now_us(),
        }
    }

    /// Run one named stage and record how long it took.
    pub(in crate::ui::window) fn run(&mut self, name: &'static str, stage: impl FnOnce()) {
        stage();
        self.cursor_us = self.window.record_construction_stage(name, self.cursor_us);
    }
}

impl LushtextWindow {
    /// Whether no other LushText window of this window's application exists.
    pub(in crate::ui::window) fn is_last_lushtext_window(&self) -> bool {
        self.application().is_none_or(|app| {
            !app.windows().iter().any(|other| {
                other != self.upcast_ref::<gtk4::Window>() && other.is::<LushtextWindow>()
            })
        })
    }

    /// Record the stage that ran from `start_us` until now, and return now so
    /// the next stage can start there.
    pub(crate) fn record_construction_stage(&self, name: &'static str, start_us: i64) -> i64 {
        let end_us = construction_spans::now_us();
        self.record_stage_span(name, start_us, end_us);
        end_us
    }

    /// Record one stage span and forward it to the evidence stream.
    fn record_stage_span(&self, name: &'static str, start_us: i64, end_us: i64) {
        self.imp()
            .construction
            .borrow_mut()
            .record_stage(name, start_us, end_us);
        construction_spans::emit(name, start_us, end_us);
    }

    /// Begin this window's construction record at `started_us` (the start of
    /// `LushtextWindow::new`, before the template was instantiated), and watch
    /// for the end of the constructing dispatch and the first painted frame.
    pub(in crate::ui::window) fn begin_construction_record(&self, started_us: i64) {
        let template_end_us = {
            let mut construction = self.imp().construction.borrow_mut();
            construction.begin(started_us);
            construction.stages().first().map(|stage| stage.start_us)
        };
        // `constructed` recorded its stages while the object was being built;
        // the time before its first stage is template instantiation.
        if let Some(template_end_us) = template_end_us {
            self.record_stage_span("template", started_us, template_end_us);
        }

        // GLib dispatches a high-priority idle on the first iteration after
        // the dispatch that is building this window, before default-priority
        // work: that is when construction returned control to the main loop.
        let window_weak = self.downgrade();
        glib::idle_add_local_full(glib::Priority::HIGH, move || {
            if let Some(window) = window_weak.upgrade() {
                let end_us = construction_spans::now_us();
                window
                    .imp()
                    .construction
                    .borrow_mut()
                    .finish_construction(end_us);
                construction_spans::emit("window_construction", started_us, end_us);
            }
            glib::ControlFlow::Break
        });

        let window_weak = self.downgrade();
        self.connect_realize(move |_| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            window.watch_first_frame(started_us);
        });
    }

    /// Record the first `after-paint` of this window's frame clock painted
    /// while the window is mapped. A frame that runs between realize and map
    /// is never presented (ledger A51), so it is not the first frame: neither
    /// the evidence nor the settings writer's gate may rest on it.
    fn watch_first_frame(&self, started_us: i64) {
        if self.imp().construction.borrow().first_frame_painted() {
            return;
        }
        let Some(clock) = self.frame_clock() else {
            return;
        };
        let handler = Rc::new(RefCell::new(None::<glib::SignalHandlerId>));
        let handler_slot = Rc::clone(&handler);
        let window_weak = self.downgrade();
        let id = clock.connect_after_paint(move |clock| {
            let window = window_weak.upgrade();
            if window.as_ref().is_some_and(|window| !window.is_mapped()) {
                return;
            }
            if let Some(id) = handler_slot.borrow_mut().take() {
                clock.disconnect(id);
            }
            let Some(window) = window else {
                return;
            };
            let painted_us = construction_spans::now_us();
            window
                .imp()
                .construction
                .borrow_mut()
                .record_first_frame(painted_us, construction_spans::largest_dispatch_so_far_us());
            construction_spans::emit("first_frame", started_us, painted_us);
            // The settings writer persists nothing before the first frame
            // painted on the mapped window.
            settings_writes::first_frame_presented();
        });
        handler.replace(Some(id));
    }
}

/// What the window's construction cost, and what the settings writer did.
///
/// Integers only (the automation snapshot projects it as `window_construction`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WindowConstructionEvidence {
    /// GTK's own startup (`parent_startup`), reported, not budgeted.
    pub startup_toolkit_us: Option<u64>,
    /// LushText's startup work.
    pub startup_lushtext_us: Option<u64>,
    /// From `LushtextWindow::new` until control returned to the main loop.
    pub window_construction_us: Option<u64>,
    /// From `LushtextWindow::new` until the window's first frame painted while
    /// it was mapped (never a frame on the realized, unmapped window, A51).
    pub first_frame_us: Option<u64>,
    /// The largest main-loop dispatch before the first frame, when the
    /// measurement build's watchdog ran; `None` otherwise, never `Some(0)`.
    pub largest_dispatch_before_first_frame_us: Option<u64>,
    /// Composition stages this window schedules (`composition/`): W when its
    /// request ran it, then C, R and P (R and P in one dispatch), and B1, B2
    /// and B3 (in one dispatch after the first frame), always (a rail the
    /// restored layout hides gets its content revealed into the hidden rail).
    pub stages_total: u32,
    /// Scheduled composition stages that have not run.
    pub stages_pending: u32,
    /// Composition stages an operation forced ahead of, or without, a schedule.
    pub forced_stages: u32,
    /// From the window request (or the construction) to the first painted
    /// frame, the shell with its final chrome.
    pub shell_first_frame_us: Option<u64>,
    /// From the window request (or the construction) until composition
    /// completed; `None` until it did.
    pub composition_complete_us: Option<u64>,
    /// From the window request (or the construction) to the first frame
    /// painted after the activation's file opens ran; `None` when the request
    /// named no files.
    pub activation_shown_us: Option<u64>,
    /// The largest main-loop dispatch until composition completed, when the
    /// measurement build's watchdog ran; `None` otherwise, never `Some(0)`.
    pub largest_composition_dispatch_us: Option<u64>,
    /// Backend writes the settings writer made before the first frame.
    pub settings_writes_before_first_frame: u64,
    /// Keys whose newest value the writer has not persisted yet.
    pub settings_pending: u64,
    /// Values the writer has flushed.
    pub settings_flushed: u64,
    /// Backend writes (flushes) the writer has made.
    pub settings_flushes: u64,
    /// Writes the writer dropped as equal to the current or persisted value.
    pub settings_dropped_equal: u64,
}

impl LushtextWindow {
    /// Read the window-construction part of the shell-geometry evidence
    /// (`geometry::evidence`), which the automation snapshot also projects.
    pub(crate) fn window_construction_evidence(&self) -> WindowConstructionEvidence {
        let startup = construction_spans::startup_spans();
        let writer = settings_writes::evidence();
        let composition = self.composition_evidence();
        let construction = self.imp().construction.borrow();
        WindowConstructionEvidence {
            startup_toolkit_us: startup.toolkit_us,
            startup_lushtext_us: startup.lushtext_us,
            window_construction_us: construction.window_construction_us(),
            first_frame_us: construction.first_frame_us(),
            largest_dispatch_before_first_frame_us: construction
                .largest_dispatch_before_first_frame_us(),
            stages_total: composition.stages_total,
            stages_pending: composition.stages_pending,
            forced_stages: composition.forced_stages,
            shell_first_frame_us: composition.shell_first_frame_us,
            composition_complete_us: composition.composition_complete_us,
            activation_shown_us: composition.activation_shown_us,
            largest_composition_dispatch_us: composition.largest_composition_dispatch_us,
            settings_writes_before_first_frame: writer.writes_before_first_frame,
            settings_pending: writer.pending,
            settings_flushed: writer.flushed,
            settings_flushes: writer.flushes,
            settings_dropped_equal: writer.dropped_equal,
        }
    }
}
