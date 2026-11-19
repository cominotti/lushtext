## ADDED Requirements

### Requirement: Merge-lock gates reuse evidence only for an identical tree
Under `MERGE-LOCK`, a gate SHALL be skipped only when `scripts/gate-trees.sh` finds a pass for it on the post-merge `HEAD^{tree}`, recorded on a clean worktree. An `origin/main` merge that made a commit SHALL rerun every gate on the new tree, the smokes included when either side touches a smoke-relevant file. Before `gh pr merge`, the agent SHALL re-fetch and require `origin/main` to be an ancestor of `HEAD`.

#### Scenario: A no-op merge reuses the recorded runs
- **WHEN** `git merge origin/main` reports "Already up to date" and `gate-trees.log` has passing `make check` and `make test` lines with `tree=` equal to `HEAD^{tree}`, written on a clean worktree
- **THEN** neither gate is rerun, and the stage names the reused lines

#### Scenario: A merge that changes the tree reruns every gate
- **WHEN** the merge brings in any commit that changes `HEAD^{tree}`, even a docs-only one
- **THEN** `make check` and the full `make test` run on the post-merge tree, and so do the three smokes when the branch or the merged-in commits touch a smoke-relevant file, each recorded through `gate-trees.sh record`

#### Scenario: A fixed tree without a full record runs the gate
- **WHEN** stage 4 committed a fix and reran only the affected tests, so no full `make test` line names the current tree
- **THEN** the merge step runs `make test`

#### Scenario: A dirty run records nothing
- **WHEN** `gate-trees.sh record` runs a gate that passes while `git status --porcelain` is not empty before or after it
- **THEN** no line is written to `gate-trees.log`, and `gate-trees.sh lookup` for that tree exits 1

#### Scenario: A gate that passed and failed on one tree is not reused
- **WHEN** `gate-trees.log` holds a pass and a fail of `make test` for the current `HEAD^{tree}`, both recorded on a clean worktree
- **THEN** `gate-trees.sh lookup` for that tree exits 1, naming the fail, and the merge step runs the gate

#### Scenario: A gate that never ran is not a result
- **WHEN** the recorded command exits 75, 78, 126 or 127 (busy, off its CPUs, not executable, not found)
- **THEN** `gate-trees.sh record` writes no line and returns that status

#### Scenario: A hang or a kill leaves a trace
- **WHEN** the recorded command times out (124) or is killed by a signal (128 and above) on a clean, unchanged tree
- **THEN** `gate-trees.sh record` appends a `result=interrupted` line and returns that status, and a later `lookup` for that tree neither reuses nor refuses on it but names it on stderr

#### Scenario: Main moves before the PR merge
- **WHEN** `origin/main` gains a commit after the merge step and before `gh pr merge`
- **THEN** the ancestor check fails and the agent repeats the merge step instead of merging

### Requirement: The apply tree's records are checked before the merge stage
Before the merge stage starts, the script SHALL look up `make check` and `make test` on the apply tree, each in its own lookup. It SHALL also look up the three smokes when a merge in the branch brought a smoke-relevant file (`is_relevant` of `scripts/accessibility_source_fingerprint.py`) in and no later non-merge commit of the branch's own line carried a file under the UI prefixes, since merge commits skip the pre-commit hook's smoke checks. A no-op merge SHALL rerun only a gate without a pass.

#### Scenario: An apply tree without a make test record stops before the merge stage
- **WHEN** the apply stage's tree has a `make check` pass on record and no `make test` pass, and `origin/main` has not moved
- **THEN** the script's apply verification stops the run before the merge stage starts, and no merge prompt tells the agent to rerun nothing without a lookup

