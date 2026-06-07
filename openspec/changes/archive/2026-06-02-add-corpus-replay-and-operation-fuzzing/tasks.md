## 1. Stable Corpus Replay

- [x] 1.1 Add a stable Rust corpus replay target or equivalent command that reads committed seeds from `fuzz/corpus/**`.
- [x] 1.2 Map each replayed corpus directory to the same deterministic helper surface used by its matching fuzz target.
- [x] 1.3 Ensure replay runs without `cargo-fuzz`, `libfuzzer-sys`, nightly-only flags, sanitizer runtime, or C/C++ compiler setup.
- [x] 1.4 Make replay failures report the logical target and corpus seed path.
- [x] 1.5 Ensure replay does not mutate committed corpus seeds or write fuzz artifacts, coverage files, or generated corpus growth.

## 2. Structured Operation Fuzzing

- [x] 2.1 Define a bounded deterministic byte-to-operation script decoder with explicit limits for input length, operation count, generated strings, paths, file counts, and per-operation work.
- [x] 2.2 Add a non-LibAFL structured operation fuzz target or generated-input target using existing `cargo-fuzz`, property-test, or stable Rust test infrastructure.
- [x] 2.3 Include initial pure operation coverage for combinations of editor save-formatting, byte decode/redecode, Markdown preprocessing, replacement preview generation, session serialization, or draft serialization.
- [x] 2.4 Add tempdir-backed operations only if they are tiny, deterministic, and independent of watchers, portals, file choosers, live sessions, and user home directories.
- [x] 2.5 Add reviewable seed inputs for the structured operation target and keep generated crash/artifact output ignored.
- [x] 2.6 Verify no LibAFL dependency, custom fuzzer scheduler, custom feedback system, distributed launcher, or custom fuzzer state persistence is introduced.

## 3. Commands and Documentation

- [x] 3.1 Add documented `make` target(s) for stable corpus replay and bounded structured operation fuzz smoke.
- [x] 3.2 Keep corpus replay and operation fuzzing out of default test, property, widget, benchmark, and mutation lanes unless explicitly invoked.
- [x] 3.3 Update `docs/fuzzing.md` with corpus replay, operation fuzzing, command examples, seed handling, failure promotion, and the explicit no-LibAFL policy.
- [x] 3.4 Update `.agents/rules/build.md` and `.agents/skills/gtk-testing/SKILL.md` so future agents choose replay, property tests, fuzz targets, and widget tests at the right boundaries.

## 4. Validation

- [x] 4.1 Run `cargo fmt --all -- --check`.
- [x] 4.2 Run formatting for the isolated fuzz project if it is not covered by root workspace formatting.
- [x] 4.3 Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 4.4 Run any focused Clippy command needed for feature-gated replay or operation-fuzz helper code.
- [x] 4.5 Run `cargo nextest run --workspace` and verify replay/operation fuzz targets stay out of default discovery unless explicitly configured otherwise.
- [x] 4.6 Run the new stable corpus replay command.
- [x] 4.7 Run the bounded structured operation fuzz smoke command.
- [x] 4.8 Run `make test-prop` to verify the property lane remains separate and green.
- [x] 4.9 Run `make fuzz-list` and `make fuzz-smoke` to verify existing byte-ingestion fuzzing still works.
- [x] 4.10 Run `make mutants-smoke` or an equivalent focused mutation-wrapper check to verify replay and operation fuzzing did not enter mutation defaults.
- [x] 4.11 Run `git diff --check`.
- [x] 4.12 Run `openspec validate add-corpus-replay-and-operation-fuzzing --strict`.
