// SPDX-License-Identifier: GPL-3.0-or-later

//! Unit tests for the GTK-free action catalog and drift checks.

use super::*;
use crate::model::action_catalog::{
    ActionCatalogEntry, ActionCoverageLane, ActionExposure, ActionScope, ActionSurface,
    ActionValueType, ExternalActivationSafety, ObservedAction,
};

#[test]
fn catalog_entries_have_unique_ids_docs_and_coverage() {
    assert_eq!(audit_catalog(entries()), Ok(()));
}

#[test]
fn app_actions_match_recorded_baseline() {
    assert_eq!(
        audit_observed_actions(ActionScope::App, BASELINE_APP_ACTIONS),
        Ok(())
    );
}

#[test]
fn window_actions_match_recorded_baseline() {
    assert_eq!(
        audit_observed_actions(ActionScope::Window, BASELINE_WINDOW_ACTIONS),
        Ok(())
    );
}

#[test]
fn command_palette_commands_are_cataloged() {
    assert_eq!(audit_command_palette_actions(), Ok(()));
}

#[test]
fn static_visible_actions_are_cataloged() {
    assert_eq!(audit_visible_static_actions(), Ok(()));
}

#[test]
fn developer_reference_rows_are_complete_and_serializable() {
    let rows = developer_reference_rows();

    assert_eq!(rows.len(), entries().len());
    assert!(rows.iter().any(|row| row.action_id == "win.save"));
    assert!(rows.iter().any(|row| {
        row.action_id == "win.show-help-overlay" && row.exposure == ActionExposure::Exported
    }));

    let json = developer_reference_json().expect("catalog reference should serialize");
    assert!(json.contains("\"action_id\": \"win.save\""));
    assert!(json.contains("\"docs_anchor\": \"action-win-save\""));
}

#[test]
fn help_overlay_command_is_a_supported_exported_action() {
    let row = developer_reference_rows()
        .into_iter()
        .find(|row| row.action_id == "win.show-help-overlay")
        .expect("help overlay command should stay cataloged");

    assert_eq!(row.exposure, ActionExposure::Exported);
    assert_eq!(
        row.external_activation,
        ExternalActivationSafety::StableUserCommand
    );
    assert!(row.surfaces.contains(&ActionSurface::PrimaryMenu));
    assert!(row.surfaces.contains(&ActionSurface::CommandPalette));
    assert!(row.surfaces.contains(&ActionSurface::DbusAction));
    assert!(row.coverage_lanes.contains(&ActionCoverageLane::Unit));
    assert!(row.coverage_lanes.contains(&ActionCoverageLane::Widget));
}

#[test]
fn catalog_audit_fails_on_duplicate_action_ids() {
    let entry = minimal_entry("save", "action-win-save");
    let catalog = [entry, entry];

    let failures = audit_catalog(&catalog).expect_err("duplicate ids should fail");

    assert!(matches!(
        &failures[..],
        [ActionCatalogAuditFailure::DuplicateActionIds(ids)] if ids == &vec!["win.save".to_string()]
    ));
}

#[test]
fn catalog_audit_fails_on_missing_docs_anchor() {
    let catalog = [minimal_entry("save", "")];

    let failures = audit_catalog(&catalog).expect_err("missing docs anchor should fail");

    assert!(matches!(
        &failures[..],
        [ActionCatalogAuditFailure::MissingDocsAnchors(ids)] if ids == &vec!["win.save".to_string()]
    ));
}

#[test]
fn catalog_audit_fails_on_missing_coverage_lane() {
    let catalog = [ActionCatalogEntry::new(
        ActionScope::Window,
        "save",
        "Save",
        ActionValueType::None,
        ActionValueType::None,
        "Requires an active tab.",
        "window/actions",
        &[ActionSurface::PrimaryMenu],
        ExternalActivationSafety::ContextualUserCommand,
        ActionExposure::Exported,
        "action-win-save",
        &[],
    )];

    let failures = audit_catalog(&catalog).expect_err("missing coverage should fail");

    assert!(matches!(
        &failures[..],
        [ActionCatalogAuditFailure::MissingCoverage(ids)] if ids == &vec!["win.save".to_string()]
    ));
}

