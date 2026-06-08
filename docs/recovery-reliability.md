# Recovery Reliability

LushText treats app-owned recovery metadata as user-work evidence. When session,
draft, sidecar, local-history, migration-ledger, or Replace All recovery state
is malformed or only partly usable, the app should preserve what it found,
restore the safest subset, and report a concise diagnostic instead of silently
pretending the state was absent.

## App Data

Recovery state lives under the app data directory:

- default source run: `$XDG_DATA_HOME/lushtext`
- widget or smoke isolation: `LUSHTEXT_DATA_DIR`
- Flatpak confinement: the runtime's app-data location for
  `dev.cominotti.lushtext`

Important files and directories:

- `session.json` stores global tab layout, selected tab, cursor position,
  scroll position, and pinned state.
- `drafts/manifest.json` maps draft IDs to file-backed or untitled draft files.
- `drafts/*.draft` stores unsaved buffer text as plain UTF-8.
- `bookmarks/`, `document-notes/`, and `folder-notes/` store per-identity
  JSON sidecars; older `workspace-notes/` sidecars remain legacy-compatible
  folder-note data.
- `local-history/` stores per-file lineage indexes plus snapshot bodies.
- `search-backups/` stores Replace All undo journals during the active safety
  window.
- `migration-ledger.json` records retryable post-rename sidecar and
  local-history migration work.

## Quarantine And Repair

`services::recovery_metadata` owns the shared recovery load contract. Recovery
callers receive typed outcomes and diagnostics for missing, malformed,
unreadable, non-file, oversized, preserved, quarantined, repaired, and skipped
metadata.

Malformed metadata is moved or copied to `recovery-quarantine/` before the app
writes replacement metadata for the same logical state. If preservation fails,
the original remains in place and the caller must not overwrite it with a
default file during that load. Repair is conservative: draft manifests,
sidecars, and local-history indexes are rebuilt only when surviving files prove
identity and ordering without guessing user intent.

User-facing recovery messages stay grouped, for example "Some recovery data
could not be loaded" or "Some bookmark data could not be loaded." Full paths,
quarantine paths, metadata classes, and failure categories belong in logs,
tests, and smoke artifacts.

## Migration Ledgers

In-app file and directory renames can finish the visible rename before all
sidecar and local-history cleanup has completed. The migration ledger records
pending work by source path, target path, generation, affected metadata kind,
attempt count, completion state, and diagnostics.

Startup reconciliation retries incomplete ledger entries before browse
surfaces rely on bookmark, document-note, folder-note, or local-history listings.
Duplicate reconciliation must write the merged target durably before removing
an obsolete non-empty source. Failed cleanup remains diagnostic and retryable
instead of becoming a tight loop.

Save As remains separate from rename migration: a pending rename lineage must
not be consumed by a later Save As destination.

## Validation Tiers

Cheap pull-request coverage should stay deterministic:

- service tests for loader outcomes, quarantine, repair, migration ledgers, and
  sidecar/local-history merge safety
- integration tests for restart-style retry with temp directories
- property tests for malformed metadata and generated duplicate/journal states
- widget tests for visible grouped diagnostics and partial browse surfaces

Host-sensitive coverage stays scheduled, manual, release-only, or local until a
lane proves stable enough for PR gating:

- `make crash-recovery-smoke`
- `make visual-smoke`
- `make portal-sandbox-smoke`
- `make accessibility-smoke`
- `make performance-smoke`, whose `recovery_performance` Criterion filter
  records malformed metadata, pending migration, duplicate sidecar,
  many-lineage local-history, and first-dirty autosave fixture timings
- `make bench-report` or `make bench-report-full`

## Crash Smoke Artifacts

`make crash-recovery-smoke` stores artifacts under
`build/smoke/crash-recovery` by default:

- `environment.txt` for host/runtime details
- `state-dir.txt` and `data-dir.txt` for isolated state locations
- `metadata/before-crash/` and `metadata/after-relaunch/` for bounded metadata
  summaries and small copied metadata files
- `logs/` for the D-Bus, Mutter, app, PipeWire, WirePlumber, and AT-SPI logs
- `assertions/` for SIGKILL, AT-SPI, warning-scan, and PNG checks
- `screenshots/after-relaunch.png` when monitor capture is available

The lane is expected to skip clearly when required compositor, screenshot,
D-Bus, or AT-SPI support is unavailable. A skip is useful host-support
evidence, but it is not proof that crash recovery works on that host.

## Support Triage

When investigating a recovery report, collect:

- LushText version and packaging/runtime type
- app data directory path, noting whether it is source, Flatpak, Snap, or test
  isolation
- `recovery-quarantine/` listing with file sizes and timestamps
- `migration-ledger.json`, if present
- relevant `session.json`, `drafts/manifest.json`, sidecar index, or
  local-history `index.json` files
- logs containing `RecoveryDiagnostic` details
- crash smoke artifacts, if the issue reproduces in the smoke fixture

Avoid copying unbounded user document contents into bug reports. Prefer file
sizes, hashes, metadata class names, app-data-relative paths, and concise
diagnostic categories.
