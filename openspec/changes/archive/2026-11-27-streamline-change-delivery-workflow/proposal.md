# Proposal: Streamline Change Delivery Workflow

## Why

Measured execution across five sequential OpenSpec changes (`complete-hexagonal-crate-organization`, `domain-value-types-and-primitive-boundaries`, `typed-service-error-boundaries`, `idiomatic-rust-state-machines-and-c-ism-removal`, and the first 57 min of `cold-path-ergonomics-and-shared-metadata`) totaled **~20.75 h (1,245 min)**, of which only **~363 min (29.2%)** was direct implementation, model reasoning, tests, and 3-lens code review:

| Category | Measured time | Share | Evidence / detail |
| --- | ---: | ---: | --- |
| Local UI smokes (`accessibility-smoke`, `visual-smoke`, `visual-geometry-smoke`) | ~449 min (~7.5 h) | 36.1% | 26× accessibility, 26× visual, 28× visual-geometry under Mutter |
| GitHub CI waits (main PRs + archive PRs) | ~202 min (~3.4 h) | 16.2% | ~106 min on PRs #134/#136/#138/#140 + ~96 min waiting on 4 docs-only archive PRs |
| Implementation (workers, edits, patches, shell) | ~181 min (~3.0 h) | 14.5% | Direct code/edit work |
| Model reasoning & planning | ~113 min (~1.9 h) | 9.1% | Inference between tool calls |
| Local static gates (`git commit -S`, clippy, policy, doc, Kani) | ~91 min (~1.5 h) | 7.3% | ~42 min in `git commit -S` hooks + ~34 min clippy/policy/doc + ~15 min Kani |
| Benchmarks / timing (`measure.sh`) | ~90 min (~1.5 h) | 7.3% | Baseline vs branch comparisons |
| Tests (unit, integration, prop, widget headless) | ~69 min (~1.1 h) | 5.5% | ~55 min non-widget + ~14 min direct widget tests |
| Archive subagents (beyond CI wait) | ~32 min (~0.5 h) | 2.6% | Spec sync, archive, docs, PR creation (128 min total incl. 75% CI wait) |
| `/swarm` 3-lenses code review | ~17 min (~0.3 h) | 1.4% | Highest ROI step in the pipeline |

Three structural bottlenecks account for ~60% of the total wall time:

