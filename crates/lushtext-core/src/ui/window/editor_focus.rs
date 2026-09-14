// SPDX-License-Identifier: GPL-3.0-or-later

//! Cross-cutting editor focus restoration after a shell action.
//!
//! **This module carries no workflow role**, and that is the recorded
//! conclusion rather than an omission. `focus_selected_editor_after_action` has
//! **three** owning workflows and no single one of them may take it:
//!
//! | Caller | Owning workflow |
//! | --- | --- |
//! | `actions.rs` `win.new-tab` | `WFR-TAB-STRIP` |
//! | `actions.rs` `select_tab_by_index` | `WFR-TAB-STRIP` |
//! | `recent_open.rs`, on recent-row activation | `WFR-RECENT-DOCUMENTS` |
//! | `recent_open.rs`, on keyboard dismissal | `WFR-RECENT-DOCUMENTS` |
//!
//! It is the same disposition `.agents/rules/rust.md` gives cross-cutting pure
//! policy: it stays in a shared location and the matrix names the workflows
//! that share it. Moving it beside any one of them would leave the others
//! reaching across a role home for a helper that is not theirs.
//!
//! It was extracted in slot 7b from `ui/window/focus_indexing.rs`, whose
//! inherited "geometry story" label was wrong in a way worth recording: the
//! block contained **no geometry code**, and re-reading it dissolved the story
//! into three different owners rather than one.

use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::ui::editor_page::LushtextEditorPage;

use super::LushtextWindow;

/// Delay between focus retries after tab selection or adaptive layout changes.
///
/// Thirty milliseconds keeps retries below perceptible interaction latency while
/// giving GTK a frame to settle newly mapped or reparented editor widgets.
pub(super) const EDITOR_FOCUS_RETRY_INTERVAL: Duration = Duration::from_millis(30);
/// Maximum retry count for editor focus handoffs before giving control back to
/// GTK's normal focus model. Six attempts covers roughly 180ms of settling.
pub(super) const EDITOR_FOCUS_MAX_ATTEMPTS: u8 = 6;

impl LushtextWindow {
    /// Move keyboard focus to the editor selected when a shell action runs.
    ///
    /// Command-palette activation restores its saved focus after running the
    /// action, so this schedules the editor handoff for a later main-loop tick
    /// and retries briefly while GTK finishes selecting or mapping the tab.
    pub(super) fn focus_selected_editor_after_action(&self) {
        let Some(page) = self.imp().tab_view.selected_page() else {
            return;
        };
        let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>().cloned() else {
            return;
        };

        let window_weak = self.downgrade();
        let page_weak = page.downgrade();
        let editor_weak = editor.downgrade();
        let attempts = std::rc::Rc::new(std::cell::Cell::new(0u8));

        glib::timeout_add_local(EDITOR_FOCUS_RETRY_INTERVAL, move || {
            let Some(window) = window_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let Some(page) = page_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let Some(editor) = editor_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if window.imp().tab_view.selected_page().as_ref() != Some(&page) {
                return glib::ControlFlow::Break;
            }

            let source_view = editor.source_view();
            let source_ptr = source_view.upcast_ref::<gtk4::Widget>().as_ptr();
            gtk4::prelude::GtkWindowExt::set_focus(
                &window,
                Some(source_view.upcast_ref::<gtk4::Widget>()),
            );
            source_view.grab_focus();

            let focused = gtk4::prelude::GtkWindowExt::focus(&window).map(|widget| widget.as_ptr())
                == Some(source_ptr);
            let next_attempt = attempts.get().saturating_add(1);
            attempts.set(next_attempt);

            if focused || next_attempt >= EDITOR_FOCUS_MAX_ATTEMPTS {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }
}
