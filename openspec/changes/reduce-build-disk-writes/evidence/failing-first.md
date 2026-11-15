# Failing-first runs (stage 3)

Every self-test below was written before its implementation and run red
first. Each was also run with `GIT_INDEX_FILE` pointing at a missing file, the
way git's pre-commit hook exports it, so the fixture repositories cannot read
the commit's index.

## 1.1 `scripts/build-cache.sh --self-test`

Stub: every command printed the fixed path `/stub/lushtext-build`.

```
$ GIT_INDEX_FILE=/nonexistent <stub>/build-cache.sh --self-test
build-cache self-test: HOME root was '/stub/lushtext-build'
stub exit 1
```

Implementation (D1):

```
$ GIT_INDEX_FILE=/nonexistent scripts/build-cache.sh --self-test
build-cache self-test: ok
```

## 1.2 `scripts/mutants-mirror.py --self-test`

The merge fixture `scripts/mutants-mirror-fixture.json` is real cargo-mutants
27.0.0 output, captured first from a 12-mutant probe crate under
`~/.cache/rdbw-probe` (`cargo mutants --in-place --shard 0/2`, `--shard 1/2`,
and an empty shard, `--shard 1/2 --re 'untested -> u32 with 0'`). It confirmed
the design's facts: sharding is by slice, each shard writes its own
`Baseline` and `log/baseline.log`, `log/src__lib.rs_line_3_col_37.log` exists
in both shards for different mutants, and the empty shard exits 0 with a
`mutants.out` holding no `outcomes.json`.

Stubs (run from a harness that loads the module and replaces functions):

```
# sync, merge, scan and status all stubbed
FileNotFoundError: [Errno 2] No such file or directory: '/tmp/.../slot/src/lib.rs'
stub exit 1
# real sync, stubbed merge
AssertionError: 0
# real sync and merge, stubbed scan and exit status
AssertionError: 0
```

Implementation: `mutants-mirror self-test: ok`.

## 1.3 `scripts/test-address-space-runner.sh --self-test`

Stub: the runner without its `ulimit -v`.

```
test-address-space-runner self-test: a 2 GiB allocation succeeded under a 1G limit
stub exit 1
```

Implementation: `test-address-space-runner self-test: ok`.

## 1.4 `scripts/precommit-scope.py --self-test`

Stub: `docs_only` always true and `include_findings` always empty.

```
AssertionError: crates/**/*.md edit: docs_only=True (stub)
stub exit 1
```

Implementation: `precommit-scope self-test: ok`;
`precommit-scope.py check-includes` passes on the current tree (792 sources).

## 1.5 `scripts/proof-strength.py --self-test` assertions

The D6 assertions, checked against `HEAD`'s `proof-strength.py`:

```
default WORKTREE .../reduce-build-disk-writes/target/proof-strength/wt is not under <checkout-cache>/proof-strength
tests arm passes --jobs
tests arm is not --in-place
tests arm build dir is not pinned at wt/target/tests-build
old exit 1
```

Implementation: `proof-strength self-test passed`, and `~/.cache/lushtext-build`
still did not exist afterwards (the self-test's import probe runs under a
temporary `LUSHTEXT_BUILD_CACHE` and asserts it was not created).
`kani-shards.py --self-test` pins no path task 6.1 changes, so it is unchanged.

## 1.6 `make check-build-cache-helpers` (in `make check-policy`)

With the 1.1 stub in place of `scripts/build-cache.sh`:

```
./scripts/build-cache.sh --self-test
build-cache self-test: HOME root was '/stub/lushtext-build'
make: *** [Makefile:474: check-build-cache-helpers] Error 1
stub exit 2
```

So `make check-policy`, which depends on it, failed before the
implementations landed. With them: every self-test, `build-cache.sh audit`
and `check-precommit-scope` pass (8.4 s together, no cargo). The three audits
each fail on their own fixture in `build-cache.sh --self-test`
(`CARGO_INCREMENTAL=1` exported; `CARGO_INCREMENTAL` in `check-clippy` and in
`run-widget-tests.sh`; a build dir under `XDG_CACHE_HOME`; the cache root's
name outside the helper).

## 3.3 `scripts/run-mutants.sh --self-test`

`HEAD`'s `run-mutants.sh` has no mirror mode and no self-test
(`run-mutants.sh --self-test` printed the usage and exited 2). The self-test
drives a stub `cargo` under a temporary `LUSHTEXT_BUILD_CACHE`: two slots with
`--shard 0/2` and `--shard 1/2`, `--in-place`, no `--jobs`, and
`CARGO_MUTANTS_JOBS` unset; one slot with `--output <checkout>`; the
`MUTANTS_SHARD` refusal with two slots before any slot is taken; exit 75 with
every holder named while three test processes hold the locks; a stale `owner`
with a free lock taken normally; a daemon left by the run not keeping the
lock; an oversized slot emptied; and the `ci-pr`, `ci-full` and `list`
argument lists byte-for-byte as before. `run-mutants self-test: ok`.

## 7.1 `scripts/test-cominotti-flatpak-repo.sh`

Against `HEAD`'s generator the new `--state-dir` assertion fails (the stub
`flatpak-builder` got no `--state-dir`); with the change it passes.
`scripts/test-release.sh` gained the same assertion for the release
validation build (`<checkout-cache>/flatpak-release-builder` beside
`<checkout-cache>/flatpak-release-app`).
