// SPDX-License-Identifier: GPL-3.0-or-later

//! Keeps the editor view's line validation out of GTK's snapshot.
//!
//! Cross-cutting toolkit workaround with **no role**: it owns no decision, only
//! the wiring that steers one GTK behaviour (ledger A37) that every editing
//! workflow shares.
//!
//! `GtkTextView` validates the lines an edit, a caret move or a tag change
//! invalidated from an idle (`first_validate_callback`, priority
//! `GTK_PRIORITY_RESIZE - 2`). A frame runs its layout and paint in one
//! dispatch, so the idle cannot run first when the change was made inside the
//! frame (GTK delivers input in its flush-events phase), nor when it was made
//! just before a frame that input started: GDK starts that frame from a
//! source of priority `G_PRIORITY_DEFAULT + 1`, above the idle's (a
//! `GtkSourceSearchContext` scan after a find is such a change).
//! Unless the view itself is allocated in that frame's layout phase,
//! `gtk_text_view_paint` then flushes the validation from inside the
//! snapshot: a line that grew past the viewport widens the horizontal
//! adjustment, a queued scroll moves either adjustment, and the scrollbar —
//! already allocated, and snapshotted after its child — logs
//! `Trying to snapshot GtkGizmo … without a current allocation` for its trough.
//!
//! The fix makes the layout phase allocate the view whenever validation may be
//! pending, because `gtk_text_view_size_allocate` flushes it there, before the
//! scrollbars are allocated:
//!
//! - an edit or caret move queues the view's allocation at once, which also
//!   covers changes made in the frame's update phase;
//! - a tag change (applied, removed, or a size-changing property of a tag)
//!   only marks the view, and the frame queues the allocation:
//!   at `before-paint` for a change made before the frame, after its layout
//!   handlers for one made in the update or layout phase (the frame clock then
//!   runs its layout loop once more). Tags also change *inside* the snapshot
//!   (GtkSourceView highlights the visible region there), where queueing an
//!   allocation would only schedule a needless frame, so the mark waits for
//!   the next frame.
//!
//! What GtkSourceView invalidates from inside its own snapshot cannot be moved
//! out of that frame; the allocation queued for the next frame keeps it from
//! lingering.

use std::cell::Cell;
use std::rc::Rc;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_signals::SignalBag;
use gtk4::prelude::*;

use super::LushtextEditorPage;

/// Per-tab state of the frame-validation workaround.
#[derive(Default)]
pub struct FrameValidationState {
    /// A tag changed since the last frame's `before-paint`. Shared with the
    /// tag handlers, which fire once per highlighted token, so they set it
    /// without reaching the editor.
    tags_changed: Rc<Cell<bool>>,
    /// The `before-paint` and `layout` handlers on the view's current frame
    /// clock.
    clock_signals: SignalBag,
}

impl FrameValidationState {
    /// Disconnect from the frame clock (unrealize and teardown).
    pub(crate) fn clear(&self) {
        self.clock_signals.clear();
        self.tags_changed.set(false);
    }
}

impl LushtextEditorPage {
    /// Make the frame's layout phase validate what an edit, caret move or tag
    /// change invalidated, so GTK never does it inside the snapshot (ledger
    /// A37).
    pub(super) fn setup_frame_validation(&self) {
        let buffer = self.buffer();
        {
            let editor_weak = self.downgrade();
            let handler_id = buffer.connect_changed(move |_| {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.validate_in_next_layout();
                }
            });
            self.imp().editing_buffer_signals.track(&buffer, handler_id);
        }
        {
            let editor_weak = self.downgrade();
            let handler_id = buffer.connect_notify_local(Some("cursor-position"), move |_, _| {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.validate_in_next_layout();
                }
            });
            self.imp().editing_buffer_signals.track(&buffer, handler_id);
        }
        let tags_changed = &self.imp().frame_validation.tags_changed;
        for signal in ["apply-tag", "remove-tag"] {
            let tags_changed = Rc::clone(tags_changed);
            let handler_id = buffer.connect_local(signal, false, move |_| {
                tags_changed.set(true);
                None
            });
            self.imp().editing_buffer_signals.track(&buffer, handler_id);
        }
        {
            // A tag whose properties change size (scale, weight, `invisible`)
            // invalidates every range it covers, like re-applying it.
            let tag_table = buffer.tag_table();
            let tags_changed = Rc::clone(tags_changed);
            let handler_id = tag_table.connect_tag_changed(move |_, _, size_changed| {
                if size_changed {
                    tags_changed.set(true);
                }
            });
            self.imp()
                .editing_buffer_signals
                .track(&tag_table, handler_id);
        }

        let source_view = self.source_view();
        {
            let editor_weak = self.downgrade();
            source_view.connect_realize(move |view| {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.watch_frame_clock(view);
                }
            });
        }
        {
            let editor_weak = self.downgrade();
            source_view.connect_unrealize(move |_| {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.imp().frame_validation.clear();
                }
            });
        }
        if source_view.is_realized() {
            self.watch_frame_clock(source_view);
        }
    }

    /// Consume a tag change at the frame's `before-paint`, and again after
    /// its layout handlers ran (a change made in the update or layout phase),
    /// by queueing the view's allocation. Queued during layout, the
    /// allocation re-runs the frame's layout loop before the paint.
    fn watch_frame_clock(&self, view: &sourceview5::View) {
        let state = &self.imp().frame_validation;
        state.clear();
        let Some(clock) = view.frame_clock() else {
            return;
        };
        // Every realized tab runs these on every frame, so they reach the
        // editor only when a tag changed.
        let consume = {
            let tags_changed = Rc::clone(&state.tags_changed);
            let editor_weak = self.downgrade();
            move || {
                if tags_changed.replace(false)
                    && let Some(editor) = editor_weak.upgrade()
                {
                    editor.validate_in_next_layout();
                }
            }
        };
        let at_before_paint = consume.clone();
        let before_paint = clock.connect_before_paint(move |_| at_before_paint());
        state.clock_signals.track(&clock, before_paint);
        let after_layout = clock.connect_closure(
            "layout",
            true,
            glib::closure_local!(move |_: gtk4::gdk::FrameClock| consume()),
        );
        state.clock_signals.track(&clock, after_layout);
    }

    /// Queue the view's allocation, so the next layout phase flushes its
    /// pending validation.
    fn validate_in_next_layout(&self) {
        self.source_view().queue_allocate();
    }
}
