## Why

The GNOME 50 spike proved that runtime `GTK_DEBUG=builder,builder-objects` can catch template issues that standalone Blueprint and `gtk4-builder-tool` checks cannot see, but the current recipe is manual and depends on whether the host GTK build exposes debug channels. LushText needs a repeatable builder diagnostics lane that works locally and in CI through a reusable debug-enabled GTK runtime, without rebuilding GTK on every run.

## What Changes

- Add an automated builder diagnostics lane that runs LushText UI/template probes with `GTK_DEBUG=builder,builder-objects`.
- Require the lane to run against a debug-enabled GTK runtime provider that can be reused locally and in CI instead of compiling GTK during every invocation.
- Add runtime capability detection so unsupported local hosts report an explicit skip or setup problem rather than producing false-clean builder evidence.
- Add explicit surface/template coverage reporting for runtime-instantiated, standalone-validated, skipped, and uncovered templates.
- Add a diagnostic classifier that separates actionable template defects, known standalone-tool limitations, benign runtime noise, unsupported runtime state, and future-gate candidates.
- Integrate the lane with existing smoke artifacts and scheduled/manual CI while keeping pull-request CI bounded unless a later promotion proves the runtime and classifier are stable enough.

## Capabilities

### New Capabilities

- `automated-builder-diagnostics`: Defines LushText's reusable local and CI builder diagnostics lane, including debug-enabled GTK runtime requirements, runtime probes, diagnostic classification, artifact output, and coverage accounting.

### Modified Capabilities

- `gtk-builder-diagnostics-spike`: Updates the completed spike outcome so the follow-up implementation path points to the automated diagnostics lane rather than a manual-only recipe.

## Impact

- Affected validation surfaces: `make` smoke targets, `scripts/run-*.sh` smoke helpers, `scripts/check-end-user-smoke-workflow.py`, `.github/workflows/end-user-smoke.yml`, and ignored `build/smoke/**` artifacts.
- Affected template/tooling docs: `docs/blueprint-validation.md`, `docs/end-user-coverage.md`, and GTK/Libadwaita agent guidance for builder diagnostics.
- Affected runtime environment: local and CI runs need a documented debug-enabled GTK runtime provider, preferably a pinned prebuilt container image or equivalent reusable runtime, plus a fallback/skip path when the developer's current host GTK cannot honor `GTK_DEBUG`.
- Existing gates remain authoritative: `make check-blueprint`, `make lint-blueprint`, template-contract checks, widget tests, visual smoke, and automation smoke are complemented, not replaced.
- No user-visible UI behavior, app-data format, GSettings schema, Flatpak permission, automation API, or Cargo dependency change is required.
