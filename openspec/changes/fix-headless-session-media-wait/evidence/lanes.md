# Implementation evidence (stage 3, 2026-11-19)

Host `sophia`, Fedora 45 toolbox, PipeWire 1.6.11, WirePlumber 0.5.19,
mutter 51.1. Every figure is **non-verdict** wall time, run through
`scripts/heavy.sh` on the shared host with the 1-minute load recorded beside
it. Raw logs are in `.claude/handoffs/fix-headless-session-media-wait/`
(`runs.log`, `failing-first.log`, `task-2.4-sessions.txt`,
`task-2.5-expect-dirty.txt`, `capture-final/`, `seeded/`, `seeded-6.1/`,
`lanes/`).

## Failing first (tasks 1.3, 3, 6.1)

- Task 1.3: against stubs (`wireplumber_policy_ready` false,
  `wait_for_wireplumber_policy` an immediate miss, `wireplumber_miss_line`
  empty), the four new tests failed and the 33 others passed. With the
  implementation all 54 crate tests pass, and Clippy (`-D warnings`, all
  targets and features) is clean.
- Task 3.1: with `wireplumber.profiles = { policy = { policy.standard =
  disabled } }` appended to `wireplumber-quiet.conf` (WirePlumber accepted
  it, so the fallback seed was not needed), a `screencast` session around a
  marker command exited 101 after 10.36 s with
  `… within 10 s (session.services never listed policy.linking.standard; last pw-dump in <log-dir>/pw-dump-not-ready.json)`.
  The command never ran (no marker file). The saved dump's WirePlumber client
  has `"session.services": "[]"`, the empty-list form (fixture covered). The
  same session without `--log-dir` exited 101 after 10.55 s, and its line
  ended with `rerun with --log-dir to keep the last pw-dump`. No daemon of
  these sessions was left running.
- Task 3.2: with the seed in place, one case of each lane, artifacts under
  the handoff `seeded/` directory, every one exit 1 and 0 `SKIP:` lines, the
  D3 line in the case's session log:
  - visual `--case short-layout`;
  - accessibility `--case workspace-tree`;
  - visual-geometry `--case-filter command-palette-overlay--constrained--force-light`:
    case status `failed`, exit code 101, no `session-retry.json`;
  - command-palette notes.
- Task 6.1: a CSS animation on `headerbar` (red to blue, 3 s) made both
  `command-palette-overlay` cases fail their protected region `header-bar`
  (59,220 of 59,220 pixels differ; `status-bar` passed). Run through the
  `make visual-geometry-smoke` runner with that case filter. A wrong
  accessible name (`Refresh Workspace Folderz`) made
  `run-accessibility-smoke.sh --case workspace-tree` fail with
  `accessibility anchor 'Refresh Workspace Folders' missing`. Both seeds were
  reverted and the binaries rebuilt before the green runs.

## The miss line is never a skip (task 4.1)

The four forms of the D3 line (`last pw-dump in <path>`,
`no pw-dump succeeded`, `rerun with --log-dir to keep the last pw-dump`,
`the last pw-dump could not be saved to <path>`) match none of:

- the skip regex of `scripts/run-visual-smoke.sh` (0 of 4);
- the skip regex of `scripts/run-command-palette-notes-smoke.sh` (0 of 4);
- the AT-SPI startup retry texts of `crates/cargo-gtk-proof/src/live.rs` (0 of
  4), whose `session_status` maps exit 101 to `failed`.

`scripts/run-accessibility-smoke.sh`, `scripts/crash-recovery-smoke-driver.py`,
`scripts/visual-geometry-smoke.py` and
`.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py`
decide host support before the session and fail on a nonzero session status.

## Sessions by hand (tasks 2.4, 2.5)

`gtk-lush-session session --profile screencast [--media off] -- true`,
private XDG homes, alternating, load 3.63 → 4.22:

| Run | media quiet | media off |
| --- | --- | --- |
| 1 | 0.82 s | 0.54 s |
| 2 | 0.82 s | 0.59 s |
| 3 | 0.77 s | 0.59 s |

All six exited 0 with a clean guard. Before: 11.00-11.37 s quiet,
0.78-1.06 s off (`baseline.md` §1).

`--expect-dirty -- true`, three runs, each in its own owned runtime
directory, load 5.28 → 5.42: each exited 0 with
`the bare configuration was caught (rtkit, bluez, hardware-device, module-rt)`
in 1.21-1.48 s. Each run reported 13 `bluez`, 4 `hardware-device`,
16 `module-rt` and 30-33 `rtkit` lines plus the `config` line. D6's fallback
was not needed.

## First-try link (task 2.6)

`gtk-lush-session run --profile screencast --monitor 640x480 --log-dir <dir>
-- /usr/bin/python3 capture-probe.py <png>`, no experiment patch, 10 runs
each:

| Helper | Load | link + first buffer | session wall |
| --- | --- | --- | --- |
| G1 (`6c7cd399` tree) | 5.74 → 5.78 | 0.087-0.237 s | 1.59-7.07 s |
| final, with D9 (`1531e284`) | 5.54 → 5.55 | 0.087-0.107 s | 1.53-1.61 s |

