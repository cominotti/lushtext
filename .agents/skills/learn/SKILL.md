---
name: learn
description: Automatically perform a final repository-learning review after completing architecture changes, refactors, features, or bug fixes. Identify durable guidance, hook, rule, and skill updates; detect stale or contradictory agent instructions; and produce a concrete candidate report. Keep automatic use read-only unless the current task explicitly authorizes repository-guidance edits, and never write Codex memory unless the user explicitly requests a memory update.
---

# Review durable learnings

Run this workflow after implementation and validation, before the final handoff. Automatic
invocation is a review gate, not blanket permission to expand the change.

## Inspect

1. Read `SOUL.md`, root `AGENTS.md`, and nested `AGENTS.md` files that govern the changed paths.
2. Read the relevant files under `.agents/rules/` and `.agents/skills/` plus repository hooks or validation scripts directly affected by the completed work.
3. Inspect the final task-owned diff and test evidence. Extract only lessons that are durable, non-obvious, repository-specific, and likely to prevent repeated mistakes.
4. Check each candidate against existing guidance for duplication, contradiction, obsolete paths/commands, excessive specificity, and the correct owning document.
5. Run `make check-agent-docs` when guidance was changed or when the completed work could expose stale guidance.

Define scope before reviewing:

- For a branch or PR, use its explicit base/head range.
- For local work, combine staged, unstaged, and untracked paths with `git diff --cached --name-only`, `git diff --name-only`, and `git ls-files --others --exclude-standard`.
- Record the task-owned path set as implementation work proceeds, then intersect it with the Git-derived set. Existing unrelated worktree changes are context only and must not become learning candidates or edits.
- If no reliable task-owned record exists in a mixed worktree and ownership cannot be proven from the current request plus exact diffs, report the ambiguous paths and remain read-only; never guess ownership.

Do not broadly scan unrelated documentation. Follow Markdown links from the governing files and
task-owned changed surfaces, then stop when the candidate set is resolved.

## Default output: candidate report

When repository-guidance edits were not authorized, remain read-only and report:

- **Candidate**: the durable lesson in one sentence.
- **Evidence**: changed file, invariant, failure, or test that supports it.
- **Owner**: the exact existing `AGENTS.md`, rule, skill, hook, or check that should own it.
- **Decision**: add, update, remove, or no change.
- **Reason**: why the candidate is durable and not duplicate noise.

Explicitly say when no guidance update is warranted. Do not create churn merely to prove the
workflow ran.

## Mutation boundary

- Update repository guidance only when the user explicitly authorized guidance edits in the current task. Automatic invocation is always read-only; “necessary,” “helpful,” or adjacent implementation work is not implicit authorization.
- Keep updates minimal, local to the owning file, and synchronized with the root rules index or nested-AGENTS index when those contracts require it.
- Do not edit hooks, rules, skills, or `AGENTS.md` based only on speculative future use.
- Do not independently broaden the review into a Codex-memory scan. Follow the platform's memory-read policy when prior project context is relevant, but treat the current checkout and test evidence as authoritative. Only write memory after an explicit user request, using the platform's memory-update contract.
- Do not treat conversation history or an agent's recollection as stronger evidence than the current checkout and test output.

## Acceptance criteria

- Every proposed learning has current-tree evidence and one clear owner.
- No proposal duplicates or contradicts existing guidance.
- Paths and commands exist and validation succeeds, or the report names the exact blocker.
- Automatic invocation causes no persistent repository mutation unless the task already authorized it. Run Python validation with `-B` or `PYTHONDONTWRITEBYTECODE=1`, and keep temporary output outside tracked guidance paths.
- Memory remains untouched unless the user explicitly requested a memory update.
