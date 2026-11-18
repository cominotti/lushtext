## ADDED Requirements

### Requirement: Merge-lock gates reuse evidence only for an identical tree
Under `MERGE-LOCK`, a gate the merge step owes SHALL be skipped only when the change's gate record has a pass for that gate on the post-merge `HEAD^{tree}`, written while the worktree was clean. Every other case SHALL run the gate. Before `gh pr merge`, the agent SHALL re-fetch and require `origin/main` to be an ancestor of `HEAD`.

#### Scenario: A no-op merge reuses the recorded runs
- **WHEN** `git merge origin/main` reports "Already up to date" and `gate-trees.log` has passing `make check` and `make test` lines with `tree=` equal to `HEAD^{tree}`, written on a clean worktree
- **THEN** neither gate is rerun, and the stage names the reused lines

#### Scenario: A merge that changes the tree reruns the gates
- **WHEN** the merge brings in any commit that changes `HEAD^{tree}`, even a docs-only one
- **THEN** `make check` and the tests covering both sides run as before

#### Scenario: A fixed tree without a full record runs the gate
- **WHEN** stage 4 committed a fix and reran only the affected tests, so no full `make test` line names the current tree
- **THEN** the merge step runs `make test`

#### Scenario: A dirty run records nothing
- **WHEN** a gate passes while `git status --porcelain` is not empty
- **THEN** no line is written to `gate-trees.log`

#### Scenario: Main moves before the PR merge
- **WHEN** `origin/main` gains a commit after the merge step and before `gh pr merge`
- **THEN** the ancestor check fails and the agent repeats the merge step instead of merging

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
A non-inferiority change SHALL merge only on a pre-registered non-inferiority verdict through `measure.sh` at load < 4, interleaved. It passes when every touched case is within its band of `main` and no case `main` meets breaks `B_INTERACT_US` or `B_SETTLE_US`. The improvement verdict SHALL go to a named follow-up. No document SHALL claim the improvement before that verdict passes.

#### Scenario: The verdict is within the band
- **WHEN** four interleaved rounds at load < 4 put every touched case's branch median within the pre-registered band of `main`'s
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
A change MAY use the fast lane only when `scripts/check-fast-lane.py` finds no path in shipped app crates, resources, proven modules or gate implementations, and it plans no timing verdict. Rule-text paths SHALL carry a declared "Gate effect: none". One agent writes and self-reviews the spec. The stage-4 reviewer SHALL review the spec, re-run the check and verify the declaration. A violation SHALL require an independent spec review.

#### Scenario: A docs and tooling change
- **WHEN** a change edits only `docs/`, `openspec/` and a release-notes helper outside every excluded path
- **THEN** the checker reports it eligible, and one agent writes and self-reviews the spec

#### Scenario: A shipped file appears late
- **WHEN** the stage-4 diff includes a file under `crates/lushtext-core/src/`
- **THEN** the checker fails and an independent spec review runs before the merge

#### Scenario: Rule text needs a declaration
- **WHEN** a fast-lane change edits `.agents/rules/build.md` to document its new behaviour
- **THEN** the checker reports `declare`, and `proposal.md` holds "Gate effect: none" for that file
- **AND** stage 4 verifies it line by line, and a rule edit that narrows a gate is a violation

#### Scenario: Gate implementations are not fast
- **WHEN** a change edits `scripts/check-visual-proof-policy.py` or `crates/gtk-lush/proof-harness/`
- **THEN** the checker reports it not eligible

### Requirement: Implementation slots shrink while a verdict campaign is ready
At most five implementation stages SHALL run at once, and at most three while `MEASURE-QUEUE.md` holds an entry whose last `Status:` line starts with `READY`. A stage SHALL queue a verdict whose tree is final as `READY <sha>`, and only verdict campaigns SHALL use `READY`. `scripts/impl-slots.sh` SHALL compute the cap at every acquisition attempt.

#### Scenario: A ready verdict lowers the cap
- **WHEN** one queue entry's last status is `READY 5855754a` and three slots are held
- **THEN** `impl-slots.sh acquire` for a fourth change waits in the foreground and exits 124 at its timeout

#### Scenario: A superseded READY does not count
- **WHEN** an entry has `Status: READY` followed by `Status: RAN`
- **THEN** that entry does not lower the cap

#### Scenario: A stage-queued verdict lowers the cap
- **WHEN** a stage-3 agent queues its four interleaved rounds for its final commit
- **THEN** the entry's status is `READY <that sha>`, and the cap is three until the last status changes

