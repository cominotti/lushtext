# Design: adopt-faster-change-flow

Timing: none (no timed path touched). Kani: none (no proven module touched). GTK axioms: none (no toolkit behaviour relied on).

## Context

The maintainer's decisions of 2026-11-18 (ledger line 155) accept interventions 1-5 and decisions 1, 2, 3, 4 and 6 of the change-flow analysis, reject decision 5 and leave decision 7 open. Later the same day the maintainer approved decision 7 with conditions, owned by the smoke-gate-coherence change (ledger 18:36), and replaced the quiet-window criterion (ledger 19:39, D7). They also ask that the decisions become permanent for future sessions and that the adjustments be reviewed. The analysis itself is copied into `docs/memorials/2026-11-18-change-flow-time.md` (task 1.1). This design records only the figures each decision depends on.

### Measured baseline (from the analysis; method and limits in the memorial)

| Figure | Value | Kind |
|---|---|---|
| Agent-hours per change, median / mean (24 changes) | 5.2 h / 7.2 h | measured |
| Before / after 2026-11-14 (5 slots, merge on local evidence), median | 5.2 h / 5.5 h | measured |
| Before / after, mean | 5.5 h / 8.4 h (four changes above 10 h) | measured |
| Tool time rise in the median change after 11-14 (upper bound of contention) | 0.54 h | measured |
| `git commit` with hook, median before / after | 1.2 / 2.2 min | measured (n = 120 / 140) |
| Gates under `MERGE-LOCK` after a no-op merge, `stream-replace-all` | about 28 min | measured |
| `kani.yml` dispatch-and-watch inside stage 4, #103 | 12:04-13:16; net block about 20-25 min | measured |
| Spec review stage, median | 17 min | measured |
| `slice-large-directory-collapse` | 26 agent-hours over 93 calendar hours; design past R14 | measured |
| Desktop load during measurement windows | 5-6 (coordinator's report); the `top` snapshots name a game, an IDE toolbox at about 150 %, Java, a browser and the file indexer | reported; process names measured |
| Share of wall time in lock and slot waits | 0.3 % | measured |
| PR merges among the last 25 first-parent merges on `main` (20 PRs; the other 5 are branch merges) that the D5 allowlist admits | 2 of 20 (#116 eligible, #98 `declare`). Stage 1's hand denylist admitted 5, but #113 and #101 edit scripts that `check-policy` gates run (`check-flatpak-source-tree`, `test-flatpak-install-origin`), and #102 edits `.gitignore` | measured (`git diff --name-only` per merge; stage 1, corrected at stage 2) |
| `kani.yml` concurrency | group `kani-${{ github.event.pull_request.number \|\| github.ref }}`, `cancel-in-progress: true` | read from `.github/workflows/kani.yml` |

The figures say the decisions here are about calendar time and tail changes (design loops, measurement queues), not the median change. The analysis estimates the median change goes from 5.2 h to about 4.5 h from engineering fixes. The rules here add roughly another 30-50 min in the cases they apply to. Applied to the median change, those are estimates, ±30 %.

### Is the change worth doing as described?

Yes, with five refinements, each justified below:

1. The three scripts become **one** script with modes. Reason: their five shared prompt fragments (`RULES`, `KANI`, `EFFICIENCY`, `SLOTS`, `MEASURE`) and the stage-4 merge text are byte-identical copies today. Three versioned copies would drift without a gate.
2. The fast lane is an **allowlist** (stage-2 correction; stage 1 wrote a denylist). Only documentation, OpenSpec artifacts and agent guidance can be fast, guidance needs a declaration, and the two change-delivery rule files are never fast. Reason: a hand denylist fails open. Stage 1's list missed the GTK Lush runtime crates, `crates/lushtext/tests/`, the Cargo manifests, `.cargo/`, `build-aux/`, `.github/actions/`, the scripts `check-policy` runs, and this change's own lens-routing inputs. A gate edit, or an edit to the change-delivery rules, is where the quality floor erodes, so it keeps an independent spec review, which costs 17 min; other guidance is fast only with a declaration that stage 4 verifies.
3. Calibration rounds are taken from the load as it occurs, not produced by a pinned load generator. Reason: the rules forbid agents to pin (`parallel-agent-work.md`, "CPU and memory"). The cpuset is host-owned, and the maintainer rejected decision 5, which was about steering desktop apps. (Stage-3 revision: the maintainer's level 2 calibration, queued by the coordinator, follows the same principle; see D7.)
4. B1 adds a cheap strengthening: before `gh pr merge`, re-fetch and require `origin/main` to be an ancestor of `HEAD`. Reason: today `main` can move between the lock's merge and `gh pr merge`, through a direct docs push or an external session. GitHub's merge commit would then have a tree no gate ran on. The rule "every gate on every distinct tree that reaches `main`" needs this check.
5. The slot snippet becomes a tested helper (`scripts/impl-slots.sh`). Reason: the cap now depends on parsing the queue, and an untested inline `awk` in a prompt is how the cap would silently fail open.

## Decisions

### D1. No-op merge reuses recorded gate evidence (intervention 3)

- **Evidence record.** After a gate passes, the stage records it with `scripts/gate-trees.sh record <worktree> --gate '<gate>' -- <the heavy.sh command>`, which appends one line to `$H/<change>/gate-trees.log`: `<date -Is> <gate> tree=<git rev-parse HEAD^{tree}> result=pass log=<path>`. The lookup is `scripts/gate-trees.sh lookup <worktree> --gate '<gate>'` (exit 0 with the line, 1 with none). Both are a tested helper (stage-2 addition), for the reason given in refinement 5: an agent appending to a log by hand is how a dirty run would get recorded.
  - It writes the line only when `git status --porcelain` printed nothing before the gate started and nothing after it ended, and `HEAD^{tree}` did not change in between. So `record` takes both readings itself and runs the gate command as its child; `heavy.sh` stays the inner wrapper, one command per call. Gates on a dirty worktree record nothing.
  - A full `make test` pass on tree `T` also covers any subset of tests the merge step owes on `T`.
  - `<gate>` is the `make` target with its variables, for example `make check`, `make test` or `make test-widget-shard WIDGET_SHARD=editor-page`. The `heavy.sh` wrapper and its envelope are not part of the key: they change placement, not the result.
- **Under `MERGE-LOCK`, step 2.**
  1. After `git fetch origin` and `git merge origin/main`, compute `T = HEAD^{tree}` and require a clean status.
  2. Each gate the step owes (`make check`, `make test` or the affected tests, the three smokes when they are owed) is skipped exactly when `gate-trees.log` has a passing line for that gate with `tree=T`. The agent states which lines it reused.
  3. Any other case runs the gate as today. That covers a merge that changed the tree, and a tree whose last full gate run was before stage-4 fixes. Stage 4 reruns only the gates its fixes affect, so a fixed tree often has no full record.
- **Before `gh pr merge` (new).**
  1. `git fetch origin`, then `git merge-base --is-ancestor origin/main HEAD`.
  2. If that fails, `main` moved: go back to step 2.
  3. If it holds, GitHub's `--merge` commit has the tree `T` the gates ran on.
- **Why identical trees suffice.** Every gate input that can differ between two runs on one tree is outside the tree: the toolchain, the host, the smoke artifacts. It is the same for a rerun under the lock today, so reuse loses no assurance a rerun would give.
- **Rejected:**
  - Skipping on a docs-only merge. A "docs-only" merged-in diff can carry `.agents/` or `openspec/changes/` entries that `check-kani-shards` and `check-agent-docs` read. Only byte identity is safe.
  - A commit-SHA key. A merge commit with an identical tree, or a re-made commit, would miss a key it should hit.

### D2. Kani budget dispatch at the end of implementation (intervention 5)

- **What the gate does today.**
  - `make check-kani-shards` accepts `PENDING DISPATCH (owed by <change>)` while `openspec/changes/<change>/` is active, and fails once it is not. Code: `scripts/kani-shards.py` `pending_problem`; rule: `build.md`, "Shard gates and budgets".
  - So the archive commit is refused while the figure is owed. This stays as it is: the archive still records the figures.
- **Concurrency of the runs.** `kani.yml` cancels an in-progress run of its group, and its group is the PR number or the ref. So two dispatches on one ref cancel each other, which is why #103 ran its two one after the other. A dispatch and the PR's own run use different groups and do run at once.
- **Stage 3.** As soon as the final code is pushed and the PR is open, stage 3 dispatches without watching:
  - **A pull-request shard:** one dispatch on the branch. The PR's own Kani run is the second measurement.
  - **A scheduled shard:** two measurements are needed, and the PR run does not run scheduled shards. Stage 3 pushes the same commit to a twin ref, `<branch>-kani-2`, and dispatches once on each ref. The two runs use different groups, so they run at once. The twin ref is deleted after the archive. If pushing the twin ref is refused, it dispatches once, and stage 4 dispatches the second run when it reads the first as finished. It does not watch either run.

  It records the run ids in `$H/<change>/kani-dispatch.md` and in the PR body.
- **Stage 4 reads, never watches** (`gh run view`).
  - **One recording path.** The figures always land in the archive commit, never on the branch before the merge. Recording them on the branch would change `HEAD^{tree}`, void the D1 records and force a full `make test` under the lock. The archive commit edits only the `measured_in` text and figure fields of the pending records in `scripts/kani-shards.py`. Stage 4 checks that its diff touches nothing else.
  - **Finished green, within margins:** merge, then archive as today, with the figures in the archive commit.
  - **Finished red, or over a margin:** a seen failure blocks the merge (`parallel-agent-work.md`, "Merging"; `preexisting-blockers.md`). Fix it in the work stream.
  - **Not finished by step 3:** merge on the local evidence. Correctness evidence is unchanged: the shards whose harness inputs the change edits ran locally and passed (`verification-runs.md`, "Kani"). Then:
    1. Release `MERGE-LOCK` and append `ARCHIVE OWED <change>: kani <ids>` to the ledger.
    2. Wait in the foreground outside every lock, as in "Waiting".
    3. Retake `MERGE-LOCK` for the archive commit alone. A green result is recorded in that commit. A red one keeps the change active, and the fix goes through a new PR in the same change before the archive.
  - **The archive tree's gates.** `scripts/kani-shards.py` is not on `scripts/precommit-scope.py`'s docs-only allowlist. So the archive commit's hook runs the full `make check`, including `check-kani-shards`, on the archive tree. Today a docs-only archive gets only `make check-policy`. The figure fields are read only by `check-kani-shards` and the CI matrix output (`github-outputs`).
- **Re-dispatch.** The stage-3 runs measure the stage-3 tree. Re-dispatch after the final push when stage-4 fixes, or the `origin/main` merge, touch the harness inputs of a shard whose figure is pending. Harness inputs are found by the "Kani" policy: `kani-shards.py oracle` plus the `grep` over `kani_proofs.rs`. Otherwise the stage-3 runs stand.
- **Rule edits:**
  - `verification-runs.md`, "Shard budgets": when to dispatch;
  - `parallel-agent-work.md`, "Merging": the CI exception now covers only evidence that is correctness evidence, and budget figures are archive evidence;
  - `build.md`: one sentence pointing to the dispatch timing.
- **Rejected:**
  - Waiting while holding `MERGE-LOCK`. "Merging" forbids it.
  - Deferring the dispatch to stage 4. That is today's cost.
- **Loosening check.** Today a budget-only CI result is read before the merge. After this change it is read before the archive. The rule that every gate runs locally on every tree is not affected: budgets are a fit to the CI runner, and the local gate still enforces the recorded figures. Merge on local evidence (2026-11-14) already defers every unread CI result.

### D3. Non-inferiority merges (decision 1)

- **The class is set before any verdict round.** `design.md` states one of two classes, with a reason:
  - `Verdict class: non-inferiority`: the change's value does not depend on its improvement claim, such as a correctness, safety or structure change on a timed path.
  - `Verdict class: improvement-gated`: the change's purpose is the improvement, or its mechanism trades metrics so branch and `main` are not ordered. Example: #109 trades completion time against the worst frame.

  Stage 2 confirms the class. Once a verdict round exists, only the maintainer can change it, so a failed improvement verdict cannot be relabelled.
- **What the non-inferiority verdict pre-registers:**
  - the primary metric per lane case;
  - a numeric noise band per metric, taken from an earlier main-versus-main spread or stated with its reason;
  - the touched lane cases.
- **Pass bar.** The verdict passes when both hold:
  - for every touched case, the branch median is no worse than `main`'s median plus the band;
  - no case that `main` meets within `B_INTERACT_US` or `B_SETTLE_US` goes over that budget on the branch.
- **How it runs.** It runs like every verdict today: four rounds interleaved `main` → branch → `main` → branch, through `measure.sh`, judged quiet by the criterion of "Measurements" (D7). **It blocks the merge.** A queued non-inferiority verdict still means "ready, measurement pending".
- **The improvement verdict** is queued against a follow-up named in the ledger as `NEW CHANGE PROPOSED: <change>-improvement-verdict`, because the change itself is archived.
  - Until that verdict passes, no document claims the improvement: README, `AGENTS.md`, `CHANGELOG`, programme records and the PR body.
  - The change's own requirements do not assert the improvement bar. That requirement belongs to the follow-up.
  - If the follow-up fails, an independent agent decides whether to keep the code (it is non-inferior) or revert it, and the maintainer is told.
- **Rejected:**
  - Queuing the non-inferiority verdict too. That would merge timing-path code with no verdict, which breaks the quality floor.
  - Classifying after the first measurement. That is the relabelling the freeze prevents.

### D4. Design-loop limit (decision 2)

- **Counting.**
  - A design revision is a run of the revise mode's stage 1 on an existing change, or any rewrite of `design.md` decisions after a blocked stage 4 or a failed pre-registered gate.
  - `design.md` keeps a running list, `## Design revisions`, with one line per revision giving its date, trigger and id.
  - The original design is revision 0.
- **The trigger.** A change with two revisions is blocked again. That happens when its stage 4 blocks, or a pre-registered gate fails and calls for a third revision.
- **Independent decision.**
  - An agent that wrote none of the change's design or code reads the design, the gates, the evidence and the attributions so far. It returns `continue`, `re-scope` or `shelve`, with reasons.
  - The coordinator records the decision in the ledger and tells the maintainer.
  - This follows the standing instruction to decide autonomously with every decision reviewed by a subagent, and it adds the report to the maintainer.
- **Attribution spike.** If the decision is `continue` or `re-scope`, the third revision starts with an attribution spike:
  - one agent;
  - no edits to OpenSpec artifacts;
  - its output is `$H/<change>/spike-<n>.md`, saying where the time or failure goes, with figures;
  - attribution rounds go through `measure.sh` or are queued, as today.
  The revision then cites the spike.
- **Shelve.**
  - The ledger and the programme record mark the change `SHELVED <date>: <reason>`.
  - Branch, worktree and PR stay as they are, and no lock or slot is held.
  - It resumes only through the coordinator.
- **In-flight changes.** The rule applies at their next trigger, without retroactive stops. #109 is past two revisions, so its next block triggers the decision.
- **Rejected:** asking the maintainer for each decision. That conflicts with the standing autonomy decision.

### D5. Fast lane (decision 3)

- **Eligibility: an allowlist, fail-closed** (stage-2 correction). `scripts/check-fast-lane.py --base <ref>` classifies each path: the planned file list at stage 1, and `git diff --name-status --find-renames <base>...HEAD` (both sides of a rename) plus untracked files at stage 4. Every path gets one class:
  - **`eligible`**: under `docs/` or `openspec/`, or a root-level `*.md` file not listed below;
  - **`declare`**: agent guidance: `.agents/rules/**` except the two change-delivery rule files below, `.agents/skills/**/*.md`, `.agents/skills/*/agents/*.yaml`, the root `AGENTS.md`, `SOUL.md` and `.claude/CLAUDE.md`. (The `Makefile` was `declare` until the fix round, which made it not eligible because it wires every gate: Fable S7.) An `eligible` path that a gate names literally (the path string appears in `Makefile`, `.githooks/`, `.github/` or `scripts/`, for example `docs/automation-reference.md`) is promoted to `declare`;
  - **`not eligible`**: every other path. That includes all of `crates/` (shipped app and GTK Lush runtime crates, proven modules, widget and axiom tests, proof tools), `resources/`, `data/`, `po/`, `scripts/`, `.githooks/`, `.github/`, `.claude/workflows/`, `.agents/skills/skill-policy.toml` and any other non-guidance file under `.agents/skills/`, `Cargo.toml`, `Cargo.lock`, `workspace-hack/`, `.cargo/`, `.config/`, `build-aux/`, `build/`, `meson*`, `snap/`, `fuzz/`, `formal/`, `fixtures/`, `samples/`, `.gitignore`, the lint and deny configurations, and the change-delivery rules `.agents/rules/parallel-agent-work.md` and `.agents/rules/verification-runs.md`. Those two are `not eligible` because "Process rule changes keep the quality floor" requires a spec review for any change to them (stage-2 correction); neither recent fast-lane PR touches them, so the yield is unchanged.

  A new top-level directory is `not eligible` until a change adds it to one of the first two lists, so a new crate or gate can never be fast by omission. Proven modules need no table of their own: every `kani_proofs*.rs` and every `ORACLES` module is under `crates/` (checked at stage 2 with `git ls-files`), and the self-test asserts that this still holds. The design also must not plan a timing verdict.
- **Guidance is allowed, with a declaration.** Rule and skill text is not excluded, because `documentation.md` makes most changes touch it, but it is where the floor erodes, so the checker reports it as `declare`. `proposal.md` must then hold a "Gate effect: none" section, which says for each of those files that no gate is narrowed, removed or re-keyed and no item of the quality floor changes. Stage 4 verifies the declaration against the diff, line by line. A false declaration counts as a violation. Nested `AGENTS.md` files all live under `crates/`, so they are `not eligible`; those under `crates/lushtext-core/src/ui/` also make a commit owe the three smokes (`testing.md`, "Files under the UI prefixes need the three smokes").
- **Yield.** The allowlist admits 2 of the last 20 PR merges (#116, #98); see the baseline table. That is the honest yield of "outside shipped code, proofs and gates". The rule is not widened to recover stage 1's 5: the three it drops edit gate scripts or `.gitignore`, which is the class the fast lane must keep out.
- **Stages in the fast lane:**
  1. Stage 1 writes `proposal.md`, the delta specs and `tasks.md`. `design.md` is optional: `openspec/config.yaml`'s design rules apply only when `design.md` exists, so `proposal.md` carries a "Decisions" section. `openspec validate --strict` accepts a change without `design.md`: checked at stage 1 on 2026-11-18 with a scratch copy of this change minus `design.md` ("Change 'fl-probe' is valid"; task 2.4 re-checks it on the CLI version in use at implementation).
  2. Stage 1 also does the spec-review checklist itself and records it in `proposal.md` under "Self-review".
  3. Stage 3 is unchanged.
  4. Stage 4 does the independent code review, the independent review of the spec artifacts, and the eligibility check on the real diff.
- **Violation.** If stage 3 or stage 4 finds a path that is not eligible, or a `declare` path without a verified declaration, the change leaves the fast lane. An independent spec review runs before any further implementation or the merge. The workflow script makes this a hard branch.
- **Rejected:**
  - A denylist of excluded prefixes, stage 1's form (refinement 2). It admitted `scripts/flatpak-source-tree.sh`, the widget tests, the GTK Lush runtime crates and the lens-routing inputs, and a new crate would have been admitted by omission.
  - The narrower `{ui,services,model}` set.
  - Excluding all guidance text. It would leave 1 of 20 PR merges (#116), and the declaration plus stage-4 verification already covers the risk.
  - Dropping the stage-4 code review. The analysis classes that as the one stage merge that lowers quality.

### D6. Slot cap from the queue (decision 4)

- **The cap.**
  - 5 by default.
  - 3 while `$H/MEASURE-QUEUE.md` holds at least one **ready unit**. A unit is a `## ` entry, or each `### ` part of an entry that has parts. A unit is ready when its **last** `- Status:` line starts with `READY`. Queue entries already carry several `Status:` lines, which is why only the last one counts. Parts count separately because the live `stream-replace-all` entry keeps one `Status:` per part: judged per `## ` section, a `READY` part followed by a `DONE` part would leave the cap at 5 (stage-2 correction).
  - **What `READY` means** (stage-2 correction). `READY <sha>` marks rounds that are runnable now and that a change is waiting on: a verdict campaign, or pre-registered gate or selection rounds whose result decides the change's next step. The live `slice-large-directory-collapse` "D25 gates" entry (`Kind: SELECTION ROUNDS`, `Status: READY 5855754a`) is one, and ledger line 155 already counts it. `PENDING` marks rounds that are not runnable yet, or that no change is waiting on: non-blocking attribution, and the calibration of D7, which would otherwise hold the cap at 3 for as long as it waits for outside load.
  - **The entry rule changes with it** (`parallel-agent-work.md`, "Measurements"). Today the rule says a stage queues every entry as `Status: PENDING`, so a ready campaign queued by the rule would sit outside the count, and the cap would fail open. From now on:
    - a stage queues rounds it waits on, whose tree is final, as `Status: READY <sha>`, where `<sha>` is the commit the rounds measure;
    - rounds it will wait on that are not runnable yet stay `PENDING (NOT READY: <reason>)` until their owner replaces the line with `READY <sha>`;
    - non-blocking attribution and calibration entries stay `PENDING`.
- **`scripts/impl-slots.sh`:**
  - `acquire <change> [--timeout S]` waits in the foreground, retrying every 30 s, at most `S` seconds, default 540. Exit 0 means acquired, 124 means timed out.
  - `release <change>` removes only that change's slot.
  - `cap` prints the cap and the `READY` entries behind it.
  - It reads the queue without the queue lock, so a torn read miscounts one attempt at most. It never nests the queue lock inside `IMPL-SLOTS/.lock`, which keeps the "never hold a lock while waiting" rule.
  - Slots already held above the cap are not evicted. New acquisitions wait.
- **`--self-test`:**
  - fixture queues for:
    - no entries;
    - one `READY`;
    - a stage-queued verdict `Status: READY <sha>`;
    - `Kind: SELECTION ROUNDS` with `Status: READY <sha>`;
    - `READY` superseded by a later `RAN`;
    - a `PENDING (NOT READY …)` verdict;
    - a `PENDING` calibration entry;
    - a `READY` in an earlier entry with a later entry `DONE`;
    - an entry with `### ` parts, where part B's last status is `READY` and part C's is `DONE` (counts);
    - a copy of the live queue's structure at stage 2 (one ready unit);
  - a temporary slot directory for acquire, wait, timeout and release.
  It joins `make check-agent-helpers`.
- **Rejected:**
  - A fixed 3. It would lower throughput more than it shortens a change: 13 merges in 19 h with 5 slots.
  - Counting every non-`DONE` entry. Pending attribution entries would hold the cap at 3 for days.
  - Reserving `READY` for final verdict campaigns, stage 1's form. The live selection rounds that ledger line 155 counts would have had to be relabelled `PENDING`, and they lower the cap for the same reason a verdict does: a change waits on a quiet window.

### D7. Quiet-window criterion: drain first, judged on busy CPUs (decision 6; maintainer decision 2026-11-18 19:39, level 1)

Stage-3 revision. Stage 1 and stage 2 wrote "calibrate, do not change": keep the 1-minute load below 4 and queue a calibration of a reference-CPU criterion. At 19:39 the maintainer found the load rule too strict and approved three levels (ledger, 2026-11-18T19:39:20; coordinator note of 19:47). This decision now carries level 1 only.

- **Why the old criterion failed.** In the #109 G1 window, 3 of 4 tries in 2 h 40 min exited 75: our own `make visual-geometry-smoke` and `make check-clippy` held `LOCK` past the 300 s lock timeout. The fourth was voided at a pre-drain load of 5.69. The 1-minute load lags: just after `measure.sh` drains our heavy work it still holds that work's average, so a pre-lock load check refuses windows that are quiet by the time the round runs. Desktop apps alone hold the load at 5-6.
- **Level 1, in force with this change** (`parallel-agent-work.md`, "Measurements"):
  1. **Outsiders first, holding nothing.** Wait until host CPU use outside our own work is below 3.5 CPUs over 30 s. Our own work is the `user-lushtext.slice` cgroup (the parent of `scripts/with-memory-cap.sh --print-slice`), the legacy `lushtext-heavy.slice`, and the shared `cst-sccache-*.scope` siblings, which can run our compiles. Reason: our own heavy work drains under `measure.sh`'s `INTENT`; only outside load decides whether a window can be quiet.
  2. **Drain.** `measure.sh --lock-timeout 2400`, long enough for the longest heavy command to finish under `INTENT`, instead of 300 s.
  3. **In-lock gate.** Inside the exclusive lock, sample busy CPUs (`/proc/stat`) for 30 s. Run the round only below 4. A round is **kept** when it exits 0 and both the busy CPUs over the whole command (the round's own use included, as the old "after" load was) and a 30 s sample after it are below 4.
  - The bar stays 4. Only the metric changes, from the 1-minute load to busy CPUs. `measure.sh` still records the load, which is no longer the criterion.
  - **This admits rounds the old rule refused.** Busy CPUs are at most the load: the load also counts runnable tasks waiting for a CPU and tasks in D state. That is a loosening, and it is the maintainer's decision. The floor table records it.
- **The helper.** `scripts/quiet-gate.py` replaces the coordinator's reference scripts (`rev/busy.py`, `rev/gated-run.sh` of `slice-large-directory-collapse`); nothing in a rule points at a handoff script.
  - `outsiders [--window S]` prints the outside CPU use; `wait-outsiders [--below 3.5] [--window 30] [--timeout S]` waits holding nothing (exit 0, or 124 at the timeout); `busy [--window S]` prints busy CPUs.
  - `run --label L --log FILE [--mode gate|calib] -- <command…>` is the in-lock wrapper. In `gate` mode it exits 76 without running the command when the pre-sample is not below 4, and 77 when the command exited 0 but the run or post sample was not below 4 (ran, not kept). Otherwise it returns the command's status. `calib` records the samples and never gates. Every reading goes to the log as one line.
  - It fails closed: an unreadable `/proc/stat` or `cpu.stat`, or an unresolvable slice, exits 2 and prints no reading. Exit 76 and 77 are distinct from `measure.sh`'s 75 and 78.
  - Windows, thresholds and the cgroup and stat sources are overridable, so the self-test in `make check-agent-helpers` never sleeps 30 s and reads no real slice.
- **A stage agent and the 2400 s timeout.** A foreground Bash call lasts at most 600 s. A stage agent runs the round detached (`setsid nohup`), as scheduled Kani shards do, and waits with `scripts/wait-for.sh --pid <pid> --timeout 540` in repeated foreground calls. If `wait-outsiders` times out, or `measure.sh` exits 75, the remaining rounds are queued as before.
- **Level 2, the calibration.** The coordinator's queue entry "Calibration: G0 rounds under outside load" (`Kind: CALIBRATION`, `Status: PENDING`) is the experiment; this change appends no other entry. Its pre-registered reading (direction: load never makes a round look better; magnitude: reported) is in that entry.
- **Level 3 is not written into any rule.** For absolute budget gates, a PASS under load would count and a FAIL under load would be inconclusive. The maintainer approved it conditionally: it takes effect only after level 2's direction reading holds, and then by a rule edit of its own. Comparative and improvement verdicts stay quiet-only either way. The memorial records the condition.
- **Rejected:**
  - Stage 1's calibration of a per-reference-CPU criterion (reference mean busy < 0.10). The reference set 8-23 is the toolbox's whole affinity, and the sample showed about 50 % busy on each reference CPU, so the candidate arm could not fill.
  - Keeping the 1-minute load and only raising `--lock-timeout`. The lagging average still voids the drained window.
  - Applying level 3 now. Its premise is what level 2 tests.

### D8. Versioned workflow scripts

- **Location.** `.claude/workflows/programme-change.js`, tracked; `.gitignore` covers only `/.claude/worktrees/` and `/.claude/handoffs/`, checked with `git check-ignore`.
  - The guaranteed way to run it is `Workflow({scriptPath: '<main checkout>/.claude/workflows/programme-change.js', args})`.
  - `Workflow({name: 'programme-change'})` also works if the harness registers project workflows by name. The rule documents both, with `scriptPath` as the fallback.
  - `precommit-scope.py` treats `.claude/**` as docs-only, so editing the script alone skips Clippy, and `make check-policy` still runs.
- **Modes,** through `args.mode` (the coordinator's 18:50 note):
  - `new`: archive previous (optional), spec, spec review, implementation, then the post-implementation stages of D10.
  - `fast-lane`: as `new`, but the spec and spec-review stages are one agent (D5).
  - `revise`: design revision in place. With `args.revision >= 3`, the D4 decision agent runs first, and on `continue` or `re-scope` the spike agent, before the revision. `args.design_only` stops after the review.
  - `continue`: implementation rounds, then the D10 stages; `args.skip_impl` goes straight to them.

  Other parameters: `args.change`, `args.pr` and `args.review_agent_type` (the review stages' `agentType`, default the workflow agent). The D10 stages are one function used by every mode.
- **Stages bind to skills; their procedure is not described again.**
  - Spec: `openspec-propose` for a new change, `openspec-update-change` for a change already on `main`.
  - Implementation: `openspec-apply-change`.
  - Archive: `openspec-archive-change`.
  - Each prompt names the skill and the change, and adds only what the skill does not do: slots, gates, verdicts, the guard of D11.
- **Sanitizing.** These come from `args`, never from the file. The repository path is derived in the prompt from `git rev-parse --git-common-dir`.

  | Item | `args` field |
  |---|---|
  | push command, SSH key path | `args.local` (free text the coordinator keeps outside the repo, for example in `$H/LOCAL-ARGS.md`) |
  | gh account | `args.local` |
  | absolute repository path | derived from `git rev-parse --git-common-dir` |
  | commit trailer model name | named by `git.md` (the running model) |
- **Prompts point to rules, not copies.** Each stage prompt names the rule sections it must follow:
  - `parallel-agent-work.md`: "CPU and memory", "Implementation slots", "Waiting", "Measurements", "Merging", "Running a change";
  - `verification-runs.md`: "Kani", "Shared-machine agent work";
  - `testing.md`: "Gate Order Before a Commit";
  - `git.md`.

  Normative items that exist today only in the `RULES` string move into `parallel-agent-work.md`, "Running a change":
  - stay in your own worktree and handoff directory;
  - read `COORDINATOR-NOTE.txt` at the start and before each heavy command group;
  - headless only;
  - never `SendMessage` a workflow agent; steer it through `COORDINATOR-NOTE.txt`;
  - write commit messages to a file for `heavy.sh`.
- **`scripts/check-workflow-scripts.sh` (`make check-workflow-scripts`, in `make check-policy`).** For each `.claude/workflows/*.js` it:
  1. requires `export const meta = {` as the first statement, with `name` and `description`;
  2. builds a wrapper module: it removes the `export` keyword from `meta` and places the body inside `async function __workflow(args, agent, parallel, pipeline, phase, log, budget, workflow) { … }`, so top-level `await` and `return` parse;
  3. runs `node --check` on the wrapper;
  4. requires every `phase('<t>')` title to appear in `meta.phases`;
  5. fails on any of these patterns: `/var/home/`, `/home/[a-z]`, `/tmp/claude-`, `id_[a-z0-9_]*` key names, `ssh -i`, `GIT_SSH_COMMAND`, `gh auth`, `Co-Authored-By:`, any email address, and any run of 12 consecutive words (compared case- and punctuation-insensitively) that also appears in `.agents/rules/*.md`. A run, not a whole sentence, so a copy with one word changed is still caught.

  With no `.claude/workflows/*.js` present it passes and says so, because the helpers commit lands before the script. `--self-test` seeds each failure. With node absent, the check fails with a message naming node. Node is already required for `openspec`. CI does not run `check-policy` (it runs named targets), so the gate is local, like the other `check-policy` audits.
- **Rejected:**
  - Three scripts plus a parity test. That is more code for the same guarantee.
  - Keeping the scripts in the session directory. A new session would not find them, and keeping the rules permanent is the point of this change.

### D9. Records and ledger

- Memorial `docs/memorials/2026-11-18-change-flow-time.md`: the analysis's sections 1-4 and 8, its figures and method, and the decisions with their dates. Indexed in `docs/memorials/README.md`.
- `PARALLEL-LEDGER.md`, append-only, coordinator file (gitignored), each line written only if not already present:
  - (a) "smoke gate coherence" absorbs intervention 2 (parallel smoke cases, `--jobs N` sized by memory, after `fix-headless-session-media-wait`) and intervention 4 (the `make check` tree-hash stamp for the commit hook);
  - (b) decision 7 (drop `crates/lushtext/tests/widget/` from the three smoke fingerprint lists, with a parity test) was APPROVED by the maintainer at 18:36 with conditions (the three fingerprint lists only, the static accessibility check stays, the residual risk declared). It belongs to that change and is not implemented here. Stage-3 revision: stage 1 recorded it as open, which the ledger had already superseded;
  - (c) ordering: smoke gate coherence starts after `fix-headless-session-media-wait` merges, since intervention 2 depends on intervention 1.

### D10. Stages after implementation (coordinator notes of 18:50-19:25, maintainer-approved)

- **Order.** Implementation → `/simplify` → review lenses in parallel → one independent apply agent, which runs the full gates once → merge and archive agent. The last code-editing pass is the bug-hunting one.
- **`/simplify`.**
  - One agent, quality only.
  - It is invoked only with an absolute change-worktree path as its argument, never bare. Reason: a workflow agent's cwd is the main checkout and cannot move. `cd` does not persist, and `EnterWorktree` refuses. The coordinator's probe 3 showed an absolute worktree path keeps the edits inside the worktree.
  - It runs the tests of what it touched and leaves its edits uncommitted, so that one smoke trio covers it and the apply stage. It also saves them as `$H/<change>/simplify.patch`.
- **Review lenses, in parallel, findings only.** Each writes `$H/<change>/review-findings-<lens>.md` and returns its findings.
  - **(a) `code-review`.** The lens must review **both** the branch's commits ahead of `origin/main` and `/simplify`'s uncommitted edits (stage-2 correction: a branch target alone misses the uncommitted edits, and a path target may miss the commits). **Fix round (probe C4, Fable M1):** a path target diffs against the branch's upstream, so on a pushed change branch tracking `origin/<branch>` it reviews only the uncommitted edits; the stage-3 conclusion that one path call covers both (P2b) held only because the scratch branch tracked `origin/main`. The lens therefore runs two calls: `high <PR>` for the committed work (probes C2 and C3) and, when `/simplify` left edits, `high <absolute worktree path>` for those. The prepare stage makes the branch track `origin/<branch>` at `HEAD`, a scope reading confirms `HEAD` = PR head = upstream, and each call's quoted scope is checked against the committed and uncommitted file counts. Committing `/simplify`'s edits before the lenses would also make `high <PR>` cover everything, but a UI-prefix commit owes a smoke trio, so it stays rejected (below).
    - It never gets `--fix`: with `--fix` a forked review edited the main checkout in probe 2.
    - **Where the findings are** (stage-3 revision, notes 19:47 and 19:52). Observed return paths differ: a caller without a schema got the findings as text in the Skill result; a caller with a schema got only "Skill execution completed"; another got only `{status: "forked", agentId, "Skill execution completed"}`, with the findings as the input of the fork's last `StructuredOutput` call in `subagents/agent-<agentId>.jsonl` of the session's transcript directory. The lens has no schema, takes the findings from the result text, and reads the fork transcript whenever the result carries none. It prints `skill_findings_source: result-text` or `skill_findings_source: fork-transcript` with them; a lens output with neither is a failed lens.
    - Its agent has no schema, because with a schema the fork's output was lost. Its text is passed on verbatim.
    - A "(none)" with no explanation is a failed lens, not zero findings. The script reruns it once, then stops.
    - **Empty reviews (maintainer rule (b), fix round).** `CODE-REVIEW-STATUS: empty` is a distinct status, allowed only with `findings_count: 0` and a non-zero quoted scope (files and lines) for every call. The script stops with its own message and goes on only when the coordinator resumes the run with `args.accept_empty_review`. `findings_count:` replaces the phrase list as the backstop for `ok` (N1).
  - **(b) Domain lenses, only in a fan-out:**
    - the `data-safety` full audit;
    - the `gtk-perf-review` leaf checklists, called by name, because a workflow agent cannot delegate: `gtk-responsiveness`, `gtk-perf-scale` and `gtk-perf-rust-optimize`, in the umbrella's fixed domain order;
    - `rust-hex-arch` when modules are added, moved or split.
- **When to fan out** (19:02 note): when the change touches data or performance. Each domain lens follows its own flag (stage-2 correction; the coordinator's note says "the applicable domain reviewers"):

  | Flag from `review-lenses` | Lenses added to `code-review` |
  |---|---|
  | `data` | `data-safety` full audit |
  | `performance` | `gtk-responsiveness`, `gtk-perf-scale`, `gtk-perf-rust-optimize` (the umbrella dispatches all three domains whenever its scope is non-empty) |
  | `modules_moved`, and `data` or `performance` | `rust-hex-arch` |
  | none | none: `code-review` alone |

  - The triggers are the ones those skills already declare. Performance comes from `scripts/agent-topology.py performance-scope`, the umbrella's own scope helper over Cargo metadata and `.agents/skills/skill-policy.toml`.
  - Data safety comes from a new `[data_safety]` table in `skill-policy.toml`: the machine form of the skill's description, with path suffixes and content patterns. `validate-agent-skills.py` checks that each pattern's keyword appears in that description, so the two cannot drift.
  - A new subcommand, `scripts/agent-topology.py review-lenses --base <ref>`, prints `{data, performance, modules_moved, paths}`. It is tested in `scripts/test-agent-topology.py`.
  - In the workflow script, a pure function `selectLenses(flags)` maps the flags to lenses. The script asserts its cases at start-up (its self-check). Without a fan-out, the `code-review` lens runs alone.
- **Apply agent.**
  - It wrote none of the code. It dedupes the findings, checks each one against the code, applies the confirmed fixes in the worktree, and records the rejected ones with reasons.
  - It runs the three smokes once, when UI-prefix files changed, and commits `/simplify`'s and its own edits together.
  - It runs the full gates once on the result and records the tree in `gate-trees.log` (D1).
  - In `fast-lane` mode it is also the independent reviewer of the spec artifacts, and it re-runs `check-fast-lane.py` on the real diff and checks any "Gate effect: none" declaration (D5).
- **Merge and archive agent.** It checks what the skills do not: tasks, verdicts, that the data-safety audit ran when it was owed, and docs. Then it runs "Merging" (D1, D2) and `openspec-archive-change`. **Fix round (maintainer rule (a), Fable M3, S5, M5):** it edits nothing. It reviews exactly the apply range the script passes (lens `HEAD` to the apply `HEAD`, read back by a read-only agent with the gated tree and the push), checks `HEAD^{tree}` before "Merging" step 2, and re-checks every correctness finding the apply stage rejected. A confirmed defect, or a conflicting `origin/main` merge, returns `blocked`; its verdict is one of a fixed set, and a dead agent or an unknown verdict stops the run.
- **Rejected:**
  - `code-review --fix` from a workflow agent. It edits the main checkout.
  - Agents that spawn reviewers. A workflow agent has no Agent tool. Several reviewers come from the script instead.
  - Committing `/simplify` separately. That would cost a second smoke trio for a UI change.

### D11. Workflow-agent guard and outward writes

- **Main-checkout guard, enforced by the script around every stage:**
  - The readings come from **guard-read agents**, not from the stage agent (stage-2 correction). The `code-review` lens must have no schema, so it cannot return structured readings, and a stage agent that broke the checkout would be reporting on itself. Before and after every stage, and once before and once after each parallel lens group, the script runs a small agent (lowest effort, a schema of `{porcelain, head, branch}`) that only runs `git -C <main checkout> status --porcelain`, `rev-parse HEAD` and `branch --show-current`.
  - The script compares the two readings and stops on any difference. `status` alone misses commits, which is why `HEAD` and branch are compared too.
  - **Fix round (Fable S6, note 23:20).** The status is compared as a sha256 printed by the guard command, not as LLM-copied text, and untracked files stay in it. The reading also says whether this change holds its implementation slot or `MERGE-LOCK`, which catches a review fork that ran the slot helper. A stop prints a token bound to the stage and every field of the after reading; resuming the same run with that token in `args.guard_ack` accepts that one reading, so the cached stages are reused instead of rerunning `/simplify`, the lenses and the gates.
  - The comparison is before against after, never against `origin/main`: the main checkout may lag `origin/main` (at stage 2 it was at `bc109b04`, behind `187c6299`). The coordinator does not move the main checkout while a run is in flight. If it must, the run stops at the next guard, which is the fail-closed outcome. The coordinator checks the difference, records it in the ledger and resumes the run with `continue`.
  - Every Bash command uses `git -C <worktree>` or absolute paths.
- **Per-agent hygiene:**
  - `TMPDIR` is `$H/<change>/tmp/<stage>` (`testing.md`, "Temporary Files", still applies);
  - every harness or probe command runs under `timeout`;
  - each agent writes its brief to `$H/<change>/brief-<stage>.md` and re-reads it before its final check.
- **Outward writes (19:05 note), a new floor rule in `parallel-agent-work.md`.** A workflow agent's outward writes are only these:
  - the push of the change branch, and its twin Kani ref (D2), and their deletion after the merge and archive (`parallel-agent-work.md`, "Durability", step 4);
  - PR create and update, and the PR merge under `MERGE-LOCK`;
  - the archive push to `main`;
  - `kani.yml` dispatches for shard budgets.

  It does not change external state on its own: no Sonar issue status or quality-gate edits, no GitHub settings, rulesets or labels, no upstream comments, no releases. Such needs go to `$H/<change>/host-asks.md` for the maintainer.
- **Sonar wording.** `scripts/sonar-local.sh` (the `fail` line at about line 449) tells the reader to "mark them accepted/false-positive in SonarQube Cloud". It becomes: fix them, or record a suspected false positive in the change's `host-asks.md`; only the maintainer changes Sonar state. Rule and skill text is checked for the same instruction (stage-1 grep: none found).
- **Evidence.** The observed agent behaviours are recorded, informatively, in the maintainer's global note `~/.claude/rules/workflow-agents-observed-behaviour.md`, outside the repository. The repository's rules state the requirements with a one-line reason each, and cite that note only as the source of the observations, so a session without it still gets the rule.
- **Probes owed before the template's first use** (19:15 and 19:20 notes; results in `$H/adopt-faster-change-flow/probes.md`). Who runs each (stage-2 correction): a stage agent has no Agent or Workflow tool, and a planted-bug PR is an outward write outside the list above. So the coordinator runs the `isolation: 'worktree'` probe and the open-PR probe, and the stage-3 agent runs the others, which need only the Skill tool and local branches. The template is not used until `probes.md` holds every result and the target form and fallback match them:
  - `code-review` on an open PR with a planted bug whose code differs from the main checkout;
  - `code-review` with a local-branch target, and with an absolute worktree-path target;
  - `/simplify` with a worktree directory argument, across several files, and on changes already committed on the branch. If the committed case does not stay in the worktree, the fallback is to run it on an uncommitted staging of the branch diff;
  - whether `agent(..., {isolation: 'worktree'})` gives the stage agent a cwd inside a worktree.

## Quality floor (stage 2 and stage 4 check every row)

| Rule edit | Existing rule it touches | Why the floor holds |
|---|---|---|
| D1 reuse on identical tree | "Merging" step 2 (always rerun) | A gate is skipped only for a byte-identical, clean tree with a recorded pass, and only the tested `gate-trees.sh` writes or reads a record. The new ancestor check makes the tree that reaches `main` equal the gated tree, a strengthening. Residual: `main` can still move in the seconds between the check and `gh pr merge`; that window existed before and is now narrower. |
| D2 dispatch in stage 3; merge without unfinished budget runs | "Merging" CI exception; "Shard budgets" | Correctness shards still run locally. A seen failure still blocks. `check-kani-shards` still refuses the archive. No wait under `MERGE-LOCK`. |
| D2 archive commit carries figure fields in `scripts/kani-shards.py` | "Merging" step 4 (the archive is the only direct push) | It is still the one direct push. Its tree gets the hook's full `make check`, because that file is not docs-only. Its diff is limited to pending records' `measured_in` and figure fields, and stage 4 checks that. |
| D6 entry rule `READY <sha>` | "Measurements" (queue entries `Status: PENDING`) | Only the label changes. The verdict, its gates and who runs it do not change. |
| D3 non-inferiority merge | "Timing campaigns"; "Measurements" ("ready, measurement pending") | The non-inferiority verdict still runs through `measure.sh`, interleaved, judged quiet by the criterion of "Measurements", and blocks the merge. The class is frozen before measurement. The budgets are unchanged, and a case `main` meets cannot be broken. No improvement claim without its verdict. |
| D4 design-loop limit | standing autonomy decision; `preexisting-blockers.md` | Adds review; removes no gate. A shelved change merges nothing. |
| D5 fast lane | four-stage workflow | An allowlist: only `docs/`, `openspec/` and root Markdown are eligible, guidance needs a declared, verified "Gate effect: none", and every other path, including any new directory, is not eligible. Keeps the independent stage-4 reviewer, who now also reviews the spec. Fails closed to a full review. |
| D6 slot cap | "Implementation slots and ordering" | Fewer concurrent stages; no gate changes. |
| D7 busy CPUs replace the 1-minute load | "Measurements" (load < 4 for 30 s before each `measure.sh` call; recorded load < 4) | **A loosening, decided by the maintainer** (2026-11-18 19:39, ledger). Busy CPUs are at most the load, so some rounds the old rule refused are now kept. The bar stays 4, and the samples now describe the window actually measured, after the drain, where the old pre-lock check read a lagging average. Every verdict still runs through `measure.sh`, interleaved, pre-registered, and blocks the merge as before. |
| D7 `--lock-timeout 2400` | "Measurements" (`--lock-timeout 300`) | Changes placement only: `measure.sh` holds `INTENT` up to 40 min while our heavy work drains, so new heavy starts wait longer. No gate is skipped. |
| D7 in-lock gate and kept-round rule | "Measurements" (a round whose recorded load is >= 4 is non-verdict) | A round runs only after a quiet in-lock sample and is kept only when the run and a post sample are also below 4: three readings where there were two. The helper fails closed. |
| D7 level 3 not written | none | No rule text admits a PASS under load until the maintainer's condition (level 2 direction) is met and a later change writes it. |
| D8 scripts | every rule the `RULES` string copied | Rules stay the only normative text. The scripts point to them. The checker refuses copies and personal data. |
| D10 review stages | four-stage workflow; "an independent reviewer" | Adds `/simplify` and parallel lenses. The reviewer that applies fixes wrote none of the code. The gates run once, on the final tree. The merge agent still checks tasks, verdicts, the data-safety audit and docs. |
| D11 guard and outward writes | "Host ownership" (agents never change host-owned state) | Strengthens: script-enforced main-checkout guard; outward writes limited to an explicit list. |

Unchanged:

- data-safety review;
- an independent code reviewer on every change;
- every gate on every distinct tree reaching `main`;
- pre-registered verdicts through `measure.sh`, judged quiet by the criterion of "Measurements" (its metric changed by the maintainer's decision, D7);
- `B_INTERACT_US` and `B_SETTLE_US`;
- flakes as blockers;
- no lane, shard, scenario or assertion deleted;
- `testing.md` "Gate Order Before a Commit".

## Risks

- **A script edit breaks a future run.** `make check-workflow-scripts` catches syntax and phase drift, but not prompt semantics. Mitigation: the stage-4 review reads the script diff against the rules.
- **The fast-lane allowlist is too narrow to pay off.** At 2 of 20 recent PRs it saves one 17-min stage on about one change in ten. That is accepted: a fast lane that admits gate scripts would trade the floor for minutes. Widening the allowlist is a later change with its own declaration.
- **D3 band chosen too wide.** Stage 2 checks that the band comes from a measured main-versus-main spread or is justified. A band wider than the budget headroom is refused.

## Stage-2 review (2026-11-18)

The spec review checked the artifacts against the code, the rules, the live queue and ledger, and the last 25 merges. It confirmed D2's concurrency reading (`kani.yml`) and `pending_problem`, that only `main` pushes and pull requests trigger CI (so a twin ref starts no extra run), that `.claude/workflows/` is tracked, and the lane flags D7 uses. It changed these, each recorded where it applies:

| Finding | Change | Why |
|---|---|---|
| D5's denylist failed open: it admitted gate scripts (#113, #101), widget tests, GTK Lush runtime crates, Cargo manifests and the lens-routing inputs, and listed nested `AGENTS.md` as both excluded and `declare` | D5 is an allowlist; yield restated as 2 of 20 PR merges | The requirement says "outside shipped code, proofs and gates"; only an allowlist enforces that for paths nobody listed |
| D6 reserved `READY` for final verdicts, but the live ready entry is selection rounds that ledger line 155 counts; the per-`##` reading missed multi-part entries | `READY` = runnable rounds a change waits on; units are entries or `###` parts | The cap must see what the coordinator already counts, and must not fail open on the live format |
| D7's off-reference arm contradicts the load precondition of "Measurements"; a pre-lock probe sees heavy work `measure.sh` then drains | Explicit bounded exception with a floor row; a second probe inside the measured command, both must agree | Every exception must be explicit; the arm must describe the measured window |
| D10's `code-review` target could miss `/simplify`'s uncommitted edits; one fan-out ran every domain lens | Coverage of both is required, with a named fallback; one lens per flag | The bug-hunting lens must see every edit; the coordinator's note says "the applicable" reviewers |
| D11's guard relied on schema fields the `code-review` lens cannot have, and on the agent reporting on itself; the outward list omitted branch deletion | Guard-read agents; legitimate main-checkout moves stop the run; deletions listed | The guard must be buildable and independent |
| D11's probes included two a stage agent cannot run | The coordinator runs those two; the template is not used until all are recorded | A stage agent has no Agent or Workflow tool, and a probe PR is outside the outward list |
| D1's records were written by hand | `scripts/gate-trees.sh` with a self-test | Same reason as refinement 5 |
| `make check-agent-skills` is outside `check-policy` and runs only in CI | Tasks 2.5 and 5.1 run it locally | Merges go in on local evidence |


## Stage-3 revision (2026-11-18, implementation stage)

The coordinator's notes of 19:25 and 19:47 arrived after the stage-2 review (its last commit is 19:41), and the ledger had already superseded one stage-1 record. The implementation stage folded them in. **No spec review has seen these rows: stage 4 checks each against the "Quality floor" table.**

| Source | Change | Why |
|---|---|---|
| Note 19:47 (maintainer decision 19:39, level 1) | D7 rewritten: drain first, busy CPUs instead of the 1-minute load, `--lock-timeout 2400`, in-lock gate; `scripts/quiet-gate.py` replaces tasks 1.4/1.5's handoff probe and calibration entry; the spec requirement on the quiet-window criterion is replaced; D3 and the floor list cite the criterion instead of "load < 4"; three floor rows replace two | The maintainer replaced the criterion; the floor table must say it is a loosening |
| Note 19:47 (level 2) | No calibration entry appended; the coordinator's "Calibration: G0 rounds under outside load" is the experiment | One experiment, already queued |
| Note 19:47 (level 3) | Not written into any rule; memorial only | It is conditional on level 2 |
| Notes 19:47 and 19:52 | The `code-review` lens takes its findings from the result text or, when it carries none, from the fork transcript (`subagents/agent-<agentId>.jsonl`, the last `StructuredOutput` input), reports `skill_findings_source`, and a lens with neither source is FAILED (D10). The probes add the same review with and without a caller schema | The return path depends on the caller |
| Note 20:16 | Every stage prompt and `parallel-agent-work.md` "Waiting" say never to return with a command still running; the apply agent commits locally before the long gates and pushes at the end; the script has a stub-agent harness, `programme-change.test.mjs` (dry run, bad arguments, failed lenses: "(none)", a dead agent, a missing status line; a guard violation; a malformed selector; the fast lane; the finished path), which `make check-workflow-scripts` requires beside every script and which fails when a copy of the script with its guard comparison disabled passes it; `scriptPath` loads the main checkout's copy; CI runs are watched by id | An agent that returned with its gates in the background lost them and left its fixes uncommitted; a guard nobody tests can silently stop guarding |
| Note 20:34 (maintainer decision) | Mode `review-only`: an existing pull request and worktree from `/simplify` on, stopping before the merge unless `args.merge`; `args.dry_run` prints the stages of a mode | Two changes wait at "ready, review pass pending" for this template |
| Note 20:50 | The rule gives the relative `scriptPath` form from the main checkout as the guaranteed launch and the name form as a convenience for sessions started after the merge; the script is a real file, not a symlink | A workflow added mid-session was not found by name in invowk |
| Stage 3 decision | `.claude/workflows/` stays out of `sonar.sources`: the scripts are agent guidance, checked by `make check-workflow-scripts` and their harnesses (`build.md`, "SonarQube Cloud Scope", keeps agent guidance out) | Sonar scope review the rule asks for |
| Note 20:38 (lessons from invowk's template review) | Stage agents never force-push and treat a rejected push as fetch, merge, rerun, push; the script refuses main-branch aliases, worktrees outside `.claude/worktrees/` and PR values that are not a number, `#number` or pull URL, and normalizes the PR; the `code-review` lens ends with `CODE-REVIEW-STATUS: ok|failed` (the check) after `skill_findings_source`, with an empty or summary-only body as a backstop failure; `scripts/agent-topology.py validate` checks the lens selector against the `data-safety` and `gtk-perf-review` trigger text in both directions, reusing `performance_path`'s scope; the merge agent reviews the apply commit only and blocks only on a confirmed defect it introduced or one in the guard, main protection or push path; the harness's stubs return distinct values per stage and test precedence | Each was a defect invowk's review found in its own template |
| Ledger 18:36 | Decision 7 recorded as APPROVED with conditions and owned by smoke-gate-coherence, not "open" (D9 (b), proposal, memorial, PR body) | Stage 1's record was stale |

## Stage-4 review (2026-11-18)

An independent review of `origin/main..HEAD` (55e27161): the script, its harness, the helpers, the rules and the evidence. It checked the six gate records against `HEAD^{tree}` (f14a43bf), read each fail-first log, ran five hand mutants of the script through the harness (all caught), ran `scripts/check-fast-lane.py --base origin/main` (not eligible, as a change to the delivery rules must be) and grepped the rules, runbooks and scripts for the retired load criterion (none left). It also checked every row of the stage-3 revision, which no spec review had seen, against the "Quality floor" table. Each row holds. The two loosenings are D7's metric, decided by the maintainer, and the merge stage's stopping rule, recorded below.

Findings, all fixed failing-first. In the handoff directory, `failfirst-stage4-harness.log` shows the eight harness cases failing before the fix (seven new, one extended), then the resumed-fast-lane case. `failfirst-stage4-impl-slots.log` and `failfirst-stage4-quiet-gate.log` hold the helpers' new self-test cases:

| Severity | Finding | Fix |
|---|---|---|
| must-fix (main protection) | `continue` mode took `args.spec.worktree` and `args.spec.branch` without the alias and worktree checks, so a resumed run could aim every later stage at `main` or at the main checkout | One `specProblem` check for every mode's spec, from a stage or carried in the arguments |
| must-fix (main protection) | The worktree pattern accepted `.claude/worktrees/..` and `.claude/worktrees/.`; the first is inside the main checkout, so `git -C` there works on it | `isChangeWorktree` refuses `.` and `..`; `@` joins the main aliases |
| must-fix (outward writes) | A not-viable spec stage was told to make "a docs-only commit to main", a direct push the outward-writes list does not allow ("the archive commit is the only direct push") | It writes `$H/<change>/not-viable.md`, pushes nothing, and the coordinator records it |
| should-fix (correctness) | The fast-lane instructions keyed on `args.mode`, so a fast-lane run its spec stage found not eligible got the full spec review and was then told by the implementation and apply prompts to stop at the first not-eligible path, ending in a false violation | `FAST` is set from the spec stage's `fast_lane: eligible` |
| should-fix (rules) | The revision decision agent appended to `PARALLEL-LEDGER.md`, which "Design revisions" leaves to the coordinator | It writes `$H/<change>/decision-<n>.md` |
| should-fix (correctness) | `continue` with `skip_impl` and no PR in the arguments or the carried report ran every later stage with `PR #null` | The run stops before `/simplify` when there is no PR; an argument `pr: null` counts as absent |
| should-fix (rules) | Coordinator note 21:35 arrived after stage 3 and was in neither the rules nor the script: `git fetch --prune`, merging `origin/<branch>` when GitHub added a commit, and reading a helper's exit status rather than its empty output | "Running a change", "Merging" steps 2-3, "Durability" and every stage prompt. The note's fourth lesson, ordering checks that compare two `indexOf` results, is already met: the harness's order check uses `findIndex` from the previous match, and a missing stage fails it |
| should-fix (review coverage) | `FAST` needed `args.mode === 'fast-lane'`, so a fast-lane change resumed with `continue` or `review-only`, after a guard stop or a failed lens, lost the apply agent's review of its spec, and it had skipped the separate spec review | `FAST` follows the spec's `fast_lane` answer, carried in `args.spec` (continue) or `args.fast_lane` (review-only). Outside the fast-lane mode a stage answer is normalized to `not-applicable`, and the violation stop says to resume with `not-eligible` |
| should-fix (fail open) | `impl-slots.sh` counted only `- Status:` lines, while "Measurements" writes the line as `Status: READY <sha>`, so a dashless ready line left the cap at 5 | A `Status:` line counts with or without the dash. The rule says so |
| nit (fail open) | `quiet-gate.py`'s `busy_between` read counters that had not advanced as 0 busy CPUs, a quiet reading, against its own "a missing reading never admits a round" | It raises `ReadingError` (exit 2) |
| nit | The flags stage ran `cd <worktree> && scripts/…`, against the script's own rule of naming the target | `<worktree>/scripts/agent-topology.py`, whose repository root comes from its own path |

Floor rows for this review's rule edits:

| Rule edit | Existing rule it touches | Why the floor holds |
|---|---|---|
| `git fetch --prune` in "Running a change", "Merging" and "Durability" | "Merging" steps 2-3; "Durability" | Removes stale remote-tracking refs only; every gate, the ancestor check and the merge path are unchanged. |
| Merge `origin/<branch>` before pushing | "Running a change" (rejected push) | Adds a merge and drops nothing. A GitHub-side commit changes the tree, so the gate records no longer match it and the gates rerun. |
| A helper's undocumented exit status means blocked | "Running a change" | Fails closed. |
| Refusals widened (`@`, dot segments, carried specs, no PR) | "Stages after implementation" | Strengthens the main-branch protection. |
| A dashless `Status:` line counts for the slot cap | "Implementation slots and ordering" | Fewer concurrent stages in more cases; no gate changes. |

Decisions on the open questions of the stage-3 report (user experience first):

1. **The stage-3 revision without a spec review.** This review checked each of its rows against the floor table, as that section asked, and found none that lowers the floor. The coordinator's Fable review of the template (task 5.3) reviews it a second time. No further spec-review stage is owed.
2. **The merge stage's stopping rule** (review only the apply commit; "no findings" is not the merge test). It is kept. The lenses already review every commit before the apply commit, and the apply agent is an independent reviewer that wrote none of the code. So the floor item "an independent code reviewer" holds, and the rule only stops an unbounded review-fix loop. It comes from coordinator note 20:38, not from a maintainer decision, so it is listed for the maintainer. **Confirmed by the maintainer on 2026-11-18 with conditions** ("Fable review fix round", rule (a)): the reviewer edits nothing, gets the exact range and tree, and re-checks rejected correctness findings.
3. **An empty `code-review` result counts as a failed lens.** It is kept. A false stop costs one coordinator resume: a `review-only` run on the same pull request reruns the lenses. A false pass merges a change no lens read. Hollow passes have been observed: a forked review's output was lost behind a schema, and bracket-only or summary-only bodies slipped past invowk's filter. In this review's probes, `code-review high` returned findings every time, so the stop should be rare. This reading comes from coordinator notes 19:25 and 20:38, so it is also listed for the maintainer. **Confirmed by the maintainer on 2026-11-18 with a way through** ("Fable review fix round", rule (b)): `CODE-REVIEW-STATUS: empty` and `args.accept_empty_review`.
4. **The probes the coordinator owes** (`host-asks.md`: the cwd under `isolation: 'worktree'`, `code-review` on an open PR, and `code-review` without a caller schema) stay owed. The template is not used before they are recorded, as task 4.4 says.
5. **No merge in this stage** (task 5.3). The verdict is "ready, Fable template review pending".

## Fable review fix round (2026-11-18)

The coordinator ran probes C1-C4 and an independent Fable review of PR #118 at `d2f1b58b` (handoff `fable-review.md`, `probes.md`). Its verdict was "merge after the listed fixes". The maintainer confirmed both open questions above with Fable's conditions (coordinator note 23:20):

- **(a)** The final review covers only the apply range: the reviewer edits nothing, the script passes the exact range and tree (merge commits included), and every rejected lens correctness finding is re-checked.
- **(b)** An empty `code-review` is not a pass: a distinct `CODE-REVIEW-STATUS: empty` with a quoted non-zero scope stops the run, and only `args.accept_empty_review` lets it through.

Each must-fix has a case written first and seen failing against `d2f1b58b`: the template cases on the old harness interface (`failfirst-fix-round-harness.log`) and the helper cases (`failfirst-fix-round-helpers.log`).

| Item | Disposition |
|---|---|
| M1 code-review reviews only `/simplify`'s edits on a pushed branch | Fixed. `high <PR>` for the committed work and `high <worktree>` for the uncommitted edits; prepare sets the upstream, the scope reading requires `HEAD` = PR head = upstream, and each call's quoted scope is checked against the file counts (D10 (a)). |
| M2 `review-only` fails on the waiting branches | Fixed. A prepare stage in every mode (clean, listed worktree; merges `origin/<branch>` and `origin/main`, aborts on conflict; pushes; sets the upstream). The scope agent returns the selector's exit status and raw output, which the script parses; `review-lenses` exits 2 without a `[data_safety]` table. |
| M3 the merge agent edits code and reviews its own fix | Fixed. The final review edits nothing; a confirmed defect or a conflicting `origin/main` merge returns `blocked`, and the coordinator sends the change back through an implementation round. |
| M4 a domain lens that did not run, or a mistranscribed flag, passes | Fixed. `status: ran` and a non-empty `examined` list are required; the flags come from the script's own parse of the helper's JSON, and the apply agent re-runs the selector and blocks on a difference. |
| M5 a dead merge agent returns no `stopped` | Fixed. A dead agent, an unknown verdict, `merged` without merge and archive commits, and `merged` under `no_merge` all stop the run. |
| S1 `revise` needs no PR or worktree | Fixed: both required. |
| S2 `continue` validates late | Fixed: every argument check runs before the first agent; `prev_change` needs `prev_pr` and `prev_tip`; a carried spec gets `change_name` and `branch` defaults. |
| S3 round 2 told to open a PR | Fixed: the prompt uses the PR an earlier round reported. |
| S4 a stage PR beats `args.pr` | Fixed: a mismatch stops the run. |
| S5 apply tree not tied to the gated tree | Fixed: the apply stage returns `head` and `tree`, a read-only agent confirms them, the push and a clean worktree, and the final review checks the tree before "Merging" step 2. |
| S6 brittle guard, expensive stop | Fixed: sha256 comparison, per-stage `args.guard_ack` token, corrected stop message; untracked files stay in (maintainer). |
| S7 `Makefile` is `declare` | Fixed: not eligible. The 25-merge baseline is unchanged (2 of 20): #98 does not touch the `Makefile`. |
| S8 worktree not tied to the change | Fixed: the directory must be named after the change unless `args.worktree_override`; prepare checks `git worktree list`. |
| S9 `/simplify` base in prose | Fixed: literal commands for both diffs, and a clean worktree at start (else `blocked`). |
| S10 Kani re-dispatch not in the apply prompt | Fixed: one sentence, with the stage-3 run ids passed in. |
| S11 only the first cited section checked | Fixed: every title of a citation list, across parentheticals. |
| S12 `gate-trees.sh` records passes only | Fixed: fails are recorded on a clean tree, and lookup refuses a tree with a recorded fail of any named gate (stricter than Fable's "newest line": a pass after a fail on one tree is a flake). |
| S13 `no_merge` keeps the merge paragraph | Fixed: omitted. |
| S14 `review-only` ignores an incomplete carried report | Fixed: stops before any agent. |
| N1 phrase-list backstop | Fixed: `findings_count:`. |
| N2 tabs in `--log-ref`, `--change` | Fixed. |
| N3 guest time double-counted | Fixed: first eight fields. |
| N4 Markdown `Status:` | Fixed. |
| N5 `isMainRef` denylist | Fixed: an allowlist of lowercase names that also refuses hex hashes and `main`/`master`/`head` in any case. |
| N6 duplicated helpers in `agent-topology.py` | Partly fixed: `GIT_PREFIX` added to the fallback. Not deduplicated: the two scripts are standalone CLIs with their own test suites, and importing a hyphenated script by path would couple them for twelve lines. |
| N7 header comment | Fixed. |
| N8 `simplify.patch` omits new files | Fixed: `git add --intent-to-add` before the diff. |
| Note 23:20: a fork runs repository scripts | Fixed: the lens agent runs none itself and releases this change's slot if the review created it; `impl-slots.sh acquire` refuses a name without a change worktree; the guard compares this change's slot and `MERGE-LOCK` ownership. While `/simplify`'s edits keep the worktree dirty, a fork's `gate-trees.sh record` writes nothing; when `/simplify` left none, a line it writes is a real run of that gate on the lens tree (a pass is true evidence, and a fail voids reuse, S12, so the apply read-back stops). |
| Fable note on "Merging" step 2 (narrower than the floor's wording) | Not changed here: step 2 reruns `make check` and the tests covering both sides, which predates this change and is the floor's accepted form ("every gate on every distinct tree" is met by the record-or-rerun rule of D1). Listed for the maintainer. |

A review of the first fix commit (`24dee3a0`) hardened four of these further, in the same round:

- **M1:** the lens prompt no longer carries the counts the lens must report (it could echo them), and the worktree call's quoted base must be `HEAD`, `@{upstream}`, `origin/<branch>` or a prefix of the lens `HEAD`: a matching count over `main` is still the P4 over-scope.
- **M2, S6 again:** the scope reading no longer has an agent copy the selector's JSON and two file lists. The shell prints the counts (`wc -l`), whether `HEAD`, the PR head and the upstream are equal, and the one-line output of the new `review-lenses --flags-only` (test in `scripts/test-agent-topology.py`).
- **S5:** the apply read-back compares in the shell (`rev-parse --verify '<hash>^{commit}'`, `'<hash>^{tree}'`), so an abbreviated hash is not a false stop, refuses a value that is not a hash before it reaches a command, and requires a `gate-trees.sh lookup` pass of `make check` for the tree, so "gated tree" is a record, not a claim.
- **S12:** an interrupted run (exit 75, 78, 124, or a signal) records nothing; it is not a flake. The stage-4 review added 126 and 127 (a command that could not start).
- **S6:** a slot or lock given up during a stage is no difference; only one taken and still held is.
- **Rule (b):** `args.accept_empty_review` is the reviewed `HEAD` the stop prints, so it accepts that one review and not a later one.
- **Recovery:** every stop between `/simplify` and the apply commit says that the worktree holds `/simplify`'s edits, which a `continue` round commits or drops before a new `review-only` run (prepare refuses them).
- **`args.worktree_override`:** `impl-slots.sh` keys slots on the worktree's directory name, so a stage under the override acquires under that name ("Implementation slots and ordering").

Floor rows for this round's rule edits:

| Rule edit | Existing rule it touches | Why the floor holds |
|---|---|---|
| Prepare stage; uncommitted work refused | "Stages after implementation" | Adds a check and a merge that resolves nothing; a conflict stops the run. |
| `code-review high <PR>` plus `high <worktree>`, scope checked | "Stages after implementation" 2 | Strengthens: the bug-hunting lens now provably covers the committed work (C4), and a scope mismatch fails the lens. |
| Empty review stops; `args.accept_empty_review` | "Stages after implementation" 2 | A human look replaces a silent pass; an empty review was a failure before and still never passes on its own. |
| Domain lenses need `status: ran` and `examined` | "Stages after implementation" 2 | Strengthens: a lens that did not run is a failure. |
| The final review edits nothing; exact range and tree; rejected correctness findings re-checked | "Stages after implementation" 4 | Strengthens "an independent code reviewer": no agent reviews its own fix, and conflict resolution goes through the review stages. |
| Guard by status hash, slot and lock; `args.guard_ack` | "Workflow agents" | The comparison covers more (slot, lock); an acknowledgement accepts one named reading and is given only by the coordinator. |
| `Makefile` not eligible for the fast lane | "Fast lane" | Strengthens: fewer changes skip the spec review. |
| A recorded fail voids gate reuse on that tree; an interrupted or unstartable run (exit 75, 78, 124, 126, 127 or a signal) records nothing; only the coordinator removes a fail line, and only one whose log shows the gate never ran | "Merging" step 2 | Strengthens: a flake can no longer hide behind a later pass. An interrupted run was never a result, so leaving it out loosens nothing. |
| Apply read-back in the shell, with a `gate-trees.sh` `make check` record for the tree | "Stages after implementation" 3 | Strengthens "every gate on every tree": the tree the final review gets is the gated one by record, not by the apply agent's word. |
| Slot acquire exits 2 for a name without a change worktree; a Markdown `Status:` line counts; slots under `args.worktree_override` use the worktree's name | "Implementation slots and ordering" | Fewer stray slots, and the cap fails open less often. No gate changes. |
| `revise` and `review-only` need the PR and worktree; every argument is checked before the first agent | "Running a change" | Fails closed earlier: no agent runs on a half-specified change, and no archive push happens before a bad argument is seen. |

This round's stage 4 reviews and fixes as usual but does not merge: it ends "ready, Fable fix check pending", and the coordinator runs a narrow second Fable pass on the fix diff, then a merge-only run with `make test` on the final tree.

### Stage-4 review of the fix round (2026-11-19)

An independent stage-4 agent reviewed `d2f1b58b..12995cef` (code, harness, helpers, rule text and the gate records). The fixes are in one commit after `12995cef`, each with a case written first (`failfirst-stage4-fixround-harness.log`, `failfirst-stage4-fixround-gate-trees.log`):

| Finding | Severity | Disposition |
|---|---|---|
| R1. The final review's prompt said "the only commit you may make is the origin/main merge", yet the same prompt asks for the archive, which edits the programme record and commits to `main`. An agent that obeys the first sentence skips the archive or returns a confused verdict. | should-fix | Fixed: the prompt allows exactly the conflict-free `origin/main` merge and the archive commit, and under `no_merge` no file and no commit. |
| R2. `merged-archive-owed` passed as finished without a `merge_commit`, so a stage that never merged could end the run with a merged verdict (M5's check covered `merged` only). | should-fix | Fixed: both merged verdicts need the merge commit; `merged` also needs the archive commit. |
| R3. The `apply stage: blocked` and fast-lane-violation stops did not say that `/simplify`'s edits may still be uncommitted, and the violation stop sent the coordinator to `continue` with `skip_impl`, whose prepare stage refuses a dirty worktree. | should-fix | Fixed: both stops carry the leftovers wording, the violation stop sends the change to an implementation round, and the spec review after a violation stages only the spec artifacts it changes. |
| R4. `gate-trees.sh` recorded a command that could not start (126, 127) as a fail, which voids the tree for good. | should-fix | Fixed: 126 and 127 are interrupted runs. A fail line otherwise stays; only the coordinator removes one, and only when its log shows the gate never ran (a wrapper's usage refusal, which exits 2 like a `make` failure). Rule "Merging" step 2 and the gate-trees header say so. |
| R5. The rule's "merge-only run" ("Measurements") named no invocation. | should-fix | Fixed in the rule: a resume of the change's run with `args.review_extra` naming the verdict (a `review-only` run that stopped before the merge: with `args.merge`). |
| R6. That resumes with changed arguments reuse the cached stages (`args.guard_ack`, `args.accept_empty_review`, `args.merge`, `args.review_extra`) rests on the Workflow reference ("the longest unchanged prefix of agent() calls returns cached results"), not on a probe. The script keeps those arguments out of every earlier prompt for this reason. | open | Owed probe, listed in `probes.md` beside the lens-tail probe: both block the template's first real use, not this PR. |
| R7. Task 6.1 claimed failing-first evidence for N2 and the slot tolerance, which the helper log did not show (each self-test stops at its first failure). | evidence | Closed: `failfirst-stage4-n2-slot.log` runs both cases against the `d2f1b58b` helpers (accepted, exit 0, slot created) and the fixed ones (exit 2, no slot). |
| R8. The disposition row "a fork runs repository scripts" said a fork's `gate-trees.sh record` writes nothing; that holds only while `/simplify`'s edits keep the worktree dirty. | nit | Fixed in the row above. |
| R9. `--PUSHED--` in the apply read-back reads the PR head from `gh`, which can trail a push by a moment, so a fast reading may stop the run spuriously. | nit | Kept: it fails closed, and a resume reruns only the reading. |
| R10. The worktree path is interpolated unquoted into the shell lines, and only its suffix is checked. | nit | Kept: the path comes from the coordinator's arguments and every change path under the main checkout has no spaces; recorded for the maintainer. |
| R11. `openspec validate --all --strict` fails on 86 main specs (long-requirement warnings), none touched by this change (`git diff origin/main...HEAD -- openspec/specs` is empty). | pre-existing | Not fixed here: a failure known on `main` gets its own change in the ledger ("Merging"), listed for the coordinator. |

Floor rows for these edits:

| Rule edit | Existing rule it touches | Why the floor holds |
|---|---|---|
| Unstartable runs (126, 127) record nothing; only the coordinator removes a fail line whose log shows the gate never ran | "Merging" step 2 | A run that never started was never a gate result; a gate that ran and failed keeps its line, so no flake is hidden, and the removal is the coordinator's and recorded. |
| "Merge-only run" defined as a resume with `args.review_extra` or `args.merge` | "Measurements" | The final review still runs, independently, over the same range and tree, and "Merging" step 2 reruns the gates on any new tree. |

