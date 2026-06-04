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
| Visual smoke | `make visual-smoke` | Rendered desktop screenshots, coarse pixel sanity, compositor behavior, and visual artifacts | No, local, scheduled, or release validation |
| Portal and sandbox smoke | `make portal-sandbox-smoke` | Confined Flatpak/Snap state, portal/sandbox runtime diagnostics, and host support reporting | No, local, scheduled, or release validation |
| Accessibility smoke | `make accessibility-smoke` | AT-SPI-enabled focus and accessible metadata checks outside the accessibility-disabled widget harness | No, local or scheduled |
| Performance smoke | `make performance-smoke` | Lightweight latency and throughput sanity checks distinct from full Criterion reports | No by default |
| Full benchmarks | `make bench-report`, `make bench-report-full` | Reviewable Criterion benchmark reports for release and performance-sensitive work | No, release or manual |
| Mutation testing | `make mutants-smoke`, `make mutants-diff`, `make mutants-full` | Test strength for deterministic model, service, and pure helper code | Diff in PR/scheduled lanes, full manual or scheduled |

## Fast Pull-Request Expectations

Pull-request CI should stay bounded and deterministic. It should run non-widget
tests, property tests, stable fuzz corpus replay, widget tests, benchmark
compile checks, dependency policy, and changed-code mutation where configured.
The default PR lane should not require installed Flatpak/Snap artifacts, live
portal services, AT-SPI, screenshot capture, or full benchmark timing unless a
future change proves a narrow check is cheap and stable enough.

## Scheduled Or Manual Expectations

Host-sensitive lanes should be available through stable Make targets even when
they are not default PR gates:

- `make visual-smoke` captures a representative real-session screenshot using
  isolated XDG state and preserves logs and environment metadata.
- `make portal-sandbox-smoke` records available Flatpak/Snap runtime state and
  runs supported confined smoke checks while skipping clearly when runtimes are
  unavailable.
- `make accessibility-smoke` keeps the accessibility bridge enabled and uses the
  AT-SPI path, complementing widget tests that intentionally set
  `NO_AT_BRIDGE=1`.
- `make performance-smoke` runs a small Criterion smoke filter with coarse
  timing artifacts, including worker-side Replace preview generation so preview
  responsiveness changes have a lightweight elapsed-time tripwire.
- Full fuzz smoke, deep property runs, full mutation, and full benchmark reports
  remain opt-in or scheduled because they are intentionally more expensive.

GitHub Actions mirrors that split: `.github/workflows/ci.yml` owns the bounded
pull-request lanes, `.github/workflows/end-user-smoke.yml` runs visual,
portal/sandbox, accessibility, performance-smoke, and full benchmark-report
artifact lanes on a schedule or manual dispatch, and
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
make visual-smoke
make portal-sandbox-smoke
make accessibility-smoke
make performance-smoke
make bench-report
```

If a host-dependent lane skips, record the exact missing dependency and the
runner or manual environment that will cover it. A skip is useful evidence about
host support, but it is not proof that the skipped behavior works.

## Lane Boundaries

Keep GTK widgets, compositor behavior, D-Bus or portal state, file chooser
flows, watcher timing, installed package behavior, and AT-SPI coverage out of
property tests, fuzz targets, and mutation defaults. Those lanes are strongest
when they stay deterministic.

Use widget tests for GTK state and allocation contracts whenever possible.
Reach for visual, portal/sandbox, accessibility, or performance smoke only when
the existing widget and integration harnesses cannot prove the end-user risk.

When a smoke lane needs automation support, prefer stable actions, accessible
names, read-only debug state, and observable predicates. Avoid coordinate-only
input, fixed sleeps, and broad production debug controls.
