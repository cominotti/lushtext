// SPDX-License-Identifier: GPL-3.0-or-later

//! Wait for the frame that lays out an editor's last insert
//! (`bound-post-open-settle`, review R14, ledger A55).
//!
//! **Cross-cutting, no role**: the paced install of a dense document
//! (`WFR-DOCUMENT-LOAD`) and the paced whole-buffer replacement
//! (`WFR-BUFFER-REPLACEMENT`) both queue their next slice from the
//! `after-paint` of the frame that validated the previous one, so each
//! validation holds one slice's lines.
//!
//! A window that is not presented (minimized, or on a workspace the
//! compositor does not draw) may receive no frame for as long as it stays
//! hidden. The wait therefore also ends after [`FRAME_WAIT_FALLBACK`], so a
//! slice never waits for a frame that does not come; GtkTextView then
//! validates the inserted lines from its own idle, still one slice at a time.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;

/// How long a slice waits for its frame before it runs anyway: twelve
/// 120 Hz reference frames, far beyond any presented frame.
pub(crate) const FRAME_WAIT_FALLBACK: Duration = Duration::from_millis(100);

#[derive(Default)]
struct Pending {
    paint: Option<(gtk4::gdk::FrameClock, glib::SignalHandlerId)>,
    fallback: Option<glib::SourceId>,
    then: Option<Box<dyn FnOnce()>>,
}

impl Pending {
    /// Disarm both triggers and hand back the continuation, once.
    fn take(&mut self) -> Option<Box<dyn FnOnce()>> {
        if let Some((clock, handler)) = self.paint.take() {
            clock.disconnect(handler);
        }
        if let Some(source) = self.fallback.take() {
            source.remove();
        }
        self.then.take()
    }
}

/// A pending wait; dropping it does not cancel it, [`FrameWait::cancel`] does.
pub(crate) struct FrameWait(Rc<RefCell<Pending>>);

impl FrameWait {
    /// Run `then` once, after the next frame `view` paints or after
    /// [`FRAME_WAIT_FALLBACK`], whichever comes first. `None` when the view is
    /// not mapped or has no frame clock: nothing would lay the slice out, and
    /// the caller schedules its ordinary turn.
    pub(crate) fn after_next_frame(
        view: &gtk4::Widget,
        then: impl FnOnce() + 'static,
    ) -> Option<Self> {
        if !view.is_mapped() {
            return None;
        }
        let clock = view.frame_clock()?;
        let pending = Rc::new(RefCell::new(Pending {
            then: Some(Box::new(then)),
            ..Pending::default()
        }));
        let fire = {
            let pending = Rc::downgrade(&pending);
            move || {
                let then = pending
                    .upgrade()
                    .and_then(|pending| pending.borrow_mut().take());
                if let Some(then) = then {
                    then();
                }
            }
        };
        let on_paint = fire.clone();
        let handler = clock.connect_after_paint(move |_| on_paint());
        let fallback = glib::timeout_add_local_once(FRAME_WAIT_FALLBACK, {
            let pending = Rc::downgrade(&pending);
            move || {
                // This source is the one running: forget it before `take`
                // removes it.
                if let Some(pending) = pending.upgrade() {
                    pending.borrow_mut().fallback = None;
                }
                fire();
            }
        });
        {
            let mut state = pending.borrow_mut();
            state.paint = Some((clock, handler));
            state.fallback = Some(fallback);
        }
        Some(Self(pending))
    }

    /// Disarm the wait without running its continuation.
    pub(crate) fn cancel(self) {
        drop(self.0.borrow_mut().take());
    }
}
