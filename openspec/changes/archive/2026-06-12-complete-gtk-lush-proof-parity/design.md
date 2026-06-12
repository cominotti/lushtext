## Context

The previous proof-toolchain phase extracted the reusable proof spine and
created the Rust `cargo gtk-proof` workspace tool, but it deliberately stopped
short of moving live visual execution. Today the Rust tool owns schemas,
bounded artifact summaries, corpus replay, and proof-policy parity work, while
the Python visual runner still owns same-session headless Mutter execution,
Automation1 readiness waits, screenshots, pixel-anchor comparisons, animation
stream capture, warning scans, and the default Makefile/workflow wrappers.

That split was useful while the toolchain was being carved out, but it leaves
GTK Lush in a half-open phase: governance can say proof extraction happened,
yet the default live proof path still depends on the legacy runner and
`cargo gtk-proof run` still reports non-coverage. This change closes that gap
without starting Phase 5 publishing. The work is a parity/completion phase: the
Rust runner becomes authoritative only after it proves it can preserve the
current Python contracts, artifact shapes, status vocabulary, and privacy
boundaries.

The stakeholders are maintainers, agents running visual-sensitive changes, and
future GTK Lush adopters. Maintainers need proof artifacts that remain stable
and reviewable. Agents need machine-readable pass/fail/skip statuses that do
not overclaim coverage. Future adopters need a completed Phase 4 story before
publication gates are opened.

## Goals / Non-Goals

**Goals:**

- Make `cargo gtk-proof run` the authoritative live visual-geometry runner for
  LushText after parity is recorded.
- Preserve same-session capture semantics, Automation1 readiness waits,
  scenario expansion, screenshot-derived rendered anchors, masks, protected
  region comparisons, animation-stream proof, warning scans, and bounded
  artifact writing.
- Expand the compatibility corpus so Python and Rust agree on representative
  pass, fail, skip, unsupported-host, malformed-artifact, rendered-drift,
  warning-scan, stale-frame, and final-settle-only cases before wrapper
  defaults flip.
- Keep `make visual-geometry-smoke`, scheduled smoke workflow artifacts, proof
  policy checks, and automation-client artifact summaries stable for existing
  maintainers and agents.
- Update documentation, schema descriptors, governance, and roadmap notes so
  Phase 4 proof extraction is genuinely closed and Phase 5 remains separate.
- Include enough test and review work to make the one-shot phase safe: Rust
  unit/property/corpus tests, Python/Rust parity replay, live smoke, widget
  gates, documentation drift checks, and focused specialist reviews.

**Non-Goals:**

- No crates.io publication, second-consumer adoption, repository split, or
  first `0.1.0` GTK Lush release.
- No Phase 6 upstreaming work.
- No new GTK Lush public crate or reusable public API beyond the already
  staged proof crates and workspace tool.
- No replacement of GTK, Libadwaita, GtkSourceView, headless Mutter, PipeWire,
  D-Bus, or Automation1 contracts with a custom framework abstraction.
- No portal-only or sandbox-only proof migration. Host capability checks remain
  diagnostics and skip/failure reasons.
- No relaxation of privacy constraints for artifacts, logs, or client output.

## Decisions

1. Implement live orchestration in `cargo-gtk-proof`, not in GTK Lush family
   crates.

   The live runner is a workspace proof command that launches LushText, manages
   host tools, reads Automation1, captures screenshots, and writes policy
   artifacts. Keeping that code in `cargo-gtk-proof` avoids turning the leaf
   GTK Lush crates into a test framework or runtime owner. The alternative was
   to put more orchestration in `gtk-lush-proof-harness`, but that would blur
   the family-crate boundary and make eventual publication harder to explain.

2. Port behavior in layers and keep Python as the oracle until parity passes.

   The implementation should first move typed scenario/artifact models, then
   artifact writing, host/session orchestration, Automation1 interaction, PNG
   and pixel-anchor logic, animation proof, policy integration, and finally
   wrapper defaults. This order keeps each layer testable and prevents the
   wrapper flip from being the first time Rust output is compared against the
   established runner. The alternative was a big-bang rewrite, which would make
   mismatches hard to localize.

3. Treat the compatibility corpus as the wrapper-flip gate.

   The corpus must include both pure fixtures and generated or captured
   artifact envelopes for real failure classes: pass, fail, skip,
   unsupported-host, malformed artifact, rendered drift, warning scan failure,
   stale frame/sample pairing, missing intermediate animation frames, and
   final-settle-only evidence. A mismatch in status, invariant IDs, required
   fields, exit class, or bounded artifact path blocks migration. The
   alternative was relying on live smoke alone, but live smoke is host-sensitive
   and does not cover enough negative cases deterministically.

4. Preserve artifact and result-envelope compatibility first, then enrich.

   Rust may add documented metadata such as engine name, tool version, schema
   version, parity corpus identity, and Python-oracle comparison status, but it
   must keep existing summary paths, status vocabulary, skip/failure semantics,
   and automation-client envelope fields stable. The alternative was a clean
   Rust-only schema reset, which would break maintainer tooling and make
   archive evidence harder to compare.

