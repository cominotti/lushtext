// SPDX-License-Identifier: GPL-3.0-or-later

//! Render Markdown into a read-only preview.
//!
//! Most blocks become `GtkTextTag`s in a `GtkTextBuffer`, so the preview stays
//! native and cheap. Tables, local images, and fenced code blocks are the
//! exceptions: they are real GTK widgets anchored into the text flow through
//! `GtkTextChildAnchor`, because styled text is not expressive enough for them.
//! Two display states — content mode (the scrolled text view) and placeholder
//! mode (an `AdwStatusPage`) — plus a third the note editors ask for, a
//! placeholder rendered *inside* content mode so an Edit/Render stack measures
//! the same text surface before and after its first render.
//!
//! # Stage orders and where control resumes
//!
//! **Seven ordered stage orders and sixteen resumption points.** The census
//! recorded five inversions; that figure was low by roughly 3.2x, and the
//! discrepancy matters because two of the uncounted ones re-enter with **no
//! settle armed** and one reverses the usual direction of control entirely.
//!
//! **A. Render.** Entry: `render_markdown`, `render_markdown_with_context`,
//! `render_snapshot_with_context`, or the note surfaces. `admission` decides
//! whether the source may be owned at all — source bytes, retained bytes, and
//! disposal-lane capacity — and either opens a render session generation or
//! shows a paused, limited, or memory-pressure state. Small sources project
//! immediately; large ones go to planning. A source refused for **retirement
//! backlog** parks, and stage E is the only thing that will wake it.
//!
//! **B. Plan** (`planning_execution`, sources over the background threshold).
//! One active worker plus one replaceable latest plan. *(resume 1)* the worker
//! completion; *(resume 2)* a **second** resumption inside that same completion,
//! where a queued superseding plan is re-dispatched rather than starting a
//! second worker.
//!
//! **C. Project** (`projection_execution`). At most one batch per main-loop
//! turn, so a long document renders progressively instead of freezing the frame.
//! *(resume 3)* each queued batch turn, every one revalidating the session
//! generation before it may publish. Cross-turn state is reified in
//! `MarkdownProjectionContinuation`; a `ContinuationBreach` fails the render
//! loudly rather than projecting onto a buffer it cannot account for.
//!
//! **D. Images** (`images`). A serial queue: each decode runs on a worker.
//! *(resume 4)* the decode completion, and *(resume 5)* the queue drain
//! re-entering itself, because one completion starts the next decoder.
//!
//! **E. Retire** (`retirement`). Detaching a rendered buffer is O(1); freeing it
//! is not, so it is retired in bounded turns and its document-sized body is
//! destroyed off the GTK thread. *(resume 6)* each bounded drain turn;
//! *(resume 7)* the plain-disposal lane's terminal.
//!
//! **The backpressure inversion — read this one.** *(resume 8)* When retirement
//! is at capacity, stages A and C **park**, and **only the retirement drain
//! un-parks them**. The *retirement* lane restarts *production* work, which is
//! the opposite of the usual direction and appears in no recorded stage trace of
//! this row. `evidence.retirement.deferred_work_pending` is how a test sees it.
//!
//! **F. Code-block width repair** (`code_blocks`). Anchored widgets do not fill
//! the text column by themselves. *(resume 9)* a settle burst after a shell
//! transition; *(resume 10)* its idle width pass; *(resume 11)* a replaceable
//! generation-guarded timeout that closes the settle. Then two **passive
//! re-entries by a different actor, with no settle armed**: *(resume 12)*
//! `notify::width` / `notify::left-margin` / `notify::right-margin`, and
//! *(resume 13)* `connect_map`.
//!
//! **G. Refresh debounce.** *(resume 14)* a 300 ms debounce on buffer change,
//! and *(resume 15)* the chunked source-capture completion, which revalidates the
//! editor, the draft generation, the load generation, and the path before it
//! renders anything. *(resume 16)* the shell's own preview layout-settle path,
//! owned by `ui/window/preview.rs`.
//!
//! # Module roles
//!
//! | Module | Role |
//! | --- | --- |
//! | `mod.rs` (this file) | narrative facade |
//! | `policy` | pure policy — inline-footnote lowering, the limited-plan shapes, and the fuzz/property entry points |
//! | `admission` | coordination — ownership ceilings and the render session generation |
//! | `planning_execution` | coordination — the parse worker, stage-order-qualified |
//! | `projection_execution` | coordination — bounded per-turn projection, stage-order-qualified |
//! | `retirement` | coordination — bounded destruction and the backpressure inversion |
//! | `seams` | seam value objects — freshness/ownership seams and render-time projection values |
//! | `evidence` | evidence surface; `test-utils`-gated, so production never reads it |
//! | `test_policy` | test policy — one timing override and one direct-render actuator; `test-utils`-gated |
//!
//! **Called presentation surfaces, which are not roles:** `widgets` (the text
//! view, scroller geometry, readable-column margins, placeholder and failure
//! states, embedded-widget insertion), `imp` (template children and tag table),
//! and the topical renderers `code_blocks`, `images`, `links`, `tables`,
//! `text_flow`, and `continuation`. **Those seven carry the topical
//! decomposition two earlier changes paid for and were deliberately not
//! re-decomposed here** — this migration assigned roles to code that had none
//! and moved no responsibility between existing modules. The only edits they
//! received were import paths.
//!
//! `ui/window/preview.rs` is this workflow's **window-side** called presentation
//! surface, owning the Alt+P shell workflow and the layout-settle path. The note
//! browser, bookmark, and editor surfaces are a **second independent consumer**
//! of this same widget, belonging to the migrated `WFR-NOTES-BOOKMARKS` row.
//! `services/markdown_render.rs` is the shared GTK-free planner; it owns four of
//! this row's six seam value objects and is not this row's to move.

