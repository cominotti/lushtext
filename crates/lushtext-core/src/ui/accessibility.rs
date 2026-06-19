// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared GTK accessibility helpers for UI adapters.
//!
//! This module stays in the UI layer because it works directly with GTK's
//! `GtkAccessible` contract. It keeps product-facing names, descriptions,
//! states, row metadata, and announcements consistent without pulling GTK into
//! domain or service code.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use gtk4::prelude::*;

/// Maximum text length for app-owned accessibility announcements.
///
/// Announcements are for workflow context, not content export. Capping them
/// avoids flooding screen readers and protects private document/note text from
/// accidentally becoming an unbounded accessibility event.
pub const DEFAULT_ANNOUNCEMENT_LIMIT: usize = 240;

/// Debounce window for result-count announcements after a user is typing.
const DEBOUNCED_RESULTS_COOLDOWN: Duration = Duration::from_millis(500);
/// Minimum spacing for progress milestones from long-running workflows.
const PROGRESS_MILESTONE_COOLDOWN: Duration = Duration::from_secs(1);
/// Minimum spacing for repeated visible status updates with the same meaning.
const STATUS_UPDATE_COOLDOWN: Duration = Duration::from_secs(2);

/// Announcement lane for user-meaningful accessibility events.
///
/// The lane decides both priority and throttling. High-priority alerts bypass
/// throttling because errors and destructive confirmations should not be
/// hidden behind a previous status update.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AnnouncementLane {
    /// Debounced search or filter result summaries.
    DebouncedResults,
    /// Bounded milestones from indexing, refresh, or replace workflows.
    ProgressMilestone,
    /// Repeated non-blocking status text that should not chatter.
    StatusUpdate,
    /// High-priority alerts such as failed saves or destructive confirmations.
    Alert,
}

impl AnnouncementLane {
    /// Return the GTK announcement priority for this lane.
    #[must_use]
    pub fn priority(self) -> gtk4::AccessibleAnnouncementPriority {
        match self {
            Self::DebouncedResults | Self::ProgressMilestone | Self::StatusUpdate => {
                gtk4::AccessibleAnnouncementPriority::Medium
            }
            Self::Alert => gtk4::AccessibleAnnouncementPriority::High,
        }
    }

    /// Return the minimum interval before the same lane/key may announce again.
    #[must_use]
    pub fn cooldown(self) -> Duration {
        match self {
            Self::DebouncedResults => DEBOUNCED_RESULTS_COOLDOWN,
            Self::ProgressMilestone => PROGRESS_MILESTONE_COOLDOWN,
            Self::StatusUpdate => STATUS_UPDATE_COOLDOWN,
            Self::Alert => Duration::ZERO,
        }
    }

    fn bypasses_throttle(self) -> bool {
        matches!(self, Self::Alert)
    }
}

/// Per-surface throttle for repeated accessibility announcements.
///
/// Widgets keep one instance for the workflow they announce. The helper tracks
/// lane/key pairs instead of raw text so callers can use stable privacy-safe
/// keys such as `workspace-search-results` while keeping the spoken message
/// user-friendly.
#[derive(Debug, Default)]
pub struct AnnouncementThrottler {
    /// Last accepted announcement time per lane/key pair.
    last_by_key: RefCell<HashMap<String, Instant>>,
}

impl AnnouncementThrottler {
    /// Create a fresh announcement throttler with no recorded announcements.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Return whether this lane/key should announce at `now`, updating state
    /// only when the event is accepted.
    pub fn should_announce_at(&self, lane: AnnouncementLane, key: &str, now: Instant) -> bool {
        if lane.bypasses_throttle() {
            return true;
        }

        let storage_key = format!("{lane:?}:{key}");
        let mut last_by_key = self.last_by_key.borrow_mut();
        let Some(previous) = last_by_key.get(&storage_key).copied() else {
            last_by_key.insert(storage_key, now);
            return true;
        };

        if now.saturating_duration_since(previous) < lane.cooldown() {
            return false;
        }

        last_by_key.insert(storage_key, now);
        true
    }

