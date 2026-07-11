## 1. Shared Draft Limits and Snapshot Outcome

- [x] 1.1 Introduce separately named 64 MiB per-draft automatic-recovery and aggregate eager-preload constants, and update existing read/preload callers to use the correct policy.
- [x] 1.2 Add a budget-aware chunked buffer snapshot outcome for captured, over-limit, and cancelled states with bounded chunk overhead.
- [x] 1.3 Add direct/chunked snapshot tests for empty, exact-limit, one-byte-over, multibyte, cancellation, and no-partial-success cases.

## 2. Bounded Autosave Pipeline

- [x] 2.1 Replace the autosave `Vec<DirtyDraftSnapshot>` accumulator with a lightweight candidate queue and one-body pipeline state.
- [x] 2.2 Snapshot one current candidate, validate draft identity/generation, write it durably on a worker, release its body, and only then advance to the next candidate.
- [x] 2.3 Accumulate only compact successful `DraftEntry` and completion metadata and commit them through one serialized manifest update at the end of the pass.
- [x] 2.4 Clear draft-dirty state only for same-ID/same-generation editors after both body and manifest acceptance; leave snapshot, write, and manifest failures retryable.
- [x] 2.5 Preserve `autosave_inflight`, one pending rerun, first-dirty timing, periodic timing, editor-close behavior, and stale weak-reference handling.

## 3. Close-Time Safety and Oversized Feedback

- [x] 3.1 Migrate close-time draft flush to the same one-body pipeline while preserving discarded-ID exclusion and keeping close pending through the manifest commit.
- [x] 3.2 Block close with grouped visible feedback when any eligible draft body or manifest update cannot be confirmed, without clearing retryable state.
- [x] 3.3 Add document-scoped automatic-recovery-limit feedback for over-limit dirty editors while preserving Save, Save As, and continued editing.
- [x] 3.4 Ensure a later below-limit generation can clear the warning only after successful matching acceptance.

## 4. Bounded Startup Restore

- [x] 4.1 Preserve aggregate eager preload and classify individually eligible drafts skipped only by that aggregate cap as lazy-restore candidates.
- [x] 4.2 Add a serialized background lazy-read queue that applies one body only after draft ID, file freshness, editor lifetime, and restore generation still match.
- [x] 4.3 Keep oversized, stale, malformed, missing, and unreadable draft outcomes distinct and preserve their files/manifest entries according to current recovery policy.
- [x] 4.4 Add startup tests for multiple aggregate-cap drafts, stale lazy completions, user edits before completion, lazy read failure, and unaffected tab restoration.

## 5. Failure, Crash, and Memory Proof

- [x] 5.1 Add fault-injection tests for snapshot cancellation, body-write failure, final manifest-write failure, partial success, newer edits, and close-time failure recovery.
- [x] 5.2 Add instrumentation/scale tests proving at most one complete draft body is retained by a pass, including many large dirty tabs.
- [x] 5.3 Extend crash/restart smoke to distinguish accepted generations from in-flight/retryable generations and to exercise lazy aggregate-cap restore.
- [x] 5.4 Run formatting, data-safety review, focused service/widget/integration tests, `make check`, relevant crash smoke, `make lint-advisory`, and `make pre-commit`; fix every issue found.
- [x] 5.5 Run the learning workflow and update recovery guidance only for durable implementation discoveries.
