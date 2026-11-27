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

/// A tab scroller's horizontal adjustment in whole pixels, each value rounded
/// once by the adapter: the unpinned scroller's, which every step, enablement
/// and reveal decision reads, or the pinned scroller's, whose overflow alone
/// the reconcile and the reveal read. Its lower bound is always 0
/// (`gtk_adjustment_configure` in the tab box's allocation, ledger A132).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StripScrollExtent {
    /// The adjustment's value.
    pub value_px: i32,
    /// The visible width of the scroller's tabs.
    pub page_px: i32,
    /// The total width of the scroller's tabs.
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
    /// Whether the unpinned adjustment overflows ([`strip_overflows`] of its
    /// extent).
    pub unpinned_overflows: bool,
    /// `AdwTabBar:is-overflowing`, which a close animation's resize freeze
    /// holds true (ledger A133, design F4).
    pub bar_overflowing: bool,
    /// Whether the pinned adjustment overflows.
    pub pinned_overflows: bool,
}

/// Whether a tab scroller's tabs are wider than its visible width by at
/// least one pixel: the one overflow rule both scrollers' readings share.
#[must_use]
pub const fn strip_overflows(extent: StripScrollExtent) -> bool {
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

/// Whether a step toward `direction` can still move the strip.
#[must_use]
pub const fn can_scroll(extent: StripScrollExtent, direction: ScrollDirection, rtl: bool) -> bool {
    can_move(extent, moves_up(direction, rtl))
}

/// Whether Scroll Tabs Back can still move the strip.
#[must_use]
pub const fn can_scroll_back(extent: StripScrollExtent, rtl: bool) -> bool {
    can_scroll(extent, ScrollDirection::Back, rtl)
}

/// Whether Scroll Tabs Forward can still move the strip.
#[must_use]
pub const fn can_scroll_forward(extent: StripScrollExtent, rtl: bool) -> bool {
    can_scroll(extent, ScrollDirection::Forward, rtl)
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

/// What the reconcile reads before it plans the controls.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OverflowControlsInputs {
    /// Whether the lookup found the unpinned tab scroller. Without it the
    /// scroll buttons never show (design D3, fail closed).
    pub scroller_found: bool,
    /// Whether the scroll buttons show now (the hysteresis of
    /// [`controls_visible`]).
    pub buttons_visible: bool,
    /// What the reconcile read from the bar.
    pub facts: OverflowFacts,
    /// The unpinned adjustment, when the scroller was found.
    pub extent: Option<StripScrollExtent>,
    /// Whether the strip lays out right to left.
    pub rtl: bool,
    /// How many tabs are open.
    pub tab_count: u32,
}

/// What the reconcile publishes: every control's state at once, so no
/// action is enabled while its control is hidden.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OverflowControlsPlan {
    /// Whether Back and Forward show.
    pub buttons_visible: bool,
    /// Whether `win.scroll-tabs-back` is enabled: only while the buttons show.
    pub can_scroll_back: bool,
    /// Whether `win.scroll-tabs-forward` is enabled: only while the buttons
    /// show.
    pub can_scroll_forward: bool,
    /// Whether the All Tabs button shows (and its actions are enabled).
    pub all_tabs_visible: bool,
    /// Whether the end group is attached to the bar.
    pub group_attached: bool,
}

/// The controls' plan for one reconcile: the scroll buttons show only with
/// the scroller found and by [`controls_visible`], a scroll action is enabled
/// only while its button shows and can move, All Tabs follows
/// [`all_tabs_button_visible`], and the group is attached while anything in
/// it shows.
#[must_use]
pub const fn plan_overflow_controls(inputs: OverflowControlsInputs) -> OverflowControlsPlan {
    let buttons_visible =
        inputs.scroller_found && controls_visible(inputs.buttons_visible, inputs.facts);
    let (can_scroll_back, can_scroll_forward) = match inputs.extent {
        Some(extent) if buttons_visible => (
            can_scroll_back(extent, inputs.rtl),
            can_scroll_forward(extent, inputs.rtl),
        ),
        _ => (false, false),
    };
    let all_tabs_visible = all_tabs_button_visible(inputs.tab_count, inputs.facts.strip_shown);
    OverflowControlsPlan {
        buttons_visible,
        can_scroll_back,
        can_scroll_forward,
        all_tabs_visible,
        group_attached: group_attached(buttons_visible, all_tabs_visible),
    }
}

