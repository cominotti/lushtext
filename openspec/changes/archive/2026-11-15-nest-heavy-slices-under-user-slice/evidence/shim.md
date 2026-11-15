# The placement shim's cost (task 2.6, last item)

These are non-verdict figures (design "Timing"). They are not compared with a
budget, because no timed path runs a capped scope.

The coordinator ran it on 2026-11-15 11:22:24-03:00 on the branch at
`dca85e7b` as one `scripts/measure.sh --label shim --log <evidence>/shim.log`
call, with `--lock-timeout 60`, from a caller pinned with `taskset -c 16-23`.
Its command was the stage's queued `shim-rounds.sh`:

- four rounds, interleaved main → branch → main → branch;
- each round makes five `/usr/bin/time -f %e bash <copy> 64M true` runs;
- main's copy is `evidence/main-with-memory-cap.sh` (`git show
  origin/main:scripts/with-memory-cap.sh`, byte-identical to the copy the
  script ran);
- the branch's copy is `scripts/with-memory-cap.sh`.

A 64M cap skips admission and starts one real scope, so each run is one scope
start.

## The `measure.sh` records (`shim.log`)

- before: `cpus=8-23 reference=8-23 source=default`, 1-minute load 2.04;
- after: `status=0 seconds=0.589 cpus=8-23 reference=8-23 source=default`,
  1-minute load 2.20.

The load was below 4, so the call needs no load label. `measure.sh` widened
the caller's `16-23` mask to the reference.

## Wall times (s; `/usr/bin/time` resolution 0.01 s)

| round | side | runs | median |
| --- | --- | --- | --- |
| 1 | main | 0.02 0.01 0.01 0.01 0.01 | 0.01 |
| 2 | branch | 0.03 0.03 0.03 0.04 0.03 | 0.03 |
| 3 | main | 0.02 0.01 0.01 0.01 0.01 | 0.01 |
| 4 | branch | 0.03 0.03 0.03 0.03 0.04 | 0.03 |

- main: median 0.01 s, mean 0.012 s over 10 runs;
- branch: median 0.03 s, mean 0.032 s over 10 runs.

The shim adds about 20 ms per scope start, at the timer's 10 ms resolution.
That time covers three things: the second `bash` that the scope runs first
(`--exec-in-scope`) sourcing `cpu-lists.sh`, one `systemctl --user show`
call, and the cgroup and cpuset reads before the command execs. The cost is
paid once per capped command (a build, a test shard, a Kani shard), whose
own runtime is seconds to hours.

Stage 4 checked that both copies start a real scope in their own slice, so
main's lower figure is not a run that started nothing. Run 2026-11-15 12:33
-03, `bash <copy> 64M sh -c 'sed -n "s/^0:://p" /proc/self/cgroup'`:

```
main:   …/user@1000.service/lushtext.slice/lushtext-heavy.slice/run-p220324-i42106155.scope                       wall=0.03 exit=0
branch: …/user@1000.service/user.slice/user-lushtext.slice/user-lushtext-heavy.slice/run-p220336-i42106158.scope wall=0.03 exit=0
```

This single spot check was not interleaved, at a 1-minute load of about 3-4,
and is no figure. Only the interleaved figures above count.
