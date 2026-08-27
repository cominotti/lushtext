// SPDX-License-Identifier: GPL-3.0-or-later

//! The bounded buffer-replacement workflow's observable state, in one typed
//! value.
//!
//! [`BufferReplacementEvidence`] is the single source of truth for observers of
//! this workflow. Widget tests read it instead of calling per-field `*_for_test`
//! getters or reaching through `imp()`. A test that needs a fact this surface
//! does not carry **extends the surface**; adding another per-field inspection
//! function is the regression this type exists to prevent.
//!
//! Reading evidence is pure observation: it never schedules a turn, advances a
//! generation, cancels a session, or requires the workflow to be in a particular
//! stage. [`record_terminal`] is the workflow's own named operation, called from
//! coordination when a session finishes — it is not part of the read path.
//!
//! **Reentrancy constraint.** [`LushtextEditorPage::buffer_replacement_evidence`]
//! takes shared `RefCell` borrows of the active session, the parked pending
//! request, and the last terminal diagnostic. It must therefore not be called
//! from code already holding a `borrow_mut()` on any of them — which is why every
//! derived scalar below is computed and every `Ref` dropped **before** the struct
//! literal is built. Every live caller observes from outside a mutation.
//!
//! **Disposed-widget rule.** A disposed page is a stage. GTK4 clears template
//! children in `dispose()`, before Rust's `Drop`, and this workflow's whole
//! subject is the source view's buffer — so the buffer-derived field reads
//! through `TemplateChild::try_get()` and answers honestly when the child is
//! gone. The panicking accessor would turn a teardown observation into a crash.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use sourceview5::prelude::*;

use super::execution::BufferReplacementTerminalDiagnostic;
use super::policy::{
    BufferReplacementMetrics, BufferReplacementTicket, BufferReplacementWorkflow, ReplacementPhase,
};
use crate::ui::editor_page::LushtextEditorPage;

/// One consistent read of the bounded buffer-replacement workflow.
///
/// Field groups follow the workflow's stages: who owns the editor, what the
/// active session is doing, how bounded it has stayed, and how the last session
/// ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BufferReplacementEvidence {
    // --- ownership ---
    /// Whether a replacement session currently owns this editor.
    pub in_progress: bool,
    /// Whether a newer request is parked behind the active session.
    ///
    /// Only the newest intent is ever parked, so this is at most one.
    pub pending_request: bool,
    /// Whether editor projections are suspended for a replacement.
    ///
    /// Read by `load_projection_suspended`, so a document-amplifying callback
    /// stands down for either workflow's suspension.
    pub projection_suspended: bool,

    // --- active session identity ---
    /// The owning workflow of the active session's ticket.
    pub active_workflow: Option<BufferReplacementWorkflow>,
    /// The active session's caller-owned freshness generation.
    pub active_generation: Option<u64>,
    /// Which bounded turn the active session will run next.
    pub phase: Option<ReplacementPhase>,
    /// Whether the active session has already mutated the buffer.
    ///
    /// The load-bearing flag for cancellation: a session that has mutated owes
    /// the user a bounded clear pass before it may report a terminal, because a
    /// half-installed document must never be left visible.
    pub mutation_started: bool,

    // --- boundedness ---
    /// Turns the active session has completed, or the last session's total.
    pub slice_count: u64,
    /// Characters cleared by the active session.
    pub cleared_characters: u64,
    /// High-water installed byte offset reached by the active session.
    pub inserted_bytes: usize,
    /// Peak complete bodies the active session has held at once.
    pub peak_retained_bodies: usize,

    // --- buffer state ---
    /// Live buffer character count, or `None` when the template child is gone.
    ///
    /// `None` is the honest answer for a disposed page, not a zero.
    pub buffer_char_count: Option<i32>,

    // --- last terminal ---
    /// How the most recent session for this editor ended.
    pub last_terminal: Option<BufferReplacementTerminalDiagnostic>,
}

impl LushtextEditorPage {
    /// Read this editor's whole buffer-replacement workflow state at once.
    ///
    /// See the module documentation for the reentrancy constraint and the
    /// disposed-widget rule.
    #[must_use]
    pub fn buffer_replacement_evidence(&self) -> BufferReplacementEvidence {
        let imp = self.imp();

        // Every borrow is taken, read, and dropped before the struct literal, so
        // no `Ref` outlives the value it produced.
        let active = imp.replacement.active.borrow().clone();
        let (active_workflow, active_generation, phase, mutation_started, metrics) =
            match active.as_ref() {
                Some(session) => {
                    let session = session.borrow();
                    let BufferReplacementTicket {
                        workflow,
                        generation,
                    } = session.ticket;
                    (
                        Some(workflow),
                        Some(generation),
                        Some(session.phase),
                        session.mutation_started,
                        session.metrics,
                    )
                }
                None => (None, None, None, false, BufferReplacementMetrics::default()),
            };
        let in_progress = active.is_some();
        drop(active);

        let pending_request = imp.replacement.pending.borrow().is_some();
        let last_terminal = *imp.replacement.last_terminal.borrow();
        let projection_suspended = imp.replacement.projection_suspended.get();
        let published_slice_count = imp.replacement.slice_count.get();
        // A disposed page has no source view, so this is the honest `None`
        // rather than the panicking `source_view()` accessor.
        let buffer_char_count = imp
            .source_view
            .try_get()
            .map(|view| view.buffer().char_count());

        BufferReplacementEvidence {
            in_progress,
            pending_request,
            projection_suspended,
            active_workflow,
            active_generation,
            phase,
            mutation_started,
            slice_count: if in_progress {
                metrics.slice_count
            } else {
                published_slice_count
            },
            cleared_characters: metrics.cleared_characters,
            inserted_bytes: metrics.inserted_bytes,
            peak_retained_bodies: metrics.peak_retained_bodies,
            buffer_char_count,
            last_terminal,
        }
    }
}

/// Record how one session ended, from the terminal that ended it.
pub(super) fn record_terminal(
    editor: &LushtextEditorPage,
    diagnostic: BufferReplacementTerminalDiagnostic,
) {
    editor
        .imp()
        .replacement
        .last_terminal
        .replace(Some(diagnostic));
}
