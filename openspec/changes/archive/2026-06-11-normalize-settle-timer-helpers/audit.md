## Timer-Site Audit

Scope: `crates/lushtext-core/src/ui/**` plus directly related service helpers.
This phase stays application-private: no public GTK Lush API, no family-crate
dependency, and no extraction into a `gtk-lush-settle` crate.

## Convert With Private Helpers

### Debounce

- `ui/command_palette/imp.rs` query debounce: convert to `Debounce`; preserve
  immediate empty-query rebuild and async stale-result rejection.
- `ui/command_palette/mod.rs` index-update flush: convert to `Debounce`;
  preserve pending-update coalescing and visible-palette rebuild.
- `ui/search_panel/imp.rs` search and glob entry debounce: convert to
  `Debounce`; preserve history-restore guard and immediate empty-query search.
- `ui/window/preview.rs` preview render debounce: convert to `Debounce`;
  preserve markdown-only rebuild trigger.
- `ui/window/focus_indexing.rs` command-palette file-index rebuild debounce:
  convert to `Debounce`; preserve async stale-generation checks.
- `ui/window/session_persistence.rs` debounced save: convert to `Debounce`
  while keeping ordered generation values for `session_service::save_ordered`.
- `ui/sidebar/workspaces.rs` workspace persistence debounce: convert to
  `Debounce`; preserve dirty/inflight latest-state-wins ordering.
- `ui/sidebar/workspace_section/refresh.rs` refresh debounce: convert to
  `Debounce`; preserve pending path accumulation and full-reload promotion.
- `ui/editor_page/monitor.rs` file-monitor debounce: convert to `Debounce`;
  preserve mtime probe freshness.
- `ui/window/notes.rs` notes-browser and bookmark dialog search debounces:
  convert to `Debounce`; preserve live model rebuild and preview freshness.

### Superseding One-Shot

- `ui/status_bar/mod.rs` pulse cleanup: convert to `SupersedingTimer`.
- `ui/window/focus_mode.rs` focus-mode affordance hide: convert to
  `SupersedingTimer`.
- `ui/window/search.rs` delayed search-progress visibility: convert to
  `SupersedingTimer`; keep recurring heartbeat explicit.
- `ui/window/drafts.rs` first-dirty autosave one-shot: convert only if the
  no-op-on-stale behavior still matches explicit cancellation.
- `ui/sidebar/workspace_section/watch.rs` deferred watcher startup: convert
  only if stop/restart invalidation remains explicit; keep watch poller as
  `SourceId`.

### Delayed Settle / Repair

- `ui/window/preview.rs` preview layout settle: convert to `SettleBurst`;
  preserve `preview_transition_active` readiness.
- `ui/markdown_preview/mod.rs` code-block width repair: convert only if
  pending code-block repair state and source cleanup semantics remain intact.
- `ui/editor_page/minimap.rs` minimap refresh, reflow settle, and reveal delay:
  convert to `Debounce`/`SettleBurst`; preserve `minimap-refresh` readiness
  blocker and reveal/freeze ordering.

## Keep Explicit Or Defer

- Recurring pollers and heartbeats stay explicit: automation readiness
  `timeout_future`, search runtime progress heartbeat, notification sweep,
  draft periodic autosave, local-history periodic capture, focus-index retry
  polling, and workspace watcher polling.
- Chunked-yield/model-population callbacks stay explicit in this phase:
  buffer snapshots, TreeListModel child batches, row restoration, folder
  population, pending inline rename focus, and transient-surface idle dismissal.
- Idle coalescers stay explicit until the helper has an idle/yield primitive:
  dynamic overscroll repair uses `idle_add_local_once` to run after GTK
  allocation churn without changing the timeout cadence.
- Stale async freshness tokens remain outside settle helpers: Replace preview,
  undo-backup persistence, sidebar peek, local-history preview loads, encoding
  probes, notes preview I/O, and similar worker-result guards.
- Pure service/model generation counters stay out of scope: session save
  ordering, notification-bus generations, migration-ledger generations,
  durable-write state, and content-search stream/run identifiers.
- Lifecycle or maintenance delays stay explicit unless a later helper adds a
  clearer fit: workspace filter animation fallback, draft orphan-cleanup startup
  delay, transparency style-scheme retry, async-task back-pressure retry, async
  result delivery idle hops, and Markdown code-block idle-plus-timeout repair
  with explicit `SourceId` cancellation.