#### Scenario: A merge that brought smoke-relevant files in owes smoke records
- **WHEN** a merge in the branch brought a smoke-relevant file in, and no later non-merge commit of the branch's own line carried a file under the UI prefixes (a later commit of `docs/accessibility.md` alone does not count: the hook then demands no visual-geometry proof; nor does a `main` commit a later merge brought in: its hook ran on `main`'s tree)
- **THEN** the apply stage records the three smokes on its committed tree, the script stops before the merge stage when a record is missing or the check fails, and after a merge that made no commit the post-merge check owes the same three records

### Requirement: Gate records are append-only and checked after the merge
`gate-trees.log` SHALL be append-only. A fail SHALL be set aside only by a void line the coordinator appends, naming that one fail line and the log that shows the gate never ran. After a merged verdict, the tree that reached `main` SHALL be checked against the records.

#### Scenario: The coordinator voids a fail that never ran
- **WHEN** a `make test` fail line's log shows a wrapper's usage refusal, and the coordinator runs `gate-trees.sh void` naming that line's timestamp and gate, that log and a reason
- **THEN** a `result=void` line is appended, the fail line stays in the log, and `lookup` ignores that fail line only, so a later fail of the same gate on that tree still refuses the reuse

#### Scenario: A void that does not name exactly one fail line is refused
- **WHEN** `gate-trees.sh void` names a timestamp and gate that match no fail line, two fail lines, a pass line or a fail already voided, or gives a missing or empty log or no reason
- **THEN** it exits 2 and writes no line

#### Scenario: The tree on main is checked against the records
- **WHEN** the merge stage returns a merged verdict
- **THEN** it also returns the merged tree, and a read-only agent checks that GitHub's merge commit is on `origin/main` with that tree and that `gate-trees.sh lookup --tree` finds passes of `make check`, `make test` and the three smokes, after a merge that brought commits in when the branch or those commits touch a smoke-relevant file, or after a merge that made no commit when the apply tree owed them as records; a missing helper, a failed prefix check or a missing record stops the run with a report that the change is already on `main`

### Requirement: Kani budget runs are dispatched when implementation ends
A change that adds or changes a Kani shard SHALL dispatch its budget runs once its final stage-3 code is pushed, without watching them. Stage 4 SHALL read finished runs: a failure or a margin break blocks the merge, and an unfinished run does not. The figures SHALL be recorded before the archive. No agent SHALL wait for a run while holding `MERGE-LOCK`.

#### Scenario: Stage 3 dispatches and moves on
- **WHEN** stage 3 pushes its final code for a change that adds a scheduled shard
- **THEN** it dispatches `kani.yml` once on the branch and once on a twin ref at the same commit, so the two runs are in different concurrency groups and neither cancels the other
- **AND** it records both run ids in `kani-dispatch.md` and the PR body, and ends without watching them

#### Scenario: A pull-request shard counts the PR's own run
- **WHEN** the new shard is gated `pull-request`
- **THEN** one dispatch is made and the PR's Kani run is the second measurement

#### Scenario: Figures land only in the archive commit
- **WHEN** both runs finished green within margins before stage 4 takes `MERGE-LOCK`
- **THEN** the branch tree is left unchanged, so the gate records stay valid
- **AND** the archive commit records the figures, touching only the pending records' `measured_in` and figure fields, and its hook runs the full `make check`

#### Scenario: A red run blocks the merge
- **WHEN** a dispatched run finished with a failed harness or a figure over a margin
- **THEN** the change does not merge until the failure is fixed in its work stream

#### Scenario: Unfinished runs defer only the archive
- **WHEN** a run is still in progress when stage 4 is ready to merge
- **THEN** the change merges on local evidence, releases `MERGE-LOCK` with an `ARCHIVE OWED` ledger line, and waits for the run outside every lock
- **AND** the archive commit, under a new `MERGE-LOCK` hold, records the figures, and `make check-kani-shards` refuses it while the figure is still pending

#### Scenario: Later edits to harness inputs re-dispatch
- **WHEN** a stage-4 fix or the `origin/main` merge touches the harness inputs of a shard whose figure is pending
- **THEN** the runs are dispatched again on the final tree, and the stage-3 runs no longer count

