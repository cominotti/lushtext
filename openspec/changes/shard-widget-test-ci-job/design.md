## Context

`.github/workflows/ci.yml` job `widget-tests` runs
`./scripts/run-widget-tests.sh --headless --retries 1`, one private
`mutter --headless` session running all 1,292 registered widget tests, each in
its own child process (`gtk-lush-proof-harness`). The job cap is 30 minutes and
`scripts/check-workflow-timeouts.py` forbids raising it.

Timing reconstructed from the per-line timestamps of three successful `main`
runs (35878201619, 35909749851, 35920527262) shows 13–25 minutes spent inside
the harness plus about 2–3 minutes of container setup and test-binary build.
Per-module harness minutes (worst of the three): `window` 9.4, `editor_page`
10.0, `workspace_tree_virtualization` 2.3, every other module under 0.6. Three
tests dominate and vary widely between runs (the runner is noisy):

| Test | Runner seconds (three runs) |
|---|---|
| `window::test_document_sized_local_history_restore_and_undo_are_bounded_and_exact` | 222, 124, 305 |
| `editor_page::test_minimap_mid_scan_edit_cancels_stale_generation_and_publishes_latest` | 180, 30, 302 |
| `editor_page::test_minimap_long_line_warning_scan_slices_large_many_short_buffer` | 135, 36, 175 |

Their document sizes are the thresholds under test; they are not shrunk.

The harness already supports selection: positional filters are substrings,
`--exact` makes them exact names, `--skip` excludes, `--list` lists without a
compositor. Registration is deterministic from source: `crates/lushtext/build.rs`
registers `<file stem>::<fn name>` for every `#[test]` line in
`tests/widget/*.rs` except `common.rs`. `scripts/kani-shards.py` is the
established pattern for a checked shard table feeding a CI matrix.

## Goals / Non-Goals

**Goals:**
- Every shard job finishes with a measured step wall time ≤ 20 minutes.
- One table; a gate that catches unassigned, doubly assigned, and stale
  entries statically, and a CI-side proof that the shards ran exactly the full
  compiled suite.
- Per-shard measurements recorded from real runner runs, reported per job.
- Local full-suite commands unchanged.

**Non-Goals:**
- Changing the harness's selection, retry, or FLAKY semantics.
- Making the three slow tests faster or smaller.
- Parallelising tests inside one compositor session.

## Decisions

**D1. Three shards, by module with explicit overrides.** `window`
(`window::*`), `editor-page` (`editor_page::*` except the long-line minimap
test), and `surfaces` (every other module, plus that test by explicit name).
Simulated with the worst observed per-test times this gives about 9.4, 7.1,
and 8.5 harness minutes. Each of the three threshold-sized tests lands in a
different shard, which is what bounds the worst case. Two shards would put
about 12–13 worst-case minutes in each, uncomfortably close to the margin once
setup is added; four adds a fourth full build for little gain.
*Alternative:* greedy bin-packing individual tests by measured time. Rejected:
unstable membership whenever a timing moves, and unreadable. Modules keep
ownership obvious; new tests in an existing module join its shard with no
table edit.

**D2. Precedence rule for ownership.** A test's owners are the shards naming
it explicitly; if none, the shards listing its module. Exactly one owner is
required. A module in two shards, a name in two shards, an explicit name whose
module is already owned by the same shard (redundant), and an entry that
matches nothing are all failures. There is deliberately no catch-all shard: a
new widget module file must be assigned, so it cannot silently join an
unmeasured bucket.

**D3. Static discovery in the gate, runtime cross-check in CI.**
`check-widget-shards` reimplements `build.rs`'s `extract_test_functions` rule
in Python (no build needed, so `make check` stays fast). Drift between the two
is caught at runtime: every shard job runs the binary with `--list --format
terse` and fails if the set differs from the static discovery. The self-test
pins the extraction rule on a fixture.

**D4. Selection by exact names.** The runner passes `--exact` plus the shard's
names after `--`. Substring module filters were rejected: `window` is a
substring of `window_preview::…` names, and `--skip` lists would need to
mirror the table in reverse. Argument size is ~600 names × ~90 bytes, far
below `ARG_MAX`; the harness forwards the args to each child, which ignores
them because the child selects its test by environment.

