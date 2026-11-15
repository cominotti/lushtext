Kani: none (no proven module touched).
Timing: evidence only, no verdict (design "Timing"): one lane case through
`measure.sh` showing the recorded reference set, and one override run.
GTK axioms: none (no toolkit behaviour relied on). The host behaviour H1-H3 is
restated in the wrapper and `measure.sh` headers and re-probed by the
self-tests below.

Every heavy command goes through `scripts/heavy.sh [--mem <cap>] -- <cmd>`, one
per call, and every commit through `scripts/heavy.sh --mem none -- git commit -F
<file>` (signed, conventional, staged by name). Work only in
`.claude/worktrees/nest-heavy-slices-under-user-slice`. No self-test names a CPU
number: CPUs inside or outside the cpuset are computed at run time from
`/sys/fs/cgroup` and `/sys/devices/system/cpu/online`, and a case that needs a
CPU that does not exist on the host skips and says why (design D6).

## 1. Failing-first self-test cases for (A) (no behaviour change yet)

- [ ] 1.1 Merge `origin/main` into the branch. If `reduce-build-disk-writes` has
  landed, its `admitted_kib` body (live-PID rule, `lushtext-cap-<pid>-<ns>`
  units) is the per-scope body that group 2 wraps. Resolve any conflict in that
  function by keeping their body inside this change's slice loop.
- [ ] 1.2 In `with-memory-cap.sh --self-test`, add the cases below. Each must
  fail on the current wrapper. Record the failure lines in
  `openspec/changes/nest-heavy-slices-under-user-slice/evidence/red.md`:
  - placement: a 64M scope's `/proc/self/cgroup` starts with the ControlGroup
    of the inner `user.slice` plus `/`, and with that of the slice from
    `--print-slice` plus `/`. Fails today: the path is under `lushtext.slice`,
    and `--print-slice` is unknown.
  - CPU: when the inner `user.slice`'s `cpuset.cpus` is non-empty and some
    online CPU lies outside its `cpuset.cpus.effective`, a scope started under
    `taskset -c <all online CPUs>` has a `Cpus_allowed_list` that is a subset
    of the effective cpuset. Fails today, because the scope gets all online
    CPUs (design probe 2). Also a fixture case for the re-read: a verdict
    whose first CPU reading disagrees and whose second agrees passes.
  - the guard: `LUSHTEXT_HEAVY_SLICE=lushtext-heavy-selftest.slice` exits 2
    and names `user-lushtext-heavy`. Fails today: accepted.
  - the checker with fixtures (`--placement-verdict <six values>`, design D3):
    - a pass;
    - a path outside the heavy slice;
    - a path outside `user.slice`;
    - affinity `0-23` against `8-23`;
    - an empty configured cpuset with any affinity (pass);
    - the list syntax `8-11,16,20-23`.

    Each exits 0 or 78. Fails today: unknown option.
  - refusal in a real process: `--exec-in-scope` run directly in the caller's
    own cgroup (the toolbox `libpod` scope, outside the heavy slice) exits 78
    and does not run its command, so a marker file stays absent. Fails today:
    unknown option.
  - legacy accounting: start a legacy self-test scope, `systemd-run --user
    --scope --slice=lushtext-heavy-selftest.slice -p MemoryMax=2G sleep 10`.
    Then `LUSHTEXT_ADMISSION_NOWAIT=1 LUSHTEXT_HEAVY_MEMORY_MAX=3G
    LUSHTEXT_MEMORY_RESERVE=1G <wrapper> 2G true` must exit 75. Then kill the
    sleeper. This case cannot be red on main, which has no rename. Its red run
    is the intermediate state in task 2.3 (renamed, no loop yet).
  - legacy left alone: the legacy self-test slice's `MemoryMax` and drop-in
    list (`systemctl --user show -p MemoryMax -p DropInPaths`) are the same
    before and after a current-slice admission.
- [ ] 1.3 In `heavy.sh --self-test`, change the "capped scope" case as design D6
  describes: the intersection CPU pick, the empty-intersection expectation, and
  the cgroup prefix from `with-memory-cap.sh --print-slice`. Record today's
  failure in `evidence/red.md`: `--print-slice` is unknown.
- [ ] 1.4 Run both self-tests on this state and record each red line in
  `evidence/red.md`. Do not commit a red tree: the pre-commit hook runs
  `check-agent-helpers`, and `--no-verify` is forbidden. Groups 1 and 2 land as
  one commit (task 2.7).

## 2. The rename, the check and the transition (design D1-D7)

