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
/// `None` inputs come from GtkSourceView reporting `-1` while a scan is still
/// running or when no match holds the cursor, converted at the adapter edge so a
/// mid-scan total reads as blank rather than flickering "0 of 0".
#[must_use]
pub fn match_count_label(current: Option<u32>, total: Option<u32>) -> Option<String> {
    let total = total.filter(|&count| count > 0)?;
    let current = current.filter(|&pos| pos > 0)?;
    Some(format!("{current} of {total}"))
}

/// Whether the query field should be styled and announced as invalid.
///
/// Only a query that is present *and* finished scanning (`Some(0)`) with no
/// matches is invalid. An empty query is not an error, and `None` means the
/// scan has not finished, so styling it red would punish the user mid-word.
#[must_use]
pub fn query_has_no_matches(search_text: &str, total: Option<u32>) -> bool {
    !search_text.is_empty() && total == Some(0)
}

/// The occurrence number to display for a reported scan.
///
/// `None` means GtkSourceView could not place the selection among the matches;
/// that reads as `0` ("no current match") rather than a negative position.
#[must_use]
pub fn current_occurrence(total: Option<u32>, occurrence_position: Option<u32>) -> u32 {
    match total {
        Some(count) if count > 0 => occurrence_position.unwrap_or(0),
        _ => 0,
    }
}

/// What to announce for a finished scan, or `None` to stay silent.
///
/// Silence is the right answer twice: for an empty query, because the user has
/// not asked anything yet, and for an unfinished scan (`None`), because announcing
/// an interim count would read out a number that is about to change.
#[must_use]
pub fn match_count_announcement(
    search_text: &str,
    current: u32,
    total: Option<u32>,
) -> Option<String> {
    if search_text.is_empty() {
        return None;
    }
    let total = total?;

    Some(match (total, current) {
        (0, _) => "No matches in active document".to_string(),
        (1, _) => "1 match in active document".to_string(),
        (total, current) if current > 0 => {
            format!("{total} matches in active document; current match {current}")
        }
        (total, _) => format!("{total} matches in active document"),
    })
}

/// What "Open and Replace in Editor" puts in the bar (stream-replace-all
/// design D7): the run's query and options, and the replacement translated
/// for GtkSourceView, or `None` when it names a group the pattern lacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorReplacePrefill {
    /// The search text, unchanged.
    pub query: String,
    /// The replacement in GtkSourceView's syntax; `None` leaves the field
    /// empty with [`MISSING_GROUP_PLACEHOLDER`].
    pub replacement: Option<String>,
    /// Regex mode.
    pub regex: bool,
    /// Case-sensitive matching.
    pub case_sensitive: bool,
    /// Whole-word matching (`at-word-boundaries`, ledger A128).
    pub whole_word: bool,
}

impl EditorReplacePrefill {
    /// Each option with the state the prefill sets.
    #[must_use]
    pub const fn options(&self) -> [(SearchOption, bool); 3] {
        [
            (SearchOption::Regex, self.regex),
            (SearchOption::CaseSensitive, self.case_sensitive),
            (SearchOption::WholeWord, self.whole_word),
        ]
    }

    /// The replacement field's placeholder: why it is empty, or the usual one.
    #[must_use]
    pub const fn replacement_placeholder(&self) -> &'static str {
        if self.replacement.is_some() {
            REPLACE_PLACEHOLDER
        } else {
            MISSING_GROUP_PLACEHOLDER
        }
    }
}

/// The replacement field's usual placeholder (`search-bar.blp`).
pub const REPLACE_PLACEHOLDER: &str = "Replace…";
/// The replacement field's placeholder when the run's replacement names a
/// capture group the pattern does not have, so it was not carried over.
pub const MISSING_GROUP_PLACEHOLDER: &str =
    "Replacement not carried over: it names a group the pattern lacks";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scan_in_progress_blanks_the_counter_rather_than_reading_zero() {
        // GtkSourceView's -1 while scanning becomes None at the adapter boundary.
        assert_eq!(match_count_label(Some(0), None), None);
        assert_eq!(match_count_label(Some(1), None), None);
        assert_eq!(match_count_label(Some(0), Some(0)), None);
        // A real match reads as a position.
        assert_eq!(
            match_count_label(Some(1), Some(1)),
            Some("1 of 1".to_string())
        );
        assert_eq!(
            match_count_label(Some(3), Some(12)),
            Some("3 of 12".to_string())
        );
    }

    #[test]
    fn a_total_without_a_current_position_blanks_the_counter() {
        assert_eq!(match_count_label(Some(0), Some(7)), None);
        assert_eq!(match_count_label(None, Some(7)), None);
    }

    #[test]
    fn only_a_finished_scan_of_a_present_query_is_invalid() {
        assert!(query_has_no_matches("needle", Some(0)));
        assert!(
            !query_has_no_matches("", Some(0)),
            "an empty query is not an error"
        );
        assert!(
            !query_has_no_matches("needle", None),
            "an unfinished scan must not style the query as invalid"
        );
        assert!(!query_has_no_matches("needle", Some(4)));
    }

    #[test]
    fn a_negative_occurrence_position_reads_as_no_current_match() {
        assert_eq!(current_occurrence(Some(5), None), 0);
        assert_eq!(current_occurrence(Some(5), Some(3)), 3);
        assert_eq!(current_occurrence(Some(0), Some(3)), 0);
        assert_eq!(current_occurrence(None, Some(3)), 0);
    }

    #[test]
    fn announcements_stay_silent_until_there_is_something_settled_to_say() {
        assert_eq!(match_count_announcement("", 0, Some(5)), None);
        assert_eq!(match_count_announcement("needle", 0, None), None);
    }

    #[test]
    fn announcements_render_the_exact_user_facing_wording() {
        assert_eq!(
            match_count_announcement("needle", 0, Some(0)).as_deref(),
            Some("No matches in active document")
        );
        assert_eq!(
            match_count_announcement("needle", 1, Some(1)).as_deref(),
            Some("1 match in active document")
        );
        assert_eq!(
            match_count_announcement("needle", 2, Some(7)).as_deref(),
            Some("7 matches in active document; current match 2")
        );
        assert_eq!(
            match_count_announcement("needle", 0, Some(7)).as_deref(),
            Some("7 matches in active document")
        );
    }

    #[test]
    fn one_match_reads_singular_even_with_a_current_position() {
        assert_eq!(
            match_count_announcement("needle", 1, Some(1)).as_deref(),
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
