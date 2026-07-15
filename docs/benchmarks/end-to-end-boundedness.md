# End-to-End Boundedness

This note records the constants, layered proof surfaces, and local calibration
for palette source construction, whole-buffer replacement, draft cleanup
continuation, and broad workspace-tree reconciliation.

## Production bounds

- File indexing retains at most 100,000 files. Each directory scan retains only
  the remaining capacity, checks cooperative cancellation, canonicalizes through
  the filesystem boundary, and keeps one active plus one latest compact request.
- Palette notes retain at most 10,000 entries and 64 MiB of aggregate searchable
  UTF-8 text. Sidecars are loaded and admitted in deterministic source order;
  rejected bodies are released before the next load. Note refresh ownership is
  also one active plus one latest request.
- Whole-buffer replacement is direct only when both the conservative existing
  byte charge and incoming text are at most 1 MiB. Sliced work clears at most
  65,536 characters and inserts at most 262,144 UTF-8 bytes per GTK turn.
- Draft cleanup inspects at most 2,048 manifest entries and 2,048 lexicographic
  directory rows per pass. Its optional cursor is committed only with the
  accepted durable manifest and survives restart.
- Workspace-tree reconciliation computes one plain prefix/middle/suffix plan.
  Changed ranges above the direct threshold are applied in at most 256 changed
  rows per GTK turn. Selection, expansion, caches, watcher targets, and
  `workspace-refresh-complete` finalize only for the current terminal plan.

## Layered evidence

Run the GTK-free Criterion group with:

```sh
cargo bench -p lushtext-core --bench benchmarks end_to_end_boundedness -- --noplot
```

The group emits `end-to-end-boundedness-evidence` with retained file/note
counts, searchable bytes, deterministic cancellation progress, coordinator
ownership, canonical pre-top-k examination, cleanup page size, reconciliation
range size, and replacement slice constants. `make performance-smoke` includes
the same group as a coarse regression tripwire.

Plain and service tests additionally cover canonical aliases, traversal faults,
note truncation reasons, awkward Unicode boundaries, cleanup restart/churn and
repeated faults, plus 10,000-row prefix and middle reconciliation. Headless GTK
tests cover eviction, eager/lazy draft recovery, local-history restore and undo,
save formatting, stale/disposed replacement terminals, per-turn main-loop
progress, projection guards, tree supersession, selection/expansion survival,
disposal, and exact readiness release. Buffer-replacement terminal metrics
record slice count, cleared characters, inserted bytes, and a peak of one
retained replacement body; final widget assertions compare the accepted Unicode
body and workflow state exactly.

No automation member, snapshot field, predicate name, or blocker name changed.
The existing `workspace-refresh-complete` predicate now remains blocked while a
current child-store reconciliation source exists, as documented in
`docs/automation.md` and `docs/automation-reference.md`.

## Local calibration

On 2026-07-14, an x86_64 Fedora Toolbx run using rustc/cargo 1.96.0 and the
Criterion bench profile (10 samples per case) recorded:

- a flat 10,000-entry directory retained 10,000 scan rows and produced 5,000
  indexed files in about 19.46 ms;
- the 10,000-entry note budget completed in about 2.65 ms, while the byte-budget
  fixture retained 65,014,192 searchable bytes in about 25.44 ms;
- deterministic note cancellation stopped after exactly 256 admitted rows in
  about 64.5 us;
- canonical exclusion before a top-one result examined two candidates and took
  about 65.9 ns;
- a 2,048-row page selected from 10,000 directory entries in about 3.57 ms;
- the 10,000-row middle reconciliation plan (5,000 removed and 5,000 inserted)
  computed in about 381 us.

These timings calibrate gross regressions on this host; the retained-state,
generation, terminal, and per-turn bounds are the portable acceptance criteria.
