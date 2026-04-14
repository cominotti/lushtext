## Context

LushText's sidebar already has the core pieces needed for a refresh feature: workspace sections own their own tree models, directory scans already happen off the GTK thread, and large directory population is batched back onto the main loop. What is missing is a long-lived mechanism that notices external filesystem changes and reuses that tree-loading pipeline without forcing users to manually rebuild a section.

This change crosses service, widget-template, section state, and test layers. It also introduces a new performance-sensitive behavior: external file churn must not trigger whole-sidebar rebuilds or a flood of overlapping background scans. The design therefore needs a watcher strategy, a refresh scheduler, and a state-preserving reload path that fits the existing `workspace_section/` split.

## Goals / Non-Goals

**Goals:**
- Keep each workspace section aligned with on-disk file and directory changes that happen outside LushText.
- Add a manual `Refresh` button to the workspace header immediately left of the existing replace-root button.
- Reuse the existing async tree-loading and batching model instead of inventing a second loading path.
- Coalesce bursty filesystem activity so refresh work stays bounded and predictable.
- Preserve drill-down, expanded rows, and current selection whenever the changed paths still exist after refresh.

**Non-Goals:**
- Building a generic file-watcher framework for the whole app beyond workspace-tree refresh.
- Guaranteeing zero-latency UI updates for every single filesystem event; bounded, coalesced refresh is preferred over noisy churn.
- Replacing the existing editor-tab file monitor or unifying it with workspace refresh in the first iteration.
- Introducing always-on polling loops as the primary refresh mechanism.

## Decisions

### 1. Add a service-layer watcher adapter, but scope live watches to the materialized sidebar tree

Automatic refresh still needs real external-change visibility, but broad workspace roots like a user's home directory make recursive startup watches too expensive and can fail immediately on unreadable deep descendants that are nowhere near the rendered tree. The design therefore uses a service-layer watcher adapter, likely `services/workspace_watch.rs`, backed by an OS filesystem watching crate such as `notify`, while the UI subscribes only to the directories it has actually materialized: visible root rows plus any expanded directories that currently own loaded child stores.

Keeping the watcher in the service layer preserves the codebase's architecture boundary: GTK widgets do not own backend watcher APIs directly, and the watcher output remains testable without a live widget. The UI layer only decides how to react to a refresh event.

Alternatives considered:
- Recursive watch of every configured root: simple to describe, but too costly for broad roots and fragile when unreadable deep descendants exist under paths the user is not currently viewing.
- `gio::FileMonitor` per visible directory: integrates well with GTK, but still needs the same scope decision and offers less debounced normalization than the chosen adapter.
- Periodic rescans: simpler, but wastes work when nothing changes and makes the UI feel laggy.
- Reusing editor-page monitors: those are file-scoped and intentionally tied to open tabs, not workspace trees.

### 2. Route manual and automatic refresh through one section-scoped refresh controller

`LushtextWorkspaceSection` will gain a single refresh entry point that both the new header button and watcher events use. That controller will own a refresh generation counter, a short debounce window, and an in-flight guard so bursty rename/create/remove sequences collapse into one follow-up refresh instead of spawning overlapping rebuilds.

Unifying the pipeline keeps behavior consistent: the manual button becomes a guaranteed escape hatch because it exercises the same code path as the watcher. It also keeps failure handling, selection restoration, and tree-cache cleanup in one place instead of duplicating them across button and watcher code.

Alternatives considered:
- Separate manual and automatic code paths: easier to start, but more likely to drift or fix bugs in only one path.
- Letting the top-level sidebar orchestrate every refresh: workable, but it would pull section-local tree logic up a layer that the current sidebar AGENTS guidance explicitly keeps inside `workspace_section/`.

### 3. Prefer path-scoped subtree reloads, with whole-section reload as a safe fallback

When a watcher event maps cleanly to a loaded directory row, the section should reload only that directory's child store and cache state. Root-level changes, drill-down root changes, and ambiguous rename/move events should fall back to reloading the current root model for that workspace section. This keeps refresh cost proportional to the affected area when possible while preserving correctness for shape-changing events.

The existing tree state already tracks root paths, child paths, item locations, expanded rows, and pending selection. That makes path-scoped refresh feasible without inventing a parallel tree representation. Whole-section reload remains important as the correctness fallback when the nearest affected ancestor is unknown or no longer exists.

Alternatives considered:
- Always rebuild the entire sidebar: correct but unnecessarily expensive and visually disruptive.
- Always rebuild the entire workspace section: simpler and still acceptable for some cases, but it throws away too much loaded state for common one-directory changes.
- Try to patch individual rows in place from raw watcher events: fastest in theory, but much harder to keep correct across renames, truncation placeholders, and empty-directory hints.

