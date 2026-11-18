# Tasks

Kani: none (no proven module touched).
Timing: none (no timed path touched; the helpers' behaviour is proved by
their self-tests, and `measure.sh` is unchanged).
GTK axioms: none (no toolkit behaviour relied on; no UI, GTK or Rust file
changes).

Rules for every task:

- Work only in `.claude/worktrees/close-heavy-admission-window` and
  `.claude/handoffs/close-heavy-admission-window/`.
- Every heavy command goes through `scripts/heavy.sh [--mem <cap>] -- <cmd>`,
  one command per call. This worktree's own `heavy.sh` changes under you, so
  until group 3 is committed, run lanes with the copy from `main`
  (`git show origin/main:scripts/heavy.sh`) or with the worktree's copy, and
  record which.
- Every commit goes through `scripts/heavy.sh --mem none -- git commit -F
  <file>`: signed, conventional, staged by name, ending with `Co-Authored-By:
  Claude Opus 5.5 <noreply@anthropic.com>`. Push per task group.
- Self-tests use only private lock directories (`LUSHTEXT_AGENT_LOCK_DIR`),
  private per-run slices `user-lushtext-heavy-selftest-<pid>.slice` and their
  envelope sub-slices. Never touch the real `user-lushtext-heavy.slice`
  budget, `~/.cargo/config.toml`, CST or any other slice. Host asks go to
  `.claude/handoffs/close-heavy-admission-window/host-asks.md`.
- Evidence goes in `openspec/changes/close-heavy-admission-window/evidence/`.
- Never `taskset`, never `pgrep -f`, never wait for or poll CI.

## 1. Setup and failing-first self-test cases (no behaviour change)

- [ ] 1.1 Take the implementation slot (`parallel-agent-work.md`,
  "Implementation slots and ordering"). Merge `origin/main` into the branch
  (never rebase), and resolve any conflict in `scripts/heavy.sh` or
  `scripts/with-memory-cap.sh` by keeping both sides' cases. Write
  `.claude/handoffs/close-heavy-admission-window/coordinator-note.md` (design,
  "Risks"): after merging `main`, `heavy.sh --mem none -- make
  test|test-widget|mutants-*|kani|proof-strength` exits 2. Give the
  replacement for each from the design's per-lane table.
- [ ] 1.2 Add the cases below. Each must fail on the current scripts. Record
  each failure line, and the script version it ran against, in
  `evidence/red.md`. A case that cannot run (no systemd user manager) skips
  and says why. Every case is bounded by `timeout`, and none waits more than
  about 6 s.
  - **(1) The window after the probe**, in `heavy.sh --self-test`, using the
    `systemctl` shim of design D7. It covers two private slices: the empty
    one for `--wait-admissible`, and one holding a 2G sleeper for the
    admission. Budget 3G, reserve 1G, `heavy.sh --mem 2G -- touch <marker>`
    in the background. Within 4 s, `flock -w 4 -x <private LOCK> true` must
    succeed, and `<marker>` must not exist. On `main` it fails: the wrapper's
    blocking `admit` holds `LOCK` shared. Clean up the heavy process, the
    sleeper and both slices from an `EXIT` trap.
  - **(1) Exit 75 is the command's**: `heavy.sh --mem 2G -- sh -c 'echo x
    >>"$0"; exit 75' <file>` in a private slice that fits. It must exit 75
    with exactly one line in `<file>`. (It passes on `main`, as a guard.)
  - **(2) `--mem none` inner path**, in `heavy.sh --self-test`: with the
    private slice full (2G sleeper, budget 3G, reserve 1G), run
    `heavy.sh --mem none -- with-memory-cap.sh 2G true`. It must exit 2
    within 3 s with `--envelope` in its stderr, and an exclusive try on the
    private `LOCK` must succeed during that time. On `main` it fails: the
    inner `admit` waits under `LOCK` shared.
  - **(2) The envelope admits inner scopes against itself**: `heavy.sh
    --envelope 2G -- sh -c 'LUSHTEXT_MEMORY_RESERVE=999999G
    with-memory-cap.sh 2G sh -c "<print own cgroup>"'`. The inner cgroup must
    lie under `<private slice stem>-env*.slice`, in a scope of its own. On
    `main` it fails: `--envelope` is a usage error.
  - **(3) Fair order**, in `with-memory-cap.sh --self-test`, all in the private
    slice:
    - a 3G sleeper runs (budget 6G, reserve 1G), then a 4G blocking request
      waits in the background. After 1 s, `timeout 3 with-memory-cap.sh 2G
      true` must time out (124). On `main` it is admitted at once.
    - a 2G NOWAIT request in the same state must still be admitted (0). It
      passes on `main`, as a guard.
  - **`--admission-verdict` fixtures**, for the cases in the
    `heavy-work-admission` spec's "Admission order" requirement: no head; head
    reserved; head beside; head short of memory; inside an envelope (no
    reserve check). On `main` the option is unknown.
