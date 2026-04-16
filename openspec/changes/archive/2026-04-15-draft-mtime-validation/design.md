## Context

LushText already persists unsaved file-backed and untitled drafts under the app data directory, and it already records the backing file mtime in `DraftEntry.original_mtime_secs` whenever autosave or close-time flush writes a draft. The restore path is split across two layers:

- `services/draft_service.rs` loads the manifest, session, and preloaded draft content on a background thread during startup.
- `ui/window/drafts.rs` applies untitled drafts through `check_draft_by_id` and file-backed drafts through `check_draft_on_open` after `load_file_async()` finishes successfully.

That split keeps blocking I/O off the GTK thread, but it currently leaves one safety gap: file-backed drafts are restored as soon as content is available, without validating whether the backing file changed after the draft was written. The result is a confusing recovery state where stale draft content can replace newer on-disk content and the user can see recovery messaging that no longer matches the real file state.

## Goals / Non-Goals

**Goals:**
- Validate file-backed drafts against the current backing file mtime before recovered content reaches the editor buffer.
- Reuse the same validation logic for startup preload and later file-open restore so behavior stays consistent.
- Keep untitled draft restore unchanged.
- Discard confirmed stale drafts so they are not offered again on later opens.
- Surface clear per-file feedback when a draft is skipped because the backing file changed externally.

**Non-Goals:**
- Merging stale draft content with the newer on-disk file.
- Building a compare/review UI for stale drafts in this change.
- Changing autosave cadence, manifest format, or untitled-draft identity rules.
- Solving every ambiguity caused by filesystem timestamp granularity or transient metadata failures.

## Decisions

### 1. Centralize file-backed restore validation in the draft service

Add a service-level restore-resolution helper for file-backed drafts that takes a `DraftEntry`, inspects the current backing-file mtime, and returns a typed result such as "apply restored content", "skip because stale", or "skip because validation could not complete". That helper will own the blocking filesystem and draft-file work so both startup preload and later on-demand restore can share one decision path.

This keeps GTK-facing code focused on consuming a restore outcome rather than duplicating `mtime` and cleanup logic in multiple callbacks. It also matches the repo's existing split where services own I/O and `ui/window` modules own widget reactions.

Alternatives considered:
- Validate only inside `check_draft_on_open`: simpler short-term, but it duplicates logic and leaves startup preload with a different interpretation of the same manifest entry.
- Validate on the GTK thread right before `apply_draft`: rejected because `stat` and file reads can block on slow or remote filesystems.

### 2. Preload restore outcomes, not just raw draft text

`load_restore_state()` should stop treating preloaded data as "draft id -> text only" for file-backed tabs. Instead, it should cache the restore outcome needed by `check_draft_on_open`, including the stale-draft case where no content should be applied but the UI still needs to show one warning and consume the result exactly once.

Untitled drafts can keep the simpler preload behavior because they do not participate in mtime validation. File-backed opens that were not part of the startup session will call the same service helper asynchronously after the file load succeeds.

Alternatives considered:
- Keep preloading raw text and revalidate later in `check_draft_on_open`: rejected because it doubles I/O, complicates stale-draft cleanup, and makes startup restore and later file-open flows diverge.
- Remove preload entirely and always re-read drafts later: rejected because the current batch restore intentionally keeps startup to one background round-trip.

### 3. Treat confirmed stale drafts as one-time safety warnings and delete them eagerly

When validation proves that the backing file mtime changed since the draft was written, the editor should keep the freshly loaded on-disk file contents, skip `apply_draft`, and emit an editor-scoped warning that explains the draft was not restored because the file changed externally. The stale draft file and manifest entry should then be deleted so reopening the same file does not repeatedly surface the same outdated recovery state.

Using the existing editor-scoped inline-notification path keeps the feedback attached to the affected document instead of burying it in the window status bar. Deleting only after a confirmed mismatch protects users from repeated stale restores without throwing away recovery data on weaker signals.

Alternatives considered:
- Silent discard: avoids another message, but hides the fact that unsaved work was intentionally not restored.
- Keep stale drafts on disk after warning: preserves more forensic evidence, but would re-trigger the same warning on every later open and invite accidental reapplication paths.

### 4. Preserve ambiguous drafts when metadata cannot be trusted

If validation cannot read the current file metadata after the open path succeeds, the system should skip automatic restore for that attempt but keep the draft data intact instead of deleting it. This treats metadata failure as uncertainty, not proof that the draft is stale.

That trade-off favors data preservation over aggressive cleanup. Existing open/load error handling remains responsible for missing or unreadable files, and a later open on a readable filesystem can still recover the draft if the file state becomes trustworthy again.

Alternatives considered:
- Delete drafts on any validation failure: rejected because transient metadata failures would cause unnecessary recovery loss.
- Apply drafts when metadata cannot be read: rejected because it reintroduces the original trust problem.

## Risks / Trade-offs

- [Filesystem mtimes are stored at second granularity today] -> Validation can only be as precise as the persisted data, so this change removes the common stale-restore case without claiming perfect conflict detection.
- [More restore outcome states add branching across startup and normal open flows] -> Keep one shared service-level enum and add focused unit coverage for each branch.
- [Deleting stale drafts is irreversible] -> Only do it after a confirmed mtime mismatch and pair it with explicit editor-scoped feedback.
- [Legacy drafts may exist without a stored mtime] -> Preserve backward compatibility by allowing those entries to restore once under the existing behavior; future autosaves will rewrite them with mtime data.

## Migration Plan

No manifest schema migration is required because the stored `original_mtime_secs` field already exists. New code will start honoring that field for file-backed restore decisions immediately.

Drafts that already carry a stored mtime will gain the new validation behavior on the next app start or file open. Legacy file-backed drafts without a recorded mtime will continue to restore under the current permissive path so older recoverable work is not silently dropped. Rollback is low risk because the change only affects restore orchestration and stale-draft cleanup; draft files and manifest records remain in the same storage layout.

## Open Questions

None blocking.
