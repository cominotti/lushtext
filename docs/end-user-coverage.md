# End-User Coverage Map

This document explains which validation lane owns each kind of end-user risk in
LushText. The goal is not to make every test heavy; it is to put each behavior
in the cheapest lane that can prove it honestly.

## Lane Ownership

| Lane | Command | Owns | Default PR? |
| --- | --- | --- | --- |
| Unit and service tests | `make test-unit` | Pure model, service, parser, persistence, and helper behavior that does not need GTK or a live filesystem session | Yes |
| Integration tests | `make test-int` | Cross-service filesystem workflows using deterministic temp directories | Yes |
| Property tests | `make test-prop` | Bounded generated-input invariants for pure and tiny deterministic tempdir-backed workflows | Yes, separate CI job |
| Deep property tests | `make test-prop-deep` | Higher case counts for the same property surface | No, manual or scheduled |
| Fuzz corpus replay | `make fuzz-corpus-replay` | Committed fuzz seeds replayed on stable Rust without cargo-fuzz or sanitizer setup | Yes, separate CI job |
| Fuzz smoke | `make fuzz-smoke` | Coverage-guided discovery for hostile byte and operation-script surfaces | No, scheduled or manual |
| Widget tests | `make test-widget-headless` | Real GTK widget state, signal wiring, focus, action, and allocation contracts under Mutter | Yes |
| Automation docs drift | `make check-automation-docs` | User/developer reference drift for exported actions, D-Bus members, snapshot JSON, readiness predicates/blockers, helper flags, and stable AT-SPI anchors | Yes, through `make check-policy` |
| Visual proof policy | `make check-visual-proof-policy` | Rust-backed local worktree guard that requires a passed, unfiltered visual geometry summary matching the current visual-sensitive diff and required invariant IDs | Yes, through `make check-policy` |
| Automation client self-test | `make automation-client-self-test` | Reusable D-Bus client parser, typed action-parameter rendering, result envelope, exit statuses, and smoke artifact summary reader without a live app | Yes, through `make check-policy` |
| Automation smoke | `make automation-smoke` | Real-process D-Bus introspection, action catalog, snapshots, reusable client commands, action-state sync, readiness waits, warning scans, and parameterized action activation under isolated Mutter | No, local, scheduled, or release validation |
| Visual geometry smoke | `make visual-geometry-smoke` | Rust-backed same-session before/after screenshot invariants, protected-region zero-difference comparisons, screenshot-derived pixel anchors, animation-stream evidence, bounded geometry snapshots, warning scans, and schema-valid artifacts | No, local, scheduled, or release validation |
| Visual smoke | `make visual-smoke` | Rendered desktop screenshots, coarse pixel sanity, compositor behavior, and visual artifacts | No, local, scheduled, or release validation |
| Crash recovery smoke | `make crash-recovery-smoke` | Real-process draft/session recovery across `SIGKILL` and relaunch, with recovery metadata and runtime artifacts | No, local, scheduled, or release validation |
| Portal and sandbox smoke | `make portal-sandbox-smoke` | Confined Flatpak/Snap state, full-filesystem permission posture, portal/sandbox runtime diagnostics, and host support reporting | No, local, scheduled, or release validation |
| Accessibility smoke | `make accessibility-smoke` | AT-SPI-enabled focus, accessible metadata, scenario manifests, editor text-interface evidence, and unsupported-host reporting outside the accessibility-disabled widget harness | No, local, scheduled, or release validation |
| Performance smoke | `make performance-smoke` | Lightweight latency and throughput sanity checks distinct from full Criterion reports | No by default |
| Full benchmarks | `make bench-report`, `make bench-report-full` | Reviewable Criterion benchmark reports for release and performance-sensitive work | No, release or manual |
| Mutation testing | `make mutants-smoke`, `make mutants-diff`, `make mutants-full` | Test strength for deterministic model, service, and pure helper code | Diff in PR/scheduled lanes, full manual or scheduled |

## Fast Pull-Request Expectations

Pull-request CI should stay bounded and deterministic. It should run non-widget
tests, property tests, stable fuzz corpus replay, widget tests, benchmark
compile checks, dependency policy, automation documentation drift checks, and
changed-code mutation where configured.
The default PR lane should not require installed Flatpak/Snap artifacts, live
portal services, AT-SPI, screenshot capture, or full benchmark timing unless a
future change proves a narrow check is cheap and stable enough.

## Scheduled Or Manual Expectations

Host-sensitive lanes should be available through stable Make targets even when
they are not default PR gates:

