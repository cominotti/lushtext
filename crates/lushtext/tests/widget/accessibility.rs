// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget tests for shared GTK accessibility helpers.

use crate::common::ensure_gtk_init;
use gtk4::prelude::*;
use lushtext_core::ui::accessibility::{
    self, AnnouncementLane, AnnouncementThrottler, RowAccessibility,
    test_audit::AccessibleAudit,
};
use std::time::{Duration, Instant};

#[test]
fn test_label_description_and_role_helpers_set_gtk_accessible_metadata() {
    ensure_gtk_init();
    let button = gtk4::Button::new();

    accessibility::set_role(&button, gtk4::AccessibleRole::Button);
    accessibility::set_labelled_description(&button, "Run search", "Search the workspace");

    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Button)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&button);
}

#[test]
fn test_value_text_helper_sets_current_accessible_value() {
    ensure_gtk_init();
    let button = gtk4::Button::new();

    accessibility::set_label(&button, "Choose text encoding");
    accessibility::set_read_only(&button, true);
    accessibility::set_multi_line(&button, false);
    accessibility::set_key_shortcuts(&button, "Ctrl+E");
    accessibility::set_has_popup(&button, true);
    accessibility::set_value_text(&button, "UTF-16 LE");

    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::ReadOnly,
            gtk4::AccessibleProperty::MultiLine,
            gtk4::AccessibleProperty::KeyShortcuts,
            gtk4::AccessibleProperty::HasPopup,
            gtk4::AccessibleProperty::ValueText,
        ])
        .assert_on(&button);

    accessibility::reset_property(&button, gtk4::AccessibleProperty::ReadOnly);
    accessibility::reset_property(&button, gtk4::AccessibleProperty::MultiLine);
    accessibility::reset_property(&button, gtk4::AccessibleProperty::KeyShortcuts);
    accessibility::reset_property(&button, gtk4::AccessibleProperty::HasPopup);
    accessibility::reset_property(&button, gtk4::AccessibleProperty::ValueText);
    assert!(!gtk4::test_accessible_has_property(
        &button,
        gtk4::AccessibleProperty::ReadOnly
    ));
    assert!(!gtk4::test_accessible_has_property(
        &button,
        gtk4::AccessibleProperty::MultiLine
    ));
    assert!(!gtk4::test_accessible_has_property(
        &button,
        gtk4::AccessibleProperty::KeyShortcuts
    ));
    assert!(!gtk4::test_accessible_has_property(
        &button,
        gtk4::AccessibleProperty::HasPopup
    ));
    assert!(!gtk4::test_accessible_has_property(
        &button,
        gtk4::AccessibleProperty::ValueText
    ));
}

#[test]
fn test_state_helpers_update_and_reset_accessible_state() {
    ensure_gtk_init();
    let button = gtk4::Button::new();

    accessibility::set_busy(&button, true);
    assert!(gtk4::test_accessible_has_state(
        &button,
        gtk4::AccessibleState::Busy
    ));

    accessibility::set_busy(&button, false);
    assert!(!gtk4::test_accessible_has_state(
        &button,
        gtk4::AccessibleState::Busy
    ));

    accessibility::set_disabled(&button, true);
    accessibility::set_hidden(&button, true);
    accessibility::set_invalid(&button, true);
    accessibility::set_expanded(&button, Some(true));
    accessibility::set_selected(&button, Some(true));
    accessibility::set_pressed(&button, true);
    AccessibleAudit::new()
        .states(&[
            gtk4::AccessibleState::Disabled,
            gtk4::AccessibleState::Hidden,
            gtk4::AccessibleState::Invalid,
            gtk4::AccessibleState::Expanded,
            gtk4::AccessibleState::Selected,
            gtk4::AccessibleState::Pressed,
        ])
        .assert_on(&button);

    accessibility::set_disabled(&button, false);
    accessibility::set_hidden(&button, false);
    accessibility::set_invalid(&button, false);
    accessibility::set_expanded(&button, None);
    accessibility::set_selected(&button, None);
    accessibility::reset_state(&button, gtk4::AccessibleState::Pressed);
    for state in [
        gtk4::AccessibleState::Busy,
        gtk4::AccessibleState::Disabled,
        gtk4::AccessibleState::Hidden,
        gtk4::AccessibleState::Invalid,
        gtk4::AccessibleState::Expanded,
        gtk4::AccessibleState::Selected,
        gtk4::AccessibleState::Pressed,
    ] {
        assert!(
            !gtk4::test_accessible_has_state(&button, state),
            "expected accessible state {state:?} to be reset"
        );
    }
}

