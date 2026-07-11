## 1. Cleanup Contract

- [x] 1.1 Define draft-entry fingerprints, `DraftOrphanCleanupPlan`, `DraftOrphanCleanupOutcome`, retained/failure records, and typed scan/status/manifest errors.
- [x] 1.2 Implement bounded, side-effect-free inspection using `filesystem::metadata::path_status` and the existing 2,048-entry directory scan cap.
- [x] 1.3 Make directory scan failure return no executable partial plan and record `has_more_work` when a bounded successful scan cannot prove completion.
- [x] 1.4 Add pure/fixture tests for plan contents, missing versus metadata error, unknown files, duplicate IDs, cap boundaries, and saturating confirmed counts.

## 2. Conservative Execution

- [x] 2.1 Revalidate every orphan body candidate against the latest manifest before deletion and count only confirmed deleted files.
- [x] 2.2 Recheck body status and exact entry fingerprint before removing a missing-body manifest entry so a newer same-ID draft survives stale cleanup.
- [x] 2.3 Commit manifest removals through the serialized durable update path and return exact committed fingerprints plus the latest persisted manifest.
- [x] 2.4 Preserve failed/ambiguous artifacts, report per-item failures, and avoid tight retries or destructive fallback on status errors.

## 3. Window Integration and Diagnostics

- [x] 3.1 Replace the mutable-manifest `cleanup_orphans` caller with deferred inspect/execute orchestration off the GTK main thread.
- [x] 3.2 Merge only exact committed fingerprints into the current in-memory manifest and retain any newer concurrent entry.
- [x] 3.3 Surface grouped recovery diagnostics for scan, status, delete, and manifest-write failures while leaving unaffected startup recovery usable.
- [x] 3.4 Schedule another bounded deferred pass only when the outcome reports remaining work and startup recovery still trusts the manifest.
- [x] 3.5 Remove the old `Result<usize>` API and all before/after-ID inference after callers migrate.

## 4. Fault and Concurrency Verification

- [x] 4.1 Add filesystem fault tests for unreadable directories, metadata denial, delete failure, already-absent files, and manifest-write failure.
- [x] 4.2 Add concurrency tests for a body or newer manifest entry appearing between inspection and execution and for partial successful outcomes.
- [x] 4.3 Add generated/property-style combinations proving reported removals always correspond to confirmed actions and failed evidence remains retryable.
- [x] 4.4 Run formatting, data-safety and architecture review, focused tests, `make check`, `make lint-advisory`, and `make pre-commit`; fix every issue found.
- [x] 4.5 Run the learning workflow and update recovery rules only if a new durable invariant is discovered.
