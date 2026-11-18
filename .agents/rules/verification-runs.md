---
description: How agents share the workstation (heavy work, measurements, waits) and when to run Kani, proof strength, GTK axioms and timing campaigns — only what a change needs, at the point it is needed
---
<!-- global rule: loaded for every request -->

# Verification Runs

Kani shards, proof-strength mutation, GTK axiom samples and timing campaigns
cost minutes to hours each, and a local run that CI repeats buys nothing. This
rule decides **which** of them a change runs and **when**, and "Shared-machine
agent work" below says how every agent runs heavy work, measurements and waits.
The lanes themselves (memory cap, shard table, budgets) are in
[`build.md`](./build.md), "Formal Verification (Kani)" and "GTK Axioms and
Toolkit Updates".

## Kani

Find what a change touches before running anything. A harness "reads" a
module when it calls its code, directly or through a production function it
drives, so look both ways:

- `scripts/kani-shards.py oracle <module>` names the harnesses whose oracle is
  that module (it errors for a module that is no harness's oracle);
- `grep -rln '<module_name>' crates --include=kani_proofs.rs` finds the
  harness files that reach it through another proven module
  (`paragraph_boundary` is also driven by the `buffer_replacement`,
  `highlight_chunking` and `paragraph_profile_watch` harnesses, which `oracle`
  does not list);
- `scripts/kani-shards.py list` gives each harness's shard and gate.

Then:

- **Pull-request shards** (`gate="pull-request"`) run in CI on every pull
  request and push to `main`; those in the `main` ruleset are required to
  merge. Do not run them locally unless the change edits a harness or a module
  one of them reads, and then only the affected shard.
- **Scheduled shards** (`gate="scheduled"`) run only weekly or by dispatch, so
  a local run is the only evidence before merge. When a change edits a harness
  or a module a scheduled shard reads, run **that shard once**, after the code
  is final (not after every edit), detached so an agent interruption cannot
  kill it (`setsid nohup scripts/heavy.sh --envelope 16G -- make kani
  KANI_SHARD=<shard>`; `heavy.sh` also waits for a running timing campaign).
- **No proven module touched:** the spec records "Kani: none (no proven
  module touched)" and no stage runs Kani. Never run every shard locally: a
  full `make kani` belongs to the weekly or dispatched CI lane. A GTK bump does
  not touch the proofs, and a Kani version bump is proved by that lane.
- **Review:** a reviewer reruns a shard only when its own fixes touched that
  shard's harness inputs.
- **Shard budgets:** a new or changed shard records its budget from two CI
  runs (`build.md`, "Shard gates and budgets"). For a pull-request shard the
  PR's own Kani run is one of them. Dispatch `kani.yml`, which runs every
  shard, only for a scheduled shard's figure, once the change's final code is
  pushed. A pending record names its owning change, so the archive cannot
  forget it.

## Proof strength

`make proof-strength` runs only when a change adds or changes harnesses (or
the properties they assert), or when a programme record asks for it. Shard its
Kani arm (`PROOF_STRENGTH_SHARD=k/n`) when the measured peak per CBMC fits the
memory budget, and run it detached.

## GTK axioms

- On a GTK, GtkSourceView, Pango, GLib or Libadwaita version change, run
  `make gtk-axioms` **first**, before anything else (`build.md`).
- Otherwise run only the samples of the axioms the change adds, edits or newly
  relies on (`make gtk-axiom-sample AXIOM=<id> CHECK=1`, or
  `scripts/run-gtk-axioms.sh runtimes --only <ids>` inside SDKs).
- On pull requests CI runs the probes on the shipped toolkit and on the
  GNOME 50 floor, and both jobs are required to merge. Both skip probes marked
  slow (`GTK_LUSH_AXIOMS_INCLUDE_SLOW=0`), so a change that adds or edits a
  slow probe runs its sample locally on both toolkits.

## Timing campaigns

- Every timing verdict goes through `scripts/measure.sh`, which takes the
  measurement lock and records the load and top processes before and after
  (the lock protocol below).
- Under load, queue the campaign instead of starting it
  (`parallel-agent-work.md`, "Measurements").
- An optimisation change measures the cases it affects: four rounds
  interleaved `main` → branch → `main` → branch. An environment change
  (toolkit or runtime) measures the full quick tier the same way.
- Extra rounds (attribution, repeats) run only when a verdict is ambiguous or
  a round is contaminated, and the reason is recorded.
- A campaign longer than about two hours runs detached from any agent, so a
  usage limit or an agent restart cannot kill it; the agent reads its results
  afterwards.
- Heavy work started through `scripts/heavy.sh` waits for a running campaign
  by itself; never bypass the helpers to start heavy work beside one.
- **One CPU set per comparison.** A run on a CPU set other than the reference
  (`scripts/measure.sh --print-reference-cpus`) is not comparable with
  reference runs: a `LUSHTEXT_REFERENCE_CPUS` override, a record from before
  the reference was explicit (no `reference=` in its `measure.sh` record), or
  an unpinned lane run (`report.md` says which). Label it with its CPU set
  wherever it is quoted. The interleaved sides of one verdict share one set.
  During a game, when `measure.sh` refuses the reference, wait for the game to
  end rather than override.

