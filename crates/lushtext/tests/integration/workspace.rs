// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration tests for workspace persistence and normalization.

use crate::common::TestContext;
use lushtext_core::model::workspace::{
    WorkspaceConfig, WorkspaceId, WorkspaceScope, WorkspacesFile,
};
use lushtext_core::services::workspace_manager;
use std::path::Path;

#[test]
fn test_missing_workspace_file_restores_empty_shell_state() {
    let ctx = TestContext::new();

    let file = workspace_manager::load(ctx.data_dir()).expect("expected operation to succeed");

    assert!(file.workspaces.is_empty());
    assert_eq!(file.current_scope(), WorkspaceScope::All);
}

#[test]
fn test_add_workspace_persists_single_root_and_scope() {
    let ctx = TestContext::new();
    let project_dir = ctx.mkdir("projects/my-app");

    let mut file = WorkspacesFile::default();
    let workspace_id = file.add_workspace("my project", project_dir.clone());

    workspace_manager::save(ctx.data_dir(), &file).expect("expected operation to succeed");
    let restored = workspace_manager::load(ctx.data_dir()).expect("expected operation to succeed");

    assert_eq!(restored.workspaces.len(), 1);
    assert_eq!(restored.workspaces[0].id, workspace_id.clone());
    assert_eq!(restored.workspaces[0].name, "my project");
    assert_eq!(restored.workspaces[0].root, project_dir);
    assert_eq!(
        restored.current_scope(),
        WorkspaceScope::workspace(workspace_id)
    );
}

#[test]
fn test_remove_selected_workspace_falls_back_to_all_scope() {
    let ctx = TestContext::new();
    let first_dir = ctx.mkdir("workspace-a");
    let second_dir = ctx.mkdir("workspace-b");

    let mut file = WorkspacesFile::default();
    let first = file.add_workspace("first", first_dir);
    let _second = file.add_workspace("second", second_dir);
    file.set_current_scope(WorkspaceScope::workspace(first.clone()));

    workspace_manager::save(ctx.data_dir(), &file).expect("expected operation to succeed");
    let mut restored =
        workspace_manager::load(ctx.data_dir()).expect("expected operation to succeed");

    restored.remove_workspace(&first);
    workspace_manager::save(ctx.data_dir(), &restored).expect("expected operation to succeed");

    let reloaded = workspace_manager::load(ctx.data_dir()).expect("expected operation to succeed");
    assert_eq!(reloaded.workspaces.len(), 1);
    assert_eq!(reloaded.current_scope(), WorkspaceScope::All);
}

#[test]
fn test_remove_and_add_different_root_persists_new_workspace_entry() {
    let ctx = TestContext::new();
    let old_root = ctx.mkdir("old-root");
    let new_root = ctx.mkdir("new-root");

    let mut file = WorkspacesFile::default();
    let old_id = file.add_workspace("old", old_root.clone());
    file.remove_workspace(&old_id);
    let new_id = file.add_workspace("new", new_root.clone());

    workspace_manager::save(ctx.data_dir(), &file).expect("expected operation to succeed");
    let restored = workspace_manager::load(ctx.data_dir()).expect("expected operation to succeed");

    assert_eq!(restored.workspaces.len(), 1);
    assert_eq!(restored.workspaces[0].id, new_id);
    assert_ne!(restored.workspaces[0].id, old_id);
    assert_eq!(restored.workspaces[0].root, new_root);
    assert_ne!(restored.workspaces[0].root, old_root);
    assert_eq!(restored.workspaces[0].name, "new");
}

#[test]
fn test_legacy_multi_root_workspace_normalizes_to_sibling_workspaces() {
    let ctx = TestContext::new();
    std::fs::write(
        ctx.data_dir().join("workspaces.json"),
        serde_json::json!({
            "active_workspace": "legacy",
            "workspaces": [{
                "id": "legacy",
                "name": "legacy",
                "entries": [
                    { "kind": "directory", "path": "/tmp/workspace-a" },
                    { "kind": "directory", "path": "/tmp/workspace-b" }
                ]
            }]
        })
        .to_string(),
    )
    .expect("expected operation to succeed");

    let restored = workspace_manager::load(ctx.data_dir()).expect("expected operation to succeed");

    assert_eq!(restored.workspaces.len(), 2);
    assert_eq!(restored.workspaces[0].id, WorkspaceId::new("legacy"));
    assert_eq!(restored.workspaces[0].root, Path::new("/tmp/workspace-a"));
    assert_eq!(
        restored.current_scope(),
        WorkspaceScope::workspace(WorkspaceId::new("legacy"))
    );
}

#[test]
fn test_scope_falls_back_to_all_when_persisted_target_is_missing() {
    let ctx = TestContext::new();
    let file = WorkspacesFile {
        current_scope: WorkspaceScope::workspace(WorkspaceId::new("missing")),
        workspaces: vec![WorkspaceConfig {
            id: WorkspaceId::new("existing"),
            name: "existing".into(),
            root: "/tmp/existing".into(),
        }],
    };

    workspace_manager::save(ctx.data_dir(), &file).expect("expected operation to succeed");
    let restored = workspace_manager::load(ctx.data_dir()).expect("expected operation to succeed");

    assert_eq!(restored.current_scope(), WorkspaceScope::All);
}
