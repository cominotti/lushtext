## Specialist Review Notes

Date: 2026-06-12

Review method: the required skill guidance for GTK testing, GTK live debugging,
GTK/Libadwaita internals, performance, data safety, Rust architecture, and Rust
comments was applied to the current implementation. The session advertised
subagents, but no callable subagent tool was exposed; tool discovery for an
agent/subagent runner returned no available tool. The review was therefore run
inline and bounded to the files touched by this phase.

## Review Lanes

### gtk-testing

- Reviewed the new `cargo-gtk-proof` unit, schema, PNG, corpus, policy,
  animation, warning-scan, automation-client, and synthetic paired-capture test
  coverage.
- Confirmed missing launched artifacts, malformed artifacts, unsupported-host
  summaries, final-settle-only animation evidence, stale frame/sample pairing,
  and missing intermediate stream evidence are represented by tests or corpus
  fixtures.
- Follow-up gate: `cargo test -p cargo-gtk-proof`,
  `cargo gtk-proof corpus`, `make automation-client-self-test`, and
  `make test-widget-headless`.

### gtk-agentic-debugging

- Reviewed headless live-session assumptions: host probes, per-case process
  reports, session logs, same-session capture metadata, bounded child logs,
  ScreenCast/GStreamer evidence, and warning-scan handling.
- Finding fixed: removed stale warning-scanner `dead_code` allowance that still
  described warning scans as staged before live runner output existed.
- Verification finding fixed: per-case `XDG_RUNTIME_DIR` paths under long
  artifact roots exceeded PipeWire's socket path limit. Long case roots now use
  short isolated `/tmp/lt-proof-*` runtime directories, with a regression test.
- Verification finding fixed: Rust mid-file minimap cases captured the
  "before" image before running the same search/navigation preparation used by
  Python. The preparation now runs before before-capture and waits on the same
  search and scrolled-source-view snapshot predicates.
- Follow-up gate: `make visual-geometry-smoke` must preserve a Rust-engine
  summary on a supported host.

### gtk4-libadwaita-internals

- Reviewed GTK/Libadwaita assumptions in the proof runner. This phase does not
  add app widget code; it observes the app through Automation1 snapshots,
  documented actions, GSettings, rendered screenshots, and same-session capture.
- Confirmed app-reported geometry remains diagnostic only. Screenshot-derived
  rendered pixel anchors, protected-region crops, and animation-frame evidence
  are the proof authority.
- No actionable GTK contract finding was found.

### gtk-perf-review

- Reviewed runner runtime cost and resource behavior. No app `src/ui`,
  `src/services`, or `src/model` Rust code was modified by this phase.
- Confirmed the proof path keeps resource caps: bounded JSON artifact sizes,
  bounded process logs, safe artifact resets, PNG decode/crop caps, stream frame
  limits, live session timeouts, and corpus fixture scope.
- No actionable performance finding was found.

### data-safety

- Reviewed artifact writes, reset rules, terminal output, privacy exclusions,
  and cleanup. The phase writes proof artifacts only and does not modify user
  documents, drafts, notes, bookmarks, local history, session files, or search
  sidecars.
- Confirmed summaries point to screenshots/logs instead of embedding raw image
  data, document text, note bodies, draft bodies, local-history contents,
  complete search result text, or private persistence identifiers.
- No actionable data-safety finding was found.
- Verification cleanup: leftover short runtime directories from failed/narrow
  exploratory live runs were removed after the final passing live matrix.

### rust-hex-arch

- Reviewed ownership boundaries. Live proof orchestration stays in
  `crates/cargo-gtk-proof` as a workspace tool; GTK Lush family crates remain
  `0.0.0` and publication, second-consumer, repository split, and upstreaming
  work stay out of this phase.
- Finding fixed: renamed the default `RunMode` and runner function from staged
  terminology to live terminology so the code matches the authoritative Rust
  default.
- Accepted non-blocker: `EngineMetadata::rust_staged()` remains only for
  non-proof unsupported-host summaries and historical compatibility fixtures.

### rust-comments

- Reviewed public and non-obvious orchestration code for intent comments.
- Finding fixed: added comments to the live root summary, invariant
  aggregation, case aggregation, workflow-failure summary, comparison,
  animation, allowed-relationship, pixel-anchor, geometry-summary, and
  app-vs-rendered diagnostic helpers.
- Finding fixed: added named input structs for animation and rendered-anchor
  proof helpers after lint review showed long positional argument lists.
- Accepted non-blocker: simple conversion helpers remain uncommented where
  their names and local call sites are self-explanatory.

## Accepted Non-Blockers

- Automation-client self-tests intentionally retain historical
  `rust-staged-runner` fixture metadata so old artifacts continue to be parsed
  during the transition. New authoritative summaries use `rust-live-runner`.
- Live visual smoke is host-sensitive by design. Unsupported hosts emit stable
  non-proof summaries and do not satisfy visual-sensitive proof policy.
