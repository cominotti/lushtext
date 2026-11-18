// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: pure policy for `WFR-TAB-OVERFLOW-NAVIGATION`.
//!
//! Every decision the tab-strip overflow controls make, over whole pixels and
//! plain flags: whether the unpinned tabs overflow, whether the scroll buttons
//! show (with the hysteresis band and the frozen-close hold of design D2),
//! which direction may still move, where one step lands, whether the end
//! group is attached at all, and where focus goes when a focused control
//! disables. The GTK adapter rounds each adjustment value to a whole pixel
//! once (`StripScrollExtent`); the 200 ms animation that interpolates the
//! fractional adjustment value stays in `scroll_execution.rs`.
//!
//! This module imports no toolkit crate, which keeps it inside the
//! `ui/**/policy.rs` mutation scope.
#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]

/// The accessible name and tooltip of the button that scrolls toward the first
/// tab.
pub const SCROLL_BACK_LABEL: &str = "Scroll Tabs Back";
/// The accessible name and tooltip of the button that scrolls toward the last
/// tab.
pub const SCROLL_FORWARD_LABEL: &str = "Scroll Tabs Forward";

/// How long one scroll step animates, matching Libadwaita's own tab-strip
/// scroll (`SCROLL_ANIMATION_DURATION`, `adw-tab-box.c`).
///
/// Kind: out: time; an animation length, matching the toolkit's.
pub const TAB_SCROLL_ANIMATION_MS: u32 = 200;
/// How long a scroll button must be held before it starts repeating.
///
/// Kind: out: time; a press-and-hold delay.
pub const TAB_SCROLL_REPEAT_DELAY_MS: u32 = 400;
/// How often a held scroll button repeats its step.
///
/// Kind: out: time; a repeat cadence, one animation length per step.
pub const TAB_SCROLL_REPEAT_INTERVAL_MS: u32 = 200;

/// The unpinned tab scroller's horizontal adjustment in whole pixels, each
/// value rounded once by the adapter. Its lower bound is always 0
/// (`gtk_adjustment_configure` in the tab box's allocation, ledger A132).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StripScrollExtent {
    /// The adjustment's value.
    pub value_px: i32,
    /// The visible width of the unpinned tabs.
    pub page_px: i32,
    /// The total width of the unpinned tabs.
    pub upper_px: i32,
}

impl StripScrollExtent {
    /// The largest value the adjustment can take.
    #[must_use]
    pub const fn max_value(self) -> i32 {
        let max = self.upper_px.saturating_sub(self.page_px);
        if max > 0 { max } else { 0 }
    }
}

/// What the reconcile reads from the bar before deciding whether the scroll
/// buttons show.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OverflowFacts {
    /// Whether the tab strip itself is shown (tabs exist, not Focus Mode).
    pub strip_shown: bool,
    /// Whether the unpinned adjustment overflows ([`unpinned_overflows`]).
    pub unpinned_overflows: bool,
    /// `AdwTabBar:is-overflowing`, which a close animation's resize freeze
    /// holds true (ledger A133, design F4).
    pub bar_overflowing: bool,
    /// Whether the pinned adjustment overflows.
    pub pinned_overflows: bool,
}

/// Whether the unpinned tabs are wider than their visible width by at least
/// one pixel.
#[must_use]
pub const fn unpinned_overflows(extent: StripScrollExtent) -> bool {
    extent.max_value() >= 1
}

/// Whether the scroll buttons show, given whether they show now.
///
/// Hidden, they show once the unpinned tabs overflow. Shown, they hide once
/// the unpinned tabs fit **beside them**, which is a wider strip than the one
/// they appeared in: that difference is the hysteresis band, so at a fixed
/// width and tab set the state never flips. While a closed tab animates,
/// `is-overflowing` stays true with neither adjustment overflowing (the
/// frozen hold), and a hide waits for it. A hidden strip shows nothing.
#[must_use]
pub const fn controls_visible(currently_visible: bool, facts: OverflowFacts) -> bool {
    if !facts.strip_shown {
        return false;
    }
    if facts.unpinned_overflows {
        return true;
    }
    let frozen_hold = facts.bar_overflowing && !facts.pinned_overflows;
    currently_visible && frozen_hold
}

