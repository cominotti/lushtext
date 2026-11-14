// SPDX-License-Identifier: GPL-3.0-or-later

//! Sliced retirement of a lazy list's realized rows, outside input dispatches
//! (`lighten-notes-browser-rows` D4).
//!
//! **Cross-cutting coordination, no role.** Two workflows retire a list's rows
//! this way, `WFR-NOTES-BOOKMARKS` (the browser's rows after its dialog
//! closes) and `WFR-COMMAND-PALETTE` (the category view's rows after it is
//! left), so it is shared rather than forked; its pure half is
//! `model::list_budget` (`unheld_ranges`, `retirement_unit_cut`,
//! `turn_continues`). Named in both matrix rows.
//!
//! # Why
//!
//! Emptying a lazy model in one `items-changed` tears down every row widget
//! GTK realized (about 205, ledger A1) in that one call: 13–15 ms inside the
//! notes browser's close, 8–12 ms in the palette's deferred clear. A
//! retirement removes them in turns instead:
//!
//! 1. It waits until the list is unmapped (the dialog's close animation has
//!    ended; the category view is hidden), so no visible row changes.
//! 2. Its first turn removes the rows GTK holds no object for, after, between
//!    and before the held rows (its band, its focus row, the selection), one
//!    emission per range: removing them tears nothing down and realizes
//!    nothing (ledger A110), and once they are gone GTK has no other row to
//!    realize as the band shrinks.
//! 3. Each later turn removes units of [`LIST_RETIREMENT_UNIT_ROWS`] rows from
//!    the end while `turn_continues` holds, at `G_PRIORITY_LOW`: a turn costs
//!    at most the slice plus one unit.
//!
//! The retirement holds the list view (and whatever the caller asks it to
//! keep, such as the browser state that keeps its dialog) until the model is
//! empty: finalizing the list would tear every remaining row down at once.
//! Dropping or cancelling the [`RowRetirement`] stops it; what it held is then
//! released in that dispatch (a publish over the remaining rows, a window's
//! disposal).

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

#[cfg(doc)]
use crate::model::list_budget::LIST_RETIREMENT_UNIT_ROWS;
use crate::model::list_budget::{retirement_unit_cut, turn_continues, unheld_ranges};

/// A lazy list model whose rows a [`RowRetirement`] removes.
///
/// Every removal must be announced with `items-changed` while the model
/// already reports the shorter length (GTK reads the model inside the
/// handler).
pub(crate) trait RetiringRows {
    /// Rows listed.
    fn rows_len(&self) -> u32;
    /// The positions whose row object someone still holds (GTK's realized
    /// rows, its focus row and the selection).
    fn held_positions(&self) -> Vec<u32>;
    /// Remove `count` rows from `start`, in one emission.
    fn remove_range(&self, start: u32, count: u32);
    /// The list is empty: release what the listed rows referred to.
    fn release(&self);
}

/// What a retirement owns while it runs.
struct RetirementJob {
    model: Box<dyn RetiringRows>,
    /// Kept alive until the model is empty (finalizing it would tear every
    /// realized row down at once).
    _list_view: gtk4::ListView,
    /// The caller's keep-alive (the browser state that holds the dialog).
    _keep: Box<dyn Any>,
    on_done: Option<Box<dyn FnOnce()>>,
    first_turn_done: bool,
}

#[derive(Default)]
struct RetirementInner {
    job: RefCell<Option<RetirementJob>>,
    source: RefCell<Option<glib::SourceId>>,
    unmap_wait: RefCell<Option<(glib::WeakRef<gtk4::ListView>, glib::SignalHandlerId)>>,
    remaining: Cell<u32>,
    turns: Cell<u64>,
    longest_turn_us: Cell<u64>,
}

/// Content-free evidence of one retirement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowRetirementEvidence {
    /// Rows still listed, waiting to be removed (0 once done).
    pub retiring_rows: u32,
    /// Turns run.
    pub turns: u64,
    /// The longest turn, microseconds.
    pub longest_turn_us: u64,
}

/// One running retirement; dropping it cancels it, leaving the rows it had
/// not removed listed and releasing what it held (its `on_done` does not run).
pub(crate) struct RowRetirement {
    inner: Rc<RetirementInner>,
}

