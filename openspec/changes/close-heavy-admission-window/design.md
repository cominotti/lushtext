# Design

## Context

How the helpers run on `main` (`c16d6fdc`):

- **`scripts/heavy.sh [--mem <cap|none>] -- <cmd>`** (non-nested path):
  1. `with-memory-cap.sh --wait-admissible <cap>` polls every 15 s, holding
     nothing, until `fits`.
  2. `flock -s 6` on `INTENT`, then released at once.
  3. `flock -s 7` on `LOCK`.
  4. `LUSHTEXT_ADMISSION_NOWAIT=1 with-memory-cap.sh --wait-admissible <cap>
     7>&-`. Exit 75 releases `LOCK` and loops back to step 1.
  5. `heavy_export_parallelism $(heavy_parallelism <cpus>
     <heavy_lock_shared_holders> <cap>)`, read now that `LOCK` is held (PR
     #114).
  6. Signal traps, then the marker
     `LUSHTEXT_AGENT_LOCK_HELD=shared:<heavy pid>`.
  7. `agent_run_child with-memory-cap.sh <cap> <cmd>`. The child holds
     neither lock fd, and its exit status is returned.
- **`with-memory-cap.sh <cap> <cmd>`**:
  - `none` execs the command. A nested call (`LUSHTEXT_MEMORY_CAP_KIB` set)
    runs in place, and caps of 1G or less skip admission.
  - Otherwise `budget_setup`, `set-property --runtime` of the slice budget,
    then `admit`. That loop takes the admission lock (fd 9) and checks
    `fits`. On a misfit it releases the lock and sleeps 15 s. Under
    `LUSHTEXT_ADMISSION_NOWAIT=1` it exits 75 instead.
  - On admission the lock passes to a detached watcher, which releases it
    once the new scope's PID is in the slice. Then the wrapper `exec`s
    `systemd-run --scope --unit=lushtext-cap-$$-<ns>`, whose first process
    runs `--exec-in-scope` (the placement check) and execs the command.
- **`fits`** = `admitted_kib + cap <= budget` and `MemAvailable >= cap +
  reserve`.
- **`admitted_kib`** sums `memory.max` of the `*.scope` children of the heavy
  slice and of its legacy slice. A `lushtext-cap-<pid>-<ns>.scope` counts
  only while `<pid>` is in it (D10 of `reduce-build-disk-writes`); any other
  name counts while it exists. The probe (`--wait-admissible`) and `admit`
  both call `fits`, so they count the same thing.
- **`measure.sh`** takes `INTENT` and then `LOCK`, both exclusively, and
  exports `LUSHTEXT_ADMISSION_NOWAIT=1` to its command.
- **Commands that admit their own scopes:**
  - `widget-shards.py parallel` (`make test`, `make test-widget`): a 16G
    build scope, then one 2G scope per shard, three shards today;
  - `make mutants-*`: 36G;
  - `make kani`: 16G, `--address-space`;
  - `make proof-strength`: 16G.

  Each admits through the wrapper from inside the command. The rules run the
  first two as `heavy.sh --mem none`. `heavy.sh -- make kani` runs Kani in
  place at the default 12G, because the outer scope exports
  `LUSHTEXT_MEMORY_CAP_KIB`.

### Baseline

These are records and a read-only snapshot. No timing is involved, and the
helpers' behaviour is proved by their self-tests.

| Fact | Source |
| --- | --- |
| A default-cap `heavy.sh` passed both probes, then printed `waiting for memory: 48 GiB of 48 GiB admitted …, 61 GiB available, need 12 GiB plus a 16 GiB reserve` while holding `LOCK` shared. Another change's `measure.sh` waited behind about eight shared holders. The 36G scope in front came from `heavy.sh --mem none -- make mutants-smoke`. | `.claude/handoffs/nest-heavy-slices-under-user-slice/stage3-notes.md` (2026-11-15 01:37) |
| A 36G mutants scope waited about an hour, after D10 (`a19df33d`, 2026-11-15 00:10). Its log, printed once at the first poll: `42 GiB of 48 GiB admitted, 58 GiB available, need 36 GiB plus a 16 GiB reserve`. The handoff attributes the hour to a stream of small admissions. | `.claude/handoffs/reduce-build-disk-writes/lane-11.3/run-36G-starved.log` (04:37); `implementation-handoff.md`; lane case 11.3 |
| The other seven 36G waits in that change's logs read 54-62 GiB available against the 52 GiB needed, with 36-48 GiB admitted: the **budget** was the binding dimension, not physical memory. They are not starvation evidence. Four read "36 of 48", one 36G holder; three of those are from before `a19df33d` (the sccache pin D10 fixed), and the 02:35 `main` round counted with `main`'s pre-D10 copy. | `.claude/handoffs/reduce-build-disk-writes/{warm,rounds,rounds-nonverdict}/*` (2026-11-14 23:39 to 2026-11-15 02:54) |
| 2026-11-17 snapshot: budget 48G. Three live scopes of 16G, 12G and 16G (44 GiB admitted) used 2.7, 1.2 and 5.2 GiB. MemAvailable 40 GiB, load 5.9. | `systemctl --user show`, `/sys/fs/cgroup`, `/proc/meminfo` at stage 1 |
| Widget shard peak 864 MiB, cap 2G; widget build cap 16G at `--build-jobs 4`. The whole workspace cold at 16 jobs peaks at 9.5 GiB. | Makefile `WIDGET_*` comments; `build.md`, "Compiler parallelism" |

Host behaviour the envelope relies on. It was probed by this change's spec
review on 2026-11-17 (kernel 7.2, systemd 262), in a private slice
`user-lushtext-heavy-selftest-r<pid>.slice` and its sub-slice
`…-r<pid>-env1t2.slice` (MemoryMax 512M, one `sleep` scope), then stopped and
reverted, with no drop-in left. It is recorded in the wrapper's header as H4
to H6, next to H1 to H3, and the self-test re-probes it:

- **H4.** A `<stem>-env<P>t<ns>.slice` nests directly under `<stem>.slice`
  (H1). Its scopes are not `*.scope` children of the parent slice (0 there).
- **H5.** `systemctl --user revert` of a sub-slice holding a live scope
  removes its runtime drop-ins, and `memory.max` reads `max`. The scope keeps
  running.
- **H6.** The sub-slice's `memory.peak` and the `oom_kill` line of
  `memory.events` can be read while a scope is in it. Its cgroup outlived the
  scope's exit by at least 0.5 s. The sentinel (D2) does not rely on that.

Each slice also carries the distribution's
`/usr/lib/systemd/user/slice.d/10-oomd-per-slice-defaults.conf` drop-in
(systemd-oomd; see "Risks").

The app already uses the same fairness pattern for its own loads:
`model::file_load` has
`capacity_blocked_oldest_is_not_starved_by_later_small_requests`.

## Goals / Non-Goals

**Goals:**

- No admission ever waits for memory while its process tree holds `LOCK`
  shared, except an envelope's admission of its own command's scopes (D3).
- A misfit found after `LOCK` makes `heavy.sh` release `LOCK` and wait holding
  nothing, with the same wait as step 1.
- The `--mem none` inner path is covered: a command that admits its own scopes
  either declares an envelope, or is refused at once.
- A large request cannot be starved on the slice budget by later small ones.
- Kept as they are:
  - `LUSHTEXT_ADMISSION_NOWAIT=1`'s fail-fast exit 75;
  - D10 (a leftover daemon keeps no admission);
  - legacy-slice counting;
  - one decision function for the probe and the admission;
  - the parallelism read after `LOCK` on every attempt;
  - `heavy_parallelism`'s table and the rule that a cap below 12G gets
    4 jobs.

**Non-Goals:** see the proposal ("Non-goals"). Also: no change to
`measure.sh`'s lock order, to `agent_lock_inherited`'s proof, or to
`with-memory-cap.sh`'s scope properties and placement check.

## Decisions

### D1. heavy.sh's admission after LOCK is the real one, done once

After `LOCK`, `heavy.sh` starts the child with `LUSHTEXT_ADMISSION_ONCE=<report
file>` (a private temp file owned by this `heavy.sh`). With that set, the
wrapper's admission checks once, under the admission lock.

- **Admission lock busy:** it waits for the admission lock at most 2 s, as a
  NOWAIT admission does, and a lock still busy counts as a misfit. The
  blocking `flock 9` of `admit` could otherwise wait under `LOCK` for as long
  as another admission's watcher holds it (up to 10 s,
  `release_admission_once_scoped`), or for an old wrapper copy.