### 4. Preserve user context by snapshotting section state before applying refresh work

Before any refresh mutates models, the section will snapshot the current drill-down target, expanded paths, and selected path. After the refresh completes, it will restore whichever of those paths still exist. If the selected row disappeared, the section will keep the tree consistent without forcing focus to jump elsewhere.

This matches how the sidebar already thinks about rebuilds: expanded paths and pending selection are explicit pieces of state. Extending that pattern to automatic refresh prevents a common failure mode where a correct data refresh still feels broken because it collapses the user's current navigation context.

Alternatives considered:
- Resetting the tree to a collapsed default after every refresh: easy, but frustrating for large workspaces.
- Preserving only expanded state and ignoring selection/drill-down: partially helpful, but still disruptive for active browsing workflows.

### 5. Surface refresh failures through existing sidebar-to-window feedback channels

Watcher startup failure, watcher overflow, or a manual refresh that cannot reload a path should produce lightweight user-visible feedback, most naturally through the window's existing status/notification path. The sidebar section will therefore expose refresh-status callbacks upward rather than trying to own user messaging locally.

This keeps the workspace widget focused on tree behavior while still giving users a recoverable experience: they can see that automatic watching degraded and still use the manual `Refresh` control.

Alternatives considered:
- Logging only: insufficient because the user never learns why the tree stopped updating.
- Embedding inline error chrome inside each section header: heavier UI churn than needed for a status-style event.

### 6. Start watchers after the initial tree is rendered, not in the same critical path

Even scoped directory watches have real setup cost. The section should therefore schedule watcher startup onto the next main-loop turn after the root model is installed, and it should drop stale scheduled starts if the visible scope changes again before startup runs. This lets the window paint workspaces and restored session tabs before paying even the smaller watch-setup cost.

Alternatives considered:
- Starting the scoped watcher synchronously during section construction: still risks launch-time stalls on large roots.
- Waiting for a much longer arbitrary timeout: hides the work, but makes automatic refresh feel inconsistent and harder to reason about.

### 7. Ignore watcher noise and reconcile child stores in place

The watcher service should forward only tree-shape-changing events into the sidebar refresh pipeline. Access-only noise such as `Access(Open(...))`, file content changes, and metadata churn do not affect which rows the sidebar should render, and on broad roots they can arrive continuously. Those events must be filtered out before they reach the UI.

For actual create/remove/rename changes inside a loaded directory, the section should reconcile the existing `gio::ListStore` with the new scan result using a bounded splice over the changed slice instead of `remove_all()` followed by repopulation. This keeps unchanged rows mounted, which is necessary for the no-flicker UX contract.

Alternatives considered:
- Accept all watcher events and rely on debounce alone: still causes refresh loops and visible churn when the backend emits noisy access events.
- Rebuild the whole child store on every legitimate refresh: simpler, but violates the visual-stability requirement.
- Full Myers-style diff for every directory refresh: theoretically minimal, but a prefix/suffix splice around the changed region is simpler and adequate for the sorted tree model here.

## Risks / Trade-offs

- [Filesystem watching adds a new dependency and backend-specific behavior] -> Keep the watcher behind a small service abstraction and add deterministic service-level tests for event coalescing.
- [Rapid change bursts can queue too many reloads] -> Use per-section debounce plus an in-flight generation guard so only the latest refresh runs after churn settles.
- [Path-scoped reload logic can miss edge cases around renames and deleted ancestors] -> Fall back to whole-section reload whenever the affected ancestor is ambiguous or gone.
- [State restoration can accidentally reopen collapsed areas after a big rename] -> Restore only paths that still exist after the refresh and silently drop stale ones.
- [Scoped watches may miss changes inside collapsed or not-yet-loaded descendants] -> Keep manual `Refresh` as the explicit fallback and refresh loaded scopes immediately when those directories are expanded.
- [Watcher setup may still fail in some environments] -> Surface that failure to the user and keep the manual `Refresh` button always available.
- [Incremental reconciliation can desynchronize caches if the store and path index drift] -> Rebuild the direct-child cache from the post-splice store contents after every subtree refresh.

## Migration Plan

No data migration is required. Implementation adds runtime watcher setup when a workspace section loads roots and tears that watcher down when the section is rebuilt or disposed. If the feature must be rolled back, removing the watcher adapter and refresh button returns the sidebar to its current scan-on-demand behavior without touching persisted workspace state.

Flatpak and CI follow-up will include regenerating any vendored dependency metadata if a new watcher crate is introduced.

## Open Questions

None blocking. During implementation we can still choose the exact watcher crate and debounce duration as long as the final behavior satisfies the spec and preserves the sidebar's current performance contracts.
