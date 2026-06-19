// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the LushtextStatusBar widget.

use std::time::{Duration, Instant};

use crate::common::{ensure_gtk_init, flush_after_delay};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::services::notifications::StatusMessage;
use lushtext_core::ui::accessibility::{AnnouncementLane, test_audit::AccessibleAudit};
use lushtext_core::ui::status_bar::{LushtextStatusBar, MessageKind};

// --- Construction ---

#[test]
fn test_new() {
    ensure_gtk_init();
    let _bar = LushtextStatusBar::new();
}

#[test]
fn test_default_equals_new() {
    ensure_gtk_init();
    let _bar: LushtextStatusBar = LushtextStatusBar::default();
}

#[test]
fn test_initially_no_message() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert_eq!(bar.imp().message_label.label().as_str(), "");
}

#[test]
fn test_status_controls_expose_accessibility_roles() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();

    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::ToggleButton)
        .properties(&[gtk4::AccessibleProperty::Label])
        .assert_on(&*bar.imp().sidebar_toggle_button);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Group)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&*bar.imp().metadata_box);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Button)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ValueText,
        ])
        .assert_on(&*bar.imp().line_ending_button);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::Button)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ValueText,
        ])
        .assert_on(&*bar.imp().encoding_button);

    bar.set_metadata_visible(false);
    assert!(gtk4::test_accessible_has_state(
        &*bar.imp().metadata_box,
        gtk4::AccessibleState::Hidden
    ));
    bar.set_metadata_visible(true);
    assert!(!gtk4::test_accessible_has_state(
        &*bar.imp().metadata_box,
        gtk4::AccessibleState::Hidden
    ));
}

fn status_message(text: &str, severity: MessageKind) -> StatusMessage {
    StatusMessage {
        text: text.to_string(),
        severity,
    }
}

/// Return whether the message area has any severity or animation-restart pulse state.
fn message_area_has_any_pulse_class(bar: &LushtextStatusBar) -> bool {
    let area = &bar.imp().message_area_box;
    area.has_css_class("status-pulse-info")
        || area.has_css_class("status-pulse-warning")
        || area.has_css_class("status-pulse-error")
        || area.has_css_class("status-pulse-a")
        || area.has_css_class("status-pulse-b")
}

// --- Message rendering ---

#[test]
fn test_message_area_contains_expanding_label() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    let area = &bar.imp().message_area_box;
    let label = &bar.imp().message_label;

    assert!(area.has_css_class("status-message-area"));
    assert!(label.has_css_class("status-message-label"));
    assert_eq!(
        label.parent().as_ref(),
        Some(area.upcast_ref::<gtk4::Widget>())
    );
    assert!(area.hexpands());
    assert!(label.hexpands());
    assert!((4..=8).contains(&area.margin_start()));
    assert_eq!(label.margin_start(), 12);
    assert_eq!(label.margin_top(), 6);
    assert_eq!(label.margin_bottom(), 6);
    assert_eq!(label.valign(), gtk4::Align::Center);
    assert_eq!(label.ellipsize(), gtk4::pango::EllipsizeMode::End);
    assert!(!label.wraps());
}

#[test]
fn test_long_message_ellipsizes_inside_single_message_lane() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    let message = "Saved a deeply nested workspace document with a very long path that must stay \
        inside the status bar message lane";

    bar.render_message(Some(&status_message(message, MessageKind::Info)));

    let label = &bar.imp().message_label;
    assert_eq!(label.label().as_str(), message);
    assert!(label.hexpands());
    assert_eq!(label.ellipsize(), gtk4::pango::EllipsizeMode::End);
    assert!(!label.wraps());
    assert_eq!(
        label.parent().as_ref(),
        Some(bar.imp().message_area_box.upcast_ref::<gtk4::Widget>())
    );
}

