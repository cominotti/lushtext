## Why

The completed boundedness portfolio materially reduced unbounded work and stale-state risk, but final review found a few lifecycle gaps at the edges: directory-only palette traversal can still retain unbounded visited-directory state, and large Replace Preview or Markdown payloads can still be destroyed, filtered, or queued on the GTK thread during supersession. Closing these gaps now preserves the intended end-to-end memory and responsiveness guarantees under adversarial cancellation and churn.

## What Changes

- Add a global command-palette directory traversal and retention budget independent of the existing 100,000-file cap, with typed truncation, cancellation, and measurable high-water state.
- Route previous, stale, rejected, unchecked, and superseded Replace Preview payloads through bounded retirement, and perform large confirmation selection away from the GTK action path before the current apply handoff.
- Retire stale Markdown plans and unprojected plain-Rust batch tails away from GTK, cap detached render generations, and coalesce pressure into one latest pending render request.
- Make the workspace-search per-turn event budget count every received event variant, including progress, error, cap, and terminal events.
- Make document-sized sliced buffer replacement establish mutation state before signal-emitting GTK calls so synchronous reentrant supersession cannot bypass cleanup.
- Extend deterministic service, policy, widget, responsiveness, and benchmark evidence to cover directory-only traversal, near-limit preview and Markdown churn, mixed search-event bursts, retirement high-water marks, and synchronous replacement reentrancy.
- Keep the work inside the existing palette, search, Markdown, buffer-replacement, retirement, and performance-evidence boundaries; do not introduce a new scheduler, generic retirement framework, crate, dependency, or public API.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `command-palette-source-groups`: Bound total directory traversal and retained visited-directory identity independently from the admitted file count.
- `main-thread-responsiveness`: Bound disposal, backlog, per-turn event processing, and reentrant mutation across Replace Preview, Markdown, workspace search, and sliced buffer replacement.
- `search-replace-safety`: Preserve exact current-generation checked-row semantics while moving large preview selection and rejected-payload retirement off the GTK action path.
- `performance-regression-coverage`: Add direct high-water and churn evidence for the remaining traversal and retirement boundaries.

## Impact

The implementation will primarily affect palette indexing, search-panel preview/runtime/retirement coordination, Markdown preview planning and projection ownership, editor buffer-replacement sessions, their GTK/widget and pure tests, and performance benchmarks or smoke evidence. Existing user-visible semantics, persisted formats, automation interfaces, GTK Lush public APIs, dependencies, and application data remain compatible.
