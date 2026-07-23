## Why

The recent quality programme materially closed the codebase's major ownership, persistence, and boundedness risks, but review found four narrow residual gaps: targeted workspace refresh still snapshots the full flattened tree, superseded closed-file bookmark previews can accumulate background work, very-large-file decoding and health analysis have coarse cancellation, and a process-global Replace/Undo test seam is nondeterministic under parallel tests. Closing these now gives the completed work a clean, evidence-backed finish without reopening settled architecture.

## What Changes

- Make workspace refresh preserve expansion state incrementally, so targeted in-place reconciliation does not scan every materialized GTK row; retain a full snapshot only for genuine model replacement or broad reload paths.
- Give closed-file bookmark excerpt previews one active worker and one replaceable latest compact request, with cooperative cancellation and current-generation-only publication.
- Add cooperative cancellation boundaries to large-file decode and analysis work, avoid starting optional exhaustive analysis after cancellation, and retain exact permit release and decoding correctness.
- Replace the process-global one-shot Replace/Undo fault hook with target-scoped test injection so unrelated parallel operations cannot consume one another's seams.
- Extend deterministic regression evidence for affected-row refresh work, bookmark-preview ownership, large-file cancellation and capacity release, and parallel fault-seam isolation. Keep near-limit RSS and throughput measurements in opt-in performance lanes rather than introducing host-sensitive default-CI timing gates.
- Preserve the current `ui -> services -> model` layering, filesystem boundary, GTK Lush task/settle infrastructure, persisted formats, user-facing actions, automation contract, and dependencies.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `workspace-tree-refresh`: require refresh-time expansion bookkeeping and targeted reconciliation to remain proportional to affected rows rather than rewalking the flattened tree.
- `workspace-notes`: require closed-file bookmark excerpt loading to use cancellable one-active/one-latest ownership.
- `main-thread-responsiveness`: require large-file decode and analysis stages to observe cancellation cooperatively and stop optional work while preserving exact terminal ownership.
- `performance-regression-coverage`: require deterministic direct evidence for the four residual boundaries, including parallel isolation of target-scoped failure injection.

## Impact

The implementation is expected to touch workspace-section refresh/index bookkeeping, Notes bookmark-preview coordination, bookmark excerpt and editor-load services, and test-only content-search fault injection, plus focused tests/benchmarks. No new crate, dependency, persisted-data migration, public Rust API, application action, D-Bus member, or user-visible workflow is expected.
