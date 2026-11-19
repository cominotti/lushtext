# 2026-11-18: where a programme change's time went

Why the change-delivery rules of `adopt-faster-change-flow` exist, with the
measurements they were decided on. The rules are in
[`.agents/rules/parallel-agent-work.md`](../../.agents/rules/parallel-agent-work.md)
("Implementation slots and ordering", "Running a change", "Fast lane",
"Design revisions", "Stages after implementation", "Workflow agents",
"Measurements", "Merging") and
[`.agents/rules/verification-runs.md`](../../.agents/rules/verification-runs.md)
("Kani", "Timing campaigns"). This file states no rule.

## The analysis

A read-only analysis on 2026-11-18 asked whether a programme change could be
cut from about 5 agent-hours to 1-2 without lowering quality.

**Verdict.** Only a modest cut is possible for the median change: 5.2 h to
about 4.5 h from engineering fixes, about 4 h with the maintainer decisions
below. The floor is model work (a median 28 % of a change, about 1.4 h) plus
one run of each gate (30-40 min). What had grown was the tail and the smoke
lanes, not the median.

### Method and its limits

- Source: 233 agent transcripts (paired tool-use and tool-result
  timestamps), 25 `runs.log` files, lane artifacts in the worktrees, and 83
  merged pull requests. Set: 24 programme changes from 2026-11-05, each of at
  least 2 agent-hours; six atypical changes were excluded.
- **Measured:** wall time per agent, stage and tool call; model time (wall
  minus the union of tool intervals); lane durations; per-case timelines.
- **Estimated:** the split by category, from keywords in each command. Each
  share is ±30 %, and 8 % of the time could not be attributed.
- **Not measured:** time queued in `heavy.sh` admission, which is hidden in
  each command's duration.

### Per change

| Figure | Value |
| --- | --- |
| Agent-hours per change, median / mean | 5.2 h / 7.2 h |
| Range | 2.1 h to 26.1 h; 6 of 24 above 8 h |
| Spec / spec review, median | 28 min / 17 min |
| Implementation, all rounds, median / mean | 228 min / 320 min |
| Review and merge, median / mean | 15 min / 42 min |

| Category (estimated) | Pooled share | Minutes of the 312-min median change |
| --- | --- | --- |
| Model generating | 27.6 % | 86 |
| Tests | 16.0 % | 46 |
| Smoke lanes | 11.8 % | 31 |
| Timing measurements and waits on them | 9.8 % | 14 |
| `make check`, Clippy, commit hook, builds | 9.7 % | 20 |
| Kani, proof strength, mutants | 9.0 % | 21 |
| Unattributed waits | 8.0 % | 14 |
| GTK axiom samples | 5.6 % | 2 |
| CI waits | 0.8 % | 0 |

Lock and slot waits were 0.3 % of wall time. The formal-verification family
(Kani, proof strength, mutants, axiom samples) was 14.6 % pooled, a floor:
harness authoring sits inside model time. No before/after figure for Kani's
adoption (2026-09-23) exists, because earlier work went straight to `main`.

### Before and after 2026-11-14 (five slots, merge on local evidence)

| Figure | Before (n = 10) | After (n = 14) |
| --- | --- | --- |
| Agent-hours per change, median | 5.2 h | 5.5 h |
| Agent-hours per change, mean | 5.5 h | 8.4 h |
| Tool time, median | 3.64 h | 4.18 h |
| Model time, median | 1.36 h | 1.50 h |
| `git commit` with its hook, median / p90 | 1.2 / 2.2 min | 2.2 / 5.5 min |
| `make check`, median / p90 | 1.2 / 1.5 min | 1.8 / 3.1 min |
| Smoke trio (n = 1 / 2) | 11.0 min | 47.0 min |

- Contention cost the median change at most about 30 min (the 0.54 h rise
  in tool time), an upper bound confounded with the smoke wait below.
- Throughput rose far more than latency fell: 13 merges in 19 h.
- The mean rose because of four changes above 10 h (10.7, 13.8, 19.0 and
  26.1 h), which were design loops and measurement queues. For example,
  `slice-large-directory-collapse` spent 26 agent-hours over 93 calendar
  hours, its design passed revision R14, and a pre-registered gate missed by
  0.3 ms sent it back to design.