/// How many frames a selection reveal may observe: the wait for the strip's
/// allocation, the attach scrolls landing, and the
/// [`SELECTION_REVEAL_SCROLL_WINDOW_US`] after the announcement take about 45
/// frames at 120 Hz and 90 at 240 Hz; after the cap the reveal ends wherever
/// the strip is, so a cap below that need leaves the selected tab out of view.
///
/// Kind: coverage window; spans the measured 90 frames, rounded up to a power
/// of two with a margin.
pub const SELECTION_REVEAL_MAX_FRAMES: u32 = 256;
/// How long the selection reveal waits after re-announcing the selection
/// before it may end: the strip scrolls to the tab with its own 200 ms
/// animation (`SCROLL_ANIMATION_DURATION`, `adw-tab-box.c`), whose eased
/// tail keeps the strip on one whole pixel for several frames, so a still
/// frame inside the window is not the end of the scroll.
///
/// Kind: out: time; spans the strip's 200 ms scroll with a margin.
pub const SELECTION_REVEAL_SCROLL_WINDOW_US: i64 = 256_000;
/// How long a running selection reveal holds the `tab-overflow` readiness
/// blocker. The reveal itself keeps running on the frame clock, but a window
/// whose compositor sends no frames (minimized, occluded) would otherwise
/// hold readiness until it paints again. Its 256 frames take about 4.3 s at
/// 60 Hz; an ordinary reveal ends well inside 1 s.
///
/// Kind: out: time; a readiness bound above the frame cap at 60 Hz.
pub const SELECTION_REVEAL_READINESS_US: i64 = 8_000_000;

/// Whether a selection reveal that started at `started_at_us` still holds the
/// `tab-overflow` readiness blocker at `now_us` (both monotonic): for less
/// than [`SELECTION_REVEAL_READINESS_US`].
#[must_use]
pub const fn reveal_holds_readiness(started_at_us: i64, now_us: i64) -> bool {
    now_us.saturating_sub(started_at_us) < SELECTION_REVEAL_READINESS_US
}

/// Where a selection reveal stands between two frames.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RevealProgress {
    /// The strip's value at the previous allocated frame, in whole pixels.
    pub value_px: Option<i32>,
    /// The strip's total width at the previous allocated frame.
    pub upper_px: Option<i32>,
    /// The frame time of the announcement, once it was made.
    pub announced_at_us: Option<i64>,
}

/// What one frame of the selection reveal does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RevealStep {
    /// Keep watching the next frame.
    Wait,
    /// Re-announce the unchanged selection, so the strip scrolls to it.
    Announce,
    /// The reveal is done.
    End,
}

