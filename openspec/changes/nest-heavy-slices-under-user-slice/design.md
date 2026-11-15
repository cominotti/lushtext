## Context

`scripts/with-memory-cap.sh` starts every capped command with
`systemd-run --user --scope --slice=lushtext-heavy.slice`. It caps the slice at
the host-wide budget (`MemoryMax`, `MemorySwapMax=0`, set with `set-property
--runtime` on every call). It admits each scope under a lock
(`$XDG_RUNTIME_DIR/lushtext-heavy-admission.lock`), adding up the `memory.max`
of the scopes already in the slice. `scripts/heavy.sh` runs one command per call
through it. `widget-shards.py parallel`, `make kani`, `make proof-strength` and
`make mutants-*` call it directly. Only `with-memory-cap.sh` and `heavy.sh`'s
self-test spell the slice name. The rule, docs and spec text name it too.

CST (the host's CPU/cgroup tooling, `~/Workspace/github/cominotti/cst`,
`cpu-setup.sh` and `cpu/game-profile`) configures the user manager's inner
`user.slice` with `CPUWeight=30` and an `AllowedCPUs` set in
`~/.config/systemd/user/user.slice.d/50-cpu-setup.conf`. The set was 8-23 until
2026-11-15, when the maintainer asked CST to widen it to 0,2-6,8-23. While a game
runs CST sets `CPUWeight=idle` and `AllowedCPUs=12-23` with `set-property
--runtime`. Toolboxes (podman `libpod-*.scope`) run there. `app.slice`, where
the desktop and games run, keeps weight 100 and all CPUs.

`scripts/measure.sh` runs one timing command under the exclusive lock and sets
no affinity. `scripts/run-responsiveness-smoke.sh` caps the lane at
`MAX_CPUS=16`, the first 16 CPUs of the inherited affinity
(`responsiveness-smoke-driver.py affinity-list`), and records that list. Every
verdict so far ran with the caller pinned to 8-23 by `taskset`.

## Baseline (measured 2026-11-15, kernel 7.2.15-300.fc45, systemd 262.1)

Live tree (`/sys/fs/cgroup/user.slice/user-1000.slice/user@1000.service/`), read
at 00:39 after CST's widening:

| cgroup | `cpu.weight` | `cpuset.cpus` / `.effective` | `cgroup.subtree_control` |
| --- | --- | --- | --- |
| `user.slice` (inner) | 30 | 0,2-6,8-23 / 0,2-6,8-23 | cpuset cpu io memory pids |
| `app.slice` | 100 | — / 0-23 | cpuset cpu io memory pids |
| `lushtext.slice` | 100 | — / 0-23 | memory pids |
| `lushtext.slice/lushtext-heavy.slice` | (no cpu controller) | (none) | memory pids |

`lushtext.slice/cpu.stat` read `usage_usec 68751468578` at 00:20 and
`72099902016` at 00:39 (72.1k CPU-seconds since the user manager started). The
heavy slice held two scopes. The agent shell's own cgroup is
`user.slice/libpod-<id>.scope/container`, with `Cpus_allowed_list: 8-23` (its
`taskset`) and `cpuset.cpus.effective` 0,2-6,8-23. `nproc` printed 16.

Probes at 00:20, while the cpuset was still 8-23. Each was a `systemd-run --user
--scope -p MemoryMax=64M` of a few `/proc` reads, and the probe slices were
stopped afterwards with `systemctl --user stop`:

1. `--slice=user-lushtext-heavy-probe.slice` from the toolbox (affinity 8-23):
   path `…/user@1000.service/user.slice/user-lushtext.slice/user-lushtext-heavy.slice/user-lushtext-heavy-probe.slice/run-p1248259-i30586581.scope`.
   `user-lushtext.slice` had cpu.weight 100, cpuset.cpus.effective 8-23 and
   controllers `cpuset cpu io memory pids`. The deeper slices and the scope
   had `memory pids` only, so they have no cpuset files. `Cpus_allowed_list`
   was 8-23 and `nproc` 16.