#[test]
fn test_render_message_sets_label() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.render_message(Some(&status_message("File saved", MessageKind::Info)));
    assert_eq!(bar.imp().message_label.label().as_str(), "File saved");
    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&*bar.imp().message_label);
}

#[test]
fn test_workflow_announcements_use_status_bar_throttling_policy() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();

    assert!(bar.announce_workflow_update(
        AnnouncementLane::StatusUpdate,
        "document-save",
        "File saved",
    ));
    assert!(
        !bar.imp()
            .status_announcement_throttler
            .should_announce_at(
                AnnouncementLane::StatusUpdate,
                "workflow:document-save",
                Instant::now()
            ),
        "workflow status updates should reuse the status-bar throttler"
    );

    assert!(bar.announce_workflow_update(
        AnnouncementLane::Alert,
        "document-save-failed",
        "Save failed",
    ));
    assert!(
        bar.imp()
            .status_announcement_throttler
            .should_announce_at(
                AnnouncementLane::Alert,
                "workflow:document-save-failed",
                Instant::now()
            ),
        "alert workflow updates should bypass repeated-status throttling"
    );
}

#[test]
fn test_visible_status_rendering_does_not_announce_info_or_flood_repeated_warnings() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();

    bar.render_message(Some(&status_message("File saved", MessageKind::Info)));
    assert!(
        !bar.imp()
            .status_announcement_throttler
            .has_recent_announcement_for_test(
                AnnouncementLane::StatusUpdate,
                "status:info:File saved"
            ),
        "routine info status text should stay visually visible without speech spam"
    );

    bar.render_message(Some(&status_message(
        "Save is still in progress",
        MessageKind::Warning,
    )));
    assert!(
        bar.imp()
            .status_announcement_throttler
            .has_recent_announcement_for_test(
                AnnouncementLane::StatusUpdate,
                "status:warning:Save is still in progress"
            ),
        "warning status text should be eligible for one spoken update"
    );

    bar.render_message(Some(&status_message(
        "Save is still in progress",
        MessageKind::Warning,
    )));
    assert!(
        !bar.imp()
            .status_announcement_throttler
            .should_announce_at(
                AnnouncementLane::StatusUpdate,
                "status:warning:Save is still in progress",
                Instant::now()
            ),
        "repeated visible warning status text should stay throttled"
    );
}

#[test]
fn test_render_message_info_has_css_class() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.render_message(Some(&status_message("Loaded", MessageKind::Info)));
    assert!(bar.imp().message_label.has_css_class("status-info"));
}

#[test]
fn test_render_message_warning_has_css_class() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.render_message(Some(&status_message("Caution", MessageKind::Warning)));
    assert!(bar.imp().message_label.has_css_class("status-warning"));
}

#[test]
fn test_render_message_error_has_css_class() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.render_message(Some(&status_message(
        "Permission denied",
        MessageKind::Error,
    )));
    assert!(bar.imp().message_label.has_css_class("status-error"));
}

#[test]
fn test_render_message_replaces_text() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.render_message(Some(&status_message("first", MessageKind::Info)));
    bar.render_message(Some(&status_message("second", MessageKind::Warning)));
    assert_eq!(bar.imp().message_label.label().as_str(), "second");
    AccessibleAudit::new()
        .properties(&[gtk4::AccessibleProperty::Description])
        .assert_on(&*bar.imp().message_label);
}

#[test]
fn test_render_message_replaces_css_class() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.render_message(Some(&status_message("first", MessageKind::Warning)));
    bar.render_message(Some(&status_message("second", MessageKind::Error)));
    assert!(bar.imp().message_label.has_css_class("status-error"));
    assert!(!bar.imp().message_label.has_css_class("status-warning"));
}

// --- Message clearing ---

#[test]
fn test_render_none_empties_label() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.render_message(Some(&status_message("test", MessageKind::Info)));
    bar.render_message(None);
    assert_eq!(bar.imp().message_label.label().as_str(), "");
    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&*bar.imp().message_label);
}

