## Why

Two problems with where our work runs on the shared workstation.

**(A) Heavy work escapes the host's CPU policy for the toolboxes.** Every capped
heavy command (`scripts/heavy.sh`, `make kani`, `make test`'s widget shards,
cargo-mutants, proof strength) runs in a systemd user scope that
`scripts/with-memory-cap.sh` starts in `lushtext-heavy.slice`. systemd derives a
slice's parent from its dash prefix, so that slice sits at
`user@1000.service/lushtext.slice/lushtext-heavy.slice`. Its parent is the user
manager itself. It is a sibling of `app.slice` (the desktop and games) and of the
**inner `user.slice`**. CST (the host's CPU/cgroup tooling) puts the toolboxes
in the inner `user.slice`, at `CPUWeight=30` on its own cpuset, and moves that
slice to the idle class on CPUs 12-23 while a game runs.
`systemd-run --user --scope` starts the scope from the user manager. The scope
takes the manager's placement, not the caller's. So our heavy work, started from
those toolboxes, leaves all of that behind.

Measured on this host (kernel 7.2.15, systemd 262, 2026-11-15; design
"Baseline"):

| | `lushtext.slice` (today) | inner `user.slice` |
| --- | --- | --- |
| `cpu.weight` | 100 (same as `app.slice`) | 30 (`CPUWeight=idle` during a game) |
| `cpuset.cpus.effective` | 0-23 | 0,2-6,8-23 (8-23 before CST widened it that day; 12-23 during a game) |
| CPU time so far | `usage_usec` 68,751,468,578 (68.8k CPU-seconds) | — |

With a probe slice named `user-lushtext-heavy-probe.slice`, systemd nested the
scope as
`user.slice/user-lushtext.slice/user-lushtext-heavy.slice/user-lushtext-heavy-probe.slice`.
It ran under the inner `user.slice`'s `cpu.weight` 30 and cpuset. A caller
pinned to a CPU outside that cpuset (CPU 0 when the cpuset was 8-23) got the
cpuset instead. Inside the nesting the kernel refuses an affinity that is
disjoint from the cpuset (`EINVAL`). Inside today's slice it accepts it.

Agents that run under `taskset -c 8-23` already kept their affinity in today's
slice. Since Linux 6.2 the kernel keeps a task's requested CPU mask across a
cgroup move, and the `heavy.sh` self-test asserts this ("CPU kept"). The loss
that applied to every run was the CPU weight and the game-time idle class:
heavy builds competed with a running game at the same weight as the game.

**(B) The timing reference CPU set is implicit.** Every timing verdict in the
long-line programme was taken on E-cores 8-23. The programme record names
"affinity (E-cores 8–23, inherited)" as the reference run. That held only
because every agent was started under `taskset -c 8-23` and the toolbox cpuset
was 8-23. `scripts/measure.sh` sets no affinity. The responsiveness lane takes
"the first 16 CPUs of the inherited affinity". On 2026-11-15 CST widened the
toolbox cpuset to 0,2-6,8-23 (22 CPUs, P-cores included). From an unpinned
toolbox shell the lane would now measure on 0,2-6,8-17, a mix of P-cores and
E-cores, and nothing would record that the reference changed. Lane artifacts
already hold runs on 8-23 and on 12-23 (game time) side by side, with nothing
marking which one is the reference. The kernel makes this easy to miss:
`taskset -c 1,8` in a cpuset without CPU 1 succeeds and leaves the command on
CPU 8 alone (measured, design "Baseline").

Both changes are worth doing as described.

## What Changes

- **(A) Rename the slices** so systemd nests them under the inner
  `user.slice`: `lushtext.slice` becomes `user-lushtext.slice` (implicit, never
  configured) and `lushtext-heavy.slice` becomes `user-lushtext-heavy.slice`.
  The self-test slice becomes `user-lushtext-heavy-selftest.slice`. The override
  guard accepts only `user-lushtext-heavy*.slice` and names the new prefix when
  it refuses an old one.
- Set no CPU property on our slices or scopes. They inherit the inner
  `user.slice`'s cpuset, weight and game-time class (CST's answer, 2026-11-15).
  Memory properties, caps, the host-wide budget, the desktop reserve and
  `OOMPolicy=continue` stay as they are.
