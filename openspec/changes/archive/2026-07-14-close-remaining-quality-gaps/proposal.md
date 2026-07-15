## Why

The recent quality-hardening portfolio materially improved LushText, but the final live-tree review found one default-test compilation blocker and a small set of end-to-end responsiveness, scale, and cleanup-liveness gaps that remain outside the otherwise bounded workflows. Closing them now keeps the completed portfolio honest and avoids carrying isolated large-input stalls or deferred-cleanup blind spots into release work.

## What Changes

- Restore default-feature unit-test compilation so deterministic draft-cleanup fault coverage is available to ordinary test builds and the declared `make test-unit` completion gate is real.
- Bound `Browse Notes...` source admission and retained inventory, move query matching off GTK, and use one-active/one-latest request ownership while preserving scope, grouping, preview, recovery diagnostics, and the existing rendered-result cap.
- Make local-history preview selection one-active/one-latest and install accepted large snapshot text in bounded UTF-8-safe GTK slices without weakening Copy, Restore, or snapshot-retention safety.
- Replace the workspace tree's per-row index-shifting recache with an O(n) bulk cache rebuild and extend watcher bounding to raw backend ingress before a large debounced event vector can accumulate.
- Retire replaced and rejected large command-palette file indexes off GTK in every update path, including incremental updates and stale worker completions.
- Schedule rate-limited draft orphan-cleanup follow-up work whenever the typed outcome reports `has_more_work`, including retryable failures with no pagination cursor, while retaining ambiguous or failed recovery artifacts.
- Add focused unit, property, widget, and performance evidence for the corrected test matrix, retained-state bounds, stale-request behavior, main-loop progress, bulk recache complexity, watcher ingress pressure, and cleanup retries.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `main-thread-responsiveness`: Make Notes browsing, local-history preview installation, workspace cache rebuilding, and command-palette index retirement explicitly bounded away from long GTK turns.
- `workspace-notes`: Bound Notes browser source construction, retained inventory, and background query ownership while preserving existing browsing semantics.
- `local-history`: Require stale-safe, sliced installation for accepted large snapshot previews.
- `workspace-tree-refresh`: Bound watcher state from raw ingress through GTK consumption and rebuild accepted child-row caches in linear time.
- `draft-session-recovery`: Turn every `has_more_work` cleanup outcome into a safe deferred retry, including retryable failures without a continuation cursor.
- `performance-regression-coverage`: Cover the remaining retained-state, latency, cancellation, retry, and feature-matrix regressions with deterministic evidence.

## Impact

- Affected Rust areas: `services::palette::notes`, `ui::window::notes`, `ui::window::local_history`, `services::workspace_watch`, `ui::sidebar::workspace_section`, `ui::command_palette`, `services::draft_service`, and `ui::window::drafts`.
- Affected verification: default-feature and all-feature unit tests, targeted property/widget tests, Criterion or performance-smoke fixtures, strict OpenSpec validation, and repository policy gates selected by the changed UI surfaces.
- No user-data format, public automation interface, user-visible command, dependency, or GTK Lush public-API change is intended.
