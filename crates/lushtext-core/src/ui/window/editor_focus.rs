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
//! `hand_focus_to_opened_document` has **two** owning workflows, for the same
//! reason (`investigate-first-keys-without-frames`):
//!
//! | Caller | Owning workflow |
//! | --- | --- |
//! | `composition/execution.rs`, after an activation's last open turn | `WFR-WINDOW-COMPOSITION` |
//! | `session_restore/admission.rs`, at the restore's terminal | `WFR-SESSION-RESTORE` |
//!
//! GTK gives a window its initial focus when it is shown (`gtk_window_show`
//! moves focus forward from nothing), before any document page exists: the
//! first focusable widget is the status bar's sidebar toggle. Without a
//! handoff, a document opened from the command line, a file manager, or the
//! session left the keyboard on that toggle, and the first characters typed
//! after opening it went nowhere.
//!
//! It is the same disposition `.agents/rules/workflow-convention.md` gives
//! cross-cutting pure policy: it stays in a shared location and the matrix
//! names the workflows that share it. Moving it beside any one of them would
//! leave the others reaching across a role home for a helper that is not
//! theirs.
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
use crate::ui::window::StagedSurface;

/// Delay between focus retries after tab selection or adaptive layout changes.
///
/// Thirty milliseconds keeps retries below perceptible interaction latency while
/// giving GTK a frame to settle newly mapped or reparented editor widgets.
pub(super) const EDITOR_FOCUS_RETRY_INTERVAL: Duration = Duration::from_millis(30);
/// Maximum retry count for editor focus handoffs before giving control back to
/// GTK's normal focus model. Six attempts covers roughly 180ms of settling.
pub(super) const EDITOR_FOCUS_MAX_ATTEMPTS: u8 = 6;

/// Where the window's keyboard focus sits when an open selects a document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OpenedDocumentFocus {
    /// No widget has focus.
    Unset,
    /// The window chrome: the header bar, the tab strip, or the status bar,
    /// where GTK's initial focus lands.
    Chrome,
    /// Anywhere the user can have put it: an editor, the workspace sidebar,
    /// the properties panel, a search or palette entry.
    Elsewhere,
}

/// Whether an open that selected a document hands the keyboard to its editor:
/// only while the focus is unset or on the chrome, and no dialog or transient
/// surface is up, so it never takes the keyboard from something the user is
/// working in.
pub(super) const fn opened_document_takes_focus(
    focus: OpenedDocumentFocus,
    dialog_visible: bool,
    transient_surface_open: bool,
) -> bool {
    !dialog_visible
        && !transient_surface_open
        && matches!(
            focus,
            OpenedDocumentFocus::Unset | OpenedDocumentFocus::Chrome
        )
}

impl LushtextWindow {
    /// Give the keyboard to the selected document's editor after an activation
    /// or session restore opened it, when `opened_document_takes_focus` allows.
    /// The editor is focused in this dispatch when it can take focus, so a key
    /// already queued behind it lands in the document; otherwise the retrying
    /// handoff finishes the job.
    /// While the content stage has not revealed the content yet (a restore
    /// that finished first), the handoff waits for it instead of forcing it.
    pub(super) fn hand_focus_to_opened_document(&self) {
        self.after_content_revealed(Self::hand_focus_to_selected_document_now);
    }

    fn hand_focus_to_selected_document_now(&self) {
        let Some(editor) = self.active_editor() else {
            return;
        };
        let decision = opened_document_takes_focus(
            self.opened_document_focus(),
            libadwaita::prelude::AdwApplicationWindowExt::visible_dialog(self).is_some(),
            self.transient_surface_open(),
        );
        if !decision {
            return;
        }
        // The content is revealed by now (`after_content_revealed`), so this
        // forces nothing; it keeps rule 11's contract for staged focus.
        self.ensure_composed(StagedSurface::Content);
        let source_view = editor.source_view();
        if !source_view.grab_focus() {
            self.focus_selected_editor_after_action();
        }
    }

    /// Classify the window's focus widget for `opened_document_takes_focus`.
    fn opened_document_focus(&self) -> OpenedDocumentFocus {
        let Some(focus) = gtk4::prelude::GtkWindowExt::focus(self) else {
            return OpenedDocumentFocus::Unset;
        };
        let imp = self.imp();
        let chrome: [&gtk4::Widget; 3] = [
            imp.header_bar.upcast_ref(),
            imp.tab_bar.upcast_ref(),
            imp.status_bar.upcast_ref(),
        ];
        if chrome
            .iter()
            .any(|region| focus == **region || focus.is_ancestor(*region))
        {
            OpenedDocumentFocus::Chrome
        } else {
            OpenedDocumentFocus::Elsewhere
        }
    }

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

            window.ensure_composed(StagedSurface::Content);
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