- **The scope checks its own placement.** The scope's first process (the
  existing `--exec-with-address-space` shim, renamed `--exec-in-scope` and now
  run for every scope) checks three things. Its cgroup path from
  `/proc/self/cgroup` must lie under the ControlGroup that systemd reports for
  the inner `user.slice`, and under the one for the heavy slice. When the inner
  `user.slice` has a configured cpuset, the command's CPU affinity must also be
  a subset of that slice's `cpuset.cpus.effective`, read from `/sys/fs/cgroup`
  at run time and never written into the repo. A failed check exits 78, before
  the command runs, with the expected and actual values. The checker is a pure
  function, so the self-test covers it with fixtures and with a real scope.
- **Accounting stays correct during the transition.** Admission adds up the
  caps of the scopes in the current slice and in the legacy slice of the same
  family (the name without `user-`), with the same per-scope rule. The admission
  lock file keeps its name, so old and new copies of the wrapper exclude each
  other. The wrapper never stops, reverts or resizes a legacy slice, because an
  old copy may be starting a scope in it.
- Fix `heavy.sh --self-test` ("capped scope" case). It expected the command to
  keep exactly the caller's first CPU. It now picks a CPU that the caller's
  affinity and the slice's effective cpuset share. When they share none, it
  expects the slice's whole effective set. It reads the slice name from the
  wrapper (`with-memory-cap.sh --print-slice`) instead of a second literal.
- **(B) An explicit reference CPU set for measurements.** `scripts/measure.sh`
  defines one value, `8-23` by default, overridable with
  `LUSHTEXT_REFERENCE_CPUS`. Before it takes any lock, it sets its own affinity
  to that set and reads the affinity back. When the reference is not a subset of
  the CPUs its cgroup allows (`cpuset.cpus.effective` of the nearest cgroup that
  has one), or the read-back differs, it exits 78 and names both sets. It never
  measures on the intersection. After the command it reads the affinity again.
  If the set changed during the run (CST narrowing it for a game), the record
  says so and `measure.sh` exits 78. It exports the applied set
  (`LUSHTEXT_MEASURE_CPUS`, and `LUSHTEXT_MEASURE_CPUS_SOURCE=default|override`)
  to the command, and its before/after records carry `cpus=`, `reference=` and
  `source=`.
- **The responsiveness lane records the reference.** When
  `LUSHTEXT_MEASURE_CPUS` is set, the lane requires its inherited affinity to
  equal it. It also requires it to fit the lane's 16-CPU cap, and otherwise
  fails before any case runs. It never trims the reference. It writes the set
  and its source to `environment.txt`, `run.json` and `report.md`. A run not
  started through `measure.sh` is labelled "not pinned to the reference CPU set
  (trend evidence)". `merge-runs` labels a merge whose parts ran on different
  CPU sets as mixed and not comparable. It no longer copies the first part's
  affinity.
- Heavy non-measurement work keeps whatever cpuset the toolbox grants. Nothing
  pins it to the reference.
- Documentation: `.agents/rules/build.md`, `.agents/rules/verification-runs.md`
  (heavy-work affinity wording; the reference CPU set, what is not comparable
  with it, and how such runs are labelled), `docs/runbooks/responsiveness-lane.md`,
  the Makefile comments, `docs/mutation-testing.md`, the wrapper, `heavy.sh` and
  `measure.sh` headers, and the programme record's entry. After merge,
  `.claude/handoffs/nest-heavy-slices-under-user-slice/for-cst.md` gets the
  merge SHA and the verification commands.

## Non-goals

- No change to CST or to any host configuration. CST answered that nothing is
  needed for (A) (`cst-answer.md`). (B) relies on one host fact, that CPUs 8-23
  stay inside the toolbox cpuset outside games. That is written as an ask in
  `.claude/handoffs/nest-heavy-slices-under-user-slice/cst-asks.md`.
