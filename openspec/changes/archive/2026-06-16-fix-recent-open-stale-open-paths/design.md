## Context

The Open popover already has durable recent-document storage, an app-owned row model, and explicit filtering for documents that are currently open. The reported failure is more subtle than missing persistence: valid recent-document entries can exist on disk and in memory while the popover still renders the empty state after every corresponding tab has been closed in the same process.

The live investigation found persisted recent entries, regular existing files, no restored session tabs, and an empty Open popover. Replaying an explicit file open and close updated `recent-documents.json` while the popover still showed no recent rows. That points at stale open-document identity state suppressing rows that should be eligible, rather than a failed recent save.

Existing tests cover the clean case where a row is hidden while a tab is open and revealed after a direct close. They do not prove that the visible recent list is resilient when duplicate-tab bookkeeping, canonical path refresh, page detach, or production open/close flows drift from the actual mounted `AdwTabView` pages.

## Goals

- Show persisted recent documents whenever no live editor tab currently owns the same file identity.
- Hide documents that are genuinely open, including alternate display/canonical spellings for the same file.
- Keep duplicate-tab prevention fast and accurate.
- Make same-session close behavior deterministic across tab close, page detach, path mutation, failed load, and real app activation paths.
- Add many regression tests across service logic, window state, GTK widget behavior, automation paths, visual geometry, and accessibility-relevant UI states.

## Non-Goals

- Do not change the recent-document JSON schema.
- Do not show currently-open documents in the recent list.
- Do not record session-restore opens as user recents.
- Do not redesign the Open popover layout or change the role of the file chooser action.
- Do not change `Ctrl+O` semantics.

## Decisions

### Derive Visible Filtering From Live Tabs

The Open popover should derive the "currently open" identity set from mounted editor pages in the current `AdwTabView` when rebuilding visible rows. This makes the visible recent list reflect the real UI state instead of trusting long-lived duplicate-detection state that can become stale after detach, close, failed load, Save As, rename/delete, or asynchronous canonical-path refresh.

`open_paths` remains useful for duplicate-tab detection in `open_document()`, but it should not be the sole source used to decide whether a persisted recent row is visible.

### Reconcile Duplicate-Detection State After Structural Changes

After structural tab transitions and path mutations, duplicate-detection bookkeeping should be scrubbed against the actual mounted editor pages. This keeps future duplicate checks healthy while still allowing the visible recent list to recover even if stale state appears briefly.

The reconciliation should run in the same places that already refresh recent rows or mutate open identities: page detach, close-tab paths, close-tab-for-path, Save As, rename/delete updates, failed or cancelled loads, session restore completion, and canonical path refresh callbacks.

### Batch Reconciliation During Tab Storms

Session restore, bulk tab close, and directory-scoped close-tab-for-path flows can add or remove many tabs in one burst. Those paths should defer derived tab-model projection refreshes while the batch is in progress, then reconcile `open_paths`, sidebar row state, Open popover rows, command-palette sources, content-stack state, and status-bar state once after the burst.

Ordinary single-tab transitions still update incrementally. Full cache scrubbing remains available for stale-repair paths such as failed load, Save As, rename/delete, and delayed canonical refresh.

### Keep Test Seams Narrow

If a test-only seam is needed to inject stale open identities or observe visible popover rows, it should live behind existing widget-test utilities or `cfg(test)`/test-support boundaries. Production code should not grow public APIs solely for tests.

### Regression Coverage Must Be Multilayered

This failure crosses layers, so a single widget test is not enough. Coverage should include:

- Pure recent-document filtering for stale display/canonical identities and mixed open/closed rows.
- Window/widget tests for startup-loaded persistence and same-session open/close workflows.
- Real action or automation paths that approximate `make run`/desktop activation behavior.
- Visual proof for empty, populated, dense, constrained, and all-open/all-closed states.
- Accessibility-relevant assertions for the Open button, search entry, chooser row, recent list, row controls, and empty state.

## Risks And Tradeoffs

- Deriving identities from live tabs is `O(tab_count)` during popover rebuild. This is cheap compared with UI row rebuild work and keeps correctness local to the visible workflow.
- Full reconciliation is also `O(tab_count)`, so structural tab storms must batch it to avoid repeated main-thread scans while restoring or closing many tabs.
- Reconciling `open_paths` too aggressively could weaken duplicate-tab prevention if the mounted-tab scan misses an editor during a transient state. Tests should cover open, focused-duplicate, closing, and path-refresh cases.
- Canonical-path refresh can complete after tab close. The filtering helper and reconciliation logic must ignore pages that are no longer mounted.
- Visual and automation checks are slower than unit tests, so the tasks separate focused proof from broader smoke gates.

## Migration

No data migration is required. Existing `recent-documents.json` files remain valid. Stale in-memory identity state is corrected by the new reconciliation behavior or cleared by process restart.
