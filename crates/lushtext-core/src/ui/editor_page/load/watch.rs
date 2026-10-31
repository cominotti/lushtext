// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination: the document-load workflow's settle watch
//! (`bound-post-open-settle`, design D1, review R25).
//!
//! GtkSourceView and GtkTextView expose no completion signal for highlighting
//! or line validation, so the workflow watches what they do expose: every
//! `GtkSourceBuffer::highlight-updated` of the editor's buffer and every change
//! of its vertical adjustment's `upper` is a **mark**. A mark arms one
//! `G_PRIORITY_LOW` **sentinel** idle; GTK's validation idles and the context
//! engine's first-update idle run at higher priorities, so the sentinel only
//! dispatches in an iteration where none of them was ready. The background
//! analysis worker shares `G_PRIORITY_LOW` (ledger A54) and emits a mark per
//! slice, which `SETTLE_QUIET_US` covers. The pure predicate over these facts
//! is `policy::settle_stage`; the evidence surface projects it.

use std::cell::Cell;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;

use crate::ui::editor_page::LushtextEditorPage;

/// The settle watch's state on one editor.
#[derive(Default)]
pub struct SettleWatchState {
    /// When the last mark was seen, monotonic µs.
    pub(super) last_mark_us: Cell<Option<i64>>,
    /// When the sentinel last dispatched, monotonic µs.
    pub(super) last_sentinel_us: Cell<Option<i64>>,
    sentinel_armed: Cell<bool>,
    upper_watched: Cell<bool>,
}

impl LushtextEditorPage {
    /// Install the settle watch's handlers (from `constructed`). The
    /// adjustment is watched once the view is first mapped, when its
    /// scrolled window has given it one.
    pub(crate) fn setup_settle_watch(&self) {
        let editor = self.downgrade();
        self.buffer()
            .connect_local("highlight-updated", false, move |_| {
                if let Some(editor) = editor.upgrade() {
                    editor.record_settle_mark();
                }
                None
            });
        let editor = self.downgrade();
        self.imp().source_view.connect_map(move |view| {
            let Some(editor) = editor.upgrade() else {
                return;
            };
            let watch = &editor.imp().load.settle;
            if watch.upper_watched.replace(true) {
                return;
            }
            if let Some(adjustment) = view.vadjustment() {
                let editor = editor.downgrade();
                adjustment.connect_upper_notify(move |_| {
                    if let Some(editor) = editor.upgrade() {
                        editor.record_settle_mark();
                    }
                });
            }
        });
    }

    /// Record one mark and arm the sentinel; post-open stages call it too,
    /// so a stage with no toolkit follow-up still needs a later sentinel.
    pub(crate) fn record_settle_mark(&self) {
        let watch = &self.imp().load.settle;
        watch.last_mark_us.set(Some(glib::monotonic_time()));
        if watch.sentinel_armed.replace(true) {
            return;
        }
        let editor = self.downgrade();
        glib::idle_add_local_full(glib::Priority::LOW, move || {
            if let Some(editor) = editor.upgrade() {
                let watch = &editor.imp().load.settle;
                watch.sentinel_armed.set(false);
                watch.last_sentinel_us.set(Some(glib::monotonic_time()));
            }
            glib::ControlFlow::Break
        });
    }
}
