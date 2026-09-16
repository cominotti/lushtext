## Context

The command-palette file index is the only workspace surface in LushText that
caches what exists on disk. Content search re-walks the filesystem on every
query; the sidebar re-scans a directory whenever it is expanded. Only the index
holds a snapshot, and it is consulted precisely about files that are **not** on
screen — so it is the one surface that can be confidently wrong.

Facts from the code that set the shape of this design:

1. **The index has no filesystem input.** Full rebuild from
   `refresh_workspace_scope_consumers` and `connect_visibility_changed`; three
   incremental mutations from `ui/window/documents.rs:114/125/132`.
   `notify_workspace_structure_changed` never comes from a watcher.
2. **The sidebar's watch set is a fraction of the indexed set.**
   `watch.rs:515` watches a directory row only when `row.depth() == 0 ||
   row.is_expanded()`, non-recursively.
3. **Both surfaces already own a correct, bounded, generation-guarded full
   refresh.** `rebuild_file_index()` is 300 ms-debounced, single-flight via
   `file_index_builds.submit`, off-GTK, cancellable, admitted through
   `plain_disposal`, and already covered by the `command-palette-index`
   readiness blocker (`ui/automation.rs:741`).
   `queue_auto_full_refresh()` is debounced, silent, splice-reconciling,
   preserves expansion and scroll state, and is covered by
   `workspace-refresh-complete`.
4. **A full refresh at an interactive moment is already shipped behavior.**
   `Ctrl+Shift+H` calls `rebuild_file_index()` and
   `refresh_for_visibility_change()` directly. If a refresh were too expensive to
   run at a user-attention moment, that would already be a bug today.
5. **The sidebar's frozen hint is recomputed by its own refresh.**
   `scan_execution.rs:1120` rebuilds `FileTreeItem::new(path, is_dir,
   row.is_empty)` from a fresh lookahead scan, and every collapsed row has a
   materialized parent by definition — that is what makes it rendered.

## Goals / Non-Goals

**Goals:**

- A file created, removed, or renamed by any process, at any depth the index
  covers, is reflected in the command palette by the time the user next looks at
  it.
- A collapsed directory that gained or lost content stops lying about being
  empty.
- A file the application itself saves into the workspace is indexed immediately,
  without waiting for any refresh.
- No second refresh mechanism. Each surface refreshes through the bounded path
  it already owns.

**Non-Goals:**

- Sub-second propagation, or filesystem watching for the index.
- A palette already open when a refresh completes is not *required* to update
  mid-session — but see Decision 6, which observes that the existing code
  already does this and that suppressing it would be the added work.
- Detecting content edits. The index models existence, not content.

## Decisions

### Decision 1: Reuse the existing refresh; do not build a staleness sweep

The first revision of this design specified a directory-mtime sweep over a
retained per-index directory set. Four review axes and one measurement retired
it. Recording the reasoning because the idea is attractive and will recur.

*The measurement*, on this repository (45,638 files / 3,993 directories, warm
cache):

| operation | cost |
| --- | --- |
| `readdir` walk of the whole tree | 45 ms |
| one `stat` per directory | 4 ms |
| `canonicalize` per file, which the real build does | 240 ms |

So the sweep's arithmetic is real: detection alone is roughly 60× cheaper than a
rebuild. The reasons it still loses are not about detection cost.

- **The motivating scenario defeats it.** `MAX_PENDING_INDEX_UPDATES` is 1,024
  (`ui/command_palette/policy.rs:50`); above it, `admit_index_update` returns
  `EscalateToRebuild` (`policy.rs:178`). A branch switch touches far more than
  1,024 files. The sweep would stat every directory, `read_dir` every changed
  one, descend into new subtrees, produce more than 1,024 updates, discard all
  of it, and run a full rebuild — which, via
  `rebuild_current_workspace_folders`, is **not cancellable** (`index.rs:308`
  constructs `PaletteSearchCancellation::default()`). Directory deletion makes
  this near-certain, since the three mutation verbs are per-file.
- **The cheap probe does not exist.** `metadata::file_facts`
  (`filesystem/metadata.rs:18`) is `stat` + `canonicalize` + a second `stat`.
  The 4 ms above is a bare `stat`; through the boundary as it stands the sweep
  costs roughly what the rebuild costs. A new mtime-only boundary accessor would
  have to be added — unacknowledged impact on a module the first revision did
  not list.
- **Half the record is not free.** The premise was "the build already computes
  the set, just retain it". The build computes *paths*; it reads no mtime
  anywhere. Capturing baselines adds up to 100k probes to **every** build, on
  the debounced latency-sensitive path.
- **There is no way to update a baseline.** `FileIndexUpdate` has only `Create`,
  `Delete`, and `Rename`. After a sweep rescans a changed directory, its
  recorded mtime is still the build-time value, so the next attention moment
  rescans it again, forever.