### Requirement: A change declares its verdict class before measuring
A change on a timed path SHALL state in design, before any verdict round, either `non-inferiority` or `improvement-gated`, with a reason. The spec review SHALL confirm it. After the first verdict round, only the maintainer SHALL change it. A change whose purpose is the improvement, or whose metrics trade off, SHALL be improvement-gated.

#### Scenario: A correctness fix on a timed path
- **WHEN** a change fixes a data-safety defect in the save path and does not claim a speed-up
- **THEN** its design states `Verdict class: non-inferiority` with that reason

#### Scenario: A trade-off mechanism is improvement-gated
- **WHEN** a change trades completion time against worst frame, as `slice-large-directory-collapse` does
- **THEN** its class is `improvement-gated` and its verdict runs before the merge

#### Scenario: A failed verdict cannot relabel the class
- **WHEN** an improvement-gated change misses its bar and an agent proposes changing the class
- **THEN** the class stays until the maintainer decides

### Requirement: Non-inferiority merges keep a pre-registered verdict
A non-inferiority change SHALL merge only on a pre-registered non-inferiority verdict through `measure.sh`, interleaved and judged quiet by the timing-verdict criterion. It passes when every touched case is within its band of `main` and no case `main` meets breaks `B_INTERACT_US` or `B_SETTLE_US`. The improvement verdict SHALL go to a named follow-up. No document SHALL claim the improvement before that verdict passes.

#### Scenario: The verdict is within the band
- **WHEN** four interleaved kept rounds put every touched case's branch median within the pre-registered band of `main`'s
- **THEN** the change may merge, and its improvement verdict is queued for `<change>-improvement-verdict`

#### Scenario: A budget is broken
- **WHEN** a case `main` meets within `B_SETTLE_US` goes over it on the branch, even inside the band
- **THEN** the verdict fails and the change does not merge

#### Scenario: The non-inferiority verdict is still queued
- **WHEN** the non-inferiority rounds are queued in `MEASURE-QUEUE.md`
- **THEN** stage 4 returns "ready, measurement pending" without merging

#### Scenario: No early claim
- **WHEN** the change merges with its improvement verdict pending
- **THEN** README, `AGENTS.md`, changelogs, programme records and the PR body make no improvement claim, and its requirements do not assert the improvement bar

### Requirement: Design loops are bounded by an independent decision
When a change that has had two design revisions is blocked again, an agent that wrote none of its design or code SHALL decide continue, re-scope or shelve, and the maintainer SHALL be told. A third revision SHALL start with an attribution spike by one agent that edits no OpenSpec artifact.

#### Scenario: The third block
- **WHEN** a change whose `design.md` lists two revisions is blocked at stage 4
- **THEN** an independent agent returns continue, re-scope or shelve with reasons, and the ledger records it for the maintainer

#### Scenario: Continue requires a spike
- **WHEN** the decision is continue
- **THEN** `spike-<n>.md` attributes the failure with figures before the third revision is written, and the revision cites it

#### Scenario: Shelve holds nothing
- **WHEN** the decision is shelve
- **THEN** the change is marked `SHELVED` in the ledger and the programme record, and holds no slot or lock

### Requirement: The fast lane is limited to changes outside shipped code, proofs and gates
A change MAY use the fast lane only when `scripts/check-fast-lane.py` classes every path `eligible` or `declare` from an allowlist, and it plans no timing verdict. Any path not on the allowlist SHALL make it not eligible. `declare` paths SHALL carry a "Gate effect: none" section. One agent writes and self-reviews the spec. The stage-4 reviewer SHALL review the spec, re-run the check and verify the declaration. A violation SHALL require an independent spec review.

#### Scenario: A documentation change
- **WHEN** a change edits only `docs/memorials/` and `openspec/` paths that no gate names
- **THEN** the checker reports it eligible, and one agent writes and self-reviews the spec

