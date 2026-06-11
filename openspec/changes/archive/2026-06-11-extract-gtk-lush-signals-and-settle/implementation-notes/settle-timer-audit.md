# Settle and Timer Ownership Audit

Scope: current checkout under `crates/lushtext-core/src/ui/**` plus directly
related service helpers. This note records the starting inventory and migration
outcome for `extract-gtk-lush-signals-and-settle`.

## Method

- Searched for private `crate::ui::settle` imports and uses, GLib one-shot and
  recurring timers, timeout futures, idle deferrals, `SourceId` ownership, and
  generation counters.
- Compared the results with the archived `normalize-settle-timer-helpers`
  audit and the new `gtk-lush-settle` spec.
- Classified sites as direct `gtk-lush-settle` migrations or retained explicit
  timer/freshness classes.

## Direct `gtk-lush-settle` Migration Candidates

These sites already use the private Phase 0 helper and should migrate
mechanically to the public leaf crate once the crate API exists:

| Area | Current primitive | Planned treatment |
| --- | --- | --- |
| `ui/command_palette/imp.rs` | `Debounce` for query search and index update flush. | Migrate to `gtk_lush_settle::Debounce`, preserving immediate empty-query handling and stale async result rejection. |
| `ui/search_panel/imp.rs` | `Debounce` for search and glob entries. | Migrate while preserving history-restore guards and immediate empty-query behavior. |
| `ui/window/notes.rs` | `Debounce` for notes browser and bookmark dialog search. | Migrate while retaining separate preview freshness generation. |
| `ui/window/imp.rs` and related modules | `Debounce`, `SettleBurst`, and `SupersedingTimer` fields for session save, preview render, focus-index rebuild, preview settle, focus affordance, draft first-dirty autosave, and search progress visibility. | Migrate in risk order; preserve duration constants and readiness semantics. |
| `ui/status_bar/imp.rs` | `SupersedingTimer` for pulse cleanup. | Low-risk first superseding-timer migration. |
| `ui/editor_page/imp.rs` and `ui/editor_page/minimap.rs` | `Debounce` and `SettleBurst` for file monitor, sidecar saves, minimap refresh, and minimap reflow. | Migrate after the crate API is proven; preserve minimap readiness and visual geometry. |
| `ui/sidebar/imp.rs` | `Debounce` for workspace persistence. | Preserve dirty/inflight latest-state-wins behavior. |
| `ui/sidebar/workspace_section/imp.rs` and `refresh.rs` | `Debounce` for refresh request coalescing. | Preserve pending-path accumulation and full-reload promotion. |

## Migrated Settle Sites

The private `crate::ui::settle` module was deleted after all direct consumers
were migrated to `gtk_lush_settle`.

| Area | Migrated primitive | Notes |
| --- | --- | --- |
| `ui/command_palette/imp.rs` | `Debounce` | Query search and index flush preserve immediate empty-state and stale-token checks. |
| `ui/search_panel/imp.rs` | `Debounce` | Search/glob debounces keep their existing history-restore and empty-query guards. |
| `ui/window/notes.rs` | `Debounce` | Notes and bookmark-dialog search keep separate preview freshness generations. |
| `ui/window/imp.rs` and related modules | `Debounce`, `SettleBurst`, `SupersedingTimer` | Session, preview, focus-index, focus affordance, first-dirty autosave, progress visibility, and preview settle fields now use the leaf crate. |
| `ui/status_bar/imp.rs` | `SupersedingTimer` | Pulse cleanup preserves delayed latest-generation cleanup. |
| `ui/editor_page/imp.rs` and `ui/editor_page/minimap.rs` | `Debounce`, `SettleBurst`, `SettleHandle` | File monitor, sidecar persistence, minimap refresh, and minimap reflow keep readiness-visible pending semantics. |
| `ui/sidebar/imp.rs` | `Debounce` | Workspace persistence still uses dirty/inflight latest-state-wins guards outside the timer helper. |
| `ui/sidebar/workspace_section/imp.rs` and `refresh.rs` | `Debounce` | Refresh coalescing keeps pending-path accumulation and full-reload promotion. |

## Retained Explicit Timer Classes

These sites should remain outside `gtk-lush-settle` unless a later focused
requirement changes their lifecycle:

- Recurring pollers and heartbeats:
  `ui/automation.rs` timeout futures, search runtime progress heartbeat,
  notification sweep, draft periodic autosave, local-history periodic capture,
  focus-index retry polling, and workspace watcher polling.
- Chunked-yield/model-population callbacks:
  buffer snapshots, `TreeListModel` population batches, sidebar row restore,
  folder population, pending inline rename focus, and transient-surface idle
  dismissal.
- Idle allocation repair:
  dynamic overscroll repair and other allocation-after-idle fixes whose timing
  is intentionally tied to GTK allocation churn rather than a settle window.
- Async worker freshness:
  replace preview generation, undo-backup persistence, sidebar peek preview,
  local-history preview loads, encoding probes, notes preview I/O, file loads,
  and lossy-encoding analysis.
- Pure service/model generations:
  ordered session save generations, notification-bus generations,
  migration-ledger generations, durable-write state, and content-search stream
  identifiers.
- Lifecycle delays with explicit source ownership:
  workspace watcher start/poll state, transparency style-scheme retry,
  async-task back-pressure retry, async result idle hops, and Markdown
  code-block idle-plus-timeout repair with `SourceId` cancellation.

## Conflict Check

Active OpenSpec changes at inventory time:

- `add-snap-packaging`: packaging-only artifacts; no overlap with GTK Lush
  crates, private settle helpers, or timer rules.
- `evaluate-qt-quick-redefinition`: no task artifacts or files present under
  the change directory in this checkout.

No conflicting active OpenSpec work was found for private settle migration or
the future rule updates.