- **The bounded prefix is nondeterministic.** `visited_directories` is a
  `HashSet<PathBuf>` (`index.rs:376`) with no order, so which directories stay
  fresh under the ceiling varies between builds over an identical tree.
- **The retention breaks the disposal accounting.** Both reservation sites use
  `MAX_FILE_INDEX_RETAINED_BYTES` as the weight; extending
  `retained_byte_weight()` without extending the reservation trips
  `debug_assert!(weight <= self.weight)` in `ui/plain_disposal.rs`, and in
  release builds silently over-subscribes the 128 MiB lane.
- **The sidebar cannot use it.** Its question is "is this directory empty under
  the current visibility rule", answered by `is_dir_empty` — a `readdir`, not a
  stat. An mtime can only gate that read, never answer it. And with nothing
  retained across passes the sidebar has no baseline to compare against at all.

*What would justify revisiting it:* a measured rebuild that is too slow at
attention cadence on a real large repository, **plus** a prefix-scoped
`FileIndexUpdate::SubtreeRemoved` and a raised or removed escalation threshold so
the sweep's output is not discarded, **plus** an mtime-only boundary accessor.
Until those exist the sweep is a slower, bespoke, partial reimplementation of a
rebuild that is already correct.

*Alternatives also considered and rejected:* a recursive `notify` watch
(`RecursiveMode::Recursive` has no filter hook, so it would watch `node_modules`
and `target`); a non-recursive watch per indexed directory (inotify watches are
a per-user kernel resource shared with the user's other tools — a foundation
that fails because of an unrelated process's resource use, in a way the
application cannot explain, is the wrong foundation). The latter remains a
viable *accelerator* on top of this design: if the watches register, events
arrive immediately; if not, latency degrades to the next attention moment and
correctness is unchanged.

### Decision 2: Attention moments, not a timer

Two triggers: the window becoming active, and the command palette opening.

Window activation is not a convenience — it is causally correlated with the
event being detected. External changes happen while the application is not
focused, by construction. A periodic timer would catch the same events by
accident, at the cost of waking the process during idle. Palette open covers the
remaining case: a build that completed while the application stayed focused.

### Decision 3: One process-wide adaptive interval

The interval is process-wide and keyed on the workspace folder set, not
per-window: N windows over one workspace must not run N refreshes of the same
tree.

It is **adaptive**, not a constant: the next refresh is refused until a multiple
of the previous pass's observed wall time has elapsed, with a floor and a hard
ceiling. A constant tuned on a local NVMe repository would be catastrophic on
NFS, where the same refresh can be orders of magnitude slower. Making the
throttle a function of observed cost is what lets one rule serve both.

This matters more than it would for a timer-driven design, because
`notify::is-active` is not rare: it flips on every alt-tab, and on a
focus-follows-mouse desktop it flips constantly.

### Decision 4: Suppress while user work is in flight

A refresh is refused, not queued, while any editor is saving, a close
transaction is in flight, or a draft autosave is pending.

`notify::is-active` also flips when a native or portal `FileDialog` opens and
closes — which is to say, exactly around a Save As. A save-in-flight check alone
does **not** close that hole: the window reactivates as the chooser closes,
*before* the resulting save has been queued, so at that instant no editor is
saving. The chooser sites therefore bracket themselves with a process-wide
counter, released at the end of the chooser callback on the cancelled path too.
Either interleaving is then covered — a reactivation arriving before the release
is refused by the counter, and one arriving after it is refused by the save it
has by then started. `spawn_blocking_then` has
eight global slots held until the GLib idle completion consumes the result, and
draft autosave, the close-with-changes pipeline, and the session-save safety net
all pass through it. A refresh blocked in I/O on an unresponsive mount would put
a save behind it, and `close_request` disables input across its yields — so the
user would see a hung window with input disabled. Refusing is correct rather
than queueing: the next attention moment is moments away.

### Decision 5: The save gap is fixed at the window, not in the save role home

`ui/editor_page/save/` has no palette reach-through and must not gain one — the
palette is a window template child, and every existing mutation is issued from
`ui/window/documents.rs`. The Save As completion is `complete_save_as` in
`ui/window/dialogs.rs`. Those are the correct sites, and keeping them there
preserves the `WFR-DOCUMENT-SAVE` role home's layering.

The admitted path is the **canonical identity the durable writer resolved**, not
the requested path. The write boundary keeps symlink-backed saves writing the
resolved target instead of replacing the link; indexing the requested path would
let the palette offer an entry that opens a different file than the one saved.
The canonical prefix test against the index's workspace folders is done on the
worker and carried out in the save result, not run as a blocking syscall on the
GTK save terminal.

### Decision 6: No new readiness predicate

`command-palette-index` already blocks on `file_index_builds.has_work()` and
pending index updates; `workspace-refresh-complete` already covers the sidebar.
Both new triggers are observable through them without change. Adding a predicate
would be a permanent public D-Bus commitment for an internal detail, and the
predicate list is append-only.

