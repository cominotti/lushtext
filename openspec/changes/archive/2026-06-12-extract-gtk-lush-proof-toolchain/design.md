## Context

GTK Lush Phase 4 extracts the proof infrastructure that currently keeps
LushText honest:

- `crates/lushtext/tests/widget.rs`,
  `crates/lushtext/tests/widget/common.rs`, and
  `scripts/run-widget-tests.sh` own the custom single-threaded GTK widget
  harness, private headless Mutter session, per-test child process isolation,
  retry/flake reporting, warning hygiene, and `wait_until` main-loop drain
  semantics.
- `crates/lushtext-core/src/ui/automation.rs`,
  `crates/lushtext-core/src/model/automation.rs`,
  `docs/automation.md`, and `docs/automation-reference.md` own the bounded
  Automation1 D-Bus contract, readiness predicates, workflow events, snapshots,
  privacy boundaries, and visual geometry fields.
- `scripts/visual-geometry-smoke.py`, `scripts/test-visual-geometry.py`,
  `scripts/visual_geometry_png.py`, `scripts/check-visual-proof-policy.py`,
  `scripts/lushtext-automation.py`, and
  `scripts/visual-geometry-scenarios/*.json` own same-session visual proof,
  scenario parsing, PNG crop and anchor detection, animation stream evidence,
  artifact summaries, and proof-policy enforcement.

The implementation surface is large, around ten thousand lines across the
current proof, harness, automation, and policy code. The change must therefore
be compatibility-led: extract typed Rust APIs and a cargo tool while keeping
today's Make targets, smoke lanes, artifacts, summaries, and pass/fail
decisions stable until parity is proven.

The controlling GTK Lush constraints still apply:

- family crates live under `crates/gtk-lush/<member>` and package as
  `gtk-lush-<member>`;
- family crates are independently adoptable leaves and do not depend on
  LushText crates or on each other;
- no GTK Lush crate owns GTK control flow, replaces Libadwaita behavior,
  introduces a view DSL, or adds an app state/message framework;
- functional in-tree APIs remain `0.0.0` and are not Phase 5
  publication-ready.

## Goals / Non-Goals

**Goals:**

- Add `gtk-lush-proof-harness` as a dev-dependency crate that a stock gtk-rs
  application can use to run widget tests under an isolated headless session
  with per-test subprocesses, bounded retries, warning reporting, and shared
  wait helpers.
- Add `gtk-lush-proof-spine` as a GTK-free runtime crate for versioned
  readiness, snapshot, workflow-event, blocker, and artifact envelope value
  objects plus traits that consumer apps implement with their own state.
- Add `cargo-gtk-proof` as a Rust workspace tool that validates proof schema
  descriptors, compares bounded PNG crops/anchors, replays a compatibility
  corpus, emits stable result envelopes, and enforces Rust proof-policy
  self-tests. The live `run` command remains reserved and non-authoritative
  until a later parity phase.
- Keep existing LushText commands working: `make test-widget`,
  `make test-widget-headless`, `make visual-geometry-smoke`,
  `make check-visual-proof-policy`, `make automation-client-self-test`,
  `make check-automation-docs`, `make check-gtk-lush-policy`, and the
  scheduled/manual end-user smoke visual-geometry lane.
- Prove zero Automation1 D-Bus surface drift: the object path, interface name,
  methods, properties, signals, action activation paths, readiness predicate
  names, snapshot field meanings, status vocabulary, and documentation checks
  remain stable unless an explicitly documented additive field is introduced.
- Publish versioned scenario, summary, artifact, and policy schema descriptors
  with tests and docs so agents can validate proof evidence without reading
  tool source.
- Preserve bounded privacy: no proof crate, cargo tool, wrapper, or artifact
  summary prints unbounded document text, note bodies, draft bodies,
  local-history contents, full search result text, private persistence IDs, or
  raw image/log payloads in terminal output.
- End the phase with extracted proof crates, Rust schema/corpus/policy
  validation, and explicit parity gates that keep Python authoritative for
  live visual execution until the frozen corpus and live lanes prove parity.

