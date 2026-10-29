// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: evidence surface — the window-composition workflow's single
//! observable state.
//!
//! [`CompositionEvidence`] is what widget tests read and what the window's
//! construction evidence (and through it the automation snapshot's
//! `window_construction`) projects. Always compiled: the readiness blocker
//! `window-composition` and the snapshot need it in every build.
//!
//! * **Reading must not mutate.** One scoped borrow of the runtime, dropped
//!   before any widget is read; no source is queued, no stage marked, no frame
//!   requested.
//! * **No field may be read from inside a mutable borrow.** The runtime borrow
//!   is shared and released before the struct literal is built.
//! * **A disposed widget is a stage.** The three staged surfaces' own
//!   `visible` properties are read through `try_get()`, so a teardown
//!   observation reports `false` rather than panicking.
//!
//! **No materialization.** The fields are scalars and three widget
//! properties; nothing walks a lazily created toolkit collection.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use super::super::LushtextWindow;
use super::policy::{CompositionPlan, CompositionStage, StageSet, composition_blocks_readiness};
use crate::model::construction_spans::span_duration_us;

/// Everything a test, the readiness poll, or the snapshot may observe about
/// one window's composition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompositionEvidence {
    /// Whether the window began composing (end of stage C).
    pub begun: bool,
    /// Whether the frame after the last scheduled stage painted.
    pub complete: bool,
    /// Whether the window left its application; no stage runs afterwards.
    pub torn_down: bool,
    /// The stage a test-only hold keeps composition before, if any.
    pub held_before: Option<CompositionStage>,
    /// Stages the window schedules.
    pub scheduled: StageSet,
    /// Stages that ran, scheduled or forced.
    pub done: StageSet,
    /// How many stages the window schedules.
    pub stages_total: u32,
    /// How many scheduled stages have not run.
    pub stages_pending: u32,
    /// How many stages an operation forced ahead of, or without, a schedule.
    pub forced_stages: u32,
    /// From the window request (or the construction) to the first painted
    /// frame, in microseconds.
    pub shell_first_frame_us: Option<u64>,
    /// From the window request (or the construction) until composition
    /// completed, in microseconds.
    pub composition_complete_us: Option<u64>,
    /// The largest main-loop dispatch until composition completed, from the
    /// responsiveness watchdog; `None` without it, never `Some(0)`.
    pub largest_composition_dispatch_us: Option<u64>,
    /// Activation files waiting for their bounded turn.
    pub activation_opens_queued: usize,
    /// Whether a stage dispatch is queued.
    pub dispatch_queued: bool,
    /// The content stack's own `visible` property.
    pub content_visible: bool,
    /// The workspace sidebar's own `visible` property.
    pub workspace_visible: bool,
    /// The document-properties panel's own `visible` property.
    pub properties_visible: bool,
    /// Whether the `window-composition` readiness blocker is active.
    pub blocks_readiness: bool,
}

impl LushtextWindow {
    /// Read this window's composition evidence.
    #[must_use]
    pub fn composition_evidence(&self) -> CompositionEvidence {
        let imp = self.imp();
        let facts = imp.composition.facts();
        let own_visible = |widget: Option<gtk4::Widget>| {
            widget.is_some_and(|widget| widget.property::<bool>("visible"))
        };
        let content_visible = own_visible(imp.content_stack.try_get().map(Cast::upcast));
        let workspace_visible = own_visible(imp.sidebar.try_get().map(Cast::upcast));
        let properties_visible = own_visible(imp.properties_panel.try_get().map(Cast::upcast));
        let since_origin = |at: Option<i64>| at.map(|at| span_duration_us(facts.origin_us, at));
        let plan = facts.plan;
        CompositionEvidence {
            begun: plan.is_some(),
            complete: facts.complete,
            torn_down: facts.torn_down,
            held_before: facts.hold_before,
            scheduled: plan.map(CompositionPlan::scheduled).unwrap_or_default(),
            done: plan.map(CompositionPlan::done).unwrap_or_default(),
            stages_total: plan.map_or(0, CompositionPlan::stages_total),
            stages_pending: plan.map_or(0, CompositionPlan::stages_pending),
            forced_stages: plan.map_or(0, CompositionPlan::forced_stages),
            shell_first_frame_us: since_origin(facts.first_frame_us),
            composition_complete_us: since_origin(facts.complete_us),
            largest_composition_dispatch_us: facts.largest_dispatch_us,
            activation_opens_queued: facts.activation_queued,
            dispatch_queued: facts.dispatch_queued,
            content_visible,
            workspace_visible,
            properties_visible,
            blocks_readiness: composition_blocks_readiness(
                plan.is_some(),
                facts.complete,
                facts.torn_down,
            ),
        }
    }
}