**Reversed during implementation.** This decision originally also committed the
interval and suppression state to the existing evidence surfaces. That turned
out to be the wrong shape: the observable fact is a *decision about one event*,
not retained state, and `refresh_workspace_surfaces_on_attention` already
returns it. Returning it keeps the decision at the production entry point rather
than behind either a new evidence field or a `*_for_test` getter, and the
externally reachable `*_for_test` count stays at 165, inside the recorded
ceiling. The cost is `#[must_use]` plus two explicit discards at the trigger
sites, which is cheaper than a surface field no production path reads.

On the related question of whether an already-open palette updates when a
refresh completes: `install_replacement_file_index` already calls
`restart_query_if_open()` unconditionally. The existing behavior is to update.
*Not* updating would require new suppression logic, so this design keeps the
existing behavior and the former Non-Goal is stated as non-binding rather than
as a constraint to implement against.

### Decision 7: The sidebar half is one call, and it belongs here

The first revision left this as an open question. The answer is that it belongs
here, because it is now one call to an existing function rather than the
row-splicing project it looked like when it was going to be built on a new
mechanism. `queue_auto_full_refresh()` is already silent, debounced,
generation-guarded, and state-preserving, and the refresh path already recomputes
`is_empty` from a fresh lookahead. Bundling costs nothing and keeps one trigger
serving both surfaces.

## Risks / Trade-offs

- **[A refresh at attention cadence is too expensive on a very large
  repository]** → Adaptive interval (Decision 3), the existing debounce, and the
  existing cancellable single-flight. The `Ctrl+Shift+H` precedent shows a full
  refresh at an interactive moment is already accepted. If measurement
  contradicts this, Decision 1 records exactly what a sweep would need first.
- **[`notify::is-active` storms on focus-follows-mouse]** → Process-wide
  adaptive interval; refuse rather than queue.
- **[A refresh starving a save or draft autosave]** → Decision 4 suppression,
  plus the refresh's existing cooperative cancellation.
- **[The escalation path's rebuild is uncancellable]** → Pre-existing
  (`index.rs:308`). This change does not create mutations and so does not
  increase escalation frequency, but it is recorded here because a future sweep
  would make it load-bearing.
- **[A symlinked save indexing the wrong identity]** → Decision 5.
- **[Warm-cache benchmarks hiding the real cost]** → A Criterion benchmark over
  a tempdir runs against a hot dentry cache and will report everything as fast.
  Coverage must include a delayed-filesystem lane through the existing
  `services::filesystem::fixture` seam, asserting a latency budget and that
  cancellation aborts within a bounded number of operations.

## Migration Plan

No persisted format, GSettings key, action, or automation surface changes, so
there is no migration and no rollback state. Removing the two trigger call sites
restores today's behavior exactly, which makes the change bisectable in one
edit.

1. The save / Save As index gap — independent, tiny, and the only part that is
   certainly a bug regardless of the rest.
2. The interval and suppression policy, as pure functions with unit and property
   tests, before any wiring.
3. The palette trigger.
4. The sidebar trigger.
5. Documentation and gates.

## Deferrals

Two structural observations this change surfaced but deliberately did not act
on. Both are recorded so a later change does not have to rediscover them.

- **There is no single "user work in flight" concept.** The suppression
  predicate here ORs five state sources, and it is the *fourth* place in the
  shell to ask the same question — `close_request` and the close-safety
  fingerprint check compose their own, with different memberships and no
  authoritative one. `session_restore/mod.rs` and `drafts/mod.rs` already
  document `close_safety_inflight` as shared with no owner. Converging them on
  one typed `user_work_in_flight()` is the right change and is larger than this
  one, because `close_request` needs a per-reason message while this caller
  needs only the boolean.
- **There is no general "a file was written" notification for the index.** Four
  call sites must each remember to tell the index, and nothing checks that they
  do — which is precisely the defect this change fixes, one site having
  forgotten. An audit of every writer found no *second* live instance today, so
  the fix is complete; the structural risk is that the fifth writer inherits it.
  The general alternative is one index-notification seam at the durable-write
  terminal, which already resolves the canonical target and knows whether a file
  was created or replaced.

- **The symlinked-save scenario has no test.** The spec asserts that a save
  through a symbolic link is indexed as its resolved target. That holds by
  construction — the mutation worker's `indexed_file_from_path` canonicalizes
  the admitted path, which resolves the link — but the widget suite covers only
  the plain case. A fixture creating a real symlink would close it.

## Open Questions

None outstanding. The first revision's four questions are resolved: the interval
is adaptive rather than a measured constant (Decision 3); the already-open
palette keeps its existing update behavior (Decision 6); there is no incomplete
state to surface, because there is no retained set; and the sidebar belongs in
this change (Decision 7).
