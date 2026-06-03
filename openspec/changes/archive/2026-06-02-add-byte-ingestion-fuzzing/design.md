## Context

LushText now has example tests, mutation tests, and bounded property tests, but
those lanes do not answer the hostile byte-ingestion question: can unusual file
bytes make decoding, Markdown preprocessing, or parser setup panic? The current
repo has no `fuzz/` project and no `cargo-fuzz` targets.

Official `cargo-fuzz` workflow uses commands such as `cargo fuzz init`,
`cargo fuzz add <target>`, `cargo fuzz list`, `cargo fuzz run <target>`, and
`cargo fuzz tmin <target> <crash>` for target setup, discovery, execution, and
crash minimization. This change introduces that lane without making fuzzing part
of default local tests, property tests, widget tests, or mutation tests.

## Goals / Non-Goals

**Goals:**

- Add a `cargo-fuzz` project under `fuzz/`.
- Add initial byte-ingestion fuzz targets for editor byte decoding/file-health
  logic and Markdown preprocessing.
- Keep fuzz targets GTK-free and deterministic.
- Add bounded smoke/manual commands and documentation for running, reproducing,
  and minimizing crashes.
- Keep generated crash artifacts out of normal source control while allowing
  intentional seed corpus files to be reviewed.

**Non-Goals:**

- Do not fuzz live GTK widgets, compositor behavior, file choosers, portals,
  watchers, or full app startup.
- Do not require fuzzing in default `make test`, default property tests, or
  default mutation testing.
- Do not replace example tests, property tests, mutation tests, or widget tests.
- Do not promise continuous long-running fuzzing in every pull request.

## Decisions

1. Use `cargo-fuzz` as a separate fuzz workspace.

   `cargo-fuzz` provides the standard Rust/libFuzzer harness structure and
   commands for target setup, running, listing, and minimization. Keeping it
   under `fuzz/` isolates sanitizer/fuzz dependencies from normal application
   and test dependency graphs.

2. Start with byte-ingestion targets, not UI workflows.

   The first target should feed arbitrary bytes into editor decoding and
   file-health classification through a narrow fuzz-facing helper. A second
   target should feed bounded UTF-8/lossy UTF-8 text through Markdown
   preprocessing, especially inline-footnote lowering and parser setup. Both
   targets should assert no panic and no unbounded resource growth for bounded
   input lengths.

3. Add narrow helper APIs if production internals are currently private.

   The implementation may expose feature-gated or crate-visible pure helpers
   around decoding and Markdown preprocessing so fuzz targets exercise the real
   logic without constructing `LushtextMarkdownPreview`, `GtkTextView`, or a
   GSettings-backed render context.

4. Provide bounded commands first, scheduled/manual CI later.

   A `make fuzz-smoke` target should run selected fuzz targets with bounded
   options such as max input length and max total time. Longer runs should be
   manual or scheduled so normal CI latency stays predictable.

5. Treat crashes as durable regression artifacts.

   `fuzz/artifacts/**` should stay ignored. When a crash is real, minimize it
   with `cargo fuzz tmin`, add the minimized input to the relevant seed corpus
   or regression fixture, and add or tighten a deterministic test when possible.

## Risks / Trade-offs

- [Risk] Fuzz tooling pulls sanitizer/nightly constraints into ordinary builds.
  -> Mitigation: keep fuzz dependencies in `fuzz/Cargo.toml` and keep fuzzing
  out of default workspace commands unless explicitly invoked.

- [Risk] Fuzz targets accidentally construct GTK or depend on desktop session
  state.
  -> Mitigation: require fuzz targets to call pure helpers only and document
  GTK/live-session behavior as out of scope.

- [Risk] Fuzz smoke jobs are too slow for pull requests.
  -> Mitigation: make PR fuzzing optional or tiny, and keep deeper fuzzing
  scheduled/manual with explicit time and length bounds.

- [Risk] Crashes are found but not converted into durable tests.
  -> Mitigation: document crash minimization and require fixes to add either a
  corpus seed, deterministic regression test, or both.