#### Scenario: A calibration entry does not count
- **WHEN** the only open entry is `Kind: CALIBRATION` with `Status: PENDING`
- **THEN** the cap is five

### Requirement: The quiet-window criterion changes only after a matching calibration
The quiet-window criterion SHALL stay the global 1-minute load below 4 for 30 s. It SHALL change only when a pre-registered calibration shows that rounds with quiet reference CPUs under global load match quiet-host rounds, and then only by the maintainer's decision. The calibration SHALL pin nothing and change no host state.

#### Scenario: The calibration is queued, not run as a verdict
- **WHEN** this change is implemented
- **THEN** `MEASURE-QUEUE.md` holds a `Kind: CALIBRATION` entry with `Status: PENDING`, its cases, arms, probe and match bar

#### Scenario: Distributions differ
- **WHEN** the off-reference arm's median or p90 for a case is outside the pre-registered tolerance
- **THEN** the criterion stays as it is

### Requirement: Programme workflow scripts are versioned, sanitized and checked
The programme's workflow script SHALL live in `.claude/workflows/`, take host details from `args`, and point to rule sections instead of copying them. `make check-workflow-scripts` SHALL syntax-check each script through a wrapper, check its phases, and fail on personal data, push credentials or copied rule sentences.

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

### Requirement: Implementation is followed by simplify, parallel findings-only lenses and one independent apply agent
After implementation, `/simplify` SHALL run with an absolute worktree path. Findings-only lenses SHALL then run in parallel: `code-review high <target>`, never `--fix`, plus the data-safety, perf-leaf and architecture lenses when `review-lenses` flags data or performance. One agent that wrote no code SHALL verify, apply, run one smoke trio and the gates once.

#### Scenario: A docs-and-tooling change gets one lens
- **WHEN** `review-lenses` reports neither data nor performance for the PR's paths
- **THEN** only the `code-review` lens runs, and the apply agent handles its findings

#### Scenario: A data-path change fans out
- **WHEN** the diff edits `crates/lushtext-core/src/services/draft_service.rs`
- **THEN** the `code-review`, `data-safety` and the three perf-leaf lenses run in parallel, each writing `review-findings-<lens>.md`

#### Scenario: An empty code-review result is a failure
- **WHEN** the `code-review` lens returns "(none)" with no explanation
- **THEN** the script reruns it once and then stops instead of treating it as zero findings

#### Scenario: One trio for both passes
- **WHEN** `/simplify` and the apply agent both edit files under `crates/lushtext-core/src/ui/`
- **THEN** the three smokes run once, before the single commit that holds both sets of edits

### Requirement: Workflow agents keep the main checkout untouched and make only listed outward writes
The workflow script SHALL compare the main checkout's `git status --porcelain`, `HEAD` and branch before and after every stage, and stop on any difference. A workflow agent's outward writes SHALL be limited to the change branch, its twin Kani ref, the PR, the merge under `MERGE-LOCK`, the archive push and `kani.yml` dispatches. Every other external change SHALL go to `host-asks.md`.

#### Scenario: A stage edits the main checkout
- **WHEN** an agent's after-stage status, `HEAD` or branch of the main checkout differs from its before-stage values
- **THEN** the script stops the run and reports the difference

#### Scenario: A Sonar issue looks like a false positive
- **WHEN** a review agent believes a Sonar issue is a false positive
- **THEN** it records the issue in `host-asks.md` and changes no Sonar state
- **AND** `scripts/sonar-local.sh` says the same in its failure message

### Requirement: Process rule changes keep the quality floor
A change to the change-delivery rules SHALL be reviewed at spec review and at stage 4 against the existing rules. It SHALL keep:
- the data-safety review;
- an independent code reviewer;
- every gate on every tree reaching `main`;
- pre-registered verdicts at load < 4 through `measure.sh`;
- `B_INTERACT_US` and `B_SETTLE_US`;
- flakes as blockers;
- every lane, shard, scenario and assertion.

#### Scenario: The review finds a loosening
- **WHEN** a rule edit would let a tree that no gate ran on reach `main`
- **THEN** the reviewer records a must-fix and the change does not merge

#### Scenario: The floor table is present
- **WHEN** a change edits `parallel-agent-work.md` or `verification-runs.md`
- **THEN** its design has a table naming each edit, the existing rule it touches and why the floor holds
