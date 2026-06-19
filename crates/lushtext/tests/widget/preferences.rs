// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for LushtextPreferences dialog.

use crate::common::{ensure_gtk_init, fixture, fs_metadata, isolated_data_dir, wait_until};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::config::{self, keys};
use lushtext_core::services::format_upgrade::{
    FORMAT_UPGRADE_BACKUP_DIR, test_support::ConverterRegistry,
};
use lushtext_core::services::json_format::{
    KIND_BOOKMARK_SIDECAR, KIND_SESSION, SUPPORTED_JSON_VERSION,
};
use lushtext_core::ui::accessibility::{AnnouncementLane, test_audit::AccessibleAudit};
use lushtext_core::ui::preferences::LushtextPreferences;
use libadwaita::prelude::*;
use serde_json::json;
use std::time::{Duration, Instant};

const FAST_SCAN_VISIBLE_DWELL: Duration = Duration::from_millis(900);

fn has_accessible_role(root: &impl IsA<gtk4::Widget>, role: gtk4::AccessibleRole) -> bool {
    let mut stack = vec![root.as_ref().clone()];
    while let Some(widget) = stack.pop() {
        if widget.accessible_role() == role {
            return true;
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            stack.push(current.clone());
            child = current.next_sibling();
        }
    }
    false
}

fn find_scrolled_window(root: &impl IsA<gtk4::Widget>) -> Option<gtk4::ScrolledWindow> {
    let mut stack = vec![root.as_ref().clone()];
    while let Some(widget) = stack.pop() {
        if let Ok(scroller) = widget.clone().downcast::<gtk4::ScrolledWindow>() {
            return Some(scroller);
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            stack.push(current.clone());
            child = current.next_sibling();
        }
    }
    None
}

fn list_box_child_count(list: &gtk4::ListBox) -> usize {
    let mut count = 0;
    let mut child = list.first_child();
    while let Some(current) = child {
        count += 1;
        child = current.next_sibling();
    }
    count
}

#[test]
fn test_new() {
    ensure_gtk_init();
    let _prefs = LushtextPreferences::new();
}

#[test]
fn test_default_equals_new() {
    ensure_gtk_init();
    let _prefs: LushtextPreferences = LushtextPreferences::default();
}

#[test]
fn test_preferences_controls_expose_accessibility_roles() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert_eq!(
        imp.style_scheme_row.accessible_role(),
        gtk4::AccessibleRole::ComboBox
    );
    assert_eq!(
        imp.workspace_sidebar_width_row.accessible_role(),
        gtk4::AccessibleRole::ComboBox
    );
    assert_eq!(
        imp.word_wrap_row.accessible_role(),
        gtk4::AccessibleRole::Switch
    );
    assert_eq!(
        imp.tab_width_row.accessible_role(),
        gtk4::AccessibleRole::Group
    );
    assert!(
        has_accessible_role(&*imp.tab_width_row, gtk4::AccessibleRole::SpinButton),
        "numeric tab width row should expose an internal spin button"
    );
    assert!(
        has_accessible_role(&*imp.font_button, gtk4::AccessibleRole::Button),
        "font chooser should expose a button role"
    );
    assert!(
        has_accessible_role(&*imp.transparency_button, gtk4::AccessibleRole::Button),
        "background opacity menu should expose a button role"
    );
    assert_eq!(
        imp.data_scan_button.accessible_role(),
        gtk4::AccessibleRole::Button
    );
    assert_eq!(
        imp.data_convert_button.accessible_role(),
        gtk4::AccessibleRole::Button
    );
    AccessibleAudit::new()
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&*imp.transparency_button);
    AccessibleAudit::new()
        .role(gtk4::AccessibleRole::List)
        .properties(&[
            gtk4::AccessibleProperty::Label,
            gtk4::AccessibleProperty::Description,
        ])
        .assert_on(&imp.data_details_list);
}

// --- GSettings binding tests ---

#[test]
fn test_word_wrap_row_bound_to_settings() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    // Schema default: word-wrap = true
    assert!(imp.word_wrap_row.is_active());
}