- The smoke trio grew because every headless session with media waited out a
  10 s timeout for a WirePlumber object the `policy` profile never publishes
  (about 27 of the 47 min, from PR #110).
- Under `MERGE-LOCK`, `make check` and the affected tests always reran, even
  after a no-op `origin/main` merge: about 28 min in `stream-replace-all`.
- `stream-replace-all` (#103) dispatched its two `kani.yml` budget runs one
  after the other in stage 4 and watched them from 12:04 to 13:16; the net
  block was about 20-25 min.
- Quiet windows were judged on the global 1-minute load. The coordinator
  reported desktop apps alone holding it at 5-6; the processes most often
  above 30 % CPU in the `top` snapshots were a game, an IDE toolbox (about
  150 %), Java, a browser and the file indexer.

### Ranked interventions

1. Fix the 10 s session wait (about 27 min per trio run).
2. Run smoke cases in parallel (from about 20 to 8-10 min per trio).
3. Skip the under-lock rerun when the merge left the tree unchanged.
4. Stamp `make check` by tree hash for the commit hook (about 8 min).
5. Dispatch Kani budget runs when implementation ends (20-65 min on changes
   that add or change a shard).
6. Scheduled quiet windows (calendar time only).
7. Instrument admission and per-gate minutes.

Interventions 1, 2 and 4 are engineering changes owned by
`fix-headless-session-media-wait` (1) and the smoke-gate-coherence change
(2 and 4). Interventions 3 and 5 are rules, added by
`adopt-faster-change-flow`.

### Weakest assumptions

- The category split is keyword-based (±30 %).
- The cause of the session wait was read from code and artifacts, not run.
- The `make test` and smoke before/after figures have one or two samples each.
- The contention figure is an upper bound, confounded with the smoke wait.
- Admission queueing is unmeasured.
- Some model time may be artifact churn (designs reaching revision 14).
- The sample is 13 days of one programme, mostly performance and
  infrastructure work.

## The maintainer's decisions, 2026-11-18

- **18:33.** Interventions 1-5 accepted. Decisions accepted: 1 (merge on a
  non-inferiority verdict, the improvement verdict as a follow-up), 2 (an
  independent decision after two design revisions), 3 (a fast lane), 4 (three
  implementation slots while rounds a change waits on are ready) and 6 (the
  quiet-window criterion). Decision 5 (steering desktop apps) rejected. The
  maintainer also asked that the decisions become permanent for future
  sessions, inside the repository, and that the adjustments be reviewed.
- **18:36.** Decision 7 (widget-test edits no longer voiding the three smokes)
  approved with conditions: the three fingerprint lists only, with a parity
  test; the static accessibility check stays; the residual risk declared. It
  belongs to the smoke-gate-coherence change.
- **18:50-19:25.** The stages after implementation (`/simplify`, parallel
  findings-only lenses, one independent apply agent), the fan-out criterion,
  the main-checkout guard and the outward-writes limit, from probes of how
  workflow agents run the `code-review` and `simplify` skills. A forked
  `code-review --fix` edited the main checkout in one probe. An independent
  Fable review of the versioned workflow template was made a merge condition
  of `adopt-faster-change-flow`.
- **19:39.** The load rule was found too strict. In the #109 G1 window, 3 of
  4 tries in 2 h 40 min exited 75 behind our own heavy work, and the fourth
  was voided at a pre-drain load of 5.69. Three levels were approved:
  1. **Drain first**, judged on busy CPUs instead of the 1-minute load, with
     the bar kept at 4. In force with `adopt-faster-change-flow`.
  2. **A calibration under outside load**, queued by the coordinator: at most
     six rounds of an earlier attribution command compared with quiet
     references, with a pre-registered direction reading (load never makes a
     round look better) and a magnitude reading.
  3. **For absolute budget gates, a pass under load would count and a failure
     under load would be inconclusive.** Conditional on level 2's direction
     reading; not in any rule. Comparative and improvement verdicts stay
     quiet-only.

  Stage 1 of `adopt-faster-change-flow` had proposed calibrating a
  per-reference-CPU criterion instead. It could not work: the reference set
  8-23 is the toolbox's whole affinity, and the sample showed about 50 % busy
  on each reference CPU.

- **2026-11-19, 00:58.** The maintainer closed level 2 as **not
  established** and turned level 3 **off** for good; level 1, drain first,
  stays as written. The calibration's own record (`MEASURE-QUEUE.md`,
  "Calibration: G0 rounds under outside load") shows why: quiet rounds
  against quiet rounds already differed by up to 1.79x on the worst frame and
  1.95x on the worst step, far outside the -5 % direction band the reading
  was registered with, and its two tries produced no contended round at all,
  because the drain removed the outside load before the round ran. The rules
  keep no dormant level-3 text.
