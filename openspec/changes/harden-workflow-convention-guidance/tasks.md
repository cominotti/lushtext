## 1. Guidance loading

- [x] 1.1 Repoint `.claude/CLAUDE.md` at `../AGENTS.md` and verify with `test -e .claude/CLAUDE.md`
- [x] 1.2 Convert `build.md`, `rust.md`, `ui.md` `globs:` → equivalent `paths:` YAML lists
- [x] 1.3 Make `documentation.md`, `git.md`, `preexisting-blockers.md` explicitly global (no `paths:`, plus the always-load marker comment)
- [x] 1.4 Add frontmatter with the three-glob `paths:` list to `widget-wiring.md`
- [x] 1.5 Extend `scripts/validate-agent-skills.py` with the rule-frontmatter check (valid `paths:` list or explicit global marker; no `globs:` key), wired into `check-agent-docs.sh`
- [x] 1.6 Add self-test cases to `scripts/test-validate-agent-skills.py` for: missing `paths:` and no marker, malformed `paths:`, residual `globs:`, and a conforming file
- [x] 1.7 Prove the new check with a deliberate red (temporarily reintroduce a `globs:` key), then restore

## 2. The normative rule

- [x] 2.1 Write `.agents/rules/workflow-convention.md` (`paths:`-scoped; the design's ≤170-line target was not met — the assembled rule measured **248** lines, and the review pass's restored normative statements took it to **263**; recorded rather than met, since the alternative was dropping normative sentences to hit a number), assembled from `rust.md` 116–328, `widget-wiring.md` 279–392, `AGENTS.md` 146–201, `build.md` 243–265, war stories reduced to at most a one-clause "why"
- [x] 2.2 Replace `rust.md`'s `## Workflow Vocabulary And Boundaries` (through `### Intent-first naming`) with a 3–5 line pointer; keep `## Coordination Vocabulary` intact
- [x] 2.3 Replace `widget-wiring.md`'s evidence-surface block with a 3–5 line pointer, keeping the surrounding Testing content intact
- [x] 2.4 Trim `AGENTS.md`'s `### Workflow Role Convention` to the role table plus a pointer
- [x] 2.5 Trim `build.md`'s `check-workflow-boundaries` paragraph to the gate's operational description plus a pointer
- [x] 2.6 Add the `workflow-convention.md` line to `AGENTS.md`'s Rules Index

## 3. Post-closure freeze

- [x] 3.1 `documentation.md:37` — "advance the matching slot … slot ledger" → "update the row; the programme record is frozen except its deferral inventory, which may be appended"
- [x] 3.2 `rust.md` ~132–136 — freeze wording for the programme record
- [x] 3.3 `build.md` ~257 — freeze wording for the ledger-reconciliation clause
- [x] 3.4 `AGENTS.md` ~189/196–201 — freeze wording; drop "migration slot" as a live cell
- [x] 3.5 `widget-wiring.md` ~390 and `.agents/skills/gtk-testing/SKILL.md` ~146–148 — "unmigrated workflows keep `*_for_test`" → every workflow is terminal; new state goes on the evidence surface
- [x] 3.6 Add a 2026-09-14 programme-closure line to `AGENTS.md` "Recent Changes"
- [x] 3.7 Grep rules/skills/`AGENTS.md` for remaining stale "slot" / "unmigrated" / "pending migration" phrasing and fix what is stale, leaving the programme record and matrix history alone

## 4. The skill

- [x] 4.1 Create `.agents/skills/lushtext-workflow/SKILL.md` — the procedural checklist (row, home, `policy.rs` first + `make mutants-list` proof, seams, coordination names, presentation surfaces, evidence + three proofs, facade narration, re-derived cells, gates)
- [x] 4.2 Create `references/evidence-proofs.md` — the three proofs as recipes, naming the two reference tests
- [x] 4.3 Create `references/gate-order.md` — `git add -N` first, fmt/check/cargo check/rustdoc/boundaries/mutants-diff, smoke-fingerprint hazards and the ship-time re-run
- [x] 4.4 Create `agents/openai.yaml` (initially `allow_implicit_invocation: false`; flipped to `true` by 8.8 after review)
- [x] 4.5 Register `lushtext-workflow` under `[implicit_invocation]` in `.agents/skills/skill-policy.toml` (not in `filesystem_contract`); 8.8 sets it to `true`
- [x] 4.6 Run `make check-agent-skills`

## 5. Gate: undeclared modules in a role home