2. From the toolbox, `taskset -c 0-23`, then a scope in the probe slice: 8-23.
   Inside it `taskset -c 0-3`, a set disjoint from the cpuset, failed with
   `Invalid argument`. The same two steps into `lushtext-heavy-selftest.slice`
   (today's family) gave 0-23 and 0-3.
3. From a transient service in `app.slice` (affinity 0-23), `taskset -c 0`.
   Into the probe slice the scope got 8-23: the kernel falls back to the cpuset
   when the requested mask and the cpuset are disjoint. Into
   `lushtext-heavy-selftest.slice` it got 0, a P-core.
4. A command whose caller was already pinned to 8-23 (every agent) kept 8-23 in
   today's slice too. The kernel keeps a task's requested mask across a cgroup
   move (Linux ≥ 6.2), which `heavy.sh --self-test` already asserts.

Probes at 00:35, in the toolbox after the widening (effective 0,2-6,8-23):

5. `taskset -c 1,8 sh -c 'grep Cpus_allowed_list /proc/self/status'` printed
   `8` and exited 0. A partial overlap is silently intersected. `taskset -c 1`
   (disjoint) failed with `Invalid argument`. `taskset -c 0-5` gave `0,2-5`.
   `taskset`'s exit status therefore cannot tell a reference run from a run on
   fewer CPUs. Only a read-back can.

Each user-manager slice, today's and the inner `user.slice`, carries the host
drop-in `slice.d/10-oomd-per-slice-defaults.conf`
(`ManagedOOMMemoryPressure=kill`, limit 80%, 20 s).

### CPU-count audit (requirement: nothing sized from 16 CPUs oversubscribes memory)

| knob | sizing | after this change |
| --- | --- | --- |
| `heavy.sh` `CARGO_BUILD_JOBS` | fixed 4 | unchanged |
| `make mutants-*` | fixed 3 jobs × 4 build jobs × 4 test threads | unchanged |
| `proof-strength.py` | fixed 1 job, 4 build jobs | unchanged |
| `widget-shards.py parallel` | one admitted scope per shard (shard count from the table), build jobs 4 | unchanged |
| `RESPONSIVENESS_BUILD_JOBS` | `min(nproc, 16)`, and it **overrides** `heavy.sh`'s 4 for the release build | unchanged: at most 16 |
| nextest and libtest threads inside a scope (`make test-unit`, `test-int`) | `nproc` of the command, its affinity | agents pinned to 8-23: 16, as today. Unpinned callers: 24 today, 22 after the nesting (0,2-6,8-23), bounded by the scope's `MemoryMax` with no swap |

The nesting never widens a scope's CPU set. It can only narrow it from today's
0-23 to the toolbox cpuset. Nothing gains threads from this change, so no
memory knob needs to change. CST's widening raises unpinned toolbox shells from
16 to 22 CPUs whatever this change does, and the scope cap still bounds those
runs.

## Goals / Non-Goals

**Goals:** heavy work inherits the inner `user.slice`'s CPU policy (cpuset,
weight, game-time class) whatever affinity the caller has. Each started scope
proves its own placement. Admission never ignores a scope during the rename.
The only source of the slice name stays `with-memory-cap.sh`. Every timing
measurement runs on one named reference CPU set, or fails, or carries an
explicit override label. The lane records which set it ran on.

**Non-Goals:** changing CST or host configuration; setting CPU properties on our
units; pinning heavy non-measurement work; changing memory caps, budget,
reserve or lock protocol; cleaning up legacy slices automatically; changing the
lane's behaviour outside `measure.sh`.

## Decisions

### D1. Rename to `user-lushtext-heavy.slice` (option A)

systemd derives a slice's parent by removing the last dash component, so
`user-lushtext-heavy.slice` lives at
`user.slice/user-lushtext.slice/user-lushtext-heavy.slice` (probe 1). The
self-test slice becomes a per-run `user-lushtext-heavy-selftest-<pid>.slice`
(D4), a descendant of the real heavy slice, as `lushtext-heavy-selftest.slice`
is today. Its scopes are two levels deeper, so the real slice's admission does not count them, and their
memory counts against the real slice's `MemoryMax` as it does today. The
intermediate `user-lushtext.slice` is implicit and never configured.

Alternatives rejected:

- **Set `AllowedCPUs` and `CPUWeight=30` on `lushtext.slice` ourselves.** This
  copies CST's policy into the repo. It already drifted once on the day of this
  design (8-23 became 0,2-6,8-23). It would also miss the game-time idle class
  and 12-23, and it hardcodes this host.
- **`--slice=user.slice` with no slice of our own.** That loses the host-wide
  `MemoryMax` that bounds the sum of all capped work (the 2026-10-20 incident).
