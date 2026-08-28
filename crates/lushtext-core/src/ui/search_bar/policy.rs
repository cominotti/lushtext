// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: pure policy — the in-tab find workflow's `policy.rs`.
//!
//! The census recorded this row as owning **no** pure policy. Probing the
//! adapter for separable decisions — which the convention requires before that
//! conclusion may be recorded — found four, all of them user-visible and none of
//! them needing a widget: what the match counter reads, what a screen reader is
//! told, when the query field is styled as invalid, and which search setting an
//! option name names. They were interleaved with GTK calls, so none of them had
//! mutation coverage.

/// Which GtkSourceView search setting an option action name controls.
///
/// The three names are the action names in the search-options group. Returning a
/// typed option instead of matching on the string at each call site is what lets
/// the adapter apply a setting without re-deciding what the name means; the
/// adapter had two independent copies of this match before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchOption {
    /// Interpret the query as a regular expression.
    Regex,
    /// Match case exactly.
    CaseSensitive,
    /// Match only at word boundaries.
    WholeWord,
}

/// The option action names, in the order the adapter applies them on attach.
pub const SEARCH_OPTION_ACTION_NAMES: [&str; 3] = ["regex", "case-sensitive", "whole-word"];

impl SearchOption {
    /// Resolve an option action name, or `None` if it names no search setting.
    #[must_use]
    pub fn from_action_name(name: &str) -> Option<Self> {
        match name {
            "regex" => Some(Self::Regex),
            "case-sensitive" => Some(Self::CaseSensitive),
            "whole-word" => Some(Self::WholeWord),
            _ => None,
        }
    }

    /// The action name this option is controlled by.
    #[must_use]
    pub const fn action_name(self) -> &'static str {
        match self {
            Self::Regex => "regex",
            Self::CaseSensitive => "case-sensitive",
            Self::WholeWord => "whole-word",
        }
    }
}

/// Accessible value text shown when there is no current match to report.
pub const NO_CURRENT_MATCH_VALUE_TEXT: &str = "No current search match";

/// What the match counter should read for one search state.
///
/// `None` means the counter is blank. GtkSourceView reports `total == -1` while
/// its scan is still running, so a negative total must read as "nothing to say"
/// rather than as zero matches — showing "0 of 0" mid-scan would flicker a false
/// no-match state on every keystroke.
#[must_use]
pub fn match_count_label(current: i32, total: i32) -> Option<String> {
    if total <= 0 || current <= 0 {
        None
    } else {
        Some(format!("{current} of {total}"))
    }
}

/// Whether the query field should be styled and announced as invalid.
///
/// Only a query that is present *and* finished scanning with no matches is
/// invalid. An empty query is not an error, and `total < 0` means the scan has
/// not finished, so styling it red would punish the user mid-word.
#[must_use]
pub fn query_has_no_matches(search_text: &str, total: i32) -> bool {
    !search_text.is_empty() && total == 0
}

/// The occurrence number to display for a reported scan.
///
/// A negative occurrence position means GtkSourceView could not place the
/// selection among the matches; that reads as "no current match" rather than as
/// a position behind the first one.
#[must_use]
pub fn current_occurrence(total: i32, occurrence_position: i32) -> i32 {
    if total > 0 {
        occurrence_position.max(0)
    } else {
        0
    }
}

/// What to announce for a finished scan, or `None` to stay silent.
///
/// Silence is the right answer twice: for an empty query, because the user has
/// not asked anything yet, and for an unfinished scan, because announcing an
/// interim count would read out a number that is about to change.
#[must_use]
pub fn match_count_announcement(search_text: &str, current: i32, total: i32) -> Option<String> {
    if search_text.is_empty() || total < 0 {
        return None;
    }

    Some(match (total, current) {
        (0, _) => "No matches in active document".to_string(),
        (1, _) => "1 match in active document".to_string(),
        (total, current) if current > 0 => {
            format!("{total} matches in active document; current match {current}")
        }
        (total, _) => format!("{total} matches in active document"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scan_in_progress_blanks_the_counter_rather_than_reading_zero() {
        // GtkSourceView reports -1 while scanning.
        assert_eq!(match_count_label(0, -1), None);
        assert_eq!(match_count_label(1, -1), None);
        assert_eq!(match_count_label(0, 0), None);
        // A real match reads as a position.
        assert_eq!(match_count_label(1, 1), Some("1 of 1".to_string()));
        assert_eq!(match_count_label(3, 12), Some("3 of 12".to_string()));
    }

    #[test]
    fn a_total_without_a_current_position_blanks_the_counter() {
        assert_eq!(match_count_label(0, 7), None);
        assert_eq!(match_count_label(-1, 7), None);
    }

    #[test]
    fn only_a_finished_scan_of_a_present_query_is_invalid() {
        assert!(query_has_no_matches("needle", 0));
        assert!(
            !query_has_no_matches("", 0),
            "an empty query is not an error"
        );
        assert!(
            !query_has_no_matches("needle", -1),
            "an unfinished scan must not style the query as invalid"
        );
        assert!(!query_has_no_matches("needle", 4));
    }

    #[test]
    fn a_negative_occurrence_position_reads_as_no_current_match() {
        assert_eq!(current_occurrence(5, -1), 0);
        assert_eq!(current_occurrence(5, 3), 3);
        assert_eq!(current_occurrence(0, 3), 0);
        assert_eq!(current_occurrence(-1, 3), 0);
    }

    #[test]
    fn announcements_stay_silent_until_there_is_something_settled_to_say() {
        assert_eq!(match_count_announcement("", 0, 5), None);
        assert_eq!(match_count_announcement("needle", 0, -1), None);
    }

    #[test]
    fn announcements_render_the_exact_user_facing_wording() {
        assert_eq!(
            match_count_announcement("needle", 0, 0).as_deref(),
            Some("No matches in active document")
        );
        assert_eq!(
            match_count_announcement("needle", 1, 1).as_deref(),
            Some("1 match in active document")
        );
        assert_eq!(
            match_count_announcement("needle", 2, 7).as_deref(),
            Some("7 matches in active document; current match 2")
        );
        assert_eq!(
            match_count_announcement("needle", 0, 7).as_deref(),
            Some("7 matches in active document")
        );
    }

    #[test]
    fn one_match_reads_singular_even_with_a_current_position() {
        assert_eq!(
            match_count_announcement("needle", 1, 1).as_deref(),
            Some("1 match in active document")
        );
    }

    #[test]
    fn option_names_round_trip_and_reject_anything_else() {
        for name in SEARCH_OPTION_ACTION_NAMES {
            let option = SearchOption::from_action_name(name)
                .expect("every listed option name must resolve");
            assert_eq!(option.action_name(), name);
        }
        assert_eq!(SearchOption::from_action_name("wrap-around"), None);
        assert_eq!(SearchOption::from_action_name(""), None);
    }

    #[test]
    fn the_option_action_names_are_the_exact_group_action_names() {
        assert_eq!(
            SEARCH_OPTION_ACTION_NAMES,
            ["regex", "case-sensitive", "whole-word"]
        );
    }

    #[test]
    fn the_no_current_match_value_text_is_the_exact_announced_literal() {
        assert_eq!(NO_CURRENT_MATCH_VALUE_TEXT, "No current search match");
    }
}