#### Scenario: An unlisted path is not eligible
- **WHEN** a change adds a new top-level directory, or edits `crates/gtk-lush/settle/src/`, `crates/lushtext/tests/widget/`, `Cargo.toml`, `.gitignore` or `.agents/rules/parallel-agent-work.md`
- **THEN** the checker reports it not eligible

#### Scenario: A shipped file appears late
- **WHEN** the stage-4 diff includes a file under `crates/lushtext-core/src/`
- **THEN** the checker fails and an independent spec review runs before the merge

#### Scenario: Rule text needs a declaration
- **WHEN** a fast-lane change edits `.agents/rules/build.md` to document its new behaviour
- **THEN** the checker reports `declare`, and `proposal.md` holds "Gate effect: none" for that file
- **AND** stage 4 verifies it line by line, and a rule edit that narrows a gate is a violation

#### Scenario: A fast-lane run found not eligible takes the normal stages
- **WHEN** a run in the fast-lane mode gets `not-eligible` from its spec stage
- **THEN** it takes the full spec review, and the implementation and apply prompts carry no fast-lane instruction

#### Scenario: A resumed fast-lane change keeps its spec review at apply
- **WHEN** a fast-lane change whose spec stage found it eligible resumes in `continue` or `review-only` mode
- **THEN** the apply agent still re-runs the check and reviews the spec artifacts

#### Scenario: Gate implementations are not fast
- **WHEN** a change edits `scripts/flatpak-source-tree.sh`, `scripts/agent-topology.py`, `.agents/skills/skill-policy.toml`, the `Makefile` or `crates/gtk-lush/proof-harness/`
- **THEN** the checker reports it not eligible

### Requirement: Implementation slots shrink while ready rounds are queued
At most five implementation stages SHALL run at once, and at most three while `MEASURE-QUEUE.md` holds a unit (an entry, or each `###` part of one) whose last `Status:` line starts with `READY`. A stage SHALL queue runnable rounds a change waits on, on a final tree, as `READY <sha>`; non-blocking attribution and calibration SHALL use `PENDING`. `scripts/impl-slots.sh` SHALL compute the cap at every acquisition attempt.

#### Scenario: A ready verdict lowers the cap
- **WHEN** one queue entry's last status is `READY 5855754a` and three slots are held
- **THEN** `impl-slots.sh acquire` for a fourth change waits in the foreground and exits 124 at its timeout

#### Scenario: A superseded READY does not count
- **WHEN** an entry has `Status: READY` followed by `Status: RAN`
- **THEN** that entry does not lower the cap

#### Scenario: A stage-queued verdict lowers the cap
- **WHEN** a stage-3 agent queues its four interleaved rounds for its final commit
- **THEN** the entry's status is `READY <that sha>`, and the cap is three until the last status changes

#### Scenario: A ready part of a multi-part entry counts
- **WHEN** an entry's part B ends with `Status: READY <sha>` and its later part C ends with `Status: DONE`
- **THEN** the cap is three

#### Scenario: A calibration entry does not count
- **WHEN** the only open entry is `Kind: CALIBRATION` with `Status: PENDING`
- **THEN** the cap is five

### Requirement: Timing verdicts run after a drain and are judged on busy CPUs
A timing verdict round SHALL first wait, holding no lock, for outside CPU use below 3.5 CPUs over 30 s, then run through `measure.sh --lock-timeout 2400` inside `scripts/quiet-gate.py run`. It SHALL run only after an in-lock 30 s sample below 4 busy CPUs, and be kept only when it exits 0 and the run and a 30 s post sample are both below 4 busy CPUs. The 1-minute load SHALL NOT be the criterion, and no rule SHALL count a round measured under load as a pass.

#### Scenario: Outside load holds the round back
- **WHEN** host CPU use outside `user-lushtext.slice` and the `cst-sccache-*` scopes stays at 3.5 CPUs or more for the whole wait
- **THEN** `quiet-gate.py wait-outsiders` exits 124 holding no lock, and the remaining rounds are queued in `MEASURE-QUEUE.md`

