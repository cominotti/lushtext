// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: test policy — this workflow's `test-utils` override values.
//!
//! Two things, both compiled out of a production build: the workflow's
//! `test-utils` override values, and the process-wide test-only storage those and
//! `evidence.rs` read. The storage moved here from the facade — a narrative
//! facade should own neither timing knobs nor counters — and the *readers* are
//! split by role: the three delay statics are consumed here, the seven
//! observation counters are consumed by `evidence.rs`.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

// The workflow's process-wide test-only storage, moved out of the facade. Three
// are timing overrides and seven are observation counters that `evidence.rs`
// reads; a narrative facade should own neither. The whole module is
// `test-utils`-gated, so none of this compiles into a production build and the
// per-item gates the facade needed are gone.
pub(super) static IMAGE_WORK_DELAY_MS: AtomicU64 = AtomicU64::new(0);
pub(super) static IMAGE_POST_DECODE_DELAY_MS: AtomicU64 = AtomicU64::new(0);
pub(super) static IMAGE_CANDIDATE_INSPECTIONS: AtomicUsize = AtomicUsize::new(0);
pub(super) static IMAGE_CANCELLED_WORK: AtomicUsize = AtomicUsize::new(0);
pub(super) static IMAGE_DECODED_RESULTS: AtomicUsize = AtomicUsize::new(0);
pub(super) static IMAGE_PIXEL_DROPS: AtomicUsize = AtomicUsize::new(0);
pub(super) static IMAGE_PIXEL_DROPS_ON_GTK: AtomicUsize = AtomicUsize::new(0);
pub(super) static IMAGE_TEST_GTK_THREAD: Mutex<Option<std::thread::ThreadId>> = Mutex::new(None);
pub(super) static MARKDOWN_PLAN_DELAY_MS: AtomicU64 = AtomicU64::new(0);
pub(super) static MARKDOWN_SOURCE_COPIES: AtomicU64 = AtomicU64::new(0);

use crate::ui::buffer_snapshot::BufferSnapshotPayload;

#[cfg(feature = "test-utils")]
use super::{LushtextMarkdownPreview, MarkdownPreviewRenderContext};

impl LushtextMarkdownPreview {
    /// Delay Markdown planning workers for deterministic supersession tests.
    #[cfg(feature = "test-utils")]
    pub fn set_markdown_plan_delay_for_test(delay_ms: u64) {
        MARKDOWN_PLAN_DELAY_MS.store(delay_ms, Ordering::Release);
    }

    /// Render an owned direct snapshot through the worker reservation boundary.
    #[cfg(feature = "test-utils")]
    pub fn render_snapshot_for_test(&self, source: String) {
        self.render_snapshot_with_context(
            BufferSnapshotPayload::direct(source),
            MarkdownPreviewRenderContext::default(),
        );
    }
}