**D5. Count assertion.** The runner parses the harness's `running N tests`
line(s) and fails unless every one equals the shard's expected count (a
whole-run retry prints the line again). Together with D2's partition over the
same set that D3 proves equals the binary's list, the shard counts sum to the
full suite.

**D6. Measurement.** `widget-shards.py run <shard> --measure JSON` records the
step wall time (build + list + tests), exit status, selected count, and
per-test durations derived from the completion timestamp of each
`test NAME ... ok` line (the harness prints the name and result on one line,
so a test's duration is the gap since the previous completion). Written to
JSON, uploaded as `widget-measure-<shard>`, and appended to the job summary.
Every CI shard job runs in measurement mode.

**D7. Budget margin: 20 minutes of recorded step wall time.** The job cap is
30; setup before the step measured under 3 minutes, and the runner is noisy
(the same test varied 10× across runs). The table records the maximum over at
least two measured CI runs; `check` fails above 20 minutes or when
unmeasured. `run` never refuses an over-margin shard, so it stays measurable.

**D8. CI shape.** A `widget-shards` job (`Widget Shard Table`, 5-minute
timeout, no container needed: plain Python) runs `github-outputs`, which runs
the full check and prints `shards=[…]`. `widget-tests` becomes a matrix over
that output named `Widget Tests (${{ matrix.shard }})`, `fail-fast: false`,
each with `timeout-minutes: 30`, the same container, dependencies, and rust
cache key as before. `--self-test` of the warning classifier stays in each
shard job.

## Risks / Trade-offs

- [Runner noise pushes a shard over 20 minutes] → the recorded maximum and the
  gate make it visible; the response is moving a module or an explicit test,
  not raising the cap.
- [Three builds instead of one] → the shared rust cache keeps each build to
  about 1–2 minutes; total runner minutes grow slightly while wall time drops.
- [Static extraction drifts from `build.rs`] → the per-job `--list` comparison
  fails the run; the self-test pins the rule.
- [Check names change] → only `Kani Proof Harnesses (widgets-geometry)` is a
  required check (ruleset 23911547), so nothing blocks merge.

## Migration Plan

Land on a branch, measure with at least two CI runs on the draft PR, record
the maxima in the table, then merge. Rollback is reverting the commit; the old
single job has no state.

## Open Questions

None.

## Deviations

- **Provisional bootstrap budgets.** D7 requires every shard to carry a CI
  runner measurement before `github-outputs` exports the matrix, but the
  sharded matrix had never run, so it could not produce one. The first commit
  recorded provisional figures (window 11.5, editor-page 9.1, surfaces 10.5
  minutes), labelled as such in `measured_in`: the worst per-module harness
  minutes reconstructed from the per-line timestamps of the unsharded runs
  35878201619, 35909749851, and 35920527262, plus 2 minutes for the build.
  They were replaced by the real shard measurements (below) before merge; the
  gate itself was never relaxed.
- **Durations attributed by position, not by printed name.** D6 planned to
  read each test's name from its `test NAME ... ok` line. The first local run
  recovered only 119 of 148 names and credited the minimap mid-scan test's 51 s
  to its neighbour: when a child prints on the same line as the harness's
  `test NAME ... ` prefix, the result lands on a later bare `ok` line, and
  `scripts/run-widget-tests.sh`'s benign-noise filter can drop the prefix line
  entirely. Since the harness runs the selected tests one at a time in list
  order, `widget-shards.py` now runs each shard in the binary's `--list` order
  and assigns the i-th result line (prefixed or bare) to the i-th test; if the
  result count differs from the shard size, the durations are recorded as
  unattributed (`durations_attributed: false`) instead of guessed. The shard
  count assertion (D5) never depended on this and is unchanged. The relay also
  flushes every line, because CI's stdout is a pipe and a cancelled job would
  otherwise lose its visible progress.
- **Measured budgets** (widget step wall time, including the build; the
  larger of two runs, rounded up):

  | Shard | Tests | Run 35946276130 | Run 35947171706 | Recorded |
  |---|---|---|---|---|
  | `window` | 376 | 10.23 min | 11.10 min | 11.1 |
  | `editor-page` | 148 | 8.75 min | 7.01 min | 8.8 |
  | `surfaces` | 768 | 9.03 min | 6.55 min | 9.1 |

  Both runs: 376 + 148 + 768 = 1,292 selected, which is the binary's full
  `--list`. The longest job took 12.0 minutes end to end, compared with the
  15–30 minutes of the single job.
