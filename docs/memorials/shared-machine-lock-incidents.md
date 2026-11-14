# Shared-machine lock incidents

Why agents run heavy work, measurements and waits only through
`scripts/heavy.sh`, `scripts/measure.sh` and `scripts/wait-for.sh`. The rules
are in [`.agents/rules/verification-runs.md`](../../.agents/rules/verification-runs.md),
"Shared-machine agent work".

## The two failures that shaped the protocol

Several agents and terminals share one workstation, its CPUs, memory and
disk, across every worktree of this repository. Two failures shaped the rules.
Agents wrote wait loops such as
`until ! pgrep -f "cargo build -p foo"; do sleep 15; done`: `pgrep -f` matches
whole command lines, the waiting shell's own command line contains the
pattern, so the loop matched itself and never ended (some ran 32 hours). And an
agent took the measurement lock exclusively and then waited, inside it, for
other work with such a loop, deadlocking two agents for three hours.

## Rev 1 of the lock protocol

Rev 1 took `LOCK` alone: overlapping shared holders kept the exclusive request
of a measurement waiting forever. Rev 2 makes a measurement take `INTENT`
exclusively first (writer preference), so new heavy starts wait while the
running ones drain.

## The whole-script hold

One whole-script shared hold blocked a measurement and a commit for 28+
minutes, which is why `heavy.sh` is taken per command.

## The stale marker

A bare or stale `LUSHTEXT_AGENT_LOCK_HELD` marker (carried by a measured
command's children, a leftover daemon, a copied environment) once let heavy
work run in place in the middle of a measurement. The marker is now honoured
only when its PID is an ancestor of the nested call and its fd 7 is still
`LOCK`.

## The background-job SIGINT

The memory-cap wrapper `exec`s `systemd-run`; run as a background job, an
earlier form once made every capped process ignore SIGINT.

## The per-command cap alone

On 2026-10-20 three agents each held to 12G, plus a 16G Kani shard, added up
past the desktop's headroom, which is why every capped scope runs inside the
host-wide `lushtext-heavy.slice` with admission. The same day `cargo-mutants`
runs at `MUTANTS_LOCAL_BUILD_JOBS=8 MUTANTS_LOCAL_TEST_THREADS=12`, beside a
second one and a Kani shard, under a 12G cap, were OOM-killed and raised a
CRITICAL gsd-housekeeping notification, which is why the scope runs with
`OOMPolicy=continue` (see `docs/mutation-testing.md`).