- [ ] 2.1 `HEAVY_SLICE` defaults to `user-lushtext-heavy.slice`, the self-test
  slice is `user-lushtext-heavy-selftest.slice`, and the guard follows D5.
  `ADMISSION_LOCK` keeps its file name. Add `--print-slice`, which prints
  `$HEAVY_SLICE`.
- [ ] 2.2 Add `scripts/cpu-lists.sh`, sourced by the wrapper and later by
  `measure.sh`. It holds `cpu_list_expand` (a CPU list to one CPU per line,
  exit 2 on a malformed list), `cpu_list_compress`, `cpu_list_subset` and
  `cpu_list_equal`, plus `cgroup_effective_cpus` (nearest
  `cpuset.cpus.effective` above `/proc/self/cgroup`'s `0::` path, or the
  online CPUs) and `cpu_outside_cpuset` (the first online CPU the effective
  set lacks, or nothing). Add `shellcheck -x scripts/cpu-lists.sh` to
  `check-agent-helpers`.
- [ ] 2.3 Add `placement_verdict` (pure; the six inputs from design D3),
  `--placement-verdict` (fixtures entry, self-test only) and
  `--exec-in-scope <slice> <as-kib|-> <command…>`, which replaces
  `--exec-with-address-space`. Every scope launches through it. A failure exits
  78 with one `with-memory-cap: scope placement: expected …, got …` line.
- [ ] 2.4 Add `legacy_slice_of`. Wrap the existing per-scope body of
  `admitted_kib` in a loop over the current and legacy slices (D4), leaving the
  body itself unchanged. Run the self-test before the loop exists and see the
  legacy case fail, then add the loop. Record both runs in `evidence/red.md`.
- [ ] 2.5 Update the wrapper header: the slice family and why it nests under
  the inner `user.slice`; no CPU properties (CST); the placement check and exit
  78; legacy accounting and why legacy slices are never touched; H1-H3. Update
  `heavy.sh`'s header, step 4: "the CPU affinity this process inherited,
  intersected with the slice's cpuset".
- [ ] 2.6 Real placement. Run `scripts/heavy.sh -- sh -c 'cat /proc/self/cgroup;
  grep Cpus_allowed_list /proc/self/status; nproc'`. Run one parallel widget
  shard, whose scope `widget-shards.py` admits itself: `scripts/heavy.sh --mem
  none -- ./scripts/widget-shards.py parallel <smallest shard>`. In
  `evidence/placement.md`, record:
  - the cgroup paths and the affinity;
  - if a game runs at that moment, `cpu.idle` of the inner `user.slice` and the
    scope's affinity, otherwise "game-time class not observed" (design D12);
  - `cpuset.cpus.effective` and `cpu.weight` of `user-lushtext.slice` and of the
    inner `user.slice`, read from `/sys/fs/cgroup` at that moment;
  - the wall time of `scripts/with-memory-cap.sh 64M true`, five runs on main and
    five on the branch, through `heavy.sh`, with the load. These are
    non-verdict figures for the shim's cost (design D3).
- [ ] 2.7 Run `scripts/heavy.sh --mem none -- make check-agent-helpers`
  (self-tests plus shellcheck) and `scripts/heavy.sh --mem none -- make
  check-policy`. Both must pass. Run the two self-tests once more in isolation
  (`scripts/with-memory-cap.sh --self-test`, `scripts/heavy.sh --self-test`) to
  rule out a flake. Commit (`fix(build): nest heavy scopes under the user
  manager's user.slice`) and push.

## 3. Failing-first cases for (B), then the reference CPU set (design D8-D9)

- [ ] 3.1 In `measure.sh --self-test`, first export `LUSHTEXT_REFERENCE_CPUS`
  set to the self-test's own affinity (always a subset of the allowed set, and it
  keeps a hook's or agent's `taskset` pin), so every existing case passes on
  hosts without 8-23 (CI). Then add the cases below and
  record each red line in `evidence/red.md`:
  - `--print-reference-cpus` with the variable unset prints `8-23`;
  - with the reference set to one allowed CPU `c`, the command's
    `Cpus_allowed_list` is exactly `c`, the log's `before` and `after` lines
    carry `cpus=c reference=c source=override`, and the command sees
    `LUSHTEXT_MEASURE_CPUS=c` and `LUSHTEXT_MEASURE_CPUS_SOURCE=override`;
  - an unavailable reference: the first online CPU outside the allowed set, or
    when there is none, one past the highest possible CPU. Expect exit 78, the
    message naming both sets, and a marker file absent. Run it while a
    background holder keeps LOCK shared, with `--lock-timeout 1`. It must exit
    78 at once, not 75, which proves the refusal comes before any lock (design
    D8);
  - a partial overlap, `c` plus an unavailable CPU: exit 78, marker absent
    (H3, probe 5);
  - a malformed reference, `abc`: exit 2;
  - a change during the run (needs two allowed CPUs, otherwise skip): the
    reference is `c1,c2`, and the command runs `taskset -pc c1 "$PPID"`. Expect
    exit 78, `cpus_changed=c1` and `status=0` in the `after` line, and both locks
    free afterwards;
  - order: `heavy.sh --mem none -- measure.sh -- true` with an unavailable
    reference still exits 2, so the inside-heavy refusal comes first;
  - a change during the lock wait (two allowed CPUs, otherwise skip): a
    background holder keeps LOCK shared, `measure.sh` with reference `c1,c2`
    waits, the test narrows its affinity with `taskset -pc c1 <pid>` and then
    releases the holder. Expect exit 78, the command not run, and both locks
    free;
  - a transient disagreement: with a test hook that changes the first reading
    only (`LUSHTEXT_MEASURE_TEST_FLIP_ONCE`, self-test only), the re-read
    passes and the command runs.