#### Scenario: A busy in-lock sample skips the round
- **WHEN** the 30 s sample taken inside the exclusive lock shows 4 or more busy CPUs
- **THEN** `quiet-gate.py run` exits 76 without running the round, and the round is retried or queued, never kept

#### Scenario: A busy post sample voids the round
- **WHEN** the round exits 0 but the 30 s sample after it shows 4 or more busy CPUs
- **THEN** `quiet-gate.py run` exits 77, the log line says the round was not kept, and it is not a verdict

#### Scenario: Unreadable counters fail closed
- **WHEN** `/proc/stat` or a slice's `cpu.stat` cannot be read
- **THEN** the helper exits 2 and prints no reading, so no round is admitted on a missing measurement

#### Scenario: A pass under load is not a pass
- **WHEN** a round exits 0 within its budget but the busy CPUs over the run or the 30 s after it are 4 or more
- **THEN** `quiet-gate.py run` exits 77, the round is not kept, and no rule counts it as a pass for an absolute budget or any other verdict

### Requirement: Programme workflow scripts are versioned, sanitized and checked
The programme's workflow script SHALL live in `.claude/workflows/`, take host details from `args`, and point to rule sections instead of copying them. `make check-workflow-scripts` SHALL syntax-check each script through a wrapper, check its phases, fail on personal data, push credentials or copied rule sentences, and run the stub-agent harness `<name>.test.mjs` beside each script, failing when it is missing or fails.

#### Scenario: A personal path is refused
- **WHEN** a workflow script contains `/var/home/` or an SSH key name
- **THEN** `make check-workflow-scripts` fails naming the line

#### Scenario: A copied rule sentence is refused
- **WHEN** a prompt repeats a 12-word sentence of `.agents/rules/parallel-agent-work.md`
- **THEN** the check fails and names the rule file

#### Scenario: Top-level await parses
- **WHEN** a script uses top-level `await` and `return` after `export const meta`
- **THEN** the wrapper makes them parse and `node --check` passes

#### Scenario: Node is missing
- **WHEN** `node` is not on `PATH`
- **THEN** the check fails with a message naming node instead of passing

#### Scenario: The harness catches a disabled guard
- **WHEN** the harness runs a copy of the script whose guard comparison is disabled
- **THEN** its guard-violation case fails, and the harness reports the mutant as not caught if it does not

#### Scenario: A script without a harness is refused
- **WHEN** `.claude/workflows/<name>.js` has no `<name>.test.mjs` beside it
- **THEN** `make check-workflow-scripts` fails naming the missing harness


### Requirement: The programme workflow can start a change at its review stage
The workflow script SHALL start a change at its prepare stage on an existing pull request and worktree (`mode: 'review-only'`) and stop before the merge stage unless the caller asks it to merge. Every mode SHALL run the prepare stage before `/simplify`.

#### Scenario: A review-only run starts at prepare
- **WHEN** the script runs with `mode: 'review-only'`, an existing pull request and its worktree
- **THEN** no spec or implementation agent runs, the first stage is the prepare stage, then `/simplify`, and the run stops before the merge stage unless `merge` is set

#### Scenario: Prepare refuses uncommitted work and conflicts
- **WHEN** the change worktree has uncommitted or untracked entries, is not in the main checkout's `git worktree list`, or merging `origin/main` conflicts
- **THEN** the prepare agent aborts any merge, edits nothing and returns blocked, and no review stage runs

#### Scenario: Arguments are checked before the first agent
- **WHEN** a `revise` run has no `pr` or `worktree`, a `continue` run carries a spec outside a change worktree or skips implementation with no pull request, an archive of a previous change lacks its PR or tip, a carried implementation is not complete, `args.lens_round` is not a positive integer, or a stage or carried PR differs from `args.pr`
- **THEN** the script stops before any agent runs, or, for a stage's PR, before the next stage

