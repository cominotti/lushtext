# Remaining Quality-Gap Scale Evidence

This note records the production limits and regression evidence for the Notes
browser, local-history previews, raw workspace-watch ingress, terminal child
cache rebuilds, command-palette index retirement, and draft-cleanup retries.

## Production limits

- Browse Notes admits at most 10,000 rows and 64 MiB of aggregate searchable
  UTF-8 text. Each sidecar scan retains at most 10,000 candidates, recovery
  evidence retains at most 1,024 diagnostics, and the aggregate open-editor
  snapshot contributes at most 10,000 note/bookmark rows. Queries retain at
  most 500 ordered render indexes, check cancellation between rows and every
  1,024 Unicode scalars within large fields, and own one active worker plus one
  latest compact request.
- Local-history preview reads check cancellation between 64 KiB filesystem
  chunks. Bodies up to 1 MiB install directly; larger accepted UTF-8 bodies
  install in at most 256 KiB per GTK turn. The browser keeps Copy and Restore
  disabled until the current generation is complete and retains one accepted
  snapshot payload.
- Workspace-watch callbacks normalize individual raw events. The mailbox keeps
  at most 1,024 unique paths, one bounded diagnostic, and disconnect state.
  Overflow, ambiguous rename sequences, or producer lock contention latch one
  conservative full refresh without blocking the producer.
- Terminal child-cache publication belongs to the current scan token. The bulk
  reducer replaces old and new sibling evidence without per-row index-map
  scans; its calibrated unique-row fixture stays below eight plain operations
  per old-plus-new row.
- Command-palette file-index retirement uses the bounded palette worker lane for
  full, accepted-incremental, and rejected-incremental replacements. Test hooks
  expose only scalar last-owned retirement counts.
- Draft orphan cleanup waits 2 seconds after startup, schedules ordinary
  continuation after 30 seconds, and exponentially backs retryable failures up
  to 15 minutes. Each window owns at most one worker and one timer; scalar hooks
  report timer state, worker state, starts, and active high water.

## Criterion and headless evidence

Run the GTK-free scale group with:

```sh
cargo bench -p lushtext-core --bench benchmarks quality_gap_scale -- --noplot
```

The `quality-gap-scale-evidence` line records admitted Notes rows/searchable
bytes/full-scan candidates, Notes and preview coordinator ownership, preview
bytes/slices/retained payloads, raw watcher event and retained-path counts, and
terminal cache input/operation counts. The benchmark measures a 10,000-row
no-match Notes query, a 4 MiB preview read, and a 10,000-row terminal cache
replacement separately from reconciliation planning.

`make performance-smoke` also runs three headless proofs. They require main-loop
progress during a capped Notes query and a sliced Unicode local-history preview,
then assert the accepted superseding workspace reconciliation publishes a
linear terminal cache rebuild. Runtime evidence contains counts and booleans
only; it exposes no note text, snapshot identifier, or filesystem path.

## Calibration

The portable gates are ownership and retained-state limits, exact final UI
state, and the eight-operations-per-input-row cache tripwire. Wall-clock values
are host calibration only and are recorded with the smoke environment artifact.
The 10,000-row Notes and cache fixtures represent the production admission/tree
scale; the 4 MiB preview forces sixteen or more 256 KiB installation slices
without making the lightweight smoke lane depend on the maximum supported file
size.

On 2026-07-14, an x86_64 Fedora Toolbx run using rustc/cargo 1.96.0 and the
Criterion bench profile (10 samples, one-second warm-up and measurement) found:

- the 10,000-row no-match Notes query examined all 10,000 candidates, retained
  2,860,000 searchable bytes, and completed in about 4.17-4.38 ms;
- the 4,194,317-byte local-history preview planned 17 slices and loaded in about
  497-536 us while coordinator evidence stayed at one active and one pending;
- 1,536 raw watcher events retained 512 unique paths; and
- the 10,000-row-to-10,000-row terminal cache replacement performed 100,000
  plain operations over 20,000 input rows and completed in about 11.4-13.5 ms.