#[test]
fn test_render_none_removes_css_classes() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.render_message(Some(&status_message("test", MessageKind::Error)));
    bar.render_message(None);
    assert!(!bar.imp().message_label.has_css_class("status-error"));
    assert!(!bar.imp().message_label.has_css_class("status-warning"));
    assert!(!bar.imp().message_label.has_css_class("status-info"));
}

#[test]
fn test_render_none_clears_message_area_pulse() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.pulse_message_area(MessageKind::Info);
    assert!(message_area_has_any_pulse_class(&bar));

    bar.render_message(None);
    assert!(!message_area_has_any_pulse_class(&bar));
}

// --- Message-area pulse ---

#[test]
fn test_info_pulse_applies_to_message_area_not_label() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.pulse_message_area(MessageKind::Info);

    assert!(bar.imp().message_area_box.has_css_class("status-pulse-info"));
    assert!(bar.imp().message_area_box.has_css_class("status-pulse-a"));
    assert!(!bar.imp().message_label.has_css_class("status-pulse-info"));
    assert!(!bar.imp().sidebar_toggle_button.has_css_class("status-pulse-info"));
    assert!(!bar.imp().metadata_box.has_css_class("status-pulse-info"));
    assert_eq!(bar.imp().message_area_box.margin_start(), 6);
}

#[test]
fn test_warning_and_error_pulses_choose_severity_classes() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();

    bar.pulse_message_area(MessageKind::Warning);
    assert!(bar.imp().message_area_box.has_css_class("status-pulse-warning"));
    assert!(!bar.imp().message_area_box.has_css_class("status-pulse-info"));
    assert!(!bar.imp().message_area_box.has_css_class("status-pulse-error"));

    bar.pulse_message_area(MessageKind::Error);
    assert!(bar.imp().message_area_box.has_css_class("status-pulse-error"));
    assert!(!bar.imp().message_area_box.has_css_class("status-pulse-info"));
    assert!(!bar.imp().message_area_box.has_css_class("status-pulse-warning"));
}

#[test]
fn test_repeated_pulse_restarts_with_alternating_class() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();

    bar.pulse_message_area(MessageKind::Info);
    let first_used_a = bar.imp().message_area_box.has_css_class("status-pulse-a");
    let first_used_b = bar.imp().message_area_box.has_css_class("status-pulse-b");

    bar.pulse_message_area(MessageKind::Info);
    let second_used_a = bar.imp().message_area_box.has_css_class("status-pulse-a");
    let second_used_b = bar.imp().message_area_box.has_css_class("status-pulse-b");

    assert_ne!(first_used_a, second_used_a);
    assert_ne!(first_used_b, second_used_b);
    assert!(bar.imp().message_area_box.has_css_class("status-pulse-info"));
}

#[test]
fn test_message_area_pulse_cleans_up_after_duration() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.pulse_message_area(MessageKind::Error);
    assert!(message_area_has_any_pulse_class(&bar));

    // The delay intentionally exceeds the 420ms pulse cleanup duration.
    flush_after_delay(Duration::from_millis(500));
    assert!(!message_area_has_any_pulse_class(&bar));
}

// --- Metadata visibility ---

#[test]
fn test_metadata_visible_by_default() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert!(bar.imp().metadata_box.is_visible());
}

#[test]
fn test_set_metadata_hidden() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.set_metadata_visible(false);
    assert!(!bar.imp().metadata_box.is_visible());
}

#[test]
fn test_set_metadata_visible_again() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.set_metadata_visible(false);
    bar.set_metadata_visible(true);
    assert!(bar.imp().metadata_box.is_visible());
}

#[test]
fn test_hidden_metadata_keeps_empty_status_strip_structure() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();

    bar.set_metadata_visible(false);

    assert!(!bar.imp().metadata_box.is_visible());
    assert!(bar.imp().sidebar_toggle_button.is_visible());
    assert!(bar.imp().message_area_box.is_visible());
    assert!(bar.imp().message_area_box.hexpands());
    assert_eq!(bar.imp().message_area_box.margin_start(), 6);
    assert_eq!(bar.imp().message_label.label().as_str(), "");
}

