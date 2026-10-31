// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination (`capture_execution`, the edit-entry source capture of
//! the Protected View workflow; design D4).
//!
//! A session entered by an edit has no install body to keep, so its text
//! source is captured from the buffer once, after the entering emission
//! returns. The shared chunked snapshot cannot do it: it resumes from a mark,
//! and positioning an iterator at a mark or a character offset deep inside one
//! giant character segment costs time in proportion to the depth (ledger
//! A66), which makes a capture of one giant paragraph quadratic. This capture
//! keeps **one iterator** across its dispatches, advanced with `forward_chars`
//! in steps below GtkTextIter's linear-scan limit, which A66 measured at a
//! depth-independent cost per chunk. The iterator stays valid only while the
//! buffer's segments do not change; Protected View admits no text change, and
//! any mark or tag change between two dispatches (the view moves its own marks
//! when it scrolls) is observed through the buffer's signals and answered by
//! re-positioning by line and byte index, which pays one depth-proportional
//! character-offset derivation for that dispatch only.
//!
//! The chunks are joined and owned for off-GTK disposal on a worker, under a
//! disposal reservation taken on GTK before the join (retried when disposal
//! capacity frees). The completion keeps the body only while the session is
//! still protected at the entry's generation.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;

use super::policy::{SOURCE_CAPTURE_CHUNK_CHARS, SOURCE_CAPTURE_STEP_CHARS};
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::plain_disposal::{
    DisposalCapacityWakeup, DisposalOwned, disposal_capacity_epoch, try_reserve_for_gtk,
};

/// One running capture.
struct SourceCapture {
    generation: u64,
    /// Where the next chunk starts: valid while `segments_changed` is false.
    iter: gtk4::TextIter,
    /// The same position as line and byte index, to re-position from.
    line: i32,
    line_index: i32,
    chunks: Vec<String>,
    bytes: u64,
    segments_changed: Rc<Cell<bool>>,
    handlers: Vec<glib::SignalHandlerId>,
    source: Option<glib::SourceId>,
    capacity: DisposalCapacityWakeup,
}

/// The editor's edit-entry capture slot.
#[derive(Default)]
pub(crate) struct SourceCaptureState {
    running: RefCell<Option<SourceCapture>>,
}

impl std::fmt::Debug for SourceCaptureState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceCaptureState")
            .field("running", &self.running.borrow().is_some())
            .finish()
    }
}

impl LushtextEditorPage {
    /// Start the capture for the edit-entered session at `generation`, after
    /// the entering emission returns.
    pub(in crate::ui::editor_page) fn start_protected_source_capture(&self, generation: u64) {
        self.cancel_protected_source_capture();
        let page = self.downgrade();
        let start = glib::idle_add_local_once(move || {
            if let Some(page) = page.upgrade() {
                page.begin_protected_source_capture(generation);
            }
        });
        let buffer = self.buffer();
        let segments_changed = Rc::new(Cell::new(false));
        let handlers = watch_segment_changes(buffer.upcast_ref(), &segments_changed);
        self.capture_state().running.replace(Some(SourceCapture {
            generation,
            iter: buffer.start_iter(),
            line: 0,
            line_index: 0,
            chunks: Vec::new(),
            bytes: 0,
            // The entry applied its fold tags after `start_iter`: position on
            // the first turn.
            segments_changed: {
                segments_changed.set(true);
                segments_changed
            },
            handlers,
            source: Some(start),
            capacity: DisposalCapacityWakeup::default(),
        }));
    }

    /// Drop a running capture (the entry undo, a new install, eviction).
    pub(in crate::ui::editor_page) fn cancel_protected_source_capture(&self) {
        let Some(capture) = self.capture_state().running.take() else {
            return;
        };
        let buffer = self.buffer();
        for handler in capture.handlers {
            buffer.disconnect(handler);
        }
        if let Some(source) = capture.source {
            source.remove();
        }
        capture.capacity.cancel();
        let chunks = capture.chunks;
        if !chunks.is_empty() {
            // Captured text is plain data: free it off the GTK thread.
            std::thread::spawn(move || drop(chunks));
        }
    }

    /// Cancel from inside the capture's own dispatch, whose source ends by
    /// returning `Break` and must not be removed twice.
    fn cancel_capture_from_its_dispatch(&self) {
        if let Some(capture) = self.capture_state().running.borrow_mut().as_mut() {
            capture.source = None;
        }
        self.cancel_protected_source_capture();
    }