- **Misfit:** the wrapper writes one line starting `misfit: ` (the waiting
  message) to the report file, and exits 75. No scope and no command have
  started.
- **Fit:** it admits and starts the scope exactly as today. It unsets
  `LUSHTEXT_ADMISSION_ONCE` before the `exec`, so nothing inside the command
  inherits it.

`heavy.sh` treats "75 **and** a `misfit: ` line in the report" as a misfit.
The report is not otherwise empty in every case, because an envelope's
placeholder writes its `envelope: ` line to the same file (D2), and the
envelope lanes are the ones whose commands exit 75 (`run-mutants.sh`, slots
busy). On a misfit `heavy.sh` closes fd 7, removes its traps and the marker,
truncates the report, sleeps one second, and goes back to step 1. Exit 75
without a `misfit: ` line is the command's own status (mutation slots busy, a
measurement inside) and is returned unchanged. Each attempt
recomputes the parallelism after `LOCK`, as #114 requires, and re-arms the
signal traps, because `agent_run_child` clears them. The step-4 NOWAIT
re-probe is removed: the admission itself is now the check, so there is no
window left between a check and the admission.

The one-second pause is there for a real race. Step 1 normally sees the same
misfit and waits its 15 s polls, but it may see a slot that frees and refills
at once. The pause keeps that from becoming a hot loop of `LOCK` takes.