#[test]
fn test_show_line_numbers_row_bound_to_settings() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    // Schema default: show-line-numbers = true
    assert!(imp.show_line_numbers_row.is_active());
}

#[test]
fn test_highlight_line_row_bound_to_settings() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert!(imp.highlight_line_row.is_active());
}

#[test]
fn test_show_minimap_row_bound_to_settings() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert!(!imp.show_minimap_row.is_active());
}

#[test]
fn test_minimap_long_line_markers_row_defaults_off_and_updates_setting() {
    ensure_gtk_init();
    let settings = gio::Settings::new(config::APP_ID);
    settings.reset(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE);

    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert!(!settings.boolean(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE));
    assert!(!imp.minimap_long_line_markers_row.is_active());

    imp.minimap_long_line_markers_row.set_active(true);
    while glib::MainContext::default().iteration(false) {}

    assert!(settings.boolean(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE));
}

#[test]
fn test_minimap_controls_are_grouped_on_editor_page() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert_eq!(imp.minimap_group.title().as_str(), "Minimap");
    assert!(imp.show_minimap_row.is_ancestor(&*imp.minimap_group));
    assert!(
        imp.minimap_long_line_markers_row
            .is_ancestor(&*imp.minimap_group)
    );
}

#[test]
fn test_focus_mode_column_width_row_bound_to_settings() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert_eq!(imp.focus_mode_target_columns_row.value(), 80.0);
}

#[test]
fn test_focus_mode_typewriter_scrolling_defaults_off() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert!(!imp.focus_mode_typewriter_scrolling_row.is_active());
}

#[test]
fn test_insert_spaces_row_bound_to_settings() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert!(imp.insert_spaces_row.is_active());
}

#[test]
fn test_use_system_font_row_bound_to_settings() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert!(imp.use_system_font_row.is_active());
}

#[test]
fn test_workspace_auto_collapse_row_bound_to_settings() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert!(imp.workspace_auto_collapse_row.is_active());
}

#[test]
fn test_workspace_empty_folder_lookahead_cap_row_bound_to_settings() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert_eq!(imp.workspace_empty_folder_lookahead_cap_row.value(), 1000.0);
}

#[test]
fn test_custom_font_row_disabled_when_system_font() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    // System font is on by default → custom font row should be insensitive
    assert!(!imp.custom_font_row.is_sensitive());
}

#[test]
fn test_color_scheme_row_populated() {
    ensure_gtk_init();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    // The combo row model should have been populated with available schemes
    assert!(imp.style_scheme_row.model().is_some());
}

#[test]
fn test_background_opacity_row_is_visible_with_default_percentage() {
    ensure_gtk_init();
    let settings = gio::Settings::new(config::APP_ID);
    settings
        .set_double(keys::TAB_CONTENT_OPACITY, 1.0)
        .expect("reset tab-content-opacity");

    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert!(imp.transparency_row.is_visible());
    assert_eq!(imp.transparency_row.title().as_str(), "Background Opacity");
    assert_eq!(
        imp.transparency_row.subtitle().as_deref(),
        Some("Lower values make editor and Markdown preview backgrounds more transparent")
    );
    assert_eq!(imp.transparency_adjustment.value(), 1.0);
    assert_eq!(imp.transparency_label.label().as_str(), "100%");
    AccessibleAudit::new()
        .properties(&[gtk4::AccessibleProperty::ValueText])
        .assert_on(&*imp.transparency_button);
}

#[test]
fn test_background_opacity_row_updates_setting_and_label() {
    ensure_gtk_init();
    let settings = gio::Settings::new(config::APP_ID);
    settings
        .set_double(keys::TAB_CONTENT_OPACITY, 1.0)
        .expect("reset tab-content-opacity");

    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();
    imp.transparency_adjustment.set_value(0.85);

    while glib::MainContext::default().iteration(false) {}

    assert!((settings.double(keys::TAB_CONTENT_OPACITY) - 0.85).abs() < f64::EPSILON);
    assert_eq!(imp.transparency_label.label().as_str(), " 85%");
    AccessibleAudit::new()
        .properties(&[gtk4::AccessibleProperty::ValueText])
        .assert_on(&*imp.transparency_button);
    assert_ne!(
        imp.transparency_label.label().as_str(),
        " 15%",
        "the row displays opacity, not inverted transparency"
    );
}