## Checkpoints

Commit and push per task group during implementation, so work survives an
interruption. A resumed stage reads the branch, `tasks.md` and the handoff
directory before redoing anything.

## Shared-machine agent work

Several agents and terminals share one workstation, its CPUs, memory and
disk, across every worktree of this repository
([why these rules exist](../../docs/memorials/shared-machine-lock-incidents.md)).
How parallel changes take implementation slots, queue measurements and merge
is in [`parallel-agent-work.md`](./parallel-agent-work.md).

**The helpers are THE way agents run heavy work, measurements, and waits.**
Each has a `--self-test`; `make check-agent-helpers` (in `make check-policy`)
runs those three, `scripts/with-memory-cap.sh`'s, the CI image lane's
(`scripts/ci-image-widget-test.sh`, [`build.md`](./build.md), "CI image
lane") and `shellcheck`, against temporary locks and private slices only.
Their headers document the mechanics.

```
scripts/heavy.sh [--mem <cap|none>] -- <command> [args...]       # every heavy non-measurement command
scripts/measure.sh [--label L] [--log FILE] [--lock-timeout S] -- <command> ... # every timing verdict
scripts/wait-for.sh (--pid N | --name EXACT_NAME | --file PATH [--nonempty]) [--timeout S]
```

**The lock protocol.** Two files in the **main checkout's** `target/`, shared by
every worktree (resolved from `git rev-parse --git-common-dir`;
`LUSHTEXT_AGENT_LOCK_DIR` overrides it for the self-tests): `INTENT` =
`target/responsiveness-measure.intent` and `LOCK` =
`target/responsiveness-measure.lock`.

- **Heavy work** — `cargo build`/`check`/`clippy`/`test`, `make test`, Kani,
  cargo-mutants, proof-strength, smoke lanes, and `git commit` (its hook runs
  Clippy) — goes through `scripts/heavy.sh`. It takes one place in the
  admission order (a ticket, kept across its retries), waits, holding
  nothing, until `scripts/with-memory-cap.sh --wait-admissible <cap>` would
  admit the cap (default `12G`), probes `INTENT` shared, takes `LOCK` shared
  for the one command, sets its compiler parallelism (`CARGO_BUILD_JOBS` =
  its CPUs up to 16 and 8 front-end threads when it is the only holder of
  `LOCK`, 4 jobs and 1 thread otherwise: [`build.md`](./build.md), "Compiler
  parallelism"), and runs it as a child inside `scripts/with-memory-cap.sh
  <cap>`. That admission is the real one and checks once: if another
  admission took the memory since the probe, it starts nothing, and
  `heavy.sh` releases `LOCK`, waits for memory holding nothing and retries,
  re-reading the parallelism after `LOCK` each time. A command's own exit 75
  is returned unchanged. Its CPU affinity is the inherited one,
  intersected with the slice's cpuset: the capped scope nests under the user
  manager's `user.slice` and inherits the toolboxes' CPU policy, the kernel
  refuses CPUs outside that cpuset, and a self-imposed mask (`taskset`) only
  narrows it. By the host's CPU tooling (CST), heavy work runs on CPUs 12-23
  in the idle class while a game runs. **Memory before
  locks**: no admission waits for memory while its process tree holds
  `LOCK`, except an envelope's admission of its own command's scopes. A
  ticket kept while `heavy.sh` waits for `INTENT` or `LOCK` holds a place in
  the order, not memory, and no measurement waits behind it.
  Admission reserves a scope's cap only while its command runs: a daemon the
  command leaves in the scope (the sccache server its first compile started)
  keeps the scope alive but not the reservation. Scopes are named
  `lushtext-cap-<pid>-<ns>.scope` for this; one an older copy of the script
  started (`run-p<pid>-*.scope`) still counts while anything is in it, and
  `systemctl --user set-property --runtime <scope> MemoryMax=infinity`
  releases it without killing the daemon.
- **Commands that admit their own scopes run in an envelope**:
  `scripts/heavy.sh --envelope <size> -- <command>` reserves `<size>` on the
  host once, and the command runs in a sub-slice of that size, where each of
  its capped calls gets a scope of its own admitted against the envelope
  alone. So it waits only for the same command's earlier scopes, never for
  other agents' memory under `LOCK`. An OOM kill anywhere in the envelope
  prints `OOM-KILL:` and fails the run; a clean run prints
  `heavy: envelope <sub-slice>: peak <MiB> MiB, oom_kill 0`. Size it by the
  inner caps that run at once, and check the size against that peak:

  | Lane | Run it as |
  | --- | --- |
  | `make test`, `make test-widget`, `make test-widget-headless` | `heavy.sh --envelope 16G` (the 16G widget build, then 2G per shard) |
  | `make mutants-smoke`, `mutants-diff`, `mutants-full` | `heavy.sh --envelope <MUTANTS_MEMORY_MAX>` (36G by default) |
  | `make kani` | `heavy.sh --envelope 16G` (`KANI_MEMORY_MAX`; `heavy.sh -- make kani` runs it in place at 12G) |
  | `make proof-strength` | `heavy.sh --envelope 16G` (`PROOF_STRENGTH_MEMORY_MAX`) |

  `--mem none` is only for commands that admit nothing (`git commit`, the CI
  image lane, Docker). Any other capped admission above 1G whose process
  tree provably holds `LOCK` shared is refused with exit 2 and a message
  naming `--envelope`, and the recipes above refuse `heavy.sh --mem none` at
  once through `with-memory-cap.sh --preflight`. A nested `heavy.sh` with a
  cap inside `heavy.sh --mem none` is refused the same way.
- **Admission order is fair.** Waiting admissions are ordered by arrival.
  While the oldest live waiter has the host memory for its cap plus the
  reserve, a later request is admitted only if it also fits beside that
  waiter's cap, so a stream of small requests cannot starve a large one.
  The cost is a head-of-line wait: while a 36G request heads the queue, a
  later one is admitted only while the caps admitted plus it stay within
  12G of the 48G budget. A waiter short of host memory reserves nothing.
  Measurement admissions (`LUSHTEXT_ADMISSION_NOWAIT=1`, or inside a proven
  exclusive `LOCK`) ignore the order and fail fast with 75.
- **Measurements** — the responsiveness lane, interleaved benches, any timing
  verdict — go through `scripts/measure.sh`: `INTENT` exclusively, then `LOCK`
  exclusively, so new heavy starts wait while the running ones drain. Before
  any lock it pins itself, and so the command, to the reference CPU set
  (`--print-reference-cpus`, default `8-23`, overridden by
  `LUSHTEXT_REFERENCE_CPUS`) and reads it back; it exits 78 when the set is not
  available (a game narrows the toolbox cpuset), when the read-back differs, or
  when the set changes during the lock wait or the command, and never measures
  on the intersection. Its records carry `cpus=`, `reference=` and `source=`.
  It gives up after `--lock-timeout` seconds (default 1800), printing every
  holder and exiting 75. It records the load and the top five CPU processes before and
  after, and runs the command with `LUSHTEXT_ADMISSION_NOWAIT=1`. Build the
  measured binary first, with `heavy.sh`: `measure.sh` refuses to run inside
  heavy work (exit 2). Test-suite wall-time comparisons are not verdicts: run
  them through `heavy.sh` and record the load.
- **One lock per command.** Never wrap a whole multi-hour script or agent
  session in one `heavy.sh`; call it per command, so a pending measurement
  runs between steps. A nested `heavy.sh` runs in place only with proof: the
  marker `LUSHTEXT_AGENT_LOCK_HELD=<shared|exclusive>:<pid>` counts only when
  that PID is an ancestor and its fd 7 is still `LOCK`.
- **Never wait for other work while holding a lock**, least of all `LOCK`
  exclusively. `measure.sh` does nothing but run its command.

**Mutation slots never wait.** A local `make mutants-*` run takes its mirror
slots with a non-blocking lock (`.agents/rules/build.md`, "Mutation Testing"):
waiting for a slot held by another agent would mean waiting while holding
`LOCK` shared. Exit status 75 means every slot is busy and names the holders;
retry the command later (or with `MUTANTS_LOCAL_JOBS=1`), never in a loop
inside one `heavy.sh`.

**Commits.** Commit through the protocol: `scripts/heavy.sh --mem none -- git
commit ...` (`heavy.sh` does not pass stdin, so give the message with `-F`), or
by hand `flock -s target/responsiveness-measure.intent true && flock -s
target/responsiveness-measure.lock git commit ...` from the main checkout. A
docs-only commit skips Clippy but still runs `make check-policy`, which builds
crates, so it goes through `heavy.sh` too. The
gate order before a commit is in [`testing.md`](./testing.md).

**Waiting.** Use `scripts/wait-for.sh`, or `tail --pid=<pid> -f /dev/null` / a
loop on `pgrep -x <name>` directly. **Never `pgrep -f`**, and never `pkill -f`
with a pattern that matches your own shell: `-f` matches the waiting shell's own
command line. Prefer `--pid` whenever the PID is known, and wait on processes
you started. `--name` matches the kernel's process name exactly (at most 15
characters). `wait-for.sh` exits 0 when the condition holds, 124 on
`--timeout`, and 2 on a usage error. Its self-test fails if `pgrep -f` or
`pkill -f` appears in `scripts/`, the `Makefile`, `.githooks/`, `.github/` or
`.agents/skills/`, except on a line marked `agent-pgrep-f-ok: <reason>`,
reserved for one-shot snapshots that filter out their own shell — never a wait
loop.

**Profiling.** A profiled measurement still goes through `scripts/measure.sh`,
profiles user space only and launches the target as a child; a run under
`perf` is attribution evidence, never a verdict figure. The commands are in
[`docs/runbooks/profiling.md`](../../docs/runbooks/profiling.md).
