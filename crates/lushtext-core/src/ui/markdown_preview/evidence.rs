// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: evidence surface — the Markdown preview workflow's single observable
//! state. `test-utils`-gated, so production never reads it.
//!
//! # What this replaced
//!
//! **Thirteen inspection seams, eleven of which returned bare tuples** — one of
//! them a seven-element tuple whose reader had to know that position 3 was a
//! configured ceiling and position 4 was a disposal job count. The census
//! recorded this row's evidence surface as
//! `partial: MarkdownImageAdmissionSnapshot`; that type is declared in
//! `services/markdown_render.rs`, so the row had **no** evidence type of its own
//! and every observation was positional.
//!
//! Positional tuples are the specific failure this surface exists to remove: a
//! test asserting `counters.3 == 2` reads as correct whichever field position 3
//! holds, so reordering the tuple silently rewrites what every caller asserts.
//!
//! # Constraints this surface owes
//!
//! * **No field may be read from inside a mutable borrow** of the state it
//!   reads. Every scalar is copied out and each `Ref` is dropped before the
//!   struct literal is built, which is why the accessor reads as a sequence of
//!   `let` bindings rather than one nested expression.
//! * **A disposed widget is a stage.** All but one field come from the `imp`
//!   struct's own `Cell`/`RefCell`/atomic state or from a process-wide static,
//!   none of which GTK clears. The exception is
//!   [`MarkdownPreviewEvidence::placeholder_description`], which reads the
//!   `AdwStatusPage` **template child** — so it goes through `try_get()` and
//!   answers `None` once GTK has cleared it. The pre-consolidation seam it
//!   replaced dereferenced that child directly and would have panicked; the
//!   surface must not, because one accessor now reaches every field from every
//!   observation point, including teardown.
//! * **Reading must not make the toolkit do work.** Nothing here walks a lazily
//!   created collection: the retirement session's `states` is an owned `Vec`, and
//!   the image admission snapshot is a scalar copy of an already-owned ledger.

use std::sync::atomic::Ordering;

use glib::subclass::prelude::ObjectSubclassIsExt;

use crate::services::markdown_render::MarkdownRenderState;

use super::test_policy::{
    IMAGE_CANCELLED_WORK, IMAGE_CANDIDATE_INSPECTIONS, IMAGE_DECODED_RESULTS, IMAGE_PIXEL_DROPS,
    IMAGE_PIXEL_DROPS_ON_GTK, MARKDOWN_SOURCE_COPIES,
};
use super::{LushtextMarkdownPreview, MAX_MARKDOWN_RETIREMENT_GENERATIONS};

/// One-active-plus-latest planning ownership.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MarkdownPlanningEvidence {
    /// Whether a planning worker owns the source right now.
    pub worker_running: bool,
    /// Whether a newer plan is queued behind the active one.
    pub queued: bool,
    /// Source copies admitted into guarded planning ownership, process-wide.
    pub source_copies: u64,
}

/// Bounded projection-slice accounting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MarkdownProjectionEvidence {
    /// Projection slices dispatched for the live render session.
    pub dispatch_count: u64,
    /// Times a slice hit its per-turn ceiling and yielded.
    pub high_water_events: usize,
}

/// Detached-render retirement and the backpressure it applies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MarkdownRetirementEvidence {
    /// Characters cleared in the largest single retirement turn.
    pub chars_high_water: usize,
    /// Widget/link references released in the largest single turn.
    pub items_high_water: usize,
    /// Detached generations still awaiting retirement.
    pub detached_generations: usize,
    /// Most detached generations ever retained at once.
    pub generations_high_water: usize,
    /// The retained-generation ceiling that triggers latest-render backpressure.
    pub max_generations: usize,
    /// Whether a render or projection is **parked** waiting for retirement room.
    ///
    /// This is the field that makes the row's backpressure inversion visible:
    /// only the retirement drain un-parks deferred work, so a test proving the
    /// drain resumes production reads this going true and then false.
    pub deferred_work_pending: bool,
    /// Off-GTK disposal jobs this widget has handed to the plain lane.
    pub plain_jobs: u64,
    /// Payload bytes still pending in the plain disposal lane.
    pub plain_pending: usize,
    /// Highest pending byte count the plain lane has held for this widget.
    pub plain_pending_high_water: usize,
}

/// Image work ownership, its ceilings, and its off-GTK disposal evidence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MarkdownImageEvidence {
    /// Image work items owned right now.
    pub owned_count: usize,
    /// Bytes owned by image work right now.
    pub owned_bytes: u64,
    /// Most image work items ever owned at once.
    pub high_water_count: usize,
    /// Most bytes ever owned by image work at once.
    pub high_water_bytes: u64,
    /// Configured ceiling on concurrently owned image work items.
    pub max_work_items: usize,
    /// Configured conservative ceiling on concurrently owned image bytes.
    pub max_work_bytes: u64,
    /// Per-file source byte ceiling.
    pub max_source_bytes: u64,
    /// Per-file decoded source-pixel ceiling.
    pub max_source_pixels: i64,
    /// Image candidates inspected, process-wide.
    pub candidate_inspections: usize,
    /// Image work cancelled before completion, process-wide.
    pub cancelled_work: usize,
    /// Decoded image results delivered, process-wide.
    pub decoded_results: usize,
    /// Decoded pixel buffers dropped, process-wide.
    pub pixel_drops: usize,
    /// Decoded pixel buffers dropped **on the GTK thread**, which should stay 0.
    pub pixel_drops_on_gtk: usize,
}

