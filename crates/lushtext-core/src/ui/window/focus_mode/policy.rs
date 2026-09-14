// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: pure policy — what Focus Mode captures on entry and restores on exit.
//!
//! Probed rather than assumed: the census recorded this row as owning no pure
//! policy, and the probe found the workflow's central decision sitting inline in
//! the GTK adapter. Focus Mode is *reversible chrome suppression*, and the whole
//! of "reversible" is the rule below — which of the shell states it borrowed it
//! must hand back, and which it must not. Getting that wrong is silent: exit
//! still looks like it worked, and the user simply loses a preview pane or gets
//! dropped out of a fullscreen they had chosen themselves.
//!
//! This module imports no toolkit crate, which is what keeps it inside the
//! `ui/**/policy.rs` mutation scope.

/// Pointer distance from the top edge that reveals the Focus Mode affordance.
///
/// Forty-eight pixels is large enough to hit deliberately while staying above
/// the centered prose column during ordinary typing.
pub const TOP_EDGE_REVEAL_HEIGHT: f64 = 48.0;

/// The shell state Focus Mode borrowed when it was entered.
///
/// Seam value object. These two flags are captured in `enter`, live across the
/// whole focused session, and are consumed in `exit` — two function boundaries,
/// with the pure decision below as a third consumer. They were three separate
/// same-typed `Cell<bool>`s on the `imp` state group, which is the shape where a
/// value can be renamed while crossing a seam: reading
/// `restore_side_by_side_preview` where `was_fullscreen_on_entry` was meant
/// compiles, passes every test that only asserts Focus Mode toggled, and loses
/// the user's window state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FocusModeEntry {
    /// Whether the window was **already** fullscreen when Focus Mode began.
    ///
    /// Focus Mode takes fullscreen if it does not already have it, so this
    /// records whether the fullscreen is *ours to give back*.
    pub was_fullscreen: bool,
    /// Whether side-by-side Markdown preview was visible when Focus Mode began.
    pub had_side_by_side_preview: bool,
}

/// What exit must hand back to the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusModeRestore {
    /// Restore the side-by-side preview pane.
    pub side_by_side_preview: bool,
    /// Leave fullscreen, because Focus Mode is the one that took it.
    pub leave_fullscreen: bool,
}

/// Decide what one Focus Mode exit restores.
///
/// Two rules, each with a user-visible failure if inverted:
///
/// * The preview pane comes back only if Focus Mode hid it **and** the user did
///   not make an explicit preview choice while focused. An explicit choice wins:
///   restoring over it would undo something the user just asked for.
/// * Fullscreen is dropped only if Focus Mode took it. A user who was already
///   fullscreen before entering keeps it, because it was never ours.
#[must_use]
pub fn exit_restoration(
    entry: FocusModeEntry,
    preview_changed_while_focused: bool,
) -> FocusModeRestore {
    FocusModeRestore {
        side_by_side_preview: entry.had_side_by_side_preview && !preview_changed_while_focused,
        leave_fullscreen: !entry.was_fullscreen,
    }
}

/// Whether a pointer at this distance from the top edge reveals the affordance.
///
/// Only meaningful while Focus Mode is active; the caller gates on that, and this
/// takes it as an argument so the whole predicate is one testable decision rather
/// than a bare comparison guarded elsewhere.
#[must_use]
pub fn pointer_reveals_affordance(focus_mode_active: bool, pointer_y: f64) -> bool {
    focus_mode_active && pointer_y <= TOP_EDGE_REVEAL_HEIGHT
}

/// Whether the timed affordance hide should actually hide it.
///
/// The timer fires unconditionally; the affordance stays if keyboard focus is
/// inside it, so a user tabbing into the leave button does not have it vanish
/// mid-interaction.
#[must_use]
pub fn timed_hide_hides_affordance(affordance_contains_focus: bool) -> bool {
    !affordance_contains_focus
}