**Non-Goals:**

- No Phase 5 publishing, crates.io functional release, second real consumer
  gate, timed afternoon-adoption journal, or repository split.
- No Phase 6 upstreaming work.
- No redesign of LushText Automation1 as a generic D-Bus interface. The
  reusable crate provides traits and value objects; LushText keeps the app-
  specific D-Bus surface.
- No state/message framework, component model, view DSL, custom renderer, or
  Libadwaita adaptive replacement.
- No weakening of existing visual invariants, animation-stream requirements,
  screenshot-derived pixel-anchor oracle, final geometry waits, warning scans,
  or proof-policy negative tests.
- No immediate deletion of Python before compatibility artifacts prove the
  Rust implementation preserves required behavior.

## Decisions

### Keep the cargo tool outside the GTK Lush family

`gtk-lush-proof-harness` and `gtk-lush-proof-spine` are family crates under:

- `crates/gtk-lush/proof-harness`
- `crates/gtk-lush/proof-spine`

`cargo-gtk-proof` is a workspace tool, recommended at:

- `crates/cargo-gtk-proof`

Rationale: the existing family policy requires package names to be
`gtk-lush-<member>` and forbids family interdependencies. A cargo subcommand is
not a leaf library crate and may reasonably depend on `gtk-lush-proof-spine`
or shared workspace test fixtures. Keeping it outside `crates/gtk-lush/`
preserves the anti-framework leaf rule without inventing an exception that
would weaken the family definition.

Alternative considered: place `cargo-gtk-proof` under
`crates/gtk-lush/proof`. That blurs the family policy, creates package-name
exceptions, and encourages future tooling to masquerade as adoptable leaf
crates.

### Extract harness API around process/session orchestration, not app setup

`gtk-lush-proof-harness` owns generic mechanics:

- pre-GTK environment setup hooks;
- private `dbus-run-session` plus `mutter --headless` launch;
- monitor configuration;
- parent harness argument parsing for list/filter/exact/skip behavior;
- per-test child process execution;
- bounded retry and explicit flake reporting;
- stable exit-code classes;
- warning/log scan integration points;
- `flush_events`, `flush_after_delay`, `wait_until`, and realization helpers
  with documented GLib idle-drain behavior.

LushText keeps app-specific mechanics:

- GResource registration;
- GSettings backend/data-dir isolation details;
- `LushtextApplication` creation;
- filesystem fixture helpers;
- widget-test module registry generation unless and until the harness exposes
  an optional build-script helper that works for any consumer.

Rationale: app initialization is where GTK applications differ. The reusable
crate should remove harness lore, not require consumers to restructure their
test layout.

Alternative considered: move the entire generated test registry into the crate
immediately. That risks overfitting to LushText's `build.rs` parser and can be
deferred behind a small adapter if the first extraction already makes LushText
a consumer.

### Extract spine as value objects and traits, not a D-Bus server

`gtk-lush-proof-spine` provides serializable value objects and traits for:

- interface/tool schema versions;
- readiness predicates and blockers;
- workflow events;
- bounded snapshot envelopes;
- surface and geometry summaries;
- privacy classification;
- artifact summary envelopes and status vocabulary;
- implementor-side snapshot/readiness providers.

It does not create, own, or register a D-Bus object. LushText's Automation1
adapter maps its existing app state into the crate's value objects and then
serializes them through the current D-Bus contract.

Rationale: D-Bus names, action paths, and snapshot fields are app-specific and
already documented as LushText's contract. The reusable layer is the protocol
shape and safety discipline, not the app's object.

Alternative considered: make `gtk-lush-proof-spine` a full Automation1 server
implementation. That would either expose LushText-specific fields generically
or force apps into a D-Bus surface they did not choose.

### Port visual proof compatibility-first

`cargo-gtk-proof` is implemented in vertical slices:

1. schema parsing/validation and fixture loading;
2. PNG decode, crop, mask, exact diff, pixel-anchor, and relative-anchor
   evaluation;
