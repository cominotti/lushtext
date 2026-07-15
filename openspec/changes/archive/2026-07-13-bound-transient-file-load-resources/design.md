## Context

The current load path performs metadata, full-file read, decode, health analysis, and `TextBuffer::set_text()` as one worker/result workflow. The generic task executor caps active/completed workers at eight, but it is count-based: eight near-limit payloads may coexist, and queued closures are not byte-admitted. The live editor budget starts after buffers are installed and intentionally protects loading/active pages, so it cannot control raw bytes, decoded strings, or installation overlap.

The supported-file threshold is much larger than the steady-state editor budget. A robust design must allow one supported large file without allowing several such payloads to overlap, must enforce limits despite metadata/read races, and must not consume generic worker slots while waiting for memory capacity.

## Goals / Non-Goals

**Goals:**

- Bound aggregate raw/decoded/result/install ownership with a conservative byte-weighted policy.
- Admit before document-sized worker work and retain ownership until GTK consumes the payload.
- Keep queued work compact, cancellable, and freshness-aware.
- Enforce size limits during ingestion and yield while installing large text.
- Preserve exact decoded contents, encoding/health behavior, and protected-editor safety.

**Non-Goals:**

- Exact allocator/RSS accounting or a hard process OOM guarantee.
- Memory-mapped editing, partial-document editing, or virtualized GtkTextBuffer storage.
- Changing the generic GTK Lush worker API without second-consumer evidence.
- Automatically discarding modified or active editors to make room.

## Decisions

### 1. Split load planning from payload execution

The first worker performs only metadata/canonical identity planning and returns a compact plain-Rust `FileLoadPlan`. GTK validates the load generation and submits the plan to a process-wide app-specific admission coordinator. Only an admitted request starts bounded read/decode/health work.

This avoids workers waiting on permits and starving unrelated filesystem work. Planning remains off GTK because metadata may block.

**Alternative considered:** acquire a semaphore inside `load_text_file`. Rejected because blocked workers would occupy the generic eight-slot cap without doing work.

### 2. Use a pure byte-weighted admission policy plus a GTK runtime coordinator

Place saturating weight calculation, budget decisions, exclusive-oversize behavior, and queue selection in a GTK-free model/service policy with deterministic tests. Keep weak editors, load generations, dispatch, and completion application in the driving adapter.

The conservative weight accounts for at least source bytes and decoded UTF-8 ownership with documented overhead. Requests at or below the shared budget may coexist while their summed weight fits. A supported request above it runs exclusively. Queued entries contain only path, plan facts, priority/sequence, generation, and weak ownership.

**Alternative considered:** lower the supported-file threshold until eight loads fit. Rejected because it changes user capability without addressing payload ownership or `set_text()` stalls.

### 3. Bound the read itself and revalidate identity

Add a filesystem-boundary streaming/bounded read that accepts the planned ceiling, reads at most the supported limit plus one sentinel unit, and returns typed oversize/cancelled/identity-changed outcomes. Recheck stable facts required by the load plan before accepting decoded content. Cancellation checkpoints occur during read and before expensive decode/health passes.

**Alternative considered:** trust initial metadata. Rejected because files can grow or be replaced after the probe.

### 4. Retain the permit through chunked GTK installation

The admitted result carries an RAII ownership token into GTK. Small content uses current direct installation. Large content is installed while the view is non-editable through bounded idle slices using a stable end mark, with minimap/history/draft/projection amplification suspended. Generation and cancellation are checked before each slice. Final language, cursor, health, monitor, history seed, modified-state, and memory-policy updates occur once after complete installation.

The token releases only after finalization or cancellation drops all retained text. This closes the gap where `spawn_blocking_then` releases a worker slot while a large result remains in a UI closure.

### 5. Use fair, freshness-aware queue progression

Preserve request sequence for ordinary fairness while allowing an active/current tab priority boost only through a deterministic policy. Before admission, discard dead weak references and stale generations. Releasing a permit schedules one coalesced GTK drain; it never recursively starts unbounded work in the completion callback.

## Risks / Trade-offs

- **[Risk] Conservative weights still underestimate GTK internal storage.** → Document the estimate, use saturation and safety margin, benchmark RSS diagnostically, and allow only one above-budget supported load.
- **[Risk] Chunked insertion emits expensive signals per slice.** → Suppress owned projections and irreversible/undo bookkeeping until finalization; add runtime warning and latency proof.
- **[Risk] Partial buffer contents become visible after cancellation.** → Keep the page non-editable/loading and clear or replace partial content only for the same load generation before exposing an error/retry state.
- **[Risk] Two-phase planning adds latency to small files.** → Measure it; permit a compact fast path only if it preserves pre-admission and in-read bounds.
- **[Trade-off] Large session restores complete more slowly.** → Intentional: predictable bounded progress is preferable to concurrent multi-gigabyte peaks or main-loop stalls.

## Migration Plan

1. Add pure weight/admission policy and bounded-read service tests.
2. Introduce plan/admission state while keeping direct installation for all accepted loads.
3. Add chunked installation and projection suppression with focused widget tests.
4. Route session restore/reload/cancel paths through the coordinator and remove the one-phase payload dispatch.
5. Add benchmarks and runtime proofs, then run performance, data-safety, full repository, and strict OpenSpec gates.

No persisted format changes. Rollback returns to the one-phase loader; test fixtures remain compatible.

## Open Questions

- Calibrate the concrete transient budget, weight multiplier, and installation threshold from existing large-file benchmarks before implementation locks constants. Their policy relationships—shared bound, one exclusive oversize, saturating arithmetic—are not open.