/// One frame of the selection reveal, from where the previous frames left
/// it, the unpinned strip's `extent` now, whether the pinned tabs overflow
/// their own scroller, the 1-based `frame` count and the frame's time.
///
/// Not allocated yet (no page width): wait, since the box would skip the
/// scroll. The selection is re-announced once, on the first frame that finds
/// the strip where the previous frame left it, so a scroll already under way
/// (an attach scroll landing at this allocation, the strip's own animation)
/// finishes first. When that still frame also keeps the strip's width and
/// neither the unpinned nor the pinned tabs overflow, no scroller can hide
/// the selected tab and the reveal ends without announcing. Announcing again would restart the strip's
/// animation from wherever its eased tail stands, so the reveal never does;
/// it ends on the first still frame once [`SELECTION_REVEAL_SCROLL_WINDOW_US`]
/// has passed since the announcement, and after
/// [`SELECTION_REVEAL_MAX_FRAMES`] frames in any case.
#[must_use]
pub const fn reveal_frame(
    progress: RevealProgress,
    extent: StripScrollExtent,
    pinned_overflows: bool,
    frame: u32,
    frame_time_us: i64,
) -> (RevealStep, RevealProgress) {
    if frame > SELECTION_REVEAL_MAX_FRAMES {
        return (RevealStep::End, progress);
    }
    if extent.page_px <= 0 {
        return (RevealStep::Wait, progress);
    }
    let still = matches!(progress.value_px, Some(value) if value == extent.value_px);
    let width_kept = matches!(progress.upper_px, Some(upper) if upper == extent.upper_px);
    let next = RevealProgress {
        value_px: Some(extent.value_px),
        upper_px: Some(extent.upper_px),
        announced_at_us: progress.announced_at_us,
    };
    match progress.announced_at_us {
        None if !still => (RevealStep::Wait, next),
        None if width_kept && !strip_overflows(extent) && !pinned_overflows => {
            (RevealStep::End, next)
        }
        None => (
            RevealStep::Announce,
            RevealProgress {
                announced_at_us: Some(frame_time_us),
                ..next
            },
        ),
        Some(announced_at_us)
            if still
                && frame_time_us.saturating_sub(announced_at_us)
                    >= SELECTION_REVEAL_SCROLL_WINDOW_US =>
        {
            (RevealStep::End, next)
        }
        Some(_) => (RevealStep::Wait, next),
    }
}

/// Whether a tab drawn from `start_px` for `width_px`, both relative to the
/// unpinned scroller and rounded once by the adapter, lies wholly inside the
/// scroller's `page_px`-wide view: the strip shows the whole tab. A tab or a
/// view with no width (not allocated yet) is never in view.
#[must_use]
pub const fn tab_in_view(start_px: i32, width_px: i32, page_px: i32) -> bool {
    width_px > 0 && page_px > 0 && start_px >= 0 && start_px.saturating_add(width_px) <= page_px
}

/// The All Tabs button's tooltip, before its count.
pub const ALL_TABS_LABEL: &str = "All Tabs";
/// The subtitle of an All Tabs row whose document has no file.
pub const ALL_TABS_UNTITLED_SUBTITLE: &str = "Not saved to a file";
/// The All Tabs popover's state when no open tab matches the query.
pub const ALL_TABS_NO_MATCH_TITLE: &str = "No Open Tab Matches";
/// The accessible name of the popover's search entry.
pub const ALL_TABS_SEARCH_LABEL: &str = "Search open tabs";
/// The accessible name of the popover's list.
pub const ALL_TABS_LIST_LABEL: &str = "Open tabs";
/// The accessible description of the popover's search entry.
pub const ALL_TABS_SEARCH_DESCRIPTION: &str = "Filter the open tabs by name or folder";
/// The accessible description of the popover's list.
pub const ALL_TABS_LIST_DESCRIPTION: &str = "Every open tab of this window, in tab strip order";
/// The tooltip of a row's pinned icon.
pub const ALL_TABS_PINNED_TOOLTIP: &str = "Pinned";
/// The tooltip of the current tab's row marker.
pub const ALL_TABS_CURRENT_TOOLTIP: &str = "Current tab";

/// How long the All Tabs query must rest before its match count is
/// announced.
///
/// Kind: out: time; a settle delay, the debounced-results lane's cooldown.
pub const ALL_TABS_ANNOUNCE_DELAY_MS: u32 = 500;

/// The longest accessible row name of the All Tabs list, in characters: a
/// tab's title, which a file name keeps far below it, plus its state words.
///
/// Kind: presentation cap; characters announced, a power of two at or above
/// the longest name the app builds (a 255-byte file name and its state).
pub const ALL_TABS_ROW_NAME_MAX_CHARS: usize = 512;

