# Delta for change-delivery-flow

## MODIFIED Requirements

### Requirement: The apply tree's records are checked before the merge stage
Before the merge stage starts, the script SHALL look up `make check` and `make test` on the apply tree, each in its own lookup. Because `pre-commit` enforces only static checks while live smokes run at `pre-push` and `make verify-local`, it SHALL also look up the three smokes whenever `origin/main...HEAD` touches a smoke-relevant file (`is_relevant` of `scripts/accessibility_source_fingerprint.py`). A no-op merge SHALL rerun only a gate without a pass.

#### Scenario: An apply tree without a make test record stops before the merge stage
- **WHEN** the apply stage's tree has a `make check` pass on record and no `make test` pass, and `origin/main` has not moved
- **THEN** the script's apply verification stops the run before the merge stage starts, and no merge prompt tells the agent to rerun nothing without a lookup

#### Scenario: A merge that brought smoke-relevant files in owes smoke records
- **WHEN** a merge in the branch brought a smoke-relevant file in
- **THEN** `smokeCheck` returns `records`, the apply stage records the three smokes on its committed tree, and after a merge that made no commit the post-merge check owes the same three records

#### Scenario: A non-merge UI commit owes smoke records
- **WHEN** a non-merge branch commit touches a file under `crates/lushtext-ui/src/ui/`
- **THEN** `smokeCheck` returns `records` (never `hook`) and `apply-verify` requires passing `gate-trees.sh` records for all three smokes on the apply tree

## ADDED Requirements

### Requirement: Authorized agent workflows merge on local evidence and archive in-PR
Authorized `/goal` and agent workflows MAY merge under `MERGE-LOCK` on local `gate-trees.sh` passes recorded under exact canonical `GATE_NAMES` on the final `HEAD^{tree}`, checking CI asynchronously afterwards unless `.github/workflows/**` changed. Standalone workflows without pending Kani CI figures SHALL sync specs and archive in a docs-only commit on the PR branch before that final `gate-trees.sh` pass, while `programme-change.js` keeps post-merge archiving.

#### Scenario: In-PR archive merges on exact final-tree records without a second pull request
- **WHEN** a standalone `/goal` change has no pending Kani CI figures, commits `openspec sync` and archive on the PR branch, and then records `make check`, `make test`, and required smoke passes on that post-archive `HEAD^{tree}`
- **THEN** the change merges under `MERGE-LOCK` without opening a separate archive pull request

#### Scenario: Pending Kani figures still defer the archive
- **WHEN** a change adds or updates a Kani shard whose CI budget runs are still in progress
- **THEN** the PR merges without the archive commit and the archive follows under a second `MERGE-LOCK` hold once Kani finishes

### Requirement: Pre-commit caches clean checks by tree hash while pre-push enforces smoke proofs
`.githooks/pre-commit` SHALL run `make pre-commit-docs` on docs-only staged diffs and MAY skip redundant Clippy when `scripts/precommit-scope.py stamp-valid` matches a clean working tree against `git write-tree` within 1800 seconds. `stamp-write` SHALL write a stamp only when no unstaged edits, untracked files, or `MERGE_HEAD` exist. `.githooks/pre-push` SHALL run `make check-smoke-proof` against `origin/main...HEAD`.

#### Scenario: Stamped clean tree skips redundant Clippy on commit
- **WHEN** `make check` succeeded on the exact `git write-tree` within 1800 seconds, no unstaged or untracked files exist, and `MERGE_HEAD` is absent
- **THEN** `.githooks/pre-commit` exits 0 using the stamp without re-running Clippy

#### Scenario: Dirty worktree refuses both stamp write and stamp reuse
- **WHEN** unstaged edits, untracked files, expired stamp, `MERGE_HEAD`, or `LUSHTEXT_SKIP_PRECOMMIT_STAMP=1` is present
- **THEN** `stamp-write` deletes any stale stamp and `.githooks/pre-commit` runs `make pre-commit`

#### Scenario: Pre-push blocks unverified UI branch commits
- **WHEN** a branch modifies UI-relevant files relative to `origin/main` and lacks matching smoke summaries
- **THEN** `.githooks/pre-push` fails and points to `make smoke-ui`

### Requirement: Local CI-parity verification runs in one ordered gate
`make verify-local` SHALL execute `make check`, default-feature `cargo check -p lushtext --bins --locked`, strict `rustdoc` lints, `cargo bench --no-run`, `cargo deny`, `make test`, `make test-prop`, `make fuzz-corpus-replay`, conditional GTK Lush checks when `crates/gtk-lush*` changed, and `make check-smoke-proof` in sequence, while `make smoke-ui` runs the three UI smokes in parallel after one shared build.

#### Scenario: Local parity catches rustdoc broken links before push
- **WHEN** a public module doc comment links a private item
- **THEN** `make verify-local-fast` and `make verify-local` fail on `cargo doc --workspace --no-deps`

### Requirement: CI workflows short-circuit docs-only pull requests without dropping required checks
Pull-request CI workflows SHALL classify `base...head` via `scripts/precommit-scope.py docs-only-range` and skip heavy build/test jobs on docs-only diffs while keeping every ruleset-required check job reporting `Success`.

#### Scenario: Docs-only PR keeps required ruleset checks green
- **WHEN** a pull request touches only `docs/`, `openspec/`, `.claude/`, `.agents/`, or root `*.md`
- **THEN** `GTK Axioms`, `GTK Axioms (GNOME 50 floor)`, and the PR `Kani Proof Harnesses` jobs skip heavy containers via step conditions and exit 0, while non-required heavy jobs skip

### Requirement: Zero-cost structural refactors may claim Timing: none after dual review
A change touching a timed path solely for zero-cost structural refactoring (`#[repr(transparent)]` wrappers, typed errors, or module moves with unchanged runtime logic) SHALL be permitted to declare `Timing: none` only with explicit rationale confirmed by both independent spec review and final code review.

#### Scenario: Transparent wrapper refactor avoids unnecessary measure.sh rounds
- **WHEN** a change introduces `#[repr(transparent)]` wrappers without changing allocations, locks, or control flow and both spec review and code review confirm the diff
- **THEN** `design.md` and `tasks.md` record `Timing: none` with that rationale instead of running `measure.sh`

### Requirement: Python policy and smoke scripts pass static hygiene checks
`make check-python-hygiene` SHALL run in `make check-policy` and CI `lint`, failing on Python syntax errors, bare `except:`, duplicate dict keys, or redundant regex alternation branches unless explicitly annotated with `# python-hygiene-ok:`.

#### Scenario: Redundant regex alternation fails locally
- **WHEN** a Python script under `scripts/` contains `re.compile(r"foo|foo")` without an exemption comment
- **THEN** `make check-python-hygiene` fails naming the file and line
