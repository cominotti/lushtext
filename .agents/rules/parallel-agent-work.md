---
description: How several agents run changes in parallel on the shared workstation — host ownership, CPU and memory sizing, implementation slots, ordering, foreground waits, measurement queueing, serialized merges on local evidence, and durability
---
<!-- global rule: loaded for every request -->

# Parallel Agent Work

Several agents often run changes at once on one workstation: workflow runs,
their stage subagents, and other sessions. This rule says how they share the
workstation and how their work reaches `main`. The helpers (`heavy.sh`,
`measure.sh`, `wait-for.sh`) and the lock protocol are in
[`verification-runs.md`](./verification-runs.md), "Shared-machine agent work".
The reasons are in
[`docs/memorials/shared-machine-lock-incidents.md`](../../docs/memorials/shared-machine-lock-incidents.md).

The **coordinator** is the main session that keeps `PARALLEL-LEDGER.md`. It
launches runs and sets their queue order. It runs `MEASURE-QUEUE.md` in quiet
windows. It removes placeholder or stale slots and locks.

Coordination files live in the main checkout's `.claude/handoffs/`, which is
gitignored. From any worktree:
`H="$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")/.claude/handoffs"`.
Set `H` and `C` (the change name) in the same Bash call as every command
below that uses them: shell variables do not survive between calls. `$H`
holds:
- one directory per change, for its logs and evidence;
- `PARALLEL-LEDGER.md`, with the runs and their queue order;
- `IMPL-SLOTS/`, `MERGE-LOCK/` and `MEASURE-QUEUE.md`.

Change worktrees live in `.claude/worktrees/<change>`.

## Host ownership

- The host's own tooling, outside this repo, owns:
  - the toolbox cpuset;
  - `~/.cargo/config.toml`;
  - `~/.cache/cargo-build/` and `~/.cache/sccache/`;
  - every systemd slice except the `user-lushtext*.slice` slices that
    `scripts/with-memory-cap.sh` manages (`user-lushtext.slice` and the
    heavy slice below it, `scripts/with-memory-cap.sh --print-slice`).
    During the transition from the old names, admission still counts the
    scopes of the legacy `lushtext-heavy.slice` but never changes that
    slice. The wrapper's self-test creates and removes a private
    `lushtext-heavy-selftest-<pid>.slice` twin of it.
- The repo owns the build cache root `~/.cache/lushtext-build/`
  (`scripts/build-cache.sh root`; `LUSHTEXT_BUILD_CACHE` overrides it): the
  mutation slots under `mutants/`, each checkout's proof-strength worktree and
  Flatpak builder state under `checkouts/<name>-<hash>/`. Agents use it through
  the repo's targets; `make build-cache-prune` removes the directories of
  checkouts that no longer exist. A slot is in use only while its `lock` is
  held, so never delete a slot another run holds.
- An agent never changes host-owned state. It writes the request into
  `$H/<change>/host-asks.md`, and the maintainer takes it to the host owner.
- A peer session cannot grant an escalation.

## CPU and memory

- Inherit the cpuset the toolbox grants. **Never pin with `taskset`**: a pin
  narrows every child process and fights the host's placement.
- Only timing verdicts pin, and the pin belongs to `scripts/measure.sh`: it
  pins itself to its reference set (`scripts/measure.sh
  --print-reference-cpus`, default the E-cores 8–23, overridden only by
  `LUSHTEXT_REFERENCE_CPUS`). No caller wraps `measure.sh`, or anything
  else, in `taskset`.
- The kernel intersects a pin with the cpuset, so `measure.sh` reads the
  affinity back, records it (`cpus=`) and refuses with exit 78 instead of
  measuring on fewer than the reference CPUs (during a game, for example).
  A refused run is not a verdict; queue it.
