## Context

The Kani lane (`make kani`, `scripts/kani-shards.py`, `.github/workflows/kani.yml`)
runs 27 harnesses in five shards. Only local times exist (Kani 0.68.0 / CBMC
6.11.0, one toolbox, 40 min 29 s for the whole lane):

| Shard | Harnesses | Longest local harness | Local peak memory |
|---|---|---|---|
| `widgets-geometry` | 9 | `slice_covers_…` 176.9 s | small |
| `widgets-slice-loop-rest` | 3 | `…three_bins` 328.7 s | not recorded |
| `widgets-slice-loop-requests` | 6 | `…two_bins` 366.3 s | not recorded |
| `core-journal-and-write` | 8 | `journal_invariants_hold_under_crashes` 487 s | about 8 GB |
| `core-second-writer` | 1 | 500.8 s | about 9 GB |

Each job builds the crate's GTK dependencies under Kani's nightly. It also
runs `cargo install kani-verifier` from source and then `cargo kani setup`.
Neither step is cached today. The runner is `ubuntu-latest` with a Fedora 44
container. The repository is public, so that runner has 4 vCPUs, 16 GB RAM, and
about 14 GB of SSD. `check-workflow-timeouts` caps every job at 30 minutes.

On the draft side, `write_draft(data_dir, &RegisteredDraft, content)` is the only
production body writer, and only registration mints the token. Two public
modules are compiled into every build and can skip it:

- `services::draft_service::fixture::write_body` calls the private
  `write_body_file` directly;
- `services::filesystem::fixture::write_text` and its siblings call `sys::write`,
  so they can put bytes at `drafts/<id>.draft` with no token and no durable
  write.

They are used by unit tests (`cfg(test)`), the `lushtext` integration and widget
tests (whose dev-dependency already enables `lushtext-core/test-utils`), the
`lushtext-core` integration and property tests, and the benchmarks. The
blocking Clippy gate runs with `--all-features`, so it enables `test-utils` and
cannot see a production caller of either module.

## Goals / Non-Goals

**Goals:**

- Measure every shard's wall time and peak memory on the real runner, record
  the figures, and make the gate enforce them with margin.
- Put `widgets-geometry` on the pull-request gate inside the 30-minute cap, and
  keep the other shards scheduled or manual.
- Make "a body without an entry" a compile-time fact for shipping builds, not a
  convention.

**Non-Goals:**

- New harnesses. N2 is the separate change `extend-kani-to-pure-policies`.
- Promoting the slice-loop or journal shards to the pull-request gate. They are
  5–10 minutes of CBMC each, even locally.
- Reducing any proof bound, unless measurement forces it (see D3).
- Changing any draft behaviour or persisted format.

## Decisions

### D1. Measure inside `kani-shards.py`, not with an external tool

`run` gains `--measure <json-path>`. Around each `cargo kani` invocation it
records:

- **wall time**, from `time.monotonic()`;
- **peak memory**, from `resource.getrusage(RUSAGE_CHILDREN).ru_maxrss` taken
  after the child exits. On Linux this is the largest resident set of any
  waited-for descendant. `cargo-kani` waits on `kani-driver`, which waits on
  CBMC, so the figure is CBMC's peak;
- **per-harness time**, from Kani's own `Verification Time:` line. The script
  already streams Kani's output, so it tees and parses it.

With `GITHUB_STEP_SUMMARY` set, the script appends a Markdown table. The
workflow uploads the JSON with `actions/upload-artifact`.

*Alternatives:*

- GNU `/usr/bin/time -v`. It needs an extra Fedora package and parsing in shell.
- Sampling the cgroup's `memory.peak`. It works in a container, but it also
  counts the page cache from the crate build, which overstates CBMC's needs.
- The rusage figure is the one that can OOM a job, and it needs no dependency.

### D2. The shard table carries gate and budget; the check enforces them

Each `SHARDS` entry becomes a small record:

```python
Shard(package, filters, gate="pull-request" | "scheduled",
      ci_minutes=<float | None>, ci_peak_gib=<float | None>, measured_in="<run URL or id>")
```

`check`:

- keeps its exactly-one-shard rule;
- fails when `ci_minutes` or `ci_peak_gib` is `None`, when `ci_minutes > 25`,
  when `ci_peak_gib > 12`, or when a `pull-request` shard has
  `ci_minutes > 15`;
- has self-test cases for each of these failures.

`github-outputs` emits `shards` (all) and `pr-shards` (gated `pull-request`).

*Margins:*

- 25 of 30 minutes, which leaves room for runner variance and a slower Kani
  install on a cache miss.
- 12 of 16 GB, which leaves room for the runner agent, the container, and
  kernel memory.
