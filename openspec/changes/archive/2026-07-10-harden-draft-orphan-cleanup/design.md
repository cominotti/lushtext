## Context

Deferred startup cleanup receives a trusted manifest snapshot and currently mutates it in memory. Presence checks collapse metadata errors into `false`, directory scan errors are ignored, failed orphan-file deletions still increment the cleaned count, and the caller ignores the returned `Result`. A later UI merge removes IDs by comparing before/after snapshots, which can also hide concurrent manifest evolution. Cleanup is recovery work: ambiguous state must be preserved, and reported success must correspond to confirmed filesystem and manifest outcomes.

## Goals / Non-Goals

**Goals:**

- Make inspection failures, mutation failures, and confirmed cleanup distinguishable.
- Plan from trusted evidence before destructive work.
- Revalidate against the latest manifest and file state before applying cleanup.
- Preserve ambiguous artifacts and keep failures retryable and diagnostic.
- Merge confirmed manifest changes without overwriting newer draft entries.

**Non-Goals:**

- Deleting unclassified files during manifest repair or untrusted startup recovery.
- Changing draft naming, retention, automatic restore policy, or v1 metadata shape.
- Turning cleanup into a synchronous startup blocker.
- Creating a generic repository abstraction for filesystem cleanup.

## Decisions

### Separate inspection plan from execution outcome

`inspect_orphan_cleanup` will perform bounded directory scanning and recovery-aware `path_status` queries and return a `DraftOrphanCleanupPlan`. The plan contains entry fingerprints confirmed missing, orphan draft-file candidates confirmed present, the scan bound state, and no destructive side effects. A directory scan failure returns a typed error and produces no executable partial plan.

`execute_orphan_cleanup` consumes the plan in background work and returns `DraftOrphanCleanupOutcome` with confirmed deleted files, confirmed removed manifest fingerprints, retained/skipped items, per-item failures, the latest persisted manifest when changed, and whether more bounded work remains.

Alternatives considered:

- Keeping `Result<usize>` was rejected because one number cannot express partial and retryable results.
- Logging and continuing on all failures was rejected because callers cannot distinguish success from lost evidence.
- A trait-based cleanup repository was rejected because the existing filesystem boundary and fixtures already provide the seam.

### Use recovery-aware status and revalidation

Manifest entries are candidates for removal only when `path_status` confirms the body is missing. Permission, symlink, metadata, and other I/O errors retain the entry with diagnostics. Before persisted manifest removal, execution rechecks the path and verifies that the latest entry still matches the inspected fingerprint, including draft ID and saved generation metadata.

Orphan body candidates are deleted only after the latest persisted manifest still has no entry for that ID. A successful no-op because the file has already disappeared is recorded as absent, not as a deletion. Failed deletion remains a retained candidate and is not included in the cleaned count.

### Commit manifest changes through merge-safe update

Confirmed missing-entry fingerprints are removed through the serialized `update_manifest` path against the latest on-disk manifest. If the manifest write fails, the outcome reports zero committed manifest removals and returns the write failure; the in-memory window manifest is not edited as though persistence succeeded. The GTK completion merges exact committed fingerprints into current state and leaves a newer same-ID entry intact.

### Keep cleanup deferred and bounded

The existing 2,048-entry scan cap remains. `has_more_work` schedules a later bounded pass through the existing deferred mechanism rather than looping. Cleanup still runs only when startup recovery marked the manifest trusted, and all filesystem work remains off the GTK thread.

## Risks / Trade-offs

- [Revalidation adds filesystem probes] → Keep the pass bounded, perform it on a worker, and prefer preservation when status is uncertain.
- [A race can create or remove a draft between inspection and execution] → Recheck body status and latest manifest fingerprints immediately before each destructive decision.
- [Partial file deletions can succeed before manifest persistence fails] → Report each confirmed file deletion independently; never report manifest removal until its durable update succeeds.
- [Persistent failures may generate repeated warnings] → Group diagnostics per bounded pass and retry only on normal deferred recovery opportunities, not a tight loop.

## Migration Plan

1. Add plan, fingerprint, outcome, and error types plus fixture fault tests.
2. Implement bounded inspection with `path_status` and no mutation.
3. Implement revalidated orphan-file deletion and serialized manifest merge.
4. Migrate the deferred window caller to consume exact outcomes and surface grouped diagnostics.
5. Remove the mutable-manifest `cleanup_orphans` API after all call sites and tests migrate.
6. Rollback can restore the former caller because no persisted format changes are introduced.

## Open Questions

None. Ambiguous evidence is always retained; cleanup optimization must not change that rule.
