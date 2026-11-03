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
//! The handoff runs only when the open added or selected a page in this
//! window, and its decision is the pure `policy::opened_document_takes_focus`:
//! the focus must still be where GTK put it (unset, or on the window's own
//! chrome, never inside a popover or menu), no dialog or transient surface may
//! be up, and the window must not have seen a key or button press since it was
//! mapped (`setup_opened_document_focus`'s latch). The latch keeps any focus a
//! user chose, including a screen-reader user tabbing through the chrome while
//! a session restores.
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

mod policy;

use policy::{OpenedDocumentFocus, OpenedDocumentFocusFacts, opened_document_takes_focus};

/// The name of the two capture-phase controllers that latch user input for
/// the opened-document handoff (`setup_opened_document_focus`).
const USER_INPUT_LATCH_CONTROLLER: &str = "lushtext-user-input-latch";

/// Whether one handoff attempt finished (focused, or decided not to) or the
/// editor could not take focus yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HandoffAttempt {
    Done,
    Retry,
}

/// Delay between focus retries after tab selection or adaptive layout changes.
///
/// Thirty milliseconds keeps retries below perceptible interaction latency while
/// giving GTK a frame to settle newly mapped or reparented editor widgets.
pub(super) const EDITOR_FOCUS_RETRY_INTERVAL: Duration = Duration::from_millis(30);
/// Maximum retry count for editor focus handoffs before giving control back to
/// GTK's normal focus model. Six attempts covers roughly 180ms of settling.
pub(super) const EDITOR_FOCUS_MAX_ATTEMPTS: u8 = 6;

impl LushtextWindow {
    /// Install the user-input latch `opened_document_takes_focus` reads: a key
    /// or button press anywhere in the window, seen in the capture phase before
    /// any child handles it, marks the window touched until it is mapped again.
    /// Neither controller claims or stops anything.
    pub(super) fn setup_opened_document_focus(&self) {
        let keys = gtk4::EventControllerKey::new();
        keys.set_name(Some(USER_INPUT_LATCH_CONTROLLER));
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let window_weak = self.downgrade();
        keys.connect_key_pressed(move |_, _, _, _| {
            if let Some(window) = window_weak.upgrade() {
                window.imp().user_input_since_map.set(true);
            }
            glib::Propagation::Proceed
        });
        self.add_controller(keys);

        let buttons = gtk4::GestureClick::new();
        buttons.set_name(Some(USER_INPUT_LATCH_CONTROLLER));
        buttons.set_button(0);
        buttons.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let window_weak = self.downgrade();
        buttons.connect_pressed(move |_, _, _, _| {
            if let Some(window) = window_weak.upgrade() {
                window.imp().user_input_since_map.set(true);
            }
        });
        self.add_controller(buttons);

        self.connect_map(|window| window.imp().user_input_since_map.set(false));
    }

    /// Give the keyboard to the selected document's editor after an activation
    /// or session restore opened a page in this window, when
    /// `opened_document_takes_focus` allows. The editor is focused in this
    /// dispatch when it can take focus, so a key already queued behind it
    /// lands in the document; otherwise a short retry finishes the job,
    /// re-deciding on every attempt.
    /// While the content stage has not revealed the content yet (a restore
    /// that finished first), the handoff waits for it instead of forcing it,
    /// and decides then.
    pub(super) fn hand_focus_to_opened_document(&self) {
        self.imp().opened_document_handoff_pending.set(true);
        self.after_content_revealed(|window| {
            if window.hand_focus_to_selected_document_now() == HandoffAttempt::Retry {
                window.retry_opened_document_handoff();
            } else {
                window.finish_opened_document_handoff();
            }
        });
    }

    /// Whether the selected document is about to take the keyboard: a
    /// handoff is pending, or a session restore or activation opens that will
    /// request one are still running. An inline alert is announced only after
    /// that (`notifications::execution`): a screen reader interrupts speech on
    /// a focus change, so an announcement before the handoff would be cut off.
    /// The ordering used to hold only because a selected document's install
    /// waited behind its own layout; a view that lays nothing out while its
    /// tab is not yet shown (`defer-background-tab-layout`) installs sooner.
    pub(in crate::ui::window) fn opened_document_focus_pending(&self) -> bool {
        self.imp().opened_document_handoff_pending.get()
            || self.imp().session.restore_runtime.borrow().is_some()
            || self.composition_activation_adds_pages()
    }

    /// The handoff finished (focused, or decided not to): project the
    /// notification surfaces again, so an alert it held back is announced.
    pub(in crate::ui::window) fn finish_opened_document_handoff(&self) {
        self.imp().opened_document_handoff_pending.set(false);
        crate::ui::responsiveness_probe::span("focus_handoff_render", || {
            self.render_notifications();
        });
    }

    fn hand_focus_to_selected_document_now(&self) -> HandoffAttempt {
        let Some(editor) = self.active_editor() else {
            return HandoffAttempt::Done;
        };
        let facts = OpenedDocumentFocusFacts {
            focus: self.opened_document_focus(),
            user_input_since_map: self.imp().user_input_since_map.get(),
            dialog_visible: libadwaita::prelude::AdwApplicationWindowExt::visible_dialog(self)
                .is_some(),
            transient_surface_open: self.transient_surface_open(),
        };
        if !opened_document_takes_focus(facts) {
            return HandoffAttempt::Done;
        }
        // The content is revealed by now (`after_content_revealed`), so this
        // forces nothing; it keeps rule 11's contract for staged focus.
        self.ensure_composed(StagedSurface::Content);
        let focused = crate::ui::responsiveness_probe::span("focus_handoff_grab", || {
            editor.source_view().grab_focus()
        });
        if focused {
            HandoffAttempt::Done
        } else {
            HandoffAttempt::Retry
        }
    }

    /// Retry a handoff whose editor could not take focus yet (not mapped),
    /// deciding again each time, so a key or a focus change in between wins.
    fn retry_opened_document_handoff(&self) {
        let window_weak = self.downgrade();
        let attempts = std::rc::Rc::new(std::cell::Cell::new(0u8));
        glib::timeout_add_local(EDITOR_FOCUS_RETRY_INTERVAL, move || {
            let Some(window) = window_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let next_attempt = attempts.get().saturating_add(1);
            attempts.set(next_attempt);
            if window.hand_focus_to_selected_document_now() == HandoffAttempt::Done
                || next_attempt >= EDITOR_FOCUS_MAX_ATTEMPTS
            {
                window.finish_opened_document_handoff();
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    /// Classify the window's focus widget for `opened_document_takes_focus`.
    /// Only a widget in the window's own surface can be chrome: a menu or
    /// popover opened from the header bar or the tab strip has its own native,
    /// and focus in it is the user's.
    fn opened_document_focus(&self) -> OpenedDocumentFocus {
        let Some(focus) = gtk4::prelude::GtkWindowExt::focus(self) else {
            return OpenedDocumentFocus::Unset;
        };
        let in_window_surface = focus.native().is_some_and(|native| {
            native.upcast_ref::<gtk4::Widget>() == self.upcast_ref::<gtk4::Widget>()
        });
        if !in_window_surface {
            return OpenedDocumentFocus::Elsewhere;
        }
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
