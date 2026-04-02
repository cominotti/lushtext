// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration tests for workspace management.

use crate::common::TestContext;
use lushtext_core::model::workspace::{
    WorkspaceConfig, WorkspaceEntry, WorkspaceId, WorkspacesFile,
};
use lushtext_core::services::workspace_manager;

#[test]
fn test_full_workspace_lifecycle() {
    let ctx = TestContext::new();

    // Start with empty state
    let mut file = workspace_manager::load(ctx.data_dir()).unwrap();
    assert!(file.workspaces.is_empty());

    // Get active workspace (creates default)
    let ws = file.active_workspace();
    assert_eq!(ws.name, "New Workspace");
    let ws_id = ws.id.clone();

    // Add entries
    let project_dir = ctx.mkdir("projects/my-app");
    file.add_entry(
        &ws_id,
        WorkspaceEntry::Directory {
            path: project_dir.clone(),
        },
    );

    let readme = ctx.write_file("projects/README.md", "# Hello");
    file.add_entry(
        &ws_id,
        WorkspaceEntry::File {
            path: readme.clone(),
        },
    );

    assert_eq!(file.workspaces[0].entries.len(), 2);

    // Save and reload
    workspace_manager::save(ctx.data_dir(), &file).unwrap();
    let reloaded = workspace_manager::load(ctx.data_dir()).unwrap();
    assert_eq!(reloaded.workspaces[0].entries.len(), 2);
    assert_eq!(reloaded.active_workspace, Some(ws_id.clone()));

    // Remove an entry
    let mut file = reloaded;
    file.remove_entry(&ws_id, &project_dir);
    assert_eq!(file.workspaces[0].entries.len(), 1);
}

#[test]
fn test_multiple_workspaces() {
    let ctx = TestContext::new();
    let mut file = workspace_manager::load(ctx.data_dir()).unwrap();

    // Create default workspace
    let _ = file.active_workspace();

    // Add a second workspace manually
    let ws2 = WorkspaceConfig {
        id: WorkspaceId("second".into()),
        name: "rust-projects".into(),
        entries: vec![],
    };
    file.workspaces.push(ws2);

    // Switch active
    file.active_workspace = Some(WorkspaceId("second".into()));

    workspace_manager::save(ctx.data_dir(), &file).unwrap();
    let reloaded = workspace_manager::load(ctx.data_dir()).unwrap();

    assert_eq!(reloaded.workspaces.len(), 2);
    assert_eq!(
        reloaded.active_workspace,
        Some(WorkspaceId("second".into()))
    );
}

#[test]
fn test_add_workspace_persist_roundtrip() {
    let ctx = TestContext::new();
    let mut file = WorkspacesFile::default();

    let ws_id = file.add_workspace("my project");
    let project_dir = ctx.mkdir("projects/my-app");
    file.add_entry(
        &ws_id,
        WorkspaceEntry::Directory {
            path: project_dir.clone(),
        },
    );

    workspace_manager::save(ctx.data_dir(), &file).unwrap();
    let reloaded = workspace_manager::load(ctx.data_dir()).unwrap();

    assert_eq!(reloaded.workspaces.len(), 1);
    assert_eq!(reloaded.workspaces[0].name, "my project");
    assert_eq!(reloaded.workspaces[0].id, ws_id);
    assert_eq!(reloaded.workspaces[0].entries.len(), 1);
}

#[test]
fn test_remove_workspace_persist_roundtrip() {
    let ctx = TestContext::new();
    let mut file = WorkspacesFile::default();

    let ws1 = file.add_workspace("first");
    let ws2 = file.add_workspace("second");
    file.active_workspace = Some(ws1.clone());

    workspace_manager::save(ctx.data_dir(), &file).unwrap();
    let mut file = workspace_manager::load(ctx.data_dir()).unwrap();

    file.remove_workspace(&ws1);
    workspace_manager::save(ctx.data_dir(), &file).unwrap();

    let reloaded = workspace_manager::load(ctx.data_dir()).unwrap();
    assert_eq!(reloaded.workspaces.len(), 1);
    assert_eq!(reloaded.workspaces[0].name, "second");
    assert_eq!(reloaded.active_workspace, Some(ws2));
}

#[test]
fn test_rename_workspace_persist_roundtrip() {
    let ctx = TestContext::new();
    let mut file = WorkspacesFile::default();

    let ws_id = file.add_workspace("original");
    file.rename_workspace(&ws_id, "renamed");

    workspace_manager::save(ctx.data_dir(), &file).unwrap();
    let reloaded = workspace_manager::load(ctx.data_dir()).unwrap();

    assert_eq!(reloaded.workspaces[0].name, "renamed");
}

#[test]
fn test_multiple_workspaces_with_entries() {
    let ctx = TestContext::new();
    let mut file = WorkspacesFile::default();

    let ws1 = file.add_workspace("project-a");
    let ws2 = file.add_workspace("project-b");

    let dir_a = ctx.mkdir("project-a/src");
    let dir_b = ctx.mkdir("project-b/src");

    file.add_entry(
        &ws1,
        WorkspaceEntry::Directory {
            path: dir_a.clone(),
        },
    );
    file.add_entry(
        &ws2,
        WorkspaceEntry::Directory {
            path: dir_b.clone(),
        },
    );

    workspace_manager::save(ctx.data_dir(), &file).unwrap();
    let reloaded = workspace_manager::load(ctx.data_dir()).unwrap();

    assert_eq!(reloaded.workspaces.len(), 2);
    assert_eq!(reloaded.workspaces[0].entries.len(), 1);
    assert_eq!(reloaded.workspaces[1].entries.len(), 1);
    assert_eq!(reloaded.workspaces[0].entries[0].path(), dir_a.as_path());
    assert_eq!(reloaded.workspaces[1].entries[0].path(), dir_b.as_path());
}