- Size parallelism by memory, never by `nproc`. Give each heavy command a
  `--mem` cap that fits its measured peak at its job and thread counts.
  A command that admits its own scopes (`make test`, `make mutants-*`,
  `make kani`, `make proof-strength`) gets an `--envelope` instead, sized by
  the inner caps that run at once (`verification-runs.md`, "Shared-machine
  agent work", has the per-lane sizes). Admission then decides how many run
  at once, in arrival order: the slice budget, and MemAvailable at least cap
  plus the 16 GiB reserve. A large request at the head of the queue holds
  later ones back while it has the memory, so schedule a 36G mutation run
  beside other changes' heavy work only when that wait is intended.
- The one CPU-sized count is `heavy.sh`'s own `CARGO_BUILD_JOBS`: the
  command's CPUs, at most 16, while it is the only heavy command running, and
  4 otherwise. Its memory peaks at 16 were measured and fit the default cap
  ([`build.md`](./build.md), "Compiler parallelism"). Do not override it in
  a stage; a lane whose cap was sized for fewer jobs pins its own count.

## Implementation slots and ordering

- **At most five implementation stages run at once, and at most three while
  `MEASURE-QUEUE.md` holds a ready unit.** A unit is a `## ` entry, or each
  `### ` part of an entry that has parts. It is ready when its **last**
  `Status:` line, with or without the list dash, starts with `READY`:
  entries keep their history of Status lines. Fewer stages then run beside the rounds a change waits on, so the
  quiet window comes sooner.
- Before its first heavy command, a stage acquires its slot with
  `scripts/impl-slots.sh acquire "${C:?}"` (`C` is the change name), which
  waits in the foreground for at most 540 s per call and exits 124 when it
  timed out: repeat the call ("Waiting"). It counts every slot directory,
  coordinator placeholders included, and evicts nothing held above the cap.
  `scripts/impl-slots.sh cap` prints the cap and the ready units behind it.
  - Before the stage ends, whatever the outcome, it runs
    `scripts/impl-slots.sh release "${C:?}"`.
  - Only the coordinator removes another change's slot.
- Spec, spec-review and review stages take no slot. Their heavy commands
  still go through `heavy.sh`.
- When two changes edit the same files, the later one in the ledger starts
  implementation only after the earlier one has merged, from the updated
  `origin/main`.
- A change whose `openspec/changes/<name>/` already exists on `main` is
  revised in place (the `openspec-update-change` skill), never proposed
  again.

## Waiting

- **A subagent or workflow stage waits in the foreground.** It repeats a
  bounded Bash call (tool timeout 600 s) until the condition holds:
  - `scripts/wait-for.sh … --timeout 540`, or
  - for a lock or slot directory,
    `timeout 540 bash -c 'until <try>; do sleep 30; done'`.
- It never waits with `run_in_background`, a monitor, or by ending its turn.
  A subagent that ends its turn is finished, and its work stays undone.
- **It never returns while a command it started is still running.** Gates run
  in the foreground (a Bash timeout of up to 600 s), or a detached command is
  checked in repeated foreground calls until it exits. An agent that returned
  with its gates in the background left them to die and its fixes
  uncommitted.
- Only the main session may end its turn to wait for a task notification.
- Never hold `INTENT`, `LOCK`, `IMPL-SLOTS/.lock` or the `MEASURE-QUEUE.md`
  lock while waiting for anything else (`verification-runs.md`). `MERGE-LOCK`
  is the one lock held across work, in "Merging".
- Never wait for or poll CI, except as in "Merging".

## Measurements

- The wall time of a build, a test suite or a smoke run is not a verdict. Run
  it through `heavy.sh`, never `measure.sh`, and record `/proc/loadavg`
  before and after.
- **Timing verdicts drain first and are judged on busy CPUs** (the
  maintainer's decision of 2026-11-18, level 1;
  [why](../../docs/memorials/2026-11-18-change-flow-time.md)). The
  responsiveness lane, interleaved benches and every other verdict round go
  through three steps, with `scripts/quiet-gate.py`:
  1. Holding nothing, `scripts/quiet-gate.py wait-outsiders` waits until host
     CPU use outside our own work (`user-lushtext.slice`, the legacy slice and
     the shared `cst-sccache-*` scopes) is below 3.5 CPUs over 30 s. It exits
     124 after 540 s.
  2. `scripts/measure.sh --lock-timeout 2400`, so our own heavy work finishes
     under `INTENT`, runs the round as
     `scripts/quiet-gate.py run --label <label> --log <file> -- <command>`.
  3. Inside the lock the wrapper samples busy CPUs for 30 s and runs the
     round only below 4 (else exit 76, the round did not run). The round is
     **kept** only when it exits 0 and the busy CPUs over the whole round and
     over 30 s after it are both below 4 (else exit 77).
  - The bar is 4 busy CPUs. The 1-minute load is still recorded by
    `measure.sh` and is not the criterion.
  - Meet any stricter precondition the lane's runbook sets.
  - A stage agent cannot hold one Bash call for 2400 s: it starts step 2
    detached (`setsid nohup … &`) and waits for its PID with
    `scripts/wait-for.sh --pid <pid> --timeout 540` in repeated foreground
    calls.
  - A round measured under outside load is never counted as a pass. The
    maintainer's calibration under outside load is a queue entry of its own;
    changing this needs a maintainer decision and a rule edit.
- If `wait-outsiders` exits 124, `measure.sh` exits 75, or a round exits 76
  or 77 and the window stays busy, append the remaining rounds to
  `MEASURE-QUEUE.md` under `flock "$H/MEASURE-QUEUE.md"`, then carry on. The
  entry gives:
  - the change, branch and worktree;
  - the exact commands, cases and round order;
  - the artifact directories and the budget;
  - its status: `Status: READY <sha>` for rounds a change waits on whose
    tree is final (`<sha>` is the commit they measure), `Status: PENDING
    (NOT READY: <reason>)` for rounds it will wait on that cannot run yet
    (their owner replaces the line with `READY <sha>`), and
    `Status: PENDING` for non-blocking attribution and calibration. Only
    `READY` lowers the slot cap.
- **A verdict class is set before any verdict round** (`verification-runs.md`,
  "Timing campaigns"): a non-inferiority verdict blocks the merge like any
  other, and an improvement verdict owed by a follow-up change does not.
- A queued verdict does not hold up implementation. The review reports
  "ready, measurement pending" and ends without merging.
- Once the coordinator has run the queue and recorded the verdict:
  - if the verdict meets the budget, it starts a merge-only run;
  - if it does not, it sends the change back.

## Merging

- **Merge on local evidence (programme changes only).** The maintainer
  authorized this on 2026-11-14 for changes the coordinator runs from the
  ledger, when the stage brief says to merge. It covers no other pull
  request.
- A failure already seen locally, or known on `main`, still blocks the merge,
  or gets its own change in the ledger. Only CI results nobody has read wait
  for the end of the programme. CI is checked once then, and every red job is
  fixed in that work stream (`preexisting-blockers.md`).
- Exception: a change whose **correctness** evidence exists only in CI, such
  as a `.github/workflows` change, may dispatch and read its own runs. It
  waits with `timeout 540 gh run watch <id> --exit-status` in repeated
  foreground calls, one run at a time by its id (`gh pr checks --watch` has
  returned while runs were still pending), and merges only when those runs
  pass. It waits before
  step 1, never while holding `MERGE-LOCK`. If the `origin/main` merge in
  step 2 changes files that evidence covers, it releases the lock, pushes,
  and waits for the new runs first.
- **Kani shard budgets are archive evidence, not merge evidence**
  (`verification-runs.md`, "Kani", "Shard budgets"). Stage 3 dispatches the
  runs and never watches them; stage 4 reads the finished ones with
  `gh run view`. A finished red run or a figure over a margin blocks the
  merge. A run still in progress does not: the change merges on local
  evidence, releases `MERGE-LOCK`, appends `ARCHIVE OWED <change>: kani
  <run ids>` to the ledger, waits for the runs in the foreground outside
  every lock, and takes `MERGE-LOCK` again for the archive commit alone.
  - The figures are written only in the archive commit, never on the branch
    (a branch edit changes the tree and voids the gate records of step 2).
    That commit's diff of `scripts/kani-shards.py` touches only the pending
    records' `measured_in` and figure fields; check it before committing.
  - A red result found then keeps the change active, and the fix goes
    through a new pull request of the same change before the archive.
- **Merges are serialized under `$H/MERGE-LOCK`.**
  1. Take it:
     `timeout 540 bash -c 'until mkdir "$0/MERGE-LOCK" 2>/dev/null; do sleep 30; done' "${H:?}"`,
     repeated. Then write `owner.txt`, one item per line, the change name
     first: change, PR number, run id, `date -Is`.
  2. Holding it:
     - `git fetch --prune origin`, then `git merge origin/main`, always;
     - with a clean status, rerun `make check` and the tests covering files
       changed on either side, through `heavy.sh`, **except** a gate that
       `scripts/gate-trees.sh lookup <wt> --gate '<gate>'` finds already
       passed on this exact `HEAD^{tree}` (a full `make test` record covers
       any subset of tests: pass `--gate 'make test'` as well). Name the
       reused lines. A merge that changed the tree, even by a docs-only
       commit, finds no record and reruns. Record each gate you run with
       `scripts/gate-trees.sh record <wt> --gate '<gate>' -- scripts/heavy.sh … -- make <gate>`;
       only that helper writes `gate-trees.log`, and it records nothing for
       a dirty or changing worktree. It records a failing run too, and a
       tree with a recorded failure of a named gate is never reused, since
       a pass and a fail on one tree is a flake;
     - `git push origin HEAD`, to the branch of the same name (never
       `HEAD:main`).
  3. `git fetch --prune origin`, then `git merge-base --is-ancestor origin/main HEAD`.
     If it fails, `main` moved since step 2: repeat step 2. If it holds,
     GitHub's merge commit gets the tree the gates ran on. Then merge with
     `gh pr merge <n> --merge --admin --match-head-commit "$(git rev-parse HEAD)"`.
     `--admin` skips required checks and reviews.
  4. Archive the change in a fresh worktree on `origin/main` after
     `git fetch`, commit it through `heavy.sh`, and push that one commit with
     `git push origin HEAD:main`, under the same authorization.
     An archive that still owes Kani figures follows in a second
     `MERGE-LOCK` hold, as above; the archive commit's hook then runs the full
     `make check`, because `scripts/kani-shards.py` is not docs-only.
  5. Once the archive is on `origin/main`, release the lock only if it is
     yours:
     `[ "$(head -n1 "${H:?}/MERGE-LOCK/owner.txt")" = "${C:?}" ] && rm -rf "${H:?}/MERGE-LOCK"`.
- If GitHub or the permission prompt refuses the merge or the push, release
  `MERGE-LOCK` without archiving, then stop and report. Any other early exit
  also releases it. Never retry another way: no other flag, account or API
  call, no ruleset edit, and never push the change's own commits to `main`.
  The archive commit in step 4 is the only direct push.
- Only the coordinator breaks a lock: one whose owner's PR is merged or
  closed, or whose run the ledger shows as ended. It records the break in
  that change's handoff.

## Durability

- Push per task group (`verification-runs.md`, "Checkpoints").
- Run the non-visual gates first, and the visual and smoke lanes once, on the
  final tree.
- A commit that touches the UI prefixes still needs the three smokes current
  (`testing.md`), so put those files in as few task-group commits as
  possible, ideally the last.
- After the `origin/main` merge under `MERGE-LOCK`, rerun a visual lane only
  if the merged-in changes touch the same UI-prefix files.
- After the merge:
  1. `git fetch --prune origin`.
  2. Check that `git -C <wt> status --porcelain` is empty and that
     `git -C <wt> merge-base --is-ancestor HEAD origin/main` succeeds.
  3. Remove the worktree first, since a branch checked out in a worktree
     cannot be deleted: `git worktree remove <wt>`.
  4. `git branch -D <branch>`, then `git push origin --delete <branch>`, and
     the same for the twin Kani ref `<branch>-kani-2` when stage 3 pushed one.

## Running a change

- **One workflow run per change, one agent per stage.** The stages, in order:
  spec, spec review, implementation, prepare, `/simplify`, review lenses,
  apply and gates, final review with merge and archive ("Stages after
  implementation"). Each stage is
  bound to a skill and the prompt adds only what the skill does not cover:
  - spec: `openspec-propose` for a new change, `openspec-update-change` for
    one whose `openspec/changes/<name>/` is already on `main`;
  - implementation: `openspec-apply-change`;
  - archive: `openspec-archive-change`.
- **The versioned script is `.claude/workflows/programme-change.js`**, a
  real file, never a symlink. The guaranteed way to run it is the path form
  from the main checkout,
  `Workflow({scriptPath: '.claude/workflows/programme-change.js', args})`:
  `scriptPath` loads only files inside the session's directories, so never
  run a copy in a sibling worktree. The name form
  `Workflow({name: 'programme-change'})` is a convenience for sessions
  started after the script reached `main`; a session that was already
  running when it was added may not find it by name. `args.mode` is `new`,
  `fast-lane`, `revise`, `continue` or `review-only` (an existing pull
  request from the prepare stage on, stopping before the merge unless
  `args.merge`); `revise` and `review-only` need `args.pr` and
  `args.worktree`, and every argument is checked before the first agent
  runs. `args.dry_run` prints the stages a mode would run. The script's header
  lists the other arguments. Host details (the push command, the gh account) go in
  `args.local`, free text the coordinator keeps outside the repository;
  nothing host-specific is written into the script. Its prompts cite rule
  sections and never copy them (`make check-workflow-scripts`), so a rule
  edit reaches every later run. Each script has a stub-agent harness beside
  it, `<name>.test.mjs`, which the same target runs; a change to a script's
  control flow changes its harness in the same commit.
- **What every stage agent does**, whatever its prompt says:
  - stays in its own worktree and handoff directory, and never touches
    another worktree, another change's branch or the main checkout;
  - reads `$H/<change>/COORDINATOR-NOTE.txt` at the start and before each
    heavy command group; the coordinator steers a running stage only through
    that file;
  - works headless: never `make run`, never the live desktop, never the
    real application data or `~/.config`;
  - writes commit messages to a file and commits with `git commit -F`
    through `heavy.sh`, which forwards no stdin;
  - fetches with `git fetch --prune`, so a branch deleted on GitHub leaves
    no stale `refs/remotes/origin/<branch>`;
  - merges `origin/main` into its branch and never rebases a pushed branch
    or pushes with `--force` or `--force-with-lease`: a rejected push means
    fetch, merge, rerun the affected gates and push again. When
    `origin/<branch>` holds a commit made on GitHub (an "Update branch"
    merge or a web conflict resolution), it merges `origin/<branch>` too
    before pushing and never drops that commit. The
    `refs/remotes/origin/*` refs are shared by every worktree, so a tip read
    before another session's fetch is not evidence about your branch;
  - reads a helper's exit status, never its output alone: empty output
    means "nothing to do" only when the helper documents that status, and
    any status it does not document as success means blocked.
- **Never `SendMessage` a running workflow agent.** It revives a second copy
  of that agent, which then edits the same worktree.
- **Changes to the change-delivery rules keep the quality floor.** An edit
  to this file or to `verification-runs.md` gets an independent spec review
  and a stage-4 review against the existing rules, and its `design.md` has a
  table naming each rule edit, the rule it touches and why the floor holds.
  The floor: the data-safety review; an independent code reviewer; every
  gate on every distinct tree that reaches `main`; pre-registered verdicts
  through `measure.sh`, judged by "Measurements"; `B_INTERACT_US` and
  `B_SETTLE_US`; flakes as blockers; no lane, shard, scenario or assertion
  removed. An edit that would let an ungated tree reach `main` is a
  must-fix.
- **A new crate or top-level directory** is outside the fast lane until a
  change adds it to `scripts/check-fast-lane.py`'s allowlist ("Fast lane").

## Fast lane

- A change may skip the separate spec-review stage when
  `scripts/check-fast-lane.py` classes every path it plans (`--paths`) and
  every path it changed (`--base origin/main`) `eligible` or `declare`, and
  it plans no timing verdict.
  - `eligible`: `docs/`, `openspec/`, and Markdown files at the repository
    root; a path a gate names literally becomes `declare`.
  - `declare`: agent guidance (rules, skill text, `AGENTS.md`, `SOUL.md`,
    `CLAUDE.md`).
  - Everything else is not eligible: shipped code, proofs, tests, gate
    implementations (the `Makefile` among them, since it wires every gate),
    packaging, CI, the workflow scripts, any new directory,
    and the two change-delivery rules, this file and `verification-runs.md`,
    whose edits always get a spec review ("Running a change").
- **Every `declare` path needs a "Gate effect: none" section** in
  `proposal.md`, saying for each such file that no gate is narrowed, removed
  or re-keyed and no item of the floor changes.
- In the fast lane, stage 1 writes `proposal.md`, the delta specs and
  `tasks.md` (`design.md` is optional, so `proposal.md` carries a
  "Decisions" section) and records its own spec-review checklist under
  "Self-review". The independent apply agent of stage 4 then also reviews the
  spec artifacts, re-runs the check on the real diff and verifies each
  declaration against it line by line.
- **Leaving the fast lane.** A not-eligible path found at any stage, or a
  false or missing declaration, sends the change to an independent spec
  review before any further implementation or the merge.

## Design revisions

- A design revision is a run of the revise mode's stage 1 on an existing
  change, or any rewrite of `design.md` decisions after a blocked stage 4 or
  a failed pre-registered gate. `design.md` lists them under
  `## Design revisions`, one line each (date, trigger, id); the original
  design is revision 0.
- **When a change with two revisions is blocked again**, an agent that wrote
  none of its design or code reads the design, the gates, the evidence and
  the attributions, and returns `continue`, `re-scope` or `shelve` with
  reasons. The coordinator records it in the ledger and tells the
  maintainer.
- `continue` or `re-scope` starts the next revision with an attribution
  spike: one agent, no edits to OpenSpec artifacts, output
  `$H/<change>/spike-<n>.md` with figures (its rounds go through
  "Measurements"). The revision cites it.
- `shelve` marks the change `SHELVED <date>: <reason>` in the ledger and the
  programme record. Branch, worktree and pull request stay; it holds no slot
  or lock and resumes only through the coordinator.

## Stages after implementation

Workflow agents have no `Agent` tool, so every parallel review comes from the
script, and inside a workflow both skills below run their single-pass forms.

0. **Prepare**, in its own agent, which edits nothing. The worktree must be
   one listed by the main checkout's `git worktree list`, on the change's
   branch, with an empty `git status --porcelain`: uncommitted work belongs
   to an implementation round, never to the review stages. It fetches,
   merges `origin/<branch>` and `origin/main` when they hold commits the
   branch lacks, pushes, and sets the branch to track `origin/<branch>`. A
   conflict aborts the merge and stops the run: resolving it is
   implementation work, which the review stages must see. Every mode runs
   it, `review-only` included.
1. **`/simplify`**, in its own agent, quality only. It starts from a clean
   worktree and is invoked with the change worktree's absolute path as its
   argument, never bare: the agent's working directory is the main checkout
   and cannot move. Its prompt names the base itself (`origin/main...HEAD`
   for the committed work, `HEAD` for its own edits), because the recipe's
   default range is empty on a pushed branch. It runs the tests of what it
   touched, leaves its edits uncommitted, and saves them, new files included,
   as `$H/<change>/simplify.patch`.
2. **Review lenses, in parallel and findings only.** Each writes
   `$H/<change>/review-findings-<lens>.md` and returns its findings.
   - Before them, a read-only agent records the scope: `HEAD`, the pull
     request's head and the upstream (all three must be equal), the
     committed files (`git diff --name-only origin/main...HEAD`), the
     uncommitted ones, and the exit status and raw output of
     `scripts/agent-topology.py review-lenses --base origin/main`, which the
     script parses itself. A non-zero exit stops the run: a selector that
     failed is never "no domain lens".
   - **`code-review high <PR>`** reviews the committed work, and, when
     `/simplify` left edits, a second call **`code-review high <absolute
     worktree path>`** reviews only those. The path form diffs against the
     branch's upstream, so on a branch tracking `origin/<branch>` at `HEAD`
     it sees exactly the uncommitted edits and none of the commits (probe
     C4); the PR form sees the PR's files (probes C2 and C3). Never pass
     `--fix` (a forked review edits the main checkout) and never a bare
     level (it reviews the main checkout's last commit). The lens agent has
     **no schema**, takes the findings from the Skill result's text or, when
     that carries none, from the fork transcript
     (`subagents/agent-<agentId>.jsonl` of the session, the input of the
     fork's last `StructuredOutput` call), and ends with one
     `skill_findings_source:` line per call, one `CODE-REVIEW-SCOPE:` line
     per call (the PR, or the base the fork named, with file and line
     counts), a `findings_count:` line and a last `CODE-REVIEW-STATUS:` line
     (`ok`, `empty` or `failed`). The script checks each count against the
     scope reading. A missing line, a scope that does not match, or `ok`
     over no findings is a failed lens: one rerun, then the run stops.
   - **An empty review stops the run.** `CODE-REVIEW-STATUS: empty` is
     allowed only with a non-zero quoted scope for every call; the script
     then stops with its own message, and only the coordinator, after
     reading the findings file, resumes it with `args.accept_empty_review`.
   - **The fork may run repository scripts** (probe C4 ran
     `impl-slots.sh acquire`). The lens agent runs none itself, releases
     this change's slot if the review created it, and lists what the fork
     ran in its findings file; `impl-slots.sh` gives no slot to a name
     without a change worktree, and the guard (below) compares this
     change's slot and `MERGE-LOCK` ownership around the lens group.
   - **Domain lenses, only when the change touches data or performance**, as
     the skills' own triggers declare. The selector's flags: `data` (the
     `[data_safety]` table of `.agents/skills/skill-policy.toml`, the
     machine form of the `data-safety` description) adds the `data-safety`
     full audit; `performance` (`performance-scope`) adds the three
     gtk-perf-review leaf checklists by name, since a workflow agent cannot
     delegate; `modules_moved`, inside such a fan-out, adds `rust-hex-arch`.
     With neither flag, `code-review` runs alone. A domain lens returns
     `status: ran` with a non-empty `examined` list (files read, checklist
     sections applied), or it is a failed lens.
     `scripts/agent-topology.py validate` (in `make check-agent-skills`)
     checks the selector against the skills' trigger text in both
     directions: every path and keyword the `data-safety` description names
     is selected and every selected one is named, and the same for the areas
     of the `gtk-perf-review` description against the performance scope.
   - The script refuses, before its first agent, any change or branch name
     that is not a lowercase name (a main-branch alias such as `main`,
     `HEAD` or `@`, a ref path, a revision expression or a commit hash), a
     worktree that is not one directory under `.claude/worktrees/` named
     after the change (`args.worktree_override` relaxes the name only), a
     PR value that is not a number, `#number` or pull URL, a stage or
     carried PR that differs from `args.pr`, and a carried implementation
     report that is not complete. A stage after implementation never runs
     without a pull request.
3. **One apply agent**, which wrote none of the code, first re-runs the
   selector and stops when its flags differ from the ones the lenses ran
   under. It dedupes the findings, checks each against the code, applies the
   confirmed ones in the worktree and records the rejected ones with reasons,
   marking those a lens raised as correctness bugs. It runs the three smokes
   once when UI-prefix files changed, commits `/simplify`'s edits and its own
   together locally **before** any long gate, runs the full gates once on the
   result, each through `scripts/gate-trees.sh record`, re-dispatches a Kani
   budget run its edits make stale (`verification-runs.md`, "Kani"), and
   pushes at the end. It is the last agent that edits code. A read-only
   agent then confirms its `HEAD`, its gated tree and the push, and the
   script stops on any difference.
4. **The final review, merge and archive agent edits nothing.** It reviews
   exactly the apply range the script passes (the lens `HEAD` to the apply
   `HEAD`, merge commits included), checks that `HEAD^{tree}` is the gated
   tree before "Merging" step 2, and re-checks every correctness finding the
   apply agent rejected. "No findings" is not the merge test, because every
   review pass returns unverified findings: it blocks only on a confirmed
   defect of the apply range, a wrongly rejected correctness finding, or one
   in the main-checkout guard, the main-branch protection or the push path,
   and then it returns `blocked` and fixes nothing; the coordinator sends the
   change back through an implementation round and the review stages. An
   `origin/main` merge under `MERGE-LOCK` that conflicts is aborted, the lock
   released and the run blocked; a clean one joins the reviewed range. It
   also checks what the skills do not: every `tasks.md` box, the verdicts,
   that the data-safety audit ran when it was owed, and the docs. Then it
   follows "Merging". Its verdict is one of a fixed set, and a missing or
   unknown verdict stops the run as unfinished.

## Workflow agents

- **The script guards the main checkout around every stage.** Before and
  after each stage, and around each parallel lens group, a guard agent reads
  the main checkout's `git status --porcelain` (as a sha256 of the output,
  untracked files included), `HEAD` and branch, and whether this change
  holds an implementation slot or `MERGE-LOCK`, and the script stops the run
  on any difference (porcelain alone misses a commit or a branch switch; the
  slot and lock readings catch a review fork that took one). The comparison
  is before against after; the coordinator does not move the main checkout
  while a run is in flight, and if it must, the run stops at the next guard.
- **Resuming after a guard stop.** The stop message prints a token bound to
  the stage and to every field of the after reading. When the difference is
  not the run's doing, the coordinator resumes the same run, same mode and
  arguments, with that token in `args.guard_ack`; the cached stages are
  reused, and the token accepts that one stage's reading and nothing else.
  `args.guard_ack` and `args.accept_empty_review` are read by the script
  only and never reach a prompt.
- Every command names its target: `git -C <worktree>` or absolute paths,
  because `cd` does not persist between a workflow agent's calls.
- Each agent sets `TMPDIR=$H/<change>/tmp/<stage>` (`testing.md`, "Temporary
  Files", still applies), runs every harness or probe under `timeout`, writes
  its brief to `$H/<change>/brief-<stage>.md` and re-reads it before its final
  check.
- **Outward writes are limited to this list:** pushing the change branch and
  its twin Kani ref, and deleting both after the merge ("Durability");
  creating and updating the change's pull request and merging it under
  `MERGE-LOCK`; the archive push to `main`; `kani.yml` dispatches for shard
  budgets. An agent changes no other external state: no Sonar issue status
  or quality-gate edits, no GitHub settings, rulesets or labels, no
  upstream comments, no releases. It writes any such need to
  `$H/<change>/host-asks.md` for the maintainer.
- The observed behaviours behind these rules (cwd, forks, schemas, return
  paths) are recorded outside the repository in the maintainer's note
  `~/.claude/rules/workflow-agents-observed-behaviour.md`, as evidence only;
  the rules above stand without it.