- **A single dash-free name such as `user-lushtextheavy.slice`.** It avoids the
  intermediate slice but drops the `lushtext` family node that CST's planned
  doctor check and `systemd-cgls` readers group by. No benefit was measured.

### D2. No CPU properties on our units

CST's answer (`.claude/handoffs/nest-heavy-slices-under-user-slice/cst-answer.md`):
"Keep CPUWeight UNSET on our scopes". The game-time class and the cpuset come
from the ancestor whatever our properties say. The cpu controller is not even
enabled below `user-lushtext.slice` (probe 1), so all our scopes share one
weight-100 share inside the inner `user.slice`, beside the toolbox scopes.
Ranking our own lanes against each other is out of scope.

### D3. Placement check inside the scope, keyed on computed cgroup paths

The wrapper already has an in-scope shim, `--exec-with-address-space`. It is
renamed `--exec-in-scope` and now runs for every scope:

```
systemd-run … -- bash with-memory-cap.sh --exec-in-scope <slice> <as-kib|-> <command…>
```

It runs `placement_verdict`, then execs the command (after lowering `RLIMIT_AS`
when asked). `placement_verdict` is a pure function of six strings:

1. own cgroup path (`/proc/self/cgroup`, the `0::` line);
2. the ControlGroup of the inner `user.slice`;
3. the ControlGroup of the heavy slice, both read with one call,
   `systemctl --user show -p ControlGroup --value user.slice <slice>`;
4. own `Cpus_allowed_list`;
5. the inner `user.slice`'s `cpuset.cpus` (configured, may be empty), read from
   `/sys/fs/cgroup<(2)>/cpuset.cpus`. A missing file (the cpuset controller is
   not enabled for that slice) reads as empty;
6. its `cpuset.cpus.effective`, read the same way.

It passes when (1) starts with (2) + `/` and with (3) + `/`, and, when (5) is
non-empty, the CPU set of (4) is a subset of (6). A read that fails (no
`/proc/self/cgroup` `0::` line, `systemctl --user show` failing or printing no
ControlGroup) is a failed check that names the read, never a pass. On failure it prints one line,
`with-memory-cap: scope placement: expected …, got …`, and exits 78 (EX_CONFIG)
before running the command. CPU lists are parsed by `cpu_list_expand` (the
`a-b,c` syntax) in `scripts/cpu-lists.sh` (D8), which `measure.sh` shares.

Why each choice:

- The scope has no cpuset files (probe 1), so the check reads the task's
  `Cpus_allowed_list` and the inner `user.slice`'s files.
- The affinity must be a subset of the parent's effective set. It is never
  compared with a literal. The set changes with CST's configuration (0,2-6,8-23
  today, 8-23 before) and with games (12-23). CST's answer asks for "within",
  not equality. A literal would also tie the repo to this host.
- When the inner `user.slice` has no configured cpuset (a host without CST),
  there is nothing to inherit, and only the path is checked.
- It does not assert `cpu.weight` or `cpu.idle`. CST: a `daemon-reload` during
  a game resets them (a CST bug being fixed), so the check would fail for CST's
  reason, not ours.
- The check runs in the scope, not before `systemd-run`. Only the started
  process can see where systemd actually put it. The shim already exists, so
  `--address-space` callers get no new process layer. Other callers gain one
  bash start and one `systemctl show` per capped command, a few tens of
  milliseconds against commands that run for seconds to hours. Task 2.6
  records that cost through `scripts/measure.sh`, main's copy against the
  branch's, interleaved. It is not a budget, because no timed path runs
  through it.
- `systemd-run --scope` execs the command in the caller's own process, so the
  shim sees the caller's cgroup namespace. Toolbox shares the host namespace
  (probe 1 printed the full path). In a private namespace the path would be
  relative, so the check would fail loudly with both values shown rather than
  pass silently.
- CST changes the cpuset at a game's start and end, so the affinity and the
  effective cpuset can be read on either side of a flip. Before failing on the
  CPU half, the check reads both once more and fails only when the second
  reading also disagrees. A spurious exit 78 on a commit hook would be a flake.
- Nested in-place runs (`LUSHTEXT_MEMORY_CAP_KIB` set) start no scope and run
  no check. Neither do `none` caps or the CI uncapped path.

