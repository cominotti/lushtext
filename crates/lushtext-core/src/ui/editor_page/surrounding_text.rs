// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded surrounding text for the editor's input method
//! (`explore-editing-long-paragraphs`, design D6).
//!
//! Cross-cutting with **no role**: it owns no decision but the window, which
//! is `model::surrounding_text::surrounding_window`.
//!
//! GNOME delivers keys to GTK 4 through the Wayland text-input protocol, and
//! GTK's text-input module asks the text view for its surrounding text on
//! every committed character. `GtkTextView`'s own answer walks three words
//! back and forward, which computes the **whole paragraph's** log attributes
//! (ledger A26), and slices from the paragraph start; measured on the
//! reference lane, that is about five whole-paragraph log-attribute passes per
//! keystroke (13 ms at 50,000 characters). The protocol carries at most
//! 4,000 bytes anyway, centred on the cursor, so the editor answers instead
//! with a window of at most `SURROUNDING_WINDOW_CHARS` characters on each side
//! of the cursor, within its paragraph.
//!
//! GTK offers no way to replace the handler: it is connected to the view's
//! private input-method context, which the view's key controller exposes
//! (`gtk_event_controller_key_get_im_context`), and a handler connected later
//! never runs because the view's returns `TRUE` first. So the view's own
//! handler is **blocked** — matched by signal and by its data, the view — and
//! the editor connects its own. Ledger A54 pins the view's answer and why a
//! later handler never runs; the widget test
//! `test_surrounding_text_is_a_bounded_window_around_the_cursor` pins the
//! match (the answer would be the whole paragraph if it found nothing). If no
//! handler matches, the view keeps GTK's behaviour.

use glib::prelude::ObjectType;
use glib::translate::{IntoGlib, ToGlibPtr};
use gtk4::prelude::*;

use super::LushtextEditorPage;
use crate::model::surrounding_text::{SurroundingWindow, surrounding_window};

/// Block the text view's own `retrieve-surrounding` handler on `im`, matched
/// by its data (the view). Returns how many handlers were blocked.
fn block_view_handler(im: &gtk4::IMContext, view: &gtk4::TextView) -> u32 {
    // SAFETY: both pointers come from live, typed GObject wrappers held for
    // the duration of the call; the signal name is a valid NUL-terminated
    // string, and the match only blocks handlers (it frees nothing).
    unsafe {
        let signal = glib::gobject_ffi::g_signal_lookup(
            c"retrieve-surrounding".as_ptr(),
            gtk4::IMContext::static_type().into_glib(),
        );
        if signal == 0 {
            return 0;
        }
        let view_ptr: *mut gtk4::ffi::GtkTextView = view.to_glib_none().0;
        glib::gobject_ffi::g_signal_handlers_block_matched(
            im.as_ptr().cast(),
            glib::gobject_ffi::G_SIGNAL_MATCH_ID | glib::gobject_ffi::G_SIGNAL_MATCH_DATA,
            signal,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            view_ptr.cast(),
        )
    }
}

/// The view's input-method context, from its key controller.
fn view_im_context(view: &gtk4::TextView) -> Option<gtk4::IMContext> {
    let controllers = view.observe_controllers();
    (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .filter_map(|item| item.downcast::<gtk4::EventControllerKey>().ok())
        .find_map(|key| key.im_context())
}

/// The bounded window around the cursor in `buffer`, as GTK expects it: the
/// text, and the cursor and anchor byte indices into it.
fn bounded_surrounding(buffer: &gtk4::TextBuffer) -> (String, i32, i32) {
    let insert = buffer.iter_at_mark(&buffer.get_insert());
    let anchor = buffer.iter_at_mark(&buffer.selection_bound());
    let line_start = {
        let mut start = insert;
        start.set_line_offset(0);
        start
    };
    let line_end = {
        let mut end = insert;
        if !end.ends_line() {
            end.forward_to_line_end();
        }
        end
    };
    let SurroundingWindow { start, end, anchor } = surrounding_window(
        line_start.offset(),
        insert.offset(),
        anchor.offset(),
        line_end.offset(),
    );
    let start = buffer.iter_at_offset(start);
    let end = buffer.iter_at_offset(end);
    let anchor_iter = buffer.iter_at_offset(anchor);
    // Hidden text included, as GTK's own answer (`gtk_text_iter_get_slice`)
    // does, so the input method's `delete-surrounding` offsets count the
    // same characters the buffer does.
    let before = buffer.slice(&start, &insert, true);
    let cursor = i32::try_from(before.len()).unwrap_or(i32::MAX);
    let anchor = i32::try_from(buffer.slice(&start, &anchor_iter, true).len()).unwrap_or(cursor);
    (buffer.slice(&start, &end, true).to_string(), cursor, anchor)
}

impl LushtextEditorPage {
    /// Answer the input method's surrounding-text requests with a bounded
    /// window (from `constructed`). When the view's own handler cannot be
    /// found, GTK's behaviour stays and a warning says so.
    pub(super) fn setup_bounded_surrounding_text(&self) {
        let view: gtk4::TextView = self.source_view().clone().upcast();
        let Some(im) = view_im_context(&view) else {
            tracing::warn!(
                "editor input method context not found; surrounding text stays unbounded"
            );
            return;
        };
        if block_view_handler(&im, &view) == 0 {
            tracing::warn!(
                "text view surrounding-text handler not found; surrounding text stays unbounded"
            );
            return;
        }
        let view_weak = view.downgrade();
        im.connect_retrieve_surrounding(move |im| {
            let Some(view) = view_weak.upgrade() else {
                return false;
            };
            let (text, cursor, anchor) = bounded_surrounding(&view.buffer());
            im.set_surrounding_with_selection(&text, cursor, anchor);
            true
        });
    }
}