/// The All Tabs button's accessible name and tooltip: "All Tabs, 12 open".
#[must_use]
pub fn all_tabs_button_name(tab_count: u32) -> String {
    format!("{ALL_TABS_LABEL}, {tab_count} open")
}

/// The match count announced once a query settles.
#[must_use]
pub fn all_tabs_match_announcement(matches: u32, total: u32, query_empty: bool) -> String {
    if query_empty {
        return match total {
            1 => "1 open tab".to_owned(),
            _ => format!("{total} open tabs"),
        };
    }
    match matches {
        0 => "No open tab matches".to_owned(),
        1 => format!("1 of {total} open tabs matches"),
        _ => format!("{matches} of {total} open tabs match"),
    }
}

/// The All Tabs list's accessible value text: the highlighted row's name
/// first, so an assistive technology reading the list hears which tab Enter
/// selects while the focus stays in the search entry, then the match count.
/// The name is a row name, already bounded by [`all_tabs_row_name`].
#[must_use]
pub fn all_tabs_list_value_text(highlighted_row_name: Option<&str>, match_count: &str) -> String {
    match highlighted_row_name {
        Some(name) => format!("Selected {name}. {match_count}"),
        None => match_count.to_owned(),
    }
}

/// One row's accessible name: its title and the states a sighted user reads
/// from its icons, cut to [`ALL_TABS_ROW_NAME_MAX_CHARS`] characters.
#[must_use]
pub fn all_tabs_row_name(title: &str, modified: bool, pinned: bool, current: bool) -> String {
    let mut name: String = title
        .chars()
        .take(ALL_TABS_ROW_NAME_MAX_CHARS / 2)
        .collect();
    for (holds, word) in [
        (current, ", current tab"),
        (pinned, ", pinned"),
        (modified, ", unsaved changes"),
    ] {
        if holds {
            name.push_str(word);
        }
    }
    name.chars().take(ALL_TABS_ROW_NAME_MAX_CHARS).collect()
}

/// A tab's folder as its All Tabs row shows it: under the home folder `home`
/// shortened to `~` (or `~/rest`), otherwise the absolute folder.
#[must_use]
pub fn all_tabs_folder_label(folder: &std::path::Path, home: &std::path::Path) -> String {
    match folder.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => folder.display().to_string(),
    }
}

/// What the All Tabs filter reads of one open tab.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AllTabsCandidate<'a> {
    /// The tab's title.
    pub title: &'a str,
    /// The folder of the tab's file as its row shows it
    /// (`all_tabs_folder_label`), `None` for an untitled document.
    pub folder: Option<&'a str>,
    /// The same folder as an absolute path, which the filter matches too: the
    /// shown `~` form hides the home folder's own path.
    pub folder_path: Option<&'a str>,
}

/// How one candidate matched, best first.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum AllTabsMatch {
    /// The title starts with the query.
    TitlePrefix,
    /// The title contains the query.
    TitleSubstring,
    /// The folder, as shown or as an absolute path, contains the query.
    FolderSubstring,
    /// The title matches the query fuzzily, by descending score.
    TitleFuzzy(std::cmp::Reverse<u32>),
}