Rejected: checking only in the self-test. The self-test runs at pre-commit, so
it would miss a slice override, a CST change or a systemd change that misplaces
real work between commits.

### D4. Transition accounting

Wrapper copies older than this change (in worktrees that have not merged main)
keep starting scopes in `lushtext-heavy.slice` for a while.

- `admitted_kib` loops over `"$HEAVY_SLICE"` and `legacy_slice_of
  "$HEAVY_SLICE"` (the name without its `user-` prefix: `lushtext-heavy.slice`,
  or the self-test's private legacy slice, below). It applies the existing
  per-scope body to each slice whose ControlGroup exists. That body is whatever
  main has: today the `memory.max` sum, after `reduce-build-disk-writes` the
  live-PID rule. This keeps the merge surface to the loop header.
- `ADMISSION_LOCK` keeps its file name, so old and new copies serialize their
  admissions against each other.
- Residual, accepted and bounded: an old copy counts only the legacy slice. So
  while a new-slice scope runs, an old copy can admit a cap that the new-slice
  scope has already reserved. Both copies still require `MemAvailable` ≥ cap +
  16G reserve. Each slice keeps its own `MemoryMax` (48G), so the nominal
  ceiling over both families is 96G, twice the host-wide budget, until the
  window closes. What bounds it in practice is the per-admission
  `MemAvailable` ≥ cap + 16G check on the 94 GB host, and every scope's own cap
  with no swap. The window closes when every active worktree has merged main.
  The coordinator tells each run to merge origin/main before its final gates;
  a worktree that runs heavy work for days should merge sooner.
- The self-test must not see other runs' scopes, nor show its scopes to them.
  Old copies' self-tests run concurrently in other worktrees and start 2G
  scopes in `lushtext-heavy-selftest.slice`, and with legacy accounting a shared
  self-test slice would count them in both directions: a fit-expecting case
  (`reduce-build-disk-writes` adds one at a 3G budget) would then read 75. The
  self-test therefore uses a per-run slice, `user-lushtext-heavy-selftest-<pid>.slice`.
  It still nests under the real heavy slice. Its legacy slice is
  `lushtext-heavy-selftest-<pid>.slice`. An `EXIT` trap, so on every exit
  path including each early `return 1`, runs `systemctl --user stop` and then
  `systemctl --user revert` on both private slices; no other process can be
  using them. Both steps are needed (probed in the spec review, 2026-11-15): a
  stopped slice keeps its runtime drop-in directory in
  `$XDG_RUNTIME_DIR/systemd/user.control/` until it is reverted, which would
  leave one directory per commit until logout; after both, the unit unloads. The rule that legacy slices are
  never stopped applies to the real `lushtext-heavy.slice` and to the shared
  `lushtext-heavy-selftest.slice`.
- The wrapper never stops, reverts or resizes the legacy slices:
  - stopping a slice stops its scopes, and an old copy may be starting one;
  - lowering the legacy `MemoryMax` would OOM-kill old work.
- An empty legacy slice holds nothing. systemd keeps it loaded until the user
  manager stops (the probe slices stayed `active` until stopped), and its
  runtime drop-ins in `$XDG_RUNTIME_DIR/systemd/user.control/` vanish with it.
  `for-cst.md` tells CST's doctor check to expect it until the next login. It
  also gives the manual command for an operator who wants it gone sooner:
  `systemctl --user stop lushtext.slice`, once `lushtext.slice/cgroup.events`
  reads `populated 0`.
- The legacy branch is temporary. Its removal is recorded as a deferral in the
  programme record: once no worktree branched before the merge SHA remains,
  delete `legacy_slice_of`, the loop and the two legacy self-test cases. Until
  then every self-test run creates the top-level `lushtext.slice` again for its
  private legacy slice, which is expected.

Rejected: a migration that moves running scopes into the new slice. systemd
cannot re-parent a running scope.

### D5. The guard

`LUSHTEXT_HEAVY_SLICE` (self-test only) must match `user-lushtext-heavy*.slice`.
A value matching the old `lushtext-heavy*.slice` gets a specific message,
"…was renamed to user-lushtext-heavy*.slice so it nests under the user
manager's user.slice", and exit 2.

### D6. `heavy.sh` self-test

The "capped scope" case pinned the command to the caller's first CPU and
expected exactly that CPU in the scope. A caller whose first CPU lies outside
the toolbox cpuset (CPU 1 today, CPU 0 before the widening, from a host
terminal) would get the whole cpuset in the new slice (probe 3), and the case
would fail through no fault of `heavy.sh`. The case now reads the inner
`user.slice`'s effective cpuset, or `/sys/devices/system/cpu/online` when none
is configured. It picks the first CPU of the intersection with the caller's
affinity and expects exactly that CPU. When the intersection is empty, it
expects the slice's whole set. The cgroup assertion drops the literal
`grep -c "/lushtext-heavy.slice/"`. Instead, the ControlGroup of
`$(with-memory-cap.sh --print-slice)` must be a prefix of the command's
`/proc/self/cgroup`, so the slice name has one source.

Self-test cases that need a CPU outside the cpuset compute it at run time, as
the first online CPU that `cpuset.cpus.effective` lacks. When every online CPU
is in the cpuset, they skip and print why. No test names a CPU number. A
caller pinned to such a CPU cannot be made inside the toolbox (the kernel
refuses a disjoint mask, H3), so the case that needs one starts its caller in a
transient scope in `app.slice` (cpuset 0-23 here), and runs the wrapper from
there. Probed in the spec review (2026-11-15): from such a scope pinned to
CPU 1, a scope in a `user-lushtext-heavy-selftest-*` slice got 0,2-6,8-23, and
`taskset -c 1` inside it was refused. The outer scope needs an explicit
`--unit=` name: `taskset` and the wrapper exec, so the wrapper's `systemd-run`
keeps the outer scope's PID, and the default name `run-p<pid>-i<id>.scope`
collided with it ("already loaded").

### D7. Risk: systemd-oomd candidate set (recorded, no change)

The same drop-in monitors every user-manager slice, today's slices included:
`kill` above 80% memory pressure for 20 s. What changes is that the inner
`user.slice`'s pressure now includes our scopes, and its kill candidates include
the toolbox containers. A pressure kill selects the descendant with the most
reclaim. A thrashing heavy scope would be that descendant itself, and the deeper
`user-lushtext-heavy.slice` crosses its own limit first. Each scope's
`MemoryMax` with no swap also makes the kernel kill inside the scope before
sustained pressure builds. No evidence calls for a CST ask. The risk is written
in `build.md` so a future oomd kill of a toolbox is traced here.

### D8. One reference CPU set, applied and verified by `measure.sh`

- **One value.** `measure.sh` defines `MEASURE_REFERENCE_CPUS_DEFAULT=8-23` and
  reads `LUSHTEXT_REFERENCE_CPUS` as the override.
  `measure.sh --print-reference-cpus` prints the effective value, so docs and
  tests never copy the literal. 8-23 is the set of every earlier verdict:
  programme record "affinity (E-cores 8–23, inherited)",
  `docs/benchmarks/transient-file-load.md`, and the lane `environment.txt`
  files, all 20 of which on disk on 2026-11-15 record `affinity=8-23`.
- **Allowed set = the cgroup's effective cpuset, not the caller's mask.**
  `measure.sh` reads its own `0::` path from `/proc/self/cgroup`. It walks up to
  the nearest directory under `/sys/fs/cgroup` that has `cpuset.cpus.effective`
  (the toolbox's `…/container` has one). With no such file (no cpuset
  controller, cgroup v1) it uses `/sys/devices/system/cpu/online`. This is the
  set the kernel lets the process use. A caller pinned narrower than the
  reference (say 12-23 by its own choice) is widened to the reference: the
  reference wins over an incidental caller mask. That does not contradict the
  lane's "never widened" rule. The lane still never widens the mask it
  inherits, and under `measure.sh` the mask it inherits is the reference. Under
  `measure.sh` the lane also never trims it (D9), which is why the core-cap
  scenario is MODIFIED to apply outside `measure.sh` only.
- **Apply, then read back, and never intersect.** Order inside `measure.sh`:
  parse arguments, then the existing inside-heavy-work refusal (exit 2, which
  stays first), then the reference check, then the locks. The check is:
  1. parse the reference (malformed list: exit 2);
  2. require reference ⊆ allowed. Otherwise exit 78 with
     `measure: reference CPU set R is not available (this cgroup allows A); not
     measuring elsewhere`, and a second line naming the usual cause (a CST
     game profile narrows the toolbox cpuset while a game runs) and the override;
  3. `taskset -pc R $$` on itself;
  4. read `Cpus_allowed_list` from `/proc/self/status` and require equality
     with R as sets. Otherwise exit 78 with both sets. Probe 5 shows why the
     read-back is required: `taskset` succeeds on a partial overlap.

  This runs before any lock is taken, so a refused measurement never drains
  other agents' work. A failing step 2 or 4 reads both sets once more before
  refusing, because a CST flip can fall between the two reads (D3). The lock
  wait can last up to `--lock-timeout` (1800 s), so once both locks are held
  `measure.sh` reads its affinity again before the `before` record. If the set
  changed, it releases the locks and exits 78 without running the command.
  The command inherits the mask. The before/after `snapshot` processes run
  pinned too, which is harmless.
- **The set may change during the run.** CST narrows the cpuset at game start,
  and the kernel then shrinks every task's effective mask (probe 3's rule).
  After the command, `measure.sh` reads its own affinity again. If it differs
  from R, the `after` record carries `cpus_changed=<now>` and `measure.sh` exits
  78 after releasing the locks, whatever the command's status. The command's
  own status stays in the record (`status=`). A non-zero exit is what keeps a
  disturbed run from being read as a verdict. A narrowing that is undone before
  the command ends goes unseen. That residual is accepted: game sessions last
  much longer than a measurement, and the lane additionally records its own
  affinity per run.