- No `CPUWeight`, `AllowedCPUs` or `CPUQuota` on our units.
- No change to memory caps, the 48G host-wide budget, the reserve, admission
  timing or the lock protocol.
- No affinity for heavy non-measurement work. Agents may still pin themselves.
- No automatic cleanup of the legacy slices. An empty legacy slice holds no
  process and no budget. Its runtime drop-ins live in `/run` and go away when
  the user manager stops (logout or reboot).
- No change to the lane's cap or its behaviour when it is not run through
  `measure.sh` (nightly CI, quick trend runs).

## User-visible impact

- The desktop and games: heavy builds no longer compete with a game at equal
  weight. During a game they are idle class and confined to CPUs 12-23, like the
  toolboxes. Outside games they run on whatever cpuset CST grants the toolboxes
  (0,2-6,8-23 today).
- Loss compared with main (accepted by the maintainer): all heavy work runs at
  `cpu.weight` 30 against desktop load, and heavy work started without a narrow
  mask loses CPUs 1 and 7. Builds and tests may be slower when the desktop is
  busy, and they stall while a game runs.
- Measurements: a verdict now runs on 8-23 whatever the caller's affinity, the
  same set as every earlier verdict. During a game `measure.sh` refuses (exit
  78) instead of measuring on 12-23. A run on any other set needs an explicit
  override and is labelled non-reference.
- No app behaviour, persisted format, GSettings key, action or D-Bus member
  changes.

## Capabilities

### New Capabilities

- `heavy-work-placement`: Purpose: "Where the shared-machine helpers place
  memory-capped heavy work in the systemd user manager's cgroup tree, so that it
  inherits the host's CPU policy for the toolboxes and admission counts every
  admitted scope exactly once."
- `measurement-reference-cpus`: Purpose: "Which CPUs a timing measurement runs
  on, and how a run records them, so that verdicts taken at different times and
  on different host configurations compare like with like or are labelled as not
  comparable."

### Modified Capabilities

- `widget-test-ci-sharding`: the requirement "Parallel local shards are
  isolated from each other" no longer names the slice literally. It refers to
  the host-wide heavy slice, and gains a scenario for the nesting. Every
  existing scenario keeps its name.
- `responsiveness-measurement`: ADDED requirement "A lane run through
  scripts/measure.sh runs on and records the reference CPU set". The existing
  core-cap scenario is unchanged.

## Impact

- Code: `scripts/with-memory-cap.sh` (slice names and guard, legacy-aware
  `admitted_kib`, the in-scope placement check, `--print-slice`, self-test),
  `scripts/heavy.sh` (self-test CPU pick and slice name, header),
  `scripts/measure.sh` (reference set, pin and read-back, records, export,
  self-test), `scripts/run-responsiveness-smoke.sh` and
  `scripts/responsiveness-smoke-driver.py` (lane affinity from the exported
  reference, recording, `merge-runs` labelling, self-tests). A small shared CPU
  list helper, `scripts/cpu-lists.sh`, is sourced by the wrapper and
  `measure.sh`. `widget-shards.py`, `kani-shards.py`, `proof-strength.py` and
  `run-widget-tests.sh` do not name the slice and stay unchanged.
- Coordination: `reduce-build-disk-writes` rewrites the body of `admitted_kib`
  (live-PID rule, `lushtext-cap-<pid>-<ns>.scope` unit names) and adds `--unit=`
  to the `systemd-run` call. This change wraps that per-scope body in a loop over
  slices and leaves the body unchanged. Whichever change merges second
  resolves the conflict in that function.
- Tests: wrapper, `heavy.sh` and `measure.sh` self-tests
  (`make check-agent-helpers`) and the lane self-tests
  (`make check-responsiveness-lane`), both part of `make check-policy` and the
  pre-commit hook. There is no Rust, GTK or Kani surface. One lane case is run
  through `measure.sh` to show the recorded set. It is evidence, not a verdict.
