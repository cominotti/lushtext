## Why

The recent quality-hardening program materially improved LushText, but the final live-code review found one rare draft-loss path and six remaining responsiveness or scale gaps where individually bounded components do not yet compose into a safe process-wide workflow. Closing them together finishes the current quality program without reopening broad architecture work or introducing generic manager layers.

## What Changes

- Preserve every draft body when manifest repair cannot prove that its inventory is complete, including across restart boundaries, and prevent incomplete repair state from authorizing destructive orphan cleanup.
- Restore large sessions through bounded GTK turns with one terminal projection refresh and bounded in-flight load planning instead of rebuilding tab-derived state after every restored page.
- Build Replace All output without allocating a source-line range for every line, so dense short-line files remain within the documented replacement and undo-memory policy.
- Give each materialized workspace directory one active scan and at most one replaceable latest request, with weak queued ownership and generation-safe empty-folder results.
- Make document-sized plain-data disposal admission non-blocking and bound pending ownership at each producer before migrating remaining pure-drop workflows away from the shared GTK-completion task lane.
- Keep minimap wrapped-layout and long-line analysis bounded and freshness-aware for large many-short-line documents, degrading explicitly when a safe responsive analysis cannot be completed.
- Avoid scanning note bodies that cannot change an already-established palette result while preserving ranking, Unicode, cancellation, and grouping semantics.
- Add deterministic regression, widget, and performance-smoke evidence for every safety, ownership, memory, and per-turn bound introduced by this closeout.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `draft-session-recovery`: Prevent incomplete manifest-repair provenance from being forgotten by later manifest writers or restarts, and prohibit cleanup from treating a partial inventory as authoritative.
- `main-thread-responsiveness`: Bound session restoration and process-wide plain-data retirement without blocking GTK.
- `search-replace-safety`: Require dense-line Replace All construction to obey the existing byte and replacement-count bounds without whole-file line indexing.
- `workspace-tree-refresh`: Coalesce directory scans per materialized store and reject stale scan and empty-folder results.
- `editor-minimap`: Bound wrapped-layout and marker analysis for large many-short-line documents while preserving explicit availability behavior.
- `command-palette-source-groups`: Short-circuit note-body scoring only when the body cannot affect eligibility or ordering.
- `performance-regression-coverage`: Add direct safety, cardinality, ownership, byte, generation, and GTK-turn evidence for the complete closeout.

## Impact

- Affected Rust areas include draft repair and cleanup, session restoration and tab projections, Replace All text construction, workspace tree scan coordination, the app-owned plain-data disposal lane and its producers, minimap analysis, and note palette scoring.
- Tests and benchmarks will gain multi-restart draft fixtures, high-tab restore instrumentation, dense short-line replacement cases, slow-filesystem scan churn, aggregate disposal pressure, large wrapped minimap fixtures, and metadata-hit note corpora.
- No public API, persisted user-facing format, GTK Lush public API, dependency, or packaging change is intended. Any durable repair provenance will remain app-owned recovery metadata and must be forward-compatible with existing valid manifests.
