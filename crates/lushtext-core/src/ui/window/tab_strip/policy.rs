// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: pure policy for `WFR-TAB-STRIP`.
//!
//! The tab strip's decisions are all about one invariant that no widget call
//! expresses: **`AdwTabView` keeps pinned pages in a contiguous leading
//! segment**, and every target selection, move bound, and enabled state follows
//! from that. Reasoning about it through live `TabPage` objects is how a
//! bulk close comes to include a pinned tab in one edge case and not another;
//! reasoning about it over a `[TabLayoutEntry]` makes each rule one testable
//! function.
//!
//! This module imports no toolkit crate, which is what keeps it inside the
//! `ui/**/policy.rs` mutation scope.

/// Minimal tab-layout snapshot the target-selection rules operate over.
///
/// One boolean per visual position, in visual order. Everything else about a
/// page — its title, its editor, its modified state — is deliberately absent,
/// because none of it affects which positions are eligible.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TabLayoutEntry {
    /// Whether the page lives in the pinned leading segment.
    pub pinned: bool,
}

/// Return every unpinned page except the target itself.
///
/// Pinned tabs are excluded rather than merely deprioritised: "Close Other
/// Tabs" that closed a pinned tab would discard the one thing pinning is for.
#[must_use]
pub fn eligible_close_other_positions(
    layout: &[TabLayoutEntry],
    target_index: usize,
) -> Vec<usize> {
    if target_index >= layout.len() {
        return Vec::new();
    }

    layout
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| (index != target_index && !entry.pinned).then_some(index))
        .collect()
}

/// Return every unpinned page strictly after the target.
#[must_use]
pub fn eligible_close_right_positions(
    layout: &[TabLayoutEntry],
    target_index: usize,
) -> Vec<usize> {
    if target_index >= layout.len() {
        return Vec::new();
    }

    ((target_index + 1)..layout.len())
        .filter(|&index| !layout[index].pinned)
        .collect()
}

/// Whether the target can move one slot toward the pinned edge.
///
/// A pinned tab moves inside the pinned segment; an unpinned tab may not move
/// into it, so its floor is the first unpinned position rather than zero.
#[must_use]
pub fn can_move_left(layout: &[TabLayoutEntry], target_index: usize) -> bool {
    if target_index >= layout.len() {
        return false;
    }

    let first_unpinned = first_unpinned_index(layout);
    if layout[target_index].pinned {
        target_index > 0
    } else {
        target_index > first_unpinned
    }
}

/// Whether the target can move one slot away from the pinned edge.
#[must_use]
pub fn can_move_right(layout: &[TabLayoutEntry], target_index: usize) -> bool {
    if target_index >= layout.len() {
        return false;
    }

    let first_unpinned = first_unpinned_index(layout);
    if layout[target_index].pinned {
        target_index + 1 < first_unpinned
    } else {
        target_index + 1 < layout.len()
    }
}

/// Count the contiguous leading pinned segment.
#[must_use]
pub fn first_unpinned_index(layout: &[TabLayoutEntry]) -> usize {
    layout.iter().take_while(|entry| entry.pinned).count()
}

/// The pin menu item's label for a target that is or is not already pinned.
#[must_use]
pub const fn pin_menu_label(target_is_pinned: bool) -> &'static str {
    if target_is_pinned { "Unpin" } else { "Pin" }
}

/// The status message published after a pin toggle.
#[must_use]
pub fn pin_toggle_message(title: &str, now_pinned: bool) -> String {
    if now_pinned {
        format!("Pinned {title}")
    } else {
        format!("Unpinned {title}")
    }
}

/// The status message published after a successful reorder.
#[must_use]
pub fn reorder_message(title: &str, toward_pinned_edge: bool) -> String {
    if toward_pinned_edge {
        format!("Moved {title} left")
    } else {
        format!("Moved {title} right")
    }
}

/// The status message published after an authorized bulk close.
#[must_use]
pub fn bulk_close_message(close_count: usize) -> String {
    format!(
        "Closed {close_count} tab{}",
        if close_count == 1 { "" } else { "s" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pinned_layout(flags: &[bool]) -> Vec<TabLayoutEntry> {
        flags
            .iter()
            .copied()
            .map(|pinned| TabLayoutEntry { pinned })
            .collect()
    }

    #[test]
    fn test_close_other_targets_only_unpinned_tabs() {
        let layout = pinned_layout(&[true, false, false, true, false]);
        assert_eq!(eligible_close_other_positions(&layout, 1), vec![2, 4]);
        assert_eq!(eligible_close_other_positions(&layout, 0), vec![1, 2, 4]);
    }

    #[test]
    fn test_close_right_targets_skip_pinned_segment() {
        let layout = pinned_layout(&[true, true, false, false]);
        assert_eq!(eligible_close_right_positions(&layout, 0), vec![2, 3]);
        assert_eq!(eligible_close_right_positions(&layout, 2), vec![3]);
        assert!(eligible_close_right_positions(&layout, 3).is_empty());
    }

    #[test]
    fn test_move_boundaries_respect_pinned_segments() {
        let layout = pinned_layout(&[true, true, false, false]);
        assert!(!can_move_left(&layout, 0));
        assert!(can_move_left(&layout, 1));
        assert!(
            !can_move_left(&layout, 2),
            "the first unpinned tab must not move into the pinned segment"
        );
        assert!(can_move_left(&layout, 3));

        assert!(can_move_right(&layout, 0));
        assert!(
            !can_move_right(&layout, 1),
            "the last pinned tab must not move out of the pinned segment"
        );
        assert!(can_move_right(&layout, 2));
        assert!(!can_move_right(&layout, 3));
    }

    #[test]
    fn test_out_of_range_targets_are_safe_noops() {
        let layout = pinned_layout(&[false, false]);
        assert!(eligible_close_other_positions(&layout, 9).is_empty());
        assert!(eligible_close_right_positions(&layout, 9).is_empty());
        assert!(!can_move_left(&layout, 9));
        assert!(!can_move_right(&layout, 9));
    }

    #[test]
    fn test_first_unpinned_index_counts_only_the_leading_run() {
        assert_eq!(first_unpinned_index(&pinned_layout(&[])), 0);
        assert_eq!(first_unpinned_index(&pinned_layout(&[false, true])), 0);
        assert_eq!(
            first_unpinned_index(&pinned_layout(&[true, true, false])),
            2
        );
        assert_eq!(
            first_unpinned_index(&pinned_layout(&[true, false, true])),
            1,
            "a pinned page after an unpinned one is not part of the leading run"
        );
    }

    #[test]
    fn test_labels_and_messages_say_what_happened() {
        assert_eq!(pin_menu_label(true), "Unpin");
        assert_eq!(pin_menu_label(false), "Pin");
        assert_eq!(pin_toggle_message("notes.md", true), "Pinned notes.md");
        assert_eq!(pin_toggle_message("notes.md", false), "Unpinned notes.md");
        assert_eq!(reorder_message("a.rs", true), "Moved a.rs left");
        assert_eq!(reorder_message("a.rs", false), "Moved a.rs right");
    }

    #[test]
    fn test_bulk_close_message_is_singular_for_exactly_one_tab() {
        assert_eq!(bulk_close_message(1), "Closed 1 tab");
        assert_eq!(bulk_close_message(0), "Closed 0 tabs");
        assert_eq!(bulk_close_message(7), "Closed 7 tabs");
    }
}