1. **Intermediate UI commits re-triggering Mutter smokes (~36.1%)**: `openspec/config.yaml` and `verification-runs.md` require committing per task group, while `.githooks/pre-commit` enforces `check-accessibility-policy --strict-current-tree` and `check-visual-proof-policy`, forcing a full trio of smokes on every intermediate commit touching `crates/lushtext-ui/src/ui/` or `crates/lushtext/tests/widget/`. Meanwhile, 2026-11-18 Decision 7 (`crates/lushtext/tests/widget/` exclusion), Intervention 2 (parallel smoke runs), and Intervention 4 (`make check` tree stamp) recorded in `docs/memorials/2026-11-18-change-flow-time.md` remain unimplemented, and `check-accessibility-policy.py` still carries recorded hole S7(b) (clean committed UI trees pass without smoke proof).
2. **Separate docs-only archive PRs + slow CI waits (~16.2%)**: *Merge on local evidence* was scoped solely to `.claude/workflows/programme-change.js`, forcing `/goal` and `/swarm` sessions to wait for GitHub CI on every PR and separate archive PR, while CI workflows run full Fedora 45 containers even on docs-only PRs.
3. **Late reviews and CI-only Python/rustdoc catches**: Reviewing after UI commits already paid for smokes multiplies smoke reruns on review fixes, while unautomated `rustdoc` lint and Python regex hygiene (`python:S5855` on PRs #136/#140) trigger ~15-minute CI re-runs.

## What Changes

- **Decouple commit checkpoints from branch-level smoke freshness (and close S7(b))**:
  - `.githooks/pre-commit` keeps all static checks (`make pre-commit` / `make pre-commit-docs`), skipping redundant Clippy+policy via a clean-worktree `git write-tree` stamp (`scripts/precommit-scope.py stamp-write` / `stamp-valid`, 30-minute TTL, refusing dirty/untracked/`MERGE_HEAD` on both write and read) when `make check` already passed on that exact tree.
  - `make check-accessibility-policy` runs `--static-only` and `make check-visual-proof-policy` runs `--self-test` (Python + Rust parity) inside `make check-policy`. Live smoke freshness moves to `make check-smoke-proof` (enforced by `.githooks/pre-push`, `make verify-local`, and `gate-trees.sh`) over `origin/main...HEAD` (unioning committed branch changes and dirty working-tree changes in `scripts/check-visual-proof-policy.py` and `crates/cargo-gtk-proof/src/policy.rs`, plus `scripts/accessibility_source_fingerprint.py --base-ref`), closing S7(b).
  - `.claude/workflows/programme-change.js` and `programme-change.test.mjs` are updated minimally so `smokeCheck` stops relying on the removed `pre-commit` `"hook"` shortcut and always requires `gate-trees.sh` smoke records whenever `origin/main...HEAD` touches smoke-relevant files (keeping `programme-change.js`'s post-merge archive path and all existing cited rule headings intact).
  - Apply 2026-11-18 Decision 7: exclude `crates/lushtext/tests/widget/` from the three smoke fingerprint sets while keeping static accessibility policy checks on widget test files.
- **Single parallel smoke & local CI-parity commands**:
  - Add `make smoke-ui` (builds binaries once, runs `accessibility-smoke`, `visual-smoke`, and `visual-geometry-smoke` in parallel when `SMOKE_PARALLEL=1`, optionally recording each lane in `gate-trees.sh` when `GATE_TREES_CHANGE` is set).
  - Add `make verify-local` and `make verify-local-fast` (local CI parity including `rustdoc` lints, `cargo check -p lushtext --bins --locked`, `cargo bench --no-run`, `cargo deny`, `make test`, `make test-prop`, `make fuzz-corpus-replay`, conditional GTK Lush checks, and `make check-smoke-proof`).
  - Add `scripts/check-python-hygiene.py` (`make check-python-hygiene` in `check-policy` and CI `lint`) to catch Python syntax, bare `except:`, duplicate dict keys, and redundant regex alternatives locally.
- **CI docs-only fast path without losing required checks**:
  - Add `docs-only-range` to `scripts/precommit-scope.py` and wire a `scope` job into `.github/workflows/{ci,kani,flatpak,snap,sonarqube}.yml`. Docs-only PRs skip heavy builds/runners while keeping all 5 GitHub ruleset `23911547` required contexts (`GTK Axioms`, `GTK Axioms (GNOME 50 floor)`, and the 3 PR `Kani Proof Harnesses (...)` jobs) reporting green `Success`.
- **Standardize the streamlined delivery workflow**:
  - Codify *Review-Before-Smoke* and *Archive-Before-Final-Gate* for standalone `/goal` and agent workflows (non-UI foundation -> UI edits without intermediate smokes -> `/swarm 3-lenses` review + fixes -> commit code -> In-PR `openspec sync + archive` commit when no Kani CI figure is owed -> single `make smoke-ui` + `make verify-local` recorded under canonical `GATE_NAMES` on the final post-archive `HEAD^{tree}` -> Merge on local evidence under `MERGE-LOCK` -> async CI/Sonar audit) across `.agents/rules/{parallel-agent-work,verification-runs,testing,build}.md`, `openspec/config.yaml`, `AGENTS.md`, `README.md`, `docs/end-user-coverage.md`, `docs/accessibility.md`, and `.agents/skills/change-delivery/SKILL.md`.
  - Add a narrow zero-cost structural refactor `Timing: none` exemption in `verification-runs.md` confirmed by both spec review and final/code review.

## Capabilities

### New Capabilities

_(None — all requirements belong to existing workflow, accessibility-policy, and visual-proof capabilities.)_

### Modified Capabilities

- `change-delivery-flow`: Extend merge-on-local-evidence and Archive-Before-Final-Gate in-PR archive to authorized agent workflows, update `smokeCheck` for pre-push smoke enforcement, add tree-stamped pre-commit and local CI-parity/Python-hygiene gates, support docs-only CI fast path preserving required checks, and document zero-cost structural `Timing: none` exemptions.
- `gtk-accessibility-spine`: Exclude `crates/lushtext/tests/widget/` from smoke source fingerprints while preserving static accessibility policy checks, and close S7(b) on committed branch diffs via `--base-ref`.
- `visual-geometry-invariants`: Exclude `crates/lushtext/tests/widget/` from visual-sensitive smoke triggers in parity across Python and Rust, and make visual policy digests invariant across commits when `--base-ref` is active.

## Non-Goals

- No change to shipped application behavior (`crates/lushtext-model`, `crates/lushtext-services`, `crates/lushtext-ui`, `crates/lushtext`).
- No removal or weakening of any test lane, Kani shard, GTK axiom probe, visual/accessibility smoke scenario, or assertion.
- No disabling or bypassing of GitHub Ruleset `23911547` required status checks.
- No change to `.claude/workflows/programme-change.js`'s stage graph, post-merge archive flow, or cited rule section headings.

## User-Visible Impact

None. LushText users observe zero behavioral, UI, packaging, or performance changes compared with `main`. Contributors and autonomous agents gain ~50–55% faster change delivery through fewer redundant UI smoke and CI cycles while closing committed-tree smoke proof coverage (S7(b)) and adding local `rustdoc` + Python hygiene parity.
