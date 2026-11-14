# Runbooks

A runbook is an ordered procedure that a rule points to: how to run, read or
re-record something. The rule itself (what must hold, when a run is required)
stays in `AGENTS.md` or `.agents/rules/`; a runbook only says how. Update a
runbook in the same change as the procedure it describes. Where a script's own
header or docstring already documents its mechanics, the runbook points there
instead of restating it.

| Runbook | Procedure |
| --- | --- |
| [responsiveness-lane.md](responsiveness-lane.md) | driving and reading `make responsiveness-smoke` |
| [kani-shards.md](kani-shards.md) | recording a Kani shard's budget and fitting a shard that breaks a margin |
| [gtk-toolkit-update.md](gtk-toolkit-update.md) | the GTK axiom steps of a toolkit version change |
| [dev-desktop-staging.md](dev-desktop-staging.md) | what `make run` does to the desktop, and cleaning up after it |
| [profiling.md](profiling.md) | `perf` and `gdb` on the maintainer's workstation |

Mechanics documented by the scripts themselves: `scripts/with-memory-cap.sh`
(memory-capped scopes and admission), `scripts/widget-shards.py` (parallel
widget shards, logs, summary, retries), `scripts/heavy.sh`,
`scripts/measure.sh` and `scripts/wait-for.sh` (the lock protocol), and
`scripts/responsiveness-smoke-driver.py` (the lane's subcommands).