#[test]
fn test_background_opacity_row_restores_persisted_percentage() {
    ensure_gtk_init();
    let settings = gio::Settings::new(config::APP_ID);
    settings
        .set_double(keys::TAB_CONTENT_OPACITY, 0.65)
        .expect("set tab-content-opacity");

    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    assert!((imp.transparency_adjustment.value() - 0.65).abs() < f64::EPSILON);
    assert_eq!(imp.transparency_label.label().as_str(), " 65%");
}

#[test]
fn test_workspace_sidebar_width_row_lists_all_presets() {
    ensure_gtk_init();
    let settings = gio::Settings::new(config::APP_ID);
    settings
        .set_double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION, 0.3)
        .expect("set comfy preset");

    let prefs = LushtextPreferences::new();
    let model = prefs
        .imp()
        .workspace_sidebar_width_row
        .model()
        .and_downcast::<gtk4::StringList>()
        .expect("workspace width row should use a StringList model");

    assert_eq!(model.n_items(), 3);
    assert_eq!(model.string(0).as_deref(), Some("Small"));
    assert_eq!(model.string(1).as_deref(), Some("Comfy"));
    assert_eq!(model.string(2).as_deref(), Some("Large"));
    assert_eq!(prefs.imp().workspace_sidebar_width_row.selected(), 1);
}

#[test]
fn test_workspace_sidebar_width_row_updates_setting() {
    ensure_gtk_init();
    let settings = gio::Settings::new(config::APP_ID);
    settings
        .set_double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION, 0.3)
        .expect("set comfy preset");

    let prefs = LushtextPreferences::new();
    prefs.imp().workspace_sidebar_width_row.set_selected(2);

    while glib::MainContext::default().iteration(false) {}

    assert_eq!(settings.double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION), 0.4);
}

#[test]
fn test_data_page_reports_current_format_hides_actions_and_shows_verified_current() {
    let _data_dir = isolated_data_dir();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    wait_until(Duration::from_secs(10), || {
        !imp.data_operation_inflight.get()
            && imp.data_status_row.subtitle().as_deref() == Some("Data format is current")
    });

    assert_eq!(imp.data_status_row.title().as_str(), "Data Format");
    assert_eq!(
        imp.data_status_row.subtitle().as_deref(),
        Some("Data format is current")
    );
    assert!(imp.data_current_indicator.is_visible());
    assert!(
        !imp.data_actions_group.is_visible(),
        "current state should not leave an empty Actions group visible"
    );
    assert!(!imp.data_convert_row.is_visible());
    assert!(!imp.data_convert_button.is_sensitive());
    AccessibleAudit::new()
        .properties(&[gtk4::AccessibleProperty::ValueText])
        .assert_on(&*imp.data_status_row);
    AccessibleAudit::new()
        .states(&[gtk4::AccessibleState::Hidden])
        .assert_on(&*imp.data_convert_row);
    AccessibleAudit::new()
        .states(&[gtk4::AccessibleState::Disabled])
        .assert_on(&*imp.data_convert_button);
    assert!(
        !imp.data_announcement_throttler.should_announce_at(
            AnnouncementLane::StatusUpdate,
            "app-data-format-scan",
            Instant::now()
        ),
        "completed Data page scans should announce through the shared status-update lane"
    );
    assert!(
        imp.data_details_list.first_child().is_some(),
        "current state should still render a concise details row"
    );
}

