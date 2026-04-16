## Why

LushText already records the backing file mtime when it autosaves a draft, but startup restore does not consult that value before applying the draft back into the editor. If the file changed externally after the draft was written, the app currently restores stale draft content anyway and can leave the user staring at conflicting recovery warnings.

This is a data-safety gap in a recovery path that is supposed to protect work, not make the file state harder to trust. The manifest already carries the comparison data, so this change can make draft restore safer without adding a new persistence model.

## What Changes

- Validate file-backed drafts against the current file mtime before restoring them on startup.
- Restore the draft only when the stored mtime still matches the backing file; otherwise keep the file's current on-disk contents, delete the stale draft, and show clear feedback that the draft was skipped because the file changed externally.
- Keep untitled draft restore behavior unchanged because those tabs have no backing file mtime to compare.
- Extend startup-restore tests and draft-service coverage for unchanged, changed, and unavailable-file cases.

## Capabilities

### New Capabilities
- `draft-restore-validation`: Safe file-backed draft recovery that validates the saved draft against the current backing file before applying recovered content.

### Modified Capabilities
- None.

## Impact

- Affected code: `crates/lushtext-core/src/model/draft.rs`, `crates/lushtext-core/src/services/draft_service.rs`, `crates/lushtext-core/src/ui/window/drafts.rs`, `crates/lushtext-core/src/ui/window/session_persistence.rs`, and related tests.
- Affected systems: startup session restore, draft manifest interpretation, stale-draft cleanup, and inline recovery feedback.
- Dependencies and APIs: builds on the existing `DraftEntry.original_mtime_secs` field and current background restore pipeline; no new external dependency is expected.
