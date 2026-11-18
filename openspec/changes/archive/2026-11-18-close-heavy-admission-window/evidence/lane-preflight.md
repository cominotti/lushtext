# Lane case 4.5: `--mem none` refused by the preflight

Run 2026-11-17 with the worktree's `scripts/heavy.sh` at `10fa0162` + the group 4 Makefile; load 7.62 7.44 6.81.

## heavy.sh --mem none -- make mutants-smoke

```
Running cargo-mutants smoke scope (slots=3, build-jobs=4, test-threads=4)...
./scripts/with-memory-cap.sh --preflight "36G"
with-memory-cap: refusing to admit 36G under LOCK held shared by heavy.sh (pid 857846): an admission that waits for memory there stalls every pending measurement. Run the command as scripts/heavy.sh --envelope <size> -- ... (.agents/rules/verification-runs.md)
make: *** [Makefile:776: mutants-smoke] Error 2
exit 2 after 98 ms
```

## heavy.sh --mem none -- make test

```
Running all tests...
./scripts/with-memory-cap.sh --preflight "${WIDGET_BUILD_MEMORY_MAX:-16G}" && ./scripts/with-memory-cap.sh --preflight "${WIDGET_SHARD_MEMORY_MAX:-2G}"
with-memory-cap: refusing to admit 16G under LOCK held shared by heavy.sh (pid 857931): an admission that waits for memory there stalls every pending measurement. Run the command as scripts/heavy.sh --envelope <size> -- ... (.agents/rules/verification-runs.md)
make: *** [Makefile:357: test] Error 2
exit 2 after 96 ms
```

## heavy.sh --mem none -- make kani

```
with-memory-cap: refusing to admit 16G under LOCK held shared by heavy.sh (pid 858001): an admission that waits for memory there stalls every pending measurement. Run the command as scripts/heavy.sh --envelope <size> -- ... (.agents/rules/verification-runs.md)
make: *** [Makefile:420: kani] Error 2
exit 2 after 91 ms
```

## make -n test-widget WIDGET_PARALLEL=0 (no preflight)

```
echo "Running widget tests..."
./scripts/run-widget-tests.sh --headless
```
