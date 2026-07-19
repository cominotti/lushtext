# Remaining Quality-Gap Scale Evidence

This note records the production limits and regression evidence for the final
reviewed safety and boundedness closeout. It covers incomplete draft repair,
session restore, dense-line Replace All construction, per-store workspace
scans, weighted plain-data disposal, sliced minimap analysis, note-score
pruning, and the earlier Notes-browser, local-history, watcher, and child-cache
scale fixtures.

## Closeout limits and ownership rationale

| Workflow | Production bound | Ownership rationale |
| --- | --- | --- |
| Draft manifest repair | 256 directory entries per page; 2,048 draft bodies, 512 KiB reconstructed metadata, and four diagnostics per attempt | A page is temporary traversal state, while the aggregate caps are the largest state that can become one existing manifest. Reaching any aggregate or classification ambiguity preserves bodies and withholds replacement and cleanup authority. |
| Session restore | Four page creations per GTK turn and two concurrent file-plan permits per window | Pending tabs remain compact descriptors, so progressive admission never drops session intent. Four bounds widget/template work per turn; two bounds strong file-planning ownership independently of the global worker queue. Every permit terminal releases capacity, and one final rebuild publishes tab-derived projections. |
| Replace All construction | 10 MiB per source file, at most 10,000 accepted replacements, and 64 MiB aggregate durable undo bytes | Streaming line discovery retains edit records in proportion to accepted replacements, not source-line count. Output and undo payloads remain charged separately because both are unavoidable owned bytes. |
| Materialized workspace scan | One admitted worker and one replaceable latest compact request per child store | Queued state owns only weak store/lifetime identity and path/options. Strong store ownership and the mirror snapshot are captured only at worker admission, so generic worker-queue depth cannot amplify per-store payloads. |
| Plain-data disposal | Two workers, eight reserved drop slots, and 128 MiB ordinary retained weight; one overweight job may run exclusively | Count and byte admission cover different payload shapes. Document-sized values reserve before GTK publication and keep the permit through replaceable UI ownership, so final destruction is an off-main handoff rather than a rejected large payload on GTK. Capacity pressure retains one latest compact request and one retry source. Exclusive overweight progress avoids a bounded-policy deadlock. |
| Minimap analysis | 32,768 live-buffer characters per GTK turn and at most 2,000 retained long-line identities | The cursor and accumulator belong to one editor lifetime and generation. The slice is small enough to force 69 turns on the 2.23 MiB many-short-line fixture while allowing GTK heartbeat evidence; accepted cache state removes repeat full-buffer scans. |
| Note scoring | At most 4 KiB enters `nucleo` per field; oversized matching fields contribute zero; each source selector retains only its configured top results | One `nucleo` atom returns `u16`, so `u16::MAX` is a sound small-field contribution ceiling. A body is skipped only after metadata established eligibility and its ceiling cannot improve the row maximum. Body-only matches still take cancellable full-text eligibility. |

These values are policy constants rather than elapsed-time thresholds. The
fixtures deliberately exceed one page, turn, permit set, pending slot, or
analysis slice so the asserted high-water fields prove the relevant admission
boundary on every host.

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
- Command-palette file-index retirement reserves the shared plain-data disposal
  lane on the indexing worker before full or incremental results cross onto GTK.
  Superseded indexes keep their permit through final worker destruction; test
  hooks expose only scalar retirement counts.
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
bytes/full-scan candidates, optimized note-scoring counts and final-query
equivalence, Notes and preview coordinator ownership, preview bytes/slices and
retained payloads, raw watcher event and retained-path counts, per-store scan
ownership, terminal cache input/operation counts, and 2.23 MiB minimap slice
counts. The benchmark measures the 10,000-row no-match Notes query, a
10,000-row metadata-dominated scorer, a 4 MiB preview read, a 10,000-row
terminal cache replacement, and the pure minimap accumulator separately.

The smoke summary preserves one direct line for each closeout fixture:

| Fixture | Summary evidence |
| --- | --- |
| Two-startup draft repair and cleanup traversal | `draft-repair-closeout-evidence`: repair pages/page cap, raw directory visits, classified entries, bodies, metadata bytes, completeness, cleanup pages, and survivors |
| Mixed 12-tab session restore and cancellation | `session-restore-bound-evidence` and `session-restore-cancellation-evidence`: pages per turn, GTK turns, permits, pending descriptors, terminals, source ownership, and one projection publication |
| 10 MiB dense short-line Replace All | `replace-all-streaming-evidence`: source lines, accepted replacements, retained edit records/bytes, output bytes, and undo bytes |
| Rapid refreshes plus six-section aggregate scan pressure | `workspace-scan-flight-evidence` and `workspace-scan-aggregate-evidence`: per-store active/latest high water, weak pending ownership, mirror captures, cancellations, stale terminals, accepted publication, process-wide task high water, admission waiters, GTK heartbeat, and terminal drain |
| Four-producer aggregate disposal pressure | `plain-disposal-pressure-evidence`: immediate small-job full results, pending/retry high water, workers, reserved slots, bytes, GTK heartbeat, terminals, teardown cancellation, final drain, and a pre-admitted nested owner whose final destructor runs off GTK |
| 2.23 MiB many-short-line minimap plus mid-scan edit | `minimap-analysis-evidence` and `minimap-cancellation-evidence`: characters per slice, turns, GTK heartbeat, generation/cache identity, cancellation, and terminal publication |
| 10,000 metadata-dominated note rows | `note-scoring-pruning-evidence`: candidates, bodies examined/pruned, retained results, active/latest ownership, cancellation, and final-query equivalence |

`make performance-smoke` runs the integration and headless proofs above in
addition to the capped Notes query, sliced Unicode local-history preview, and
linear accepted workspace reconciliation. Runtime evidence contains counts and
booleans only; it exposes no draft body, note text, snapshot identifier, or
filesystem path.

## Calibration

The portable gates are ownership and retained-state limits, exact final UI
state, and the eight-operations-per-input-row cache tripwire. Wall-clock values
are host calibration only and are recorded with the smoke environment artifact.
The 10,000-row Notes and cache fixtures represent the production admission/tree
scale; the 4 MiB preview forces sixteen or more 256 KiB installation slices
without making the lightweight smoke lane depend on the maximum supported file
size.

Run the complete closeout artifact pass with:

```sh
scripts/run-performance-smoke.sh \
  --artifact-dir build/smoke/performance/close-reviewed-safety-and-boundedness-gaps \
  --filter 'quality_gap_scale replace_undo_workflows'
```

On 2026-07-14, an x86_64 Fedora Toolbx run using rustc/cargo 1.96.0 and the
Criterion bench profile (10 samples, one-second warm-up and measurement) found:

- the 10,000-row no-match Notes query examined all 10,000 candidates, retained
  2,860,000 searchable bytes, and completed in about 4.17-4.38 ms;
- the 4,194,317-byte local-history preview planned 17 slices and loaded in about
  497-536 us while coordinator evidence stayed at one active and one pending;
- 1,536 raw watcher events retained 512 unique paths; and
- the 10,000-row-to-10,000-row terminal cache replacement performed 100,000
  plain operations over 20,000 input rows and completed in about 11.4-13.5 ms.

On 2026-07-15, the added optimized Criterion fixtures measured the 2,231,000
character minimap accumulator at about 1.60-1.62 ms over 69 bounded slices and
the 10,000-row metadata-dominated note scorer at about 1.61-1.62 ms while
examining zero bodies, safely pruning 10,000, and retaining 500 results. These
times are calibration only; the direct counters and equivalence assertions are
the regression contract.
