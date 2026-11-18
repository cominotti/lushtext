## Why

On 2026-11-18 the maintainer accepted the recommendations of the change-flow analysis: interventions 1-5 and decisions 1, 2, 3, 4 and 6. Decision 5 was rejected. Decision 7 was approved later that day with conditions, and belongs to the smoke-gate-coherence change. At 19:39 the maintainer also replaced the quiet-window criterion (levels 1-3 below). The maintainer asked that they become permanent for all new work, in future sessions too, and that the adjustments themselves be reviewed. The accepted process decisions so far live only in the coordinator's session: three workflow scripts under `~/.claude/projects/.../workflows/scripts/`, ledger lines in the gitignored `PARALLEL-LEDGER.md`, and the user's auto-memory. A new session would not find them. The scripts also repeat rule text in a `RULES` string, and that copy has already drifted from the rules: the rules say nothing about `COORDINATOR-NOTE.txt`, and the scripts still carry a five-slot cap that decision 4 changed.

Measured baseline, from the analysis (233 agent transcripts, 25 `runs.log` files, 83 merged PRs, 2026-11-05 to 2026-11-18; figures and method in the memorial this change adds):

- A median programme change costs 5.2 agent-hours (24 changes). Before and after 2026-11-14, the median moved from 5.2 h to 5.5 h. After that date it splits into 4.2 h of tool time and 1.5 h of model time. The mean moved from 5.5 h to 8.4 h, driven by four changes above 10 h, which were design loops and measurement queues.
- Under `MERGE-LOCK`, `make check` and the affected tests always rerun, even when `git merge origin/main` changes nothing. In `stream-replace-all` that step took about 28 min.
- `stream-replace-all` (#103) dispatched two `kani.yml` runs one after the other in stage 4 and watched them from 12:04 to 13:16. The net block was about 20-25 min, because a 48-min smoke trio ran beside them.
- `slice-large-directory-collapse` (#109) spent 26 agent-hours over 93 calendar hours. Its design is past revision R14, and a pre-registered gate missed by 0.3 ms sent it back to design.
- Contention with five implementation slots costs the median change at most about 30 min, and each command runs 1.5-2x slower. The benefit was 13 merges in 19 h.
- Quiet windows are judged on the global 1-minute load, but `measure.sh` pins to CPUs 8-23. The coordinator reports that desktop apps alone hold the load at 5-6. In the `top` snapshots around measurements, the processes most often above 30 % CPU are a game, an IDE toolbox (usually about 150 %), Java, a browser and the file indexer.

Interventions 1 (the 10 s headless-session media wait), 2 (parallel smoke cases) and 4 (a tree-hash stamp for the commit hook) are engineering changes. They belong to `fix-headless-session-media-wait`, which is already running, and to the approved "smoke gate coherence" change, not to this one.

## What Changes

The decisions become normative rules in `.agents/rules/`, and the programme's workflow scripts move into the repository. No product code changes.

- **Gate reuse on a no-op merge** (intervention 3, `parallel-agent-work.md` "Merging"):
  - When the `origin/main` merge under `MERGE-LOCK` leaves the tree identical to one a gate passed on, recorded by tree hash through the tested `scripts/gate-trees.sh`, that gate is not rerun.
  - Any merge that changes the tree reruns the gates for both sides, as today.
  - New: just before `gh pr merge`, the agent re-fetches and requires `origin/main` to be an ancestor of `HEAD`. GitHub's merge commit then has exactly the tree the gates ran on.
- **Kani budget dispatch at the end of implementation** (intervention 5, `verification-runs.md` "Shard budgets", `parallel-agent-work.md` "Merging"):
  - Stage 3 dispatches the budget runs as soon as its final code is pushed, and does not watch them.
  - Stage 4 reads whatever has finished. A finished failure blocks the merge. An unfinished run does not.
  - The archive records the figures. `make check-kani-shards` already refuses an archive that still owes them.
- **Non-inferiority merges** (decision 1, `verification-runs.md` "Timing campaigns"):
  - A change declares its verdict class in design before any verdict round is run.
  - A change in the non-inferiority class merges on a pre-registered non-inferiority verdict, which is still run through `measure.sh`, interleaved, and judged quiet.
  - Its improvement verdict is queued for a named follow-up, and no document claims the improvement until that verdict passes.
- **Design-loop limit** (decision 2, `parallel-agent-work.md`): when a change that has had two design revisions is blocked again, an independent agent decides whether to continue, re-scope or shelve it, and the maintainer is told. A third revision starts with an attribution spike that writes no OpenSpec artifacts.
- **Fast lane** (decision 3, `parallel-agent-work.md`):
  - It applies to changes whose every path is on an allowlist (`docs/`, `openspec/`, root Markdown, and agent guidance with a declaration, except the two change-delivery rule files), and no timing verdict. Any other path, including a new directory, makes a change not eligible, so shipped code, proofs and gate implementations can never be fast by omission. That admits 2 of the last 20 PR merges.
  - Edits to rule and skill text are allowed only with a declared "Gate effect: none", which stage 4 verifies.
  - Spec and spec review are done by one agent. `design.md` is optional.
  - The independent stage-4 reviewer also reviews the spec and re-checks eligibility against the real diff. A violation sends the change back to a full spec review.
  - A versioned checker decides eligibility from paths.
- **Slot cap** (decision 4, `parallel-agent-work.md` "Implementation slots and ordering"):
  - At most five implementation slots, or three while `MEASURE-QUEUE.md` holds an entry or `###` part whose latest status is `READY`.
  - A stage that queues runnable rounds a change waits on (a verdict, or pre-registered gate or selection rounds) now marks them `READY <sha>`. `PENDING` is kept for rounds that are not runnable yet, and for non-blocking attribution and calibration entries.
  - Acquisition and release move from a copy-pasted snippet into a tested helper.
- **Quiet-window criterion: drain first, judged on busy CPUs** (decision 6, replaced by the maintainer's decision of 2026-11-18 19:39, level 1; `parallel-agent-work.md` "Measurements", `verification-runs.md` "Timing campaigns"):
  - Holding nothing, wait until CPU use outside our own slices is below 3.5 CPUs over 30 s; then `measure.sh --lock-timeout 2400`; inside the lock, run only after a 30 s sample below 4 busy CPUs, and keep the round only if the run and a 30 s sample after it are also below 4.
  - The bar stays 4; the metric changes from the 1-minute load to busy CPUs, which admits some rounds the old rule refused. The maintainer decided that loosening.
  - A tested helper, `scripts/quiet-gate.py`, implements the three steps and fails closed.
  - Level 2, a calibration under outside load, is the coordinator's queue entry. Level 3 (a pass under load counts for absolute budgets) waits for it and is not written into any rule.
- **Stages after implementation** (coordinator notes of 2026-11-18 18:50-19:25, maintainer-approved):
  1. `/simplify`, invoked with an absolute worktree path;
  2. findings-only review lenses in parallel: `code-review high <target>` never with `--fix`, plus one domain lens per flag: the `data-safety` audit when the change touches data, the `gtk-perf-review` leaf checklists when it touches performance, and `rust-hex-arch` when modules move inside such a fan-out;
  3. one independent agent verifies and applies the findings, runs one smoke trio and the full gates once;
  4. a merge and archive agent.

  The fan-out is chosen by a self-checked function over flags from `scripts/agent-topology.py review-lenses`. That subcommand reads the triggers the skills declare: `performance-scope` and a new `[data_safety]` table in `skill-policy.toml`.
- **Workflow-agent guard and outward writes:**
  - The script compares the main checkout's status, `HEAD` and branch, read by guard agents before and after every stage, and stops on any difference.
  - Agents make no outward writes beyond the listed ones (branch and twin-ref push and deletion, PR, merge, archive push, `kani.yml` dispatch). Sonar, GitHub settings, upstream comments and releases go to `host-asks.md`.
  - `scripts/sonar-local.sh` stops telling the reader to mark issues accepted.
- **Versioned workflow scripts** (`.claude/workflows/`):
  - One sanitized `programme-change.js` replaces the three session scripts. It has modes `new`, `fast-lane`, `revise` and `continue`, and stages bound to the OpenSpec skills, so no fragment exists in three copies.
  - Host-specific details (push command, gh account, repository path, commit trailer) come from `args`, never from the file.
  - Prompts point to rule sections instead of restating them.
  - `make check-workflow-scripts` (in `make check-policy`) syntax-checks each script through a wrapper and refuses personal data and copied rule text.
- **Records:**
  - A dated memorial holds the analysis and its figures.
  - Append-only ledger lines record that interventions 2 and 4 belong to "smoke gate coherence", and that decision 7 (widget-test edits no longer voiding the smokes) was approved with conditions and is implemented there, not here.
  - The `AGENTS.md` rules index entries are kept in sync.
- **This change's own merge waits for the maintainer.** It requires an independent Fable review of the new template. Its stage 4 ends with "ready, Fable template review pending" and does not merge. The coordinator runs that review, then a merge-only run with its fixes.

## Non-goals

- No change to any gate's coverage:
  - no lane, shard, scenario or assertion removed;
  - `B_INTERACT_US` and `B_SETTLE_US` unchanged;
  - flakes stay blockers;
  - the data-safety review and the independent code reviewer stay on every change.
- Interventions 1, 2 and 4, and instrumentation (intervention 7), are not implemented here.
- Decision 5 (host desktop apps): nothing. Decision 7: recorded only; smoke-gate-coherence implements it.
- Level 3 of the quiet-window decision is not implemented.
- In-flight workflow runs keep the session scripts they started with. The new rules apply at each run's next stage boundary.

## User-visible impact

None for LushText users: no product code, persisted format, setting, action or D-Bus member changes. Compared with `main`, agents gain these rules: an archive can come after the merge in a second lock hold, a change may merge on non-inferiority, fast-lane changes get no separate spec-review agent, and implementation slots drop to three while rounds a change waits on are ready. Each of these is listed with the existing rule it touches in `design.md`, "Quality floor".

## Capabilities

### New Capabilities

- `change-delivery-flow`: Purpose: "How a LushText change moves from spec to `main` while agents work in parallel: the stages each change runs, implementation slots, gate reuse under the merge lock, the timing of Kani budget dispatches, non-inferiority merges, the design-loop limit, the quiet-window criterion, and the versioned workflow scripts that run them."

### Modified Capabilities

None. The Kani shard-budget requirement in `formal-verification-kani` already says a pending figure is replaced before the archive, and that stays true. This change moves only when the dispatch happens, so that requirement is not touched.

## Impact

- **Rules:** `.agents/rules/parallel-agent-work.md` (slots, "Running a change", design revisions, fast lane, measurements, merging), `.agents/rules/verification-runs.md` ("Timing campaigns", "Shard budgets"), `.agents/rules/build.md` ("Shard gates and budgets", one sentence), `AGENTS.md` rules index (the `parallel-agent-work.md` and `verification-runs.md` entries).
- **Scripts:**
  - `scripts/impl-slots.sh`, new, with `--self-test`;
  - `scripts/gate-trees.sh`, new, with `--self-test`;
  - `scripts/check-fast-lane.py`, new, with `--self-test`;
  - `scripts/check-workflow-scripts.sh`, new, with `--self-test`;
  - `scripts/quiet-gate.py`, new, with `--self-test`;
  - the `Makefile` targets `check-workflow-scripts` and `check-agent-helpers`.
- **Workflows:** `.claude/workflows/programme-change.js`, new and versioned.
- **Skill routing and wording:**
  - `.agents/skills/skill-policy.toml` gets a `[data_safety]` table;
  - `scripts/validate-agent-skills.py` gets its consistency check;
  - `scripts/agent-topology.py` gets `review-lenses`, tested in `scripts/test-agent-topology.py`;
  - `scripts/sonar-local.sh` gets its failure message reworded.
- **Docs:** `docs/memorials/2026-11-18-change-flow-time.md` and its index row in `docs/memorials/README.md`.
- **Handoff files (gitignored, append-only):** `PARALLEL-LEDGER.md` lines. No `MEASURE-QUEUE.md` entry: the coordinator's calibration entry is the experiment.