// --- Metadata controls ---

#[test]
fn test_encoding_button_shows_utf8() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert_eq!(bar.imp().encoding_button.label().as_deref(), Some("UTF-8"));
}

#[test]
fn test_line_ending_button_shows_lf() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert_eq!(bar.imp().line_ending_button.label().as_deref(), Some("LF"));
}

#[test]
fn test_editorconfig_separator_hidden_by_default() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert!(!bar.imp().editorconfig_separator.is_visible());
}

#[test]
fn test_set_editorconfig_active_shows_badge_and_separator() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.set_editorconfig_active(true);
    assert!(bar.imp().editorconfig_label.is_visible());
    assert!(bar.imp().editorconfig_separator.is_visible());
}

#[test]
fn test_set_editorconfig_inactive_hides_badge_and_separator() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    bar.set_editorconfig_active(true);
    bar.set_editorconfig_active(false);
    assert!(!bar.imp().editorconfig_label.is_visible());
    assert!(!bar.imp().editorconfig_separator.is_visible());
}

#[test]
fn test_metadata_control_updates_refresh_accessible_value_text() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();

    bar.set_line_ending_label("CRLF");
    bar.set_encoding_label("UTF-16 LE");

    assert_eq!(bar.imp().line_ending_button.label().as_deref(), Some("CRLF"));
    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ValueText,
        ])
        .assert_on(&*bar.imp().line_ending_button);
    assert_eq!(
        bar.imp().encoding_button.label().as_deref(),
        Some("UTF-16 LE")
    );
    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
            gtk4::AccessibleProperty::ValueText,
        ])
        .assert_on(&*bar.imp().encoding_button);
}

#[test]
fn test_metadata_controls_keep_compact_readable_chrome() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    let imp = bar.imp();

    assert_eq!(imp.editorconfig_label.margin_top(), 6);
    assert_eq!(imp.editorconfig_label.margin_bottom(), 6);
    assert_eq!(imp.editorconfig_label.valign(), gtk4::Align::Center);
    assert_eq!(imp.editorconfig_separator.margin_top(), 8);
    assert_eq!(imp.editorconfig_separator.margin_bottom(), 8);

    for button in [&imp.line_ending_button, &imp.encoding_button] {
        assert!(button.has_css_class("status-metadata-control"));
        assert_eq!(button.margin_top(), 3);
        assert_eq!(button.margin_bottom(), 3);
        assert_eq!(button.valign(), gtk4::Align::Center);
    }
}

// --- Sidebar toggle button ---

#[test]
fn test_sidebar_toggle_button_exists() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    let _button = &bar.imp().sidebar_toggle_button;
}

#[test]
fn test_sidebar_toggle_button_icon() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert_eq!(
        bar.imp()
            .sidebar_toggle_button
            .icon_name()
            .expect("expected operation to succeed")
            .as_str(),
        "sidebar-show-symbolic"
    );
}

#[test]
fn test_sidebar_toggle_button_is_flat() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert!(bar.imp().sidebar_toggle_button.has_css_class("flat"));
}

#[test]
fn test_sidebar_toggle_button_has_tooltip() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    let tooltip = bar
        .imp()
        .sidebar_toggle_button
        .tooltip_text()
        .expect("expected operation to succeed");
    assert!(tooltip.contains("Sidebar"));
}

#[test]
fn test_sidebar_toggle_button_action_name() {
    ensure_gtk_init();
    let bar = LushtextStatusBar::new();
    assert_eq!(
        bar.imp()
            .sidebar_toggle_button
            .action_name()
            .expect("expected operation to succeed")
            .as_str(),
        "win.toggle-sidebar"
    );
}