- `make visual-smoke` captures isolated headless Mutter screenshots for
  search/minimap, modified-tab and destructive close states, file-health
  properties, local-history restore state, normal/compact/constrained document
  properties, normal and constrained Markdown preview, zero-folder/representative/dense/awkward/
  constrained workspace states, workspace-refresh readiness, short-layout
  chrome, no-notes, few/dense/constrained notes, few/dense/constrained
  bookmarks, command-palette files/commands/notes/no-results/dense-files/
  dismissed states, dark style, high contrast, large text, reduced motion,
  transparency/readability, and recovery startup diagnostics. Each capture
  preserves logs, environment metadata, warning scans, PNG sanity checks,
  bounded per-capture manifests, AT-SPI excerpts when a dialog is under test,
  and Automation1 snapshot assertions where a state contract exists. Use
  `scripts/run-visual-smoke.sh --list-cases` and repeated `--case PATTERN`
  filters for focused visual accessibility debugging. The root `summary.json`
  records scenario sources, warning status, screenshots, and
  `visual_accessibility_coverage` groups for focus, variants, color-not-only
  cues, constrained geometry, and unsupported variants.
- `make visual-geometry-smoke` runs `cargo gtk-proof run` for same-session
  before/after visual invariants under isolated headless Mutter. The Rust lane
  waits on Automation1 `visual-geometry-settled`, captures bounded
  `visual_geometry` snapshots and screenshots from one app process, compares
  protected regions exactly except for declared masks, asserts
  allowed-changing-region geometry relationships, verifies declared
  screenshot-derived pixel anchors and relative pixel-anchor deltas, samples
  animation frames in stream mode for animation-sensitive invariants, scans
  runtime warnings, and writes per-case manifests plus a root `summary.json`
  with authoritative `cargo-gtk-proof` engine metadata, scenario-source
  metadata, parity status, `visual_proof_policy`, `verified_invariant_ids`,
  `pixel_verified_invariant_ids`, and `animation_verified_invariant_ids`.
  Automation geometry can bound crops and aid diagnosis, but it cannot satisfy
  rendered-effect coverage on its own. It skips clearly when host compositor,
  PipeWire, D-Bus, GSettings, or screenshot capture tooling is unavailable;
  skipped cases do not count as verified coverage. `make
  visual-geometry-oracle-smoke` remains available for Rust-supervised Python
  oracle diagnostics under `build/smoke/visual-geometry-python-oracle`.
- `make check-visual-proof-policy` is a fast Rust-backed local policy gate for agents and
  contributors. If UI Rust, widget tests, Blueprint/UI templates, or CSS files
  are locally changed, it requires a passing, unfiltered
  `build/smoke/visual-geometry/summary.json` whose visual-sensitive diff
  fingerprint still matches the current worktree and whose
  `pixel_verified_invariant_ids` and `animation_verified_invariant_ids` cover
  any named invariants required by the changed files. The check does not rerun
  the compositor lane itself; it verifies that the proof artifact exists, is
  current, includes required invariant coverage, identifies authoritative Rust
  proof when requested, and does not count skipped visual geometry coverage as
  verification. The compatibility script delegates command execution to
  `cargo gtk-proof policy`; Python oracle summaries remain diagnostic and
  non-authoritative.
- `make automation-smoke` launches the real debug binary under an isolated
  D-Bus session and headless Mutter, introspects the app-owned automation
  object, reads catalog/snapshot state, checks stateful action state against
  snapshot fields, runs the reusable automation client against the live app for
  catalog/snapshot/predicate/wait/event/action commands, waits for idle,
  activates a parameterized GTK action, scans runtime logs for unexpected
  GTK/GDK/Libadwaita/GIO/D-Bus/portal/AT-SPI or filesystem warnings, and
  preserves D-Bus/log/assertion artifacts.
- `make automation-client-self-test` proves the reusable
  `scripts/lushtext-automation.py` parser, typed action-parameter rendering,
  result statuses, and artifact-summary reader without launching LushText. Use
  it as a fast local guard whenever the client or its documentation changes;
  it is not a substitute for the real-process automation smoke lane.
- `make crash-recovery-smoke` launches the real debug binary in isolated app
  state, creates file-backed and untitled draft/session recovery data through
  GTK, sends `SIGKILL`, relaunches with the same data directory, waits for
  Automation1 `recovery-restore-complete`, asserts restored tabs, draft
  metadata, and recovery diagnostics from the relaunch snapshot, and preserves
  before/after metadata summaries, logs, assertions, a bounded scenario
  manifest, and a relaunch screenshot.
- `make portal-sandbox-smoke` records available Flatpak/Snap runtime state,
  writes `permission-posture.txt`, preserves portal bus-name diagnostics, and
  runs supported confined smoke checks while skipping clearly when runtimes are
  unavailable. Portal diagnostics are evidence only; they do not imply a
  portals-only migration while the Flatpak keeps full filesystem access.
