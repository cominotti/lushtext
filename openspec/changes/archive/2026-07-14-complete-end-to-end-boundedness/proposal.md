## Why

The recent safety and performance portfolio bounded many individual stages, but the live implementation still has a few end-to-end gaps: canonical palette duplicates can survive bounded selection, source inventories can grow or overlap before search begins, document-sized GTK replacements can monopolize one main-loop turn, bounded draft cleanup can repeatedly scan the same directory prefix, and large tree refreshes can reconcile all rows in one callback. Closing these localized exceptions now lets LushText treat the completed portfolio as a consistent workflow contract rather than a collection of strong inner stages with weaker callers.

## What Changes

- Carry canonical file identity separately from display and activation paths, exclude every open canonical identity before workspace top-k retention, and preserve deterministic workspace/folder precedence without underfilling result groups.
- Bound and cooperatively cancel command-palette file-index and note-source construction, including huge flat directories, aggregate note count/byte retention, and superseded rebuild/refresh requests.
- Introduce one editor-owned bounded buffer-replacement session for large clear/replace workflows and adopt it for memory eviction, draft recovery, local-history restore/undo, and save-time formatting rewrites.
- Classify draft-recovery history policy from incoming content, avoid unnecessary full-body cloning, and keep partially installed recovery/history/save content non-editable and non-saveable until exact current-generation finalization.
- Make bounded draft orphan cleanup eventually cover the complete directory by carrying a deterministic directory continuation through inspection, execution, deferred retries, and restart-safe evidence.
- Apply large workspace-tree reconciliation through generation-guarded GTK batches, while keeping expansion, selection, cache, and readiness finalization atomic at the accepted terminal outcome.
- Correct recovery compatibility-loader documentation so its error contract matches recovery-to-default behavior.
- Add deterministic tests, benchmarks, and smoke evidence for identity aliases, excluded top results, huge source inventories, superseded work, large buffer replacement, cleanup continuation, and broad directory refresh.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `command-palette-source-groups`: Require canonical identity before bounded selection and bound/cancel the source inventories that feed palette search.
- `main-thread-responsiveness`: Require document-sized buffer clearing/replacement and large refreshed-tree reconciliation to yield in bounded current-generation GTK slices.
- `draft-session-recovery`: Require eventual bounded directory coverage and memory-safe, freshness-gated installation of restored drafts.
- `live-editor-memory-budget`: Require eviction accounting to complete only after bounded buffer clearing reaches a current terminal outcome.
- `local-history`: Require bounded, non-saveable installation for large restore/undo bodies while preserving reversible history semantics.
- `workspace-tree-refresh`: Require large accepted reconciliation plans to apply incrementally without exposing a partial refresh as complete.
- `performance-regression-coverage`: Add scale and responsiveness evidence for the newly completed end-to-end bounds.

## Impact

- Affected model/service areas: palette file and note inventory types, canonical identity, cancellable traversal, draft cleanup continuation, and related typed outcomes.
- Affected GTK adapters: command-palette source refresh, editor buffer mutation, memory eviction, draft recovery, local-history restore/undo, save finalization, and workspace-section reconciliation.
- Affected tests and benchmarks: palette, recovery, editor-page/window/widget, workspace refresh, property tests, Criterion, performance smoke, and visual/readiness proof where terminal state changes.
- No user-data format migration, public CLI break, new crate, generic task scheduler, or new external dependency is intended.
- The existing `finish-search-pipeline-hardening` change remains a prerequisite context; this change owns the canonical-identity correction discovered before that change is archived.