- [ ] 3.2 In the driver's self-test (`make check-responsiveness-lane`), add
  cases for `lane_affinity(inherited, measure_cpus, max_cpus)`:
  - unset: the first 16 CPUs;
  - equal: the set unchanged;
  - mismatch: an error naming both sets;
  - over the cap: an error naming the set and 16.

  Also add cases for `run_metadata`'s `measure_cpus`, `measure_cpus_source` and
  `reference_run`, for the three `render_markdown` affinity labels, and for
  `merge_runs` over two parts whose affinity differs (`affinity: "mixed"`,
  `reference_run: false`, the per-part set in `merged_from`). Record the red
  lines.
- [ ] 3.3 Implement D8 in `measure.sh`. Source `cpu-lists.sh`, add
  `MEASURE_REFERENCE_CPUS_DEFAULT=8-23`, the override, `--print-reference-cpus`,
  and the check order: arguments, then the inside-heavy refusal, then the
  reference check (subset, pin itself, read back), then the locks. Also add the
  after-run read-back with `cpus_changed` and exit 78, the record fields and the
  two exported variables. Update the header with the reference rule, exit 78,
  H3, and why the read-back replaces `taskset`'s status.
- [ ] 3.4 Implement D9. Add the driver subcommand `lane-affinity MAX_CPUS`,
  which replaces `affinity-list`, and use it in `run-responsiveness-smoke.sh`,
  where a failure is a `smoke_fail` before any case runs. Write
  `measure_cpus=` and `measure_cpus_source=` to `environment.txt`, add the three
  `run.json` fields, the `report.md` label, and the `merge_runs` mixed rule.
  Keep `MAX_CPUS=16` and the unpinned path as they are.
- [ ] 3.5 Run `scripts/heavy.sh --mem none -- make check-agent-helpers`,
  `scripts/heavy.sh --mem none -- make check-responsiveness-lane` and
  `scripts/heavy.sh --mem none -- make check-policy`. All must pass. Rerun
  `scripts/measure.sh --self-test` once more in isolation.
- [ ] 3.6 Lane evidence, not a verdict (design "Timing"). Build the measurement
  binary with `scripts/heavy.sh -- make responsiveness-build`. Measure with the
  script, not `make responsiveness-smoke`, which would rebuild inside the
  exclusive lock. Take the exact baseline case name from
  `scripts/run-responsiveness-smoke.sh --list-cases`. From a shell started
  without `taskset` (`taskset -c <all allowed CPUs> bash`), run
  `scripts/measure.sh --label ref-cpus --log <evidence>/measure.log -- env
  RESPONSIVENESS_VARIANTS=plain ./scripts/run-responsiveness-smoke.sh
  --artifact-dir build/smoke/reference-cpus --case <baseline case>`. Then run
  the same with `LUSHTEXT_REFERENCE_CPUS=12-23` into a second artifact
  directory. In `evidence/reference-cpus.md` record:
  - `environment.txt`'s `affinity`, `measure_cpus` and `measure_cpus_source`;
  - `run.json`'s `reference_run`;
  - the `report.md` line;
  - the `measure.log` records with their 1-minute load.

  A run at load ≥ 4 is still valid evidence of what was recorded, so label it
  and do not retry it. Also record one refused run, if a game happens to narrow
  the cpuset during the stage, with its exit 78 message. Otherwise record the
  self-test's refusal case.
- [ ] 3.7 Commit (`feat(build): pin measurements to an explicit reference CPU
  set`) and push.

## 4. Documentation (documentation.md)