Every run exited 0, linked and delivered its first buffer on its single
`pipewiresrc` attempt within 1 s, wrote a 640 x 480 RGBA PNG and had a clean
guard. In the G1 series, run 7's wall (7.07 s, capture 5.87 s after start) is
the probe's own race, before `pipewiresrc` starts: its capture thread can
lose the `PipeWireStreamAdded` quit to the main thread and then waits out its
5 s timeout with the node already known. Its link took 0.237 s.

## A pre-existing blocker found by the lane proof (task 2.7, design D9)

The first round of task 6.2 (three lanes streamed concurrently in this one
worktree, on `6c7cd399`) failed visual run 4, case `workspace-refresh`:
`UNSUPPORTED-HOST: GTK_LUSH_SESSION_BIN=…/target/debug/gtk-lush-session (deleted) is not executable`.
That was 1 nested session in about 470. Before it the round had 10 green
runs (visual-geometry 1-2, accessibility 1-3, visual 1-3, plus the
in-flight ones), and none of them counts toward the final proof.

- **Reproducer** (`race-repro.sh` in the handoff directory): a private copy
  of the helper re-uplifted the way cargo does it (remove, then hard link)
  every 2 ms, while each session runs a nested
  `scripts/headless-session check-tools` through the exported variable.
  Before the fix: 20 of 20 failed with exit 77.
- **Unit test** `the_exported_session_bin_names_the_file_after_a_rebuild_replaced_it`:
  failed against a stub, passes with `exported_session_bin`.
- **After the helper half alone:** 39 of 40 passed. The one failure was the
  path briefly absent between cargo's remove and its link.
- **After the shim's retry as well:** 100 of 100 passed.

Tasks 2.3 (55 crate tests, Clippy) and 2.6 were rerun on the fixed helper,
and task 6.2 was restarted from zero. Tasks 2.4, 2.5, 3.x, 4.1 and 6.1 do
not depend on the exported path.

## Lane runs on the final tree (task 6.2)

`1531e284` (G1 plus D9). Each run is one `heavy.sh` call. The three lanes ran
**concurrently** in this worktree (visual-geometry; accessibility; visual
then crash-recovery then command-palette notes), which is also the D9 race's
own trigger. The 2026-11-18 baseline ran its three lanes one after another.

- `lane` is timed from memory admission: `/usr/bin/time` inside
  `heavy.sh`.
- `heavy wall` adds `heavy.sh`'s wait for memory beside other agents' work.
  Other worktrees held up to 44 of the slice's 48 GiB during these runs.

A run counts as bad if any of these holds:

- it exits nonzero;
- it prints a `FLAKY`, `FLAKY-SHARD:`, `OOM-KILL:`, `SESSION-HYGIENE:` or
  `SKIP:` line;
- a `skip.txt` or `skip-reason.txt` exists in the lane's artifacts;
- the lane's summary has a skipped or unsupported status.

Every run was green on all four checks. The results are in
`lanes-final/results.txt`, with each run's log and a copy of its summary.

| Lane | Runs green | Lane mean (min-max) | Heavy wall mean (max) | 1-min load | Baseline 2026-11-18 | Before PR #110 |
| --- | --- | --- | --- | --- | --- | --- |
| `make visual-geometry-smoke` | 10 / 10 | 7.13 min (7.03-7.29) | 8.86 min (12.90) | 2.35-9.10 | 24.2 min | 7.1 min |
| `make accessibility-smoke` | 10 / 10 | 3.17 min (2.98-3.43) | 5.62 min (9.28) | 3.71-7.78 | 13.0 min | 2.4 min |
| `make visual-smoke` | 10 / 10 | 2.32 min (2.20-2.76) | 4.90 min (10.22) | 3.65-13.63 | 9.8 min | 1.5 min |
| `make crash-recovery-smoke` | 1 / 1 | 0.56 min | 3.32 min | 7.27-8.95 | | |
| `make command-palette-notes-smoke` | 1 / 1 | 0.08 min | 0.59 min | 5.89-7.27 | | |

The trio's lane time falls from 47.0 min to 12.6 min, close to design D8's
expected 14 min (8 + 3.5 + 2.5).

The last run of each lane is the one left under `build/smoke/` (task 6.4):

- visual-geometry: unfiltered (`case_filter: null`), 99 cases launched and
  passed. Its one `not-run` status is the Python oracle's parity block,
  `diagnostic-only` in the default mode;
- accessibility and visual: passed. The accessibility warnings are
  `allowlisted`, with 0 unexpected.

## Comparison with the baseline (task 6.3)

Every lane is within D8's expectation. The visual lane's mean, 2.32 min, is
below the 3 min line D8 set for a remaining cause, so no `NEW CHANGE
PROPOSED` line is owed. The lanes stay about 0.4-0.8 min above their
2026-11-06 figures. Those figures are a single run each, and they were taken
before PR #110 gave every session its private system bus and monitors. The
new figures also come from concurrent streams. Neither gap is attributed
further here.
