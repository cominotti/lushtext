## Context

LushText's current workspace behavior is split across two different eras of the feature.

- The persisted model still treats a workspace as a named collection of mixed directory and file roots plus an `active_workspace` reference.
- The sidebar UI increasingly behaves as if each workspace is one navigable root directory rendered as one section with one header, one refresh flow, one replace-root action, and one drill-down stack.
- The top selector already exposes `All workspaces` plus one entry per workspace, but that selector is still local sidebar state instead of the app-wide workspace concept. Today notes and export flows use the filtered scope, while search and palette indexing still fan out across all directory roots.
- The current empty-state shell is intentional: when no workspaces are restored, the sidebar still shows the pinned top affordance row and no workspace sections.

That mismatch is now costly. It makes the product harder to explain, keeps legacy model affordances alive even though the UI no longer supports them cleanly, and prevents workspace selection from behaving as a trustworthy global scope.

## Goals / Non-Goals

**Goals:**
- Make a workspace a single-root directory concept everywhere in the product.
- Promote workspace selection into a real app-wide scope that workspace-aware features share consistently.
- Keep the current empty-sidebar shell intentional instead of backsliding into placeholder workspace UI.
- Preserve the existing sidebar shell patterns that already fit the single-root model well: one section per workspace, one refresh flow, one replace-root action, and local drill-down.
- Provide a safe migration path for existing persisted multi-root and file-root workspace state.

**Non-Goals:**
- Redesigning file peek, preview, or other sidebar-adjacent interactions that do not define workspace identity.
- Changing the adaptive sidebar width policy or compact secondary-surface behavior.
- Redesigning search or notes UI beyond the scope-routing changes needed to honor the new workspace contract.
- Keeping legacy multi-root or standalone-file workspaces as first-class behavior after this change.

## Decisions

### 1. A persisted workspace becomes one named root directory

The canonical workspace entity will be narrowed to one stable ID, one display name, and one root directory. Directory collections and standalone file roots will no longer be part of the supported product contract.

This matches the sidebar's current shape. Each visible workspace section already behaves like one root-centered browsing surface, and most higher-level affordances such as refresh, replace root, focus-folder drill-down, and header actions read much more naturally when the section owns exactly one persisted root.

Alternatives considered:
- Keep `Vec<WorkspaceEntry>` and merely spec the current UI as a partial view of a richer model: rejected because it preserves the same ambiguity that caused the current mismatch.
- Ban file roots but keep multi-root directory workspaces: rejected because it still forces the section and scope model to answer whether the workspace is one browsing root or a bundle.

### 2. The app-wide source of truth becomes an explicit workspace scope, not a sidebar-local filter

LushText will treat workspace scope as first-class shared state. The scope will have two legal states:

- a specific workspace ID
- the explicit aggregate scope `All workspaces`

The sidebar selector row becomes the primary control for this state, but not its only owner. The window shell should own the current scope and expose it to search, palette indexing, note and export workflows, and any future workspace-aware feature. The sidebar will reflect and edit that shared state instead of hiding its own local `selected_workspace_filter`.

This keeps the architecture honest: a real product concept should not live in one widget's private `RefCell`.

Alternatives considered:
- Keep the current sidebar-local filter and have each consumer infer scope ad hoc: rejected because it is exactly the inconsistency we are trying to remove.
- Remove `All workspaces` entirely and require a concrete workspace at all times: rejected because the existing selector and multi-workspace browsing story already establish a useful aggregate scope, and an explicit aggregate choice is still compatible with a real workspace model.

### 3. Empty state remains an intentional empty shell, not an implicit placeholder workspace

When no workspaces exist, the user-facing sidebar should remain empty below the fixed top affordance row. The app should not create a visible `New Workspace` section just to satisfy old model assumptions.

That means the new contract must stop treating "usable state" as "there is always at least one workspace section." A usable empty state is instead:

- zero persisted workspaces
- the fixed selector and new-workspace affordance still visible
- no fake or auto-created workspace section

Internal helpers that lazily materialize a default workspace should be retired from user-facing restore flows or replaced with explicit creation paths that do not contradict the shell.

Alternatives considered:
- Preserve a hidden default workspace in the model and keep pretending the shell is empty: rejected because it leaks legacy semantics into persistence and selection logic.
- Show a visible default section again: rejected because it conflicts with the current intentional shell design the user wants to keep.

### 4. Current scope restoration and fallback should be explicit and conservative

Persisted state should restore the user's last explicit workspace scope when it still exists. If the stored workspace scope points at a missing workspace, the system should fall back to the explicit aggregate `All workspaces` scope rather than silently retargeting another workspace.

This is safer once workspace affects search, indexing, and export results. Automatically rebasing to the first remaining workspace would change the meaning of global actions without a user choice. Falling back to `All workspaces` preserves access while making the changed state easy to understand from the selector row.

Creation and deletion flows should also follow that principle:

