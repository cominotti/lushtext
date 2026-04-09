---
description: Mandatory rule for handling pre-existing blockers and broken verification paths
globs: *
---

# Pre-existing Blockers Rule

## Critical Rule

If implementation or verification reveals a pre-existing blocker, fix it in the same work stream instead of deferring around it, documenting it as acceptable debt, or treating it as out of scope.

This rule is mandatory and has no exceptions.

## Required Behavior

- Do not close work while known failing checks, broken test harnesses, or reproducible runtime warnings remain.
- Do not justify leaving a blocker unfixed by saying it was already present before the current change.
- If a pre-existing problem prevents verification, the blocker itself becomes part of the task and must be resolved before sign-off.
- Update documentation, rules, and test infrastructure in the same change set when that is required to eliminate the blocker permanently.

## Examples

- A full test suite fails because of an old harness/threading issue: fix the harness, then run the suite again.
- A runtime warning appears in an untouched subsystem but blocks acceptance of the feature: fix the warning before calling the work done.
- An outdated rule or missing documentation caused the blocker to recur: update `.claude/CLAUDE.md` and the relevant `.claude/rules/*.md` entry in the same work stream.
