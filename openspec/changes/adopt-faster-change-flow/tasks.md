Scope:
- **Tests:** none for product behaviour, because no behaviour changes. The new helpers are tested by `--self-test` cases, written first and seen failing against a stub that exits 0, then made to pass.
- **Kani:** none (no proven module touched).
- **Timing:** none (no timed path touched). The calibration was the coordinator's non-verdict queue entry (design D7, stage-3 revision); the maintainer closed it not established on 2026-11-19, and level 3 is off.
- **GTK axioms:** none.
- **Smokes:** none (no UI-prefix path).

Every commit goes through `scripts/heavy.sh --mem none -- git commit -F <file>`, because `make check-policy` still runs for docs-only commits. Before each commit group, read `COORDINATOR-NOTE.txt` in `.claude/handoffs/adopt-faster-change-flow/`.

## 1. Records (memorial and handoff files)

- [x] 1.1 Write `docs/memorials/2026-11-18-change-flow-time.md`:
  - the analysis's verdict, the time breakdown with its method and limits, the before/after table, the Kani share, the ranked interventions and the weakest assumptions (sections 1-4 and 8 of the analysis);
  - the maintainer's decisions of 2026-11-18, dated: 1, 2, 3, 4 and 6 accepted, 5 rejected, 7 approved later with conditions (smoke-gate-coherence); the 19:39 decision on the quiet-window criterion (levels 1, 2 and 3; level 2 closed not established and level 3 off on 2026-11-19, appended as a dated line);
  - which engineering changes own interventions 1, 2 and 4.

  No normative text: every rule it explains links back to its rule section.
- [x] 1.2 Add its row to the table in `docs/memorials/README.md`: "the change-flow rules (no-op merge reuse, Kani dispatch timing, non-inferiority merges, design-loop limit, fast lane, slot cap, drain-first quiet criterion) and their measured baseline".
- [x] 1.3 Append to `.claude/handoffs/PARALLEL-LEDGER.md` (gitignored, append-only) the lines of design D9 (a), (b) and (c). Grep first, and write only the lines that are not already there.
- [x] 1.4 Confirm that `.claude/handoffs/MEASURE-QUEUE.md` holds the coordinator's "Calibration: G0 rounds under outside load" entry (`Kind: CALIBRATION`, `Status: PENDING`) and append nothing (design D7, stage-3 revision: it replaces stage 1's handoff probe, round runner and calibration entry). The entry is CLOSED since 2026-11-19 (level 2 not established).

## 2. Helpers with self-tests (one commit)

- [x] 2.1 `scripts/impl-slots.sh` (`acquire <change> [--timeout S]`, `release <change>`, `cap`), as in design D6.
  - Write `--self-test` first, against a stub, and record it failing. It uses fixture queues for:
    - no entries;
    - the last status `READY`;
    - a stage-queued verdict `Status: READY <sha>`;
    - `Kind: SELECTION ROUNDS` with `Status: READY <sha>` (counts);
    - a `PENDING (NOT READY …)` verdict;
    - `READY` then `RAN`;
    - `PENDING` calibration;
    - an earlier `READY` superseded and a later entry `DONE`;
    - an entry with `### ` parts whose part B ends `READY` and part C ends `DONE` (counts);
    - a copy of the live queue's heading and `Status:` structure at implementation time (expect the ready units the coordinator counts in the ledger);
    - plus acquire, wait, timeout (exit 124) and release on a temporary slot directory.

  It honours `LUSHTEXT_AGENT_LOCK_DIR`-style overrides for the handoff directory, so the self-test touches no real queue or slot.
- [x] 2.1a `scripts/gate-trees.sh` (`record <worktree> --gate '<gate>' -- <command…>`, `lookup <worktree> --gate '<gate>'`), as in design D1. `record` reads `git status --porcelain` and `HEAD^{tree}` before and after, runs the command as its child, and appends a line only on exit 0 with a clean, unchanged tree. It honours the same handoff-directory override as 2.1.
  - Its `--self-test`, written first and seen failing against a stub, uses a temporary repository: a clean pass records; a dirty-before, dirty-after or tree-changing run records nothing; a failing command records nothing and returns its exit status; `lookup` finds only the same gate string on the same tree.