### Requirement: Implementation is followed by simplify, parallel findings-only lenses and one independent apply agent
After implementation, `/simplify` SHALL run on a clean worktree with an absolute worktree path. Findings-only lenses SHALL then run in parallel: `code-review high`, never `--fix`, over the PR's commits and `/simplify`'s edits, plus one domain lens per `review-lenses` flag. One agent that wrote no code SHALL apply the findings, run one smoke trio and the gates once, and the final review SHALL edit nothing.

#### Scenario: A docs-and-tooling change gets one lens
- **WHEN** `review-lenses` reports neither data nor performance for the PR's paths
- **THEN** only the `code-review` lens runs, and the apply agent handles its findings

#### Scenario: A data-path change fans out
- **WHEN** the diff edits `crates/lushtext-core/src/services/draft_service.rs`
- **THEN** the `code-review`, `data-safety` and the three perf-leaf lenses run in parallel, each writing `review-findings-<lens>.md`

#### Scenario: A performance-only change gets the perf lenses only
- **WHEN** the diff edits only `crates/gtk-lush/settle/src/` and `review-lenses` reports performance but not data
- **THEN** the `code-review` and the three perf-leaf lenses run, and the `data-safety` lens does not

#### Scenario: The code-review lens reviews the committed work through the PR
- **WHEN** the branch is pushed and tracks `origin/<branch>`, so a worktree-path target would diff against that upstream and see no commits
- **THEN** the lens runs `high <PR>` for the committed work, after the scope reading confirmed that `HEAD`, the PR head and the upstream are equal

#### Scenario: The code-review lens sees the simplify edits
- **WHEN** `/simplify` left uncommitted edits
- **THEN** the lens also runs `high <absolute worktree path>`, which on that branch reviews only the uncommitted edits, and both outputs form the one lens

#### Scenario: A review whose scope does not match fails the lens
- **WHEN** a call's `CODE-REVIEW-SCOPE` line names another PR, a file count other than the committed files or the uncommitted ones of the scope reading, or, for the worktree call, a base other than `HEAD` or `origin/<branch>`
- **THEN** the script treats the lens as failed, reruns it once, and then stops

#### Scenario: A failing selector stops the run
- **WHEN** `scripts/agent-topology.py review-lenses` exits non-zero (an old helper, or a skill policy without its `[data_safety]` table) or prints output that is not its JSON
- **THEN** the script stops before any lens runs, instead of running `code-review` alone

#### Scenario: The code-review lens reads the fork's findings
- **WHEN** the Skill call returns only "Skill execution completed" and an `agentId`
- **THEN** the lens reads the findings from that fork's transcript and prints `skill_findings_source: fork-transcript` with them, and a lens output with no `skill_findings_source` line fails the lens

#### Scenario: An empty code-review result is a failure
- **WHEN** the `code-review` lens returns "(none)" with no explanation
- **THEN** the script reruns it once and then stops instead of treating it as zero findings

#### Scenario: An empty review over a quoted scope needs the coordinator
- **WHEN** the `code-review` lens ends with `findings_count: pr=0` and a non-zero scope for every call
- **THEN** the script stops with its own message printing a token of the reviewed `HEAD` and a digest of the review's text, and continues past the lens only when the run is resumed with `args.accept_empty_review` set to that token; the `HEAD` alone, or the token of another review of it, accepts nothing

#### Scenario: Findings on the simplify edits do not hide a hollow PR review
- **WHEN** the lens reports `findings_count: pr=0 worktree=2` and `CODE-REVIEW-STATUS: ok`
- **THEN** the script treats it as an empty review and stops for the coordinator, without spending the rerun

#### Scenario: A failed lens is rerun alone on a resume
- **WHEN** the coordinator resumes a run whose lens failed twice with `args.lens_round` one higher, the worktree unchanged
- **THEN** the scope reading and every lens run again, and prepare and `/simplify` replay from the first run