- [x] 5.1 Implement the role-home derivation and the undeclared-module finding in `scripts/check-workflow-boundaries.py` (facade directory plus row-named subdirectories, non-recursive; `mod`/`imp`/fixed role/bounded coordination names exempt; declaration = backticked repository path in the row's cells or its roles subsection)
- [x] 5.2 Run it against the current matrix and record the real hits before fixing anything
- [x] 5.3 Add positive and negative self-test fixtures, and prove a deliberate red
- [x] 5.4 Declare each hit in its row's `Migrated Workflow Roles` subsection, classified from the module's own doc (called presentation surface vs non-role module)
- [x] 5.5 Re-run the gate and confirm zero findings

## 6. Gate: `*_for_test` declaration ratchet

- [x] 6.1 Recompute the count with the programme record's predicate and record it in the matrix's `Measurement Definitions` section
- [x] 6.2 Implement the ratchet in `scripts/check-workflow-boundaries.py` (fail only on excess; message names both remedies in order)
- [x] 6.3 Self-test both directions: over-count fails, under-count passes
- [x] 6.4 Confirm `cfg(feature = "test-utils")` sites are not ratcheted
- [x] 6.5 Make the ratchet fixture mixed-visibility (`pub` + `pub(crate)` counted, `pub(super)` not) and assert the counted total inside the fixture, then prove it with a deliberate red: narrowing `FOR_TEST_DECLARATION_RE` to `pub\s+fn` leaves the pre-review all-`pub fn` fixture **green** and fails the new one

## 7. Records and verification

- [x] 7.1 Append one entry to `docs/next/workflow-readability.md`'s deferral/closure inventory noting these guidance hardenings, without rewriting history
- [x] 7.2 `openspec validate harden-workflow-convention-guidance --strict`
- [x] 7.3 `./scripts/check-workflow-boundaries.py --self-test` and `make check-workflow-boundaries`
- [x] 7.4 `make check-agent-skills` and `make check-agent-docs`
- [x] 7.5 `make check-policy`
- [x] 7.6 `test -e .claude/CLAUDE.md`

## 8. Review pass

- [x] 8.1 Widen the two `paths:` lists the review found too narrow: `build.md` gains `crates/**/*.rs` (fmt / rustdoc / `git add -N` / runtime-warning guidance applies to any Rust edit) and `widget-wiring.md` gains `crates/lushtext-core/src/services/action_catalog/**`; record the deliberate-narrowing rationale as design.md's "Loading scope" note
- [x] 8.2 Restore the normative statements condensation dropped: pre-convention siblings are **classified**, not chosen between topical and role decomposition (rule + skill); the `journal` role's operational definition; the still-parented-dispose half of the disposal invariant; and the gain-from-zero vs parity-claim distinction for a rename into the mutation scope
- [x] 8.3 Stop hardcoding the facade budget figure in the rule; name the matrix's "Facade size budget" section instead
- [x] 8.4 Re-key the three ratchet pointers onto `### Externally reachable test-seam ceiling` rather than its parent `## Measurement Definitions` heading
- [x] 8.5 State what the role-home check reaches (facade directory plus row-named subdirectories, non-recursive) and that moving a module up out of a home is a classification change, not a way to clear a finding
- [x] 8.6 Add the two remaining rule-scope findings to `scripts/validate-agent-skills.py` — `paths:` plus the global marker is scope stated twice, and a `paths:` glob matching no tracked file is dead scope — with self-tests in both directions and a deliberate red for each
- [x] 8.7 Fix `documentation.md` item 8, which named the frozen `docs/next/workflow-readability.md` as the example of a record whose posture must be updated, contradicting its own trigger list twenty lines later
- [x] 8.8 Make `lushtext-workflow` implicitly invocable (`skill-policy.toml` and `agents/openai.yaml`), since the skill now solely owns the demoted procedural obligations
- [x] 8.9 Repoint the matrix's `.agents/rules/rust.md` / `widget-wiring.md` citations for the cross-cutting criterion, the seam rule, the argument-count exemption, and the one-`test_policy.rs` rule at `workflow-convention.md`, and give `rust.md` and `widget-wiring.md` an explicit "the criteria formerly stated here now live there" sentence so any older citation resolves in one hop
- [x] 8.10 README: the programme is "closed", matching the rule and the matrix, not "complete"

**Follow-up for the next change that edits these files.** Three modules under
`crates/lushtext-core/src/ui/` carry the same stale rule citations 8.9 repoints
(`.agents/rules/rust.md` / `.agents/rules/widget-wiring.md` for criteria that now
live in `workflow-convention.md`). They are deliberately **not** edited here,
because a single byte under `crates/lushtext-core/src/ui/` voids the
accessibility, visual, and visual-geometry summaries, which this docs-and-gates
change cannot re-earn. 8.9's one-hop pointer sentences keep them resolving in the
meantime. Repoint them in the next change that already touches `ui/` and is
re-running those lanes:

- `crates/lushtext-core/src/ui/sidebar/test_policy.rs`
- `crates/lushtext-core/src/ui/window/editor_focus.rs`
- `crates/lushtext-core/src/ui/window/dialogs.rs`