5. Keep wrapper names stable while changing their implementation.

   `make visual-geometry-smoke`, scheduled end-user smoke, and documented
   script entry points should continue to exist. After parity, their default
   path calls `cargo gtk-proof run`; Python remains available as an explicit
   oracle or diagnostic compatibility path during the transition. This keeps
   muscle memory and CI workflow names intact while moving authority to Rust.

6. Keep unsupported-host behavior explicit and non-proof.

   Rust must probe compositor, screenshot, D-Bus, PipeWire, GStreamer, image
   decoding, and binary availability before claiming coverage. Unsupported
   hosts produce stable skipped or unsupported statuses with bounded artifacts,
   and those statuses never satisfy policy for visual-sensitive changes. The
   alternative was trying to hide host gaps behind broad success summaries,
   which would undermine the proof system.

7. Bound privacy and resource usage at every edge.

   Scenario loading, JSON parsing, PNG decoding, logs, summaries, and
   screenshot/frame artifacts need caps and safe path handling. Terminal output
   should point to evidence files rather than embedding screenshots, raw image
   data, large logs, document text, note bodies, draft bodies, local-history
   contents, or private persistence identifiers. This mirrors the current
   Automation1 and visual-proof privacy model.

8. Require specialist review before archive.

   This phase touches Rust process orchestration, GTK live behavior,
   documentation contracts, artifact privacy, and CI runtime. Archive should
   require focused review lanes for GTK testing, live GTK debugging,
   GTK/Libadwaita contracts, responsiveness and runtime cost, data safety,
   Rust architecture, and comment quality. The alternative was treating this as
   a narrow CLI change, which would miss the real blast radius.

## Risks / Trade-offs

- Live runner drift from the Python oracle -> Mitigate with deterministic
  corpus fixtures, Python/Rust parity replay, and blocking wrapper migration on
  any status, field, path, or exit-class mismatch.
- Host-sensitive flakes -> Mitigate with explicit capability probes, stable
  unsupported-host statuses, bounded artifacts, and scheduled/manual smoke
  lanes rather than pretending every developer machine can run live proof.
- Animation proof regression -> Mitigate with negative fixtures for
  final-settle-only, missing stream mode, missing mapped intermediate PNG,
  stale frame/sample pairing, missing anchors, and rendered drift hidden by app
  geometry.
- Artifact schema churn -> Mitigate with versioned descriptors, compatibility
  aliases, schema validation, documentation drift checks, and automation-client
  self-tests.
- Rust orchestration complexity -> Mitigate by splitting the runner into small
  modules for scenario expansion, host probing, process/session management,
  Automation1, screenshots, comparison, animation, warning scans, artifacts,
  and policy.
- Large or private artifacts -> Mitigate with size caps, safe artifact root
  resets, relative-path reporting, redacted summaries, and tests for bounded
  output.
- Wrapper migration masking diagnostics -> Mitigate by keeping an explicit
  Python oracle/debug path during the transition and requiring docs to state
  which engine produced each artifact.
- Phase creep into publication -> Mitigate with governance and roadmap deltas
  that keep Phase 5 publishing, second consumer work, repository split, and
  upstreaming out of scope.

## Migration Plan

1. Build Rust scenario/artifact models and validators for all live-runner
   inputs and outputs while preserving existing schema versions or documenting
   additive version bumps.
2. Port artifact writing, safe artifact-root reset, bounded JSON/log handling,
   PNG primitives, masks, pixel-anchor detectors, and warning-scan summaries.
3. Implement host/session orchestration for the same isolated headless Mutter
   session model used by the Python runner, including runtime directories,
   GSettings setup, PipeWire/WirePlumber startup, LushText launch, cleanup, and
   unsupported-host summaries.
4. Implement the Rust Automation1 client path for readiness waits, action
   activation through documented actions, snapshots, and workflow evidence.
5. Port same-session before/after capture and comparison, then animation-frame
   stream capture and timestamp-correlated frame/sample evaluation.
6. Expand the compatibility corpus and add a parity mode that compares Rust
   output with the Python oracle for representative positive and negative
   cases.
7. Flip local wrappers, proof-policy checks, and scheduled smoke workflow
   defaults to Rust only after parity evidence is recorded.
8. Update docs, schema references, governance audit entries, and the GTK Lush
   roadmap to record the completed Phase 4 boundary and the remaining Phase 5
   gates.
9. Run the full verification ladder and delegated review lanes before archive.

If the Rust default path fails during implementation, the rollback path is to
leave wrappers on the Python runner, keep Rust as parity-only/non-authoritative,
and document the remaining mismatch. After the phase archives, Python should
remain only as an explicit diagnostic or oracle compatibility path, not as the
default proof authority.

## Open Questions

No product-scope questions are blocking the proposal. Implementation may still
choose the exact internal Rust module names and whether the Python runner is
kept as a thin compatibility wrapper or a separate oracle/debug script, but the
default proof authority after this change must be Rust.
