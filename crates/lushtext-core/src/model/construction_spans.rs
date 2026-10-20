// SPDX-License-Identifier: GPL-3.0-or-later

//! Monotonic spans of application startup and window construction.
//!
//! `bound-window-construction-dispatch` (C4 of the long-line programme,
//! `docs/next/long-line-documents.md`) measures where the time before a
//! window's first presented frame goes, so it can be bounded by the settle
//! budget (`model::responsiveness_budget::B_SETTLE_US`). This module holds the
//! measurement as plain data; the GTK side (`ui::construction_spans`) reads
//! `g_get_monotonic_time()` and feeds it in. Every time here is in those
//! microseconds, the clock the responsiveness lane also uses.
//!
//! The record is always compiled, in every build: the shell-geometry evidence
//! and the automation snapshot project it, and recording costs a handful of
//! clock reads per window. It carries names the code chose and durations,
//! never a path, a setting value, or document content.

/// How many construction stages one window records; later stages are counted
/// in [`WindowConstructionSpans::dropped_stages`] instead of growing the record.
pub const MAX_CONSTRUCTION_STAGES: usize = 48;

/// The process-wide startup spans: GTK's own startup and LushText's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StartupSpans {
    /// `parent_startup`, which is mostly `gtk_init` and `adw_init`
    /// (toolkit-owned; measured and reported, not budgeted).
    pub toolkit_us: Option<u64>,
    /// LushText's own `startup` work after `parent_startup`.
    pub lushtext_us: Option<u64>,
}

/// One named sub-span of a window's construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstructionStageSpan {
    /// The stage's stable name (an identifier, never content).
    pub name: &'static str,
    /// When it started.
    pub start_us: i64,
    /// How long it took.
    pub duration_us: u64,
}

/// The construction record of one window.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WindowConstructionSpans {
    started_us: Option<i64>,
    stages: Vec<ConstructionStageSpan>,
    dropped_stages: u32,
    construction_end_us: Option<i64>,
    first_frame_us: Option<i64>,
    largest_dispatch_before_first_frame_us: Option<u64>,
}

/// `end − start` as a duration; a clock that ran backwards reads as zero.
#[must_use]
pub fn span_duration_us(start_us: i64, end_us: i64) -> u64 {
    u64::try_from(end_us.saturating_sub(start_us)).unwrap_or(0)
}

impl WindowConstructionSpans {
    /// Start the record at `now_us`; a second call keeps the first start.
    pub fn begin(&mut self, now_us: i64) {
        self.started_us.get_or_insert(now_us);
    }

    /// Record one stage that ran from `start_us` to `end_us`.
    pub fn record_stage(&mut self, name: &'static str, start_us: i64, end_us: i64) {
        if self.stages.len() >= MAX_CONSTRUCTION_STAGES {
            self.dropped_stages = self.dropped_stages.saturating_add(1);
            return;
        }
        self.stages.push(ConstructionStageSpan {
            name,
            start_us,
            duration_us: span_duration_us(start_us, end_us),
        });
    }

    /// Construction returned control to the main loop at `now_us`. Only the
    /// first call counts.
    pub fn finish_construction(&mut self, now_us: i64) {
        if self.started_us.is_some() {
            self.construction_end_us.get_or_insert(now_us);
        }
    }

    /// The window's first frame was painted at `now_us`, and the largest main
    /// loop dispatch observed until then was `largest_dispatch_us` (`None`
    /// when no watchdog measured it). Only the first call counts.
    pub fn record_first_frame(&mut self, now_us: i64, largest_dispatch_us: Option<u64>) {
        if self.started_us.is_none() || self.first_frame_us.is_some() {
            return;
        }
        self.first_frame_us = Some(now_us);
        self.largest_dispatch_before_first_frame_us = largest_dispatch_us;
    }

    /// When construction began, if it has.
    #[must_use]
    pub fn started_us(&self) -> Option<i64> {
        self.started_us
    }

    /// Whether the window has painted its first frame.
    #[must_use]
    pub fn first_frame_painted(&self) -> bool {
        self.first_frame_us.is_some()
    }

    /// From `LushtextWindow::new` until control returned to the main loop.
    #[must_use]
    pub fn window_construction_us(&self) -> Option<u64> {
        Some(span_duration_us(
            self.started_us?,
            self.construction_end_us?,
        ))
    }

    /// From `LushtextWindow::new` until the first painted frame.
    #[must_use]
    pub fn first_frame_us(&self) -> Option<u64> {
        Some(span_duration_us(self.started_us?, self.first_frame_us?))
    }

    /// The largest main-loop dispatch before the first frame, when a watchdog
    /// measured it (the smoke-only responsiveness probe); never `Some(0)` for
    /// "unmeasured".
    #[must_use]
    pub fn largest_dispatch_before_first_frame_us(&self) -> Option<u64> {
        self.largest_dispatch_before_first_frame_us
    }

    /// The recorded stages, in the order they ran.
    #[must_use]
    pub fn stages(&self) -> &[ConstructionStageSpan] {
        &self.stages
    }

    /// Stages not recorded because the record was full.
    #[must_use]
    pub fn dropped_stages(&self) -> u32 {
        self.dropped_stages
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_measure_from_the_first_begin() {
        let mut spans = WindowConstructionSpans::default();
        assert_eq!(spans.window_construction_us(), None);
        spans.begin(1_000);
        spans.begin(5_000);
        spans.record_stage("template", 1_000, 1_400);
        spans.finish_construction(3_000);
        spans.finish_construction(9_000);
        spans.record_first_frame(4_500, Some(2_100));
        spans.record_first_frame(8_000, Some(9));
        assert_eq!(spans.started_us(), Some(1_000));
        assert_eq!(spans.window_construction_us(), Some(2_000));
        assert_eq!(spans.first_frame_us(), Some(3_500));
        assert_eq!(spans.largest_dispatch_before_first_frame_us(), Some(2_100));
        assert!(spans.first_frame_painted());
        assert_eq!(spans.stages()[0].duration_us, 400);
    }

    #[test]
    fn nothing_finishes_before_it_began() {
        let mut spans = WindowConstructionSpans::default();
        spans.finish_construction(10);
        spans.record_first_frame(20, Some(5));
        assert_eq!(spans.window_construction_us(), None);
        assert_eq!(spans.first_frame_us(), None);
        assert!(!spans.first_frame_painted());
    }

    #[test]
    fn a_backwards_clock_reads_as_zero() {
        assert_eq!(span_duration_us(10, 4), 0);
        assert_eq!(span_duration_us(i64::MIN, i64::MAX), u64::MAX >> 1);
    }

    #[test]
    fn stages_are_bounded_and_counted() {
        let mut spans = WindowConstructionSpans::default();
        spans.begin(0);
        for index in 0..(MAX_CONSTRUCTION_STAGES + 3) {
            let start = i64::try_from(index).expect("fits");
            spans.record_stage("stage", start, start + start);
        }
        assert_eq!(spans.stages().len(), MAX_CONSTRUCTION_STAGES);
        assert_eq!(spans.dropped_stages(), 3);
        assert_eq!(
            spans.stages().last().map(|stage| stage.duration_us),
            Some(u64::try_from(MAX_CONSTRUCTION_STAGES - 1).expect("fits"))
        );
    }
}