#[test]
fn test_data_page_refresh_keeps_verifying_state_visible_for_fast_current_scan() {
    let _data_dir = isolated_data_dir();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    wait_until(Duration::from_secs(10), || {
        !imp.data_operation_inflight.get()
            && imp.data_status_row.subtitle().as_deref() == Some("Data format is current")
    });

    imp.data_scan_button.emit_clicked();
    let started_at = Instant::now();

    assert!(imp.data_operation_inflight.get());
    assert_eq!(
        imp.data_status_row.subtitle().as_deref(),
        Some("Verifying app data formats")
    );
    assert!(!imp.data_scan_button.is_sensitive());
    assert!(!imp.data_convert_button.is_sensitive());
    assert!(!imp.data_current_indicator.is_visible());
    AccessibleAudit::new()
        .states(&[gtk4::AccessibleState::Busy])
        .assert_on(&*imp.data_status_row);
    AccessibleAudit::new()
        .states(&[
            gtk4::AccessibleState::Busy,
            gtk4::AccessibleState::Disabled,
        ])
        .assert_on(&*imp.data_scan_button);

    while started_at.elapsed() < FAST_SCAN_VISIBLE_DWELL {
        glib::MainContext::default().iteration(false);
        assert!(
            imp.data_operation_inflight.get(),
            "fast current scan should stay visibly verifying for a perceptible dwell"
        );
        assert_eq!(
            imp.data_status_row.subtitle().as_deref(),
            Some("Verifying app data formats")
        );
        assert!(!imp.data_scan_button.is_sensitive());
        assert!(!imp.data_current_indicator.is_visible());
    }

    wait_until(Duration::from_secs(10), || {
        !imp.data_operation_inflight.get()
            && imp.data_status_row.subtitle().as_deref() == Some("Data format is current")
    });

    assert!(imp.data_scan_button.is_sensitive());
    assert!(imp.data_current_indicator.is_visible());
    assert!(!imp.data_actions_group.is_visible());
    AccessibleAudit::new()
        .states(&[gtk4::AccessibleState::Hidden])
        .assert_on(&*imp.data_convert_row);
}

#[test]
fn test_data_page_does_not_offer_convert_for_future_version() {
    let data_dir = isolated_data_dir();
    let future_session = json!({
        "kind": KIND_SESSION,
        "version": 2,
        "data": { "tabs": [], "active_tab_index": null }
    });
    fixture::write_text(
        &data_dir.path().join("session.json"),
        &serde_json::to_string_pretty(&future_session).expect("future session JSON"),
    );

    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    wait_until(Duration::from_secs(10), || {
        !imp.data_operation_inflight.get()
            && imp
                .data_status_row
                .subtitle()
                .is_some_and(|subtitle| subtitle.contains("created by a newer LushText"))
    });

    assert_eq!(
        imp.data_status_row.subtitle().as_deref(),
        Some("Some app data was created by a newer LushText")
    );
    assert!(
        !imp.data_convert_row.is_visible(),
        "newer/future metadata must not be presented as convertible"
    );
    assert!(!imp.data_actions_group.is_visible());
    assert!(!imp.data_convert_button.is_sensitive());
    assert!(!imp.data_current_indicator.is_visible());
    assert!(
        !imp.data_last_scan_offers_convert.get(),
        "future metadata must not arm the Convert action"
    );
}

#[test]
fn test_data_page_keeps_many_awkward_items_in_bounded_details_scroller() {
    let data_dir = isolated_data_dir();
    let bookmarks_dir = data_dir.path().join("bookmarks");
    fixture::create_dir_all(&bookmarks_dir);
    for index in 0..16 {
        let path = bookmarks_dir.join(format!(
            "{index:02}-very-long-bookmark-sidecar-name-that-should-not-expand-the-dialog.json"
        ));
        fixture::write_text(
            &path,
            &serde_json::to_string_pretty(&json!({
                "kind": KIND_BOOKMARK_SIDECAR,
                "version": SUPPORTED_JSON_VERSION + 1,
                "data": { "bookmarks": [] }
            }))
            .expect("future bookmark sidecar JSON"),
        );
    }

    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    wait_until(Duration::from_secs(10), || {
        !imp.data_operation_inflight.get()
            && imp
                .data_status_row
                .subtitle()
                .is_some_and(|subtitle| subtitle.contains("created by a newer LushText"))
    });

    let scroller =
        find_scrolled_window(&*imp.data_details_group).expect("data details scroller");
    assert_eq!(scroller.hscrollbar_policy(), gtk4::PolicyType::Never);
    assert_eq!(scroller.max_content_height(), 240);
    assert!(list_box_child_count(&imp.data_details_list) >= 16);
    assert!(!imp.data_actions_group.is_visible());
    assert!(!imp.data_convert_row.is_visible());
}

