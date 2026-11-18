# Proposal

## Why

The shared-machine protocol's first rule for heavy work is **memory before
locks**: `scripts/heavy.sh` never waits for memory while it holds `LOCK`
shared (`.agents/rules/verification-runs.md`, "Shared-machine agent work"). A
heavy command that waits for memory under `LOCK` keeps a pending measurement
(`scripts/measure.sh` holds `INTENT` exclusively and waits for `LOCK`
exclusively) from starting. Every new heavy start then queues behind
`INTENT`. One memory-short admission therefore stalls every agent on the
host. `main` (`c16d6fdc`) breaks the rule in two ways, and its admission
order is unfair besides. This is a pre-existing blocker queued by the
programme (`preexisting-blockers.md`; deferral in
`docs/next/long-line-documents.md`, "Observed, pre-existing, deferred").

1. **The window after the probe.** `heavy.sh` probes with
   `with-memory-cap.sh --wait-admissible <cap>` holding nothing. It takes
   `INTENT` and `LOCK`, then re-probes with `LUSHTEXT_ADMISSION_NOWAIT=1`. Then
   it starts `with-memory-cap.sh <cap> <command>`, whose admission (`admit`)
   **blocks**. Another admission that lands between the re-probe and `admit`
   makes `admit` wait while `LOCK` is held shared. This was observed on
   2026-11-15 at 01:37. A default-cap `heavy.sh` passed both probes, then
   printed `waiting for memory: 48 GiB of 48 GiB admitted to
   user-lushtext-heavy.slice, 61 GiB available, need 12 GiB plus a 16 GiB
   reserve` with `LOCK` held. Another change's `measure.sh` waited behind about
   eight shared holders
   (`.claude/handoffs/nest-heavy-slices-under-user-slice/stage3-notes.md`).
2. **The documented `--mem none` inner path.** `make test` and `make
   test-widget` (`widget-shards.py parallel`: a 16G build scope, then one 2G
   scope per shard), `make mutants-*` (one 36G `MUTANTS_MEMORY_MAX` scope),
   `make kani` and `make proof-strength` (16G each) admit their own scopes.
   The rules say to run the first two as `heavy.sh --mem none`. Their
   admissions then run inside the command while `heavy.sh` holds `LOCK`
   shared, and they block by design. The 36G scope in the incident above came
   from `heavy.sh --mem none -- make mutants-smoke`.
3. **First-fit admission starves large requests.** In
   `reduce-build-disk-writes` stage 3, a 36G `make mutants-smoke` scope
   waited about an hour behind a stream of small admissions and was rerun at
   24G (lane case 11.3). Its log reads `waiting for memory: 42 GiB of 48 GiB
   admitted to lushtext-heavy.slice, 58 GiB available, need 36 GiB plus a 16
   GiB reserve`. That is 58 GiB available against the 52 GiB the request
   needed, so the slice budget was what blocked it, not physical memory.
   Every recorded 36G wait of that change shows the same: 54-62 GiB
   available, budget full (`.claude/handoffs/reduce-build-disk-writes/rounds*/`
   and `warm/` logs). Each small request fit on its own and took the space the
   large one was waiting for.

Measured state of the slice at the start of this change (2026-11-17,
read-only): budget 48G (`MemoryMax=51539607552`) and three live scopes of
16G, 12G and 16G, so 44 GiB admitted. Those scopes used 2.7, 1.2 and 5.2 GiB.
MemAvailable was 40 GiB and the one-minute load 5.9. A 12G request would wait
right now.

## What Changes

- **The admission after `LOCK` is the real one, and it never waits.**
  `heavy.sh` runs `with-memory-cap.sh` in a check-once mode. A misfit exits
  before any scope or command starts and reports itself apart from the
  command's own exit status. `heavy.sh` then releases `LOCK`, waits for memory
  holding nothing, and retries. The extra NOWAIT re-probe goes away. The
  parallelism read (`heavy_lock_shared_holders`, `heavy_parallelism`) still
  happens after `LOCK` is held, on every attempt.
- **Envelopes for commands that admit their own scopes.** A new
  `scripts/heavy.sh --envelope <size> -- <command>` admits `<size>` once,
  through the fixed path, as a reservation the host-wide accounting sees. The
  command runs in a memory sub-slice of `<size>`. The scopes it admits are
  placed in that sub-slice and admitted against the envelope, so they wait
  only for the same command's earlier scopes, never for other agents' memory.
  Each widget shard keeps its own scope and its OOM-kill report.
