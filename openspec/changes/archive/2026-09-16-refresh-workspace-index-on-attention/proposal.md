## Why

Switching a git branch, running a build, or creating a file from a terminal
leaves the command palette blind to the result. The palette's file index has
exactly three update paths — a full rebuild on workspace-membership or
visibility change, and three incremental mutations driven by the sidebar's own
context-menu create/rename/delete (`ui/window/documents.rs:114/125/132`) — and
none of them observes the filesystem.
`notify_workspace_structure_changed` is emitted only for workspace membership
operations; the sidebar's filesystem watcher never reaches the index. An
externally created file therefore never appears in `Ctrl+Shift+P` until the user
changes workspace scope, toggles hidden files, or restarts the application.

Widening the sidebar's watcher would not fix it. The sidebar deliberately
watches only materialized rows — top-level folders and expanded directories,
non-recursively (`workspace_section/watch.rs:515`) — while the index is built
recursively over the whole workspace. The notifying set is a small fraction of
the indexed set.

The sidebar has a narrower instance of the same staleness. Expanding a directory
re-scans it, so collapsed subtrees are correct the moment they are opened. But
the `is_empty` hint (`services/file_tree.rs:36`) is computed by the **parent's**
lookahead scan and then frozen, and a non-recursive watch on the parent does not
fire when an entry is created inside a collapsed child. A directory that gained
content externally keeps its `(Empty)` label, keeps its expander arrow hidden,
and keeps `Focus Folder` disabled.

Finally, a gap entirely inside the application: saving an untitled document into
a workspace folder, or a Save As to a new workspace path, never updates the
index either. The only three mutation call sites are sidebar-driven.

## What Changes

- Refresh both workspace surfaces at **user-attention moments** — the window
  becoming active (`notify::is-active`) and the command palette opening — by
  calling machinery that already exists:
  - the palette's existing debounced, generation-guarded, cancellable
    `rebuild_file_index()` (`ui/window/palette_shell.rs:149`);
  - the sidebar's existing silent, debounced, splice-reconciling
    `queue_auto_full_refresh()`
    (`ui/sidebar/workspace_section/refresh_execution.rs:110`), which is the same
    path `Ctrl+Shift+H` already drives interactively today and which recomputes
    the frozen `is_empty` hints as a matter of course
    (`scan_execution.rs:1120`).
- Add one process-wide minimum interval, adaptive to the previous pass's
  observed duration, so a focus-follows-mouse desktop, a multi-window session,
  or a slow network filesystem cannot turn attention into an I/O storm.
- Suppress a refresh while a save, a close transaction, a draft autosave, or a
  file chooser is in flight. `notify::is-active` also flips when a native or
  portal `FileDialog` opens and closes, so the naive trigger would fire
  precisely when a save is about to be queued — and a save-in-flight check alone
  is too late, because the window reactivates before the save starts. Both
  compete for `spawn_blocking_then`'s eight global slots.
- Route save and Save As of a path inside the current workspace scope through
  `update_index_file_created`, admitting the canonical identity the durable
  writer resolved rather than the requested path, so a symlink-backed save
  cannot index an entry that opens a different file.
- No new readiness predicate. The existing `command-palette-index` blocker
  already covers `file_index_builds.has_work()` and pending index updates
  (`ui/automation.rs:741`), and `workspace-refresh-complete` already covers the
  sidebar. Both new triggers are observable through them unchanged.

### What this change deliberately does not build

An earlier revision of this proposal specified a directory-mtime staleness
sweep, a retained per-index directory set under its own byte budget, a typed
incomplete-revalidation outcome, and a new readiness predicate. Review and
measurement retired it; `design.md` records the evidence. The short version is
that for the motivating scenario — a branch switch — a change set above
`MAX_PENDING_INDEX_UPDATES` (1,024) escalates to a full rebuild anyway
(`ui/command_palette/policy.rs:178`), so the sweep would pay full detection cost
and then discard it. The deferral and the conditions that would justify
revisiting it are recorded in `design.md`.

## Capabilities

### New Capabilities
- `workspace-attention-refresh`: the attention-moment trigger contract shared by
  the palette index and the workspace tree — which moments refresh, the
  process-wide adaptive interval, suppression while user-work operations are in
  flight, and the requirement that refreshing reuses each surface's existing
  bounded path rather than introducing a second one.

### Modified Capabilities
- `command-palette-source-groups`: the file index gains a freshness obligation
  against externally created, removed, and renamed files, and in-app saves
  become an index mutation source.
- `workspace-tree-refresh`: the existing automatic-refresh requirement is
  amended to state that watch-driven refresh covers the materialized scope while
  entry-set changes inside collapsed children are covered by attention-moment
  refresh, which is what makes the frozen emptiness hint correct again.

## Impact

> **Corrected after implementation.** The list below was written as a plan and
> named three destinations the code did not take. What shipped, per the
> `design.md` decisions and both matrix rows: the throttle turned out to have
> **two** owning workflows, which the workflow convention makes cross-cutting,
> so the pure policy went to a new `model/attention_refresh.rs` rather than to
> `ui/command_palette/policy.rs`, and its coordination to a new roleless
> `ui/window/attention_refresh.rs` rather than to `index_admission.rs` and
> `ui/sidebar/policy.rs`. **No evidence surface gained fields**: the observable
> fact is a decision about one event rather than retained state, so
> `refresh_workspace_surfaces_on_attention` returns it to its caller
> (Decision 6 records that reversal). The modal-surface suppression also grew
> past "a file chooser" to cover the sidebar's Add Folder chooser and the native
> print dialog, which review found unguarded.

- New: `crates/lushtext-core/src/model/attention_refresh.rs` (cross-cutting pure
  throttle policy) and `crates/lushtext-core/src/ui/window/attention_refresh.rs`
  (cross-cutting coordination, no role).
- Modified: `ui/window/imp.rs` (the `notify::is-active` hookup, beside the
  existing visibility hookup), `ui/window/palette_shell.rs` (trigger on open and
  the settle hook in the build terminal), `ui/window/dialogs.rs` (the Save As
  index mutation plus two modal guards — **not** `ui/editor_page/save/`, which
  has no palette reach-through and must not gain one), `ui/sidebar/mod.rs` and
  `ui/sidebar/workspace_section/refresh_execution.rs` (one silent refresh entry
  point instead of a copied one), `ui/sidebar/dialogs.rs` and
  `ui/window/print/execution.rs` (the two further modal guards),
  `ui/window/mod.rs` (re-exports).
- No new service, no new dependency, no new filesystem-boundary API, no new
  persisted format, no new GSettings key, no new action, no new D-Bus surface,
  and no new readiness predicate.
- Docs: `AGENTS.md` (key design decisions), `docs/workflow-readability-matrix.md`
  (both `WFR-COMMAND-PALETTE` and the workspace-tree row record the shared
  cross-cutting policy and the roleless GTK half), `docs/accessibility.md` and
  `docs/accessibility-matrix.md` (the sidebar row's expander and `Focus Folder`
  affordances change under a user who may be focused on the row, silently),
  `docs/end-user-coverage.md`, and `README.md` (a new user-visible behavior).
  No `.agents/rules/` change: the work applied existing conventions rather than
  establishing one.