    /// Return whether the lane/key has already accepted an announcement.
    ///
    /// Widget tests use this as a non-mutating probe for paths that must stay
    /// silent, such as info-only progress heartbeats.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn has_recent_announcement_for_test(&self, lane: AnnouncementLane, key: &str) -> bool {
        let storage_key = format!("{lane:?}:{key}");
        self.last_by_key.borrow().contains_key(&storage_key)
    }

    /// Announce through GTK when the lane/key is not currently throttled.
    ///
    /// The return value tells callers whether an announcement was actually
    /// emitted, which keeps widget tests and workflow logging deterministic.
    pub fn announce_if_allowed<W: IsA<gtk4::Accessible>>(
        &self,
        widget: &W,
        lane: AnnouncementLane,
        key: &str,
        message: &str,
    ) -> bool {
        if !self.should_announce_at(lane, key, Instant::now()) {
            return false;
        }

        announce_with_lane(widget, message, lane);
        true
    }
}

/// Bounded per-row metadata used by list and tree factories.
///
/// GTK recycles rows aggressively. Passing all row-specific accessibility data
/// through one value makes bind/unbind code refresh and clear the same fields
/// consistently.
#[derive(Clone, Copy, Debug)]
pub struct RowAccessibility<'a> {
    /// Spoken row label, usually a bounded title or action phrase.
    label: &'a str,
    /// Optional detail that helps distinguish similar rows.
    description: Option<&'a str>,
    /// Whether the row should expose a selected state.
    selected: Option<bool>,
    /// Optional position metadata for rows inside a known result set.
    position: Option<RowPosition>,
}

impl<'a> RowAccessibility<'a> {
    /// Start row metadata with the required label.
    #[must_use]
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            description: None,
            selected: None,
            position: None,
        }
    }

    /// Add a row description.
    #[must_use]
    pub fn description(mut self, description: &'a str) -> Self {
        self.description = Some(description);
        self
    }

    /// Add an explicit selected state.
    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = Some(selected);
        self
    }

    /// Add row position metadata for assistive technologies.
    #[must_use]
    pub fn position(mut self, pos_in_set: i32, set_size: i32) -> Self {
        self.position = Some(RowPosition {
            pos_in_set,
            set_size,
        });
        self
    }
}

/// One-based row position metadata for a finite result set.
#[derive(Clone, Copy, Debug)]
struct RowPosition {
    /// One-based position of this row inside the current set.
    pos_in_set: i32,
    /// Total number of rows in the current set.
    set_size: i32,
}

/// Assign an explicit accessible role to a widget.
pub fn set_role<W: IsA<gtk4::Accessible>>(widget: &W, role: gtk4::AccessibleRole) {
    widget.set_accessible_role(role);
}

/// Assign the accessible label used as the widget's primary spoken name.
pub fn set_label<W: IsA<gtk4::Accessible>>(widget: &W, label: &str) {
    widget.update_property(&[gtk4::accessible::Property::Label(label)]);
}

/// Assign the accessible description used as secondary spoken context.
pub fn set_description<W: IsA<gtk4::Accessible>>(widget: &W, description: &str) {
    widget.update_property(&[gtk4::accessible::Property::Description(description)]);
}

/// Assign accessible label and description together.
pub fn set_labelled_description<W: IsA<gtk4::Accessible>>(
    widget: &W,
    label: &str,
    description: &str,
) {
    widget.update_property(&[
        gtk4::accessible::Property::Label(label),
        gtk4::accessible::Property::Description(description),
    ]);
}

/// Assign a read-only accessible property when GTK defaults do not express it.
pub fn set_read_only<W: IsA<gtk4::Accessible>>(widget: &W, read_only: bool) {
    widget.update_property(&[gtk4::accessible::Property::ReadOnly(read_only)]);
}

/// Mark multiline text surfaces explicitly when their role alone is ambiguous.
pub fn set_multi_line<W: IsA<gtk4::Accessible>>(widget: &W, multi_line: bool) {
    widget.update_property(&[gtk4::accessible::Property::MultiLine(multi_line)]);
}

/// Assign keyboard shortcut text in GTK's accessible metadata.
pub fn set_key_shortcuts<W: IsA<gtk4::Accessible>>(widget: &W, shortcuts: &str) {
    widget.update_property(&[gtk4::accessible::Property::KeyShortcuts(shortcuts)]);
}

/// Mark controls that open popups or menus.
pub fn set_has_popup<W: IsA<gtk4::Accessible>>(widget: &W, has_popup: bool) {
    widget.update_property(&[gtk4::accessible::Property::HasPopup(has_popup)]);
}

/// Assign the current value text for compact value-like controls.
pub fn set_value_text<W: IsA<gtk4::Accessible>>(widget: &W, value_text: &str) {
    widget.update_property(&[gtk4::accessible::Property::ValueText(value_text)]);
}

