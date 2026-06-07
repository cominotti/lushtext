// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace persistence for the public v1 JSON format.
//!
//! This service owns the app-data boundary for `workspaces.json`. The runtime
//! format is a clean break from pre-public bare workspace JSON, so recovery
//! handles preservation before the sidebar consumes a default state.

use crate::model::workspace::WorkspacesFile;
use crate::services::json_format::KIND_WORKSPACE_STATE;
use crate::services::recovery_metadata::{
    RecoveryLoad, RecoveryLoadConfig, RecoveryMetadataClass, load_enveloped_json_or_default,
    save_enveloped_json_path,
};
use anyhow::Result;
use std::path::Path;

/// Fixed filename for workspace state.
const WORKSPACES_FILE: &str = "workspaces.json";

/// Load workspaces from disk, returning default state with diagnostics if needed.
///
/// # Errors
///
/// This compatibility wrapper currently returns recovered state. Use
/// [`load_recovering`] when diagnostics matter to the caller.
pub fn load(data_dir: &Path) -> Result<WorkspacesFile> {
    Ok(load_recovering(data_dir).value)
}

/// Load workspaces through recovery-aware v1 envelope handling.
#[must_use]
pub fn load_recovering(data_dir: &Path) -> RecoveryLoad<WorkspacesFile> {
    let path = data_dir.join(WORKSPACES_FILE);
    let mut load: RecoveryLoad<WorkspacesFile> = load_enveloped_json_or_default(
        &RecoveryLoadConfig::new(data_dir, &path, RecoveryMetadataClass::WorkspaceState),
        KIND_WORKSPACE_STATE,
    );
    load.value.normalize_scope();
    load
}

#[cfg(test)]
fn trace_recovery_diagnostics(load: &RecoveryLoad<WorkspacesFile>) {
    for diagnostic in &load.diagnostics {
        tracing::warn!("{}", diagnostic.summary());
    }
}

/// Save workspaces to disk.
///
/// # Errors
///
/// Returns an error if the workspace file cannot be serialized or written.
pub fn save(data_dir: &Path, file: &WorkspacesFile) -> Result<()> {
    let path = data_dir.join(WORKSPACES_FILE);
    let config = RecoveryLoadConfig::new(data_dir, &path, RecoveryMetadataClass::WorkspaceState);
    let diagnostics = save_enveloped_json_path(&config, KIND_WORKSPACE_STATE, file)?;
    for diagnostic in diagnostics {
        tracing::warn!("{}", diagnostic.summary());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::workspace::WorkspaceScope;
    use crate::services::filesystem::fixture;
    use crate::services::json_format::JsonEnvelopeRef;
    use crate::services::recovery_metadata::RecoveryProblem;
    use tempfile::TempDir;

    #[test]
    fn test_load_missing_file_returns_default() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let result = load(dir.path()).expect("expected operation to succeed");
        assert!(result.workspaces.is_empty());
        assert_eq!(result.current_scope, WorkspaceScope::All);
    }

    #[test]
    fn test_load_non_file_workspace_path_returns_error() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::create_dir(&dir.path().join("workspaces.json"));

        let loaded = load_recovering(dir.path());

        assert!(loaded.value.workspaces.is_empty());
        assert!(!loaded.replacement_allowed());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let mut file = WorkspacesFile::default();
        let workspace_id = file.add_workspace("my workspace", "/home/user/project".into());
        file.set_current_scope(WorkspaceScope::workspace(workspace_id.clone()));

        save(dir.path(), &file).expect("expected operation to succeed");
        let loaded = load_recovering(dir.path());
        trace_recovery_diagnostics(&loaded);
        let loaded = loaded.value;

        assert_eq!(loaded.workspaces.len(), 1);
        assert_eq!(loaded.workspaces[0].name, "my workspace");
        assert_eq!(loaded.workspaces[0].root, Path::new("/home/user/project"));
        assert_eq!(
            loaded.current_scope(),
            WorkspaceScope::workspace(workspace_id)
        );
    }

    #[test]
    fn test_load_rejects_pre_public_multi_root_workspace() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(
            &dir.path().join("workspaces.json"),
            &serde_json::json!({
                "active_workspace": "legacy",
                "workspaces": [{
                    "id": "legacy",
                    "name": "Legacy",
                    "entries": [
                        { "kind": "directory", "path": "/tmp/one" },
                        { "kind": "directory", "path": "/tmp/two" }
                    ]
                }]
            })
            .to_string(),
        );

        let loaded = load_recovering(dir.path());

        assert!(loaded.value.workspaces.is_empty());
        assert!(matches!(
            loaded.diagnostics[0].problem,
            RecoveryProblem::UnsupportedFormat { .. }
        ));
        let quarantine_path = loaded.diagnostics[0]
            .preservation
            .quarantine_path()
            .expect("quarantined unsupported workspace");
        assert!(fixture::read_text(quarantine_path).contains("active_workspace"));
    }

    #[test]
    fn test_load_falls_back_to_all_scope_when_target_is_missing() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(
            &dir.path().join("workspaces.json"),
            &serde_json::to_string_pretty(&JsonEnvelopeRef::new(
                KIND_WORKSPACE_STATE,
                &serde_json::json!({
                    "current_scope": { "kind": "workspace", "workspace_id": "missing" },
                    "workspaces": [{
                        "id": "existing",
                        "name": "Existing",
                        "root": "/tmp/existing"
                    }]
                }),
            ))
            .expect("workspace fixture"),
        );

        let loaded = load(dir.path()).expect("expected operation to succeed");
        assert_eq!(loaded.current_scope(), WorkspaceScope::All);
    }

    #[test]
    fn save_quarantines_unsupported_workspace_before_replacement() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(&dir.path().join(WORKSPACES_FILE), r#"{"workspaces":[]}"#);

        save(dir.path(), &WorkspacesFile::default()).expect("save v1 workspace");

        let quarantine_dir = dir
            .path()
            .join(crate::services::recovery_metadata::QUARANTINE_DIR);
        let quarantine_entries = crate::services::filesystem::tree::scan_directory(
            &quarantine_dir,
            crate::services::filesystem::DirectoryScanPolicy::visible_workspace(),
        )
        .expect("quarantine entries");
        assert_eq!(quarantine_entries.len(), 1);
        let loaded = load_recovering(dir.path());
        assert!(loaded.diagnostics.is_empty());
    }
}
