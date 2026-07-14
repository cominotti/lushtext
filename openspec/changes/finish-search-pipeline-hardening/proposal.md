## Why

Shared fuzzy scoring removed duplicated matcher setup, but command-palette queries still collect and sort every match before keeping a small result set, and superseded searches can accumulate in the global worker FIFO. Replace Preview also emits a matched document substring in a default-visible warning even though the typed outcome already records the skipped row.

## What Changes

- Select each command-palette source's best bounded result set during scoring, using deterministic ranking and O(result-limit) retained candidates instead of collecting all matches.
- Make command-palette search one-active/one-latest with cooperative cancellation so rapid input cannot queue obsolete full-index scans.
- Preserve source grouping, deduplication, ranking semantics, accessibility state, and current-result generation checks.
- Remove content-bearing Replace Preview diagnostics from the model; expose only typed bounded reason counts and non-private metadata to the owning UI/service layer.
- Add equivalence, Unicode, tie, cancellation, 100k-index, rapid-input, and diagnostic-privacy coverage plus focused benchmarks.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `command-palette-source-groups`: Preserve deterministic per-source ranking and grouping while bounding query ownership and superseding obsolete searches.
- `main-thread-responsiveness`: Keep rapid palette input to one active search plus one latest request and make cancellation observable to readiness state.
- `search-replace-safety`: Prohibit document/search-result contents from diagnostics and represent invalid Replace Preview rows through typed bounded outcomes.
- `performance-regression-coverage`: Add top-k, cancellation, and rapid-query scale coverage at the indexed-file limit.

## Impact

- Affects GTK-free palette scoring, command-palette runtime state, Replace Preview outcome handling, tests, and Criterion benchmarks.
- Adds no new dependency and keeps search decisions in services/model with GTK projection in UI.
- Does not change visible source-group order, result limits, command actions, or Replace All confirmation semantics.
- Is independent of the editor/draft/watcher changes and may be implemented after the buffer-snapshot foundation whenever a separate work stream is available.
