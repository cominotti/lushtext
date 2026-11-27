// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure half of the opened-document keyboard handoff: whether a document an
//! activation or a session restore opened takes the keyboard.
//!
//! Cross-cutting, like its GTK half (`super`): the decision has two owning
//! workflows, `WFR-WINDOW-COMPOSITION` (after an activation's last open turn)
//! and `WFR-SESSION-RESTORE` (at the restore's terminal), so it stays shared
//! rather than moving beside either. It is named `policy.rs` so the
//! `ui/**/policy.rs` mutation scope reaches it.

/// Where the window's keyboard focus sits when an open selects a document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui::window) enum OpenedDocumentFocus {
    /// No widget has focus.
    Unset,
    /// The window's own chrome — the header bar, the tab strip, or the status
    /// bar — where GTK's initial focus lands. A widget inside a popover that
    /// one of them owns (a menu, the Open popover) is not chrome: it belongs to
    /// another surface (`Elsewhere`).
    Chrome,
    /// Anywhere the user can have put it: an editor, the workspace sidebar,
    /// the properties panel, a search or palette entry, a popover.
    Elsewhere,
}

/// What the window observed when an open selected a document in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui::window) struct OpenedDocumentFocusFacts {
    /// Where the keyboard focus is.
    pub focus: OpenedDocumentFocus,
    /// Whether the window has seen a key or button press since it was
    /// mapped: then any focus it has is the user's, however it looks.
    pub user_input_since_map: bool,
    /// Whether an `AdwDialog` is presented.
    pub dialog_visible: bool,
    /// Whether a dismissible transient surface (the command palette, the
    /// search panel, the search bar, a peek) is up.
    pub transient_surface_open: bool,
}

/// Whether the opened document's editor takes the keyboard: only when the
/// focus is still where GTK put it (unset, or on the window chrome), the user
/// has not pressed a key or a button in the window since it was mapped, and
/// no dialog or transient surface is up. It never takes the keyboard from
/// anything the user chose, including a screen-reader user tabbing through the
/// chrome while a session restores.
pub(in crate::ui::window) const fn opened_document_takes_focus(
    facts: OpenedDocumentFocusFacts,
) -> bool {
    !facts.user_input_since_map
        && !facts.dialog_visible
        && !facts.transient_surface_open
        && matches!(
            facts.focus,
            OpenedDocumentFocus::Unset | OpenedDocumentFocus::Chrome
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOCI: [OpenedDocumentFocus; 3] = [
        OpenedDocumentFocus::Unset,
        OpenedDocumentFocus::Chrome,
        OpenedDocumentFocus::Elsewhere,
    ];

    fn facts(
        focus: OpenedDocumentFocus,
        user_input_since_map: bool,
        dialog_visible: bool,
        transient_surface_open: bool,
    ) -> OpenedDocumentFocusFacts {
        OpenedDocumentFocusFacts {
            focus,
            user_input_since_map,
            dialog_visible,
            transient_surface_open,
        }
    }

    #[test]
    fn untouched_initial_focus_hands_the_keyboard_to_the_document() {
        assert!(opened_document_takes_focus(facts(
            OpenedDocumentFocus::Unset,
            false,
            false,
            false
        )));
        assert!(opened_document_takes_focus(facts(
            OpenedDocumentFocus::Chrome,
            false,
            false,
            false
        )));
    }

    #[test]
    fn a_focus_elsewhere_is_kept() {
        assert!(!opened_document_takes_focus(facts(
            OpenedDocumentFocus::Elsewhere,
            false,
            false,
            false
        )));
    }

    #[test]
    fn user_input_since_map_keeps_any_focus_even_on_the_chrome() {
        for focus in FOCI {
            assert!(
                !opened_document_takes_focus(facts(focus, true, false, false)),
                "{focus:?}"
            );
        }
    }

    #[test]
    fn a_dialog_or_transient_surface_keeps_the_focus() {
        for focus in FOCI {
            assert!(!opened_document_takes_focus(facts(
                focus, false, true, false
            )));
            assert!(!opened_document_takes_focus(facts(
                focus, false, false, true
            )));
        }
    }

    #[test]
    fn the_handoff_happens_for_exactly_one_combination_per_untouched_focus() {
        let mut taken = 0;
        for focus in FOCI {
            for input in [false, true] {
                for dialog in [false, true] {
                    for transient in [false, true] {
                        let expected = !input
                            && !dialog
                            && !transient
                            && focus != OpenedDocumentFocus::Elsewhere;
                        let decided =
                            opened_document_takes_focus(facts(focus, input, dialog, transient));
                        assert_eq!(decided, expected, "{focus:?} {input} {dialog} {transient}");
                        taken += usize::from(decided);
                    }
                }
            }
        }
        assert_eq!(taken, 2);
    }
}
