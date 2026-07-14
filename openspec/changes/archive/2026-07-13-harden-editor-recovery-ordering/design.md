## Context

The bounded draft pipeline snapshots and writes one complete body at a time, then commits compact manifest entries. The service-level manifest lock prevents concurrent read-modify-write corruption, but it does not define user-intent order across an autosave body worker and a later Save/discard deletion. Ordinary fallback restore paths also apply worker results with lifetime checks only, while the lazy aggregate-budget path already demonstrates the stronger identity/path/generation contract. Local-history baseline admission similarly bounds payload ownership, but `take()` transfers the only clean text to a worker with no failure return path.

The change spans GTK-owned editor facts, background filesystem work, and plain recovery metadata. GTK must remain the freshness authority; services must remain GTK-free; all production I/O must stay behind `services::filesystem`; and no fix may retain more than one complete draft body per pipeline.

## Implementation Baseline

The pre-change proof surface was anchored by `test_draft_pipeline_lazy_restore_rejects_stale_editor_and_advances_queue`, the first-dirty/partial-failure autosave tests, `test_draft_pipeline_retains_at_most_one_complete_body_across_many_tabs`, the async close-flush tests, and the local-history capture-policy/periodic freshness tests. Ordinary untitled and file-backed fallback reads had no equivalent delayed-completion coverage, draft deletion had no stage-specific ordering seam, and baseline failure returned only an error.

The mutation call graph before this change was:

```text
buffer edit -> autosave_tick -> snapshot one body -> write_draft
            -> batch update_manifest(upsert) -> clear matching draft_dirty

Save/discard/close resolution -> delete_draft_by_id
                              -> delete_draft_file + update_manifest(remove)

startup restore -> check_draft_by_id/check_draft_on_open/lazy queue
                -> service read/resolve -> callback applies or deletes

first modified transition -> take(last_clean_text)
                          -> capture_snapshot_for_path(Baseline)
                          -> error drops the moved text
```

The implementation keeps those entry points but routes their effects through `DraftMutationOrder`, the window-owned single-flight mutation gate, one shared `DraftRestoreTicket`, and a failure-returned baseline outcome. Deterministic test seams cover ordinary restore delay, body completion, manifest admission, post-upsert completion, deletion, and baseline persistence failure.

## Goals / Non-Goals

**Goals:**

- Make all asynchronous restore paths reject stale editor, path, load, and manifest-entry state before any buffer mutation or recovery deletion.
- Make the final durable draft state follow main-thread user-intent order across autosave, Save, discard, stale cleanup, and later edits.
- Keep body and manifest failures retryable without introducing overlapping autosave passes or synchronous I/O.
- Preserve a failed local-history baseline only for its original editor/path/editing cycle.
- Encode the temporal rules in named tickets and coordinator phases rather than comments alone.

**Non-Goals:**

- Changing draft/session/local-history persisted formats or recovery limits.
- Combining draft and session persistence into one workflow.
- Replacing the filesystem durable-write implementation or orphan-cleanup algorithm.
- Adding a generic repository, actor framework, or cross-application persistence trait.

## Decisions

### 1. Use one restore ticket shape for every asynchronous path

Create a private plain-data restore ticket containing draft ID, the exact manifest-entry fingerprint or snapshot, expected path, dirty/edit generation, load generation, and a weak editor reference kept only by the GTK adapter. Both untitled and file-backed fallback reads, plus the existing lazy queue, use the same final `is_current` check immediately before apply, feedback, or deletion.

The check remains in `ui/window/drafts.rs` because only the driving adapter can inspect live editor identity and GTK lifecycle. `draft_service` continues to resolve plain entries and return typed outcomes.

**Alternative considered:** check only dirty generation in each callback. Rejected because path reuse, reload, and a replaced manifest entry can be stale without a new buffer edit.

### 2. Assign draft mutation intent before document-sized work begins

Maintain window-owned per-draft monotonic mutation epochs and a global sequence for persistence commands. Autosave captures its ticket and persistence intent before its body worker is admitted. Save/discard advances the same draft's epoch synchronously on GTK and enqueues an ordered delete command. A completion whose epoch is no longer current cannot enqueue a later upsert.

Sequence assignment must happen at intent time, not worker-completion time; otherwise a slow old body write can appear newer than a subsequent Save.

**Alternative considered:** rely on the existing manifest mutex. Rejected because a mutex serializes acquisition order, not user intent, and body files are mutated outside the manifest critical section.

### 3. Execute body plus manifest effects through one single-flight coordinator

Use one app-specific draft persistence coordinator with compact queued commands and at most one active mutation. An autosave command retains the existing candidate pipeline and one-body admission, but the coordinator owns the ordered lifetime through body write and manifest acceptance. A delete command removes the body and manifest entry after every earlier command. Errors yield typed completion without skipping later authoritative commands.

Commands do not contain GTK objects. The UI side stores weak/scalar completion facts and applies visible state only after current-generation success. This preserves `ui -> services -> model` and avoids blocking the generic `gtk-lush-tasks` queue with a worker waiting on another worker.

**Alternative considered:** put all draft operations on a dedicated OS thread. Rejected because it duplicates the bounded task infrastructure and complicates GTK completion/lifecycle handling.

### 4. Make baseline text a failure-returned payload

The baseline worker returns an outcome that includes the original text on failure. The GTK completion restores it only when editor generation, path generation, and clean-baseline generation match and the baseline slot is still empty. Retry admission uses the existing weak/scalar waiter and never queues multiple text payloads.

**Alternative considered:** clone the clean text before dispatch. Rejected because it doubles document-sized ownership and undermines the admission work.

### 5. Preserve observable failure semantics

Stale results are silent unless an already documented user-visible state needs refreshing. Durable failures keep draft-dirty state or baseline retryability and publish bounded content-free feedback. No outcome includes document or draft body text in diagnostics.

## Risks / Trade-offs

- **[Risk] A coordinator can accidentally hold several complete bodies.** → Keep snapshot admission outside compact queues, assert the retained-body high-water mark, and release each body before advancing.
- **[Risk] Sequence and per-draft epoch can diverge.** → Centralize allocation and currentness checks in one state object with pure unit tests for interleavings and wrap-safe equality semantics.
- **[Risk] A failed delete could block later legitimate autosave forever.** → Complete the failed command with retry evidence, release the single-flight gate, and allow a strictly later edit epoch to enqueue new recovery.
- **[Risk] Restoring failed baseline text can cross a Save As or save cycle.** → Require editor, path, and clean-baseline generations plus an empty target slot before restoration.
- **[Trade-off] Fully ordered persistence may delay deletion behind an active autosave.** → This is intentional; deletion remains background work and becomes the final authoritative command without GTK blocking.

## Migration Plan

1. Add pure ticket/ordering state and deterministic interleaving tests without changing current dispatch.
2. Unify restore completion validation, then add delayed-read widget tests.
3. Route autosave and delete effects through the coordinator while retaining current one-body instrumentation.
4. Return failed baseline payload ownership and add injected filesystem-failure coverage.
5. Run the explicit data-safety audit, focused widget/integration tests, full repository gates, and strict OpenSpec validation.

Rollback is code-only because no persisted format changes. Existing draft bodies and manifests remain readable throughout.

## Open Questions

- None. Exact private type and module names may follow the current window workflow layout, but the ordering and ownership contracts are fixed.