impl RowRetirement {
    /// Start retiring `model`'s rows from `list_view` once the list is
    /// unmapped, holding `keep` until the model is empty, then run `on_done`.
    pub(crate) fn start(
        model: Box<dyn RetiringRows>,
        list_view: &gtk4::ListView,
        keep: Box<dyn Any>,
        on_done: Box<dyn FnOnce()>,
    ) -> Self {
        let inner = Rc::new(RetirementInner::default());
        inner.remaining.set(model.rows_len());
        inner.job.replace(Some(RetirementJob {
            model,
            _list_view: list_view.clone(),
            _keep: keep,
            on_done: Some(on_done),
            first_turn_done: false,
        }));
        if list_view.is_mapped() {
            let weak = Rc::downgrade(&inner);
            let handler = list_view.connect_unmap(move |list_view| {
                if let Some(inner) = weak.upgrade() {
                    if let Some((_, handler)) = inner.unmap_wait.take() {
                        list_view.disconnect(handler);
                    }
                    schedule_turns(&inner);
                }
            });
            inner
                .unmap_wait
                .replace(Some((list_view.downgrade(), handler)));
        } else {
            schedule_turns(&inner);
        }
        Self { inner }
    }

    /// Whether rows are still being retired.
    #[must_use]
    pub(crate) fn is_retiring(&self) -> bool {
        self.inner.job.borrow().is_some()
    }

    /// The retirement's evidence.
    #[must_use]
    pub(crate) fn evidence(&self) -> RowRetirementEvidence {
        RowRetirementEvidence {
            retiring_rows: if self.is_retiring() {
                self.inner.remaining.get()
            } else {
                0
            },
            turns: self.inner.turns.get(),
            longest_turn_us: self.inner.longest_turn_us.get(),
        }
    }
}

impl Drop for RowRetirement {
    fn drop(&mut self) {
        cancel(&self.inner);
    }
}

fn cancel(inner: &RetirementInner) {
    if let Some(source) = inner.source.take() {
        source.remove();
    }
    if let Some((list_view, handler)) = inner.unmap_wait.take()
        && let Some(list_view) = list_view.upgrade()
    {
        list_view.disconnect(handler);
    }
    let job = inner.job.take();
    drop(job);
}

fn schedule_turns(inner: &Rc<RetirementInner>) {
    let weak = Rc::downgrade(inner);
    let source = glib::idle_add_local_full(glib::Priority::LOW, move || {
        let Some(inner) = weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        if run_turn(&inner) {
            glib::ControlFlow::Continue
        } else {
            inner.source.take();
            glib::ControlFlow::Break
        }
    });
    inner.source.replace(Some(source));
}

/// One turn; `true` while rows are left.
fn run_turn(inner: &RetirementInner) -> bool {
    let started = glib::monotonic_time();
    let elapsed = || u64::try_from(glib::monotonic_time() - started).unwrap_or(0);
    let left = {
        let mut slot = inner.job.borrow_mut();
        let Some(job) = slot.as_mut() else {
            return false;
        };
        if !job.first_turn_done {
            job.first_turn_done = true;
            for (start, count) in unheld_ranges(job.model.rows_len(), &job.model.held_positions()) {
                job.model.remove_range(start, count);
            }
        }
        let mut units = 0usize;
        while turn_continues(elapsed(), units) {
            let Some(next) = retirement_unit_cut(job.model.rows_len()) else {
                break;
            };
            let len = job.model.rows_len();
            job.model.remove_range(next, len - next);
            units += 1;
        }
        let left = job.model.rows_len();
        inner.remaining.set(left);
        left
    };
    inner.turns.set(inner.turns.get().saturating_add(1));
    inner
        .longest_turn_us
        .set(inner.longest_turn_us.get().max(elapsed()));
    if left > 0 {
        return true;
    }
    let job = inner.job.take();
    if let Some(mut job) = job {
        job.model.release();
        if let Some(on_done) = job.on_done.take() {
            on_done();
        }
    }
    false
}

/// The positions of a weak row map whose object is still alive (held by
/// GTK or a selection).
pub(crate) fn held_positions<T: IsA<glib::Object>>(
    live: &std::collections::HashMap<u32, glib::WeakRef<T>>,
) -> Vec<u32> {
    live.iter()
        .filter(|(_, row)| row.upgrade().is_some())
        .map(|(position, _)| *position)
        .collect()
}