    fn begin_protected_source_capture(&self, generation: u64) {
        let page = self.downgrade();
        let source = glib::idle_add_local(move || {
            let Some(page) = page.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if page.capture_one_chunk(generation) {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
        match self.capture_state().running.borrow_mut().as_mut() {
            Some(capture) if capture.generation == generation => capture.source = Some(source),
            _ => source.remove(),
        }
    }

    /// One dispatch: copy at most `SOURCE_CAPTURE_CHUNK_CHARS` characters.
    /// Returns whether another dispatch is needed.
    fn capture_one_chunk(&self, generation: u64) -> bool {
        let session = self.protected_session();
        if !session.protected || session.generation != generation {
            self.cancel_capture_from_its_dispatch();
            return false;
        }
        let buffer = self.buffer();
        let finished = {
            let mut running = self.capture_state().running.borrow_mut();
            let Some(capture) = running.as_mut() else {
                return false;
            };
            if capture.segments_changed.replace(false) {
                let Some(iter) = buffer.iter_at_line_index(capture.line, capture.line_index) else {
                    drop(running);
                    self.cancel_capture_from_its_dispatch();
                    return false;
                };
                capture.iter = iter;
            }
            let start = capture.iter;
            let mut end = start;
            let mut copied = 0;
            while copied < SOURCE_CAPTURE_CHUNK_CHARS
                && end.forward_chars(SOURCE_CAPTURE_STEP_CHARS)
            {
                copied += SOURCE_CAPTURE_STEP_CHARS;
            }
            let chunk = buffer.text(&start, &end, true).to_string();
            capture.bytes = capture
                .bytes
                .saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
            capture.chunks.push(chunk);
            capture.iter = end;
            capture.line = end.line();
            capture.line_index = end.line_index();
            if end.is_end() {
                capture.source = None;
                true
            } else {
                false
            }
        };
        if finished {
            self.finish_protected_source_capture(generation);
        }
        !finished
    }

    /// Reserve the body's off-GTK disposal, join the chunks on a worker, and
    /// keep the result while the session holds.
    fn finish_protected_source_capture(&self, generation: u64) {
        let weight = match self.capture_state().running.borrow().as_ref() {
            Some(capture) if capture.generation == generation => capture.bytes,
            _ => return,
        };
        let Some(reservation) = try_reserve_for_gtk(weight) else {
            let epoch = disposal_capacity_epoch();
            let page = self.downgrade();
            if let Some(capture) = self.capture_state().running.borrow().as_ref() {
                capture.capacity.arm(epoch, move || {
                    if let Some(page) = page.upgrade() {
                        page.finish_protected_source_capture(generation);
                    }
                });
            }
            return;
        };
        let Some(capture) = self.capture_state().running.take() else {
            return;
        };
        let buffer = self.buffer();
        for handler in capture.handlers {
            buffer.disconnect(handler);
        }
        let chunks = capture.chunks;
        gtk_lush_tasks::spawn_blocking_then(
            self.clone(),
            move || reservation.own(chunks.concat()),
            move |page, body: DisposalOwned<String>| {
                page.accept_captured_source(generation, body);
            },
        );
    }

    fn capture_state(&self) -> &SourceCaptureState {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        &self.imp().protected_source_capture
    }
}

/// Mark `changed` on every buffer change that moves segments: a mark set,
/// moved or deleted, or a tag applied or removed.
fn watch_segment_changes(
    buffer: &gtk4::TextBuffer,
    changed: &Rc<Cell<bool>>,
) -> Vec<glib::SignalHandlerId> {
    let on_mark_set = Rc::clone(changed);
    let on_mark_deleted = Rc::clone(changed);
    let on_apply = Rc::clone(changed);
    let on_remove = Rc::clone(changed);
    vec![
        buffer.connect_mark_set(move |_, _, _| on_mark_set.set(true)),
        buffer.connect_mark_deleted(move |_, _| on_mark_deleted.set(true)),
        buffer.connect_apply_tag(move |_, _, _, _| on_apply.set(true)),
        buffer.connect_remove_tag(move |_, _, _, _| on_remove.set(true)),
    ]
}
