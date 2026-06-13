## Why

LushText already has a public versioned JSON envelope and recovery/quarantine handling, but it does not yet have a user-facing path for upgrading supported older app-owned metadata into the latest format. Before the persisted formats evolve beyond the current v1 baseline, the app needs a deterministic upgrade workflow that protects drafts, sessions, workspaces, sidecars, history, and undo evidence without letting old-format knowledge leak into normal runtime models.

## What Changes

- Add a format-upgrade workflow that scans app-owned metadata before normal startup restore consumes it, distinguishes current, supported older, future/newer, damaged, and unsafe-to-replace states, and produces a user-facing decision.
- Add a startup compatibility gate for critical upgradeable or incompatible metadata, with safe defaults: Convert for supported older formats, Quit as the safest default for newer/future formats, and Start Fresh only after preserving existing data.
- Add a `Preferences > Data` page for current format status, manual scan, retryable conversion, and recovery/backup visibility after startup.
- Keep normal production readers and domain models latest-only. Legacy format structs and conversion intelligence live only in a sealed format-upgrade service area.
- Preserve the existing recovery metadata contract for malformed, oversized, unreadable, wrong-kind, and otherwise damaged data; supported older versions are upgrade candidates, not ordinary corruption.
- Require careful deterministic testing across service, integration, widget, generated-input/property-style, and smoke/documentation layers before implementation is considered complete.

## Capabilities

### New Capabilities

- `format-upgrade-workflow`: Preflight inventory, user-mediated upgrade decisions, sealed legacy converters, Preferences Data status/retry surface, and end-to-end testing expectations for app-owned metadata format upgrades.

### Modified Capabilities

- `persistent-json-format-contract`: Replace the old optional script-only migration posture with an app-owned sealed upgrade workflow while preserving latest-only runtime readers and explicit versioned envelopes.
- `recovery-metadata-integrity`: Clarify how upgradeable older formats are classified and protected before normal recovery/defaulting, while damaged or unsafe metadata remains covered by existing quarantine diagnostics.

## Impact

- Affected Rust areas: `services::json_format`, `services::recovery_metadata`, a new GTK-free `services::format_upgrade` module family, startup orchestration in `app.rs` or `ui/window`, `ui/preferences`, and any metadata service participating in the upgrade inventory.
- Affected persisted data: app-owned JSON metadata under `$XDG_DATA_HOME/lushtext`, draft manifest and draft bodies, session state, workspaces, saved searches/search history, bookmark/document-note/folder-note sidecars, local-history indexes, migration ledgers, and Replace All undo journals.
- Affected UI: a startup compatibility dialog/gate and a new `Preferences > Data` page.
- Affected tests and docs: persistent JSON fixtures, recovery metadata tests, service/integration/widget tests, crash/restart or visual smoke coverage where useful, `docs/recovery-reliability.md`, and any automation/action catalog documentation if new visible actions are exported.
- No new runtime database, no SQLite migration, and no old-format compatibility in ordinary latest-format model/service readers.