- [x] 2.1b `scripts/quiet-gate.py` (`outsiders`, `wait-outsiders`, `busy`, `run`, `--self-test`), as in design D7. Our own work is read from the parent slice of `scripts/with-memory-cap.sh --print-slice` (by `systemctl --user show -p ControlGroup`), the legacy slice, and the `cst-sccache-*.scope` siblings; no uid or home path is hardcoded.
  - Its `--self-test`, written first and seen failing against a stub, uses fixture `/proc/stat` and `cpu.stat` files and sub-second windows: busy CPUs from two stat reads; outsiders = root minus our slices; `wait-outsiders` succeeds below the bar and exits 124 above it; `run --mode gate` exits 76 without running the command on a busy pre-sample, 77 on a busy post sample after exit 0, and the command's status otherwise; `--mode calib` never gates; every reading is logged; an unreadable stat or `cpu.stat` exits 2 with no reading.
- [x] 2.2 `scripts/check-fast-lane.py [--base REF] [--paths FILE] [--self-test]`, as in design D5: an allowlist with the classes `eligible`, `declare` and `not eligible`, failing closed on any unlisted path, reading both sides of a rename, and promoting an `eligible` path that `Makefile`, `.githooks/`, `.github/` or `scripts/` names literally to `declare`.
  - Its self-test, written first and seen failing, covers:
    - eligible: a `docs/memorials/` and `openspec/changes/` path set;
    - `declare`: `.agents/rules/build.md`, `.agents/skills/x/SKILL.md`, `AGENTS.md`, `SOUL.md`, `Makefile`, and a `docs/` path a gate names;
    - not eligible: `crates/lushtext-core/src/app.rs`, `crates/lushtext-core/src/ui/AGENTS.md`, `crates/lushtext/tests/widget/x.rs`, `crates/gtk-lush/settle/src/lib.rs`, `resources/ui/x.blp`, `Cargo.lock`, `.cargo/mutants.toml`, `.gitignore`, `.agents/rules/parallel-agent-work.md`, `.agents/rules/verification-runs.md`, `scripts/flatpak-source-tree.sh`, `scripts/agent-topology.py`, `.agents/skills/skill-policy.toml`, `.claude/workflows/x.js`, a rename from `docs/x.md` to `scripts/x.sh`, an untracked new file, and a new top-level directory;
    - that every `kani_proofs*.rs` in `git ls-files` and every `ORACLES` key of `scripts/kani-shards.py` classes `not eligible`.
  - As evidence that it is fit for use, run it over the PR merges among the last 25 first-parent merges on `main` at stage 2 (`bc109b04` and back). Stage 2 counted 2 of 20 (#116 eligible, #98 `declare`) with the classifier saved at `.claude/handoffs/adopt-faster-change-flow/fast-lane-baseline.py`, and the run must reproduce that or explain the difference.
- [x] 2.3 `scripts/check-workflow-scripts.sh [--self-test]`, as in design D8: meta check, wrapper, `node --check`, phase titles, personal-data and credential patterns, copied 12-word rule sentences, and rule references.
  - **Rule references:** prompts cite sections as `<file>.md "<Section>"`, and the check fails when a cited heading does not exist in that file.
  - When `node` is missing, it fails with a message. With no `.claude/workflows/*.js` present it passes and says so (this commit lands before the script).
  - Copied rule text is any run of 12 consecutive words, normalized for case and punctuation, that also appears in `.agents/rules/*.md`.
  - The self-test, written first and seen failing, seeds each failure, including a top-level `await` and `return` that must parse.
- [x] 2.3a Review-lens routing (design D10):
  - a `[data_safety]` table in `.agents/skills/skill-policy.toml`, holding path suffixes and content patterns, the machine form of the `data-safety` description;
  - `scripts/validate-agent-skills.py` fails when a pattern's keyword is missing from that description;
  - `scripts/agent-topology.py review-lenses --base <ref>` prints `{data, performance, modules_moved, paths}`. `performance` reuses `performance-scope`. `modules_moved` is an added, deleted or renamed `.rs` under a `src/` tree, read from `git diff --find-renames --name-status`.

  The tests in `scripts/test-agent-topology.py` and `test-validate-agent-skills.py` are written first and seen failing. They cover: a draft-service edit (data and performance), a `docs/` edit (neither), a `crates/gtk-lush/settle/` edit (performance only), a renamed module (`modules_moved`) and a content-pattern hit outside the path suffixes.
- [x] 2.4 Re-check that `openspec validate <change> --strict` passes for a change with no `design.md`, on the CLI version in use. Stage 1 found it valid on 2026-11-18 (design D5). Use a temporary copy of this change in a scratch directory. If it fails, change D5 to require a short `design.md` in the fast lane.
- [x] 2.5 `Makefile`:
  - `check-workflow-scripts` (in `check-policy`; put the prerequisite on its own line if `check-policy`'s list has been split by then);
  - `check-agent-helpers` runs `impl-slots.sh --self-test`, `gate-trees.sh --self-test`, `check-fast-lane.py --self-test` and `quiet-gate.py --self-test`, and shellchecks the shell scripts.

  Run `scripts/heavy.sh -- make check-agent-helpers`, `scripts/heavy.sh -- make check-workflow-scripts` and `scripts/heavy.sh -- make check-agent-skills`. The last one is not in `check-policy` (only CI's `ci.yml` runs it), and 2.3a edits its inputs.
- [x] 2.6 Commit `chore(build): add the slot, gate-record, quiet-gate, fast-lane, review-lens and workflow-script helpers`, and push.

## 3. Rules (one commit)

- [x] 3.1 Edit `.agents/rules/parallel-agent-work.md`:
  - **"Implementation slots and ordering":** the cap of design D6 (ready units: entries or `###` parts) and `scripts/impl-slots.sh` replace the inline snippet. `READY` means runnable rounds a change waits on. The rule names `.claude/workflows/programme-change.js`, which lands in commit 4; no gate checks paths cited in rules (`scripts/check-agent-docs.sh` checks the index and `openspec/config.yaml` only, read at stage 2).
  - **New "Running a change"** section:
    - the stage sequence;
    - the versioned script, its modes and the fast-lane flag;
    - how to launch it: `scriptPath` is guaranteed, the name form is optional;
    - `args.local` for host details, kept outside the repo;
    - the items moved from the `RULES` string (design D8): own worktree, `COORDINATOR-NOTE.txt`, headless only, never `SendMessage` a workflow agent, commit messages by file;
    - when a new crate must be added to the fast-lane exclusions.
  - **New "Fast lane"** section (design D5): the exclusions, `declare` paths and the "Gate effect: none" declaration that stage 4 verifies.
  - **New "Design revisions"** section (design D4).
  - **New "Stages after implementation"** section (design D10): order, single-pass behaviour of both skills inside workflows, absolute-path `/simplify`, findings-only `code-review` without a schema, failed-lens rule, fan-out criterion by reference to the skills' triggers, the apply agent, and one smoke trio.
  - **New "Workflow agents"** section (design D11): the script-enforced main-checkout guard read by guard agents, and what the coordinator does when the main checkout must move during a run; `git -C`/absolute paths; per-agent `TMPDIR`; `timeout` on every harness; the brief file re-read; the outward-writes list (including deleting the change branch and twin ref), with everything else going to `host-asks.md`; and a citation of the maintainer's observed-behaviour note as evidence only.
  - **"Measurements":**
    - the drain-first criterion of design D7 replaces the 1-minute-load check: `quiet-gate.py wait-outsiders`, `measure.sh --lock-timeout 2400`, `quiet-gate.py run`, the kept-round rule, and the detached run plus `wait-for.sh` for a stage agent; level 3 is not written (off since 2026-11-19);
    - non-inferiority changes as in D3;
    - the entry rule of D6: runnable rounds a change waits on, on a final tree, are queued as `Status: READY <sha>`, ones not runnable yet as `PENDING (NOT READY: <reason>)`, and non-blocking attribution and calibration entries as `PENDING`.
  - **"Merging":**
    - step 2: reuse on an identical tree via `scripts/gate-trees.sh` (design D1);
    - a new check before step 3: re-fetch, and require `origin/main` to be an ancestor of `HEAD`;
    - the CI exception now covers only correctness evidence, with budget runs as in D2;
    - an archive may follow in a second `MERGE-LOCK` hold when it owes Kani figures;
    - Kani figures land only in the archive commit, limited to the pending records' fields;
    - stage 3 dispatches on a twin ref `<branch>-kani-2` for a second concurrent run, deleted after the archive.
- [x] 3.2 `.agents/rules/verification-runs.md`:
  - **"Timing campaigns":** the verdict class and the non-inferiority verdict (design D3), and the drain-first criterion in place of "under load, queue" (design D7);
  - **"Shard budgets":** dispatch when stage 3 ends, stage 4 reads, re-dispatch triggers (design D2).
- [x] 3.3 `.agents/rules/build.md`, "Shard gates and budgets": one sentence pointing to `verification-runs.md` "Shard budgets" for when to dispatch. The gate's behaviour is unchanged.
- [x] 3.4 `AGENTS.md` rules index: update the `parallel-agent-work.md` entry (running a change, fast lane, design revisions, slot cap) and the `verification-runs.md` entry (verdict classes, dispatch timing).
  - Add `make check-workflow-scripts` to "Build Commands" if `make check-policy`'s neighbours are listed there.
  - Check `README.md` for a list of policy targets and update it if one exists.
  - Run `scripts/heavy.sh -- make check-agent-docs`.
- [x] 3.4a Reword the failure message in `scripts/sonar-local.sh` (design D11), and its header comment at line 6, which says the same: fix the issues, or record a suspected false positive in the change's `host-asks.md`; only the maintainer changes Sonar state. Grep `.agents/`, `AGENTS.md`, `docs/` and `scripts/` for the same instruction, and reword any hit.
- [x] 3.5 Write the "Quality floor" self-check into the commit's handoff note `.claude/handoffs/adopt-faster-change-flow/floor-check.md`: for every edited rule paragraph, the existing rule it touches and why the floor holds, against design's "Quality floor" table.
- [x] 3.6 Commit `docs(rules): adopt the faster change flow`, and push.

## 4. Versioned workflow script (one commit)

- [x] 4.0 Run the stage agent's probes of design D11 first, under a manual main-checkout guard (status, `HEAD` and branch before and after each): `code-review` with a local-branch target and with an absolute worktree-path target, each over a scratch branch whose committed and uncommitted edits both hold a planted bug; and `/simplify` with a worktree directory argument, across several files, and on committed branch changes. Record where each `code-review` run's findings came from (result text or fork transcript); this agent runs with a caller schema, so the same review without one is owed by the coordinator (note 19:52). Record each command, result and conclusion in `.claude/handoffs/adopt-faster-change-flow/probes.md`. A probe that edits the main checkout stops the work, and the coordinator is told. Then append a request to `.claude/handoffs/adopt-faster-change-flow/host-asks.md` for the two probes only the coordinator can run (a stage agent has no Agent or Workflow tool, and a probe PR is outside the outward-writes list): `agent(..., {isolation: 'worktree'})` and its cwd, and `code-review` on an open PR with a planted bug that differs from the main checkout.
- [x] 4.1 Write `.claude/workflows/programme-change.js` from the three session scripts (`programme-change-v2.js`, `-revise.js`, `-continue.js`; paths in the stage brief), as in design D8:
  - modes `new`, `fast-lane`, `revise`, `continue` and `review-only` (design D8; note 20:34: an existing PR from `/simplify` on, stopping before the merge unless `args.merge`), and `args.dry_run`; `revision`, which runs the D4 decision and spike agents; `design_only`; `skip_impl`;
  - one shared stage-4 function;
  - stage 3 dispatches Kani budget runs (D2);
  - stage 4 reuses gate evidence on an identical tree and checks the ancestor (D1);
  - the slot helper (D6);
  - the non-inferiority branch of "ready, measurement pending" (D3);
  - the stages after implementation (D10), the `code-review` lens taking its findings from the result text or the fork transcript and failing without `skill_findings_source`, with `selectLenses` as a pure function mapping each flag to its own lenses (design D10 table) whose cases the script asserts at start-up, the `code-review` lens without a schema covering both the commits and `/simplify`'s edits in the form 4.0 chose (or the named two-call fallback), and `args.review_agent_type`;
  - the guard around every stage (D11): guard-read agents before and after every stage and around each parallel group, a script comparison and a stop, plus per-stage `TMPDIR` and brief files;
  - stage prompts that name `openspec-propose`/`openspec-update-change`, `openspec-apply-change` and `openspec-archive-change` instead of describing their procedure;
  - prompts cite rule sections and carry no copied rule text, path, account, key or trailer;
  - every stage prompt says never to return while a command it started is still running, and the apply agent commits locally before the long gates and pushes at the end (note 20:16).
- [x] 4.1a Write the stub-agent harness `.claude/workflows/programme-change.test.mjs` (note 20:16): dry run and bad arguments; failed lenses ("(none)", a dead agent, a missing status line); a guard violation; a malformed selector; fast-lane ineligibility and violation; the finished path; review-only with and without `merge`. A copy of the script with its guard comparison disabled must fail it. `scripts/check-workflow-scripts.sh` runs `<name>.test.mjs` beside each script and fails when it is missing or fails, with self-test cases for both.
- [x] 4.1c Apply note 20:38 to the template, its harness and the rules: no force-push; refuse main-branch aliases (`heads/`, `refs/`, `origin/`, `remotes/`, `tags/`, `HEAD`) and worktrees outside `.claude/worktrees/`; normalize PR values; `CODE-REVIEW-STATUS: ok|failed` as the primary lens check with text heuristics as the backstop; the two-way lens-trigger check in `scripts/agent-topology.py validate`, failing-first in `scripts/test-agent-topology.py`; the merge stage's stopping rule (review the apply commit only); stubs that return distinct values per stage, with precedence cases.
- [x] 4.1b Fold notes 20:16 and 20:34 into `parallel-agent-work.md`: "Waiting" (never return with a command running), "Merging" (watch CI runs by id), "Running a change" (`review-only`, `dry_run`, the harness, `scriptPath` from the main checkout), "Stages after implementation" (commit before the long gates).
- [x] 4.2 Record in the commit message which session-script behaviours were kept, changed or dropped. The archive-previous stage's hardcoded `docs/next/long-line-documents.md` becomes `args.programme_record`.
- [x] 4.3 Run `scripts/heavy.sh -- make check-workflow-scripts`, then `git check-ignore -v .claude/workflows/programme-change.js`, which must print nothing.
- [x] 4.4 Check that the template's `code-review` target form and `/simplify` fallback match `probes.md` (4.0). State in `probes.md` which two probes the coordinator still owes. The template is not used until they are recorded and the template matches them; if `isolation: 'worktree'` gives a worktree cwd, the coordinator's merge-only run may simplify the template accordingly.
- [x] 4.5 Commit `chore(workflow): version the programme-change workflow`, and push.

## 5. Final gates and PR

- [x] 5.1 Run these on the final tree through `scripts/gate-trees.sh record` (design D1):
  - `scripts/heavy.sh -- make check-policy`;
  - `scripts/heavy.sh -- make check-agent-skills` and `scripts/heavy.sh -- make check-agent-docs` (neither is in `check-policy`);
  - `openspec validate adopt-faster-change-flow --strict`;
  - `openspec validate --all` (errors gate).
- [x] 5.2 Open the PR. Its body lists:
  - the local evidence;
  - the "Quality floor" table;
  - that in-flight runs keep their session scripts until their next stage boundary;
  - that decision 7 was approved with conditions and is implemented by smoke-gate-coherence, not here;
  - that the stage-3 revision (design "Stage-3 revision") had no spec review.

  It ends with the required Claude Code line. Do not merge, and do not wait for CI.
- [x] 5.3 **This change's stage 4 does not merge** (maintainer, coordinator note 2026-11-18 19:00): it reviews and fixes as usual, then ends with the verdict "ready, Fable template review pending". The coordinator runs the independent Fable review of `.claude/workflows/programme-change.js` and its two owed probes (4.4), then a merge-only run with their fixes.

  Done: stage 4 (`d2f1b58b`) ended "ready, Fable template review pending"; the coordinator ran probes C1-C4 (`probes.md`) and the independent Fable review (`fable-review.md`, verdict "merge after the listed fixes"). The fixes are section 6.

## 6. Fable review fix round (coordinator note 2026-11-18 23:20)

Scope: M1-M5, S1-S14 and N1-N8 of `fable-review.md`, the maintainer's rules (a) and (b), and a disposition for every item not fixed (design "Fable review fix round"). Every must-fix gets a case written first and seen failing against `d2f1b58b`. This round's stage 4 reviews and fixes as usual but does not merge: it ends "ready, Fable fix check pending", and the coordinator then runs a narrow Fable pass on the fix diff and a merge-only run with `make test` on the final tree.

- [x] 6.1 Failing-first cases: the M1-M5 cases on the `d2f1b58b` harness interface (`failfirst-fix-round-harness.log`), and the helper cases for M2, S7, S11, S12, N3 and N4 (`failfirst-fix-round-helpers.log`); N2 and the slot tolerance against the `d2f1b58b` helpers in `failfirst-stage4-n2-slot.log` (stage 4).
- [x] 6.2 Helpers (one commit, `8e1162b3`): `review-lenses` exits 2 without `[data_safety]` (M2), `GIT_PREFIX` (N6); `gate-trees.sh` fail records and lookup refusal (S12), one-line fields (N2); `impl-slots.sh` refuses a name without a change worktree, Markdown `Status:` (N4); `check-fast-lane.py` classes the `Makefile` not eligible (S7, baseline re-run: still 2 of 20); `check-workflow-scripts.sh` checks every cited title (S11); `quiet-gate.py` first eight fields (N3).
- [x] 6.3 Template: prepare stage (M2, S8); `/simplify` with literal bases, a clean start and new files in the patch (S9, N8); scope reading with the selector's exit status and raw JSON (M2, M4) and `HEAD` = PR head = upstream (M1); `code-review` as `high <PR>` plus `high <worktree>` with scope, `findings_count` and `ok|empty|failed` lines (M1, N1, rule (b)); domain lenses with `status` and `examined` (M4); the apply stage re-runs the selector, re-dispatches stale Kani budget runs and returns `head` and `tree` (M4, S10, S5); a read-only check of the apply output (S5); a final review that edits nothing over the exact range and tree, re-checking rejected correctness findings, with a fixed verdict set (M3, M5, rule (a), S13); argument checks before the first agent (S1, S2, S4, S14, N5); the PR an earlier round opened (S3); the guard by status hash with slot and lock readings and `args.guard_ack` (S6); the header (N7).
- [x] 6.3a Hardening after a review of the first fix commit (design "Fable review fix round"): no expected counts in the lens prompt and a base check on the worktree call; a shell-computed scope reading with `review-lenses --flags-only` (failing-first test); a shell-compared apply read-back with a `gate-trees.sh lookup` of `make check`; interrupted runs record nothing in `gate-trees.sh` (failing-first self-test case); slot and lock release is no guard difference; `args.accept_empty_review` bound to the reviewed `HEAD`; recovery wording for stops that leave `/simplify`'s edits; slot naming under `args.worktree_override` documented.
- [x] 6.4 Harness: the cases above on the new interface, the inverted S4 and M3 cases, and the guard mutant still caught.
- [x] 6.5 Rules: `parallel-agent-work.md` "Stages after implementation", "Workflow agents", "Running a change", "Fast lane" and "Merging" step 2; the delta spec; design D10, D11 and "Fable review fix round" with its floor rows; `probes.md` records the M1 target form.
- [x] 6.6 Gates on the final tree: `make check`, `make check-agent-skills`, `make check-agent-docs`, `make check-workflow-scripts` (in `check-policy`), the helpers' self-tests (`make check-agent-helpers`, in `check-policy`) and `openspec validate --strict`. `make test` is not owed: no Rust or test input changed (the coordinator's merge-only run runs it).
- [x] 6.7 Commit and push per task group; update the body of PR #118.
- [x] 6.8 Stage-4 review of the fix round (design "Stage-4 review of the fix round"): R1 the final review may make the archive commit, R2 every merged verdict names its merge commit, R3 the apply-stage stops name the uncommitted `/simplify` edits, R4 unstartable runs record nothing in `gate-trees.sh`, R5 the merge-only run defined; cases written first; gates rerun on the final tree.

## 7. Fable fix check round (coordinator note 2026-11-19 01:10)

Scope: F1-F11 of `fable-fix-check.md`, the `worktree_override` slot name, and the maintainer's decisions of 2026-11-19 01:08 (R4 append-only void record; "Merging" step 2 reruns every gate after a non-empty `origin/main` merge) and 00:58 (level 2 closed not established, level 3 off). Dispositions are in design "Fable fix check round". This round's stage 4 does not merge: it ends "ready, Fable fix check 2 pending", and `make test` is still owed on the final tree before the merge.

- [x] 7.1 Failing-first cases against `8a2bce8b`: the new harness cases (F1, F3, F4, F5, F6, F8, F9, F10, step 2, slot name) in `failfirst-fixround2-harness.log`; the new `gate-trees.sh` self-test and direct probes of `void`, `lookup --tree` and exit 124 in `failfirst-fixround2-gate-trees.log`; seven hand mutants of the fixes, all caught, in `mutants-fixround2-harness.log`.
- [x] 7.2 `scripts/gate-trees.sh`: `result=interrupted` for exit 124 and 128+ (F7), named on stderr by lookup and never reused; the coordinator's append-only `void` (R4) naming exactly one fail line with its evidence log and reason; `lookup --tree <hash> --change` for a merged change (F6); nanosecond line timestamps; unknown results refuse; header and self-test.
- [x] 7.3 Template: one findings count per call and the PR call judged alone (F1); the archive commit of "Merging" step 4 and the blocked-after-merge stop (F3); a prompt digest in every guard reading before a stage (F4); `args.lens_round` (F5); `final_tree` required and the post-merge reading with `gate-trees.sh lookup --tree` and the UI-prefix list (`RELEVANT_PREFIXES`) read from `origin/main`, hashes resolved in the shell, failing closed (F6, decision step 2); one set of gate strings named to every stage that records gates; the merge prompt's every-gate rerun after a non-empty merge (step 2); `continue`+`skip_impl` needs a PR before any agent (F8); anchored worktree paths (F9); the empty-review token bound to the review's text (F10); the slot under the worktree's directory name.
- [x] 7.4 Harness: the cases of 7.1 pass with the fixes, a replay of a first run the way the runtime resumes (probes C5, C6), and the guard mutant still caught.
- [x] 7.5 Rules and records: `parallel-agent-work.md` "Implementation slots and ordering", "Measurements" (level 3 text removed; merge-only resume and its fallback), "Merging" step 2 (no-op merge, every gate after a non-empty merge, append-only records, interrupted lines, the coordinator's void), "Durability", "Stages after implementation" 2, 4 and the new 5, "Workflow agents"; the delta spec; design D7 and "Fable fix check round" with its floor rows; the proposal, the memorial (an appended dated line), `AGENTS.md` and `README.md`.
- [x] 7.6 Gates on the final tree, each through `scripts/gate-trees.sh record`: `make check` (with `make check-workflow-scripts` and `make check-agent-helpers` inside it), `make check-agent-docs`, `make check-agent-skills`, and `openspec validate adopt-faster-change-flow --strict`. `make test` is not owed by this round (no Rust or test input changed); the coordinator's merge-only run runs it.
- [x] 7.7 Commit and push per task group (`06aa14be` helper, `1620ca17` template and rules, then the review fixes and these boxes); update the body of PR #118.

## 8. Fable fix check 2 round (coordinator note 2026-11-19 02:45)

Scope: H1 of `fable-fix-check-2.md`, C7 #4 and C7 #2 of the real code-review probe, and recorded dispositions for C7 #3, #5-#9 and the ten Fable-2 follow-ups (design "Fable fix check 2 round"). This round's stage 4 reviews this round's diff only and does not merge: it ends "ready, Fable fix check 3 pending", and `make test` on the final tree is the coordinator's merge-only run.

- [x] 8.1 Failing-first: the H1, C7 #2 and inverted C7 #4 harness cases and the new S5 variants against a67e0a98 (`failfirst-fixround3-harness.log`); the C7 #2 probes (`probe-c7-2-smoke-currency.log`), the smoke check on eight real histories (`smokecheck-cases-fixround3.log`) and the post-merge classifier on five path lists (`uicheck-cases-fixround3.log`).
- [x] 8.2 Template: a separate `make test` lookup in the apply verification; the conditional no-op sentence and the pre-merge record check in the merge prompt; the full `make test` named in the apply prompt; the shared smoke check, its records in the apply stage and the verification, and the no-op post-merge owed list; `UI_CHECK` with `is_relevant`.
- [x] 8.3 Rules and spec: "Merging" step 2, "Durability", "Stages after implementation" 3 and 5; the delta spec's new requirement and scenarios; dispositions in design.
- [x] 8.4 Gates on the final tree: `make check` (with `check-workflow-scripts` and `check-agent-helpers`), `make check-agent-docs`, `make check-agent-skills`, `openspec validate adopt-faster-change-flow --strict`; commit and push; update the body of PR #118.
