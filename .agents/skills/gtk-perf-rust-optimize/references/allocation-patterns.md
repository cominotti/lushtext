# Ownership and Allocation Patterns

Use these patterns to understand intentional ownership costs. Verify implementation details against current code before treating a status statement as current.

## Contents

1. [Document decode](#document-decode)
2. [GTK-to-worker save snapshots](#gtk-to-worker-save-snapshots)
3. [Typed failures](#typed-failures)
4. [GTK model updates](#gtk-model-updates)

## Document decode

Document loading is encoding-aware. `services/editor_io.rs` reads bytes through the filesystem boundary and produces owned Unicode text plus encoding and line-ending metadata. `simdutf8` accelerates the valid-UTF-8 branch; safe conversion and legacy-encoding fallbacks preserve correctness.

Avoid duplicating this policy in UI code. A copied “UTF-8 only” fast path is a correctness regression even if it allocates less.

## GTK-to-worker save snapshots

GTK text objects are main-thread-bound and cannot be read from a worker. Saving therefore requires an owned snapshot before durable filesystem work begins. Current editor code chooses snapshot strategy from live buffer state: ordinary buffers may snapshot synchronously, while large or uncertain buffers can be copied in bounded main-loop slices. The editor remains protected for the save and applies completion on the GTK thread.

Review peak lifetime rather than declaring the copy avoidable or unavoidable in isolation:

- GTK buffer storage remains live;
- an owned save snapshot is produced;
- durable-write buffers and metadata may overlap;
- stale or duplicate saves must not bypass ordering and modified-state rules.

Do not suggest sending `GString`, `GtkTextBuffer`, or another GTK object to a worker.

## Typed failures

Use typed errors when callers must distinguish cancellation, refusal, pre-rename failure, durability uncertainty, or another workflow state. Preserve sources and relevant path/context. A private helper with one obvious failure shape does not need a new public enum merely for style consistency.

Never discriminate operational errors by display-string equality or substring matching when a typed variant is available.

## GTK model updates

Batch model reconciliation can reduce signal and allocation churn, but the correct operation depends on identity and selection contracts. `ListStore::splice` is useful when replacing a known contiguous range. Incremental reconciliation may be required to preserve row identity, expansion, selection, or watcher state.

Do not mechanically replace every append loop with `splice`; inspect factory and model semantics first.
