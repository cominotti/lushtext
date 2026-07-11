## Why

Draft orphan cleanup returns `Result`, but currently suppresses directory-scan and deletion errors, counts failed deletions as successful cleanup, and can interpret metadata failures as ordinary absence. That makes recovery diagnostics overstate success and can remove manifest evidence without confirming the corresponding filesystem action. Cleanup needs an honest, conservative result contract before further draft-persistence work builds on it.

## What Changes

- Introduce a typed draft-cleanup outcome that distinguishes confirmed removals, absent entries, retained entries, scan failures, metadata failures, and delete failures.
- Use recovery-aware path status so missing files are distinguishable from permission and I/O errors.
- Remove manifest entries and increment cleanup counts only after the corresponding decision is confirmed safe.
- Keep ambiguous or failed cleanup retryable and visible without deleting recovery evidence or blocking unaffected recovery work.
- Split cleanup planning from filesystem mutation so the service contract is straightforward to test and concurrent manifest additions can be merged safely.
- Add deterministic filesystem fault, partial-directory, concurrent-update, and restart tests.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `draft-session-recovery`: Makes orphan cleanup conservative, diagnostic, retryable, and truthful about which recovery artifacts were actually removed.

## Impact

- Affects `crates/lushtext-core/src/services/draft_service.rs`, startup draft reconciliation, recovery diagnostics, and their tests.
- Does not change valid draft IDs, on-disk draft bodies, or the public v1 manifest format.
- Is the first implementation step in the portfolio because `pipeline-draft-persistence` should consume the hardened cleanup outcome rather than the current ambiguous contract.