/// The rows the All Tabs list shows for `query`, as indexes into
/// `candidates` (strip order).
///
/// An empty query keeps strip order. Otherwise the title is matched first
/// (prefix, then substring, then the shared fuzzy matcher), and the folder
/// (as shown, or as an absolute path) only by substring, case-insensitively: a short query matches nearly every
/// absolute path fuzzily, which would list every tab. Ties keep strip order.
#[must_use]
pub fn rank_all_tabs(query: &str, candidates: &[AllTabsCandidate<'_>]) -> Vec<u32> {
    let indexed = candidates.iter().zip(0u32..);
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return indexed.map(|(_, index)| index).collect();
    }
    let folded = trimmed.to_lowercase();
    let mut fuzzy = lushtext_services::fuzzy::FuzzyQuery::new(trimmed);
    let mut ranked: Vec<(AllTabsMatch, u32)> = indexed
        .filter_map(|(candidate, index)| {
            let title = candidate.title.to_lowercase();
            let matched = if title.starts_with(&folded) {
                AllTabsMatch::TitlePrefix
            } else if title.contains(&folded) {
                AllTabsMatch::TitleSubstring
            } else if [candidate.folder, candidate.folder_path]
                .into_iter()
                .flatten()
                .any(|folder| folder.to_lowercase().contains(&folded))
            {
                AllTabsMatch::FolderSubstring
            } else {
                AllTabsMatch::TitleFuzzy(std::cmp::Reverse(fuzzy.score(candidate.title)?))
            };
            Some((matched, index))
        })
        .collect();
    ranked.sort_unstable();
    ranked.into_iter().map(|(_, index)| index).collect()
}