/// Which way a step moves, in reading order: Back is toward the first tab.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollDirection {
    /// Toward the first tab.
    Back,
    /// Toward the last tab.
    Forward,
}

/// Whether the adjustment value moves up (toward `upper`) for `direction`.
/// In right-to-left the first tab sits at the upper end (ledger A132).
const fn moves_up(direction: ScrollDirection, rtl: bool) -> bool {
    matches!(
        (direction, rtl),
        (ScrollDirection::Forward, false) | (ScrollDirection::Back, true)
    )
}

const fn can_move(extent: StripScrollExtent, up: bool) -> bool {
    if up {
        extent.value_px < extent.max_value()
    } else {
        extent.value_px > 0
    }
}

/// Whether Scroll Tabs Back can still move the strip.
#[must_use]
pub const fn can_scroll_back(extent: StripScrollExtent, rtl: bool) -> bool {
    can_move(extent, moves_up(ScrollDirection::Back, rtl))
}

/// Whether Scroll Tabs Forward can still move the strip.
#[must_use]
pub const fn can_scroll_forward(extent: StripScrollExtent, rtl: bool) -> bool {
    can_move(extent, moves_up(ScrollDirection::Forward, rtl))
}

/// Where one step from `extent.value_px` lands: `increment_px` (the
/// adjustment's own page increment, 0.9 of the visible width, ledger A132)
/// in `direction`, clamped to `[0, upper - page]`.
#[must_use]
pub const fn next_scroll_target(
    extent: StripScrollExtent,
    increment_px: i32,
    direction: ScrollDirection,
    rtl: bool,
) -> i32 {
    let step = if increment_px > 0 { increment_px } else { 0 };
    let target = if moves_up(direction, rtl) {
        extent.value_px.saturating_add(step)
    } else {
        extent.value_px.saturating_sub(step)
    };
    let max = extent.max_value();
    if target < 0 {
        0
    } else if target > max {
        max
    } else {
        target
    }
}

/// Whether the All Tabs button shows: two or more tabs on a shown strip.
#[must_use]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the All Tabs half of the change (task 3.3) consumes it"
    )
)]
pub const fn all_tabs_button_visible(tab_count: u32, strip_shown: bool) -> bool {
    strip_shown && tab_count >= 2
}

/// Whether the end group is attached to the bar at all. With nothing to show
/// it is detached (`set_end_action_widget(None)`): a hidden group would still
/// leave the bar's end action bin and its padding visible (design F11).
#[must_use]
pub const fn group_attached(controls_visible: bool, all_tabs_visible: bool) -> bool {
    controls_visible || all_tabs_visible
}

/// Whether the scroll buttons can exist at all: only once the unpinned tab
/// scroller was found (design D3, fail closed).
#[must_use]
pub const fn scroll_controls_available(scroller_found: bool) -> bool {
    scroller_found
}

/// One control of the end group.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverflowControl {
    /// Scroll Tabs Back.
    Back,
    /// Scroll Tabs Forward.
    Forward,
    /// The All Tabs button.
    AllTabs,
}

/// Which controls can take focus when a focused one disables or hides.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlFocusFacts {
    /// Back is shown and enabled.
    pub back_available: bool,
    /// Forward is shown and enabled.
    pub forward_available: bool,
    /// All Tabs is shown.
    pub all_tabs_available: bool,
}

impl ControlFocusFacts {
    /// The three availabilities, in group order.
    #[must_use]
    pub const fn new(
        back_available: bool,
        forward_available: bool,
        all_tabs_available: bool,
    ) -> Self {
        Self {
            back_available,
            forward_available,
            all_tabs_available,
        }
    }
}

/// Where focus goes from a control that can no longer hold it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocusHandoff {
    /// Another control of the group.
    Control(OverflowControl),
    /// The selected editor.
    Editor,
}

