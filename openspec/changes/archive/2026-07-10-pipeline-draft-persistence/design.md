## Context

Draft autosave uses generation guards, chunked GTK snapshots, background durable writes, and a serialized manifest update. The remaining amplification occurs between those stages: the window gathers complete `String` bodies for all dirty tabs before any body is written, and close-time flush does the same. Startup eagerly preloads at most 64 MiB, so a successfully written larger draft or a valid draft skipped by the aggregate preload cap may not be automatically applied even though the user reasonably expects persisted recovery to be restorable.

## Goals / Non-Goals

**Goals:**

- Bound a draft pass to one full draft body plus small candidate/completion metadata at a time.
- Preserve generation-safe dirty clearing, one logical autosave pass, pending reruns, and close-time safety.
- Align automatic draft writes with a 64 MiB per-draft restore capability while retaining a 64 MiB aggregate eager preload cap.
- Lazily restore size-eligible drafts skipped only by the eager aggregate cap.
- Make snapshot, body-write, and manifest failures visible and retryable.

**Non-Goals:**

- Changing draft IDs, the v1 manifest envelope, autosave timing, or session ownership.
- Persisting drafts larger than the automatic recovery limit and implying that they can be restored automatically.
- Running concurrent manifest writers or parallel full-buffer snapshots.
- Replacing the durable filesystem write boundary.

## Decisions

### Model one pass as a bounded sequential pipeline

The window will keep a queue of lightweight `DraftCandidate` records. It snapshots one candidate, checks that the captured UTF-8 body stays within `MAX_AUTOMATIC_DRAFT_BYTES` (64 MiB), immediately sends that body to a worker for durable draft-file write, and releases the `String` when the worker returns. Successful writes accumulate only compact `DraftEntry` and `DraftCompletion` metadata. The next candidate is not snapshotted until the prior body has been released.

After all candidates are attempted, one serialized `update_manifest` commits successful entries. Only then may matching editor generations clear `draft_dirty`. A failed manifest update makes every body from that pass retryable; the already written bodies remain conservative orphan/recovery evidence.

Alternatives considered:

- Gathering all bodies and writing them in one worker was rejected because it is the present peak-memory problem.
- Updating the manifest after every body was rejected because it rewrites shared metadata N times and exposes more intermediate generations.
- Parallel snapshots/writes were rejected because they raise peak RAM and complicate close ordering.

### Use a budget-aware snapshot outcome

The shared chunked snapshot helper will gain a budgeted variant whose outcome is `Captured(String)`, `ExceededLimit { observed_at_least }`, or `Cancelled`. It stops accumulating once UTF-8 bytes exceed 64 MiB plus the bounded current chunk. Direct snapshots use the same postcondition. The editor remains draft-dirty on limit or cancellation, and the window surfaces a document-scoped warning that automatic crash recovery is not current.

The 64 MiB limit is shared with draft read policy as `MAX_AUTOMATIC_DRAFT_BYTES`; the aggregate eager preload cap remains separately named because it protects startup peak memory.

Alternatives considered:

- Relying on character count before snapshot was rejected because multibyte UTF-8 makes exact acceptance ambiguous.
- Writing arbitrarily large drafts but refusing to restore them was rejected because it creates false recovery confidence.

### Lazy-load valid drafts skipped by aggregate eager preload

Startup still preloads no more than 64 MiB across draft bodies. For a valid manifest entry skipped only because the aggregate preload budget was exhausted, tab restoration will recreate the editor first and then schedule a single background draft read. The completion applies only when draft ID, file freshness decision, editor lifetime, and restore generation still match. Only one lazy draft body is admitted to the GTK application path at a time, so startup does not recreate the original aggregate spike.

Oversized, stale, malformed, missing, or unreadable drafts keep their existing distinct diagnostics and are not relabeled as aggregate-cap skips.

### Share one driver between autosave and close-time flush

Autosave and close use the same pipeline state machine with different completion policies. Autosave returns control after each asynchronous stage and coalesces a pending rerun. Close keeps the close request pending until all eligible candidates and the manifest commit finish; any failure leaves the window open with explicit recovery feedback. Explicitly discarded draft IDs remain excluded from the close candidate queue.

## Risks / Trade-offs

- [Sequential per-draft worker round trips make a many-tab pass take longer] → Keep the pipeline single-body for memory safety, retain the five-second/pending coalescing semantics, and measure completion latency in scale tests.
- [A crash after body write but before manifest commit leaves an orphan body] → Preserve the body, rely on hardened conservative orphan/repair handling, and never report that editor generation as accepted.
- [Lazy restore could apply to a reused tab] → Carry draft identity, file-freshness outcome, editor weak reference, and restore generation through completion.
- [A document above 64 MiB loses automatic draft protection] → Stop before write, keep dirty state, show persistent retryable feedback, and preserve normal explicit Save/Save As paths.

## Migration Plan

1. Introduce shared automatic-draft constants and budgeted snapshot outcomes with unit tests.
2. Add the pipeline state and migrate periodic/first-dirty autosave while retaining old close code behind the same service boundary.
3. Migrate close-time flush and verify explicit discard and failure blocking.
4. Add lazy restore for aggregate-preload skips and crash/restart coverage.
5. Remove batch `Vec<DirtyDraftSnapshot>` paths after all parity tests pass.
6. Rollback restores the former batch orchestrator; no persisted format migration is required.

## Open Questions

None. The automatic per-draft limit deliberately matches the current 64 MiB read capability; future changes must update write, read, diagnostics, and smoke coverage together.
