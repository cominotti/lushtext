## Why

The recent boundedness, durability, and adapter-decomposition work materially improved LushText, but the follow-up code-quality review found five remaining end-to-end gaps where an existing bound or safety primitive stops short of the actual retained object or terminal state. Closing them now prevents large no-match searches and Notes inventories from exceeding their advertised memory envelopes, keeps document-sized load and draft bodies from being finally destroyed on GTK, and makes workspace persistence failures visible and close-safe.

## What Changes

- Bound workspace content-search identity retention by eliminating per-file visited ownership for single-root searches and normalizing overlapping traversal roots before scanning; retain an explicitly bounded fallback only where unresolved aliases still require it.
- Carry pre-admitted plain-disposal ownership with decoded file bodies from worker completion through direct or sliced GTK installation, accepted baseline transfer, stale rejection, cancellation, and editor teardown.
- Carry guarded ownership with eager and lazy draft bodies from startup preload or worker read through bounded buffer replacement and accepted baseline transfer, without treating document-sized recovery text as an unreserved small sentinel.
- Replace the workspace sidebar's boolean persistence bookkeeping with explicit dirty, in-flight, failed/retry, and terminal state so failures stay visible and retryable, readiness remains truthful, and close flushes the newest workspace snapshot before destruction.
- Extend Notes source construction limits to include traversal paths, current sidecar input, recovery diagnostics, canonicalization, and category-building scratch rather than accounting only for the final retained rows.
- Add direct service, policy, widget, release-semantic, and benchmark evidence for every new byte, cardinality, disposal-thread, retry, readiness, and close-time guarantee.
- Preserve current persisted formats, actions, D-Bus automation contracts, user-visible search ordering, workspace semantics, and Notes grouping. No external dependency or broad abstraction layer is introduced.

## Capabilities

### New Capabilities

<!-- No new standalone capability is introduced; this change closes ownership and terminal-state gaps in existing contracts. -->

### Modified Capabilities

- `main-thread-responsiveness`: Extend workspace-search traversal bounds and require document-sized file-load and draft payloads to retain guaranteed off-GTK final-disposal ownership across every terminal path.
- `live-editor-memory-budget`: Extend transient file-load admission through decoded-body installation, accepted baseline ownership, cancellation, and teardown instead of ending at byte accounting.
- `draft-session-recovery`: Require eager and lazy recovered draft bodies to preserve guarded ownership through extraction, GTK replacement, cancellation, and local-history baseline transfer.
- `workspace-state-persistence`: Make failed saves durable in workflow state, visible, retryable, readiness-aware, and part of the close-time persistence transaction.
- `workspace-notes`: Charge source-construction scratch and sidecar inputs against explicit byte limits in addition to final retained Notes rows.
- `performance-regression-coverage`: Add deterministic high-water and failure-path evidence for search identity retention, file/draft disposal, workspace persistence retries and close, and Notes construction scratch.

## Impact

- Affected services and models: `services/content_search`, Notes/palette source construction, file-load and disposal admission policy, and a small plain workspace-persistence state machine.
- Affected GTK adapters: editor load installation and local-history baseline ownership, draft restore/preload handoff, sidebar workspace persistence, Notes browser source admission, and automation readiness projection.
- Verification expands focused unit/property tests, headless GTK/widget tests, release-semantic disposal tests, persistence fault seams, and same-environment performance-smoke or Criterion high-water evidence.
- Existing `model -> services -> ui` ownership remains intact. The implementation reuses the filesystem boundary, `gtk-lush-tasks`, plain-disposal lanes, buffer-replacement sessions, typed generations, and current test infrastructure; it does not add a generic manager, scheduler, trait hierarchy, crate, or dependency.
