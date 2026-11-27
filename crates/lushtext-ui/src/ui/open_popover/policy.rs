// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: pure policy — what the recent-documents popover says, and when it
//! rebuilds.
//!
//! The *ranking and filtering* of recent documents lives in
//! `model::recent_documents` / `services`, shared with other callers and staying
//! there. What this module owns is the decision set that lived inline in the GTK
//! adapter: which empty-state copy the user sees, what a screen reader is told
//! the list contains, and whether a row rebuild is worth doing at all.
//!
//! The copy decisions are here rather than inline because they are exactly the
//! kind that regresses silently. *"No Recent Documents"* and *"No Matching
//! Documents"* are two different statements about the user's situation — one
//! says the history is empty, the other says the filter excluded everything —
//! and a change that collapses them still renders an empty state that looks
//! plausible in a screenshot.
//!
//! This module imports no toolkit crate, which is what keeps it inside the
//! `ui/**/policy.rs` mutation scope.

/// What the popover shows when no rows are visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyStateCopy {
    /// Title shown in the empty-state page.
    pub title: &'static str,
    /// Accessible name for the empty-state page.
    pub accessible_name: &'static str,
    /// Accessible description for the empty-state page.
    pub accessible_description: &'static str,
}

/// The copy for an empty history — nothing has been opened yet.
pub const NO_RECENT_DOCUMENTS: EmptyStateCopy = EmptyStateCopy {
    title: "No Recent Documents",
    accessible_name: "No recent documents",
    accessible_description: "No recently opened documents are available",
};

/// The copy for a filter that excluded every row.
pub const NO_MATCHING_DOCUMENTS: EmptyStateCopy = EmptyStateCopy {
    title: "No Matching Documents",
    accessible_name: "No matching recent documents",
    accessible_description: "No recent documents match the current filter",
};

/// Choose the empty-state copy for a blank or non-blank query.
///
/// A query of only whitespace counts as blank: the user has not actually
/// filtered anything, so telling them nothing *matched* would be wrong.
#[must_use]
pub fn empty_state_copy(query: &str) -> EmptyStateCopy {
    if query.trim().is_empty() {
        NO_RECENT_DOCUMENTS
    } else {
        NO_MATCHING_DOCUMENTS
    }
}

/// What a screen reader is told the recent list currently contains.
///
/// Bounded by construction: it reports a count, never row text, so a long
/// document title cannot leak into an announcement.
#[must_use]
pub fn list_value_text(query: &str, visible_count: usize) -> String {
    match visible_count {
        0 => empty_state_copy(query).accessible_name.to_string(),
        1 => "1 recent document".to_string(),
        count => format!("{count} recent documents"),
    }
}

/// Whether the row list is currently worth rebuilding.
///
/// Rebuilding walks the recent entries and the open-document set, so it is
/// skipped while nothing can see the result. The caller marks the rows dirty
/// instead, and the next show rebuilds them.
#[must_use]
pub fn should_rebuild_rows(menu_button_active: bool, popover_visible: bool) -> bool {
    menu_button_active || popover_visible
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_query_reports_an_empty_history_not_an_empty_filter() {
        for query in ["", "   ", "\t", "\n  "] {
            assert_eq!(
                empty_state_copy(query),
                NO_RECENT_DOCUMENTS,
                "query {query:?} is blank and must not claim a filter excluded rows"
            );
        }
    }

    #[test]
    fn a_real_query_reports_an_empty_filter() {
        for query in ["a", "  notes ", "readme.md"] {
            assert_eq!(empty_state_copy(query), NO_MATCHING_DOCUMENTS);
        }
    }

    #[test]
    fn the_two_empty_states_say_different_things() {
        // The regression this guards: collapsing the two into one still renders
        // a plausible-looking empty page.
        assert_ne!(NO_RECENT_DOCUMENTS.title, NO_MATCHING_DOCUMENTS.title);
        assert_ne!(
            NO_RECENT_DOCUMENTS.accessible_name,
            NO_MATCHING_DOCUMENTS.accessible_name
        );
        assert_ne!(
            NO_RECENT_DOCUMENTS.accessible_description,
            NO_MATCHING_DOCUMENTS.accessible_description
        );
    }

    #[test]
    fn the_value_text_matches_the_empty_copy_when_nothing_is_visible() {
        assert_eq!(list_value_text("", 0), "No recent documents");
        assert_eq!(list_value_text("query", 0), "No matching recent documents");
        assert_eq!(
            list_value_text("   ", 0),
            "No recent documents",
            "a blank query must agree with the empty-state copy"
        );
    }

    #[test]
    fn the_value_text_is_singular_for_exactly_one_row() {
        assert_eq!(list_value_text("", 1), "1 recent document");
        assert_eq!(list_value_text("q", 1), "1 recent document");
    }

    #[test]
    fn the_value_text_is_plural_beyond_one_row() {
        assert_eq!(list_value_text("", 2), "2 recent documents");
        assert_eq!(list_value_text("", 17), "17 recent documents");
    }

    #[test]
    fn a_rebuild_happens_only_while_something_can_see_it() {
        assert!(should_rebuild_rows(true, false));
        assert!(should_rebuild_rows(false, true));
        assert!(should_rebuild_rows(true, true));
        assert!(
            !should_rebuild_rows(false, false),
            "an invisible popover must defer the walk rather than pay for it"
        );
    }
}