- **Records and export.** The `before` and `after` lines gain
  `cpus=<read-back> reference=<R> source=<default|override>`. The command gets
  `LUSHTEXT_MEASURE_CPUS=<R, canonical list>` and
  `LUSHTEXT_MEASURE_CPUS_SOURCE=<default|override>`. A nested in-place
  `measure.sh` (LOCK already held exclusively) re-runs the check on the same
  inherited environment, a no-op.
- **During a game.** The default refuses (exit 78): never silently on a
  different set. Measuring anyway needs `LUSHTEXT_REFERENCE_CPUS=12-23`, which
  runs and is recorded `source=override`, so the run is labelled non-reference.
  `verification-runs.md` says such a run is not comparable with the reference
  and that interleaved sides must share one set.
- **Callers that are tests.** `heavy.sh --self-test` runs `measure.sh -- true`
  directly twice (the nested-heavy and leftover-daemon cases) and expects it to
  run. With the default 8-23 those cases would exit 78 during every game and
  on any host without CPUs 8-23. They run in the pre-commit hook, so every
  commit would then fail. `heavy.sh`'s self-test therefore exports
  `LUSHTEXT_REFERENCE_CPUS` set to its own affinity, as `measure.sh`'s does
  (task 3.1). No other script in the repo calls `measure.sh`.

