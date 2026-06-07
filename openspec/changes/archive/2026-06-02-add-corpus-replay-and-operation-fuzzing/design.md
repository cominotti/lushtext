## Context

LushText now has three complementary robustness lanes:

- example and widget tests for named behavior and GTK surfaces,
- property tests for bounded deterministic invariants, and
- `cargo-fuzz` targets for hostile byte ingestion through deterministic helper
  surfaces.

The remaining practical gaps do not require LibAFL. First, committed fuzz corpus
seeds should be replayable on stable Rust so known interesting inputs can run in
cheap local or CI checks without nightly, sanitizer runtime, `cargo-fuzz`, or a
C/C++ compiler. Second, LushText can benefit from a small structured operation
fuzz target that turns arbitrary bytes into bounded service/editor operation
scripts, exercising operation ordering without constructing GTK or introducing a
custom fuzzing framework.

## Goals / Non-Goals

**Goals:**

- Add a stable corpus replay lane for committed fuzz corpus inputs.
- Add structured operation fuzzing using existing `cargo-fuzz` or property-test
  infrastructure, not LibAFL.
- Keep replay and operation fuzzing deterministic, bounded, GTK-free, and
  separate from default test/property/widget/benchmark/mutation lanes.
- Document how replay, operation fuzzing, property tests, and mutation tests fit
  together.
- Preserve crash/failure inputs as reviewable seeds or deterministic regression
  tests.

**Non-Goals:**

- Do not add LibAFL.
- Do not build a custom fuzzing framework with bespoke schedulers, feedback,
  distributed orchestration, or state persistence.
- Do not fuzz live GTK widgets, compositor behavior, portals, file choosers,
  watchers, or full application sessions.
- Do not make fuzzing or corpus replay part of default validation unless an
  explicit future policy change requests that.

## Decisions

1. Use a normal stable Rust test lane for corpus replay.

   The replay lane should run committed files from `fuzz/corpus/**` through the
   same deterministic helper surfaces as the fuzz targets. A `make
   fuzz-corpus-replay` command can invoke a dedicated stable test target such as
   `cargo nextest run -p lushtext-core --features fuzzing --test
   fuzz_corpus_replay`, or an equivalent stable Cargo command. This keeps replay
   independent of `libfuzzer-sys`, nightly, sanitizer runtime, and C/C++ compiler
   setup while still reusing the feature-gated helper APIs created for fuzzing.

2. Keep corpus replay read-only and diagnostic.

   Replay should read committed corpus seeds, run them through their matching
   helper surface, and fail with the target name plus seed path when a seed
   panics or violates the replay contract. It should not mutate corpus files,
   write `fuzz/artifacts`, minimize crashes, or discover new inputs. Discovery
   remains the job of `cargo-fuzz`; replay is the stable regression harness.

3. Add structured operation fuzzing as an operation script, not UI fuzzing.

   The operation target should parse arbitrary bytes into a small deterministic
   script. Initial operations should favor pure or tiny deterministic service
   surfaces already covered by examples/properties, such as save-formatting,
   Markdown preprocessing/parser setup, generated replacement previews,
   session/draft serialization, and byte decode/redecode passes. Tempdir-backed
   operations are allowed only if they remain tiny, deterministic, and do not use
   watchers, portals, file choosers, or live sessions.

4. Prefer existing tooling over LibAFL.

   `cargo-fuzz` already provides coverage-guided fuzzing, corpus management,
   artifact paths, and minimization commands. `proptest` already provides
   generated deterministic invariants and persisted regressions. This change
   should reuse those lanes rather than adding LibAFL components such as custom
   executors, observers, feedback, schedulers, event managers, or distributed
   launchers.

5. Keep runtime budgets explicit.

   Structured operation fuzzing should cap input length, operation count,
   per-operation string/path sizes, generated file counts, and any tempdir-backed
   bytes. Smoke commands should include explicit run/time/input bounds. Deeper
   operation fuzzing should remain manual or scheduled.

6. Document lane separation and failure promotion.

   The docs and agent rules should explain that corpus replay checks committed
   seeds on stable Rust, `cargo-fuzz` discovers new hostile inputs, property
   tests prove deterministic invariants, and mutation testing checks assertion
   strength. Real operation-fuzz failures should become minimized corpus seeds,
   deterministic tests, or an explicit no-seed rationale.

## Risks / Trade-offs

- [Risk] Corpus replay drifts from the real fuzz targets.
  -> Mitigation: reuse the same feature-gated helper functions and keep target
  names/corpus directories mapped explicitly.

- [Risk] Operation fuzzing grows into slow random end-to-end testing.
  -> Mitigation: cap scripts aggressively, keep GTK/live session behavior out of
  scope, and reject operations that require watchers, portals, file choosers, or
  compositor state.

- [Risk] Stable replay accidentally pulls in fuzz-only dependencies.
  -> Mitigation: implement replay in the normal workspace with the helper
  feature only; do not depend on `libfuzzer-sys`, `cargo-fuzz`, nightly, or
  sanitizer flags for replay.

- [Risk] Structured operation failures are hard to reproduce.
  -> Mitigation: make the byte-to-operation decoding deterministic, print or
  persist the failing seed path/input, and promote real failures to corpus seeds
  or ordinary deterministic regression tests.

- [Risk] The additional commands confuse maintainers.
  -> Mitigation: update `docs/fuzzing.md`, `.agents/rules/build.md`, and
  `gtk-testing` guidance with a concise lane map and command list.
