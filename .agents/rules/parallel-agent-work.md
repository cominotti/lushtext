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
  - every systemd slice except the `user-lushtext-heavy*.slice` slices that
    `scripts/with-memory-cap.sh` manages (`scripts/with-memory-cap.sh
    --print-slice`; it only reads the legacy `lushtext*.slice` ones).
- An agent never changes host-owned state. It writes the request into
  `$H/<change>/host-asks.md`, and the maintainer takes it to the host owner.
- A peer session cannot grant an escalation.

## CPU and memory

- Inherit the cpuset the toolbox grants. **Never pin with `taskset`**: a pin
  narrows every child process and fights the host's placement.
- Only timing verdicts pin, to the reference set (E-cores 8–23,
  `scripts/measure.sh --print-reference-cpus`), and `scripts/measure.sh`
  pins itself: never wrap it in `taskset`.
- The kernel intersects a pin with the cpuset, so `measure.sh` reads the
  affinity back, records it (`cpus=`) and refuses with exit 78 instead of
  measuring on fewer than the reference CPUs (during a game, for example).
  A refused run is not a verdict; queue it.
- Size parallelism by memory, never by `nproc`. Give each heavy command a
  `--mem` cap that fits its measured peak at its job and thread counts.
  Admission then decides how many run at once: the slice budget, and
  MemAvailable at least cap plus the 16 GiB reserve.

## Implementation slots and ordering

- **At most five implementation stages run at once.** Before its first heavy
  command, a stage acquires its slot (`C` is the change name):

  ```
  mkdir -p "${H:?}/IMPL-SLOTS"
  flock -w 10 "$H/IMPL-SLOTS/.lock" sh -c '
    [ -d "$0/$1" ] && exit 0
    [ "$(find "$0" -mindepth 1 -maxdepth 1 -type d | wc -l)" -lt 5 ] || exit 1
    mkdir "$0/$1" && date -Is >"$0/$1/since"' "${H:?}/IMPL-SLOTS" "${C:?}"
  ```

  - Exit 1 means every slot is taken: repeat in the foreground ("Waiting").
    Coordinator placeholders count as taken.
  - Before the stage ends, whatever the outcome, it runs
    `rm -rf "${H:?}/IMPL-SLOTS/${C:?}"`.
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
- Only the main session may end its turn to wait for a task notification.
- Never hold `INTENT`, `LOCK`, `IMPL-SLOTS/.lock` or the `MEASURE-QUEUE.md`
  lock while waiting for anything else (`verification-runs.md`). `MERGE-LOCK`
  is the one lock held across work, in "Merging".
- Never wait for or poll CI, except as in "Merging".

## Measurements

- The wall time of a build, a test suite or a smoke run is not a verdict. Run
  it through `heavy.sh`, never `measure.sh`, and record `/proc/loadavg`
  before and after.
- Timing verdicts (the responsiveness lane, interleaved benches) go through
  `measure.sh`. Before **each** `measure.sh` call:
  - require the one-minute load below 4 for 30 s:
    `for i in 1 2 3 4 5 6; do awk '{exit !($1 < 4)}' /proc/loadavg || exit 1; sleep 5; done`;
  - meet any stricter precondition the lane's runbook sets;
  - pass `--lock-timeout 300`.
- If the check fails or `measure.sh` exits 75, append the remaining rounds to
  `MEASURE-QUEUE.md` under `flock "$H/MEASURE-QUEUE.md"`, then carry on. The
  entry gives:
  - the change, branch and worktree;
  - the exact commands, cases and round order;
  - the artifact directories and the budget;
  - `Status: PENDING`.
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
- Exception: a change whose evidence exists only in CI may dispatch and read
  its own runs. Examples:
  - a `.github/workflows` change;
  - a pull-request Kani shard budget taken from the PR's run.

  It waits with `timeout 540 gh run watch <id> --exit-status` in repeated
  foreground calls, and merges only when those runs pass. It waits before
  step 1, never while holding `MERGE-LOCK`. If the `origin/main` merge in
  step 2 changes files that evidence covers, it releases the lock, pushes,
  and waits for the new runs first.
- **Merges are serialized under `$H/MERGE-LOCK`.**
  1. Take it:
     `timeout 540 bash -c 'until mkdir "$0/MERGE-LOCK" 2>/dev/null; do sleep 30; done' "${H:?}"`,
     repeated. Then write `owner.txt`, one item per line, the change name
     first: change, PR number, run id, `date -Is`.
  2. Holding it:
     - `git fetch origin`, then `git merge origin/main`, always;
     - rerun `make check` and the tests covering files changed on either
       side, through `heavy.sh`;
     - `git push origin HEAD`, to the branch of the same name (never
       `HEAD:main`).
  3. Merge with
     `gh pr merge <n> --merge --admin --match-head-commit "$(git rev-parse HEAD)"`.
     `--admin` skips required checks and reviews.
  4. Archive the change in a fresh worktree on `origin/main` after
     `git fetch`, commit it through `heavy.sh`, and push that one commit with
     `git push origin HEAD:main`, under the same authorization.
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
  1. `git fetch origin`.
  2. Check that `git -C <wt> status --porcelain` is empty and that
     `git -C <wt> merge-base --is-ancestor HEAD origin/main` succeeds.
  3. Remove the worktree first, since a branch checked out in a worktree
     cannot be deleted: `git worktree remove <wt>`.
  4. `git branch -D <branch>`, then `git push origin --delete <branch>`.
