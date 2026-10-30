## ADDED Requirements

### Requirement: Automation snapshots report Protected View state without content
Tab snapshots SHALL include:
- `protected_view` (boolean);
- `protected_view_reason` (`paragraph-length`, from C1's `ReductionReason::LongParagraph`, `edit-growth`, or null);
- `entry_undo_available` (boolean);
- `folded_paragraph_count`;
- `hidden_character_count`.

The surface snapshot SHALL include `line_inspector_visible`. These fields SHALL NOT include document text, inspector window text, find queries or match text.

Readiness SHALL reuse the existing blockers and predicates:
- fold application SHALL complete inside the existing `file-load` blocker, so the `file-open-complete` and `session-restore-complete` predicates cover it;
- protected find, including the one source capture after an edit-growth entry, SHALL hold the existing `editor-search` blocker until its latest generation settles, so the `search-complete` predicate covers it;
- recovery of a protected draft, including its fold-at-insertion install, SHALL stay inside the existing `draft-autosave` blocker, so `recovery-restore-complete` covers it;
- Line Inspector window reads SHALL hold a readiness blocker that `idle` and `accessibility-settled` include. It SHALL be an existing blocker if one fits, or a new `line-inspector` blocker documented in `docs/automation.md` and `docs/automation-reference.md`.

A dedicated fold-settle blocker SHALL be added only if the Protected View verdict shows a fold settle that `file-load` does not cover, and in that case it SHALL be documented in `docs/automation.md` and `docs/automation-reference.md`.

#### Scenario: Snapshot of a protected tab
- **WHEN** an automation client reads the snapshot after `WaitForReady("file-open-complete")` on a protected document
- **THEN** the active tab reports `protected_view: true` with the reason and the counts, and no field contains document characters

#### Scenario: Protected find readiness
- **WHEN** a client starts a protected find and calls `WaitForReady("search-complete")`
- **THEN** the predicate settles only after the worker's final batch for the latest generation is published
