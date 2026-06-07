## Why

LushText already has strong durable-write, save, draft, local-history, and safety-window foundations, but the remaining reliability risk lives in the spaces between those systems: malformed recovery metadata, asynchronous sidecar migrations, session-save visibility, and lack of a whole-process crash/restart proof. This change hardens those edges so user work, annotations, workspace context, and diagnostic evidence remain recoverable even when the app, disk, or runtime behaves badly.

## What Changes

- Add a cross-cutting recovery metadata integrity contract for app-owned JSON and sidecar state, including quarantine, repair attempts, diagnostics, and non-destructive startup behavior.
- Add a crash/restart recovery smoke capability that launches the real app, mutates state, terminates the process abruptly, relaunches with the same isolated data home, and verifies recovery artifacts and user-visible feedback.
- Strengthen draft/session recovery so malformed session or draft metadata does not silently become an empty restore, close-time and debounce session-save failures become visible and retryable, and the first-dirty draft recovery window is reduced without weakening responsiveness.
- Strengthen bookmark, document-note, workspace-note, and local-history migration after in-app renames with retryable migration state, startup reconciliation, duplicate cleanup, corrupt sidecar handling, and user-visible partial-failure diagnostics.
- Strengthen Replace All undo-journal recovery so corrupted, partial, or stale recovery journals are isolated diagnostically instead of being silently ignored or destructively cleaned up.
- Extend smoke and performance coverage with crash/restart recovery, recovery-metadata corruption fixtures, sidecar migration failure fixtures, and responsiveness checks for faster autosave behavior.
- Refresh stale planning documentation that still describes already-implemented session wiring as future work.

## Capabilities

### New Capabilities
- `recovery-metadata-integrity`: Cross-cutting integrity, quarantine, repair, and diagnostic behavior for app-owned recovery metadata such as session JSON, draft manifests, sidecar documents, local-history indexes, and transient recovery journals.
- `crash-restart-recovery-coverage`: Real-process crash/restart smoke coverage for draft recovery, session restore, sidecar state, and runtime diagnostics.

### Modified Capabilities
- `draft-session-recovery`: Startup restore, draft autosave, and session persistence become diagnostic, non-destructive, retryable, and faster to establish first recovery data.
- `line-bookmarks`: Bookmark sidecar load and rename migration gain corruption handling, retryable migration state, startup reconciliation, and partial-failure diagnostics.
- `document-notes`: Document-note sidecar load and rename migration gain corruption handling, retryable migration state, startup reconciliation, and partial-failure diagnostics.
- `workspace-notes`: Workspace-note sidecar load and root rename migration gain corruption handling, retryable migration state, startup reconciliation, and partial-failure diagnostics.
- `local-history`: Local-history lineage indexes and snapshot migrations gain corruption handling, retryable migration state, startup reconciliation, and bounded partial-failure behavior.
- `search-replace-safety`: Replace All undo journals gain corruption handling, diagnostic stale cleanup, and restart-safe partial-journal behavior.
- `desktop-visual-smoke-coverage`: Desktop smoke artifacts include recovery-focused runtime diagnostics where a visual or real-session run exercises recovery state.
- `portal-sandbox-workflow-coverage`: Confined-runtime smoke coverage includes recovery-data persistence and crash/restart behavior where the runtime can support it.
- `performance-regression-coverage`: Performance and responsiveness coverage includes first-dirty draft autosave and recovery-metadata startup repair paths.

## Impact

- Affected services: `draft_service`, `session_service`, `json_store`, bookmark/document-note/workspace-note services, local-history service, search backup cleanup helpers, filesystem write/mutation boundaries, and any new recovery-metadata helper module.
- Affected UI: window session/draft lifecycle, status notifications, inline recovery warnings, note/bookmark migration messages, and close/startup flows.
- Affected tests and tooling: unit/integration tests for corrupt metadata and migration ledgers, widget tests for visible recovery warnings, a new real-process crash/restart smoke lane, performance smoke updates, and CI/scheduled smoke wiring.
- Affected documentation: OpenSpec canonical specs, developer reliability docs, smoke documentation, and stale `docs/next/session-restore-wiring.md`.
- No breaking user-facing API changes are expected.
