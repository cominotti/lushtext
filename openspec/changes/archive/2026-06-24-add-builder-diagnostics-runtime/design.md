## Context

LushText already has a strong Blueprint source-fidelity pipeline: `make check-blueprint` compiles `.blp` sources, checks generated `.ui` drift, and audits the generated template contract; `make lint-blueprint` classifies advisory Blueprint diagnostics. The GNOME 50 builder diagnostics spike showed that runtime `GTK_DEBUG=builder,builder-objects` adds a different kind of evidence because it runs after GTK, Libadwaita, GResources, and app composite widget types are initialized.

The gap is runtime reproducibility. GTK documents `GTK_DEBUG` as a debug environment variable, and GTK debug/debugoptimized Meson build types enable debugging code paths. Ordinary distro GTK packages may ignore the requested debug channel, so a useful lane needs a debug-enabled runtime provider. The user-facing constraint is explicit: the diagnostics must run locally and in CI, and CI must not compile GTK during every run.

## Goals / Non-Goals

**Goals:**

- Add a stable `make builder-diagnostics-smoke` target and script that can run locally and in CI.
- Run diagnostics through a reusable debug-enabled GTK runtime, with a prebuilt OCI image as the default provider.
- Preserve raw stdout/stderr logs, runtime metadata, command lines, classifier output, and coverage artifacts under `build/smoke/builder-diagnostics`.
- Account for every committed generated template as runtime-instantiated, standalone-validated, intentionally skipped, known unsupported, or uncovered.
- Keep the first CI integration in the scheduled/manual end-user smoke workflow, not required pull-request CI.

**Non-Goals:**

- Do not rebuild GTK from source during every diagnostics run.
- Do not make `GTK_DEBUG` an end-user setting, persisted preference, Flatpak permission, or default app runtime environment.
- Do not replace `make check-blueprint`, `make lint-blueprint`, template-contract checks, widget tests, visual smoke, or automation smoke.
- Do not change product UI behavior, GSettings schemas, app-data formats, automation APIs, or Cargo dependencies.

## Decisions

### Use A Dedicated Builder Diagnostics Smoke Lane

Add a new script, target, and smoke artifact directory instead of bolting builder debug output onto the normal widget-test command. The script should own environment setup, runtime-provider selection, standalone `gtk4-builder-tool` probes, runtime GTK probes, classification, and summary generation.

Alternative considered: always set `GTK_DEBUG=builder,builder-objects` in `scripts/run-widget-tests.sh`. Rejected because normal widget tests are a deterministic PR gate, while builder debug output is verbose, host-sensitive, and may be unavailable without a debug-enabled GTK build.

### Use A Prebuilt Debug GTK Runtime Image

The primary runtime provider should be a pinned prebuilt OCI image, for example a GHCR-hosted LushText GTK debug runtime. The image build can happen in a separate workflow or manual maintenance step when the runtime recipe changes. The diagnostics lane itself pulls or uses the image; it does not build GTK on each invocation.

Local execution should support three modes:

- `container`: run the same pinned image through `podman` or `docker`.
- `host`: run directly only when a capability probe proves the host GTK honors the requested debug channel.
- `auto`: prefer an already-debug-capable host when present, otherwise use the configured container provider when available.

CI should use the same image in the scheduled/manual end-user smoke workflow. If CI cannot prove the runtime is debug-capable, that is a setup failure, not a passing diagnostics run.

Alternatives considered:

- Build GTK during every diagnostics job. Rejected because it violates the runtime cost requirement and would make the lane slow and fragile.
- Rely on Fedora's standard CI container. Rejected because standard packages may ignore `GTK_DEBUG` when not built with debug code paths.
- Use the Flatpak SDK/runtime as the diagnostics runtime. Rejected for the first implementation because the normal runtime is optimized for app execution, not proving GTK internal debug channels.

### Make Runtime Capability Detection Explicit

The diagnostics script should run a small capability probe before claiming coverage. The probe records GTK, Libadwaita, GtkSourceView, Blueprint compiler, Rust, OS/container, and image digest or source metadata. It must detect the known unsupported case where GTK reports that `GTK_DEBUG` is ignored because the library was not built with debug support.