#### Scenario: A domain lens that did not run fails
- **WHEN** a domain lens returns without `status: ran`, or with an empty `examined` list
- **THEN** the script treats it as a failed lens and stops before the apply stage

#### Scenario: A lens marked ok over an empty body fails
- **WHEN** the `code-review` lens ends with `CODE-REVIEW-STATUS: ok` but its findings are empty, only brackets, or only a "No findings" summary
- **THEN** the script treats the lens as failed, reruns it once, and then stops

#### Scenario: The selector and the skill triggers drift apart
- **WHEN** the `data-safety` description names a path or keyword the `[data_safety]` table does not select, or the table selects one the description does not name, or the `gtk-perf-review` areas and the performance scope differ
- **THEN** `scripts/agent-topology.py validate` fails naming the item and the direction

#### Scenario: The merge test is not zero findings
- **WHEN** the final review of the apply range finds only unconfirmed or pre-existing issues
- **THEN** it merges, and it blocks only on a confirmed defect the apply range introduced, a wrongly rejected correctness finding, or one in the guard, the main-branch protection or the push path

#### Scenario: The final review edits nothing
- **WHEN** the final review confirms a defect, or the `origin/main` merge under `MERGE-LOCK` conflicts
- **THEN** it aborts the merge, releases the lock, returns `blocked` with the reason and writes no commit, and the script stops

#### Scenario: The final review covers exactly the gated apply range
- **WHEN** the apply stage returns its `HEAD` and gated tree
- **THEN** a read-only agent confirms them in the shell, with the push, the clean worktree and a `make check` record of `gate-trees.sh` for that tree, the script passes the range from the lens `HEAD` to that `HEAD` and the tree to the final review, which checks `HEAD^{tree}` before "Merging" step 2 and re-checks every rejected correctness finding

#### Scenario: A dead merge agent is not a finished run
- **WHEN** the merge agent returns nothing, a verdict outside its fixed set, `merged` without a merge commit, a final tree and an archive commit, `merged-archive-owed` without a merge commit and a final tree, or `merged` while `no_merge` was set
- **THEN** the script stops with a message to check `MERGE-LOCK`, the PR and the archive by hand

#### Scenario: A block after the merge says the change is on main
- **WHEN** the merge stage returns `blocked` with a merge commit (the archive failed after `gh pr merge`)
- **THEN** the stop names that merge commit and does not say that nothing was merged

#### Scenario: The archive commit carries what the archive owes
- **WHEN** the merge stage archives a merged change
- **THEN** its prompt allows the archive commit of "Merging" step 4, the archive skill's spec sync and the Kani figures included, and no other commit besides the conflict-free `origin/main` merge

#### Scenario: One trio for both passes
- **WHEN** `/simplify` and the apply agent both edit files under `crates/lushtext-core/src/ui/`
- **THEN** the three smokes run once, before the single commit that holds both sets of edits

### Requirement: Workflow agents keep the main checkout untouched and make only listed outward writes
Guard agents SHALL read the main checkout (status hash, `HEAD`, branch) and this change's slot and `MERGE-LOCK` ownership before and after every stage, and the script SHALL stop on any difference that `args.guard_ack` does not acknowledge. Outward writes SHALL be limited to the change branch and its twin Kani ref, the PR, the merge under `MERGE-LOCK`, the archive push and `kani.yml` dispatches; anything else goes to `host-asks.md`.

#### Scenario: A stage edits the main checkout
- **WHEN** the guard agent's after-stage status, `HEAD` or branch of the main checkout differs from its before-stage reading
- **THEN** the script stops the run and reports the difference

#### Scenario: A resumed stage gets a live guard reading
- **WHEN** a resume changes only the merge stage's prompt, through `args.review_extra`
- **THEN** the guard reading before the merge stage is the first call that runs live, because it carries a digest of that stage's prompt, and every earlier call replays

