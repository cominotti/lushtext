// SPDX-License-Identifier: GPL-3.0-or-later

//! Preview-width clamping policy and limited-plan constructor for Markdown preview.
//!
//! Inline-footnote lowering now lives in
//! `lushtext_services::notes::markdown_render::inline_footnotes`, keeping
//! `pulldown_cmark` encapsulated outside `lushtext-ui`. The side-by-side
//! preview-width clamp stays here so Kani checks it GTK-free in
//! `policy/kani_proofs.rs` (`core-geometry-policies` shard).

#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]

#[cfg(kani)]
mod kani_proofs;

use lushtext_model::values::Sp;
use lushtext_services::notes::markdown_render::{
    MarkdownPlanLimit, MarkdownPlanMetrics, MarkdownRenderPlan,
};

/// The plan published when inline-footnote lowering hits its own limit.
///
/// Production policy with three production callers (`admission.rs` twice and
/// `planning_execution.rs` once). It reports the *source* byte count alongside
/// the `InlineFootnotes` limit so a reader of the published plan can tell a
/// limited render from an empty one.
pub(super) fn inline_footnote_limited_plan(source_bytes: usize) -> MarkdownRenderPlan {
    MarkdownRenderPlan {
        batches: Vec::new(),
        metrics: MarkdownPlanMetrics {
            source_bytes,
            ..MarkdownPlanMetrics::default()
        },
        limit: Some(MarkdownPlanLimit::InlineFootnotes),
    }
}

// ─── Side-by-side preview width ───────────────────────────────────────
//
// The window shell (`ui/window/preview.rs`, `ui/window/imp.rs`, and
// `ui/window/geometry/execution.rs`) sizes the end preview pane from these.

/// Tiny non-zero floor used only before the first real preview-width sync.
pub(crate) const PREVIEW_MIN_WIDTH_SP: Sp = Sp::new(1);
/// Fallback side-by-side preview width for invalid legacy settings.
pub(crate) const PREVIEW_DEFAULT_WIDTH_SP: Sp = Sp::new(300);
/// The side-by-side preview may consume at most one part in this many of the
/// editor content width.
pub(super) const PREVIEW_MAX_WIDTH_DIVISOR: i32 = 3;

/// The stored preferred preview width, with zero or less meaning the
/// [`PREVIEW_DEFAULT_WIDTH_SP`] default.
pub(crate) fn preferred_preview_width(width: Sp) -> Sp {
    if width > Sp::ZERO {
        width
    } else {
        PREVIEW_DEFAULT_WIDTH_SP
    }
}

/// Resolve the side-by-side preview width, in whole pixels, from the preferred
/// width and the available content width (zero or less counts as 1 sp).
///
/// The result is at least the [`PREVIEW_MIN_WIDTH_SP`] floor. With at least
/// 3 sp available it is at most one third of the available width, and a
/// preferred width between the floor and that bound is kept exactly. **Below
/// 3 sp the floor wins**: `available / 3` is 0 there, so the result is 1 sp,
/// more than one third. That state is reachable only before the first
/// allocation or with a 1–2 px content box, where no preview is readable, and
/// the floor keeps the split view's sidebar constraint non-zero.
///
/// The arithmetic is integer-only; the window converts the result to `f64`
/// once, where it sets the split view's constraints.
///
/// Kani proves all of this over every [`Sp`] preferred and available width,
/// and keeps the unconditional "at most one third" form as a `should_panic`
/// counterexample (`policy/kani_proofs.rs`).
pub(crate) fn clamped_preview_width(preferred_width: Sp, available_width: Sp) -> Sp {
    let max_width = Sp::new(
        (available_width.as_sp().max(1) / PREVIEW_MAX_WIDTH_DIVISOR)
            .max(PREVIEW_MIN_WIDTH_SP.as_sp()),
    );
    preferred_width.max(PREVIEW_MIN_WIDTH_SP).min(max_width)
}

#[cfg(test)]
mod preview_width_tests {
    use super::{Sp, clamped_preview_width};

    #[test]
    fn preview_width_floor_wins_below_three_sp() {
        assert_eq!(clamped_preview_width(Sp::new(500), Sp::new(2)), Sp::new(1));
        assert!(clamped_preview_width(Sp::new(500), Sp::new(900)) <= Sp::new(300));
        assert_eq!(
            clamped_preview_width(Sp::new(200), Sp::new(900)),
            Sp::new(200)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_inline_footnote_limited_plan_reports_the_source_size_it_refused() {
        // This function exists to publish the *source* byte count next to the
        // limit, so a reader of the plan can distinguish "we refused 1.2 MB of
        // footnotes" from "there was nothing to render". Nothing asserted that
        // until mutation testing deleted the field and no test noticed.
        let plan = inline_footnote_limited_plan(1_234_567);
        assert_eq!(plan.metrics.source_bytes, 1_234_567);
        assert_eq!(plan.limit, Some(MarkdownPlanLimit::InlineFootnotes));
        assert!(plan.batches.is_empty(), "a limited plan renders no batches");

        // Zero is a real input (an empty source can still trip the limit path),
        // and it must stay distinguishable from the limit being absent.
        let empty = inline_footnote_limited_plan(0);
        assert_eq!(empty.metrics.source_bytes, 0);
        assert_eq!(empty.limit, Some(MarkdownPlanLimit::InlineFootnotes));
    }
}