- [ ] 4.1 `.agents/rules/build.md`:
  - the Kani "Memory" bullet: the new slice name and its nesting, the inherited
    CPU policy, the placement check, and the oomd risk from design D7;
  - the `make responsiveness-smoke` bullet: under `measure.sh` the lane runs on
    the reference CPU set and refuses a mismatched one; outside it, the existing
    cap.
- [ ] 4.2 `.agents/rules/verification-runs.md`:
  - "Shared-machine agent work": the heavy-work bullet's CPU affinity wording
    (inherited, intersected with the slice's cpuset; the kernel refuses CPUs
    outside it), and one sentence that heavy work runs in the idle class while a
    game runs;
  - the measurements bullet: `measure.sh` pins to the reference CPU set
    (`--print-reference-cpus`, default 8-23, `LUSHTEXT_REFERENCE_CPUS`) and
    refuses with 78 when it is unavailable or changes;
  - "Timing campaigns", a new bullet: a run on a different CPU set (an
    override, a pre-change record without `reference=`, an unpinned lane run)
    is not comparable with the reference and must be labelled with its CPU set
    wherever it is quoted. Interleaved sides must share one set. During a game,
    wait rather than override.
- [ ] 4.3 `docs/runbooks/responsiveness-lane.md`: the affinity paragraph (the
  reference under `measure.sh`, the labels, the mixed merge), and the
  `lane-affinity` subcommand.
- [ ] 4.4 `Makefile`: the comment at the mutants block (the new slice name) and
  the `check-agent-helpers` shellcheck list. `docs/mutation-testing.md`
  ("Local Parallelism"): the new slice name.
- [ ] 4.5 `openspec/specs/widget-test-ci-sharding/spec.md` and
  `openspec/specs/responsiveness-measurement/spec.md` are updated by archive
  from the deltas. Check that no other live spec or rule names the old slice:
  `git grep -n "lushtext-heavy.slice\|lushtext\.slice"`, outside
  `openspec/changes/archive/` and `docs/memorials/`, which stay historical.
- [ ] 4.6 `docs/next/long-line-documents.md`:
  - add the change to the infrastructure follow-ups list beside
    `harden-flatpak-sdk-update-ci`, with its scope, the baseline figures and
    decisions, "Kani: none", and the timing evidence of task 3.6;
  - add a sentence where the reference run's affinity is named (the "E-cores
    8–23, inherited" passages). From this change on, the reference CPU set is
    explicit (`measure.sh --print-reference-cpus`). Earlier verdicts were all on
    8-23, so they stay comparable. Records on 12-23 are game-time runs and not
    reference runs;
  - add the deferral "remove the legacy-slice accounting once no worktree
    branched before the merge SHA remains".
- [ ] 4.7 README, AGENTS.md and `docs/runbooks/README.md` name the wrappers but
  neither the slice nor the reference set. Confirm that no edit is needed, or
  make it, and record which in the commit body.
- [ ] 4.8 Commit (`docs(build): document the nested heavy slice and the
  reference CPU set`) and push.

## 5. Final gates and handoff

- [ ] 5.1 Merge `origin/main` again (never rebase). Resolve conflicts with
  `reduce-build-disk-writes` in `admitted_kib` and the `systemd-run` line: keep
  their `--unit=` and per-scope body, and this change's loop and
  `--exec-in-scope` launch. Also resolve conflicts with
  `quiet-headless-session-services` (session helpers, its ADDED requirement in
  `widget-test-ci-sharding`; no overlap with this MODIFIED one). Rerun tasks
  2.7 and 3.5, and the placement run of 2.6.
- [ ] 5.2 `openspec validate nest-heavy-slices-under-user-slice --strict`.
- [ ] 5.3 After merge, write
  `.claude/handoffs/nest-heavy-slices-under-user-slice/for-cst.md` with the
  merge SHA and how to verify:
  - `systemd-cgls --user-unit user.slice`, or `systemd-cgls
    /user.slice/user-1000.slice/user@1000.service/user.slice`;
  - the path of a running capped scope;
  - `cpuset.cpus.effective` of `user-lushtext.slice`, which must equal the
    inner `user.slice`'s (read both files, no literal);
  - `cpu.weight` of the inner `user.slice`;
  - the absence of `CPUWeight` and `AllowedCPUs` on our units;
  - that `scripts/measure.sh -- grep Cpus_allowed_list /proc/self/status`
    prints the value of `scripts/measure.sh --print-reference-cpus`, and that
    it exits 78 while a game profile is active;
  - that the legacy `lushtext.slice` stays loaded and empty until the next
    login (or `systemctl --user stop lushtext.slice` once its `cgroup.events`
    reads `populated 0`).
