## 1. Recovery Metadata Foundation

- [x] 1.1 Add a `services::recovery_metadata` module with typed load outcomes, recovery diagnostics, metadata classes, quarantine paths, and bounded input-size handling.
- [x] 1.2 Implement durable quarantine or preserve-in-place behavior for malformed, unreadable, non-file, oversized, and unsupported recovery metadata.
- [x] 1.3 Add conservative repair hooks for metadata callers that can rebuild state without guessing user intent.
- [x] 1.4 Route recovery metadata writes and quarantine moves through the existing filesystem and durable-write boundaries.
- [x] 1.5 Add service tests for valid, missing, malformed, unreadable, non-file, oversized, quarantine-success, quarantine-failure, and repairable metadata.
- [x] 1.6 Add generated malformed-byte or corpus-replay coverage proving recovery loaders do not panic and do not overwrite preserved evidence.

## 2. Startup Restore, Drafts, and Session Persistence

- [x] 2.1 Refactor startup restore to return a structured restore result containing session data, draft manifest data, preloaded drafts, and diagnostics.
- [x] 2.2 Load `session.json` through recovery metadata handling so malformed session metadata is preserved, diagnostic, and does not erase unrelated restore state.
- [x] 2.3 Load `drafts/manifest.json` through recovery metadata handling so malformed manifests preserve draft files and produce diagnostics.
- [x] 2.4 Implement conservative draft-manifest repair or partial recovery for draft files whose identity can be proven without guessing.
- [x] 2.5 Surface grouped startup recovery diagnostics through the window notification or inline-alert path after restore completes.
- [x] 2.6 Track debounced and close-time session-save failures as dirty, retryable, generation-aware state.
- [x] 2.7 Show visible close-time session-save failure feedback after document and draft safety have succeeded.
- [x] 2.8 Add first-dirty draft autosave scheduling that reuses existing autosave batching, generation guards, chunked snapshots, and retry behavior.
- [x] 2.9 Add integration tests for corrupt session JSON, corrupt draft manifests, partial draft recovery, and unrelated valid restore state.
- [x] 2.10 Add widget tests for grouped startup diagnostics, document-scoped stale-draft warnings, visible session-save failure, and retry clearing.
- [x] 2.11 Add timing-controlled tests for first-dirty autosave on small buffers, large chunked buffers, in-flight autosave coalescing, and failed-write retry.

## 3. Migration Ledger and Startup Reconciliation

- [x] 3.1 Add a durable migration-ledger model and service for post-rename sidecar and local-history migrations.
- [x] 3.2 Record pending migration entries before or as part of bookmark, document-note, workspace-note, and local-history post-rename work.
- [x] 3.3 Track per-kind completion, bounded attempt counts, diagnostics, and durable removal of completed ledger entries.
- [x] 3.4 Run startup reconciliation before browse surfaces depend on sidecar, note, bookmark, or local-history lists.
- [x] 3.5 Detect duplicate old/new sidecars, orphaned sidecars, duplicate local-history lineages, and incomplete cleanup state.
- [x] 3.6 Implement conservative merge policies that never delete the last non-empty copy before a merged target is durably written.
- [x] 3.7 Add service tests for ledger create/update/remove ordering, stale generations, repeated failures, completed retries, and startup retry.
- [x] 3.8 Add generated state-machine tests for duplicate and orphan reconciliation so the reconciler preserves last-copy evidence.

## 4. Bookmark and Note Sidecar Hardening

- [x] 4.1 Load bookmark sidecars through recovery-aware handling and report corrupt bookmark sidecars without clearing valid bookmarks elsewhere.
- [x] 4.2 Add bookmark migration retry, duplicate merge, obsolete cleanup retry, and partial browser diagnostics.
- [x] 4.3 Load document-note sidecars through recovery-aware handling and keep files usable when their note sidecar is corrupt.
- [x] 4.4 Add document-note migration retry, newest-body reconciliation, conflict preservation, and notes-browser partial recovery.
- [x] 4.5 Load workspace-note sidecars through recovery-aware handling and keep workspaces usable when their root note is corrupt.
- [x] 4.6 Add workspace-note root-migration retry, duplicate reconciliation, aggregate-browser partial recovery, and warning feedback.
- [x] 4.7 Add service tests for corrupt bookmark, document-note, and workspace-note sidecars.
- [x] 4.8 Add integration tests for sidecar migration failure across restart and successful retry cleanup.
- [x] 4.9 Add widget tests proving notes and bookmark browse surfaces remain usable with one corrupt sidecar and visible partial-recovery feedback.

## 5. Local History Hardening