#[test]
fn test_data_page_shows_convert_only_for_registered_supported_legacy_plan() {
    let data_dir = isolated_data_dir();
    let session_path = data_dir.path().join("session.json");
    fixture::write_text(
        &session_path,
        &serde_json::to_string_pretty(&json!({
            "kind": KIND_SESSION,
            "version": 0,
            "data": { "tabs": [], "active_tab_index": null }
        }))
        .expect("legacy session JSON"),
    );
    let registry = ConverterRegistry::production().with_converter(
        KIND_SESSION,
        0,
        SUPPORTED_JSON_VERSION,
        |_| {
            Ok(serde_json::to_vec(&json!({
                "kind": KIND_SESSION,
                "version": SUPPORTED_JSON_VERSION,
                "data": { "tabs": [], "active_tab_index": null }
            }))
            .expect("converted session JSON"))
        },
    );
    let _registry_override = ConverterRegistry::override_production_for_test(registry);

    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();

    wait_until(Duration::from_secs(10), || {
        !imp.data_operation_inflight.get() && imp.data_convert_row.is_visible()
    });

    assert!(imp.data_actions_group.is_visible());
    assert!(imp.data_convert_button.is_sensitive());
    assert_eq!(imp.data_convert_button.label().as_deref(), Some("Convert"));
    assert!(!imp.data_current_indicator.is_visible());

    imp.data_convert_button.emit_clicked();
    wait_until(Duration::from_secs(10), || {
        !imp.data_operation_inflight.get()
            && imp.data_status_row.subtitle().as_deref() == Some("Data format is current")
    });

    assert!(imp.data_current_indicator.is_visible());
    assert!(!imp.data_actions_group.is_visible());
    assert!(!imp.data_convert_row.is_visible());
    let value: serde_json::Value =
        serde_json::from_str(&fixture::read_text(&session_path)).expect("converted session");
    assert_eq!(
        value.get("version").and_then(serde_json::Value::as_u64),
        Some(u64::from(SUPPORTED_JSON_VERSION))
    );
}

#[test]
fn test_data_page_keeps_failed_convert_retryable() {
    let data_dir = isolated_data_dir();
    let session_path = data_dir.path().join("session.json");
    fixture::write_text(
        &session_path,
        &serde_json::to_string_pretty(&json!({
            "kind": KIND_SESSION,
            "version": 0,
            "data": { "tabs": [], "active_tab_index": null }
        }))
        .expect("legacy session JSON"),
    );
    let registry = ConverterRegistry::production().with_converter(
        KIND_SESSION,
        0,
        SUPPORTED_JSON_VERSION,
        |_| Err(std::io::Error::other("synthetic conversion failure").into()),
    );
    let _registry_override = ConverterRegistry::override_production_for_test(registry);

    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();
    wait_until(Duration::from_secs(10), || {
        !imp.data_operation_inflight.get() && imp.data_convert_row.is_visible()
    });

    imp.data_convert_button.emit_clicked();
    wait_until(Duration::from_secs(10), || {
        !imp.data_operation_inflight.get()
            && imp.data_convert_button.label().as_deref() == Some("Retry")
    });

    assert!(imp.data_actions_group.is_visible());
    assert!(imp.data_convert_row.is_visible());
    assert!(imp.data_convert_button.is_sensitive());
    assert!(!imp.data_current_indicator.is_visible());
    assert!(
        imp.data_status_row
            .subtitle()
            .is_some_and(|subtitle| subtitle.contains("Data update failed")),
        "retry state should keep failure detail visible"
    );
    let value: serde_json::Value =
        serde_json::from_str(&fixture::read_text(&session_path)).expect("retryable session");
    assert_eq!(value.get("version").and_then(serde_json::Value::as_u64), Some(0));
    assert!(
        fs_metadata::exists(&data_dir.path().join(FORMAT_UPGRADE_BACKUP_DIR)),
        "failed convert should still leave backup evidence before retry"
    );
}
