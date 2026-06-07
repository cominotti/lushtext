## Why

LushText already has strong durable-write and recovery behavior for many app-owned JSON files, but the JSON shapes themselves do not yet have a consistent public-era format contract. Before announcing the project more broadly, app data should move to a clean versioned contract so future releases can evolve without accumulating pre-public compatibility cruft.

## What Changes

- Introduce a shared JSON persistence format contract for long-lived app-owned JSON documents, including explicit document kind, format version, and data payload.
- Keep pretty JSON as the source-of-truth format for app-owned state; do not migrate current state to TOML or SQLite.
- Treat this as a clean break from pre-public bare JSON shapes: runtime app code MUST NOT carry permanent legacy readers for old app-data formats.
- Preserve unsupported old-shape JSON through the recovery/quarantine path before writing v1 replacements.
- If old local app data needs a bridge, add at most an optional one-shot script under `scripts/migrations/`; do not embed the migration in normal runtime loading.
- Extend recovery-aware loading to important user-managed JSON files that still fall back to empty state today, especially `workspaces.json` and `saved-searches.json`.
- Preserve recent search history as low-value ephemeral state that may degrade to empty with diagnostics, while treating saved searches as user-managed state that needs preservation before replacement.
- Add stable fixture coverage for valid v1 files, unknown fields, missing optional fields, malformed files, oversized files, unsupported old-shape files, and optional migration-script output if such a script is created.
- Replace implementation-dependent persisted identifiers, such as path-backed draft IDs based on `DefaultHasher`, with explicit stable hashing in the public v1 format.
- Document SQLite as a future index/cache option only when cross-document querying, global history/notes views, or large metadata indexes create real database-shaped pressure.
- **BREAKING** for pre-public app-data JSON: existing bare JSON files are not treated as supported runtime formats. They must be preserved with diagnostics, reset to v1 defaults, or converted only by an explicit `scripts/migrations/` helper if needed.

## Capabilities

### New Capabilities

- `persistent-json-format-contract`: Defines the shared versioned pretty-JSON contract, migration rules, compatibility tests, and SQLite deferral criteria for app-owned persistence.

### Modified Capabilities

- `recovery-metadata-integrity`: Extend recovery-aware metadata handling to versioned JSON documents and additional user-managed persistence classes.
- `workspace-state-persistence`: Require `workspaces.json` to load through recovery-aware versioned parsing instead of silently defaulting on parse failure.
- `search-history-and-saved-searches`: Distinguish ephemeral recent history from durable saved searches, with saved searches using recovery-aware versioned persistence.
- `draft-session-recovery`: Require session and draft manifest JSON to participate in the versioned format contract and use explicit stable path-backed draft IDs in the new format.
- `line-bookmarks`: Require bookmark sidecars to participate in the versioned JSON format contract while preserving current corruption isolation.
- `document-notes`: Require document-note sidecars to participate in the versioned JSON format contract while preserving current corruption isolation.
- `workspace-notes`: Require workspace-note sidecars to participate in the versioned JSON format contract while preserving current corruption isolation.
- `local-history`: Require local-history index JSON to participate in the versioned JSON format contract without moving snapshot bodies into JSON or SQLite.

## Impact

- Affected services include JSON persistence, recovery metadata, workspace manager, session service, draft service, search history and saved searches, note/bookmark sidecar services, local-history service, and any shared persistence helpers introduced for envelopes.
- Affected models include workspace, session, draft manifest, saved search, bookmark sidecar, document-note sidecar, workspace-note sidecar, local-history index, migration ledger, and Replace All undo metadata shapes.
- Tests need golden fixtures, unsupported-old-shape coverage, optional migration-script coverage if such a script is created, and targeted widget paths where visible diagnostics change.
- Runtime compatibility code should stay small because pre-public app data is allowed to clean-break. Optional manual migration tooling, if created, belongs under `scripts/migrations/`.
- No new runtime dependency is expected for implementation; SQLite remains a documented future option rather than part of this change.
