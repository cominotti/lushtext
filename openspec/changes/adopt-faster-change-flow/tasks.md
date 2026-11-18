Scope:
- **Tests:** none for product behaviour, because no behaviour changes. The new helpers are tested by `--self-test` cases, written first and seen failing against a stub that exits 0, then made to pass.
- **Kani:** none (no proven module touched).
- **Timing:** none (no timed path touched). The calibration is queued as a non-verdict experiment.
- **GTK axioms:** none.
- **Smokes:** none (no UI-prefix path).

Every commit goes through `scripts/heavy.sh --mem none -- git commit -F <file>`, because `make check-policy` still runs for docs-only commits. Before each commit group, read `COORDINATOR-NOTE.txt` in `.claude/handoffs/adopt-faster-change-flow/`.

## 1. Records (memorial and handoff files)

- [ ] 1.1 Write `docs/memorials/2026-11-18-change-flow-time.md`:
  - the analysis's verdict, the time breakdown with its method and limits, the before/after table, the Kani share, the ranked interventions and the weakest assumptions (sections 1-4 and 8 of the analysis);
  - the maintainer's decisions of 2026-11-18, dated: 1, 2, 3, 4 and 6 accepted, 5 rejected, 7 open;
  - which engineering changes own interventions 1, 2 and 4.

  No normative text: every rule it explains links back to its rule section.
- [ ] 1.2 Add its row to the table in `docs/memorials/README.md`: "the change-flow rules (no-op merge reuse, Kani dispatch timing, non-inferiority merges, design-loop limit, fast lane, slot cap, quiet-window calibration) and their measured baseline".
- [ ] 1.3 Append to `.claude/handoffs/PARALLEL-LEDGER.md` (gitignored, append-only) the lines of design D9 (a), (b) and (c). Grep first, and write only the lines that are not already there.
- [ ] 1.4 Write the calibration probe `.claude/handoffs/adopt-faster-change-flow/quiet-probe.sh`:
  - it prints one line with the date, the global 1-minute load, the per-CPU busy fraction for `scripts/measure.sh --print-reference-cpus` over 30 s from two `/proc/stat` reads, their mean and maximum, and `/proc/pressure/cpu` `some avg10`;
  - it runs and prints one line (not heavy);
  - beside it, a round runner `calib-round.sh <arm-label> <case>` that runs the probe and then `scripts/measure.sh --label calib-<label> --lock-timeout 300 --log .../calibration/measure.log -- env RESPONSIVENESS_VARIANTS=plain scripts/run-responsiveness-smoke.sh --binary <prebuilt> --artifact-dir .../calibration/<label> --case <case>`.
- [ ] 1.5 Append the calibration entry to `.claude/handoffs/MEASURE-QUEUE.md` under `flock`, with `Kind: CALIBRATION` and `Status: PENDING`, never `READY`. It holds:
  - the two cases, the binary build command (through `heavy.sh`) and the arms with their pre-registered thresholds (design D7);
  - at least 8 rounds per arm, interleaving the two cases;
  - the match bar, the 7-day host-ask fallback and the artifact directory;
  - "result goes to the maintainer; the criterion changes only by maintainer decision".

## 2. Helpers with self-tests (one commit)

- [ ] 2.1 `scripts/impl-slots.sh` (`acquire <change> [--timeout S]`, `release <change>`, `cap`), as in design D6.
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
- [ ] 2.1a `scripts/gate-trees.sh` (`record <worktree> --gate '<gate>' -- <command…>`, `lookup <worktree> --gate '<gate>'`), as in design D1. `record` reads `git status --porcelain` and `HEAD^{tree}` before and after, runs the command as its child, and appends a line only on exit 0 with a clean, unchanged tree. It honours the same handoff-directory override as 2.1.
  - Its `--self-test`, written first and seen failing against a stub, uses a temporary repository: a clean pass records; a dirty-before, dirty-after or tree-changing run records nothing; a failing command records nothing and returns its exit status; `lookup` finds only the same gate string on the same tree.