Rejected alternatives:

- **Pin in the lane only.** Axiom samples, benches and the reference sweep also
  run under `measure.sh`, so the reference belongs where every timing run
  passes.
- **Pin to reference ∩ allowed.** That is the silent change (B) exists to
  remove. CST's second answer (item 2) worded the pin as "8-23 ∩ allowed".
  This design deliberately refuses instead (D13).
- **Wrap the command in `taskset -c R`.** That leaves `measure.sh`'s after-check
  with nothing to read. Pinning itself gives it a mask to read before and after.

### D9. The lane takes the reference and records it

`run-responsiveness-smoke.sh` replaces `affinity-list` with a driver subcommand,
`lane-affinity MAX_CPUS`, so the rule is unit-tested in Python:

- `LUSHTEXT_MEASURE_CPUS` set: when the inherited affinity equals it as a set
  and it has at most `MAX_CPUS` CPUs, print it. The lane then runs with no
  `taskset`. Otherwise exit non-zero with the reason ("inherited affinity X is
  not measure.sh's reference R", or "reference R has N CPUs, over the lane's
  16-CPU cap; the lane never trims a reference"). The lane fails before any
  case runs (`smoke_fail`). With the default 8-23 the cap is inert: 16 CPUs.
- unset: today's rule, the first `MAX_CPUS` CPUs of the inherited affinity
  (the core-cap scenario, MODIFIED only to say it applies outside
  `measure.sh`; its rule is unchanged).

Recording:

- `environment.txt` gains `measure_cpus=` and `measure_cpus_source=` (empty
  when unset);
- `run.json` (`run_metadata`) gains `measure_cpus`, `measure_cpus_source` and
  `reference_run` (true when `measure_cpus` is set, its source is `default`, and
  `affinity` equals it);
- `report.md`'s environment line reads one of:
  - `affinity: 8-23 (the reference CPU set, pinned by scripts/measure.sh)`;
  - `affinity: X (override of the reference CPU set: not comparable with
    reference runs)`;
  - `affinity: X (not pinned to the reference CPU set: trend evidence, not a
    verdict)`.

`merge_runs` today copies `runs[0]` and so takes the first part's affinity.
When the parts' `affinity`, `measure_cpus` or `measure_cpus_source` differ,
the merged `run.json` instead records `affinity: "mixed"` with a per-part list
in `merged_from`, and `reference_run: false`. A part whose `run.json` lacks
the fields (written before this change) counts as unset, so it is equal to an
unpinned part and differs from a pinned one. The report prints
`affinity: mixed (parts ran on different CPU sets: not comparable)`. The C6
side comparison already reads `merged_from`. It is not changed beyond carrying
the per-part set.

### D10. Heavy work keeps the toolbox's cpuset

`heavy.sh` keeps inheriting the caller's affinity. With the nesting, that
affinity is bounded by the inner `user.slice`'s cpuset (D3). Pinning builds to
8-23 would waste the CPUs CST grants and gain nothing. Only measurements need a
stable set. Agents that pin their own sessions (D13 item 1) narrow that set
for their own heavy work. The slice cannot widen it.

### D11. The host fact (B) relies on

Outside games, CPUs 8-23 must stay inside the toolbox cpuset. Both the old
(8-23) and the new (0,2-6,8-23) configurations satisfy this, and so does nothing
during games (12-23), when `measure.sh` refuses. This is a dependency on CST,
not a request for change. It goes to `cst-asks.md` as a note to keep 8-23
inside `user.slice` outside games, or to tell us first. If it ever breaks,
`measure.sh` refuses loudly. No run silently changes reference.

### D12. What is measured and what rests on CST's answer

Measured: the nesting (H1), the inherited cpuset (H2) and the kernel's
intersection (H3), all on this host. Not measured: the game-time idle class
reaching our scopes. No game ran during the spec stage, and the design rests
on CST's answer that scopes under the inner `user.slice` get it "regardless of
the scope's MemoryMax/CPUWeight". That follows from cgroup v2 (`cpu.idle` on
an ancestor applies to its subtree). Task 2.6 records `cpu.idle` and the
scope's effective cpuset if a game happens to run during implementation, and
otherwise says it was not observed. No check asserts it (CST's answer). It is
therefore a reliance of this design, not a spec scenario: the spec asserts
only what a check can observe (the scope nests under the inner `user.slice`
and sets no CPU property; its affinity lies within the inner slice's effective
cpuset, whatever that is at the time). Whether a game runs is read from the
existence of CST's `game.state` file (`/run/host/run/cpu-setup/game.state`
from a toolbox, `/run/cpu-setup/game.state` on the host), never its contents
(CST: best effort, format changing). That is evidence only, never a gate.

