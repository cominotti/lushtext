# Lane evidence on the reference CPU set (task 3.6)

Evidence of what is recorded, not a verdict (design "Timing"): nothing here is
compared with anything.

The coordinator ran both calls on 2026-11-15, on the branch at `dca85e7b`. It
used the commands of the stage's queued script, adding `--lock-timeout 60`.
Each call came from a caller pinned to the upper half of the reference,
`taskset -c 16-23` (a proper subset of `scripts/measure.sh
--print-reference-cpus` = `8-23`), so the caller's mask differed from the
reference:

```
taskset -c 16-23 env scripts/measure.sh --label ref-cpus --log <evidence>/measure.log -- \
    env RESPONSIVENESS_VARIANTS=plain ./scripts/run-responsiveness-smoke.sh \
    --artifact-dir build/smoke/reference-cpus --case baseline/small
taskset -c 16-23 env LUSHTEXT_REFERENCE_CPUS=12-23 scripts/measure.sh --label ref-cpus-override ... \
    --artifact-dir build/smoke/reference-cpus-override --case baseline/small
```

Both exited 0, and the lane reported `status: passed`. The `baseline/small`
case selector runs the six `baseline/*` cases of the quick tier. The game
profile was off for both runs.

## `measure.sh` records (`measure.log`)

| label | record | `cpus=` | `reference=` | `source=` | 1-minute load |
| --- | --- | --- | --- | --- | --- |
| ref-cpus | before 11:16:22 | 8-23 | 8-23 | default | 2.27 |
| ref-cpus | after 11:18:27, status=0, 124.5 s | 8-23 | 8-23 | default | 2.38 |
| ref-cpus-override | before 11:20:03 | 12-23 | 12-23 | override | 1.93 |
| ref-cpus-override | after 11:22:08, status=0, 123.9 s | 12-23 | 12-23 | override | 2.06 |

All loads were below 4, so no run needs a load label.

In the first run, `measure.sh` widened the caller's `16-23` mask to the
reference: `cpus=8-23` is read back from its own affinity. It never measured
on the caller's set. No record carries `cpus_changed=`.

## Lane artifacts

| | `build/smoke/reference-cpus` | `build/smoke/reference-cpus-override` |
| --- | --- | --- |
| `environment.txt` `affinity` | 8-23 | 12-23 |
| `environment.txt` `inherited_cpus` | 16 | 12 |
| `environment.txt` `measure_cpus` | 8-23 | 12-23 |
| `environment.txt` `measure_cpus_source` | default | override |
| `environment.txt` `taskset` | none | none |
| `run.json` `reference_run` | true | false |
| `run.json` `run_id` | rr-20261115T141623Z-toolbx-59123 | rr-20261115T142004Z-toolbx-65239 |

`report.md`, line 5:

- reference: ``affinity: `8-23` (the reference CPU set, pinned by scripts/measure.sh)``
- override: ``affinity: `12-23` (override of the reference CPU set: not comparable with reference runs)``

`taskset=none` in both: the inherited affinity equals the reference, which
fits the 16-CPU cap, so the lane applied no cap of its own. `lane-affinity`
returned the reference unchanged.

## A refused run

No game ran during the stage, so no reference was refused for the host's
reason. Stage 4 recorded two real refusals instead, on 2026-11-15 12:34 -03
(load 2.76, `game.state` absent, toolbox cpuset `0,2-6,8-23`). The
unavailable CPU is the first online CPU outside the cpuset,
`cpu_outside_cpuset` = 1. Each ran
`LUSHTEXT_REFERENCE_CPUS=<ref> scripts/measure.sh --lock-timeout 1 -- echo RAN`:

```
measure: reference CPU set 1 is not available (this cgroup allows 0,2-6,8-23); not measuring elsewhere
measure: a CST game profile narrows the toolbox cpuset while a game runs: wait for it to end, or measure on another set explicitly with LUSHTEXT_REFERENCE_CPUS (recorded as an override, not comparable with reference runs)
ref=1 exit=78
measure: reference CPU set 1,8-23 is not available (this cgroup allows 0,2-6,8-23); not measuring elsewhere
measure: a CST game profile narrows the toolbox cpuset while a game runs: wait for it to end, or measure on another set explicitly with LUSHTEXT_REFERENCE_CPUS (recorded as an override, not comparable with reference runs)
ref=1,8-23 exit=78
```

In both cases the command did not run (no `RAN`), and the exit was 78 at once.
The partial overlap `1,8-23` was refused rather than intersected. The
`measure.sh --self-test` refusal cases cover the same paths, and also the
refusal before any lock (78, not 75, with LOCK held), a change during the lock
wait and a change during the command.