If the caller set `LUSHTEXT_ADMISSION_NOWAIT=1` (the workaround recorded in
stage3-notes, and any `heavy.sh` started inside a measurement), a misfit exits
75 and does not loop. Step 1 already exits 75 there.

**Rejected alternative: reserve before `LOCK`, then hand the reservation to the
scope.** A reservation held while `heavy.sh` waits for `INTENT` or `LOCK`
reserves memory while waiting for a lock, which the rule forbids
(`verification-runs.md`: "never reserves memory while waiting for a lock"). It
also does harm: behind a pending measurement, queued reservations would make
the measurement's own NOWAIT admissions fail with 75, and the memory would sit
idle for the measurement's whole run.

**Rejected alternative: make `admit` itself NOWAIT under heavy.sh** (export
`LUSHTEXT_ADMISSION_NOWAIT=1` to the child). NOWAIT leaks into the command, so
an envelope's inner admissions and nested calls would fail instead of waiting
for their siblings. Exit 75 alone also cannot tell a misfit from the command's
own 75.

### D2. Envelopes: `heavy.sh --envelope <size>` and `with-memory-cap.sh --envelope <size>`

An envelope is admitted like a scope of `<size>`: queue, `fits`, the
once/NOWAIT modes, the admission lock. After that, three units exist.

- **A placeholder scope** `lushtext-cap-<P>-<ns>.scope` sits directly in the
  heavy slice, with `MemoryMax=<size>` and `MemorySwapMax=0`. `P` is its own
  first process. That is a small `setsid` shell started as
  `bash -c 'exec systemd-run --scope --unit=lushtext-cap-$$-…'`. It waits for
  the envelope's root command to exit (`tail --pid`), then reverts the
  sub-slice's runtime properties, then exits. The `admitted_kib` of `main`,
  and of every wrapper copy since D10, counts this placeholder by the
  existing rule: a `lushtext-cap-<pid>-<ns>` scope whose PID is still in it.
  Worktrees that have not merged this change therefore still see the
  envelope.
