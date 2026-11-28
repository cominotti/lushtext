---
name: change-delivery
description: Standard 6-phase OpenSpec change delivery workflow (Review-Before-Smoke, Archive-Before-Final-Gate, single final smoke-ui + verify-local pass, and merge on local gate-trees.sh evidence). Use when planning, implementing, reviewing, archiving, or merging an OpenSpec change or /goal stream.
---

# Change Delivery Workflow

Deliver an OpenSpec change end to end without redundant Mutter smoke cycles or separate archive PR waits, while keeping every quality gate intact.

## When to Apply

Use this workflow for any OpenSpec change (`/goal`, standalone agent run, or interactive session) once requirements are approved. For the automated multi-change ledger script, see `.claude/workflows/programme-change.js`.

## The 6 Phases

1. **Non-UI foundation first**: Commit `model/`, `services/`, build/tooling, and test infrastructure cleanly per deliverable phase. `.githooks/pre-commit` enforces static checks only (`make check-fmt`, all-feature Clippy, `make check-policy`) and reuses the 30-minute clean-worktree `git write-tree` stamp written by `make check`.
2. **Batch UI edits without intermediate smokes**: Keep all `crates/lushtext-ui/src/ui/`, `resources/ui/`, and `resources/style/` changes in as few commits as possible without running UI smokes between intermediate commits. `/swarm` workers should return patches or commit statically without Mutter churn. Run `make verify-local-fast` (`make check` + default-feature `cargo check -p lushtext --bins --locked` + strict `rustdoc` + `cargo bench --no-run`) while iterating.
3. **Review-Before-Smoke**: Run a parallel 3-lens independent code review (`/swarm` or independent subagents — including `data-safety`, `gtk-perf-review`, and architecture/rules checks as triggered by touched paths), plus `/simplify` when useful, and apply **all** confirmed fixes **before** running the UI smoke trio.
   - If the change declared a zero-cost structural `Timing: none (...)` exemption in `design.md` (already checked by independent spec review), the code review must explicitly confirm no new allocation, `.clone()`, lock, or dispatch change was introduced.
4. **Commit code & Archive-Before-Final-Gate**: Commit final reviewed code locally, check all `tasks.md` boxes, and — unless Kani CI shard figures are pending for `scripts/kani-shards.py` — sync delta specs into `openspec/specs/` and archive the change (`openspec-archive-change`) as a docs-only commit directly on the PR branch *before* the final gate pass.
5. **One Final Gate Pass on Exact `HEAD^{tree}`**: On the clean final post-archive `HEAD^{tree}`, run ONE single non-redundant `verify-local` invocation via `scripts/heavy.sh`:
   - If the branch touches smoke-relevant paths (`origin/main...HEAD`):
     `./scripts/heavy.sh --envelope 16G -- make -C <wt> verify-local VERIFY_RUN_SMOKES=1 GATE_TREES_CHANGE=<change>`
   - Otherwise (non-smoke branch):
     `./scripts/heavy.sh --envelope 16G -- make -C <wt> verify-local GATE_TREES_CHANGE=<change>`
   - Setting `GATE_TREES_CHANGE=<change>` records `'make check'`, `'make test'`, and (when `VERIFY_RUN_SMOKES=1`) `'make accessibility-smoke'`, `'make visual-smoke'`, and `'make visual-geometry-smoke'` once each without duplicate execution, and verifies default-feature `cargo check -p lushtext --bins --locked`, strict `rustdoc`, `cargo bench --no-run`, `cargo deny`, unit/integration/property tests, fuzz corpus replay, and `make check-smoke-proof`. (Because visual digests union `origin/main...HEAD` with working-tree status and `crates/lushtext/tests/widget/` is excluded from smoke fingerprints, digests survive `git commit`.)
6. **Push & Merge on Local Evidence**: Push explicitly (`git push -u origin <branch>`), open/update the PR, and merge under `$H/MERGE-LOCK` using `scripts/gate-trees.sh` local evidence (`parallel-agent-work.md`, "Merging").
   - Watch live PR CI runs (`timeout 540 gh run watch <id> --exit-status`, never holding `$H/MERGE-LOCK`) *only* when the change alters `.github/workflows/**` or other CI-only correctness evidence; otherwise audit CI/Sonar asynchronously after merge and fix any red finding in a follow-up stream (`preexisting-blockers.md`).

## Guardrails

- Never run bare `git commit --no-verify` or bypass `scripts/heavy.sh` for heavy Cargo/smoke steps.
- Never hold `$H/MERGE-LOCK` while waiting on CI runs or long tests.
- Never write code in the main checkout while working in a dedicated worktree.
