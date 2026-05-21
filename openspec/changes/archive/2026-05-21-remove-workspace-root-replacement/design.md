## Context

The workspace sidebar now represents each persisted workspace as one named directory root, with a top-level `New Workspace` affordance for adding folders and a header context menu for renaming or removing existing workspaces. Each section header also exposes `Refresh` and `Replace Workspace Root`; the latter mutates an existing workspace ID to point at another directory.

That replacement flow no longer earns its complexity. It is an alternate path for the same user intent as remove-and-add, but it forces the app to preserve a workspace slot while changing its root identity. That makes persistence, sidecar notes, tests, documentation, and mental model language carry a special case.

## Goals / Non-Goals

**Goals:**

- Remove the user-facing `Replace Workspace Root` control and dialog.
- Keep per-section manual refresh and move it into the rightmost header-control position currently occupied by replace-root.
- Remove root-replacement callback plumbing and model mutation paths.
- Preserve existing add, remove, rename, current-scope, refresh, drill-down, and workspace-note behaviors.
- Update tests, specs, and documentation so they describe one clear root-change path: remove the old workspace and add a new one.

**Non-Goals:**

- Do not change the persisted workspace file format for existing valid single-root workspaces.
- Do not delete workspace-note, bookmark, annotation, or local-history sidecars when a workspace is removed.
- Do not add a bulk migration, alias, compatibility shim, or replacement-root hidden action.
- Do not change the top-row `New Workspace` affordance.

## Decisions

### Remove replacement instead of hiding it

The implementation should delete the replace-root dialog, callback field, signal connector, button template child, and model method rather than leave an unreachable internal API. Keeping unused replacement code would preserve the same conceptual branch that this change is meant to remove, and future work could accidentally revive it.

Alternative considered: hide the button but keep `replace_root` for internal callers. Rejected because there is no remaining product path that needs it.

### Move Refresh into the terminal header-control slot

The workspace-section header should keep the label at the left and expose `Refresh` as the only button at the right. This preserves a compact header and satisfies the requested visual placement: the remaining refresh affordance occupies the space where replace-root used to sit.

Alternative considered: leave Refresh in its existing position and remove the trailing button. Rejected because the requested UI intent is for Refresh to remain where Replace was.

### Treat different roots as different workspace entries

Changing from one project folder to another should be modeled as removing the existing workspace and adding a new workspace. This gives the new root its own workspace ID and display name while the old root's root-keyed sidecar data remains available if the same root is added again later.

Alternative considered: remove the button but keep root replacement through another entry point. Rejected because it leaves the product concept in place under a different surface.

### Keep storage format stable

The persisted `workspaces.json` shape already supports the desired model: zero or more named single-root workspaces plus the current workspace scope. Existing valid files need no migration. The only cleanup is removing the in-app operation that rewrites a workspace's root in place.

Alternative considered: introduce a schema version or migration marker for this behavioral change. Rejected because no persisted data shape changes.

## Risks / Trade-offs

- [Users who relied on replacement need one extra action] -> The remove-and-add workflow is explicit and already available through existing workspace controls.
- [Deleting replacement code may reveal tests that depended on internal helpers] -> Replace those tests with add/remove/rename/current-scope assertions and widget tests for the single Refresh control.
- [Documentation can keep stale wording] -> Search the repo for `Replace Workspace Root`, `replace root`, `replace_root`, and `add_folder_button` during implementation and update every product/spec/comment reference.
- [Refresh placement can regress subtly] -> Add or update widget tests to assert Refresh is the rightmost workspace-section header button.
