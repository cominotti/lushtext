## MODIFIED Requirements

### Requirement: The fast lane is limited to changes outside shipped code, proofs and gates
A change MAY use the fast lane only when `scripts/check-fast-lane.py` classes every path `eligible` or `declare`, plans no timing verdict, and keeps `declare` paths annotated with "Gate effect: none". Any unlisted path SHALL make it ineligible, and stage 4 SHALL require an independent spec review on violation.

#### Scenario: A documentation change
- **WHEN** a change edits only `docs/memorials/` and `openspec/` paths that no gate names
- **THEN** the checker reports it eligible, and one agent writes and self-reviews the spec

#### Scenario: An unlisted path is not eligible
- **WHEN** a change adds a new top-level directory, or edits `crates/gtk-lush/settle/src/`, `crates/lushtext/tests/widget/`, `Cargo.toml`, `.gitignore` or `.agents/rules/parallel-agent-work.md`
- **THEN** the checker reports it not eligible

#### Scenario: A shipped file appears late
- **WHEN** the stage-4 diff includes a file under `crates/lushtext-ui/src/`, `crates/lushtext-services/src/`, or `crates/lushtext-model/src/`
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

### Requirement: Implementation is followed by simplify, parallel findings-only lenses and one independent apply agent
After implementation, `/simplify` SHALL run on a clean absolute worktree path, followed by parallel findings-only lenses (`code-review high` over PR commits and `/simplify` edits, plus domain lenses per `review-lenses`). One independent apply agent runs one smoke trio and gates once; the final review edits nothing.

#### Scenario: A docs-and-tooling change gets one lens
- **WHEN** `review-lenses` reports neither data nor performance for the PR's paths
- **THEN** only the `code-review` lens runs, and the apply agent handles its findings

#### Scenario: A data-path change fans out
- **WHEN** the diff edits `crates/lushtext-services/src/draft_service.rs` (or `crates/lushtext-services/src/persistence/draft_service.rs`)
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
- **THEN** the script treats the lens as failed and stops before the apply stage

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
- **WHEN** `/simplify` and the apply agent both edit files under `crates/lushtext-ui/src/ui/`
- **THEN** the three smokes run once, before the single commit that holds both sets of edits