- **A sub-slice** `<heavy-stem>-env<P>t<ns>.slice` nests under the heavy
  slice (H1 of `heavy-work-placement`), with `MemoryMax=<size>` and
  `MemorySwapMax=0`. The kernel holds the whole command, and every scope it
  admits, to `<size>`.
- **A root scope** `lushtext-envroot-<pid>-<ns>.scope` sits in the sub-slice
  with no `MemoryMax`, plus `OOMPolicy=continue` and the usual
  `--exec-in-scope` placement check. The wrapper `exec`s into it, as today,
  so signals and the exit status pass straight through. The root command does
  **not** get `LUSHTEXT_MEMORY_CAP_KIB`, so inner capped calls start their
  own scopes instead of running in place. `widget-shards.py` therefore does
  not print its "every shard shares that one cap" warning, and each shard's
  `memory.events` stays its own.

The admission lock is released once the placeholder is in the heavy slice,
which is the existing `release_admission_once_scoped`, with the placeholder's
PID.

The placeholder is not counted twice: the sub-slice is a `*.slice`, and the
host-level count reads only `*.scope` children. The scopes inside the
sub-slice are not children of the heavy slice.

**Envelope peak and OOM kills.** An OOM kill in the root scope would otherwise
go unreported: only shard scopes print `OOM-KILL:`, so a killed nextest
process would read as an ordinary test failure. So a sentinel scope in the
sub-slice (no `MemoryMax`, one process that exits when the placeholder exits)
keeps the sub-slice alive until the placeholder has read its `memory.peak` and
`memory.events` `oom_kill`, which cgroup v2 counts over the whole sub-tree.
The placeholder writes them to `heavy.sh`'s report file (`envelope: peak=<MiB>
oom_kill=<n>`, a prefix distinct from the misfit line). `heavy.sh` waits a
bounded few seconds for that line after the child exits. When `oom_kill > 0`
it prints `OOM-KILL: …` and exits non-zero, even if the command exited 0,
as `run-widget-tests.sh` does for a shard. The self-tests check the reading
on a normal run, and the decision on fixture lines. They never provoke a real
OOM kill, because one in a user scope can raise a GNOME notification.

The sentinel scope gets `OOMPolicy=continue`, like every scope the wrapper
starts, so that an OOM kill inside the sub-slice never marks a unit
`oom-kill`. That marking is the result gsd-housekeeping turns into a desktop
notification.

**D10 holds.** When the root command exits, `tail --pid` returns, the
placeholder reverts the sub-slice and exits, and the envelope's admission
ends. That happens a moment after the root, once the placeholder has written
its `envelope: ` line, so a check that the reservation has ended first waits
for that line. An sccache server left in the sub-slice keeps it alive, but
holds no admission and no `MemoryMax`. The placeholder also removes the
sub-slice's queue directory (D5).

**A SIGKILLed placeholder.** Its PID leaves its scope, and the reservation
ends with it. Its sub-slice's runtime drop-in and queue directory would then
stay until logout, because each sub-slice name is unique and nothing reverts
it again. So every envelope admission, under the admission lock, sweeps
first. It reverts each `<heavy-stem>-env<P>t<ns>.slice` drop-in of its own
heavy slice whose placeholder no longer holds `P` (no live
`lushtext-cap-<P>-*.scope` with `P` in its `cgroup.procs`), and removes
that sub-slice's queue directory. The self-test's cleanup and leftover check
cover the envelope sub-slices too (D7).

**Nested envelopes.** `with-memory-cap.sh --envelope <size>` runs its command
in place when it is already inside a capped scope (`LUSHTEXT_MEMORY_CAP_KIB`
set) or inside an envelope (D3's cgroup proof). The outer scope or envelope
then bounds it and reports its OOM kills. Inside a measurement (a proven
exclusive `LOCK`) it is admitted at the host level, checking once (D4).
`heavy.sh`'s nested in-place path for `--envelope` does not `exec`. It runs
the wrapper as a child with a report file (`agent_trap_signals`,
`agent_run_child`) and applies the same OOM decision, so an envelope inside a
measurement still fails on an OOM kill.

`heavy.sh --envelope <size>` runs the D1 path with `with-memory-cap.sh
--envelope <size> <cmd>` as its child. `heavy_parallelism` receives `<size>`
as its cap. That bounds what `--mem none` left uncapped (the non-widget build
of `make test`): alone, 16 jobs and 8 front-end threads, whose measured
9.5 GiB peak fits a 16G envelope; under 12G, 4 jobs and 1 thread, as today.
New table rows pin this. `--envelope` and `--mem` are mutually exclusive.

**Per-lane envelope sizes.** Each size is the largest set of inner caps that
can run at once.

| Lane | Inner admissions | Envelope | Reserved compared with `main` |
| --- | --- | --- | --- |
| `make test`, `make test-widget` | 16G build (`WIDGET_BUILD_MEMORY_MAX`), then 3 x 2G shards (`WIDGET_SHARD_MEMORY_MAX`; 864 MiB measured peak) | 16G, provisional | 16G for the whole run. `main` holds 0 in the non-widget phase (uncapped), 16G for the build and 6G for the shards |
| `make mutants-*` | `MUTANTS_MEMORY_MAX` (36G default) | the same value | unchanged |
| `make kani` | `KANI_MEMORY_MAX` 16G, `--address-space` | 16G | `heavy.sh -- make kani` on `main` runs in place at 12G; the envelope gives Kani its 16G |
| `make proof-strength` | `PROOF_STRENGTH_MEMORY_MAX` 16G | 16G | unchanged |

The `make test` size is provisional. The sub-slice now also bounds the
non-widget phase, which `main` runs uncapped, and 9.5 GiB is a cold-build
peak, not a peak of the tests running. Lane case 4.3 records the sub-slice's
`memory.peak` and `oom_kill` for a whole `make test`. The documented size is
then set to cover that peak, at a power of two of at least 16G; a peak that
does not fit 16G raises it in the same change, with the figure recorded.

If inner caps add up to more than the envelope, those scopes run one after
another within it (D3). An inner cap larger than the envelope fails at
`budget_setup` with exit 2 ("cap exceeds the host-wide budget"), which
names the envelope's size. It cannot wait forever.

**Rejected alternative: `heavy.sh --mem <size>` for these lanes.** Inner calls
would run in place in one scope. Shards would lose their own scopes, and with
them their OOM-kill attribution (`run-widget-tests.sh`,
`report_memory_scope`).

**Rejected alternative: let the inner admission fail with 75 and let
`heavy.sh` retry the command.** `make test` admits its widget build only after
the whole non-widget phase, so a retry would redo minutes of finished work.
Mutants would work, since its admission comes first, but that is the
exception.

**Rejected alternative: drop `LOCK` while an inner admission waits.** The
command's other parts, such as running shards, would then be heavy work
outside `LOCK`, and a measurement could start beside them.

### D3. Inside an envelope, admission is against the envelope

A capped call is inside an envelope when its own cgroup path
(`/proc/self/cgroup`) has, directly below the heavy slice's `ControlGroup`, a
component named `<heavy-stem>-env<digits>t<digits>.slice`. The proof is the
cgroup path, never an environment variable: a stale marker once let heavy work
run in place inside a measurement (memorial, "The stale marker"). The
envelope's admission then works as follows:

- the slice is that sub-slice, and the budget is its `memory.max`, read from
  `/sys/fs/cgroup` rather than an environment value;
- the wrapper's per-call `set-property --runtime <slice> MemoryMax=<budget>`
  is skipped. Done there, it would reset the sub-slice to the host default
  (48G) and remove the envelope's bound. A self-test checks that
  `memory.max` is unchanged after an inner admission;
- no legacy slice is counted, since no old copy can start a scope in an
  envelope;
- the MemAvailable and reserve check is skipped. The envelope's admission
  checked `<size> + reserve` once, and a scope admitted at 36G does not
  re-check MemAvailable as its processes grow either;
- it may wait. What it waits for is only its own command's earlier scopes,
  which is part of the command running, not memory held by other agents;
- NOWAIT does not apply inside an envelope, for the same reason. An envelope
  started inside a measurement fails fast at its own admission, at the host
  level.

The queue (D5) applies per slice, so an envelope has its own queue. An inner
admission never consults the heavy slice's queue. If it did, it would
deadlock: an older host-level head waits for the envelope's reservation to
end, while the envelope's own scopes wait behind that head.

### D4. No other admission under a shared LOCK

Before admitting, the wrapper sources `scripts/agent-locks.sh` and runs
`agent_lock_inherited`. That is the existing proof: the marker's PID is an
ancestor, and its fd 7 is `LOCK`. `LUSHTEXT_AGENT_LOCK_DIR` is honoured, so
self-tests use their private locks. The check runs only for a cap above 1G,
outside an in-place nested call, with scopes available, and only when
`LUSHTEXT_AGENT_LOCK_HELD` is set. Without the marker there is nothing to
prove, and `agent_lock_paths` (which creates the lock directory and runs
`git`) is not called. Then, by mode:

- **`exclusive`** (inside a measurement, outside an envelope; inside one,
  D3 applies): check once, exit 75 on a misfit,
  whether or not NOWAIT is set. This is the measurement contract, now
  independent of the variable.
- **`shared`**: allowed only with `LUSHTEXT_ADMISSION_ONCE` (D1) or inside an
  envelope (D3). Otherwise exit 2, before any admission, with
  `with-memory-cap: refusing to admit <cap> under LOCK held shared by
  heavy.sh (pid N): run the command as scripts/heavy.sh --envelope <size> --
  …`.
- **No proof** (no enclosing helper): unchanged. A plain caller may wait,
  because it holds no lock.

The refusal uses exit 2 rather than 75 because 75 means "busy, retry later" to
`run-mutants.sh` and to the agents, and this is a usage error that retrying
does not fix. A nested `heavy.sh` with a cap inside `heavy.sh --mem none` is
refused the same way. Inside `--mem <cap>` it still runs in place, and inside
an envelope it is admitted against the envelope.

**Rejected alternative: admit if it fits at once, fail 75 otherwise.** The
failure would be random, and old `--mem none` call sites would keep working
until memory runs short. The refusal is deterministic, so they fail the first
time and name their fix.

### D5. Fair order: tickets, and the oldest waiter's budget reservation

- **Tickets.** A waiting admission takes a ticket: a file
  `<ns>-<pid>` holding `<cap KiB> <start time>`, in
  `${XDG_RUNTIME_DIR:-/tmp}/lushtext-heavy-admission.queue/<slice>/`. It is
  created under the admission lock. A ticket counts while its owner PID lives
  with the same `/proc/<pid>/stat` start time. Dead tickets are swept under
  the lock.
  - `heavy.sh` takes one ticket per invocation through
    `with-memory-cap.sh --ticket <cap> <owner-pid>`. It keeps the ticket
    across D1 retries, so it keeps its age, and passes it as
    `LUSHTEXT_ADMISSION_TICKET` to the probe and to the once-admission. The
    ticket is removed on admission, and by `heavy.sh`'s `EXIT` trap. Like
    `LUSHTEXT_ADMISSION_ONCE`, the variable is unset before the `exec`, so
    no inner call inherits another process's ticket.
  - A blocking `admit` outside `heavy.sh` takes its own ticket and removes it
    before the `exec`.
- **Verdict.** One pure function decides for the probe and the admission
  alike:

  `admission_verdict <admitted> <available> <budget> <reserve> <cap> [<head cap>]`

  The head is the oldest live ticket older than the caller's own. Admit iff:
  - `admitted + cap <= budget`, and
  - `available >= cap + reserve` (skipped inside an envelope), and
  - either there is no head, or the head is short of physical memory
    (`available < head cap + reserve`), or `admitted + head cap + cap <=
    budget`.

  It is exposed as `--admission-verdict`, for fixtures, like
  `--placement-verdict`.
- **Bypass.** NOWAIT and exclusive-mode admissions take no ticket and ignore
  the head. A measurement owns the machine, no heavy waiter can start during
  it, and holding its admission behind a ticket would only make it fail with
  75.

- **Queue directories.** A queue directory lives as long as its slice. The
  placeholder removes its envelope's directory (D2), and the self-test removes
  its private slices' directories in its `EXIT` trap. The leftover check
  covers them too (D7).
- **Poll interval.** A waiting admission and the probe sleep 15 s between
  checks, as today. `LUSHTEXT_ADMISSION_POLL_SECONDS` overrides that for the
  self-tests, which set it to 0.2 s, so that a case can see a waiter admitted
  after the scope ahead of it ends without a 15 s sleep in every commit hook.
  It is not documented as an operator knob.
- **The rule "never reserves memory while waiting for a lock".** A ticket
  that `heavy.sh` keeps while it waits for `INTENT` or `LOCK` holds a place
  in the order. It is not a reservation of memory. It holds back no
  measurement, because NOWAIT and exclusive admissions ignore the head. The
  other waiters it holds back would wait for `INTENT` anyway. Only an
  admission that holds no lock at all can wait behind it (an unwrapped run's
  inner scope), and the head is admitted next in any case. The rules text is
  reworded to say this (task 5.1).

This is EASY backfill: only the head reserves. A small request still runs
beside a large waiter whenever both fit, so the slice stays used. The head is
admitted as soon as the work admitted before it frees the budget, because
nothing admitted after it may take the space it needs. Each waiter becomes the
head in turn, so none starves on the budget.

The reservation holds only while the head has the memory. The baseline shows
the budget was the starving dimension, and today's snapshot (40 GiB available)
shows why a head that is short of physical memory must not reserve. A 36G head
would then hold every other agent to 12G of admitted caps for as long as the
memory stays short, and that memory is not ours to queue for.

**Rejected alternative: strict FIFO.** It leaves budget idle behind a large
head even when a small request would fit beside it.

**Rejected alternative: aging with a time constant.** It needs a tuned
constant and still starves the head until that time passes. Head
reservation needs neither.

### D6. What stays

- `LUSHTEXT_ADMISSION_NOWAIT=1`: fail fast with 75 at every host-level
  admission, and bypass the queue.
- D10's rule in `admitted_kib`.
- Legacy-slice counting at the host level.
- `--wait-admissible` and the admission share `admission_verdict`.
- `heavy_lock_shared_holders` is read after `LOCK` on every attempt.
- The `heavy_parallelism` table: all its rows stay, and envelope rows are
  added.
- The 64M and 1G-or-less skip.
- The scope properties and the placement check.
- `measure.sh` is not changed.

### D7. Self-tests: failing first, private only

Every new case uses private lock directories (`LUSHTEXT_AGENT_LOCK_DIR`), the
private per-run slices `user-lushtext-heavy-selftest-<pid>.slice`, and their
envelope sub-slices. None of them touches the real slice budget.

- The wrapper self-test sheds `LUSHTEXT_AGENT_LOCK_HELD`, as `heavy.sh`'s
  does. Otherwise the commit hook, which runs it under a genuinely held shared
  `LOCK`, would hit D4's refusal.
- The cleanup and the leftover-drop-in check widen from
  `*selftest-<child>.slice.d` to `*selftest-<child>*.slice.d`, which covers
  the envelope sub-slices, a SIGKILLed owner included. The leftover check
  also fails on a queue directory of those slices.
- `heavy.sh --self-test` today runs its capped case against the real slice
  (a 64M cap, with no budget override). Every new `heavy.sh` case that sets
  `LUSHTEXT_HEAVY_MEMORY_MAX` or `LUSHTEXT_MEMORY_RESERVE` first exports a
  private `LUSHTEXT_HEAVY_SLICE=user-lushtext-heavy-selftest-h<pid>.slice`,
  because the wrapper applies the budget to its slice with `set-property`.
  `heavy.sh`'s `EXIT` trap stops and reverts those slices (with their envelope
  sub-slices) and removes their queue directories, as the wrapper's does. At
  the end it checks that the real slice's `MemoryMax` is unchanged and that
  no drop-in or queue directory of its private slices is left.
- Self-tests set `LUSHTEXT_ADMISSION_POLL_SECONDS=0.2` (D5).
- The case for a busy admission lock (D1) holds a private one, named by
  `LUSHTEXT_ADMISSION_LOCK` (default
  `${XDG_RUNTIME_DIR:-/tmp}/lushtext-heavy-admission.lock`, as today), and
  never the real one: holding the real lock for seconds would hold up every
  agent's admissions. Overriding `XDG_RUNTIME_DIR` instead would break
  `systemctl --user`. Cases that use the private lock use private slices
  only, so they need no serialization with other agents.

The race in (1) is made deterministic, with no timing involved, by a
`systemctl` shim on `PATH`. It answers only `show -p ControlGroup --value
<private slice>`. When the calling script's command line is
`--wait-admissible`, it returns an empty private slice; for the real admission
it returns a private slice holding a 2G sleeper. Every other call passes
through to the real `systemctl`. This reproduces "the state changed between
the check and the admission" exactly. On `main` the blocking admission then
holds `LOCK` shared while it waits; after D1, `heavy.sh` keeps giving `LOCK`
back.

## Risks / Trade-offs

- **Envelope reservations are coarse.** A lane reserves its whole size for the
  whole run (proposal, "Loss compared with `main`"). Accepted: today the same
  phases either reserve their peak or run uncapped.
- **The refusal lands late.** Under `--mem none`, `make test` would reach its
  first inner admission only after the whole non-widget phase. The admitting
  Makefile recipes therefore start with a cheap preflight
  (`with-memory-cap.sh --preflight <cap>`): it applies D4's check without
  admitting anything, and exits 2 at once in the same case. The preflight
  checks the cap the recipe will actually admit. It is skipped where the
  recipe admits nothing above 1G: `make test` and `make test-widget` with
  `WIDGET_PARALLEL=0` (one serial session, no inner scope) and any lane whose
  cap is `none` (`KANI_MEMORY_MAX=none`). Otherwise a valid `--mem none`
  run would be refused for an admission it never makes.
- **Head-of-line waits.** While a large head has the host memory, small
  requests wait whenever they do not fit beside it (proposal, "Loss compared
  with `main`", with the 36G figures). Accepted: this is what makes the head
  admissible at all. It ends as soon as the head is admitted, and a head
  short of physical memory reserves nothing. The coordinator note states the
  figures, so that the programme does not schedule a 36G mutants run beside
  other changes' heavy work without meaning to.
- **Mixed wrapper copies during the transition.** Worktrees that have not
  merged this change still use first-fit, ignore tickets, and wait under
  `LOCK` through their own `heavy.sh`. They share the admission lock, and they
  see envelopes through the placeholder, so nothing is double-booked. The
  fairness and the lock rule hold fully once every worktree has merged
  `main`. A coordinator note in
  `.claude/handoffs/close-heavy-admission-window/coordinator-note.md` says
  that in-flight changes whose tasks read `heavy.sh --mem none -- make test`
  get exit 2 after merging `main`, and must switch to `--envelope 16G`.
- **systemd-oomd** sees envelope sub-slices like scopes (already recorded in
  `build.md`, "Memory"). Each slice carries the distribution's
  `10-oomd-per-slice-defaults.conf` drop-in (H4 probe). The sub-slice's
  `MemoryMax` makes the kernel kill inside the envelope first.
- **The `systemctl` shim** checks the caller through `/proc/<ppid>/cmdline`.
  If the wrapper's process structure changes, the shim fails loudly, because
  the case then cannot reach its precondition and says so. It does not pass
  silently.

GTK axioms: none (no toolkit behaviour relied on).
Kani: none (no proven module touched).
Timing: none (no timed path touched; helper behaviour is proved by the
self-tests, and `measure.sh` is unchanged).