- **No admission waits under a shared `LOCK` outside those two paths.** A
  capped admission whose process tree provably holds `LOCK` shared is refused
  at once (exit 2, naming `--envelope`) unless it is `heavy.sh`'s own
  check-once admission or lies inside an envelope. The proof is the
  existing ancestor-and-fd check, not an environment marker. `--mem none`
  stays for commands that admit nothing (`git commit`, the CI image lane,
  Docker).
- **Fair admission order.** Waiting admissions take a ticket in a per-slice
  queue. While the oldest live waiter has the host memory for its cap, its cap
  is reserved on the slice budget: a later request is admitted only if it
  also fits beside the oldest waiter's cap. A waiter short of physical memory
  reserves nothing. Measurement admissions (`LUSHTEXT_ADMISSION_NOWAIT=1`)
  bypass the queue and keep their fail-fast exit 75. The probe and the
  admission still share one decision function.
- Regression self-test cases that fail on `main` for (1), (2) and (3) land
  first. The documented call sites move to `--envelope`, and the rules, the
  memorial and the programme record are updated.

## Capabilities

### New Capabilities

- `heavy-work-admission`: Purpose: how the shared-machine helpers admit
  memory-capped heavy work into the host-wide heavy slice, so that no
  admission waits for memory while the shared-machine lock is held and a
  large request cannot be starved by a stream of small ones.

### Modified Capabilities

None. `heavy-work-placement` (where scopes sit, CPU inheritance, legacy-slice
counting) keeps every requirement. Envelope sub-slices nest under the heavy
slice the same way, and the placement check runs in them unchanged.

## Non-goals

- Changing caps, the 48G slice budget, the 16G reserve, the scope properties,
  slice placement or any CPU policy (host-owned, CST).
- Admission by measured use instead of declared caps. The snapshot above
  (44 GiB of caps admitted, about 9 GiB used) shows the cost of caps. Changing
  that is a policy change for its own proposal.
- The front-end-thread policy of PR #114. Its table and cases are kept, and
  only extended.
- Changing `measure.sh`'s lock order or its `--lock-timeout` behaviour.
- Fair ordering for copies of the wrapper from before this change. They share
  the admission lock but ignore the queue until their worktrees merge `main`.

## User-visible impact

There is no application change. For agents and maintainers:

- A pending measurement is no longer delayed by a heavy command that started
  waiting for memory after taking `LOCK`. A memory-short heavy start now waits
  holding nothing.
- `heavy.sh --mem none -- make test` (also `test-widget`, `mutants-*`, `kani`,
  `proof-strength`) now fails at its first inner admission with exit 2 and a
  message naming `--envelope`. The documented forms become
  `heavy.sh --envelope 16G -- make test` and
  `heavy.sh --envelope 36G -- make mutants-smoke`, with a per-lane table in
  the rules.
- **Loss compared with `main`:**
  - an envelope reserves its whole size for the whole run. `make test` holds
    16G through its non-widget phase, where today it holds no admission and
    that phase runs uncapped, and through its shard phase, where today it
    holds 6G;
  - small requests may wait behind an older large one while its cap is
    reserved;
  - a run whose inner caps add up to more than its envelope runs those scopes
    one after another within it.
- `heavy.sh -- make kani` keeps working. It runs Kani in place at the default
  12G, as `main` does; the rules move it to `--envelope 16G`, which gives
  Kani the 16G that `KANI_MEMORY_MAX` asks for.

## Impact

- `scripts/heavy.sh`, `scripts/with-memory-cap.sh`, `scripts/agent-locks.sh`
  (sourced by the wrapper for the LOCK proof), and their `--self-test`s
  (`make check-agent-helpers`).
- `Makefile` comments for `test`, `test-widget`, `mutants-*`, `kani`,
  `proof-strength`, and the `widget-shards.py` docstring.
- Docs: `.agents/rules/verification-runs.md`,
  `.agents/rules/parallel-agent-work.md`, `.agents/rules/build.md`,
  `.agents/skills/gtk-testing/references/widget-testing.md` (if it names an
  admitting call site), `docs/memorials/shared-machine-lock-incidents.md`
  (append), `docs/next/long-line-documents.md` (the deferral is done),
  `docs/mutation-testing.md` (if it names a call site).
- Host-owned state is untouched. Self-tests use private lock directories and
  private per-run slices, plus their envelope sub-slices, only.
