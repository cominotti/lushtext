## 1. Artifact Hygiene

- [x] 1.1 Add targeted ignore rules for Blueprint validation, visual comparison, screenshot, pixel-diff, and smoke-proof output under `build/` without hiding tracked smoke helper files.
- [x] 1.2 Clean any disposable generated proof artifacts from the working tree and document the remaining intentional untracked or tracked files.
- [x] 1.3 Add or update a bounded text proof-summary convention for Blueprint before/after visual comparisons.

## 2. Blueprint Compile Warning Policy

- [x] 2.1 Update the Blueprint validation script to report the active `blueprint-compiler` version and the templates covered by compile, drift, and contract checks.
- [x] 2.2 Capture `blueprint-compiler compile` output per template so warnings can be classified without losing useful diagnostics.
- [x] 2.3 Add a narrow known-warning policy for the documented `GtkShortcuts*` deprecation warnings in `resources/ui/shortcuts.blp`.
- [x] 2.4 Make `make check-blueprint` fail when any compile warning falls outside the known-warning policy.

## 3. Advisory Blueprint Lint Triage

- [x] 3.1 Add or update an advisory lint workflow that groups diagnostics by rule, file, and count.
- [x] 3.2 Classify the current lint diagnostic families, including scroll-parent structure, Adwaita container suggestions, translation text, Unicode text, descriptive text, adjustment property order, and all-caps labels.
- [x] 3.3 Apply safe lint fixes only where regenerated `.ui` output preserves the template contract.
- [x] 3.4 Record accepted advisory rationales for geometry-sensitive lint suggestions that are not fixed in this change.

## 4. Reusable Visual Comparison

- [x] 4.1 Move the one-off Blueprint before/after visual comparison into a reusable script under `scripts/`.
- [x] 4.2 Add script options for baseline ref, current checkout, artifact directory, viewport matrix, and state matrix.
- [x] 4.3 Capture representative populated states, empty or no-required-context states where relevant, constrained geometry, and Blueprint-sensitive secondary surfaces.
- [x] 4.4 Emit a concise comparison summary containing commands, baseline ref, compiler version, artifact directory, state matrix, and pixel-diff metrics.

## 5. Headless Capture Diagnostics

- [x] 5.1 Harden the capture helper so `XDG_RUNTIME_DIR` uses a short temporary path suitable for PipeWire sockets.
- [x] 5.2 Preserve logs and runtime-dir diagnostics on capture failure, avoiding stale `runtime-dir.txt` pointers to deleted paths without a cleanup marker.
- [x] 5.3 Clean temporary runtime directories on successful captures and record the cleanup status in artifacts.
- [x] 5.4 Exercise the helper through at least one successful capture path and one controlled failure or diagnostic path.

## 6. Guidance Updates

- [x] 6.1 Update contributor guidance for editing `.blp` source, regenerating committed `.ui` output, and handling disposable generated artifacts.
- [x] 6.2 Update build or agent guidance for blocking Blueprint compile warnings versus advisory lint diagnostics.
- [x] 6.3 Update visual-proof guidance so future Blueprint template reviews use the reusable comparison workflow.

## 7. Validation

- [x] 7.1 Run `make check-blueprint` and confirm drift, contract checks, and warning classification all pass.
- [x] 7.2 Run `make check-ui-template-contract`.
- [x] 7.3 Run the advisory Blueprint lint workflow and confirm all current diagnostics are fixed or classified.
- [x] 7.4 Run the visual-smoke workflow required for Blueprint template changes.
- [x] 7.5 Run the reusable Blueprint visual comparison script against an explicit baseline ref.
- [x] 7.6 Run `git diff --check`.
- [x] 7.7 Run `openspec validate harden-blueprint-validation --strict`.
- [x] 7.8 Run `openspec validate --changes --strict`.