/// Whether the Markdown preview should take Focus Mode's readable-column margins.
///
/// Only preview-**only** mode gets them. Side-by-side preview keeps its ordinary
/// padding, because a narrowed column inside an already-narrow pane reads as a
/// rendering bug rather than as a reading aid.
#[must_use]
pub fn preview_takes_readable_column(focus_mode_active: bool, preview_only_mode: bool) -> bool {
    focus_mode_active && preview_only_mode
}

/// Whether persistent shell chrome is visible for this Focus Mode state.
#[must_use]
pub fn chrome_visible(focus_mode_active: bool) -> bool {
    !focus_mode_active
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_restores_a_preview_pane_focus_mode_hid() {
        let entry = FocusModeEntry {
            was_fullscreen: false,
            had_side_by_side_preview: true,
        };
        assert_eq!(
            exit_restoration(entry, false),
            FocusModeRestore {
                side_by_side_preview: true,
                leave_fullscreen: true,
            }
        );
    }

    #[test]
    fn an_explicit_preview_choice_while_focused_beats_restoration() {
        // The user chose a preview state during the focused session. Exit must
        // not undo it by restoring what was there before.
        let entry = FocusModeEntry {
            was_fullscreen: false,
            had_side_by_side_preview: true,
        };
        assert!(!exit_restoration(entry, true).side_by_side_preview);
    }

    #[test]
    fn no_preview_on_entry_means_none_to_restore() {
        let entry = FocusModeEntry {
            was_fullscreen: false,
            had_side_by_side_preview: false,
        };
        assert!(!exit_restoration(entry, false).side_by_side_preview);
        assert!(!exit_restoration(entry, true).side_by_side_preview);
    }

    #[test]
    fn fullscreen_the_user_already_had_is_not_taken_away() {
        for preview_changed in [false, true] {
            let entry = FocusModeEntry {
                was_fullscreen: true,
                had_side_by_side_preview: false,
            };
            assert!(
                !exit_restoration(entry, preview_changed).leave_fullscreen,
                "a fullscreen Focus Mode did not take is not Focus Mode's to drop"
            );
        }
    }

    #[test]
    fn fullscreen_focus_mode_took_is_given_back() {
        let entry = FocusModeEntry {
            was_fullscreen: false,
            had_side_by_side_preview: false,
        };
        assert!(exit_restoration(entry, false).leave_fullscreen);
    }

    #[test]
    fn the_two_restoration_rules_are_independent() {
        // Every combination, because the two flags are same-typed and adjacent:
        // a swap between them would satisfy any test that varied only one.
        for was_fullscreen in [false, true] {
            for had_preview in [false, true] {
                for changed in [false, true] {
                    let entry = FocusModeEntry {
                        was_fullscreen,
                        had_side_by_side_preview: had_preview,
                    };
                    let restore = exit_restoration(entry, changed);
                    assert_eq!(restore.leave_fullscreen, !was_fullscreen);
                    assert_eq!(restore.side_by_side_preview, had_preview && !changed);
                }
            }
        }
    }

    #[test]
    fn the_reveal_band_is_inclusive_and_gated_on_focus_mode() {
        assert!(pointer_reveals_affordance(true, 0.0));
        assert!(pointer_reveals_affordance(true, TOP_EDGE_REVEAL_HEIGHT));
        assert!(!pointer_reveals_affordance(
            true,
            TOP_EDGE_REVEAL_HEIGHT + 0.5
        ));
        assert!(
            !pointer_reveals_affordance(false, 0.0),
            "the band must not reveal anything outside Focus Mode"
        );
    }

    #[test]
    fn the_timed_hide_defers_to_keyboard_focus() {
        assert!(timed_hide_hides_affordance(false));
        assert!(!timed_hide_hides_affordance(true));
    }

    #[test]
    fn only_preview_only_mode_takes_the_readable_column() {
        assert!(preview_takes_readable_column(true, true));
        assert!(!preview_takes_readable_column(true, false));
        assert!(!preview_takes_readable_column(false, true));
        assert!(!preview_takes_readable_column(false, false));
    }

    #[test]
    fn chrome_is_hidden_exactly_while_focused() {
        assert!(chrome_visible(false));
        assert!(!chrome_visible(true));
    }
}
