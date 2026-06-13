## Context

LushText already persists long-lived app-owned JSON through a v1 envelope with `kind`, `version`, and `data`, and recovery loaders preserve malformed, wrong-kind, oversized, or unsupported-version metadata before replacement. The current runtime posture is intentionally latest-only: ordinary readers reject pre-public bare JSON and unsupported versions instead of carrying permanent legacy deserializers.

That posture is good for normal code, but it leaves a gap once LushText introduces v2 or later app-owned metadata. A user who launches a newer LushText with older but supported data should get a deterministic one-click upgrade path before workspace/session/draft restore defaults or quarantines state. A user who launches an older LushText against future/newer data should be protected from accidental downgrade.

Current startup matters: the window's constructed path kicks off pending rename reconciliation, workspace loading, session/draft restore, autosave, and normal UI rendering. The upgrade workflow must run before any normal metadata consumer rewrites or defaults the state it might upgrade.

## Goals / Non-Goals

**Goals:**

- Keep ordinary production models and readers latest-only.
- Add a sealed GTK-free format-upgrade service that owns old-version structs, inventory, planning, backup, conversion, and apply results.
- Run a preflight inventory before normal startup restore consumes app-owned metadata.
- Gate startup only when user choice is needed, with safest defaults:
  - supported older formats: primary action is Convert
  - future/newer formats: primary safe action is Quit
  - Start Fresh is available only after preserving incompatible data
- Add `Preferences > Data` as a secondary surface for status, manual scan, retry, and recovery/backup visibility.
- Require regression coverage across service, integration, widget, generated-input/property-style, and relevant smoke/documentation layers.

**Non-Goals:**

- No SQLite or database migration.
- No downgrade converter from future/newer formats.
- No permanent old-format compatibility inside `model/` or ordinary metadata readers.
- No automatic destructive reset.
- No broad redesign of recovery metadata quarantine, sidecar rename ledgers, or existing draft/session restore semantics.
- No hidden CLI-only migration as the primary user path.

## Decisions

### 0. Baseline Inventory And Scan Bounds

The first implementation treats the current public v1 JSON envelope as the
latest baseline. It inventories the real app-owned persistence locations below
before normal startup consumers run.

| Area | App-data path | JSON kind | Startup critical | Baseline action |
| --- | --- | --- | --- | --- |
| Workspace state | `workspaces.json` | `workspace-state` | Yes | v1 is current, missing is no-op |
| Session state | `session.json` | `session` | Yes | v1 is current, missing is no-op |
| Draft manifest | `drafts/manifest.json` | `draft-manifest` | Yes | v1 is current, missing is no-op |
| Draft bodies | `drafts/*.draft` | plain UTF-8 draft content | Yes, preservation only | not version-converted; preserved by Start Fresh |
| Saved searches | `saved-searches.json` | `saved-searches` | No | v1 is current, missing is no-op |
| Search history | `search-history.json` | `search-history` | No | v1 is current, missing is no-op |
| Bookmark sidecars | `bookmarks/*.json` | `bookmark-sidecar` | Yes, for note/bookmark recovery | v1 is current, missing is no-op |
| Document-note sidecars | `document-notes/*.json` | `document-note-sidecar` | Yes, for notes browser | v1 is current, missing is no-op |
| Folder-note sidecars | `folder-notes/*.json` | `folder-note-sidecar` | Yes, for notes browser | v1 is current, missing is no-op |
| Legacy folder-note sidecars | `workspace-notes/*.json` | `workspace-note-sidecar` | Yes, existing narrow compatibility | v1 is current for pre-rename support |
| Local-history indexes | `local-history/*/index.json` | `local-history-index` | Yes, for history recovery | v1 is current, missing is no-op |
| Migration ledger | `migration-ledger.json` | `migration-ledger` | Yes | v1 is current, missing is no-op |
| Replace All undo manifest | `replace-backup-journal/manifest.json` | `replace-all-undo-manifest` | Yes, for undo safety | v1 is current, missing is no-op |
| Replace All undo entries | `replace-backup-journal/*.json` | `replace-all-undo-entry` | Yes, for undo safety | v1 is current, missing is no-op |
| Replace All cleanup marker | `replace-backup-journal/cleanup-in-progress.json` | `replace-all-undo-cleanup-marker` | Yes, for stale cleanup safety | v1 is current, missing is no-op |
| Retired Replace All backup | `replace-backup.json` | `retired-replace-all-undo-backup` | Yes, preservation only | unsupported runtime state; no Convert |

The scan is intentionally bounded and app-data scoped:

- Metadata file reads reuse the recovery metadata byte cap: 16 MiB per JSON
  metadata file.
- Sidecar directories (`bookmarks`, `document-notes`, `folder-notes`, and
  `workspace-notes`) scan at most 10,000 entries per directory and only inspect
  `.json` files.
- `local-history` scans at most 10,000 lineage directories and reads only each
  lineage `index.json`; snapshot text bodies are not part of format preflight.
- `replace-backup-journal` scans at most 10,000 entry files plus the fixed
  manifest and cleanup marker.