#[test]
fn test_relation_helpers_update_and_reset_accessible_relation() {
    ensure_gtk_init();
    let label = gtk4::Label::new(Some("Workspace"));
    let description = gtk4::Label::new(Some("Current workspace filter"));
    let entry = gtk4::Entry::new();
    let results = gtk4::ListBox::new();
    let labels = [label.upcast_ref::<gtk4::Accessible>()];
    let descriptions = [description.upcast_ref::<gtk4::Accessible>()];
    let controls = [results.upcast_ref::<gtk4::Accessible>()];

    accessibility::set_labelled_by(&entry, &labels);
    accessibility::set_described_by(&entry, &descriptions);
    accessibility::set_controls(&entry, &controls);
    assert!(gtk4::test_accessible_has_relation(
        &entry,
        gtk4::AccessibleRelation::LabelledBy
    ));
    assert!(gtk4::test_accessible_has_relation(
        &entry,
        gtk4::AccessibleRelation::DescribedBy
    ));
    assert!(gtk4::test_accessible_has_relation(
        &entry,
        gtk4::AccessibleRelation::Controls
    ));

    accessibility::reset_relation(&entry, gtk4::AccessibleRelation::LabelledBy);
    accessibility::reset_relation(&entry, gtk4::AccessibleRelation::DescribedBy);
    accessibility::reset_relation(&entry, gtk4::AccessibleRelation::Controls);
    assert!(!gtk4::test_accessible_has_relation(
        &entry,
        gtk4::AccessibleRelation::LabelledBy
    ));
    assert!(!gtk4::test_accessible_has_relation(
        &entry,
        gtk4::AccessibleRelation::DescribedBy
    ));
    assert!(!gtk4::test_accessible_has_relation(
        &entry,
        gtk4::AccessibleRelation::Controls
    ));
}

#[test]
fn test_row_accessibility_helper_applies_and_clears_recycled_metadata() {
    ensure_gtk_init();
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);

    accessibility::apply_row_accessibility(
        &row,
        RowAccessibility::new("Open recent document notes.md")
            .description("Recent document in the current workspace")
            .selected(true)
            .position(2, 5),
    );

    assert!(gtk4::test_accessible_has_property(
        &row,
        gtk4::AccessibleProperty::Label
    ));
    assert!(gtk4::test_accessible_has_property(
        &row,
        gtk4::AccessibleProperty::Description
    ));
    assert!(gtk4::test_accessible_has_state(
        &row,
        gtk4::AccessibleState::Selected
    ));
    assert!(gtk4::test_accessible_has_relation(
        &row,
        gtk4::AccessibleRelation::PosInSet
    ));
    assert!(gtk4::test_accessible_has_relation(
        &row,
        gtk4::AccessibleRelation::SetSize
    ));

    accessibility::apply_row_accessibility(&row, RowAccessibility::new("Open recent document"));
    assert!(gtk4::test_accessible_has_property(
        &row,
        gtk4::AccessibleProperty::Label
    ));
    assert!(!gtk4::test_accessible_has_property(
        &row,
        gtk4::AccessibleProperty::Description
    ));
    assert!(!gtk4::test_accessible_has_state(
        &row,
        gtk4::AccessibleState::Selected
    ));
    assert!(!gtk4::test_accessible_has_relation(
        &row,
        gtk4::AccessibleRelation::PosInSet
    ));
    assert!(!gtk4::test_accessible_has_relation(
        &row,
        gtk4::AccessibleRelation::SetSize
    ));

    accessibility::clear_row_accessibility(&row);
    assert!(!gtk4::test_accessible_has_property(
        &row,
        gtk4::AccessibleProperty::Label
    ));
    assert!(!gtk4::test_accessible_has_property(
        &row,
        gtk4::AccessibleProperty::Description
    ));
    assert!(!gtk4::test_accessible_has_state(
        &row,
        gtk4::AccessibleState::Selected
    ));
    assert!(!gtk4::test_accessible_has_relation(
        &row,
        gtk4::AccessibleRelation::PosInSet
    ));
    assert!(!gtk4::test_accessible_has_relation(
        &row,
        gtk4::AccessibleRelation::SetSize
    ));
}