/// Reset an accessible property to GTK's default value.
pub fn reset_property<W: IsA<gtk4::Accessible>>(widget: &W, property: gtk4::AccessibleProperty) {
    widget.reset_property(property);
}

/// Set or clear the accessible busy state.
pub fn set_busy<W: IsA<gtk4::Accessible>>(widget: &W, busy: bool) {
    if busy {
        widget.update_state(&[gtk4::accessible::State::Busy(true)]);
    } else {
        reset_state(widget, gtk4::AccessibleState::Busy);
    }
}

/// Set or clear the accessible disabled state when sensitivity alone is not
/// enough for the represented workflow state.
pub fn set_disabled<W: IsA<gtk4::Accessible>>(widget: &W, disabled: bool) {
    if disabled {
        widget.update_state(&[gtk4::accessible::State::Disabled(true)]);
    } else {
        reset_state(widget, gtk4::AccessibleState::Disabled);
    }
}

/// Set or clear the hidden state for alternate layouts that temporarily hide a surface.
pub fn set_hidden<W: IsA<gtk4::Accessible>>(widget: &W, hidden: bool) {
    if hidden {
        widget.update_state(&[gtk4::accessible::State::Hidden(true)]);
    } else {
        reset_state(widget, gtk4::AccessibleState::Hidden);
    }
}

/// Set or clear the invalid state for failed-load and validation surfaces.
pub fn set_invalid<W: IsA<gtk4::Accessible>>(widget: &W, invalid: bool) {
    if invalid {
        widget.update_state(&[gtk4::accessible::State::Invalid(
            gtk4::AccessibleInvalidState::True,
        )]);
    } else {
        reset_state(widget, gtk4::AccessibleState::Invalid);
    }
}

/// Set the expanded state for collapsible controls and rows.
pub fn set_expanded<W: IsA<gtk4::Accessible>>(widget: &W, expanded: Option<bool>) {
    widget.update_state(&[gtk4::accessible::State::Expanded(expanded)]);
}

/// Set the selected state for list rows and result items.
pub fn set_selected<W: IsA<gtk4::Accessible>>(widget: &W, selected: Option<bool>) {
    widget.update_state(&[gtk4::accessible::State::Selected(selected)]);
}

/// Set the pressed state for toggles when the visible state is app-owned.
pub fn set_pressed<W: IsA<gtk4::Accessible>>(widget: &W, pressed: bool) {
    let state = if pressed {
        gtk4::AccessibleTristate::True
    } else {
        gtk4::AccessibleTristate::False
    };
    widget.update_state(&[gtk4::accessible::State::Pressed(state)]);
}

/// Reset an accessible state to GTK's default value.
pub fn reset_state<W: IsA<gtk4::Accessible>>(widget: &W, state: gtk4::AccessibleState) {
    widget.reset_state(state);
}

/// Assign a labelled-by relation to one or more visible label widgets.
pub fn set_labelled_by<W: IsA<gtk4::Accessible>>(widget: &W, labels: &[&gtk4::Accessible]) {
    widget.update_relation(&[gtk4::accessible::Relation::LabelledBy(labels)]);
}

/// Assign a described-by relation to one or more visible description widgets.
pub fn set_described_by<W: IsA<gtk4::Accessible>>(widget: &W, descriptions: &[&gtk4::Accessible]) {
    widget.update_relation(&[gtk4::accessible::Relation::DescribedBy(descriptions)]);
}

/// Assign a controls relation from a control to the surface it shows or hides.
pub fn set_controls<W: IsA<gtk4::Accessible>>(widget: &W, targets: &[&gtk4::Accessible]) {
    widget.update_relation(&[gtk4::accessible::Relation::Controls(targets)]);
}

/// Reset an accessible relation to GTK's default value.
pub fn reset_relation<W: IsA<gtk4::Accessible>>(widget: &W, relation: gtk4::AccessibleRelation) {
    widget.reset_relation(relation);
}

/// Apply all item-specific metadata for a recycled row.
pub fn apply_row_accessibility<W: IsA<gtk4::Accessible>>(
    widget: &W,
    metadata: RowAccessibility<'_>,
) {
    let mut properties = vec![gtk4::accessible::Property::Label(metadata.label)];
    if let Some(description) = metadata.description {
        properties.push(gtk4::accessible::Property::Description(description));
    }
    widget.update_property(&properties);

    if let Some(selected) = metadata.selected {
        set_selected(widget, Some(selected));
    }

    if let Some(position) = metadata.position {
        widget.update_relation(&[
            gtk4::accessible::Relation::PosInSet(position.pos_in_set),
            gtk4::accessible::Relation::SetSize(position.set_size),
        ]);
    }
}

