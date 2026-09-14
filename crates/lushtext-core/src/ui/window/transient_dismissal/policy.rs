// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: pure policy — the shell's transient dismissal ladder.
//!
//! This module owns the *order* in which the window shell gives up its transient
//! surfaces, and nothing else. It imports no toolkit crate, which is what keeps
//! it inside the `ui/**/policy.rs` mutation scope: the ladder's order is the one
//! part of this workflow a reader can get wrong without any gate noticing, so it
//! is the part that gets mutation coverage.
//!
//! The adapter observes live widget state, hands it over as one
//! `TransientSurfaceState`, and applies whatever this module decides. No decision
//! here reads or writes a widget.

/// The one transient surface an Escape press gives up.
///
/// Named for the surface rather than for the mechanism that hides it, because the
/// dismissal contract in `.agents/rules/widget-wiring.md` is stated in terms of
/// surfaces: *exactly one topmost visible dismissible surface per Escape*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransientDismissal {
    /// The window-level command palette overlay.
    CommandPalette,
    /// The active editor page's in-editor find bar.
    EditorSearch,
    /// The workspace-wide search panel revealer.
    SearchPanel,
    /// The header bar's primary menu popover.
    PrimaryMenu,
    /// The header bar's notes menu popover.
    NotesMenu,
}

/// The live shell state one dismissal decision reads.
///
/// Seam value object: this bundle crosses the facade → `escape_outcome` →
/// `topmost_dismissible` boundary and is reconstructed by the evidence surface,
/// so it is reified rather than passed as five positional booleans. Five
/// same-typed parameters is precisely the shape where a value can be renamed
/// while crossing a seam — swapping `primary_menu_active` and `notes_menu_active`
/// at a call site would be invisible to review and to every test that only
/// asserts *something* was dismissed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransientSurfaceState {
    /// Whether the palette revealer currently reveals its child.
    pub command_palette_revealed: bool,
    /// Whether the selected editor page shows its find bar.
    pub editor_search_visible: bool,
    /// Whether the workspace search panel revealer reveals its child.
    pub search_panel_revealed: bool,
    /// Whether the primary menu button's popover is up.
    pub primary_menu_active: bool,
    /// Whether the notes menu button's popover is up.
    pub notes_menu_active: bool,
}

/// What one Escape key press resolves to for the observed shell state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeOutcome {
    /// A child widget already consumed this Escape; the shell stops the event
    /// without giving up a surface of its own.
    ChildAlreadyHandled,
    /// Close exactly this surface and stop the event.
    Dismiss(TransientDismissal),
    /// No transient surface is visible, so Escape leaves Focus Mode instead.
    ExitFocusMode,
    /// Nothing for the shell to do; the event keeps propagating.
    Unhandled,
}

impl EscapeOutcome {
    /// Whether the key event should stop at the shell.
    #[must_use]
    pub fn stops_event(self) -> bool {
        !matches!(self, Self::Unhandled)
    }
}

/// The topmost dismissible surface, or `None` when the shell owns none.
///
/// The order is the contract, not an implementation detail: palette first
/// because it sits above everything in the overlay, then in-editor search, then
/// the workspace search panel, then the two header menus. Focus Mode is
/// deliberately absent — it is not a transient surface and is handled only after
/// this ladder declines, so a palette open above Focus Mode takes the first
/// Escape and Focus Mode takes the next.
#[must_use]
pub fn topmost_dismissible(state: TransientSurfaceState) -> Option<TransientDismissal> {
    if state.command_palette_revealed {
        return Some(TransientDismissal::CommandPalette);
    }
    if state.editor_search_visible {
        return Some(TransientDismissal::EditorSearch);
    }
    if state.search_panel_revealed {
        return Some(TransientDismissal::SearchPanel);
    }
    if state.primary_menu_active {
        return Some(TransientDismissal::PrimaryMenu);
    }
    if state.notes_menu_active {
        return Some(TransientDismissal::NotesMenu);
    }
    None
}

/// Resolve one Escape press against the latch, the ladder, and Focus Mode.
///
/// The latch wins first. `GtkSearchEntry::stop-search` is emitted by a child
/// before the window's Bubble controller sees the same key event, so without the
/// latch the shell would close the *next* surface underneath one the child just
/// closed — the cascade the dismissal contract forbids.
#[must_use]
pub fn escape_outcome(
    child_already_handled: bool,
    state: TransientSurfaceState,
    focus_mode_active: bool,
) -> EscapeOutcome {
    if child_already_handled {
        return EscapeOutcome::ChildAlreadyHandled;
    }
    if let Some(surface) = topmost_dismissible(state) {
        return EscapeOutcome::Dismiss(surface);
    }
    if focus_mode_active {
        return EscapeOutcome::ExitFocusMode;
    }
    EscapeOutcome::Unhandled
}

