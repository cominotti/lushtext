// SPDX-License-Identifier: GPL-3.0-or-later

//! Integration tests for workspace management.

use crate::common::TestContext;
use lushtext_core::model::workspace::{WorkspaceEntry, WorkspaceId};
use lushtext_core::services::workspace_manager;

#[test]
fn test_full_workspace_lifecycle() {
    let ctx = TestContext::new();

    // Start with empty state
    let mut file = workspace_manager::load(ctx.data_dir()).unwrap();
    assert!(file.workspaces.is_empty());

    // Get active workspace (creates default)
    let ws = workspace_manager::active_workspace(&mut file);
    assert_eq!(ws.name, "workspace");
    let ws_id = ws.id.clone();

    // Add entries
    let project_dir = ctx.mkdir("projects/my-app");
    workspace_manager::add_entry(
        &mut file,
        &ws_id,
        WorkspaceEntry::Directory {
            path: project_dir.clone(),
        },
    );

    let readme = ctx.write_file("projects/README.md", "# Hello");
    workspace_manager::add_entry(
        &mut file,
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
    assert_eq!(
        reloaded.active_workspace,
        Some(ws_id.clone())
    );

    // Remove an entry
    let mut file = reloaded;
    workspace_manager::remove_entry(&mut file, &ws_id, &project_dir);
    assert_eq!(file.workspaces[0].entries.len(), 1);
}

#[test]
fn test_multiple_workspaces() {
    let ctx = TestContext::new();
    let mut file = workspace_manager::load(ctx.data_dir()).unwrap();

    // Create default workspace
    let _ = workspace_manager::active_workspace(&mut file);

    // Add a second workspace manually
    let ws2 = lushtext_core::model::workspace::WorkspaceConfig {
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