#[test]
fn test_bounded_announcement_text_preserves_utf8_and_caps_length() {
    let bounded = accessibility::bounded_announcement_text("abcde", 4);
    assert_eq!(bounded, "a...");

    let bounded = accessibility::bounded_announcement_text("åßçde", 4);
    assert_eq!(bounded, "å...");

    let tiny = accessibility::bounded_announcement_text("abcdef", 2);
    assert_eq!(tiny, "..");
}

#[test]
fn test_announcement_throttler_suppresses_repeated_status_but_not_alerts() {
    let throttler = AnnouncementThrottler::new();
    let now = Instant::now();

    assert!(throttler.should_announce_at(AnnouncementLane::StatusUpdate, "save", now));
    assert!(!throttler.should_announce_at(
        AnnouncementLane::StatusUpdate,
        "save",
        now + Duration::from_millis(100)
    ));
    assert!(throttler.should_announce_at(
        AnnouncementLane::StatusUpdate,
        "save",
        now + AnnouncementLane::StatusUpdate.cooldown() + Duration::from_millis(1)
    ));

    assert!(throttler.should_announce_at(
        AnnouncementLane::Alert,
        "save",
        now + Duration::from_millis(110)
    ));
    assert!(throttler.should_announce_at(
        AnnouncementLane::Alert,
        "save",
        now + Duration::from_millis(120)
    ));
}

#[test]
fn test_announcement_throttler_suppresses_typing_progress_and_status_floods() {
    let throttler = AnnouncementThrottler::new();
    let now = Instant::now();

    assert!(throttler.should_announce_at(
        AnnouncementLane::DebouncedResults,
        "editor-search-results",
        now
    ));
    for offset_ms in [50, 100, 150, 200] {
        assert!(
            !throttler.should_announce_at(
                AnnouncementLane::DebouncedResults,
                "editor-search-results",
                now + Duration::from_millis(offset_ms),
            ),
            "rapid typing result updates should stay inside the debounce window"
        );
    }
    assert!(throttler.should_announce_at(
        AnnouncementLane::DebouncedResults,
        "editor-search-results",
        now + AnnouncementLane::DebouncedResults.cooldown() + Duration::from_millis(1)
    ));

    assert!(throttler.should_announce_at(
        AnnouncementLane::ProgressMilestone,
        "workspace-search-progress",
        now
    ));
    assert!(!throttler.should_announce_at(
        AnnouncementLane::ProgressMilestone,
        "workspace-search-progress",
        now + Duration::from_millis(500)
    ));

    assert!(throttler.should_announce_at(
        AnnouncementLane::StatusUpdate,
        "status:warning:Save is still in progress",
        now
    ));
    assert!(!throttler.should_announce_at(
        AnnouncementLane::StatusUpdate,
        "status:warning:Save is still in progress",
        now + Duration::from_millis(500)
    ));
}