/// Embedded code-block width repair accounting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MarkdownCodeBlockEvidence {
    /// Full embed traversals performed to repair code-block widths.
    pub width_traversal_count: u64,
}

/// Everything a test may observe about the Markdown preview workflow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownPreviewEvidence {
    /// The render session's own state machine.
    pub render_state: MarkdownRenderState,
    /// Whether any stage of this workflow still owes work.
    pub render_pending: bool,
    /// The placeholder currently shown instead of content, if any.
    pub placeholder_description: Option<String>,
    /// Stage 2 — planning ownership.
    pub planning: MarkdownPlanningEvidence,
    /// Stage 3 — bounded projection.
    pub projection: MarkdownProjectionEvidence,
    /// Stage 5 — retirement and its backpressure.
    pub retirement: MarkdownRetirementEvidence,
    /// The image stage order.
    pub images: MarkdownImageEvidence,
    /// The code-block width repair stage order.
    pub code_blocks: MarkdownCodeBlockEvidence,
}

impl LushtextMarkdownPreview {
    /// Read the whole preview surface.
    ///
    /// Each `Ref` is taken, copied from, and dropped before the next, so no
    /// field is read while another borrow of the same state is live.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn evidence(&self) -> MarkdownPreviewEvidence {
        let imp = self.imp();

        let render_state = imp.render_session.borrow().state();
        let planning = MarkdownPlanningEvidence {
            worker_running: imp.planning_worker_running.get(),
            queued: imp.queued_plan.borrow().is_some(),
            source_copies: MARKDOWN_SOURCE_COPIES.load(Ordering::Acquire),
        };
        let projection = MarkdownProjectionEvidence {
            dispatch_count: imp.projection_dispatch_count.get(),
            high_water_events: imp.projection_high_water_events.get(),
        };
        let detached_generations = imp
            .retirement
            .borrow()
            .as_ref()
            .map_or(0, |session| session.states.len());
        let retirement = MarkdownRetirementEvidence {
            chars_high_water: imp.retirement_chars_high_water.get(),
            items_high_water: imp.retirement_items_high_water.get(),
            detached_generations,
            generations_high_water: imp.retirement_generations_high_water.get(),
            max_generations: MAX_MARKDOWN_RETIREMENT_GENERATIONS,
            deferred_work_pending: imp.deferred_work.borrow().is_some(),
            plain_jobs: imp.plain_retirement_jobs.load(Ordering::Acquire),
            plain_pending: imp.plain_retirement_pending.load(Ordering::Acquire),
            plain_pending_high_water: imp
                .plain_retirement_pending_high_water
                .load(Ordering::Acquire),
        };
        let image_snapshot = imp.image_admission.borrow().snapshot();
        let images = MarkdownImageEvidence {
            owned_count: image_snapshot.owned_count,
            owned_bytes: image_snapshot.owned_bytes,
            high_water_count: image_snapshot.high_water_count,
            high_water_bytes: image_snapshot.high_water_bytes,
            max_work_items: super::MAX_PREVIEW_IMAGE_WORK_ITEMS,
            max_work_bytes: super::MAX_PREVIEW_IMAGE_WORK_BYTES,
            max_source_bytes: super::MAX_PREVIEW_IMAGE_SOURCE_BYTES,
            max_source_pixels: super::MAX_PREVIEW_IMAGE_SOURCE_PIXELS,
            candidate_inspections: IMAGE_CANDIDATE_INSPECTIONS.load(Ordering::Acquire),
            cancelled_work: IMAGE_CANCELLED_WORK.load(Ordering::Acquire),
            decoded_results: IMAGE_DECODED_RESULTS.load(Ordering::Acquire),
            pixel_drops: IMAGE_PIXEL_DROPS.load(Ordering::Acquire),
            pixel_drops_on_gtk: IMAGE_PIXEL_DROPS_ON_GTK.load(Ordering::Acquire),
        };
        let code_blocks = MarkdownCodeBlockEvidence {
            width_traversal_count: imp.code_block_width_traversal_count.get(),
        };

        // Hoisted out of the struct literal deliberately. `render_pending()`
        // re-borrows four of the same `RefCell`s this accessor already read, and
        // `placeholder_description()` touches a template child. Calling either
        // from inside the literal works only because every borrow above is a
        // statement-scoped temporary — a single later edit hoisting one of those
        // borrows into a binding would turn this into the `BorrowMutError` the
        // reentrancy constraint exists to prevent. Keeping every read in its own
        // `let` makes that impossible to reintroduce by accident.
        let render_pending = self.render_pending();
        let placeholder_description = self.placeholder_description();

        MarkdownPreviewEvidence {
            render_state,
            render_pending,
            placeholder_description,
            planning,
            projection,
            retirement,
            images,
            code_blocks,
        }
    }
}
