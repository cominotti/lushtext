## Why

The Kani consolidation left 27 harnesses in five shards, but the lane has never
run on a GitHub runner. Nobody knows whether each shard fits the 30-minute job
cap or the runner's memory: the two journal shards peak around 9 GB locally,
and a standard public runner has 16 GB. The lane is also weekly and manual only,
so a geometry regression the proofs would catch lands on `main` and is found
days later. On the data-safety side, the `RegisteredDraft` token is meant to
make "a body without an entry" unrepresentable, but
`draft_service::fixture::write_body` bypasses it, and so does the ungated
`services::filesystem::fixture`. Both are compiled into every build, so only
convention stops production from calling them. These are candidates N1 and N5
of `docs/next/formal-verification-next.md`, steps 1–3 of its suggested order.
They are small and complementary: both turn proofs that already exist into
guards that actually hold.

## What Changes

- **Measure the lane on real runners.** `scripts/kani-shards.py run` gains a
  measurement mode. It records each shard's wall time, its peak resident memory
  (the largest descendant process, which is CBMC), and each harness's
  verification time. It writes these to the job summary and to a JSON artifact.
  The implementer dispatches `kani.yml` on the change branch and records the
  per-shard figures in the programme record.
- **Fit every shard with margin.** Each shard must finish in at most 25 minutes
  and peak at no more than 12 GiB on `ubuntu-latest`. A shard that exceeds
  either is split first. Proof bounds are reduced only as a last resort, and a
  recorded reduction also changes the stated bounds.
- **Cache the Kani install.** Cache the Kani binaries and `~/.kani` bundle,
  keyed on `KANI_VERSION`, and adopt the cache only if it measurably shortens
  the job. Caching the Kani target directory is evaluated the same way and kept
  only if it saves time without crowding the repository's cache quota.
- **Record the budgets in the shard table.** The table records each shard's
  measured runner time and peak memory, plus its gate (`pull-request` or
  `scheduled`). `make check-kani-shards` fails when a shard has no measurement
  or when its measurement breaks the margins.
- **Promote `widgets-geometry` to the pull-request gate.** `kani.yml` gains
  `pull_request` and `push: main` triggers. Those events run only the shards
  gated `pull-request`; schedule and dispatch still run every shard. The
  geometry proofs then block regressions instead of trailing them weekly.
- **Make the draft-registration token airtight.** Gate
  `services::draft_service::fixture` and `services::filesystem::fixture` behind
  `#[cfg(any(test, feature = "test-utils"))]`. Benchmarks, `lushtext-core`
  integration tests, and property tests enable `test-utils` explicitly. A
  policy check verifies both gates, and verifies that no shipping build enables
  `test-utils`. CI adds a default-feature `cargo check` of the shipped binary,
  because the all-features Clippy gate cannot see a production caller.
- **Take harness and fixture code out of the mutation scope.** This is a
  pre-existing blocker found while writing this proposal: `make mutants-list`
  lists **171 mutants** in the two `cfg(kani)` harness modules (138 in
  `services/draft_service/kani_proofs.rs`, 33 in
  `services/filesystem/write_protocol/kani_proofs.rs`). No test build compiles
  those modules, so no test can kill any of those mutants and the lane counts
  them as noise. The change excludes `**/kani_proofs.rs` and `draft_service/fixture.rs`, which
  contributes 1 mutant, from the scope.
- **Recognise `kani_proofs.rs` in the workflow-boundaries gate.** The gate
  treats `kani_proofs.rs` as a verification module when its parent declares it
  under `#[cfg(kani)]`, so harnesses over `ui/**` policies (the follow-up
  change `extend-kani-to-pure-policies`) need no prose role or unmutated-ledger
  entry.
- **Update the documentation.** Update the programme record (phase 2 status,
  CI measurements, and the deferral inventory entry for `write_body`, which is
  now closed). Update the next-candidates doc (N1 and N5 done), AGENTS.md,
  `.agents/rules/build.md`, and README.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `formal-verification-kani`: The lane requirement now adds a pull-request
  gate for shards whose measured budget fits it. A new requirement says that
  shard budgets are measured on the CI runner, recorded in the shard table, and
  enforced with margin. A third new requirement says that harness modules are verification code: the
  mutation scope and the workflow role accounting do not count them.
- `draft-session-recovery`: A new requirement says that production builds can
  write a draft body only through the registration token. Unchecked fixture
  writers exist only in test and `test-utils` builds.

## Impact

- **Scripts and CI:** `scripts/kani-shards.py`, `.github/workflows/kani.yml`,
  `.github/workflows/ci.yml` (lint job: default-feature check; bench-compile
  job: `--features test-utils`), the Makefile (`bench*` targets),
  `scripts/bench-report.sh`, `scripts/run-performance-smoke.sh`, and
  `scripts/check-filesystem-boundary.sh`.
- **Code:** `crates/lushtext-core/src/services/draft_service.rs` (module gate),
  `crates/lushtext-core/src/services/filesystem/mod.rs` (module gate), and
  `crates/lushtext-core/Cargo.toml` (`property-tests` implies `test-utils`).
  No production behaviour changes, and no persisted format changes.
- **Configuration:** `.cargo/mutants.toml` gets two exclude globs, and
  `scripts/check-workflow-boundaries.py` recognises harness modules. The
  mutation scope shrinks by 172 mutants: the 171 in harness code, which no test
  can kill, and 1 in fixture code.
- **Documentation:** `docs/next/formal-verification.md`,
  `docs/next/formal-verification-next.md`, `AGENTS.md`,
  `.agents/rules/build.md`, and `README.md`.
- **Process:** measuring needs a pushed branch and `gh workflow run`, which in
  turn needs the maintainer's go-ahead to push.
