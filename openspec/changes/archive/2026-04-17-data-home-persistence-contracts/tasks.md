## 1. Storage Inventory And Boundaries

- [x] 1.1 Confirm that every write surface under `$XDG_DATA_HOME/lushtext` is mapped to exactly one new or modified capability in this change.
- [x] 1.2 Verify the design's durability classes for durable user state, bounded safety state, and derived cache state against the current code paths and tests.

## 2. New Capability Specs

- [x] 2.1 Review and refine the new `draft-session-recovery` and `document-save-safety` specs against the current startup restore, save, Save As, discard, and close-flow behavior.
- [x] 2.2 Review and refine the new `workspace-state-persistence` and `search-history-and-saved-searches` specs against the current persistence rules and restart behavior.
- [x] 2.3 Review and refine the new `search-replace-safety` spec against the current Replace All rollback, undo backup, and startup cleanup behavior.

## 3. Existing Capability Refreshes

- [x] 3.1 Refresh the `draft-restore-validation` and `local-history` deltas so they fully capture the current storage-backed cleanup, retention, and restore behavior.
- [x] 3.2 Refresh the `line-bookmarks` and `sidecar-annotations` deltas so they fully capture current app-data sidecar identity, rename migration, Save As reset, and empty-sidecar cleanup behavior.
- [x] 3.3 Refresh the `tab-content-transparency` delta so it captures the current derived style-scheme cache behavior under the app data directory.

## 4. Consistency And Sync

- [x] 4.1 Review proposal, design, and all capability files for overlap, contradictory guarantees, or any uncovered app-data surface.
- [x] 4.2 Archive the change and sync the resulting living specs into `openspec/specs/` once the wording is accepted.
