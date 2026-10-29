// SPDX-License-Identifier: GPL-3.0-or-later

//! The window-composition workflow's single test-only override value.
//!
//! Two overrides, both read once per window when it begins composing:
//!
//! * **collapse to one dispatch** reveals every scheduled stage in stage C
//!   itself, so a bisection (and a widget test's reference window) can compare
//!   staged and unstaged composition in one build (design, "Migration Plan");
//! * **hold before a stage** keeps that reveal stage and every later one from
//!   being queued until the test releases the window
//!   (`LushtextWindow::release_composition_hold_for_test`). Holding before
//!   `Content` holds composition after the first frame, so the visual-geometry
//!   lane can capture it (it otherwise lasts about one frame, design D7), and a
//!   widget test can issue a forced path before any one stage.
//!
//! Without `test-utils` neither exists and no override storage compiles.
//!
//! The smoke-only `composition-hold` feature adds one more, read from the
//! environment because the visual-geometry lane launches the real binary: with
//! `LUSHTEXT_COMPOSITION_HOLD_MS=<ms>` every window holds before `Content` for
//! that long after its first frame, so the lane can capture the first frame
//! and the composed frame in one session (design D7). A build without the
//! feature reads no environment variable here.

use super::policy::CompositionStage;

#[cfg(feature = "test-utils")]
mod overrides {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::CompositionStage;

    static COLLAPSE: AtomicBool = AtomicBool::new(false);
    static HOLD_BEFORE: Mutex<Option<CompositionStage>> = Mutex::new(None);

    /// Reveal every scheduled stage in stage C for windows constructed after
    /// this call.
    pub fn set_composition_collapsed_for_test(collapsed: bool) {
        COLLAPSE.store(collapsed, Ordering::Release);
    }

    /// Hold composition before `stage` (and every later reveal) for windows
    /// constructed after this call; `None` stops holding new windows.
    pub fn set_composition_hold_before_for_test(stage: Option<CompositionStage>) {
        *HOLD_BEFORE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = stage;
    }

    pub(in crate::ui::window::composition) fn collapse_to_one_dispatch() -> bool {
        COLLAPSE.load(Ordering::Acquire)
    }

    pub(in crate::ui::window::composition) fn hold_before() -> Option<CompositionStage> {
        *HOLD_BEFORE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(feature = "test-utils")]
pub(super) use overrides::{collapse_to_one_dispatch, hold_before};
#[cfg(feature = "test-utils")]
pub use overrides::{set_composition_collapsed_for_test, set_composition_hold_before_for_test};

/// Production: composition is never collapsed.
#[cfg(not(feature = "test-utils"))]
#[inline]
pub(super) const fn collapse_to_one_dispatch() -> bool {
    false
}

/// Production: composition is never held.
#[cfg(not(feature = "test-utils"))]
#[inline]
pub(super) const fn hold_before() -> Option<CompositionStage> {
    None
}

/// The environment variable the `composition-hold` build reads.
#[cfg(feature = "composition-hold")]
const COMPOSITION_HOLD_ENV: &str = "LUSHTEXT_COMPOSITION_HOLD_MS";

/// How long the `composition-hold` build holds a window before `Content`
/// after its first frame, when `LUSHTEXT_COMPOSITION_HOLD_MS` asks it to.
#[cfg(feature = "composition-hold")]
pub(super) fn timed_hold() -> Option<std::time::Duration> {
    std::env::var(COMPOSITION_HOLD_ENV)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|ms| *ms > 0)
        .map(std::time::Duration::from_millis)
}

/// Every other build holds nothing.
#[cfg(not(feature = "composition-hold"))]
#[inline]
pub(super) const fn timed_hold() -> Option<std::time::Duration> {
    None
}