/// Whether a primary pointer press dismisses the command palette.
///
/// A hidden palette ignores the press so an ordinary click never pays for
/// click-away, and an inside press proceeds so the palette's own entry, mode
/// selector, result rows, scrollbars, and child popup roots keep working.
#[must_use]
pub fn palette_click_away_dismisses(palette_revealed: bool, press_inside_palette: bool) -> bool {
    palette_revealed && !press_inside_palette
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with(mutate: impl FnOnce(&mut TransientSurfaceState)) -> TransientSurfaceState {
        let mut state = TransientSurfaceState::default();
        mutate(&mut state);
        state
    }

    #[test]
    fn no_visible_surface_yields_no_dismissal() {
        let state = TransientSurfaceState::default();
        assert_eq!(topmost_dismissible(state), None);
    }

    #[test]
    fn ladder_order_is_palette_then_editor_then_panel_then_menus() {
        // Every surface visible at once: each step of the ladder must be the
        // one that wins, checked by removing them from the top down.
        let mut state = TransientSurfaceState {
            command_palette_revealed: true,
            editor_search_visible: true,
            search_panel_revealed: true,
            primary_menu_active: true,
            notes_menu_active: true,
        };
        assert_eq!(
            topmost_dismissible(state),
            Some(TransientDismissal::CommandPalette)
        );
        state.command_palette_revealed = false;
        assert_eq!(
            topmost_dismissible(state),
            Some(TransientDismissal::EditorSearch)
        );
        state.editor_search_visible = false;
        assert_eq!(
            topmost_dismissible(state),
            Some(TransientDismissal::SearchPanel)
        );
        state.search_panel_revealed = false;
        assert_eq!(
            topmost_dismissible(state),
            Some(TransientDismissal::PrimaryMenu)
        );
        state.primary_menu_active = false;
        assert_eq!(
            topmost_dismissible(state),
            Some(TransientDismissal::NotesMenu)
        );
        state.notes_menu_active = false;
        assert_eq!(topmost_dismissible(state), None);
    }

    #[test]
    fn each_surface_alone_dismisses_itself() {
        for (mutate, expected) in [
            (
                Box::new(|s: &mut TransientSurfaceState| s.command_palette_revealed = true)
                    as Box<dyn FnOnce(&mut TransientSurfaceState)>,
                TransientDismissal::CommandPalette,
            ),
            (
                Box::new(|s: &mut TransientSurfaceState| s.editor_search_visible = true),
                TransientDismissal::EditorSearch,
            ),
            (
                Box::new(|s: &mut TransientSurfaceState| s.search_panel_revealed = true),
                TransientDismissal::SearchPanel,
            ),
            (
                Box::new(|s: &mut TransientSurfaceState| s.primary_menu_active = true),
                TransientDismissal::PrimaryMenu,
            ),
            (
                Box::new(|s: &mut TransientSurfaceState| s.notes_menu_active = true),
                TransientDismissal::NotesMenu,
            ),
        ] {
            let state = state_with(mutate);
            assert_eq!(topmost_dismissible(state), Some(expected));
        }
    }

    #[test]
    fn latch_beats_every_visible_surface_and_focus_mode() {
        let state = state_with(|s| s.command_palette_revealed = true);
        assert_eq!(
            escape_outcome(true, state, true),
            EscapeOutcome::ChildAlreadyHandled
        );
        assert!(escape_outcome(true, state, true).stops_event());
    }

    #[test]
    fn palette_above_focus_mode_takes_the_first_escape() {
        // The contract's exact case: one Escape closes the palette, the next
        // exits Focus Mode. Focus Mode must not exit while a surface is up.
        let with_palette = state_with(|s| s.command_palette_revealed = true);
        assert_eq!(
            escape_outcome(false, with_palette, true),
            EscapeOutcome::Dismiss(TransientDismissal::CommandPalette)
        );
        assert_eq!(
            escape_outcome(false, TransientSurfaceState::default(), true),
            EscapeOutcome::ExitFocusMode
        );
    }

    #[test]
    fn nothing_visible_and_no_focus_mode_leaves_the_event_alone() {
        let outcome = escape_outcome(false, TransientSurfaceState::default(), false);
        assert_eq!(outcome, EscapeOutcome::Unhandled);
        assert!(!outcome.stops_event());
    }

    #[test]
    fn click_away_dismisses_only_outside_presses_on_a_revealed_palette() {
        assert!(palette_click_away_dismisses(true, false));
        assert!(!palette_click_away_dismisses(true, true));
        assert!(!palette_click_away_dismisses(false, false));
        assert!(!palette_click_away_dismisses(false, true));
    }
}