#[test]
fn observed_action_audit_fails_on_missing_catalog_entry() {
    let observed = [ObservedAction::new(
        ActionScope::Window,
        "not-cataloged",
        ActionValueType::None,
        ActionValueType::None,
    )];

    let failures =
        audit_observed_actions(ActionScope::Window, &observed).expect_err("unknown action fails");

    assert!(failures.iter().any(|failure| matches!(
        failure,
        ActionCatalogAuditFailure::MissingCatalogEntries { action_ids, .. }
            if action_ids.contains(&"win.not-cataloged".to_string())
    )));
}

#[test]
fn observed_action_audit_fails_on_stale_catalog_entry() {
    let observed = BASELINE_WINDOW_ACTIONS
        .iter()
        .filter(|&action| action.name != "save")
        .cloned()
        .collect::<Vec<_>>();

    let failures =
        audit_observed_actions(ActionScope::Window, &observed).expect_err("missing export fails");

    assert!(failures.iter().any(|failure| matches!(
        failure,
        ActionCatalogAuditFailure::MissingRegisteredActions { action_ids, .. }
            if action_ids.contains(&"win.save".to_string())
    )));
}

#[test]
fn observed_action_audit_fails_on_type_mismatch() {
    let mut observed = BASELINE_WINDOW_ACTIONS.to_vec();
    let action = observed
        .iter_mut()
        .find(|action| action.name == "toggle-sidebar")
        .expect("toggle-sidebar baseline action");
    action.state_type = ActionValueType::None;

    let failures =
        audit_observed_actions(ActionScope::Window, &observed).expect_err("type mismatch fails");

    assert!(failures.iter().any(|failure| matches!(
        failure,
        ActionCatalogAuditFailure::TypeMismatches(mismatches)
            if mismatches.iter().any(|mismatch| mismatch.action_id == "win.toggle-sidebar"
                && mismatch.expected_state == ActionValueType::Bool
                && mismatch.observed_state == ActionValueType::None)
    )));
}

#[test]
fn observed_action_audit_rejects_wrong_parameter_type_for_each_parameterized_action() {
    for (action_name, expected_type) in [
        ("set-search-query", ActionValueType::String),
        ("set-sidebar-visible", ActionValueType::Bool),
        ("set-properties-visible", ActionValueType::Bool),
        ("set-minimap-visible", ActionValueType::Bool),
        ("set-search-panel-visible", ActionValueType::Bool),
        ("set-focus-mode", ActionValueType::Bool),
        ("set-preview-pane-visible", ActionValueType::Bool),
        ("set-preview-mode", ActionValueType::Bool),
        ("select-tab", ActionValueType::U32),
        ("set-notes-browser-query", ActionValueType::String),
        ("select-notes-browser-row", ActionValueType::U32),
    ] {
        let mut observed = BASELINE_WINDOW_ACTIONS.to_vec();
        let action = observed
            .iter_mut()
            .find(|action| action.name == action_name)
            .unwrap_or_else(|| panic!("{action_name} baseline action"));
        action.parameter_type = ActionValueType::None;

        let failures = audit_observed_actions(ActionScope::Window, &observed)
            .expect_err("type mismatch fails");

        assert!(failures.iter().any(|failure| matches!(
            failure,
            ActionCatalogAuditFailure::TypeMismatches(mismatches)
                if mismatches.iter().any(|mismatch| mismatch.action_id == format!("win.{action_name}")
                    && mismatch.expected_parameter == expected_type
                    && mismatch.observed_parameter == ActionValueType::None)
        )));
    }
}

#[test]
fn arbitrary_action_id_audit_fails_on_missing_catalog_entry() {
    let failures = audit_cataloged_ids("test-surface", ["win.missing"])
        .expect_err("missing action id should fail");

    assert!(matches!(
        &failures[..],
        [ActionCatalogAuditFailure::MissingCatalogEntries { source, action_ids }]
            if *source == "test-surface" && action_ids == &vec!["win.missing".to_string()]
    ));
}

fn minimal_entry(name: &'static str, docs_anchor: &'static str) -> ActionCatalogEntry {
    ActionCatalogEntry::new(
        ActionScope::Window,
        name,
        "Save",
        ActionValueType::None,
        ActionValueType::None,
        "Requires an active tab.",
        "window/actions",
        &[ActionSurface::PrimaryMenu],
        ExternalActivationSafety::ContextualUserCommand,
        ActionExposure::Exported,
        docs_anchor,
        &[ActionCoverageLane::Unit],
    )
}
