# Lane case 4.3: `scripts/heavy.sh --envelope 16G -- make test`

Launched with the worktree's `scripts/heavy.sh`, detached (`setsid`), from the
branch at `10fa0162` plus the uncommitted group 4 Makefile and `heavy.sh`
changes. Two runs: run 1 found that a clean envelope printed no summary (the
`envelope:` line lives in `heavy.sh`'s private report file), so `heavy.sh` now
prints `heavy: envelope <sub-slice>: peak <MiB> MiB, oom_kill 0`, and run 2
repeated the lane with that change and recorded the exit status. A sampler
recorded the units and the sub-slice every 60 s
(`.claude/handoffs/close-heavy-admission-window/run1-lane-make-test.samples`,
`lane-make-test.samples`).

## Units during run 1 (first sample, 22:38:02, load 7.36)

```
lushtext-cap-862173-1794965880704280123.scope           loaded active running [systemd-run] /usr/bin/bash /var/home/danilo/Workspace/github/cominotti/lushtext/.claude/worktrees/close-heavy-admission-w
lushtext-envroot-862134-1794965880704280123.scope       loaded active running [systemd-run] /usr/bin/bash /var/home/danilo/Workspace/github/cominotti/lushtext/.claude/worktrees/close-heavy-admission-w
lushtext-envsentinel-862173-1794965880704280123.scope   loaded active running [systemd-run] /usr/bin/tail --pid=862173 -s 0.2 -f /dev/null
user-lushtext-heavy-env862173t1794965880704280123.slice loaded active active  Slice /user/lushtext/heavy/env862173t1794965880704280123
sub-slice user-lushtext-heavy-env862173t1794965880704280123.slice: memory.max 17179869184 memory.current 868495360 peak 929222656
  lushtext-envroot-862134-1794965880704280123.scope: memory.max max current 868421632
  lushtext-envsentinel-862173-1794965880704280123.scope: memory.max max current 212992
=== 2026-11-17T22:39:02-03:00 load 12.60 8.88 7.36
=== 2026-11-17T22:40:02-03:00 load 12.32 9.56 7.69
```

The placeholder `lushtext-cap-<P>-<ns>.scope` sits directly in
`user-lushtext-heavy.slice`; the root and sentinel scopes sit in the
sub-slice with no limit of their own; the sub-slice has `memory.max` 16 GiB.
Later samples show the widget build's own `lushtext-cap-*` scope (16G) and
then one 2G scope per shard inside the sub-slice:

```
lushtext-cap-909496-1794965975398227403.scope: 2147483648
lushtext-cap-909498-1794965975469235099.scope: 2147483648
lushtext-cap-909500-1794965975326489497.scope: 2147483648
```

## Results

| | Run 1 | Run 2 |
| --- | --- | --- |
| Start, load | 2026-11-17T22:38:00-03:00, 7.30 | 2026-11-17T22:50:22-03:00, 25.85 |
| End, load | 2026-11-17T22:48:48-03:00, 23.97 | 2026-11-17T22:59:42-03:00, 5.47 |
| Exit status | not recorded (no `make: ***` line; every shard ok) | 0 |
| Non-widget tests | Summary [  47.086s] 2546 tests run: 2546 passed (1 slow), 12 skipped | Summary [  44.273s] 2546 tests run: 2546 passed (1 slow), 12 skipped |
| Widget shards (1696 tests) | window 426/426, editor-page 413/413, surfaces 857/857, 1 attempt each | window 426/426, editor-page 413/413, surfaces 857/857, 1 attempt each |
| `widget-memory:` (each shard's own scope) | [editor-page] 382 MiB oom 0;[window] 422 MiB oom 0;[surfaces] 1353 MiB oom 0; | [editor-page] 382 MiB oom 0;[window] 425 MiB oom 0;[surfaces] 1189 MiB oom 0; |
| FLAKY lines | 0 | 0 |
| Envelope peak, oom_kill | 8,182 MiB sampled (`memory.peak` 8579076096; the run built the worktree's widget binary cold); oom_kill 0 | `heavy: envelope user-lushtext-heavy-env1181072t1794966623109826034.slice: peak 1208 MiB, oom_kill 0` (warm build) |

## The documented size

The envelope's largest peak, 8.0 GiB with a cold widget build, fits 16G with
a margin of half, so `make test` keeps `--envelope 16G` (design D2,
"provisional": keep 16G if the peak fits with margin). The shards' peaks
(at most 1,353 MiB) stay inside their 2G scopes.

## Found by this lane

The sub-slice of a finished envelope stayed as an empty slice unit (no
process, `memory.max` reverted to `max`, no drop-in, only reclaimable page
cache charged to it), one per run until logout. It held no admission (the
count reads only `*.scope` children), but nothing stopped it. Now the
placeholder stops its sentinel and then the sub-slice when `cgroup.events`
reads `populated 0`, and the envelope sweep stops every envelope sub-slice of
the heavy slice whose placeholder is gone and that is unpopulated (one a
leftover daemon occupies stays until the daemon exits, since stopping it
would kill the daemon). Two self-test cases pin it, and a mutation that
removes either stop fails them. Lane case 4.4 saw the sweep stop both
sub-slices these runs left (`lane-mutants.md`).