/// Where focus goes when `focused` disables or hides: the other scroll
/// button if it is available, else All Tabs, else the selected editor.
#[must_use]
pub const fn focus_after_disable(
    focused: OverflowControl,
    facts: ControlFocusFacts,
) -> FocusHandoff {
    let other_scroll = match focused {
        OverflowControl::Back => {
            if facts.forward_available {
                Some(OverflowControl::Forward)
            } else {
                None
            }
        }
        OverflowControl::Forward => {
            if facts.back_available {
                Some(OverflowControl::Back)
            } else {
                None
            }
        }
        OverflowControl::AllTabs => {
            if facts.back_available {
                Some(OverflowControl::Back)
            } else if facts.forward_available {
                Some(OverflowControl::Forward)
            } else {
                None
            }
        }
    };
    if let Some(control) = other_scroll {
        return FocusHandoff::Control(control);
    }
    if facts.all_tabs_available && !matches!(focused, OverflowControl::AllTabs) {
        return FocusHandoff::Control(OverflowControl::AllTabs);
    }
    FocusHandoff::Editor
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn extent(value_px: i32, page_px: i32, upper_px: i32) -> StripScrollExtent {
        StripScrollExtent {
            value_px,
            page_px,
            upper_px,
        }
    }

    #[test]
    fn overflow_needs_at_least_one_pixel_beyond_the_page() {
        assert!(!unpinned_overflows(extent(0, 600, 600)));
        assert!(unpinned_overflows(extent(0, 600, 601)));
        assert!(!unpinned_overflows(extent(0, 0, 0)));
    }

    #[test]
    fn controls_show_on_overflow_and_hide_only_once_the_tabs_fit_beside_them() {
        let fitting = OverflowFacts {
            strip_shown: true,
            unpinned_overflows: false,
            bar_overflowing: false,
            pinned_overflows: false,
        };
        let overflowing = OverflowFacts {
            unpinned_overflows: true,
            bar_overflowing: true,
            ..fitting
        };
        assert!(!controls_visible(false, fitting));
        assert!(controls_visible(false, overflowing));
        assert!(controls_visible(true, overflowing));
        assert!(!controls_visible(true, fitting));
    }

    #[test]
    fn the_band_between_the_two_widths_keeps_either_state() {
        // Hidden: the tabs fit without the buttons, so nothing shows.
        let fits_without = OverflowFacts {
            strip_shown: true,
            unpinned_overflows: false,
            bar_overflowing: false,
            pinned_overflows: false,
        };
        assert!(!controls_visible(false, fits_without));
        // Shown: the same tabs overflow the narrower strip beside the buttons.
        let overflows_beside = OverflowFacts {
            unpinned_overflows: true,
            bar_overflowing: true,
            ..fits_without
        };
        assert!(controls_visible(true, overflows_beside));
    }

    #[test]
    fn a_frozen_close_holds_shown_controls() {
        let frozen = OverflowFacts {
            strip_shown: true,
            unpinned_overflows: false,
            bar_overflowing: true,
            pinned_overflows: false,
        };
        assert!(
            controls_visible(true, frozen),
            "a frozen hold postpones the hide"
        );
        assert!(
            !controls_visible(false, frozen),
            "a frozen hold never shows them"
        );
        let pinned_only = OverflowFacts {
            pinned_overflows: true,
            ..frozen
        };
        assert!(
            !controls_visible(true, pinned_only),
            "pinned-only overflow is not a frozen hold"
        );
    }

    #[test]
    fn a_hidden_strip_shows_no_controls() {
        let hidden = OverflowFacts {
            strip_shown: false,
            unpinned_overflows: true,
            bar_overflowing: true,
            pinned_overflows: false,
        };
        assert!(!controls_visible(true, hidden));
        assert!(!controls_visible(false, hidden));
    }

    #[test]
    fn enablement_follows_the_edges_in_reading_order() {
        let start = extent(0, 600, 2_000);
        let middle = extent(700, 600, 2_000);
        let end = extent(1_400, 600, 2_000);
        for rtl in [false, true] {
            let first_edge = if rtl { end } else { start };
            let last_edge = if rtl { start } else { end };
            assert!(!can_scroll_back(first_edge, rtl));
            assert!(can_scroll_forward(first_edge, rtl));
            assert!(can_scroll_back(last_edge, rtl));
            assert!(!can_scroll_forward(last_edge, rtl));
            assert!(can_scroll_back(middle, rtl));
            assert!(can_scroll_forward(middle, rtl));
        }
        let fits = extent(0, 600, 600);
        assert!(!can_scroll_back(fits, false));
        assert!(!can_scroll_forward(fits, false));
    }

    #[test]
    fn a_step_moves_one_increment_and_clamps_at_both_ends() {
        let start = extent(0, 600, 2_000);
        assert_eq!(
            next_scroll_target(start, 540, ScrollDirection::Forward, false),
            540
        );
        assert_eq!(
            next_scroll_target(start, 540, ScrollDirection::Back, false),
            0
        );
        let near_end = extent(1_200, 600, 2_000);
        assert_eq!(
            next_scroll_target(near_end, 540, ScrollDirection::Forward, false),
            1_400
        );
        assert_eq!(
            next_scroll_target(near_end, 540, ScrollDirection::Back, false),
            660
        );
        // Right-to-left: Back is toward the upper end, where the first tab is.
        assert_eq!(
            next_scroll_target(start, 540, ScrollDirection::Back, true),
            540
        );
        assert_eq!(
            next_scroll_target(near_end, 540, ScrollDirection::Forward, true),
            660
        );
    }

    #[test]
    fn a_zero_width_page_or_an_empty_range_never_moves_outside_it() {
        let collapsed = extent(0, 0, 0);
        assert_eq!(
            next_scroll_target(collapsed, 0, ScrollDirection::Forward, false),
            0
        );
        let page_wider_than_range = extent(0, 800, 600);
        assert_eq!(
            next_scroll_target(page_wider_than_range, 720, ScrollDirection::Forward, false),
            0
        );
        let zero_page = extent(10, 0, 100);
        assert_eq!(
            next_scroll_target(zero_page, 0, ScrollDirection::Forward, false),
            10
        );
    }

    #[test]
    fn all_tabs_shows_from_two_tabs_on_a_shown_strip() {
        assert!(!all_tabs_button_visible(0, true));
        assert!(!all_tabs_button_visible(1, true));
        assert!(all_tabs_button_visible(2, true));
        assert!(all_tabs_button_visible(128, true));
        assert!(!all_tabs_button_visible(5, false));
    }

    #[test]
    fn the_group_attaches_only_while_something_in_it_shows() {
        assert!(!group_attached(false, false));
        assert!(group_attached(true, false));
        assert!(group_attached(false, true));
        assert!(group_attached(true, true));
    }

    #[test]
    fn scroll_controls_need_the_scroller() {
        assert!(scroll_controls_available(true));
        assert!(!scroll_controls_available(false));
    }

    #[test]
    fn a_disabled_focused_button_hands_focus_on_in_order() {
        use OverflowControl::{AllTabs, Back, Forward};
        assert_eq!(
            focus_after_disable(Forward, ControlFocusFacts::new(true, false, true)),
            FocusHandoff::Control(Back)
        );
        assert_eq!(
            focus_after_disable(Back, ControlFocusFacts::new(false, true, false)),
            FocusHandoff::Control(Forward)
        );
        assert_eq!(
            focus_after_disable(Back, ControlFocusFacts::new(false, false, true)),
            FocusHandoff::Control(AllTabs)
        );
        assert_eq!(
            focus_after_disable(Forward, ControlFocusFacts::new(false, false, false)),
            FocusHandoff::Editor
        );
        assert_eq!(
            focus_after_disable(AllTabs, ControlFocusFacts::new(true, true, false)),
            FocusHandoff::Control(Back)
        );
    }

    #[test]
    fn the_accessible_names_are_the_product_names() {
        assert_eq!(SCROLL_BACK_LABEL, "Scroll Tabs Back");
        assert_eq!(SCROLL_FORWARD_LABEL, "Scroll Tabs Forward");
    }
}