- [ ] 2.2 `scripts/check-fast-lane.py [--base REF] [--paths FILE] [--self-test]`, as in design D5: an allowlist with the classes `eligible`, `declare` and `not eligible`, failing closed on any unlisted path, reading both sides of a rename, and promoting an `eligible` path that `Makefile`, `.githooks/`, `.github/` or `scripts/` names literally to `declare`.
  - Its self-test, written first and seen failing, covers:
    - eligible: a `docs/memorials/` and `openspec/changes/` path set;
    - `declare`: `.agents/rules/x.md`, `.agents/skills/x/SKILL.md`, `AGENTS.md`, `SOUL.md`, `Makefile`, and a `docs/` path a gate names;
    - not eligible: `crates/lushtext-core/src/app.rs`, `crates/lushtext-core/src/ui/AGENTS.md`, `crates/lushtext/tests/widget/x.rs`, `crates/gtk-lush/settle/src/lib.rs`, `resources/ui/x.blp`, `Cargo.lock`, `.cargo/mutants.toml`, `.gitignore`, `scripts/flatpak-source-tree.sh`, `scripts/agent-topology.py`, `.agents/skills/skill-policy.toml`, `.claude/workflows/x.js`, a rename from `docs/x.md` to `scripts/x.sh`, an untracked new file, and a new top-level directory;
    - that every `kani_proofs*.rs` in `git ls-files` and every `ORACLES` key of `scripts/kani-shards.py` classes `not eligible`.
  - As evidence that it is fit for use, run it over the PR merges among the last 25 first-parent merges on `main` at stage 2 (`bc109b04` and back). Stage 2 counted 2 of 20 (#116 eligible, #98 `declare`), and the run must reproduce that or explain the difference.
- [ ] 2.3 `scripts/check-workflow-scripts.sh [--self-test]`, as in design D8: meta check, wrapper, `node --check`, phase titles, personal-data and credential patterns, copied 12-word rule sentences, and rule references.
  - **Rule references:** prompts cite sections as `<file>.md "<Section>"`, and the check fails when a cited heading does not exist in that file.
  - When `node` is missing, it fails with a message. With no `.claude/workflows/*.js` present it passes and says so (this commit lands before the script).
  - Copied rule text is any run of 12 consecutive words, normalized for case and punctuation, that also appears in `.agents/rules/*.md`.
  - The self-test, written first and seen failing, seeds each failure, including a top-level `await` and `return` that must parse.
- [ ] 2.3a Review-lens routing (design D10):
  - a `[data_safety]` table in `.agents/skills/skill-policy.toml`, holding path suffixes and content patterns, the machine form of the `data-safety` description;
  - `scripts/validate-agent-skills.py` fails when a pattern's keyword is missing from that description;
  - `scripts/agent-topology.py review-lenses --base <ref>` prints `{data, performance, modules_moved, paths}`. `performance` reuses `performance-scope`. `modules_moved` is an added, deleted or renamed `.rs` under a `src/` tree, read from `git diff --find-renames --name-status`.

  The tests in `scripts/test-agent-topology.py` and `test-validate-agent-skills.py` are written first and seen failing. They cover: a draft-service edit (data and performance), a `docs/` edit (neither), a `crates/gtk-lush/settle/` edit (performance only), a renamed module (`modules_moved`) and a content-pattern hit outside the path suffixes.
- [ ] 2.4 Re-check that `openspec validate <change> --strict` passes for a change with no `design.md`, on the CLI version in use. Stage 1 found it valid on 2026-11-18 (design D5). Use a temporary copy of this change in a scratch directory. If it fails, change D5 to require a short `design.md` in the fast lane.
- [ ] 2.5 `Makefile`:
  - `check-workflow-scripts` (in `check-policy`; put the prerequisite on its own line if `check-policy`'s list has been split by then);
  - `check-agent-helpers` runs `impl-slots.sh --self-test`, `gate-trees.sh --self-test` and `check-fast-lane.py --self-test`, and shellchecks the three shell scripts.

  Run `scripts/heavy.sh -- make check-agent-helpers`, `scripts/heavy.sh -- make check-workflow-scripts` and `scripts/heavy.sh -- make check-agent-skills`. The last one is not in `check-policy` (only CI's `ci.yml` runs it), and 2.3a edits its inputs.
- [ ] 2.6 Commit `chore(build): add the slot, gate-record, fast-lane, review-lens and workflow-script helpers`, and push.

## 3. Rules (one commit)

- [ ] 3.1 Edit `.agents/rules/parallel-agent-work.md`:
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
    - the criterion is unchanged until the calibration matches and the maintainer decides (design D7);
    - the one exception to the load precondition: the calibration's `calib-off-*` rounds, non-verdict and never quoted as verdicts (design D7);
    - non-inferiority changes as in D3;
    - the entry rule of D6: runnable rounds a change waits on, on a final tree, are queued as `Status: READY <sha>`, ones not runnable yet as `PENDING (NOT READY: <reason>)`, and non-blocking attribution and calibration entries as `PENDING`.
  - **"Merging":**
    - step 2: reuse on an identical tree via `scripts/gate-trees.sh` (design D1);
    - a new check before step 3: re-fetch, and require `origin/main` to be an ancestor of `HEAD`;
    - the CI exception now covers only correctness evidence, with budget runs as in D2;
    - an archive may follow in a second `MERGE-LOCK` hold when it owes Kani figures;
    - Kani figures land only in the archive commit, limited to the pending records' fields;
    - stage 3 dispatches on a twin ref `<branch>-kani-2` for a second concurrent run, deleted after the archive.
- [ ] 3.2 `.agents/rules/verification-runs.md`:
  - **"Timing campaigns":** the verdict class and the non-inferiority verdict (design D3);
  - **"Shard budgets":** dispatch when stage 3 ends, stage 4 reads, re-dispatch triggers (design D2).
- [ ] 3.3 `.agents/rules/build.md`, "Shard gates and budgets": one sentence pointing to `verification-runs.md` "Shard budgets" for when to dispatch. The gate's behaviour is unchanged.
- [ ] 3.4 `AGENTS.md` rules index: update the `parallel-agent-work.md` entry (running a change, fast lane, design revisions, slot cap) and the `verification-runs.md` entry (verdict classes, dispatch timing).
  - Add `make check-workflow-scripts` to "Build Commands" if `make check-policy`'s neighbours are listed there.
  - Check `README.md` for a list of policy targets and update it if one exists.
  - Run `scripts/heavy.sh -- make check-agent-docs`.
- [ ] 3.4a Reword the failure message in `scripts/sonar-local.sh` (design D11), and its header comment at line 6, which says the same: fix the issues, or record a suspected false positive in the change's `host-asks.md`; only the maintainer changes Sonar state. Grep `.agents/`, `AGENTS.md`, `docs/` and `scripts/` for the same instruction, and reword any hit.
- [ ] 3.5 Write the "Quality floor" self-check into the commit's handoff note `.claude/handoffs/adopt-faster-change-flow/floor-check.md`: for every edited rule paragraph, the existing rule it touches and why the floor holds, against design's "Quality floor" table.
- [ ] 3.6 Commit `docs(rules): adopt the faster change flow`, and push.

## 4. Versioned workflow script (one commit)

- [ ] 4.0 Run the stage agent's probes of design D11 first, under a manual main-checkout guard (status, `HEAD` and branch before and after each): `code-review` with a local-branch target and with an absolute worktree-path target, each over a scratch branch whose committed and uncommitted edits both hold a planted bug; and `/simplify` with a worktree directory argument, across several files, and on committed branch changes. Record each command, result and conclusion in `.claude/handoffs/adopt-faster-change-flow/probes.md`. A probe that edits the main checkout stops the work, and the coordinator is told. Then append a request to `.claude/handoffs/adopt-faster-change-flow/host-asks.md` for the two probes only the coordinator can run (a stage agent has no Agent or Workflow tool, and a probe PR is outside the outward-writes list): `agent(..., {isolation: 'worktree'})` and its cwd, and `code-review` on an open PR with a planted bug that differs from the main checkout.
- [ ] 4.1 Write `.claude/workflows/programme-change.js` from the three session scripts (`programme-change-v2.js`, `-revise.js`, `-continue.js`; paths in the stage brief), as in design D8:
  - modes `new`, `revise` and `continue`; `fast_lane`; `revision`, which runs the D4 decision and spike agents; `design_only`; `skip_impl`;
  - one shared stage-4 function;
  - stage 3 dispatches Kani budget runs (D2);
  - stage 4 reuses gate evidence on an identical tree and checks the ancestor (D1);
  - the slot helper (D6);
  - the non-inferiority branch of "ready, measurement pending" (D3);
  - the stages after implementation (D10), with `selectLenses` as a pure function mapping each flag to its own lenses (design D10 table) whose cases the script asserts at start-up, the `code-review` lens without a schema covering both the commits and `/simplify`'s edits in the form 4.0 chose (or the named two-call fallback), and `args.review_agent_type`;
  - the guard around every stage (D11): guard-read agents before and after every stage and around each parallel group, a script comparison and a stop, plus per-stage `TMPDIR` and brief files;
  - stage prompts that name `openspec-propose`/`openspec-update-change`, `openspec-apply-change` and `openspec-archive-change` instead of describing their procedure;
  - prompts cite rule sections and carry no copied rule text, path, account, key or trailer.
- [ ] 4.2 Record in the commit message which session-script behaviours were kept, changed or dropped. The archive-previous stage's hardcoded `docs/next/long-line-documents.md` becomes `args.programme_record`.
- [ ] 4.3 Run `scripts/heavy.sh -- make check-workflow-scripts`, then `git check-ignore -v .claude/workflows/programme-change.js`, which must print nothing.
- [ ] 4.4 Check that the template's `code-review` target form and `/simplify` fallback match `probes.md` (4.0). State in `probes.md` which two probes the coordinator still owes. The template is not used until they are recorded and the template matches them; if `isolation: 'worktree'` gives a worktree cwd, the coordinator's merge-only run may simplify the template accordingly.
- [ ] 4.5 Commit `chore(workflow): version the programme-change workflow`, and push.

## 5. Final gates and PR

- [ ] 5.1 Run these on the final tree through `scripts/gate-trees.sh record` (design D1):
  - `scripts/heavy.sh -- make check-policy`;
  - `scripts/heavy.sh -- make check-agent-skills` and `scripts/heavy.sh -- make check-agent-docs` (neither is in `check-policy`);
  - `openspec validate adopt-faster-change-flow --strict`;
  - `openspec validate --all` (errors gate).
- [ ] 5.2 Open the PR. Its body lists:
  - the local evidence;
  - the "Quality floor" table;
  - that in-flight runs keep their session scripts until their next stage boundary;
  - that decision 7 is recorded, not implemented.

  It ends with the required Claude Code line. Do not merge, and do not wait for CI.
- [ ] 5.3 **This change's stage 4 does not merge** (maintainer, coordinator note 2026-11-18 19:00): it reviews and fixes as usual, then ends with the verdict "ready, Fable template review pending". The coordinator runs the independent Fable review of `.claude/workflows/programme-change.js` and its two owed probes (4.4), then a merge-only run with their fixes.
