## Context

The Blueprint migration and validation hardening left the project in a good state for source-format maintenance: `.blp` files are editable source, committed `.ui` files remain the runtime resource input, `make check-blueprint` catches generated drift and template-contract breaks, and `make lint-blueprint` groups current linter diagnostics.

The remaining question is policy. `blueprint-compiler lint` on 0.20.4 reports a mixed set of warnings: easy text cleanup (`use_unicode`), localization/accessibility candidates (`translate_display_string`, `missing_descriptive_text`), structural suggestions (`use_adw_bin`, `scrollable_parent`), and noisy or semantic exceptions (`adjustment_prop_order`, `avoid_all_caps`). The raw lint output should not become blocking as a whole, because several current warnings would require behavior changes or geometry-sensitive rewrites to silence.

## Goals / Non-Goals

**Goals:**

- Promote safe, high-signal Blueprint lint expectations into a checked policy.
- Fix low-risk lint findings where the generated UI contract and visible behavior are preserved.
- Keep remaining advisory exceptions narrow, documented, and tied to rule names, files, and rationale.
- Require generated `.ui`, template-contract, widget, or visual proof before accepting structural linter fixes.
- Keep local guidance, docs, and lint script behavior synchronized.

**Non-Goals:**

- Make raw `blueprint-compiler lint resources/ui/*.blp` warning-free at any cost.
- Redesign the editor shell, sidebar, properties panel, status bar, search panel, command palette, or markdown preview to satisfy structural suggestions.
- Rename compact technical labels such as `LF` or `UTF-8` just to avoid all-caps diagnostics.
- Remove adjustment increment/page properties only to silence `adjustment_prop_order`.
- Add a runtime dependency or change the committed `.ui` resource contract.

## Decisions

1. Treat Blueprint lint as a curated policy, not a blanket gate.

   `make lint-blueprint` should continue to parse `blueprint-compiler lint` output, but the policy should separate promoted blocking rules from accepted advisory exceptions. A rule that is not in either set fails the policy so new linter output cannot quietly accumulate.

   Alternative considered: make raw `blueprint-compiler lint` blocking. That would create pressure to apply layout-sensitive suggestions mechanically and would penalize technical labels and current compiler limitations.

2. Promote source-hygiene findings only after cleanup.

   The implementation should target easy, low-risk fixes first: Unicode ellipses, clearly static translatable text, and verified accessibility metadata. After each promoted rule or subset is clean, the policy can mark that subset as must-stay-zero.

   Alternative considered: keep every linter rule advisory forever. That leaves useful hygiene regressions easy to miss.

3. Keep semantic and compiler-limited exceptions advisory.

   `avoid_all_caps` findings for compact technical status labels and `adjustment_prop_order` findings where source order is already normalized should remain accepted advisory exceptions. Documentation should explain why changing these would make the UI worse or remove behavior.

   Alternative considered: rewrite labels or adjustment properties to silence the tool. That would trade correctness and clarity for an empty warning list.

4. Require proof for structural suggestions.

   `scrollable_parent` and `use_adw_bin` findings touch container ownership, scroll behavior, CSS classes, margins, template-child Rust types, and allocation-sensitive shells. These changes should be accepted only when the implementation regenerates `.ui`, passes the template contract, updates Rust template bindings if needed, and proves the visible state matrix still behaves.

   Alternative considered: apply every `Adw.Bin` or scroll-container suggestion as a cleanup. That is too risky for the current composite-template layout.

5. Keep documentation as part of the gate.

   The policy table in `docs/blueprint-validation.md`, the script classifier, and agent/build guidance should describe the same promoted and advisory rule sets. The change is incomplete if docs say a rule is advisory while the script blocks it, or vice versa.

## Risks / Trade-offs

- [Risk] A promoted rule could still include special cases such as runtime-populated empty labels. -> Mitigation: promote only a precise subset or classify specific files until a safe fix exists.
- [Risk] Structural linter fixes could regress narrow, compact, or secondary-surface geometry. -> Mitigation: require widget or visual proof over no-document, populated, many/awkward-item, and constrained-geometry states before accepting them.
- [Risk] Advisory exceptions become stale as Blueprint compiler behavior changes. -> Mitigation: print the compiler version, fail unclassified rules, and revisit policy when the compiler version changes.
- [Risk] Localization cleanup could wrap symbolic toggle labels that translators should not meaningfully change. -> Mitigation: document which symbolic or technical labels remain classified and why.

## Migration Plan

1. Capture the current raw Blueprint lint output and map each finding to a promoted cleanup, accepted advisory exception, or structural follow-up.
2. Apply the safe `.blp` text/accessibility fixes and regenerate matching `.ui` output.
3. Update `scripts/blueprint-templates.sh lint` so promoted rules or promoted rule/file subsets must stay clean, while accepted exceptions remain classified.
4. Update `docs/blueprint-validation.md` and related guidance so contributors understand blocking versus advisory Blueprint lint.
5. For any structural fix attempted during implementation, update Rust template-child bindings if needed and run the relevant widget or visual proof matrix.
6. Validate with `make blueprint-generate`, `make check-blueprint`, `make lint-blueprint`, relevant UI/widget or visual checks for changed surfaces, `git diff --check`, and strict OpenSpec validation.

Rollback is straightforward because this is a validation and source-hygiene change: revert the template edits, regenerated `.ui` output, lint classifier updates, and guidance. No persisted user data or runtime resource format migration is involved.

## Open Questions

- Whether any `use_adw_bin` finding is safe enough to include in the first implementation pass should be decided from the actual Rust binding and visual proof cost, not from the linter output alone.
- Whether runtime-populated empty labels should be wrapped for translation or classified as non-user-visible defaults should be decided per widget during implementation.