/// Clear row-specific metadata before GTK recycles a row for another item.
pub fn clear_row_accessibility<W: IsA<gtk4::Accessible>>(widget: &W) {
    reset_property(widget, gtk4::AccessibleProperty::Label);
    reset_property(widget, gtk4::AccessibleProperty::Description);
    reset_state(widget, gtk4::AccessibleState::Selected);
    reset_relation(widget, gtk4::AccessibleRelation::PosInSet);
    reset_relation(widget, gtk4::AccessibleRelation::SetSize);
}

/// Return a bounded announcement message, preserving UTF-8 character boundaries.
#[must_use]
pub fn bounded_announcement_text(message: &str, max_chars: usize) -> Cow<'_, str> {
    if max_chars == 0 {
        return Cow::Borrowed("");
    }

    if message.chars().count() <= max_chars {
        return Cow::Borrowed(message);
    }

    if max_chars <= 3 {
        return Cow::Owned(".".repeat(max_chars));
    }

    let mut bounded = message.chars().take(max_chars - 3).collect::<String>();
    bounded.push_str("...");
    Cow::Owned(bounded)
}

/// Announce a bounded workflow message through GTK.
pub fn announce<W: IsA<gtk4::Accessible>>(
    widget: &W,
    message: &str,
    priority: gtk4::AccessibleAnnouncementPriority,
) {
    let message = bounded_announcement_text(message, DEFAULT_ANNOUNCEMENT_LIMIT);
    widget.announce(message.as_ref(), priority);
}

/// Announce one workflow event through the shared lane-to-priority policy.
pub fn announce_with_lane<W: IsA<gtk4::Accessible>>(
    widget: &W,
    message: &str,
    lane: AnnouncementLane,
) {
    announce(widget, message, lane.priority());
}

#[cfg(feature = "test-utils")]
pub mod test_audit {
    //! Test-only assertions for GTK accessible metadata.
    //!
    //! Widget tests run with the AT bridge disabled, so these helpers use GTK's
    //! own metadata checks instead of querying a live AT-SPI tree.

    use gtk4::prelude::*;

    /// Expected accessible metadata for one widget under test.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct AccessibleAudit<'a> {
        /// Expected accessible role, when the test needs to pin it.
        role: Option<gtk4::AccessibleRole>,
        /// Accessible properties that must be present.
        properties: &'a [gtk4::AccessibleProperty],
        /// Accessible states that must be present.
        states: &'a [gtk4::AccessibleState],
        /// Accessible relations that must be present.
        relations: &'a [gtk4::AccessibleRelation],
    }

    impl<'a> AccessibleAudit<'a> {
        /// Start an empty audit and add role, properties, states, or relations
        /// with the builder methods below.
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        /// Require a specific accessible role.
        #[must_use]
        pub fn role(mut self, role: gtk4::AccessibleRole) -> Self {
            self.role = Some(role);
            self
        }

        /// Require accessible properties to be present.
        #[must_use]
        pub fn properties(mut self, properties: &'a [gtk4::AccessibleProperty]) -> Self {
            self.properties = properties;
            self
        }

        /// Require accessible states to be present.
        #[must_use]
        pub fn states(mut self, states: &'a [gtk4::AccessibleState]) -> Self {
            self.states = states;
            self
        }

        /// Require accessible relations to be present.
        #[must_use]
        pub fn relations(mut self, relations: &'a [gtk4::AccessibleRelation]) -> Self {
            self.relations = relations;
            self
        }

        /// Assert this audit against a GTK accessible widget.
        ///
        /// # Panics
        ///
        /// Panics when any requested role, property, state, or relation is
        /// missing from the widget under test.
        pub fn assert_on<W: IsA<gtk4::Accessible>>(&self, widget: &W) {
            if let Some(role) = self.role {
                assert!(
                    gtk4::test_accessible_has_role(widget, role),
                    "expected accessible role {role:?}"
                );
            }

            for property in self.properties {
                assert!(
                    gtk4::test_accessible_has_property(widget, *property),
                    "expected accessible property {property:?}"
                );
            }

            for state in self.states {
                assert!(
                    gtk4::test_accessible_has_state(widget, *state),
                    "expected accessible state {state:?}"
                );
            }

            for relation in self.relations {
                assert!(
                    gtk4::test_accessible_has_relation(widget, *relation),
                    "expected accessible relation {relation:?}"
                );
            }
        }
    }
}