- [x] 5.1 Load local-history lineage indexes through recovery-aware handling while preserving snapshot text files on corrupt indexes.
- [x] 5.2 Implement deterministic local-history index repair only when snapshot evidence can prove identity and ordering.
- [x] 5.3 Add retryable local-history lineage migrations for in-app file and directory renames.
- [x] 5.4 Reconcile duplicate local-history lineages with bounded scan and time budgets.
- [x] 5.5 Preserve Save As lineage separation even when pending rename migration state exists.
- [x] 5.6 Add service tests for corrupt indexes, intact snapshots, deterministic repair, ambiguous repair, and retention-safe merges.
- [x] 5.7 Add integration tests for local-history migration failure, restart retry, duplicate lineage merge, and cleanup failure diagnostics.
- [x] 5.8 Add widget tests for partial local-history browsing with valid snapshots and visible recovery diagnostics.
- [x] 5.9 Add performance coverage for many-lineage reconciliation timing and deferred work reporting.

## 6. Replace All Undo-Journal Hardening

- [x] 6.1 Load Replace All undo-journal state through recovery-aware handling for malformed, partial, stale, and unsupported journal entries.
- [x] 6.2 Ensure corrupt or partial Replace All journals are never exposed as active undo sources.
- [x] 6.3 Make stale journal cleanup restart-safe so interruption cannot resurrect an inactive undo affordance.
- [x] 6.4 Report cleanup failures for current journal directories and legacy `replace-backup.json` without tight retry loops.
- [x] 6.5 Add service tests for malformed journal entries, partial journals, legacy backup corruption, cleanup markers, and active-undo exclusion.
- [x] 6.6 Add integration tests for interrupted startup cleanup and undo-completion cleanup across restart.
- [x] 6.7 Add generated journal-state tests proving invalid or incomplete journals never produce an undo affordance.

## 7. Crash, Visual, Portal, and Confined Smoke

- [x] 7.1 Add a real-process crash/restart smoke script with isolated XDG data, config, cache, and runtime directories.
- [x] 7.2 Drive crash smoke through stable actions, accessibility-visible controls, debug-only test actions, or deterministic helper APIs instead of coordinate-only input.
- [x] 7.3 Verify file-backed draft recovery, untitled draft recovery, session tab selection, and at least one feasible sidecar recovery path across `SIGKILL` and relaunch.
- [x] 7.4 Preserve crash smoke artifacts including before-crash metadata, after-relaunch metadata, environment reports, stdout, stderr, journal logs, screenshots if available, and assertion output.
- [x] 7.5 Fail crash smoke on unexpected GTK, GDK, Libadwaita, GIO, portal, accessibility, or filesystem warnings while preserving logs.
- [x] 7.6 Add documented local and scheduled/manual commands for crash recovery smoke while keeping it out of default PR gating until stable.
- [x] 7.7 Extend visual smoke to capture intentional recovery diagnostics, quarantine summaries, and nonblank recovery-focused screenshots.
- [x] 7.8 Extend portal and sandbox smoke to verify confined recovery metadata persistence, restart recovery, denials, app-data paths, and clear unsupported-runtime skips.

## 8. Performance, Documentation, and Developer Workflow

- [x] 8.1 Add performance-smoke fixtures for malformed metadata, pending migrations, duplicate sidecars, many local-history lineages, and first-dirty autosave.
- [x] 8.2 Record recovery performance reports with fixture counts, metadata sizes, repaired or quarantined counts, timing, environment details, and thresholds.
- [x] 8.3 Keep pure recovery benchmarks and service tests display-free, and keep GTK-visible recovery responsiveness in widget or smoke harnesses.
- [x] 8.4 Tier recovery validation into cheap PR-friendly tests and deeper scheduled/manual smoke or benchmark runs.
- [x] 8.5 Refresh `docs/next/session-restore-wiring.md` so it no longer describes already-implemented session restore wiring as future work.
- [x] 8.6 Add or update developer documentation for recovery metadata, quarantine locations, migration ledgers, crash smoke, and support-triage artifacts.
- [x] 8.7 Update Makefile, CI, and AGENTS/rules build documentation for any new validation commands.

## 9. Final Verification

- [x] 9.1 Run `openspec validate --change harden-overall-reliability --strict`.
- [x] 9.2 Run `make check`.
- [x] 9.3 Run `make test`.
- [x] 9.4 Run `make test-prop`.
- [x] 9.5 Run malformed-metadata fuzz corpus replay or generated-state replay.
- [x] 9.6 Run the new crash recovery smoke locally, or document a host-support skip with artifacts.
- [x] 9.7 Run visual, portal/sandbox, and performance smoke lanes where host support is available, or document explicit skip reasons.