// Private GObject implementation for the template-backed preview surface.
mod admission;
mod code_blocks;
mod continuation;
#[cfg(feature = "test-utils")]
pub mod evidence;
mod images;
mod imp;
mod links;
mod planning_execution;
mod policy;
mod projection_execution;
mod retirement;
mod seams;
mod tables;
#[cfg(feature = "test-utils")]
mod test_policy;
mod text_flow;
mod widgets;

use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use std::sync::atomic::Ordering;

pub use crate::services::markdown_render::MarkdownRenderState;

use images::{ActiveImageWork, PendingImageWork};

#[cfg(feature = "test-utils")]
pub use evidence::{
    MarkdownCodeBlockEvidence, MarkdownImageEvidence, MarkdownPlanningEvidence,
    MarkdownPreviewEvidence, MarkdownProjectionEvidence, MarkdownRetirementEvidence,
};

// The window shell, note surfaces, and the fuzz targets reach these through
// `crate::ui::markdown_preview::…`, so the facade re-exports them from the role
// modules that own them.
#[cfg(feature = "property-tests")]
pub use policy::lower_inline_footnotes_for_property_test;
#[cfg(feature = "fuzzing")]
pub use policy::{lowered_markdown_for_fuzzing, preprocess_markdown_for_fuzzing};
pub use seams::MarkdownPreviewRenderContext;

