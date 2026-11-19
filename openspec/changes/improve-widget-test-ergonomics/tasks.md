# Tasks

Tests: no test changes what it asserts. The full widget suite before and after, with identical selected counts and results, is the oracle, plus the body diff of design D2.
Kani: none (no proven module touched).
Timing: none (no timed path touched).
Start: only after every edit, spec and queued change of the programme coordinator's session has finished (maintainer, 2026-11-19), and after the maintainer places this change in `PARALLEL-LEDGER.md`. Task 0.1 re-measures the baseline first. This change also runs while no other change in the ledger has `crates/lushtext/tests/widget/window.rs` open; it is recommended first of the refactor-1 set.

## 0. Re-measure

- [ ] 0.1 On the then-current `origin/main`:
  - re-count `window.rs`'s lines and tests;
  - list every live file naming `window::test_` (design D3);
  - list the `test_*` helpers that are not tests;
  - read whether `is_relevant` classes `crates/lushtext/tests/widget/` as smoke-relevant.
  
  Update the artifacts in place. Verify with `openspec validate improve-widget-test-ergonomics --strict`.
- [ ] 0.2 Have a Fable subagent cross-review the re-measured artifacts (read-only, against `openspec/config.yaml`'s rules and the then-current `origin/main`), and apply its findings.

## 1. Registry without rewriting

- [ ] 1.1 Record the full widget `--list` on `main` in `evidence/list-before.txt`.
- [ ] 1.2 Rename the non-test `test_*` helpers. Convert every widget file to `pub(crate) fn test_<name>()` with no `#[test]` line, and change `build.rs` and `widget-shards.py`'s discovery to design D1's rule, updating the self-test. Verify that:
  - `--list` equals `evidence/list-before.txt`;
  - `make check-widget-shards` passes;
  - a planted compile error and a planted panic report the real file path.
- [ ] 1.3 Run the gate order, and the full `make test`. Commit through `heavy.sh`, signed, and push.

## 2. Split `window.rs`

- [ ] 2.1 Write the grouping and the old-to-new name mapping into `evidence/renames.md` (design D2).
- [ ] 2.2 Move the tests into the `window_<theme>.rs` files and the helpers into `window_support.rs`. Rename every live reference (design D3). Verify that:
  - the body diff shows no change;
  - `--list` equals the mapped `evidence/list-before.txt`;
  - `make check-widget-shards` passes with unchanged shard counts;
  - the grep for `window::test_` outside the archive is empty;
  - every renamed name in scripts and docs exists in `--list`.
- [ ] 2.3 Run the gate order, the full `make test` and, if task 0.1 found the path smoke-relevant, the three smokes. Commit through `heavy.sh`, signed, and push.

## 3. Quickstart and docs

- [ ] 3.1 Write `CONTRIBUTING.md` (design D4) and link it from the README's "Contributor Map". Update the registry description in `.agents/rules/testing.md`, the `gtk-testing` skill references and `crates/lushtext/AGENTS.md`, and the test counts the README states. Verify with `make check-agent-docs` and `make check-terminology`.
- [ ] 3.2 Commit through `heavy.sh`, signed, and push.

## Workflow follow-up

- Archive after review and merge under the programme's merge rules.