Local unsupported hosts may skip with a clear setup summary unless the user sets a required-runtime mode. CI's debug-runtime lane must fail if the prebuilt image does not honor the debug channel.

Alternative considered: infer support from package names or version numbers. Rejected because debug-channel support is a build property, not just a version property.

### Drive Runtime Coverage From A Manifest

Create a manifest that maps `resources/ui/*.ui` templates to the smallest reliable probe: standalone builder-tool validation, a focused widget test, a smoke action, or an explicit uncovered/unsupported reason. The manifest keeps the lane honest when lazy dialogs, popovers, or secondary surfaces are not opened during a broad shell startup.

The first coverage set should include the main shell and existing directly testable composites: window, sidebar, workspace section, editor page, status bar, search bar, search panel, command palette, open popover, preferences, properties panel, markdown preview, info bar, and shortcuts where the runtime can instantiate it. Follow-up tasks can add lazy dialog or workflow-specific probes without changing the overall lane shape.

Alternative considered: treat one successful app startup as covering every template. Rejected because many template-backed surfaces are lazy and would otherwise appear clean without ever being built.

### Classify Diagnostics Before Enforcement

The script should split raw output into a bounded summary with categories:

- `actionable`: template, buildable-child, property, accessibility, or object-construction diagnostics that point to app-owned source.
- `known_tool_limit`: standalone validation limitations for Libadwaita or app composite types.
- `benign_noise`: known runtime/session lines unrelated to builder correctness.
- `unsupported_runtime`: missing or ignored GTK debug channel.
- `future_gate_candidate`: suspicious lines that need more triage before blocking.

The scheduled/manual lane may fail on `actionable` findings once the classifier exists, but it must still preserve raw logs and summaries. Pull-request CI promotion should require a later change after several stable scheduled/manual runs.

Alternative considered: fail on any stderr line. Rejected because GTK debug output is intentionally verbose and contains trace data as well as defects.

### Keep Documentation And Drift Checks In Sync

Update `docs/blueprint-validation.md` and `docs/end-user-coverage.md` so contributors know when to use the lane. Update `scripts/check-end-user-smoke-workflow.py` so the scheduled/manual workflow matrix does not drift from the documented smoke lanes.

Alternative considered: leave the lane as an undocumented expert command. Rejected because the whole point is repeatable local and CI evidence.

## Risks / Trade-offs

- Custom runtime image maintenance can drift from LushText's GNOME target stack -> Pin runtime metadata, publish image provenance, and add a lightweight rebuild workflow for runtime recipe changes.
- A debug GTK runtime may behave differently from the normal distro runtime -> Keep diagnostics focused on builder/template construction and continue using normal widget/smoke lanes for product behavior.
- Local developers without `podman`, `docker`, or debug-capable host GTK may see skips -> Emit setup instructions and make CI the authoritative debug-runtime proof.
- Classifier mistakes could hide real findings or fail on trace noise -> Preserve raw logs, keep classifier rules narrow, and require unclassified lines to appear in the summary.
- Lazy surfaces can remain uncovered -> Make uncovered templates explicit in the coverage report and add probes incrementally.

## Migration Plan

1. Add the builder diagnostics script, coverage manifest, classifier, Make target, and ignored artifact output.
2. Add a reusable debug GTK runtime recipe plus CI publishing or consumption path, pinned by tag or digest so diagnostics jobs do not build GTK each run.
3. Add the scheduled/manual end-user smoke lane using the prebuilt runtime image and upload `build/smoke/builder-diagnostics`.
4. Update documentation, workflow drift checks, and agent guidance.
5. Run local diagnostics through the host or container provider and run CI diagnostics through the reusable image.
6. Roll back by removing the target, script, workflow lane, runtime recipe, and docs; no product data migration is required.

## Open Questions

- What registry path and tag cadence should own the prebuilt debug GTK image?
- Should the image build workflow be manual-only at first, or also scheduled to catch stale base-image security updates?
- Which lazy surfaces should be promoted into first-pass runtime coverage versus listed as explicit uncovered follow-up?