- creating a new workspace selects that workspace as the current scope
- renaming or replacing a workspace root preserves the current scope if that same workspace remains
- unlisting the currently selected workspace falls back to `All workspaces`

Alternatives considered:
- Keep rebasing to the first remaining workspace: rejected because it is too implicit once scope governs cross-feature results.
- Reset to no scope at all: rejected because the existing selector already has a clear aggregate fallback.

### 5. Legacy multi-root and file-root state should migrate forward once, with data-preserving normalization

The loader should normalize legacy persisted workspaces into the single-root model:

- a workspace with one directory root stays as one workspace
- extra directory roots are split into additional sibling workspaces
- standalone file roots are promoted to sibling workspaces rooted at the file's parent directory when that parent can be determined
- entries that cannot be normalized safely are dropped only as a last resort, with lightweight user-visible feedback

The original workspace keeps its existing ID and name for the first surviving root. Derived sibling workspaces receive new IDs and directory-derived names.

This keeps the migration conservative and data-preserving without trying to keep unsupported legacy shapes alive forever.

Alternatives considered:
- Silently keep only the first root and discard the rest: rejected as too lossy.
- Keep multi-root data on disk forever and only narrow the UI: rejected because it preserves contradictory contracts.
- Try to represent file-root workspaces as special-case single-root files: rejected because it reintroduces the same split semantics we are removing.

### 6. Workspace-aware consumers need scope-aware APIs and notifications

The current sidebar API surface mixes "all workspace roots" with "currently filtered note scope." That should be replaced with clearer queries and callbacks, for example:

- a scope-aware directory-root query for search and palette indexing
- a scope-aware workspace-path query for note and export workflows
- a structure-changed callback for added, renamed, removed, or replaced workspaces
- a scope-changed callback for selector changes and restored-scope updates

Search and palette should stop binding themselves to "all directory roots" by default. They should rebuild from the current workspace scope unless the user explicitly selected `All workspaces`.

Alternatives considered:
- Reuse the existing `workspace_changed` callback for everything: workable, but it hides the important distinction between scope changes and structural changes.
- Let each window subsystem read selector state directly from the sidebar: rejected because it recreates the current coupling problem.

### 7. The sidebar shell spec should stay focused on workspace identity and navigation, not absorb file peek

File peek is a real sidebar behavior, but it does not define what a workspace is or how workspace scope should govern the rest of the app. This change will therefore leave peek unchanged and out of scope, so the new contract stays centered on:

- the fixed top affordance row
- empty-state shell behavior
- one section per workspace
- section header actions
- local drill-down and back navigation

Alternatives considered:
- Fold file peek into the new sidebar-shell capability: rejected because it would enlarge the change without helping the core workspace alignment problem.

### 8. Workspace refresh remains section-local, but now assumes one persisted root per section

The existing `workspace-tree-refresh` architecture still fits. Refresh, watcher targets, materialized-scope reloads, and replace-root flows already operate naturally at the section level. The main change is that this contract becomes explicit: a section refreshes one persisted workspace root and its drill-down descendants, never a bundle of unrelated roots or standalone files.

Alternatives considered:
- Rework refresh into a new top-level workspace manager abstraction immediately: rejected because the current section-local refresh architecture already matches the desired shell model.

## Risks / Trade-offs

- [Legacy migration may surprise users who previously relied on odd multi-root bundles] -> Normalize conservatively, preserve as much state as possible, and fall back to explicit `All workspaces` instead of silently narrowing results.
- [Making workspace scope real will change default search and palette results] -> Keep `All workspaces` as an explicit aggregate escape hatch and cover the new behavior with focused widget and integration tests.
- [Replacing `active_workspace` with a richer scope model touches multiple modules at once] -> Make scope ownership explicit in the window shell and separate structure-changed from scope-changed callbacks.
- [Empty-state alignment may require retiring lazy default-workspace helpers] -> Limit that cleanup to restore and persistence paths first, and keep explicit workspace-creation flows straightforward.
- [Legacy file-root migration cannot preserve exact old semantics] -> Promote file roots to parent-directory workspaces when possible and surface lightweight feedback if an entry must be dropped.

## Migration Plan

1. Introduce the new single-root workspace persistence shape and explicit workspace-scope representation in the model and loader.
2. Teach the loader to read legacy workspace files and normalize them into the new in-memory shape.
3. Route sidebar selector changes through the shared workspace-scope state and add explicit scope-changed notifications.
4. Update search, palette indexing, notes, and export flows to consume scope-aware queries instead of "all roots" queries.
5. Update sidebar shell and persistence tests to cover empty-state behavior, single-root migration, scope restoration, and downstream scoping.
6. Persist the normalized shape back to disk only through the new writer path.

If rollback is needed before normalized data is persisted, the app can continue reading the legacy file. Once a legacy file has been rewritten in normalized form, rollback should restore a backup of the original workspace file if exact legacy grouping is required.

## Open Questions

None blocking. The remaining work is mostly execution detail inside the agreed contract rather than product-direction ambiguity.