### D13. CST's second answer (00:45), item by item

CST wrote a second answer after the proposal was first committed
(`cst-answer.md`, "answer 2", marked "in scope for this change"). CST is
another agent session; its items are advice to weigh, not requirements. Each
item's disposition:

1. **"Inherit, never pin: agents drop `taskset`; update the rules that tell
   agents to pin."** No rule, skill or runbook in the repo tells agents to
   pin (`git grep taskset` over `.agents/`, `docs/runbooks/`, `AGENTS.md` finds
   none). The `build.md` passage CST cites ("16 CPUs taken from the inherited
   mask") is the lane's own cap, kept by D9. The `taskset -c 8-23` that agents
   run under comes from the maintainer's session instructions, outside the
   repo. Changing it is the maintainer's decision, so this change neither adds
   nor removes it. With the nesting, an agent's pin can only narrow what CST
   grants (H3), which costs CPUs, not correctness. Task 4.2 makes the rules say
   that heavy work inherits the toolbox cpuset and that a self-imposed mask
   only narrows it. The question goes to the maintainer through the stage
   conclusion and `for-cst.md`.
2. **"Only `measure.sh` pins: 8-23 ∩ allowed."** Adopted, except the
   intersection. `measure.sh` refuses instead of measuring on the intersection
   (D8), because an intersected run is the silent change of reference that (B)
   removes. `cst-asks.md` tells CST.
3. **"Size parallelism by memory; raise `CARGO_BUILD_JOBS` 4 → 6 → 8 while
   watching btrfs-stall-report."** Deferred to its own change. It needs a
   per-job memory peak measured across heavy runs, it changes every build's
   memory footprint, and 2026-11-14 already had two memcg OOM kills at today's
   fixed counts (CST's own note). This change keeps every count (CPU-count
   audit). The deferral goes to the programme record (task 4.6).
4. **"`game.state` exists while the game profile is on; no pause policy."**
   Used only as evidence (D12). No pause, no gate, no message depends on it.
   `measure.sh` refuses on the cpuset itself, which is what matters for a
   reference run.

## Axioms relied on

No GTK behaviour (no ledger axiom). Host behaviour, measured above and restated
in `with-memory-cap.sh`'s and `measure.sh`'s headers:

- H1. systemd places `a-b-c.slice` under `a.slice/a-b.slice` (probe 1).
- H2. A cgroup v2 cpuset restricts every descendant, whether or not the
  controller is enabled below it (probe 1: a scope without cpuset files had
  affinity 8-23).
- H3. On attach or on a cpuset change, the kernel sets a task's affinity to its
  requested mask intersected with the cpuset, or to the cpuset when they are
  disjoint (probes 2-4). `sched_setaffinity` with a mask disjoint from the
  cpuset fails with `EINVAL`. A partial overlap succeeds and is intersected
  (probe 5).

The self-tests re-probe H1 and H3 on every run (placement case,
partial-overlap case). H2 is covered by the placement case's affinity check.

## Timing

- Timed paths touched: which CPUs `measure.sh` runs its command on. No budget
  constant, lane threshold or verdict changes.
- Evidence, not a verdict: one lane case (`baseline/small`, plain variant,
  quick tier) is run through `measure.sh` from a shell pinned to a proper
  subset of the reference (its upper half, computed at run time), so the
  caller's mask differs from the reference without leaving the agents' 8-23. It
  shows `environment.txt` and `run.json` recording `affinity=8-23`,
  `measure_cpus=8-23`, `measure_cpus_source=default` and `reference_run: true`.
  One `LUSHTEXT_REFERENCE_CPUS=12-23` run of the same case shows the override
  label. The 1-minute load is recorded with each. Neither is compared with
  anything, so no interleaved main/branch rounds are needed. On main, the run
  records the same affinity whenever the caller is pinned to 8-23, so the change
  cannot move a figure for an existing pinned verdict. It removes the dependence
  on the caller's pinning.
- The placement shim's cost (D3): `with-memory-cap.sh 64M true`, main's copy
  (`git show origin/main:scripts/with-memory-cap.sh`) against the branch's,
  interleaved main → branch → main → branch, five runs per round, all inside
  one `scripts/measure.sh` call, so the exclusive lock drains other agents
  once. Non-verdict
  figures, recorded with their 1-minute load. They are not compared with a
  budget, because no timed path runs a capped scope.
- Self-test cases that need a caller outside the cpuset run only `/proc` reads
  on that CPU, for a few milliseconds. They put no load outside 8-23.
- Heavy work under the nesting: test-suite wall times, never verdicts, may grow
  under desktop load (weight 30) and stall during a game. That is the accepted
  loss.
- Local Kani `KANI_MEASURE` and proof-strength figures are not verdicts (shard
  budgets come from CI runners, `build.md`).

## Kani

Kani: none (no proven module touched).

## Migration plan

1. Merge. New wrapper copies start scopes in `user-lushtext-heavy.slice` and
   count both slices. `measure.sh` pins every measurement to 8-23.
2. Parallel worktrees merge main before their final gates (coordinator rule)
   and switch over. A worktree still on the old `measure.sh` measures on its
   caller's affinity, as today. Its records lack `reference=`, which marks them.
3. The legacy slices empty out and stay inert until the next login (D4).
4. A follow-up removes the legacy branch (deferral in the programme record).

Rollback: revert the merge. Old copies count only the legacy slice. Scopes
still running in the new slice keep their own caps until they end. Measurements
fall back to the caller's affinity.

## Open Questions

None. CST answered the CPU-property and verification questions
(`cst-answer.md`). The one host dependency of (B) is recorded as an ask (D11).