3. summary and artifact envelope generation;
4. frozen corpus replay against checked-in Python-produced fixtures;
5. proof-policy evaluation and self-tests.

Later parity phases add:

6. same-session runner orchestration;
7. animation-frame stream capture and timestamp/sample mapping;
8. script/Makefile wrapper migration.

For each slice, Rust output is compared to the existing Python behavior on a
frozen corpus before that slice becomes authoritative. This phase's corpus
covers checked-in status fixtures and pure PNG detector/comparison fixtures.
Before the live Rust path becomes authoritative, the corpus must also cover
passing, failing, skipped, unsupported-host, stale-frame, missing-anchor,
masked-diff, final-settle-only, warning-scan, and animation drift cases.

Rationale: the Python runners are currently the executable specification. A
rewrite without a corpus would be almost guaranteed to lose a failure class.

Alternative considered: rewrite the whole runner and trust live smoke tests.
Live smoke proves only the happy path on the current host; it is weak against
policy regressions and negative-evidence escapes.

### Keep wrapper command names stable

Existing entry points remain:

- `./scripts/run-widget-tests.sh`
- `./scripts/visual-geometry-smoke.py`
- `./scripts/check-visual-proof-policy.py`
- `./scripts/lushtext-automation.py artifact-summary`
- Makefile targets that call them

During this phase, the widget-test wrapper calls the extracted Rust harness
through the existing LushText test binary. Visual-geometry and proof-policy
wrappers keep their Python defaults while the Rust tool supplies schema,
corpus, and policy parity evidence. After later parity, those wrappers can call
the Rust path by default and may keep a reference/compatibility mode for
diagnosis.

Rationale: agents, CI, and docs already depend on these entry points. The
implementation can move without making every consumer update at once.

Alternative considered: replace all scripts with new cargo commands
immediately. That increases blast radius and makes failures harder to separate
from command migration.

### Publish schemas as both docs and machine-readable fixtures

The phase adds versioned JSON schemas for:

- visual scenario manifests;
- expanded case files;
- visual summary files;
- per-case comparison reports;
- animation-frame reports;
- proof-policy metadata and results;
- artifact-summary result envelopes.

Schemas are documented in crate/tool docs and checked by tests. The tool
rejects unknown required fields, reports unsupported schema versions clearly,
and keeps additive optional fields compatible.

Rationale: visual proof artifacts are consumed by humans, agents, policy
checks, and CI. A schema makes compatibility reviewable.

Alternative considered: document shapes only in Markdown. That leaves agents
and policy tests re-implementing validation by convention.

### Treat proof evidence as privacy-sensitive data

All extracted crates and tools inherit LushText's automation privacy rules.
They may record bounded paths, relative artifact paths, counters, rectangles,
hashes, status names, fixture IDs, and cropped screenshots created by test
fixtures. They must not dump user document bodies, notes, drafts, local
history, unbounded logs, or raw image payloads into terminal output or JSON
summaries intended for broad review.

Rationale: proof artifacts are often uploaded to CI or shared with agents.
Moving to reusable tooling must tighten, not relax, privacy boundaries.

Alternative considered: leave privacy only to app adapters. That misses the
places where the generic runner itself chooses what to print and summarize.

## Risks / Trade-offs

- Proof-runner parity misses a failure class -> require a frozen corpus with
  positive and negative cases, compare pass/fail decisions and required
  summary/artifact fields, and keep Python as oracle until parity is green.
- Harness extraction changes GTK timing -> preserve `wait_until` drain-all
  semantics, run the full widget suite before and after migration, and include
  explicit low-priority idle completion tests.
- Automation1 drifts while adopting the spine -> diff introspection and docs,
  run `make check-automation-docs` and `make automation-client-self-test`, and
  add adapter tests that compare old/new snapshot envelopes for representative
  states.
- The cargo tool becomes a hidden framework -> keep it outside the family,
  make it a proof runner not an app runtime, and document that apps opt into
  scenario execution rather than adopt a control-flow model.
- The family leaf rule becomes ambiguous -> update
  `scripts/check-gtk-lush-policy.py`, `gtk-lush-workspace`, and governance so
  proof family crates are checked as leaves while workspace tools use separate
  policy.
