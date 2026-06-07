## Why

Several editor workflows already push disk I/O and large saves off the GTK
thread, but a few remaining UI paths still perform synchronous filesystem work,
whole-buffer snapshots, or pure computation in response to user actions. Moving
those bounded hotspots into existing async/chunked patterns will make LushText
feel steadier on slower filesystems, large buffers, and large search result sets
without changing the editor architecture.

## What Changes

- Add a cross-cutting main-thread responsiveness contract for GTK-facing
  workflows that separates GTK-only state access from filesystem I/O and pure
  CPU work.
- Move Replace All undo-backup save/delete work out of the GTK thread while
  keeping the immediate in-memory undo state and UI affordances responsive.
- Rework draft autosave snapshot collection so large dirty buffers are captured
  in bounded GTK main-loop slices before background persistence.
- Refresh Save As canonical path bookkeeping asynchronously after successful
  saves instead of probing canonical paths synchronously on the app thread.
- Generate expensive Replace preview data off the GTK thread and ignore stale
  results when the search request changes before completion.
- Add forward-looking guardrails for Markdown preview preprocessing, minimap
  marker scans, and lossy encoding analysis so large or expensive work either
  yields in chunks, runs in a worker, or degrades explicitly instead of blocking
  input.
- Expand automated coverage with unit, widget, and performance-smoke tests that
  prove the new asynchronous paths preserve ordering, stale-result protection,
  safety windows, and user-visible responsiveness contracts.

## Capabilities

### New Capabilities

- `main-thread-responsiveness`: Cross-cutting responsiveness requirements for
  GTK-facing workflows that must keep disk I/O, large snapshots, and pure
  analysis from blocking normal interaction.

### Modified Capabilities

- `performance-regression-coverage`: Add explicit coverage expectations for
  main-thread responsiveness regressions and asynchronous workflow ordering.

## Impact

- Affected production areas include search panel undo-backup persistence and
  Replace preview state, draft autosave scheduling, Save As path bookkeeping,
  optional Markdown preview preprocessing, minimap marker collection, and
  encoding-conversion warning analysis.
- Affected test areas include search panel/service tests, draft/session tests,
  Save As or path-bookkeeping tests, GTK widget responsiveness harnesses, and
  lightweight performance-smoke coverage.
- No new runtime dependencies are expected; the change should primarily reuse
  `spawn_blocking_then`, generation counters, existing filesystem services, and
  the save pipeline's chunked buffer snapshot pattern.
