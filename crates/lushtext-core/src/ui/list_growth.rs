// SPDX-License-Identifier: GPL-3.0-or-later

//! One growth step per frame for a lazy list's staged first publish
//! (`lighten-notes-browser-rows` D5).
//!
//! **Cross-cutting coordination, no role.** `WFR-NOTES-BOOKMARKS` (the
//! browser's first listing of a source) and `WFR-COMMAND-PALETTE` (a category
//! view's first listing) both publish into an empty list in stages; the
//! growth arithmetic is the pure `model::list_budget` (`staged_first_rows`,
//! `next_growth`). Named in both matrix rows.
//!
//! A first publish of 16,384 rows used to realize GTK's whole band (about
//! 205 rows, A1) in the frame that first showed them: 29–36 ms under GL in the
//! notes browser even with measure-free row text. A staged publish lists the
//! first 64 rows, and this driver lists the next step from the list's frame
//! clock, once per frame (a tick callback runs in the frame's update phase,
//! before its layout), until the model lists every row. Rows beyond the
//! viewport appear a frame later, which nobody sees: the rows on screen are
//! all there in the first frame, and only the overlay scrollbar's thumb
//! settles over the next few frames.

use gtk4::glib;
use gtk4::prelude::*;

/// A lazy list model whose staged first publish grows one step at a time.
pub(crate) trait GrowingRows {
    /// List the next step; `false` once every row is listed.
    fn grow(&self) -> bool;
}

/// One running growth; dropping it stops the ticks (the rows listed so far
/// stay listed, the rest are listed by whoever finishes the growth).
pub(crate) struct RowGrowth {
    list_view: glib::WeakRef<gtk4::ListView>,
    tick: Option<gtk4::TickCallbackId>,
    /// Set by the callback when it returned `Break` (GTK removed it then).
    ended: std::rc::Rc<std::cell::Cell<bool>>,
}

impl RowGrowth {
    /// Grow `model` once per frame of `list_view`'s frame clock until every
    /// row is listed. The caller lists everything at once instead when the
    /// list is not realized (no frame clock would ever tick it).
    pub(crate) fn start(model: Box<dyn GrowingRows>, list_view: &gtk4::ListView) -> Self {
        let ended = std::rc::Rc::new(std::cell::Cell::new(false));
        // The first tick runs in the update phase of the frame that first
        // shows the staged rows, before its layout: growing there would make
        // that frame realize a step more. Growth starts in the next frame.
        let shown = std::cell::Cell::new(false);
        let tick = list_view.add_tick_callback({
            let ended = std::rc::Rc::clone(&ended);
            move |_, _| {
                if !shown.replace(true) {
                    return glib::ControlFlow::Continue;
                }
                if model.grow() {
                    glib::ControlFlow::Continue
                } else {
                    ended.set(true);
                    glib::ControlFlow::Break
                }
            }
        });
        Self {
            list_view: list_view.downgrade(),
            tick: Some(tick),
            ended,
        }
    }
}

impl Drop for RowGrowth {
    fn drop(&mut self) {
        // A callback that returned `Break` is already gone, and so is one of
        // a finalized list.
        if let Some(tick) = self.tick.take()
            && !self.ended.get()
            && self.list_view.upgrade().is_some()
        {
            tick.remove();
        }
    }
}