- `drafts` scans at most 2,048 draft bodies, matching the existing repair and
  orphan-cleanup safety budget; draft bodies are preserved, not converted.
- Backup directories such as `format-upgrade-backups` and recovery quarantine
  output are never recursively scanned as upgrade inputs.

### 1. Use a Hybrid Startup Gate Plus Preferences Data Page

Startup is the primary safety path. Preferences is the retry/inspection path.

```
launch
  |
  v
preflight inventory -- current only ---------> normal workspace/session/draft startup
  |
  +-- supported older critical data ---------> modal: Convert / Start Fresh / Quit
  |
  +-- future newer data ---------------------> modal: Quit / Start Fresh
  |
  +-- unsupported/damaged/unsafe data -------> existing recovery warning/default behavior
```

Preferences-only is insufficient because normal startup consumers may already default, quarantine, or skip the state the user wanted upgraded. Startup-only is too rigid because users also need a place to retry after a failed conversion, inspect backups, and verify that data is current.

### 2. Introduce `services::format_upgrade` As The Only Legacy-Knowledge Boundary

Suggested module shape:

```text
services/format_upgrade/
  mod.rs
  inventory.rs
  plan.rs
  apply.rs
  backup.rs
  diagnostics.rs
  legacy/
    mod.rs
    v1.rs
    v2.rs        # added only when v3 exists
```

The service returns plain Rust values such as `FormatInventory`, `FormatPlan`, `FormatIssue`, `FormatAction`, `FormatApplyOutcome`, and `FormatBackupRecord`. It must not depend on GTK/GLib. UI layers convert those values into rows, dialogs, and notifications.

Old payload structs live under `services::format_upgrade::legacy::*`, not in `model/`. Latest payloads are written through existing latest service save paths or through narrow helpers that use `JsonEnvelopeRef` and the durable filesystem boundary.

### 3. Keep Query And Command Phases Separate

The upgrade path should be CQS-clean: Command-Query Separation means queries answer questions and commands change state.

```text
scan(data_dir) -> FormatInventory
build_plan(inventory) -> FormatPlan
apply_plan(data_dir, plan) -> FormatApplyOutcome
```

`scan` and `build_plan` do not write. `apply_plan` backs up/preserves first, then writes latest envelopes. UI code must not infer actionability by reparsing JSON itself; it should render the plan returned by the service.

### 4. Classify Metadata Before Acting

Each app-owned metadata path gets one item-level classification:

- `Current`: latest supported format; no action
- `Missing`: no file; no warning
- `Upgradeable`: recognized older supported version with a tested converter path
- `FutureVersion`: recognized kind but version newer than this app
- `UnsupportedOld`: older/pre-public/wrong shape with no converter; reported by
  preflight but left to existing recovery/quarantine/default handling
- `Damaged`: malformed, unreadable, non-file, oversized, or unsupported payload
- `UnsafeToReplace`: preservation/backup failed or cannot be proven safe

Only `Upgradeable` items get a Convert action. `FutureVersion` never gets
Convert because that would be an untested downgrade, and it is the only
non-convert classification that forces the startup Start Fresh decision.

### 5. Make Backup And Preservation A Hard Precondition

Before writing any latest-format replacement, `apply_plan` must preserve the previous bytes for each affected item. Use the existing recovery/quarantine machinery where it fits and add a format-upgrade backup manifest when a grouped conversion needs one atomic-looking user story.

Minimum backup contract:

- app-data-local backup directory, not user document directories
- original app-data-relative path
- metadata kind
- original version/classification
- LushText version performing the upgrade
- timestamp
- result for each item

If backup/preservation fails for any item in the plan, that item is not overwritten. For a grouped plan with dependencies, fail the group before partial writes unless the plan explicitly marks independent items.

### 6. Startup Coordinator Pauses Normal Metadata Consumers

Implementation should refactor window startup so workspace loading, session/draft restore, and related metadata consumers start only after preflight completes.

One practical shape:

```text
LushtextWindow::new()
  builds shell, actions, notification bus
  does not load workspaces/session yet

window.begin_startup_data_flow(activation_intent)
  spawn_blocking_then(scan + plan)
  if no blocking decision: continue_startup_data_flow()
  if decision required: present compatibility dialog
```

`continue_startup_data_flow()` should own the existing calls now in `constructed()`:

- pending rename migration reconciliation
- sidebar workspace load
- workspace scope consumer refresh
- session/draft restore
- autosave timer start

This prevents an old workspace/session/draft file from being defaulted or rewritten before the user chooses Convert.

The first refactor keeps GTK template construction, action installation,
notification rendering, search-panel setup, sidebar signal wiring, tab
callbacks, and empty content-stack rendering available immediately. Those steps
do not consume app-owned metadata in a way that can default or rewrite
upgradeable state. The new startup-data-flow boundary owns only the calls that
read, repair, rewrite, or clean app-owned metadata.

### 7. Dialog Behavior

Supported older critical data:

- heading: older LushText data can be updated
- primary/default: Convert
- secondary: Start Fresh
- safe escape: Quit
- Convert runs in a background task and disables choices while active
- success continues startup using latest data
- failure leaves backed-up/original data preserved and offers Retry or Quit
- older or pre-public data without a converter does not show this dialog; normal
  latest-format recovery preserves it and continues with defaults

Future/newer data:

- heading: data was created by a newer LushText
- primary/default safe action: Quit
- secondary: Start Fresh
- no Convert action
- Start Fresh preserves incompatible metadata before latest defaults are written

The dialog must summarize by data kind, not show a wall of paths. Detailed paths belong in logs, backup manifests, or an optional details expander/list.

### 8. Preferences > Data Surface

Add a third preferences page, `Data`, with status and actions:

- current/no action: "Data format is current"; Convert disabled or hidden
- upgradeable data: grouped summary plus Convert
- failed conversion: result summary plus Retry
- future/newer data: "Created by a newer LushText"; no Convert
- damaged data: recovery status and quarantine/backup guidance
- many items: summary remains fixed/readable, item details scroll in their own region

The page must work with no app data, one representative item, many awkward paths/kinds, and constrained dialog geometry. Controls must remain reachable without horizontal scrolling or fake rows.

### 9. Testing Strategy

Careful testing is part of the feature, not polish.

Service/unit tests:

- inventory classification for missing/current/upgradeable/future/unsupported/damaged/unsafe
- v1-current baseline is no-op today
- future v2 fixtures prove v1 -> v2 conversion once a v2 format exists
- backup-before-write ordering with deterministic failure seams
- no writes during scan/build_plan
- no downgrade action for future versions
- malformed/generated JSON inputs do not panic

Integration tests:

- temp app-data directory with mixed session/workspace/draft/sidecar/search/history metadata
- grouped apply preserves originals and writes latest envelopes
- failed item preserves originals and reports retryable outcome
- Start Fresh preserves incompatible data before defaults can be written

Widget tests:

- startup gate appears before workspace/session restore consumes old metadata
- Convert success continues startup
- Convert failure leaves the app blocked with Retry/Quit rather than continuing on empty state
- future-version dialog has Quit/Start Fresh and no Convert
- Preferences Data page covers empty/current, representative upgradeable, many items, failure, and constrained geometry
- `make test-widget-headless` must be clean with no `FLAKY:` output

Generated/property-style tests:

- bounded generated envelopes and malformed values exercise inventory classification
- converter chains are idempotent at the latest version
- plan ordering is deterministic regardless of directory iteration order

Smoke/docs:

- update recovery documentation and persistent JSON fixture docs
- consider crash/restart smoke for a seeded old-format state when a real v2 migration exists
- run automation/action documentation checks if new visible actions are exported

## Risks / Trade-offs

- [Risk] Startup feels blocked or scary. -> Mitigation: gate only when user choice is required, summarize clearly, and continue silently for current/missing data.
- [Risk] Old-format structs spread into normal runtime code. -> Mitigation: keep them under `services::format_upgrade::legacy` and add review/test checks around module boundaries.
- [Risk] A partial upgrade loses state. -> Mitigation: backup/preservation is mandatory before writes, group dependent items, and expose retryable outcomes.
- [Risk] Future-version handling accidentally becomes a downgrade. -> Mitigation: never show Convert for versions newer than this binary; default to Quit.
- [Risk] Startup preflight scans too much on slow disks. -> Mitigation: scan known app-owned paths only, bound sidecar enumeration, run I/O through `spawn_blocking_then`, and keep main-thread work O(1).
- [Risk] Preferences becomes a dumping ground for support tooling. -> Mitigation: keep the page status/action focused; put detailed evidence in backup manifests/logs.
- [Risk] Tests prove only service logic and miss GTK timing. -> Mitigation: require widget tests for startup gating and Preferences state extremes, with headless clean-run verification.

## Migration Plan

1. Add the `format_upgrade` service as a no-op/current-v1 inventory and planning path.
2. Refactor startup orchestration so normal metadata consumers can be delayed behind preflight.
3. Add the startup compatibility dialog and the Preferences Data page, initially proving current/no-op and future/unsupported behavior with fixtures.
4. Add converter-chain scaffolding and fixture discipline so the next format bump adds old-version structs only inside `legacy/`.
5. Replace the old script-only migration statement in the persistent JSON contract with the sealed app-owned upgrade contract.
6. Add comprehensive tests before introducing any real v2 format change.

Rollback strategy: because the current baseline is v1, the first implementation can be functionally no-op for current data. If the UI or startup coordinator regresses, remove the gate call and keep ordinary v1 recovery behavior unchanged. Once a real v2 converter exists, rollback must preserve backup manifests and avoid writing older formats.

## Open Questions

- Should `Start Fresh` preserve whole metadata directories for simplicity, or only affected files with a manifest? The safest first implementation likely backs up affected files plus a manifest.
- Should the startup gate live in `app.rs` before window creation, or in `LushtextWindow` before startup data flow? Window-level is likely simpler because dialogs need a parent and existing startup flows are window-owned.
- Do we want a non-exported UI-only action for Preferences Data retry, or should manual conversion be a normal cataloged action? If exported, update automation docs and action catalog.
