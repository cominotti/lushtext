## Why

Draft recovery now has strong per-stage bounds, but ordinary asynchronous restore and autosave deletion still lack one shared ordering authority. A live editor can accept stale recovered text, or a completed Save/discard can be followed by an older autosave that recreates its draft metadata; local-history baseline failures likewise consume the only pre-edit recovery copy instead of remaining retryable.

## What Changes

- Give every asynchronous draft restore a complete freshness ticket covering draft identity, editor lifetime, expected path, dirty/edit generation, load generation, and the manifest entry being resolved.
- Serialize draft body writes, manifest upserts, and draft deletions through one ordered persistence coordinator whose intent generations make Save/discard tombstones authoritative over older autosave work.
- Preserve the existing one-complete-body autosave bound, durable filesystem boundary, failure retryability, and close-time flush semantics while adding operation ordering.
- Restore and retry a failed local-history baseline only when the same editor/path cycle still owns that clean text and no newer baseline has replaced it.
- Add deterministic race and fault coverage for edit-during-restore, reload-during-restore, Save/discard during autosave, failed baseline persistence, and stale completions.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `draft-session-recovery`: Make every asynchronous restore freshness-safe and order autosave upserts against Save/discard deletion.
- `local-history`: Preserve the pre-edit baseline across recoverable persistence failure without restoring it into a newer editor or path generation.

## Impact

- Affects `ui/window/drafts.rs`, draft-related window state, `services/draft_service.rs`, `ui/editor_page/local_history.rs`, and their service/widget tests.
- Introduces compact operation tickets and an ordered draft-persistence workflow; it does not add a generic repository/manager trait or change persisted formats.
- Keeps filesystem work off GTK and retains existing draft size, preload, orphan-cleanup, and one-body memory bounds.
- Should follow `make-buffer-snapshots-edit-safe`; the other three portfolio changes are behaviorally independent and may follow in any order.
