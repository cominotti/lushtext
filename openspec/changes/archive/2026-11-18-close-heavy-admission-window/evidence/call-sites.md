# Call sites that admit more than 1G inside a command (task 4.1)

Audit of 2026-11-17 on the branch at `10fa0162`:
`grep -rn 'with-memory-cap\.sh\|MEMORY_CAP' Makefile scripts .githooks .github`,
plus `grep -rn 'heavy\.sh'` over the same trees (the advisor's addition: a
nested `heavy.sh` with a cap above 1G inside `heavy.sh --mem none` is now
refused too).

## Admitting recipes (each now starts with `with-memory-cap.sh --preflight`)

| Recipe | Inner admissions | Envelope |
| --- | --- | --- |
| `make test`, `make test-widget`, `make test-widget-headless` (`CARGO_TEST_WIDGET`, `CARGO_TEST_WIDGET_HEADLESS` with `WIDGET_PARALLEL` != 0) | `scripts/widget-shards.py parallel`: `WIDGET_BUILD_MEMORY_MAX` (16G) for the shared build, then one `WIDGET_SHARD_MEMORY_MAX` (2G) scope per shard | `--envelope 16G` |
| `make mutants-smoke`, `mutants-diff`, `mutants-full` | `MUTANTS_MEMORY_MAX` (36G) around `scripts/run-mutants.sh` | `--envelope <MUTANTS_MEMORY_MAX>` |
| `make kani` | `KANI_MEMORY_MAX` (16G) with `--address-space` around `scripts/kani-shards.py run` | `--envelope 16G` |
| `make proof-strength` | `PROOF_STRENGTH_MEMORY_MAX` (16G) around `scripts/proof-strength.py measure` | `--envelope 16G` |

With `WIDGET_PARALLEL=0` the widget lanes run `scripts/run-widget-tests.sh`
in one serial session and admit nothing, so they have no preflight. A lane
whose cap is `none` (`KANI_MEMORY_MAX=none`) also passes its preflight.

## Other callers

- `make check-agent-helpers` (in `make check-policy`, so in the pre-commit
  hook): the wrapper's and `heavy.sh`'s self-tests. They shed the enclosing
  helper's marker and use private slices, locks and admission locks only.
  This is the only admission above 1G on the hook path.
- `scripts/formal-evaluation.sh` calls `heavy.sh --mem 16G` once per check.
  `build.md` already says to run it directly, never inside one `heavy.sh`,
  so the new refusal does not reach it.
- `scripts/ci-image-widget-test.sh` names `heavy.sh -- cargo test … --no-run`
  in a hint, and is itself run as `heavy.sh --mem none` (it admits nothing).
- `scripts/measure.sh`, `scripts/next-rustc-wrapper.py`,
  `scripts/proof-strength.py`, `scripts/check-session-hygiene.py`: comments,
  messages or test fixtures only.
- `.github/` and `.githooks/`: no direct call of either script (the hook runs
  `make pre-commit` or `make pre-commit-docs`).