- [ ] 1.3 Run `scripts/heavy.sh --mem none -- make check-agent-helpers` with
  `main`'s `heavy.sh` and confirm that exactly the cases above fail. Commit
  the cases ("test(build): failing-first cases for the heavy admission
  window"). The pre-commit hook fails on red self-tests, so either commit the
  red cases behind `LUSHTEXT_ADMISSION_WINDOW_RED=1`, an opt-in guard removed
  in 2.6, or commit groups 1 and 2 together. Record which in `evidence/red.md`.
  Push.

## 2. with-memory-cap.sh: verdict, queue, check-once, refusal, envelope

- [ ] 2.1 Add the pure `admission_verdict` (design D5) and the
  `--admission-verdict` entry. Make the probe (`--wait-admissible`) and
  `admit` both decide through it, which keeps "probe and admit share one
  function". Keep `admitted_kib` (D10, legacy counting) unchanged at the host
  level.
- [ ] 2.2 Tickets and the per-slice queue (D5): create under the admission
  lock; owner PID plus start time; sweep dead tickets. Add
  `--ticket <cap> <owner-pid>` (prints the path) and `LUSHTEXT_ADMISSION_TICKET`
  (excluded from the heads, removed on admission). A blocking `admit` without
  a ticket takes and removes its own. NOWAIT and exclusive-mode admissions
  take none and ignore the head.
- [ ] 2.3 Check-once mode (D1): with `LUSHTEXT_ADMISSION_ONCE=<file>`, a
  misfit writes the waiting message to `<file>` and exits 75 before any scope.
  Unset the variable before the `exec`.
- [ ] 2.4 The `LOCK` proof (D4): source `scripts/agent-locks.sh`, and for caps
  above 1G that are not nested in place, run `agent_lock_inherited`. With
  `exclusive` outside an envelope, check once (75). With `shared`, without
  `LUSHTEXT_ADMISSION_ONCE` and outside an envelope, exit 2 with the message
  of D4. Otherwise behave as today.
- [ ] 2.5 Envelopes (D2, D3):
  - `--envelope <size> <cmd>` starts the placeholder
    `lushtext-cap-<P>-<ns>.scope`, which waits for the owner, reverts the
    sub-slice and exits;
  - it starts the sub-slice `<stem>-env<P>t<ns>.slice` (`MemoryMax`,
    `MemorySwapMax=0`, runtime) and the root scope
    `lushtext-envroot-<pid>-<ns>.scope` (no `MemoryMax`, `OOMPolicy=continue`,
    the placement check), and does not export `LUSHTEXT_MEMORY_CAP_KIB`;
  - inner calls recognise the envelope from `/proc/self/cgroup` and admit
    against the sub-slice's `memory.max`, with no reserve check, no legacy
    slice and no NOWAIT;
  - CI without a user manager runs uncapped; elsewhere the wrapper refuses, as
    today.
- [ ] 2.6 Self-test hygiene (D7): shed `LUSHTEXT_AGENT_LOCK_HELD`; widen the
  cleanup and leftover check to `*selftest-<child>*.slice.d`, and revert any
  envelope sub-slice of the private slice in the `EXIT` trap. Add the
  envelope cases of the spec:
  - the placeholder is counted (NOWAIT 75), and its unit name and
    `memory.max` match what `admitted_kib` reads;
  - a leftover daemon in an envelope keeps no admission, and the sub-slice's
    drop-in is gone;
  - two 2G inner calls in a 2G envelope run one after the other;
  - an inner 4G call in a 2G envelope exits 2;
  - `--address-space` inside an envelope still sets `RLIMIT_AS` to 7/8 of the
    inner cap (the Kani path);
  - a stale marker is admitted as an ordinary call;
  - a head short of memory (900G budget, an 800G waiter) does not block a 2G
    request;
  - a dead ticket does not block.

  Remove the red guard of 1.3 if one was used.
- [ ] 2.7 `shellcheck -x` the scripts, run
  `scripts/heavy.sh --mem none -- make check-agent-helpers`, and check that
  every wrapper case of 1.2 now passes. Commit ("fix(build): admit heavy work
  once after LOCK, in envelopes and in a fair order"; wrapper half) and push.

## 3. heavy.sh: retry after LOCK, envelopes, tickets

- [ ] 3.1 Replace steps 3-4 with D1's loop:
  1. step 1 (the probe, with the ticket);
  2. `INTENT` and `LOCK`;
  3. the parallelism read after `LOCK`;
  4. traps and the marker;
  5. `agent_run_child` with `LUSHTEXT_ADMISSION_ONCE=<report>`;
  6. on 75 with a non-empty report, close fd 7, clear the marker and traps,
     sleep 1 s, then go round again; exit 75 at once if the caller had
     `LUSHTEXT_ADMISSION_NOWAIT=1`.

  Remove the step-4 NOWAIT re-probe. Keep the nested in-place path. There,
  `--envelope` runs `exec with-memory-cap.sh --envelope <size>`, and inside an
  envelope or a measurement the wrapper's own rules apply.
- [ ] 3.2 `--envelope <size>`: mutually exclusive with `--mem` (usage error
  otherwise). Pass `<size>` to `heavy_parallelism` as the cap. Add table rows
  for envelope sizes (for example `16 1 16G:16 8`, `16 1 36G:16 8`,
  `16 1 8G:4 1`, `16 2 16G:4 1`), keeping every existing row.
- [ ] 3.3 Take one ticket per invocation (`--ticket <cap> $$`), keep it
  across retries, remove it in the `EXIT` trap. `--mem none` takes none.
- [ ] 3.4 Update `heavy.sh`'s header: steps, memory before locks, envelopes.
  Run the whole `heavy.sh --self-test`, which must pass every case of 1.2,
  every existing case, and a new case: the parallelism probe after a forced
  misfit-retry still reports the contended or alone setting read after
  `LOCK`. Commit ("fix(build): …"; heavy half) and push.

## 4. Call sites and lane cases

- [ ] 4.1 Audit every caller that admits more than 1G inside a command:
  `grep -n with-memory-cap` over `Makefile`, `scripts/` and `.githooks/`.
  Confirm that `make check-policy` and the pre-commit hook admit nothing above
  1G except the wrapper's self-test (which sheds the marker). Record the list
  in `evidence/call-sites.md`.
- [ ] 4.2 Update the Makefile comments of `test`, `test-widget`, `mutants-*`,
  `kani` and `proof-strength`, and `widget-shards.py`'s docstring, to name
  `heavy.sh --envelope <size>` with the per-lane size. Do not change any
  default cap.
- [ ] 4.3 Lane case: run `scripts/heavy.sh --envelope 16G -- make test` once
  on the final scripts. Record the output in `evidence/lane-make-test.md`:
  - the envelope's placeholder, sub-slice and root scope
    (`systemctl --user list-units 'lushtext-*'` and the cgroup tree during
    the run);
  - each shard's own scope and `widget-memory:` line;
  - no `FLAKY:` lines;
  - the exit status;
  - `/proc/loadavg` before and after.

  A `FLAKY:` line is a blocker (`preexisting-blockers.md`).
- [ ] 4.4 Lane case: run `scripts/heavy.sh --envelope 36G -- make mutants-smoke
  MUTANTS_LOCAL_JOBS=1 MUTANTS_RE=<a narrow regex of a few mutants>`. Record
  in `evidence/lane-mutants.md` that the 36G inner scope landed in the
  envelope, the run's summary, and that no admission waited under `LOCK`.
  Exit 75 (slots busy) is retried later and is not a failure.
- [ ] 4.5 Lane case: `scripts/heavy.sh --mem none -- make mutants-smoke`
  exits 2 at once with the `--envelope` message, before any slot is taken
  (the slot lock files are unchanged).
- [ ] 4.6 Commit ("build: run self-admitting lanes in heavy.sh envelopes")
  and push.

## 5. Documentation (`documentation.md`)

- [ ] 5.1 `.agents/rules/verification-runs.md`, "Shared-machine agent work":
  - the heavy-work bullet's step order (one admission after `LOCK`, retry
    holding nothing);
  - "Memory before locks";
  - replace "run them as `heavy.sh --mem none`" with the envelope rule and
    the per-lane table;
  - the refusal (exit 2);
  - the fair order;
  - `--mem none` only for commands that admit nothing.
- [ ] 5.2 `.agents/rules/parallel-agent-work.md`, "CPU and memory": size an
  admitting command's envelope by its concurrent inner caps; a `--mem` cap
  fits the measured peak, as today.
- [ ] 5.3 `.agents/rules/build.md`:
  - "Compiler parallelism": the envelope size is the cap;
  - the CI section's "Start `make test` and `make test-widget` unwrapped"
    becomes `heavy.sh --envelope 16G`;
  - "Formal Verification (Kani)", "Memory": `heavy.sh --envelope 16G -- make
    kani` and the 12G in-place fact;
  - "Mutation Testing": the `MUTANTS_MEMORY_MAX` envelope;
  - the proof-strength lane.
- [ ] 5.4 `AGENTS.md` (and so `.claude/CLAUDE.md`): the `make test` line's
  "(start it unwrapped)" becomes the envelope form. Check `README.md`,
  `docs/mutation-testing.md`, `docs/runbooks/*.md` and
  `.agents/skills/*/references/*.md` for `--mem none` call sites of
  admitting lanes, and update each one. Run `make check-agent-docs`.
- [ ] 5.5 Append to `docs/memorials/shared-machine-lock-incidents.md` a
  dated section, "The admission window and the starved 36G scope
  (2026-11-15)". It covers the two incidents with their log lines, why
  probing first did not close the window, why `--mem none` could not, the
  budget-dimension evidence for fairness, and what replaced each.
- [ ] 5.6 `docs/next/long-line-documents.md`: mark the
  "Observed, pre-existing, deferred" entry and the "Facts
  `close-heavy-admission-window` builds on" entry done, citing this change
  (no rewrite of the history around them).
- [ ] 5.7 Commit (docs) and push.

## 6. Final gates

- [ ] 6.1 Run `scripts/heavy.sh --mem none -- make check-agent-helpers`,
  `scripts/heavy.sh -- make check` and `openspec validate
  close-heavy-admission-window --strict` on the final tree. Then run one
  commit through `heavy.sh --mem none -- git commit` with the new scripts,
  which is the hook path that runs the wrapper's self-test under a real
  shared `LOCK`. Record the results in `evidence/gates.md`.
- [ ] 6.2 Update `tasks.md`, push, and release the implementation slot
  (`rm -rf "$H/IMPL-SLOTS/close-heavy-admission-window"`).