- `make accessibility-smoke` keeps the accessibility bridge enabled, uses the
  AT-SPI path, verifies stable anchors across shell/editor/search/Open
  popover/command palette/workspace/properties/preferences/Markdown preview/
  notes/local-history surfaces, and records focus plus text-interface evidence where the host
  exposes it. The lane writes bounded per-scenario manifests, assertion JSONL,
  warning status, screenshots, AT-SPI tree/focus excerpts, `summary.txt`, and
  `summary.json`; unsupported AT-SPI or compositor hosts skip with an explicit
  reason that does not count as coverage. It complements widget tests that
  intentionally set `NO_AT_BRIDGE=1`; action or D-Bus checks alone are not
  counted as accessibility coverage. Pair this lane with
  [`docs/accessibility.md`](accessibility.md) when reviewing user-facing
  behavior or release readiness.
- `make performance-smoke` runs a small Criterion smoke filter with coarse
  timing artifacts, including worker-side Replace preview generation and
  recovery fixtures for malformed metadata, pending migrations, duplicate
  sidecars, many local-history lineages, and first-dirty autosave persistence.
- Full fuzz smoke, deep property runs, full mutation, and full benchmark reports
  remain opt-in or scheduled because they are intentionally more expensive.

GitHub Actions mirrors that split: `.github/workflows/ci.yml` owns the bounded
pull-request lanes, `.github/workflows/end-user-smoke.yml` runs automation,
visual-geometry, visual, crash-recovery, portal/sandbox, accessibility,
performance-smoke, and full benchmark-report artifact lanes on a schedule or
manual dispatch, and
`.github/workflows/release-benchmark.yml` attaches a full benchmark report to
tagged release validation.

## Release Validation Expectations

Before a public release, use the normal release preflight plus end-user smoke
lanes that are available on the host:

```sh
make test-unit
make test-int
make test-widget-headless
make test-prop
make fuzz-corpus-replay
make automation-smoke
make visual-geometry-smoke
make visual-smoke
make crash-recovery-smoke
make portal-sandbox-smoke
make accessibility-smoke
make performance-smoke
make bench-report
```

If a host-dependent lane skips, record the exact missing dependency and the
runner or manual environment that will cover it. A skip is useful evidence about
host support, but it is not proof that the skipped behavior works.

For releases that touch UI, shortcuts, accessible metadata, screen-reader
behavior, visual styling, row factories, transient surfaces, search/list
surfaces, or smoke tooling, add the accessibility release reference:

- Preserve and review `make accessibility-smoke` artifacts, including
  `build/smoke/accessibility/summary.json`, per-scenario manifests, assertion
  JSONL, AT-SPI excerpts, focus artifacts, environment reports, and warning
  scans.
- Preserve visual accessibility evidence with `make visual-geometry-smoke`,
  `make visual-smoke`, and `make check-visual-proof-policy` when focus
  indication, primary control visibility, color-not-only state, large text,
  contrast, reduced motion, transparency/readability, or constrained geometry
  can be affected.
- Run a manual Orca check in a normal GNOME session for the changed workflows,
  especially editor text, caret or selection feedback, shell navigation,
  command palette, Open popover, workspace search, workspace sidebar/file tree,
  document properties, preferences, Markdown preview, notes/bookmarks, local
  history, and destructive or close dialogs.
- Treat skipped AT-SPI, compositor, visual, or screen-reader coverage as
  unverified until another runner or manual environment covers the same
  behavior. Record the exact environment and caveat in release notes or release
  validation artifacts.
- Keep the user-facing contract in [`docs/accessibility.md`](accessibility.md)
  synchronized with any changed shortcut, accessible name, announcement
  behavior, stable AT-SPI anchor, smoke scenario, or known platform caveat.

## Lane Boundaries

Keep GTK widgets, compositor behavior, D-Bus or portal state, file chooser
flows, watcher timing, installed package behavior, and AT-SPI coverage out of
property tests, fuzz targets, and mutation defaults. Those lanes are strongest
when they stay deterministic.

Use widget tests for GTK state and allocation contracts whenever possible.
Reach for automation, visual geometry, visual, portal/sandbox, accessibility,
or performance smoke only when the existing widget and integration harnesses
cannot prove the end-user risk. Use visual geometry smoke when a change claims
that unaffected pixels stay unchanged across one same-session layout action, or
when a rendered effect such as a minimap highlight needs pixel-anchor proof; use
visual smoke for standalone rendered state coverage.

When a smoke lane needs automation support, prefer stable actions, accessible
names, read-only debug state, and observable predicates. Avoid coordinate-only
input, fixed sleeps, and broad production debug controls.