#### Scenario: A guard stop is acknowledged on resume
- **WHEN** the coordinator resumes a stopped run with `args.guard_ack` holding the token the stop printed
- **THEN** that stage's reading passes, while a token for another stage or another reading still stops the run, and neither the token nor `args.accept_empty_review` appears in any prompt

#### Scenario: A review fork takes a slot
- **WHEN** this change holds its implementation slot or `MERGE-LOCK` after the lens group and did not before
- **THEN** the script stops the run, while giving one up during a stage is no difference, and `scripts/impl-slots.sh acquire` gives no slot to a name without a change worktree

#### Scenario: A worktree under the override names the slot
- **WHEN** a run takes `args.worktree_override` for a worktree whose directory is not the change name
- **THEN** the guard readings and the lens prompt read and release `IMPL-SLOTS/<that directory name>`, the name `impl-slots.sh acquire` used

#### Scenario: A main-branch alias is refused
- **WHEN** the change, the branch argument, the spec stage's branch or a spec carried into a resumed run names `main`, `HEAD` or `@`, a branch that starts with `heads/`, `refs/`, `origin/`, `remotes/` or `tags/`, a revision expression such as `main~0` or `main^{}`, a commit hash, or anything else that is not a lowercase name
- **THEN** the script stops before the next agent runs

#### Scenario: A worktree outside the change worktrees is refused
- **WHEN** a worktree argument, the spec stage's worktree or a spec carried into a resumed run is not one directory under `.claude/worktrees/`, including `.claude/worktrees/..` and `.claude/worktrees/.`, holds a character other than letters, digits, `.`, `_`, `-` and `/` or a `.` or `..` segment anywhere in its absolute path, or its directory is not named after the change and `args.worktree_override` is not set
- **THEN** the script stops before the next agent runs

#### Scenario: Only the listed outward writes appear in stage prompts
- **WHEN** the spec stage finds a change not viable, or the revision decision agent decides
- **THEN** it writes its record to the change's handoff directory, pushes nothing, and the coordinator records it in the ledger or the programme record

#### Scenario: A PR value is normalized
- **WHEN** an argument or a stage returns the PR as `7`, `#7` or `https://github.com/<owner>/<repo>/pull/7`
- **THEN** every later prompt names `PR #7`, and any other value stops the run

#### Scenario: A schema-less lens is still guarded
- **WHEN** the `code-review` lens runs without a schema
- **THEN** guard agents read the main checkout before and after the lens group, and the script compares their readings

#### Scenario: A stage agent backgrounds its gates
- **WHEN** a stage agent's gates are still running
- **THEN** it keeps waiting for them in the foreground and does not return, and the apply agent has committed its fixes locally before the long gates started

#### Scenario: A Sonar issue looks like a false positive
- **WHEN** a review agent believes a Sonar issue is a false positive
- **THEN** it records the issue in `host-asks.md` and changes no Sonar state
- **AND** `scripts/sonar-local.sh` says the same in its failure message

### Requirement: Process rule changes keep the quality floor
A change to the change-delivery rules SHALL be reviewed at spec review and at stage 4 against the existing rules. It SHALL keep:
- the data-safety review;
- an independent code reviewer;
- every gate on every tree reaching `main`;
- pre-registered verdicts through `measure.sh`, judged quiet by the timing-verdict criterion;
- `B_INTERACT_US` and `B_SETTLE_US`;
- flakes as blockers;
- every lane, shard, scenario and assertion.

#### Scenario: The review finds a loosening
- **WHEN** a rule edit would let a tree that no gate ran on reach `main`
- **THEN** the reviewer records a must-fix and the change does not merge

#### Scenario: The floor table is present
- **WHEN** a change edits `parallel-agent-work.md` or `verification-runs.md`
- **THEN** its design has a table naming each edit, the existing rule it touches and why the floor holds
