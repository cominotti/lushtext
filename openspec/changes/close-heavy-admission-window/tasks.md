# Tasks

Kani: none (no proven module touched).
Timing: none (no timed path touched; the helpers' behaviour is proved by
their self-tests, and `measure.sh` is unchanged).
GTK axioms: none (no toolkit behaviour relied on; no UI, GTK or Rust file
changes).
Smokes: none (no UI-prefix file touched; the visual, geometry and
accessibility smokes stay current).

Rules for every task:

- Work only in `.claude/worktrees/close-heavy-admission-window` and
  `.claude/handoffs/close-heavy-admission-window/`.
- Every heavy command goes through `scripts/heavy.sh [--mem <cap>] -- <cmd>`,
  one command per call. `heavy.sh` resolves its sibling scripts from its own
  directory, so a copy extracted with `git show` does not run. Up to and
  including the commit of 3.6, launch heavy commands with the main checkout's
  `scripts/heavy.sh` (that is `main`'s copy; read-only use, the main checkout
  is never edited). The gates those commands run (`make check-agent-helpers`,
  the hook's `check-policy`) still execute the worktree's scripts, which are
  what is under test. From group 4 on, launch with the worktree's
  `scripts/heavy.sh`. Record which copy launched each lane.
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

- [x] 1.1 Take the implementation slot (`parallel-agent-work.md`,
  "Implementation slots and ordering"). Merge `origin/main` into the branch
  (never rebase), and resolve any conflict in `scripts/heavy.sh` or
  `scripts/with-memory-cap.sh` by keeping both sides' cases. Write
  `.claude/handoffs/close-heavy-admission-window/coordinator-note.md` (design,
  "Risks"): after merging `main`, `heavy.sh --mem none -- make
  test|test-widget|test-widget-headless|mutants-*|kani|proof-strength`
  exits 2. Give the
  replacement for each from the design's per-lane table. State the
  head-of-line cost too: while a 36G head has the host memory, a later
  request is admitted only while the caps admitted plus that request stay
  within 12G of the 48G budget.
- [x] 1.2 Add the cases below. Each must fail on the current scripts. Record
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
    with exactly one line in `<file>`. (It passes on `main`, as a guard.) The
    same with `heavy.sh --envelope 2G` must also exit 75 with one line. On
    `main` it fails: `--envelope` is a usage error.
  - **(1) A busy admission lock**, in `heavy.sh --self-test`: a sibling holds
    the private admission lock (`LUSHTEXT_ADMISSION_LOCK`, design D7) for 5 s
    while `heavy.sh --mem 2G -- touch <marker>` runs against an empty private
    slice. Within 4 s an exclusive try on the private `LOCK` must succeed,
    and `<marker>` must not exist until the sibling lets go. On `main` it
    fails: `admit`'s blocking `flock 9` waits under `LOCK`. Never hold the
    real admission lock in a test: it would hold up every agent's
    admissions. This case alone does not prove the 2 s bound, because
    `heavy.sh` takes its ticket under the admission lock (D5) before it
    takes `LOCK`, so it may never reach the once-admission. The bound is
    proved by the next case.
  - **(1) The once-admission's 2 s bound**, in `with-memory-cap.sh
    --self-test`: while a sibling holds the private admission lock for 6 s,
    `LUSHTEXT_ADMISSION_ONCE=<file> with-memory-cap.sh 2G touch <marker>`
    against an empty private slice must exit 75 within about 3 s, with a
    `misfit: ` line in `<file>` and no `<marker>`. On `main` it fails:
    `LUSHTEXT_ADMISSION_ONCE` is ignored, and `admit` blocks on `flock 9`.
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
    - a 3G sleeper runs (budget 6G, reserve 1G,
      `LUSHTEXT_ADMISSION_POLL_SECONDS=0.2`), then a 4G blocking request
      waits in the background. After 1 s, `timeout 3 with-memory-cap.sh 2G
      true` must time out (124). Then the sleeper is stopped, and the 4G
      request must be admitted within 3 s. On `main` the 2G request is
      admitted at once.
    - a 2G NOWAIT request in the same state must still be admitted (0). It
      passes on `main`, as a guard.
  - **`--admission-verdict` fixtures**, for the cases in the
    `heavy-work-admission` spec's "Admission order" requirement: no head; head
    reserved; head beside; head short of memory; inside an envelope (no
    reserve check). On `main` the option is unknown.
- [x] 1.3 Run `scripts/heavy.sh --mem none -- make check-agent-helpers` with
  `main`'s `heavy.sh` and confirm that exactly the cases above fail. Commit
  the cases ("test(build): failing-first cases for the heavy admission
  window") behind `LUSHTEXT_ADMISSION_WINDOW_RED=1`, an opt-in guard, because
  the pre-commit hook fails on red self-tests. Record in `evidence/red.md`
  that the guarded run fails on the current scripts and that the unguarded
  hook passes. Push.

## 2. with-memory-cap.sh: verdict, queue, check-once, refusal, envelope

Groups 2 and 3 land as **one** commit (3.6). Once the wrapper refuses an
admission under a shared `LOCK` without `LUSHTEXT_ADMISSION_ONCE` (2.4), the
old `heavy.sh` loop would make every capped `heavy.sh` call above 1G exit 2,
the default 12G included. The "window after the probe" case also needs group
3's loop. So no commit sits between the wrapper half and the `heavy.sh` half.

- [x] 2.1 Add the pure `admission_verdict` (design D5) and the
  `--admission-verdict` entry. Make the probe (`--wait-admissible`) and
  `admit` both decide through it, which keeps "probe and admit share one
  function". Keep `admitted_kib` (D10, legacy counting) unchanged at the host
  level.
- [x] 2.2 Tickets and the per-slice queue (D5): create under the admission
  lock; owner PID plus start time; sweep dead tickets. Add
  `--ticket <cap> <owner-pid>` (prints the path) and `LUSHTEXT_ADMISSION_TICKET`
  (excluded from the heads, removed on admission). A blocking `admit` without
  a ticket takes and removes its own. NOWAIT and exclusive-mode admissions
  take none and ignore the head. Inner admissions of an envelope use the
  envelope's own queue and never the heavy slice's (design D3). Read the
  poll interval from `LUSHTEXT_ADMISSION_POLL_SECONDS` (default 15) in the
  probe and in `admit`, and the admission lock path from
  `LUSHTEXT_ADMISSION_LOCK` (default as today). Both are for the self-tests
  and are documented only in the wrapper's header.
- [x] 2.3 Check-once mode (D1): with `LUSHTEXT_ADMISSION_ONCE=<file>`, the
  admission lock is waited for at most 2 s (busy counts as a misfit), and a
  misfit writes one `misfit: <waiting message>` line to `<file>` and exits 75
  before any scope. Unset it, and `LUSHTEXT_ADMISSION_TICKET`, before the
  `exec`.
- [x] 2.4 The `LOCK` proof (D4): source `scripts/agent-locks.sh`, and for caps
  above 1G that are not nested in place, run `agent_lock_inherited`, only
  when `LUSHTEXT_AGENT_LOCK_HELD` is set (then call `agent_lock_paths`
  first). With
  `exclusive` outside an envelope, check once (75). With `shared`, without
  `LUSHTEXT_ADMISSION_ONCE` and outside an envelope, exit 2 with the message
  of D4. Otherwise behave as today.
- [x] 2.5 Envelopes (D2, D3):
  - `--envelope <size> <cmd>` starts the placeholder
    `lushtext-cap-<P>-<ns>.scope`, which waits for the owner, reverts the
    sub-slice and exits;
  - it starts the sub-slice `<stem>-env<P>t<ns>.slice` (`MemoryMax`,
    `MemorySwapMax=0`, runtime) and the root scope
    `lushtext-envroot-<pid>-<ns>.scope` (no `MemoryMax`, `OOMPolicy=continue`,
    the placement check), and does not export `LUSHTEXT_MEMORY_CAP_KIB`;
  - inner calls recognise the envelope from `/proc/self/cgroup` and admit
    against the sub-slice's `memory.max`, with no reserve check, no legacy
    slice and no NOWAIT, and skip the per-call slice `set-property` (D3);
  - a sentinel scope in the sub-slice, with `OOMPolicy=continue`, keeps it
    alive until the placeholder has read its `memory.peak` and `oom_kill`
    and written the `envelope:` line to the report file (D2, "Envelope peak
    and OOM kills"). Then the placeholder removes the sub-slice's queue
    directory;
  - before admitting, under the admission lock, sweep the envelope
    sub-slices of this heavy slice whose placeholder is gone (D2, "A
    SIGKILLed placeholder");
  - inside a capped scope or an envelope, `--envelope` runs its command in
    place (D2, "Nested envelopes");
  - add H4 to H6 (design, "Baseline") to the wrapper's header, next to H1 to
    H3;
  - `--preflight <cap>` applies D4's check without admitting anything;
  - CI without a user manager runs uncapped; elsewhere the wrapper refuses, as
    today.
- [x] 2.6 Self-test hygiene (D7): shed `LUSHTEXT_AGENT_LOCK_HELD`; widen the
  cleanup and leftover check to `*selftest-<child>*.slice.d`, and revert any
  envelope sub-slice of the private slice in the `EXIT` trap. Add the
  envelope cases of the spec:
  - the placeholder is counted (NOWAIT 75), and its unit name and
    `memory.max` match what `admitted_kib` reads;
  - a leftover daemon in an envelope keeps no admission, and the sub-slice's
    drop-in is gone;
  - two 2G inner calls in a 2G envelope run one after the other;
  - the sub-slice's `memory.max` is still the envelope size after an inner
    admission;
  - an envelope's normal run produces an `envelope: peak=<MiB> oom_kill=0`
    line, read before the sub-slice went away. Never provoke a real OOM kill:
    an OOM kill in a user scope can raise a GNOME notification, which is why
    the existing cases read limits instead;
  - `--preflight 2G` under a proven shared `LOCK` exits 2, and outside one
    exits 0;
  - an inner 4G call in a 2G envelope exits 2;
  - a nested `heavy.sh --envelope 2G` under an outer `heavy.sh --mem none`
    (private locks) exits 2 naming `--envelope`, and admits nothing;
  - `--address-space` inside an envelope still sets `RLIMIT_AS` to 7/8 of the
    inner cap (the Kani path);
  - a stale marker is admitted as an ordinary call;
  - a head short of memory (900G budget, an 800G waiter) does not block a 2G
    request;
  - a dead ticket does not block;
  - an older host-level ticket (a live owner with a 4G cap) does not hold
    back a 2G inner call in a 2G envelope;
  - `--envelope 2G` inside `LUSHTEXT_MEMORY_CAP_KIB=…` and inside an
    envelope runs in place, with no new placeholder or sub-slice;
  - an envelope whose placeholder is SIGKILLed (its own private envelope,
    never another run's) leaves a drop-in that the next envelope admission
    sweeps;
  - the H4 to H6 re-probes: the sub-slice nests under the private slice and
    is not counted as a scope there, and a revert under a live scope leaves
    `memory.max` at `max` with the scope running;
  - the private slices' queue directories are gone after the run, and the
    leftover check fails on one that is not.

- [x] 2.7 `shellcheck -x` the wrapper, and run its `--self-test` directly.
  The wrapper cases of 1.2 must pass. No commit yet (group 2 note).

## 3. heavy.sh: retry after LOCK, envelopes, tickets

- [x] 3.1 Replace steps 3-4 with D1's loop:
  1. step 1 (the probe, with the ticket);
  2. `INTENT` and `LOCK`;
  3. the parallelism read after `LOCK`;
  4. traps and the marker;
  5. `agent_run_child` with `LUSHTEXT_ADMISSION_ONCE=<report>`;
  6. on 75 with a `misfit: ` line in the report, close fd 7, clear the
     marker and traps, truncate the report, sleep 1 s, then go round again.
     Exit 75 at once if the caller had `LUSHTEXT_ADMISSION_NOWAIT=1`. Exit 75
     without a `misfit: ` line is the command's and is returned.

  Remove the step-4 NOWAIT re-probe. Keep the nested in-place path. There,
  `--envelope` runs `with-memory-cap.sh --envelope <size>` as a child with
  a report file, through `agent_trap_signals` and `agent_run_child`, and
  applies 3.5's decision. Inside an envelope or a measurement, the wrapper's
  own rules apply (D2, "Nested envelopes").
- [x] 3.2 `--envelope <size>`: mutually exclusive with `--mem` (usage error
  otherwise). Pass `<size>` to `heavy_parallelism` as the cap. Add table rows
  for envelope sizes (for example `16 1 16G:16 8`, `16 1 36G:16 8`,
  `16 1 8G:4 1`, `16 2 16G:4 1`), keeping every existing row.
- [x] 3.3 Take one ticket per invocation (`--ticket <cap> $$`), keep it
  across retries, remove it in the `EXIT` trap. `--mem none` takes none.
- [x] 3.4 Update `heavy.sh`'s header: steps, memory before locks, envelopes.
  Make `heavy.sh --self-test` follow D7: a private `LUSHTEXT_HEAVY_SLICE` for
  every case with a budget override, stop and revert from its `EXIT` trap,
  the real slice's `MemoryMax` unchanged at the end, and no drop-in or queue
  directory left. Run the whole `heavy.sh --self-test`, which must pass every
  case of 1.2, every existing case, and a new case: the parallelism probe
  after a forced misfit-retry still reports the contended or alone setting
  read after `LOCK`.
- [x] 3.5 Envelope OOM: after the child exits, wait a bounded few seconds for
  the `envelope:` line. On `oom_kill > 0`, print `OOM-KILL: envelope <name>:
  …` and exit non-zero even when the command exited 0. A missing line after
  the wait is reported as a warning, never as a pass that claims no OOM. Test
  the decision as a pure function over report lines: `oom_kill=0` passes the
  command's status through; `oom_kill=2` with status 0 gives `OOM-KILL:` and
  a non-zero exit; a missing line gives the warning. Do not provoke a real OOM
  kill (2.6).
- [x] 3.6 Remove the red guard of 1.3. Run `shellcheck -x` and
  `scripts/heavy.sh --mem none -- make check-agent-helpers` with the
  worktree's scripts. Every case of 1.2 and every existing case must pass.
  Commit groups 2 and 3 together ("fix(build): admit heavy work once after
  LOCK, in envelopes and in a fair order") and push.

## 4. Call sites and lane cases

- [x] 4.1 Audit every caller that admits more than 1G inside a command:
  `grep -n with-memory-cap` over `Makefile`, `scripts/` and `.githooks/`.
  Confirm that `make check-policy` and the pre-commit hook admit nothing above
  1G except the wrapper's self-test (which sheds the marker). Record the list
  in `evidence/call-sites.md`.
- [x] 4.2 Update the Makefile comments of `test`, `test-widget`,
  `test-widget-headless` (every user of `CARGO_TEST_WIDGET` and
  `CARGO_TEST_WIDGET_HEADLESS`), `mutants-*`,
  `kani` and `proof-strength`, and `widget-shards.py`'s docstring, to name
  `heavy.sh --envelope <size>` with the per-lane size, and drop the "Start
  them unwrapped" advice. Start each of those recipes with
  `./scripts/with-memory-cap.sh --preflight <its inner cap>`, so a `--mem
  none` run fails before its first phase rather than after it. Skip the
  preflight where the recipe admits nothing above 1G (`WIDGET_PARALLEL=0`, a
  `none` cap; design, "Risks"). Do not change any default cap.
- [x] 4.3 Lane case: run `scripts/heavy.sh --envelope 16G -- make test` once
  on the final scripts. Record the output in `evidence/lane-make-test.md`:
  - the envelope's placeholder, sub-slice and root scope
    (`systemctl --user list-units 'lushtext-*'` and the cgroup tree during
    the run);
  - each shard's own scope and `widget-memory:` line;
  - the envelope's `envelope: peak=… oom_kill=…` line. Set the documented
    `make test` size from that peak (design D2, "provisional"): keep 16G if
    the peak fits with margin, otherwise raise it to the next power of two
    and record why;
  - no `FLAKY:` lines;
  - the exit status;
  - `/proc/loadavg` before and after.

  A `FLAKY:` line is a blocker (`preexisting-blockers.md`).
- [x] 4.4 Lane case: run `scripts/heavy.sh --envelope 24G -- make mutants-smoke
  MUTANTS_MEMORY_MAX=24G MUTANTS_LOCAL_JOBS=1 MUTANTS_RE=<a narrow regex of a
  few mutants>`. The case proves placement, not size. A 36G envelope would
  itself be a head that holds back the parallel programme changes, and lane
  case 11.3 of `reduce-build-disk-writes` ran at 24G. Record in
  `evidence/lane-mutants.md` that the inner scope landed in the envelope,
  the run's summary, and that no admission waited under `LOCK`. Exit 75
  (slots busy) is retried later and is not a failure.
- [x] 4.5 Lane case: `scripts/heavy.sh --mem none -- make mutants-smoke`
  and `scripts/heavy.sh --mem none -- make test` each exit 2 within a second
  with the `--envelope` message (the preflight), before any slot is taken or
  any test runs. Also check, without running it, that the `test-widget`
  recipe with `WIDGET_PARALLEL=0` has no preflight (`make -n test-widget
  WIDGET_PARALLEL=0`).
- [x] 4.6 Commit ("build: run self-admitting lanes in heavy.sh envelopes")
  and push.

## 5. Documentation (`documentation.md`)

- [x] 5.1 `.agents/rules/verification-runs.md`, "Shared-machine agent work":
  - the heavy-work bullet's step order (one admission after `LOCK`, retry
    holding nothing);
  - "Memory before locks", and the clause "never reserves memory while
    waiting for a lock": a ticket kept while waiting for a lock holds a
    place in the order, not memory, and no measurement waits behind it
    (design D5);
  - replace "run them as `heavy.sh --mem none`" with the envelope rule and
    the per-lane table;
  - the refusal (exit 2);
  - the fair order;
  - `--mem none` only for commands that admit nothing.
- [x] 5.2 `.agents/rules/parallel-agent-work.md`, "CPU and memory": size an
  admitting command's envelope by its concurrent inner caps; a `--mem` cap
  fits the measured peak, as today.
- [x] 5.3 `.agents/rules/build.md`:
  - "Compiler parallelism": the envelope size is the cap;
  - the CI section's "Start `make test` and `make test-widget` unwrapped"
    (and `make test-widget-headless`)
    becomes `heavy.sh --envelope 16G`;
  - "Formal Verification (Kani)", "Memory": `heavy.sh --envelope 16G -- make
    kani` and the 12G in-place fact;
  - "Mutation Testing": the `MUTANTS_MEMORY_MAX` envelope;
  - the proof-strength lane.
- [x] 5.4 `AGENTS.md` (and so `.claude/CLAUDE.md`): the `make test` line's
  "(start it unwrapped)" becomes the envelope form. Check `README.md`,
  `docs/mutation-testing.md`, `docs/runbooks/*.md` and
  `.agents/skills/*/references/*.md` for `--mem none` call sites of
  admitting lanes, and update each one. Update the Rules Index lines of
  `AGENTS.md` for `build.md`, `parallel-agent-work.md` and
  `verification-runs.md` if their stated scope changes. Run `make
  check-agent-docs`.
- [x] 5.5 Append to `docs/memorials/shared-machine-lock-incidents.md` a
  dated section, "The admission window and the starved 36G scope
  (2026-11-15)". It covers the two incidents with their log lines, why
  probing first did not close the window, why `--mem none` could not, the
  budget-dimension evidence for fairness, and what replaced each.
- [x] 5.6 `docs/next/long-line-documents.md`: mark the
  "Observed, pre-existing, deferred" entry and the "Facts
  `close-heavy-admission-window` builds on" entry done, citing this change
  (no rewrite of the history around them).
- [x] 5.7 Commit (docs) and push.

## 6. Final gates

- [ ] 6.1 Run `scripts/heavy.sh --mem none -- make check-agent-helpers`,
  `scripts/heavy.sh -- make check` and `openspec validate
  close-heavy-admission-window --strict` on the final tree. Then run one
  commit through `heavy.sh --mem none -- git commit` with the new scripts,
  which is the hook path that runs the wrapper's self-test under a real
  shared `LOCK`. Record the results in `evidence/gates.md`.
- [ ] 6.2 Update `tasks.md`, push, and release the implementation slot
  (`rm -rf "$H/IMPL-SLOTS/close-heavy-admission-window"`).