- 15 minutes for a pull-request shard, so it does not become the slowest
  required check. The widget-tests job is 30 minutes.

*Alternative:* keep the measurements only in the programme record. Measurements
kept only in prose drift, and nothing then stops a slow shard from being gated
`pull-request`. The table is already the single source of truth, so the budget
belongs next to the membership.

*Bootstrap:* the check requires measurements, so the implementer records them
in the same change that adds the requirement, from the dispatched run (task
group 2). Until then the check runs in a report-only mode behind
`--allow-unmeasured`, used only by the measurement dispatch. That flag is
removed before the change completes.

### D3. Fit order: split, then tune, never silently weaken

If a shard exceeds 25 minutes or 12 GiB on the runner, the implementer tries
these fixes in order:

1. **Split the shard.** `journal_invariants_hold_under_crashes` and the two
   liveness harnesses would become separate shards. That costs one more job and
   one more crate build, and weakens nothing.
2. **Tune the solver.** Try another CBMC/Kani solver (for example
   `#[kani::solver(kissat)]` or `cadical`) on the offending harness. Adopt it only
   if the local run shows the same verdict and lower memory or time.
3. **Reduce a bound.** Lower `STEPS` or an unwind bound only as a last resort.
   Record it in the programme record and in the harness's doc comment, and keep
   the draft-session-recovery wording ("within its stated bounds") true.

`core-second-writer` is a `should_panic` pin. Reducing its action count is
acceptable only if the recorded counterexample is still found. It already drops
to 6 actions for that reason.

### D4. Pull-request gating lives in `kani.yml`, driven by the table

`kani.yml` gains `pull_request:` and `push: branches: [main]` beside `schedule`
and `workflow_dispatch`. The `shards` job picks the matrix:

- `pr-shards` for `pull_request` and `push`;
- `shards` otherwise.

Concurrency grouping already keys on `github.ref`, and pull requests get
`cancel-in-progress`.

*Alternative:* add a job to `ci.yml`. That duplicates the Kani install steps
and the shard selection in two workflows, and splits the table's authority. One
workflow, one table, and event-driven selection keep a single source of truth.
The comment at the top of `kani.yml` is updated so it no longer says "outside
the pull-request gate".

### D5. Cache the Kani install; evaluate the target-dir cache by measurement

- **Install cache.** Use `actions/cache` for `~/.cargo/bin/cargo-kani`,
  `~/.cargo/bin/kani`, and `~/.kani`, keyed on
  `kani-${KANI_VERSION}-fedora-44`. On a hit, skip `cargo install` and `setup`.
  In the container `HOME` is `/github/home`, so paths use `$HOME`. The install
  cache is small and version-keyed, so it is adopted unless measurement shows no
  gain.
- **Target-dir cache.** Cache `target/kani`, keyed on the package, `Cargo.lock`,
  and `KANI_VERSION`. Adopt it only if a warm run saves at least 3 minutes on
  the pull-request shard, and the combined cache stays under half the
  repository's 10 GB Actions quota. Otherwise record it as rejected, with
  figures.
- **Existing cache.** `Swatinem/rust-cache` keeps caching the stable-toolchain
  `target/`. `target/kani` is excluded from it, so the two caches do not
  double-store.

### D6. Gate both fixture modules with the existing `test-utils` convention

Both `pub mod fixture;` declarations become
`#[cfg(any(test, feature = "test-utils"))] pub mod fixture;`. This matches the
`recent_documents` precedent.

Only gating both closes the hole: with `filesystem::fixture` left open,
`write_text(drafts_dir.join("x.draft"), …)` would still skip the token.

Consumers:

| Consumer | How it gets `test-utils` |
|---|---|
| unit tests | `cfg(test)` |
| `lushtext` integration and widget tests | existing dev-dependency feature |
| `properties` test | `property-tests = ["test-utils"]` in `lushtext-core` |
| `persistent_json_format` and any other `lushtext-core/tests/*.rs` that needs fixtures | built by workspace nextest, where feature unification already enables it |
| benchmarks | `--features test-utils` on every documented command: Makefile `bench*`, `scripts/bench-report.sh`, `scripts/run-performance-smoke.sh`, `ci.yml` bench-compile |

*Alternatives:*

- `required-features = ["test-utils"]` on the targets. Cargo then skips a
  target with only a warning, which is the silent fail-open shape the repo's
  smoke rules forbid. A missing feature should be a loud compile error.
- A self dev-dependency (`lushtext-core = { path = ".", features = [...] }`).
  Cargo rejects it as a cycle.

### D7. Policy check and a default-feature build prove the gate