- CI time grows too much -> keep corpus fixtures bounded, split pure unit/CLI
  tests from live Mutter smoke, and leave host-sensitive visual smoke in the
  scheduled/manual lane unless existing CI already requires it.
- New dependencies increase packaging surface -> prefer small pure Rust
  dependencies, keep system-tool checks explicit, update cargo-deny/hakari, and
  make unsupported host tooling report skip/unsupported instead of app failure.
- Wrapper compatibility hides new Rust failures -> wrappers must print which
  engine ran, write engine/version metadata in artifacts, and expose a way to
  run corpus parity directly.

## Migration Plan

1. Add crate/tool scaffolding:
   - create `gtk-lush-proof-harness`, `gtk-lush-proof-spine`, and
     `cargo-gtk-proof`;
   - wire workspace members, workspace dependencies, hakari, cargo-deny,
     nextest, CI, API advisory, MSRV, docs, and policy scripts;
   - update GTK Lush README, GOVERNANCE, and the umbrella vision.
2. Extract harness primitives:
   - move generic headless session and child-test orchestration into
     `gtk-lush-proof-harness`;
   - keep LushText app setup and registry adapter local;
   - migrate `crates/lushtext/tests/widget.rs` and
     `scripts/run-widget-tests.sh`;
   - prove `make test-widget` and `make test-widget-headless`.
3. Extract spine primitives:
   - add GTK-free value objects and traits;
   - map LushText Automation1 snapshots/readiness/events into the spine;
   - prove zero D-Bus drift with docs checks, client self-test, and smoke
     artifacts.
4. Build `cargo-gtk-proof` pure replay:
   - port schema parsing, PNG/diff/anchor logic, summary generation, and policy
     self-tests;
   - add frozen corpus fixtures and parity assertions against current Python
     output.
5. Preserve the Python live runner boundary:
   - keep `make visual-geometry-smoke` on the existing same-session Python
     runner until a later parity phase ports launch, readiness waits, capture
     steps, final geometry waits, warning scans, animation stream capture, and
     artifact layout;
   - document the Rust `run` command as reserved and non-authoritative.
6. Move proof policy last:
   - port `check-visual-proof-policy.py` logic and negative self-tests into the
     Rust tool;
   - keep the script wrapper on the Python default path until local policy and
     live-runner parity are separately recorded;
   - preserve `make check-visual-proof-policy`.
7. Update docs and rules:
   - refresh automation docs/reference, end-user coverage docs, GTK testing
     guidance, agent rules/skills if required, and scenario schema docs.
8. Run review and verification:
   - targeted delegated reviews for GTK testing, live GTK debugging,
     libadwaita/GTK contracts, responsiveness/performance, data privacy,
     architecture, comments, and proof evidence;
   - full local validation and host-sensitive smoke evidence before archive.

Rollback strategy: keep wrappers capable of selecting the Python runner/policy
while Rust parity is incomplete. If a late regression is found, restore the
wrapper default to Python, keep the Rust tool behind the compatibility target,
and do not delete Python execution paths until the failing corpus case is fixed.

## Open Questions

- Frozen corpus size: default to a small checked-in corpus that covers every
  status/failure class plus generated test fixtures for larger synthetic PNGs,
  rather than committing full live smoke output.
- Python file retirement: retire Python as a Makefile execution path only after
  recorded Rust corpus, live-runner, animation, and proof-policy parity; until
  then Python remains the bounded live execution path and compatibility oracle.
- `cargo-gtk-proof` CLI spelling: default to cargo subcommands such as
  `cargo gtk-proof run`, `cargo gtk-proof policy`, `cargo gtk-proof schema`,
  `cargo gtk-proof summarize`, and `cargo gtk-proof corpus`.
- Shared schema ownership: default to generated schema files under the tool
  crate with docs linking from `gtk-lush-proof-spine`; implementation can move
  schema type definitions into the spine only if that does not make the family
  crate depend on tool-only behavior.
