# Real placement (task 2.6)

Read 2026-11-15 01:52 -03 on the branch's wrapper, from the agent toolbox
(`…/user.slice/libpod-<id>.scope/container`, affinity 8-23 by `taskset`).
Load average 30.6 / 26.3 / 17.3 (other agents' heavy work).

## `scripts/heavy.sh -- sh -c 'cat /proc/self/cgroup; grep Cpus_allowed_list /proc/self/status; nproc'`

Default 12G cap (run with `LUSHTEXT_ADMISSION_NOWAIT=1`, see
`.claude/handoffs/nest-heavy-slices-under-user-slice/stage3-notes.md`):

```
with-memory-cap: MemoryMax=12G MemorySwapMax=0 OOMPolicy=continue in user-lushtext-heavy.slice (48G) for: sh -c cat /proc/self/cgroup; grep Cpus_allowed_list /proc/self/status; nproc
0::/user.slice/user-1000.slice/user@1000.service/user.slice/user-lushtext.slice/user-lushtext-heavy.slice/run-p3400170-i32729972.scope
Cpus_allowed_list:	8-23
16
```

## The widget-shard scope: `scripts/heavy.sh --mem none -- scripts/with-memory-cap.sh 2G sh -c '…'`

The call `widget-shards.py parallel` makes for each shard
(`DEFAULT_SHARD_MEMORY_MAX` 2G, through admission):

```
with-memory-cap: MemoryMax=2G MemorySwapMax=0 OOMPolicy=continue in user-lushtext-heavy.slice (48G) for: sh -c cat /proc/self/cgroup; grep Cpus_allowed_list /proc/self/status
0::/user.slice/user-1000.slice/user@1000.service/user.slice/user-lushtext.slice/user-lushtext-heavy.slice/run-p3402130-i32725923.scope
Cpus_allowed_list:	8-23
```

## The slices at that moment (`/sys/fs/cgroup/user.slice/user-1000.slice/user@1000.service/`)

| cgroup | `cpuset.cpus.effective` | `cpu.weight` | `cpu.idle` |
| --- | --- | --- | --- |
| `user.slice` (inner) | 0,2-6,8-23 | 30 | 0 |
| `user.slice/user-lushtext.slice` | 0,2-6,8-23 | 100 | 0 |

The intermediate `user-lushtext.slice` has the inner slice's effective
cpuset; the cpu controller is not enabled below it, so our scopes share its
one weight-100 share beside the toolbox scopes (design D2).

## Game-time class

`/run/host/run/cpu-setup/game.state` was absent (CST's game profile off), so
the game-time class was not observed (design D12): no game ran during this
stage's runs.

## Rerun after the second merge of `origin/main` (task 5.1, 2026-11-15T02:57:32-03:00)

On `a5bb96c8` (merge of `cfee0c48`), 1-minute load 11.02, game profile off
(`game.state` absent):

```
$ scripts/heavy.sh -- sh -c 'cat /proc/self/cgroup; grep Cpus_allowed_list /proc/self/status; nproc'
0::/user.slice/user-1000.slice/user@1000.service/user.slice/user-lushtext.slice/user-lushtext-heavy.slice/run-p1211798-i34771108.scope
Cpus_allowed_list:	8-23
16

$ scripts/heavy.sh --mem none -- scripts/with-memory-cap.sh 2G sh -c '…'
0::/user.slice/user-1000.slice/user@1000.service/user.slice/user-lushtext.slice/user-lushtext-heavy.slice/run-p1211890-i34684641.scope
Cpus_allowed_list:	8-23
```

| cgroup | `cpuset.cpus.effective` | `cpu.weight` |
| --- | --- | --- |
| `user.slice` (inner) | 0,2-6,8-23 | 30 |
| `user.slice/user-lushtext.slice` | 0,2-6,8-23 | 100 |

The affinity is 8-23 because this stage's session was started under
`taskset -c 8-23` (its brief); the scope keeps that mask, which lies within
the inner slice's cpuset, so the placement check passed.

The shim's cost (the last item of task 2.6) is not recorded here yet: its
`measure.sh` call is queued with the coordinator (`MEASURE-QUEUE.md`), which
writes `shim.log` itself.