glib::wrapper! {
    // Exposes the private preview implementation as a regular GTK widget for
    // editor tabs, note dialogs, and widget tests.
    /// Public Markdown preview widget used by editor tabs and note surfaces.
    ///
    /// The wrapper exposes render and navigation methods; the private
    /// implementation owns GtkTextView tags, anchors, and launch state.
    pub struct LushtextMarkdownPreview(ObjectSubclass<imp::LushtextMarkdownPreview>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

/// Maximum width for one rendered preview image before we scale it down.
///
/// The preview lives inside a `GtkTextView` child-anchor slot, so very large
/// images need a hard ceiling to avoid pushing the text flow into unusable
/// widths while still staying visibly image-like.
const MAX_PREVIEW_IMAGE_WIDTH: i32 = 640;
/// Minimum target size for tiny local images in preview.
///
/// Very small assets such as tiny badges or icons are technically valid, but
/// rendering them at source size makes them feel broken in a document preview.
/// A modest floor keeps them legible without pretending the preview is a full
/// graphics viewer.
const MIN_PREVIEW_IMAGE_SIZE: i32 = 72;
/// Interior horizontal inset for native code-block widgets.
///
/// The old text-tag renderer painted the block background directly behind the
/// glyphs. Keeping padding on the embedded scroller makes the source text read
/// as one deliberate surface instead of text stuck to a highlight edge.
const CODE_BLOCK_HORIZONTAL_PADDING: i32 = 12;
/// Interior vertical inset for native code-block widgets.
const CODE_BLOCK_VERTICAL_PADDING: i32 = 8;
/// CSS priority for per-render code-block palette fixes.
///
/// The bundled stylesheet gives code blocks their shape, while this provider
/// supplies the active GtkSourceView background after the user-selected scheme
/// is known. A slightly higher priority keeps the two layers from fighting.
const CODE_BLOCK_BACKGROUND_CSS_PRIORITY: u32 = gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION + 2;
/// Maximum source image bytes the preview will decode for one Markdown image.
///
/// A 32 MiB source still covers normal screenshots and exported diagrams, but
/// it prevents accidental camera originals or generated art from dominating a
/// background worker and its post-decode pixel copy.
const MAX_PREVIEW_IMAGE_SOURCE_BYTES: u64 = 32 * 1024 * 1024;
/// Maximum source pixels accepted before the preview falls back to a compact notice.
///
/// 64 megapixels is intentionally above ordinary desktop images while below
/// the point where RGB/RGBA decode buffers can spike into hundreds of MiB.
const MAX_PREVIEW_IMAGE_SOURCE_PIXELS: i64 = 64_000_000;
/// Maximum local-image descriptors that may own queued or active work.
const MAX_PREVIEW_IMAGE_WORK_ITEMS: usize = 4;
/// Conservative ownership charge for source bytes plus one bounded RGBA decode.
const PREVIEW_IMAGE_WORK_CHARGE_BYTES: u64 = MAX_PREVIEW_IMAGE_SOURCE_BYTES.saturating_add(
    (MAX_PREVIEW_IMAGE_WIDTH as u64)
        .saturating_pow(2)
        .saturating_mul(4),
);
/// Total conservative bytes admitted across active and compact queued image work.
const MAX_PREVIEW_IMAGE_WORK_BYTES: u64 =
    PREVIEW_IMAGE_WORK_CHARGE_BYTES.saturating_mul(MAX_PREVIEW_IMAGE_WORK_ITEMS as u64);
/// Maximum literal text bytes highlighted inside one native code-block widget.
///
/// GtkSourceView highlighting is excellent for excerpts, but 64 KiB keeps a
/// single fenced block from monopolizing a render turn when preview refreshes.
const MAX_PREVIEW_CODE_BLOCK_BYTES: usize = 64 * 1024;
/// Inputs above this size are parsed away from GTK before bounded projection.
const MARKDOWN_BACKGROUND_PLAN_THRESHOLD_BYTES: usize = 64 * 1024;
/// Maximum detached text characters removed in one main-loop retirement turn.
const MARKDOWN_RETIREMENT_CHARS_PER_TURN: usize = 64 * 1024;
/// Maximum detached widget/link references released in one retirement turn.
const MARKDOWN_RETIREMENT_ITEMS_PER_TURN: usize = 64;
/// Maximum ordinary detached generations retained before latest-render backpressure.
const MAX_MARKDOWN_RETIREMENT_GENERATIONS: usize = 2;

impl LushtextMarkdownPreview {
    #[must_use]
    pub fn new() -> Self {
        Object::builder().build()
    }

    /// Whether current planning/projection blocks exact preview readiness.
    #[must_use]
    pub fn render_pending(&self) -> bool {
        self.imp().render_session.borrow().pending()
            || self.imp().planning_worker_running.get()
            || self.imp().queued_plan.borrow().is_some()
            || self.imp().deferred_work.borrow().is_some()
            || self.imp().plain_retirement_pending.load(Ordering::Acquire) > 0
            || self.imp().current_image_work_count.get() > 0
            || self.imp().retirement.borrow().is_some()
    }
}

impl Default for LushtextMarkdownPreview {
    fn default() -> Self {
        Self::new()
    }
}
