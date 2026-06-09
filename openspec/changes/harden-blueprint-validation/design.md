## Context

The Blueprint migration moved UI template source to `.blp` files while preserving generated `.ui` files for the current resource pipeline. The remaining risk is not the migration itself, but the validation surface around it: generated proof artifacts can clutter Git status, `blueprint-compiler compile` currently emits known deprecation warnings, `blueprint-compiler lint` reports useful but noisy diagnostics, and the before/after visual comparison was created as a one-off artifact instead of a reusable proof path.

`blueprint-compiler` is installed locally. On version 0.20.4, `make check-blueprint` succeeds while emitting known `GtkShortcuts*` deprecation warnings from `resources/ui/shortcuts.blp`; `./scripts/blueprint-templates.sh lint` exits non-zero with diagnostics such as `scrollable_parent`, `use_adw_bin`, `translate_display_string`, `use_unicode`, `missing_descriptive_text`, `adjustment_prop_order`, and `avoid_all_caps`. The change should therefore harden the gates without pretending the full lint output is already clean.

## Goals

- Keep generated Blueprint and visual-smoke proof artifacts out of ordinary Git status through targeted ignore rules.
- Preserve the current generated `.ui` drift and contract checks as blocking validation.
- Classify compiler warnings so only documented, narrow `GtkShortcuts*` deprecation warnings are accepted and new warning classes fail.
- Make Blueprint lint advisory until every diagnostic has either a safe fix or a documented accepted rationale.
- Promote the before/after visual comparison into a reusable script with explicit baseline, artifact, and state-matrix inputs.
- Harden the headless Mutter capture helper so short `XDG_RUNTIME_DIR` handling avoids PipeWire path-length failures while preserving useful success and failure diagnostics.
- Update contributor and agent guidance so future template changes follow the same source, artifact, warning, lint, and visual-proof policy.

## Non-Goals

- Replace `GtkShortcutsWindow` or redesign the shortcuts dialog as part of this change.
- Make the full Blueprint lint output a blocking gate before triage is complete.
- Remove committed generated `.ui` templates or change the runtime GResource contract.
- Commit large screenshots, pixel-diff images, or transient capture logs as source artifacts.
- Add a new end-user runtime dependency.
- Make user-visible UI changes except for lint fixes that are separately regenerated, contract-checked, and visually verified.

## Decisions

1. Separate source, summaries, and disposable proof artifacts.

   `.blp` files remain the editable source and generated `.ui` files remain committed compatibility artifacts. Bulky proof output belongs under ignored `build/` paths. Reviewable text summaries should record the baseline ref, compiler version, commands, state matrix, and diff outcome when visual proof is needed.

2. Keep drift and contract checks blocking, but classify compiler warnings.

   `make check-blueprint` should continue to fail on generated `.ui` drift and contract mismatches. The Blueprint compile step should capture compiler output per template, print the compiler version, allow only the documented `GtkShortcuts*` deprecation warnings from `resources/ui/shortcuts.blp`, and fail on any warning that does not match that narrow policy.

3. Treat Blueprint lint as advisory until triage is explicit.

   The lint workflow should summarize diagnostics by rule and file so maintainers can distinguish safe fixes from structural suggestions. Text, translation, Unicode, accessibility, and property-order fixes may be applied when they preserve generated semantics and pass contract checks. Geometry-sensitive suggestions such as container replacement or scroll-parent restructuring should remain classified advisory items unless visual and widget checks prove the generated UI contract is preserved.

4. Rehome the visual comparison as a reusable script.

   The reusable script should accept an explicit baseline ref and artifact directory, capture the current checkout and baseline with the same fixture data and state matrix, then emit a concise summary. The state matrix should include representative populated data, empty or no-required-context states where relevant, constrained geometry, and Blueprint-sensitive secondary surfaces such as menus, dialogs, popovers, editor alerts, search, properties, preview, and sidebar states.

5. Preserve capture-helper diagnostics while using short runtime paths.

   The capture helper should keep `XDG_RUNTIME_DIR` short enough for PipeWire socket paths. On success it can clean temporary runtime directories. On failure it should leave either the runtime directory itself or an explicit cleanup marker plus logs, so `runtime-dir.txt` does not point to a deleted path without explanation.

## Risks / Trade-offs

- Warning allowlists can become stale. The mitigation is a narrow matcher tied to file and warning family, plus compiler-version output in every run.
- Lint fixes can accidentally alter layout semantics. The mitigation is to regenerate `.ui`, run the contract checker, and use visual or widget proof for structural changes.
- Ignoring build artifacts can hide useful evidence. The mitigation is to keep disposable output ignored while preserving small review summaries in proposal notes, PR text, or other intentional text artifacts.
- Visual comparison can be host-sensitive. The mitigation is to run baseline and current captures through the same headless helper, fixture, viewport matrix, and artifact directory conventions.

## Migration Plan

1. Add targeted ignore rules for Blueprint validation and visual-smoke output while preserving already tracked smoke helper artifacts.
2. Update the Blueprint validation script so compile output is captured, known warnings are classified, unknown warnings fail, and the compiler version is reported.
3. Add or adjust an advisory lint workflow that groups diagnostics by rule and file, then fix only safe diagnostics or record explicit accepted rationales.
4. Move the one-off before/after visual comparison into a reusable script with baseline, artifact, and state-matrix parameters.
5. Harden the capture helper's short runtime-dir cleanup and failure-reporting behavior.
6. Update contributor and agent guidance for Blueprint source edits, generated `.ui` regeneration, warning policy, lint triage, artifact hygiene, and visual proof.
7. Validate with `make check-blueprint`, `make check-ui-template-contract`, the advisory lint workflow, the visual-smoke workflow, the reusable visual comparison script, `git diff --check`, and strict OpenSpec validation.

Rollback is straightforward because the change is validation-only: remove the new helper behavior, ignore entries, and guidance updates, then return to the previous blocking checks. No user data or runtime template contract should be affected.

## Open Questions

- The implementation can choose the final script name, but it should live in `scripts/` rather than under `build/` so future reviewers can rerun the visual comparison.
