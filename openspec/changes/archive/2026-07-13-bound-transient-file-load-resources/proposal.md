## Why

The live editor budget controls installed buffers, but concurrent file loads can each retain raw bytes, decoded text, worker results, and a growing GTK buffer before that policy observes them. Several near-limit session restores can therefore create multi-gigabyte transient pressure, while one very large `TextBuffer::set_text()` can monopolize the main loop.

## What Changes

- Split loading into a bounded metadata/plan phase and an admitted read/decode/install phase so large payloads do not enter the global worker FIFO without capacity.
- Add a plain-Rust byte-weighted admission policy: ordinary loads share a transient budget, an individually oversized-but-supported load runs exclusively, and active/modified work remains protected rather than evicted.
- Enforce the supported read limit inside the read itself so file growth after metadata cannot allocate beyond policy.
- Retain admission ownership until the decoded payload has been consumed by GTK, and install large text in bounded main-loop slices with cancellation and load-generation checks.
- Coalesce or remove queued loads when tabs close, reload, or become stale, and add deterministic concurrency, cancellation, Unicode, file-growth, and session-restore coverage plus scale benchmarks.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `live-editor-memory-budget`: Extend memory governance to transient read/decode/install payloads without weakening protected-editor safety.
- `main-thread-responsiveness`: Bound large-buffer installation work and keep stale/cancelled loads from consuming later main-loop slices.
- `performance-regression-coverage`: Add scale evidence for concurrent large loads, transient peak ownership, and chunked installation.

## Impact

- Affects `services/editor_io.rs`, `services/file_limits.rs`, editor load state, window/process load coordination, GTK installation code, and associated tests/benchmarks.
- Keeps production reads behind `services::filesystem` and GTK objects on the main thread.
- Adds an app-specific admission policy and runtime coordinator rather than changing the generic `gtk-lush-tasks` worker API.
- Should follow `make-buffer-snapshots-edit-safe` so both chunked GTK workflows share the settled mark, cancellation, and terminal-cleanup conventions without sharing an artificial generic API.
