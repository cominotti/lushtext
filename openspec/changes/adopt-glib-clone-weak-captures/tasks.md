# Tasks

Tests: no behaviour change. The widget suite is the oracle, plus one fallback-value test each for `Propagation` and `ControlFlow` written before their sites change.
Kani: none (no proven module touched).
Timing: none (no timed path touched).
Start: only after every edit, spec and queued change of the programme coordinator's session has finished (maintainer, 2026-11-19), and after the maintainer places this change in `PARALLEL-LEDGER.md`. Task 0.1 re-measures the baseline first. This change also starts after `adopt-typed-row-state-and-dialog-futures` merges.

## 0. Re-measure

- [ ] 0.1 Re-derive the site classification in `proposal.md` on the then-current `origin/main`. Record it, with the file list, in the change's handoff. Verify with `openspec validate adopt-glib-clone-weak-captures --strict`.
- [ ] 0.2 Have a Fable subagent cross-review the re-measured artifacts (read-only, against `openspec/config.yaml`'s rules and the then-current `origin/main`), and apply its findings.

## 1. Probe

- [ ] 1.1 Convert `ui/window/imp.rs:955-1040` and run design D1's checks, each with its planted case. Verify by recording each check's red and green outcome. Commit through `heavy.sh`, signed, and push after the three smokes.

## 2. Ratchet and rule

- [ ] 2.1 Add the audit (design D3) at the post-probe count, with its self-test, and wire it into `check-policy` (and `check-policy-ci` if `close-audited-gate-gaps` has merged). Change `.agents/rules/rust.md` ("Modern Rust Idioms") to prescribe `clone!`. Verify with `make check-policy` and `make check-agent-docs`. Commit and push.

## 3. Sweep

- [ ] 3.1 Write the `Propagation` and `ControlFlow` fallback tests for one site each. Verify that they pass on the manual form.
- [ ] 3.2 Convert `ui/` by directory (window, editor_page, sidebar, search_panel, the rest) under design D2. After each directory, lower the ceiling, run the shard set for that directory and `make check-clippy`, and commit; batch the three smokes per two or three directories. Verify that the ceiling ends at the count of D2's listed exceptions.

## Workflow follow-up

- Archive after review and merge under the programme's merge rules.