- **Policy check.** `scripts/check-filesystem-boundary.sh` owns fixture
  allowlisting, so it gains a rule with a self-test. The rule fails when either
  `pub mod fixture;` lacks the gate, when `test-utils` appears in a `default`
  feature list, or when `test-utils` (directly, or through
  `manual-format-upgrade-fixtures`) is enabled by a shipping build:
  `crates/lushtext/Cargo.toml [dependencies]`, `build-aux/cargo.sh`, the
  Flatpak manifest, or `snap/snapcraft.yaml`.
- **Default-feature build.** The CI lint job gains
  `cargo check -p lushtext --bins --locked`. That is the build that fails if
  production names a fixture.
- **Failing first.** The implementer runs the policy rule before gating, where
  it must fail. A scratch production call must also compile before the gate and
  fail `cargo check -p lushtext --bins` after it. The scratch call is reverted
  and its output recorded in the programme record.

### D8. Mutation scope: exclude harness and fixture code (pre-existing blocker)

`make mutants-list` currently lists 171 mutants in `cfg(kani)` code:

- 138 in `services/draft_service/kani_proofs.rs`;
- 33 in `services/filesystem/write_protocol/kani_proofs.rs`.

`examine_globs` includes `services/**/*.rs`, but no build that cargo-mutants
runs compiles those modules. No test can kill those mutants, so they are noise
that hides real survivors. `draft_service/fixture.rs` adds 1 more, and it is
fixture code, like its already-excluded filesystem sibling.

The fix adds `crates/**/kani_proofs.rs` and
`crates/lushtext-core/src/services/draft_service/fixture.rs` to `exclude_globs`,
with a comment. The count is verified from the tool rather than the config:
`make mutants-list` drops by exactly 172 (6,001 → 5,829 at the time of
writing), and no other file's count changes.

*Alternative:* annotate harness functions with `#[mutants::skip]`. That needs
the `mutants` attribute crate in every harness, and a new harness would silently
reintroduce the noise. A glob keyed on the fixed module name, which the next
decision makes mandatory, cannot drift.

### D9. `kani_proofs.rs` is a recognised verification module

`scripts/check-workflow-boundaries.py` classifies every GTK-free `ui/` module
that declares a `fn` as decision logic. That module must then carry a role, or a
`PROSE_CLASSIFIED_UNMUTATED` entry. The role-home rule separately flags any
undeclared `.rs` file in a migrated role home. A harness over a `ui/**/policy.rs`
module would trip both, although it is neither production decision logic nor a
workflow module.

The gate gains one name, `KANI_HARNESS_MODULE_NAME = "kani_proofs.rs"`. Both
rules skip such a file only when its parent module file contains
`#[cfg(kani)]` immediately before `mod kani_proofs;`. An ungated declaration is
a finding.

The parent is found by path:

- `policy/kani_proofs.rs` → `policy.rs`;
- `x/kani_proofs.rs` → `x/mod.rs` or `x.rs`.

The self-test covers four cases: gated passes, ungated fails, missing parent
fails, and a non-harness file with the same content still fails.

This lands here, before any `ui/` harness exists, so the follow-up change adds
harnesses without touching the gate. The naming rule is also stated in the
`formal-verification-kani` spec.

## Risks / Trade-offs

- **A measured run needs a push.** `workflow_dispatch` runs the branch's version
  of `kani.yml` only when the branch exists on GitHub. Mitigation: the tasks
  push a branch only after the maintainer approves, then dispatch with
  `gh workflow run kani.yml --ref <branch>` and read the results with
  `gh run watch` and `gh run download`.
- **One measurement is noisy.** Mitigation: dispatch twice (cold cache and warm
  cache) and record the larger figure per shard. The margins absorb the rest.
- **A cache miss makes the pull-request job slower.** Mitigation: the 15-minute
  pull-request budget is measured cold. A warm cache is a bonus.
- **A pull-request Kani job adds CI minutes on every PR.** On a public repo it
  costs no money, and it replaces a weekly-only signal.
- **Benchmark commands now need a flag.** Mitigation: every documented entry
  point passes it, and a missing flag fails loudly at compile time. That is the
  intended failure mode.
- **Feature unification hides a missing flag in workspace builds.** Mitigation:
  the policy rule and the default-feature `cargo check` of the shipped binary do
  not depend on unification.

## Migration Plan

This change has no user-facing migration. Rollback is to revert the change. If
the pull-request Kani job misbehaves, the fastest relief is to regate
`widgets-geometry` as `scheduled` in the table. That is a one-line revert that
leaves the measurement machinery in place.

## Open Questions

- Should the pull-request Kani check be marked required in branch protection?
  That is a repository setting for the maintainer, not code. The change records
  a recommendation only.
