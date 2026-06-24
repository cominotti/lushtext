## 1. Runtime Provider Preflight

- [x] 1.1 Confirm the exact GTK 4.22 debug-runtime build recipe against official GTK documentation/source, including whether `debug` or `debugoptimized` is the right Meson build type for enabling builder debug channels.
- [x] 1.2 Choose and document the OCI image name, tag scheme, and digest-pinning policy for the reusable LushText GTK debug runtime.
- [x] 1.3 Decide the local provider contract for `auto`, `container`, and `host` modes, including the environment variables for image override, container runner override, artifact directory, and required-runtime behavior.

## 2. Reusable Debug Runtime

- [x] 2.1 Add a committed runtime recipe for the debug GTK diagnostics image with the GNOME 50 GTK family, Libadwaita, GtkSourceView, Blueprint compiler, Mutter/headless dependencies, Rust build prerequisites, and LushText smoke tooling prerequisites.
- [x] 2.2 Add an image self-check that proves `GTK_DEBUG=builder,builder-objects` is honored and records GTK, Libadwaita, GtkSourceView, OS/container, and build provenance metadata.
- [x] 2.3 Add a GitHub workflow or documented release command that builds and publishes the debug runtime image outside normal diagnostics runs.
- [x] 2.4 Add local documentation for pulling or refreshing the runtime image without rebuilding it during `make builder-diagnostics-smoke`.

## 3. Diagnostics Runner And Coverage

- [x] 3.1 Add `scripts/run-builder-diagnostics.sh` with isolated state setup, artifact directory handling, runtime-provider selection, debug-channel capability probing, and clear host/container/CI failure semantics.
- [x] 3.2 Add a template coverage manifest that accounts for every committed `resources/ui/*.ui` file and maps each template to standalone validation, runtime probe, intentional skip, or uncovered/deferred status.
- [x] 3.3 Add standalone `gtk4-builder-tool` validation where useful, while classifying Libadwaita or app-composite load failures as known standalone-tool limitations.
- [x] 3.4 Add runtime probes that instantiate the main shell and first-pass directly testable surfaces under `GTK_DEBUG=builder,builder-objects`.
- [x] 3.5 Ensure runtime probes record whether coverage came from no-context startup, representative content, dense or awkward content, constrained geometry, or template-only construction.

## 4. Diagnostic Classification And Artifacts

- [x] 4.1 Implement a classifier for actionable builder diagnostics, known standalone-tool limitations, benign runtime noise, unsupported runtime, future-gate candidates, and unclassified lines.
- [x] 4.2 Emit raw stdout/stderr logs, command lines, environment metadata, runtime-provider metadata, standalone validation results, runtime probe results, coverage JSON, summary JSON, and a human-readable summary under `build/smoke/builder-diagnostics`.
- [x] 4.3 Make local unsupported-host runs skip clearly unless required-runtime mode is set, and make CI debug-runtime capability failures fail as setup errors.
- [x] 4.4 Make actionable diagnostics fail the scheduled/manual smoke lane after artifacts are preserved.

## 5. Makefile And CI Integration

- [x] 5.1 Add `make builder-diagnostics-smoke` and Makefile help text, wired to `SMOKE_ARTIFACT_DIR=build/smoke` like the other smoke lanes.
- [x] 5.2 Add builder diagnostics to the local `make end-user-smoke` aggregate with explicit unsupported-host skip behavior.
- [x] 5.3 Add a scheduled/manual CI builder diagnostics lane that uses the prebuilt debug runtime image and uploads `build/smoke/builder-diagnostics`.
- [x] 5.4 Update `scripts/check-end-user-smoke-workflow.py` so the smoke matrix, Make target, artifact path, and docs stay synchronized.

## 6. Documentation And Agent Guidance

- [x] 6.1 Update `docs/blueprint-validation.md` to describe the automated builder diagnostics lane, runtime provider modes, artifacts, classifier semantics, and relationship to Blueprint checks.
- [x] 6.2 Update `docs/end-user-coverage.md` to list builder diagnostics as a scheduled/manual smoke lane and explain what a skip or unsupported runtime means.
- [x] 6.3 Update GTK/Libadwaita agent guidance and any relevant `.agents/rules/*.md` files so agents prefer the automated lane over ad hoc builder-debug commands.
- [x] 6.4 Update `AGENTS.md` rules index if any rule files are materially changed.

## 7. Verification

- [x] 7.1 Run `openspec validate add-builder-diagnostics-runtime --strict`.
- [x] 7.2 Run `openspec validate --changes --strict`.
- [x] 7.3 Run `make check-blueprint`.
- [x] 7.4 Run `make check-end-user-smoke-workflow`.
- [x] 7.5 Run `make builder-diagnostics-smoke` locally through the strongest available provider and review the generated summary.
- [x] 7.6 Run or dispatch the CI builder diagnostics lane against the prebuilt debug runtime image and verify artifacts upload.
- [x] 7.7 Run `make check-agent-docs` if agent guidance or rules are changed.
- [x] 7.8 Run `git diff --check`.
