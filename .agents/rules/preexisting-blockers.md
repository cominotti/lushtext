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
- A flaky test is a blocker. Do not tolerate a test that "passes on retry" without investigating and fixing the root cause. Retries and generous timeouts exist to keep the pipeline moving and to surface flakes loudly, not to excuse leaving them unexplained. Investigate the real failure, fix the cause (adequate timeout budget, correct predicate, shared/non-duplicated wait helper, or the underlying production race), and prove the fix by rerunning *in isolation* to separate a real break from load. A load-amplified flake is still a real fragility — fix the cause, do not blame the machine. Do not change a working test helper's mechanism without proving the replacement against the real async delivery path.

## Examples

- A full test suite fails because of an old harness/threading issue: fix the harness, then run the suite again.
- A runtime warning appears in an untouched subsystem but blocks acceptance of the feature: fix the warning before calling the work done.
- A widget test "passes on retry" / prints `FLAKY:`: read the real panic, classify the wait (sync UI flip vs async `spawn_blocking_then` completion), give async/realization waits an adequate budget (or fix the predicate/race), de-duplicate any copy-pasted wait helper, and rerun in isolation to confirm. Do not leave it as accepted flakiness. See the `gtk-testing` skill's Flake Discipline section.
- An outdated rule or missing documentation caused the blocker to recur: update `AGENTS.md` and the relevant `.agents/rules/*.md` entry in the same work stream.
