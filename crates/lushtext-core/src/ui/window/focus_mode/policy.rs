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

#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]

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

/// Smallest inner margin kept while focused.
///
/// Twenty-four pixels gives narrow windows visible breathing room without
/// stealing enough width to make wrapped prose feel cramped.
pub const MIN_FOCUS_MARGIN: i32 = 24;

/// Pango units per logical pixel (`pango::SCALE`).
///
/// Restated here because this module imports no toolkit crate; the adapter in
/// `ui/editor_page/focus_mode.rs` asserts it equals `gtk4::pango::SCALE`, so the
/// two cannot drift.
pub const PANGO_UNITS_PER_PIXEL: i32 = 1024;

/// Symmetric readable-column margin for one text surface, in whole pixels.
///
/// `approximate_char_width` is Pango's own integer measure, in Pango units
/// ([`PANGO_UNITS_PER_PIXEL`] per pixel), passed straight through so no
/// fraction of a pixel is ever formed. The margin is half of what the target
/// column (the character width times `target_columns`, clamped to 60–120)
/// leaves of the width, rounded down, then kept between
/// [`MIN_FOCUS_MARGIN`] and a third of the width. A non-positive width or
/// character width yields the minimum.
///
/// Equal, for every `i32` width, `i32` character width, and `u32` column count,
/// to the `f64` form it replaced (which divided the character width by
/// `pango::SCALE` first); the adapter's tests pin the two together.
#[must_use]
pub fn readable_column_margin(
    allocated_width: i32,
    approximate_char_width: i32,
    target_columns: u32,
) -> i32 {
    let width = allocated_width.max(0);
    if width <= 0 || approximate_char_width <= 0 {
        return MIN_FOCUS_MARGIN;
    }

    let scale = i64::from(PANGO_UNITS_PER_PIXEL);
    let target_width = i64::from(approximate_char_width) * i64::from(target_columns.clamp(60, 120));
    // floor((width - target / scale) / 2), all in Pango units.
    let available_margin = (i64::from(width) * scale - target_width).div_euclid(2 * scale);
    // At most width / 2, so it fits back into `i32`.
    let margin = i32::try_from(available_margin.max(i64::from(MIN_FOCUS_MARGIN)))
        .unwrap_or(MIN_FOCUS_MARGIN);
    margin.min(width.saturating_div(3).max(MIN_FOCUS_MARGIN))
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
    fn the_readable_column_is_half_the_leftover_width_rounded_down() {
        // 8 px characters (8192 Pango units) at 80 columns is a 640 px column.
        assert_eq!(readable_column_margin(1_000, 8 * 1024, 80), 180);
        // 180.5 rounds down.
        assert_eq!(readable_column_margin(1_001, 8 * 1024, 80), 180);
        // A fractional character width stays exact: 7.5 px * 80 = 600 px.
        assert_eq!(readable_column_margin(1_000, 7 * 1024 + 512, 80), 200);
    }

    #[test]
    fn the_readable_column_margin_stays_between_its_floor_and_a_third() {
        assert_eq!(readable_column_margin(0, 8 * 1024, 80), MIN_FOCUS_MARGIN);
        assert_eq!(readable_column_margin(-5, 8 * 1024, 80), MIN_FOCUS_MARGIN);
        assert_eq!(readable_column_margin(1_000, 0, 80), MIN_FOCUS_MARGIN);
        assert_eq!(readable_column_margin(1_000, -1, 80), MIN_FOCUS_MARGIN);
        // A column wider than the surface keeps the floor.
        assert_eq!(readable_column_margin(400, 8 * 1024, 80), MIN_FOCUS_MARGIN);
        // 1 px characters at 60 columns would leave 120 px, capped at 300 / 3.
        assert_eq!(readable_column_margin(300, 1024, 60), 100);
        // Column counts clamp to 60..=120.
        assert_eq!(
            readable_column_margin(2_000, 8 * 1024, 10),
            readable_column_margin(2_000, 8 * 1024, 60)
        );
        assert_eq!(
            readable_column_margin(2_000, 8 * 1024, 500),
            readable_column_margin(2_000, 8 * 1024, 120)
        );
    }

    #[test]
    fn chrome_is_hidden_exactly_while_focused() {
        assert!(chrome_visible(false));
        assert!(!chrome_visible(true));
    }
}