/// Which row the All Tabs list highlights: the selected tab's row for an
/// empty query, the best match otherwise, none when nothing is listed.
#[must_use]
pub fn all_tabs_highlight(query_empty: bool, listed: &[u32], selected: Option<u32>) -> Option<u32> {
    if listed.is_empty() {
        return None;
    }
    if query_empty
        && let Some(selected) = selected
        && let Some(position) = listed.iter().position(|index| *index == selected)
    {
        return u32::try_from(position).ok();
    }
    Some(0)
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

    /// Whether `control` can still hold focus.
    #[must_use]
    pub const fn available(self, control: OverflowControl) -> bool {
        match control {
            OverflowControl::Back => self.back_available,
            OverflowControl::Forward => self.forward_available,
            OverflowControl::AllTabs => self.all_tabs_available,
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
    fn a_tab_is_in_view_only_when_the_whole_tab_is_inside_the_page() {
        assert!(tab_in_view(0, 126, 598));
        assert!(tab_in_view(472, 126, 598));
        assert!(!tab_in_view(473, 126, 598), "one pixel past the end");
        assert!(!tab_in_view(-1, 126, 598), "one pixel before the start");
        assert!(!tab_in_view(-3_000, 126, 598));
        assert!(!tab_in_view(0, 0, 598), "an unallocated tab");
        assert!(!tab_in_view(0, 126, 0), "an unallocated view");
        assert!(
            !tab_in_view(i32::MAX, 126, 598),
            "no overflow at the range end"
        );
    }

    #[test]
    fn overflow_needs_at_least_one_pixel_beyond_the_page() {
        assert!(!strip_overflows(extent(0, 600, 600)));
        assert!(strip_overflows(extent(0, 600, 601)));
        assert!(!strip_overflows(extent(0, 0, 0)));
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
    fn a_control_is_available_by_its_own_flag() {
        use OverflowControl::{AllTabs, Back, Forward};
        let facts = ControlFocusFacts::new(true, false, true);
        assert!(facts.available(Back));
        assert!(!facts.available(Forward));
        assert!(facts.available(AllTabs));
        let facts = ControlFocusFacts::new(false, true, false);
        assert!(!facts.available(Back));
        assert!(facts.available(Forward));
        assert!(!facts.available(AllTabs));
    }

    const fn plan_inputs(
        scroller_found: bool,
        buttons_visible: bool,
        facts: OverflowFacts,
        extent: Option<StripScrollExtent>,
        tab_count: u32,
    ) -> OverflowControlsInputs {
        OverflowControlsInputs {
            scroller_found,
            buttons_visible,
            facts,
            extent,
            rtl: false,
            tab_count,
        }
    }

    const fn shown(unpinned_overflows: bool) -> OverflowFacts {
        OverflowFacts {
            strip_shown: true,
            unpinned_overflows,
            bar_overflowing: unpinned_overflows,
            pinned_overflows: false,
        }
    }

    #[test]
    fn no_scroller_means_no_scroll_buttons_and_no_scroll_action() {
        let plan = plan_overflow_controls(plan_inputs(
            false,
            true,
            shown(true),
            Some(extent(300, 1000, 4000)),
            40,
        ));
        assert!(!plan.buttons_visible);
        assert!(!plan.can_scroll_back);
        assert!(!plan.can_scroll_forward);
        assert!(plan.all_tabs_visible);
        assert!(plan.group_attached);
    }

    #[test]
    fn a_scroll_action_is_enabled_only_while_its_button_shows() {
        // The strip hid (Focus Mode) with the adjustment still mid-range.
        let hidden = OverflowFacts {
            strip_shown: false,
            ..shown(true)
        };
        let plan = plan_overflow_controls(plan_inputs(
            true,
            true,
            hidden,
            Some(extent(300, 1000, 4000)),
            40,
        ));
        assert_eq!(plan, OverflowControlsPlan::default());
        // Shown and overflowing mid-range: both ways move.
        let plan = plan_overflow_controls(plan_inputs(
            true,
            false,
            shown(true),
            Some(extent(300, 1000, 4000)),
            40,
        ));
        assert!(plan.buttons_visible && plan.can_scroll_back && plan.can_scroll_forward);
        // At the start edge only Forward moves.
        let plan = plan_overflow_controls(plan_inputs(
            true,
            true,
            shown(true),
            Some(extent(0, 1000, 4000)),
            40,
        ));
        assert!(!plan.can_scroll_back && plan.can_scroll_forward);
    }

    #[test]
    fn the_plan_attaches_the_group_for_all_tabs_alone() {
        let plan = plan_overflow_controls(plan_inputs(
            true,
            false,
            shown(false),
            Some(extent(0, 1000, 800)),
            2,
        ));
        assert!(!plan.buttons_visible);
        assert!(plan.all_tabs_visible);
        assert!(plan.group_attached);
        let plan = plan_overflow_controls(plan_inputs(
            true,
            false,
            shown(false),
            Some(extent(0, 1000, 400)),
            1,
        ));
        assert_eq!(plan, OverflowControlsPlan::default());
    }

    /// Runs the reveal over `frames` (value, page, upper, frame time) with
    /// pinned tabs that fit, and returns each frame's step.
    fn reveal_steps(frames: &[(i32, i32, i32, i64)]) -> Vec<RevealStep> {
        reveal_steps_pinned(frames, false)
    }

    fn reveal_steps_pinned(frames: &[(i32, i32, i32, i64)], pinned: bool) -> Vec<RevealStep> {
        let mut progress = RevealProgress::default();
        frames
            .iter()
            .zip(1u32..)
            .map(|(&(value, page, upper, time), frame)| {
                let (step, next) =
                    reveal_frame(progress, extent(value, page, upper), pinned, frame, time);
                progress = next;
                step
            })
            .collect()
    }

    #[test]
    fn the_reveal_waits_for_allocation_and_a_still_frame_then_announces_once() {
        use RevealStep::{Announce, End, Wait};
        let steps = reveal_steps(&[
            (0, 0, 0, 0),             // not allocated
            (4000, 1000, 5000, 10),   // first allocated frame
            (3000, 1000, 5000, 20),   // an attach scroll still lands
            (3000, 1000, 5000, 30),   // still: announce
            (1000, 1000, 5000, 40),   // the strip's own scroll
            (0, 1000, 5000, 50),      // its eased tail
            (0, 1000, 5000, 60),      // still, but inside the window
            (0, 1000, 5000, 256_030), // still, the window passed
        ]);
        assert_eq!(steps, [Wait, Wait, Wait, Announce, Wait, Wait, Wait, End]);
    }

    #[test]
    fn the_reveal_ends_without_announcing_when_nothing_overflows() {
        use RevealStep::{End, Wait};
        assert_eq!(
            reveal_steps(&[(0, 1000, 800, 10), (0, 1000, 800, 20)]),
            [Wait, End]
        );
        // Overflowing pinned tabs can still hide a pinned selection.
        assert_eq!(
            reveal_steps_pinned(&[(0, 1000, 800, 10), (0, 1000, 800, 20)], true),
            [Wait, RevealStep::Announce]
        );
        // A strip whose tabs still widen keeps watching.
        assert_eq!(
            reveal_steps(&[(0, 1000, 600, 10), (0, 1000, 800, 20), (0, 1000, 800, 30)]),
            [Wait, RevealStep::Announce, Wait]
        );
    }

    #[test]
    fn the_reveal_ends_at_the_frame_cap() {
        let progress = RevealProgress {
            value_px: Some(10),
            upper_px: Some(5000),
            announced_at_us: None,
        };
        let (step, _) = reveal_frame(
            progress,
            extent(20, 1000, 5000),
            false,
            SELECTION_REVEAL_MAX_FRAMES + 1,
            0,
        );
        assert_eq!(step, RevealStep::End);
        let (step, _) = reveal_frame(
            progress,
            extent(20, 1000, 5000),
            false,
            SELECTION_REVEAL_MAX_FRAMES,
            0,
        );
        assert_eq!(step, RevealStep::Wait);
    }

    #[test]
    fn a_reveal_holds_readiness_for_a_bounded_time_only() {
        assert!(reveal_holds_readiness(1_000, 1_000));
        assert!(reveal_holds_readiness(
            1_000,
            1_000 + SELECTION_REVEAL_READINESS_US - 1
        ));
        assert!(!reveal_holds_readiness(
            1_000,
            1_000 + SELECTION_REVEAL_READINESS_US
        ));
        assert!(!reveal_holds_readiness(i64::MIN, i64::MAX));
    }

    #[test]
    fn the_folder_label_shortens_the_home_folder_only() {
        let home = std::path::Path::new("/home/me");
        let label = |folder: &str| all_tabs_folder_label(std::path::Path::new(folder), home);
        assert_eq!(label("/home/me"), "~");
        assert_eq!(label("/home/me/proj/src"), "~/proj/src");
        assert_eq!(label("/home/meX/proj"), "/home/meX/proj");
        assert_eq!(label("/srv/data"), "/srv/data");
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

    fn candidates<'a>(rows: &[(&'a str, Option<&'a str>)]) -> Vec<AllTabsCandidate<'a>> {
        rows.iter()
            .map(|(title, folder)| AllTabsCandidate {
                title,
                folder: *folder,
                folder_path: None,
            })
            .collect()
    }

    #[test]
    fn filtering_by_folder_matches_the_absolute_path_behind_the_home_label() {
        let rows = [
            AllTabsCandidate {
                title: "mod.rs",
                folder: Some("~/proj/src"),
                folder_path: Some("/var/home/me/proj/src"),
            },
            AllTabsCandidate {
                title: "lib.rs",
                folder: Some("/srv/other"),
                folder_path: Some("/srv/other"),
            },
        ];
        assert_eq!(rank_all_tabs("/var/home/me/proj", &rows), vec![0]);
        assert_eq!(rank_all_tabs("home/me", &rows), vec![0]);
        assert_eq!(rank_all_tabs("~/proj", &rows), vec![0], "the shown label");
    }

    #[test]
    fn an_empty_query_keeps_strip_order_and_highlights_the_selected_tab() {
        let rows = candidates(&[("a.txt", None), ("b.txt", None), ("c.txt", None)]);
        let listed = rank_all_tabs("  ", &rows);
        assert_eq!(listed, vec![0, 1, 2]);
        assert_eq!(all_tabs_highlight(true, &listed, Some(2)), Some(2));
        assert_eq!(all_tabs_highlight(true, &listed, None), Some(0));
        assert_eq!(all_tabs_highlight(true, &[], Some(0)), None);
    }

    #[test]
    fn filtering_by_name_lists_the_matches_best_first() {
        let rows = candidates(&[
            ("main.rs", Some("/home/u/src")),
            ("notes.md", Some("/home/u")),
            ("README.md", Some("/home/u")),
        ]);
        let listed = rank_all_tabs("md", &rows);
        assert_eq!(listed, vec![1, 2], "main.rs does not match md");
        assert_eq!(all_tabs_highlight(false, &listed, Some(0)), Some(0));
        assert_eq!(
            rank_all_tabs("READ", &rows),
            vec![2],
            "case-insensitive prefix"
        );
    }

    #[test]
    fn filtering_by_folder_lists_that_folders_file_first() {
        let rows = candidates(&[
            ("mod.rs", Some("/home/u/lushtext/src/search")),
            ("mod.rs", Some("/home/u/lushtext/src/palette")),
        ]);
        assert_eq!(rank_all_tabs("palette", &rows), vec![1]);
        let untitled = candidates(&[("Untitled Document 1", None)]);
        assert!(rank_all_tabs("palette", &untitled).is_empty());
    }

    #[test]
    fn a_title_match_ranks_above_a_folder_match_and_fuzzy_comes_last() {
        let rows = candidates(&[
            ("fuzzy-target.txt", Some("/x")),
            ("other.txt", Some("/x/target")),
            ("target.txt", Some("/x")),
        ]);
        assert_eq!(rank_all_tabs("target", &rows), vec![2, 0, 1]);
        let fuzzy_only = candidates(&[("palette_shell.rs", Some("/x"))]);
        assert_eq!(rank_all_tabs("pshl", &fuzzy_only), vec![0]);
    }

    #[test]
    fn the_all_tabs_copy() {
        assert_eq!(all_tabs_button_name(12), "All Tabs, 12 open");
        assert_eq!(
            all_tabs_match_announcement(0, 4, false),
            "No open tab matches"
        );
        assert_eq!(
            all_tabs_match_announcement(1, 4, false),
            "1 of 4 open tabs matches"
        );
        assert_eq!(
            all_tabs_match_announcement(3, 4, false),
            "3 of 4 open tabs match"
        );
        assert_eq!(all_tabs_match_announcement(4, 4, true), "4 open tabs");
        assert_eq!(ALL_TABS_UNTITLED_SUBTITLE, "Not saved to a file");
    }

    #[test]
    fn a_row_name_carries_its_states_and_stays_bounded() {
        assert_eq!(
            all_tabs_row_name("notes.md", true, true, true),
            "notes.md, current tab, pinned, unsaved changes"
        );
        assert_eq!(all_tabs_row_name("a", false, false, false), "a");
        let long = "x".repeat(4_096);
        let name = all_tabs_row_name(&long, true, true, true);
        assert!(name.chars().count() <= ALL_TABS_ROW_NAME_MAX_CHARS);
        assert!(name.ends_with("unsaved changes"));
        // The longest name the app builds: a 255-byte file name and every state.
        let longest = all_tabs_row_name(&"y".repeat(255), true, true, true);
        assert!(longest.chars().count() <= ALL_TABS_ROW_NAME_MAX_CHARS);
    }

    #[test]
    fn the_list_value_text_names_the_highlighted_row_before_the_count() {
        assert_eq!(
            all_tabs_list_value_text(Some("beta.txt, pinned"), "3 open tabs"),
            "Selected beta.txt, pinned. 3 open tabs"
        );
        assert_eq!(
            all_tabs_list_value_text(None, "No open tab matches"),
            "No open tab matches"
        );
    }

    #[test]
    fn the_accessible_names_are_the_product_names() {
        assert_eq!(SCROLL_BACK_LABEL, "Scroll Tabs Back");
        assert_eq!(SCROLL_FORWARD_LABEL, "Scroll Tabs Forward");
    }
}
