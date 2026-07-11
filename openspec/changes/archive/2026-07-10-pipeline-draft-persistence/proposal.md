## Why

Draft autosave and close-time recovery already snapshot large buffers in bounded GTK main-loop slices, but a sweep can still retain every completed tab snapshot until the whole batch is ready to write. Several large dirty tabs can therefore amplify peak RAM, and the current write/restore limits do not express one coherent automatic-recovery budget. Draft capture, durable write, and release should be a bounded pipeline while preserving the existing crash-recovery guarantees.

## What Changes

- Pipeline dirty drafts through bounded snapshot, background write, manifest commit, and memory release stages instead of accumulating a full-window batch of bodies.
- Preserve newest-generation-wins behavior, one in-flight manifest writer, pending reruns, close-time completion, and retryable failure state.
- Define an explicit per-draft automatic recovery limit aligned with startup restore capabilities and surface a visible, retryable state when a dirty document cannot be automatically protected.
- Clear draft-dirty state only after the matching snapshot and durable write have been accepted for the same editor generation.
- Add fault-injection, multi-tab memory, close-flow, and crash/restart coverage for partial and stale pipeline completions.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `draft-session-recovery`: Strengthens draft autosave and close-time recovery with a bounded pipeline, aligned automatic-recovery limits, and explicit generation-safe acceptance semantics.
- `main-thread-responsiveness`: Extends bounded draft snapshotting so completed snapshots are also released incrementally rather than retained as a whole-window batch.

## Impact

- Affects draft orchestration in `crates/lushtext-core/src/ui/window/drafts.rs`, editor snapshot helpers, draft service outcomes, and recovery diagnostics.
- Preserves the existing durable-write filesystem boundary and public v1 draft-manifest envelope.
- Should follow `harden-draft-orphan-cleanup` and precede the live-memory and adapter-decomposition changes, reducing overlapping edits in draft workflows.
