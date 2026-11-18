# Gates on the final tree (task 6.1)

Tree: `93f08ecc` for the helper, `make check` and OpenSpec rows, and
`bea805bb` (one docs-only commit later) for `make test`. Branch
`close-heavy-admission-window`, merge base `c16d6fdc`; `origin/main` has not
moved. Every lane below was launched
with the **worktree's** `scripts/heavy.sh` (groups 4 and later; groups 1-3
used the main checkout's copy, as tasks.md required).

| Gate | Command | Result | Load before → after |
| --- | --- | --- | --- |
| Agent helpers | `scripts/heavy.sh --mem none -- make check-agent-helpers` | exit 0; `with-memory-cap`, `heavy`, `measure` and `wait-for` self-tests ok, CI image lane self-test and shellcheck clean | 7.18 → 4.77 |
| `make check` | `scripts/heavy.sh -- make check` | exit 0 (rustfmt, all-feature Clippy, `make check-policy`) | 5.02 → 8.24 |
| OpenSpec | `openspec validate close-heavy-admission-window --strict` | `Change 'close-heavy-admission-window' is valid` | - |
| `make test` | `scripts/heavy.sh --envelope 16G -- make test` (lane case 4.3, run 3, on `bea805bb`) | exit 0; 2546 non-widget tests passed (12 skipped); 1696 widget tests in 3 shards, 1 attempt each; 0 FLAKY; no envelope unit left | 7.64 → 7.55 |
| Preflight off a proven LOCK | `heavy.sh -- with-memory-cap.sh --preflight 16G` (inside the default 12G scope); `heavy.sh --mem none -- with-memory-cap.sh --preflight none` | both exit 0 | - |
| CI path (no user manager) | `env -i PATH HOME CI=true GITHUB_ACTIONS=true` on `--preflight 16G`, `--preflight none`, `--address-space 16G …` and `16G true` | branch: all exit 0, the capped calls `running uncapped` exactly as on `c16d6fdc` (main has no `--preflight`). So `make kani` in `kani.yml` is unaffected | - |
| Commits | `scripts/heavy.sh --mem none -- git commit -F …` | `06eeb0c2` ran the full hook (rustfmt, Clippy, check-policy with 10 self-test lines); `93f08ecc` took the docs-only path | - |

Not run, as the spec records: Kani (no proven module touched), GTK axioms (no
toolkit change), rustdoc gate (no Rust doc comment changed), visual and
accessibility smokes (no UI-prefix file changed), timing campaigns (Timing:
none).


## Observed, not caused by this change

`openspec validate --all --strict` (OpenSpec CLI 2.0.0) reports 87 of 136
items failing, every one a WARNING the `--strict` flag promotes (long
requirement text, placeholder Purpose sections) in main specs under
`openspec/specs/`, none of which this branch edits; there is no ERROR. No
Makefile target, hook or CI workflow runs `--all`. Recorded for the
coordinator as its own ledger change (see the handoff's `host-asks.md`).
