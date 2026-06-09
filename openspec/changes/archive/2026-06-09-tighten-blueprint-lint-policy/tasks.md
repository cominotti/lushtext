## 1. Lint Inventory

- [x] 1.1 Capture the current raw `blueprint-compiler lint resources/ui/*.blp` output and confirm the compiler version.
- [x] 1.2 Map every current diagnostic to one of: promoted cleanup, accepted advisory exception, or structural follow-up requiring proof.
- [x] 1.3 Identify runtime-populated labels, symbolic toggles, brand text, and compact technical labels that should remain advisory rather than blindly translated or renamed.

## 2. Safe Template Cleanup

- [x] 2.1 Fix `use_unicode` findings in `resources/ui/info-bar.blp` and preserve translated accelerator labels.
- [x] 2.2 Apply clearly safe `translate_display_string` fixes for static user-facing strings, leaving runtime-populated, symbolic, brand, or technical strings classified where appropriate.
- [x] 2.3 Verify the `missing_descriptive_text` image in preferences and either add correct accessibility metadata or document why it remains decorative/advisory.
- [x] 2.4 Regenerate affected `.ui` files with `make blueprint-generate`.

## 3. Curated Lint Policy

- [x] 3.1 Update `scripts/blueprint-templates.sh lint` to distinguish promoted must-stay-clean diagnostics from accepted advisory exceptions.
- [x] 3.2 Make the lint workflow fail on unclassified rules, lint errors, or regressions in promoted rule/file subsets.
- [x] 3.3 Keep `adjustment_prop_order` classified when source order is already lower/upper/value and increment/page properties preserve control behavior.
- [x] 3.4 Keep `avoid_all_caps` classified for intentional compact technical labels such as `LF` and `UTF-8`.
- [x] 3.5 Keep `scrollable_parent` and unresolved `use_adw_bin` findings classified unless implementation proof is added in this change.

## 4. Structural-Fix Proof

- [x] 4.1 Review each `use_adw_bin` finding against Rust `TemplateChild` bindings, CSS classes, margins, and layout semantics before deciding whether to implement it.
- [x] 4.2 For every structural lint fix accepted in this change, update Rust template-child bindings intentionally and run relevant widget or visual proof.
- [x] 4.3 Cover affected surfaces in no-required-context, representative populated, many or awkward item, and constrained-geometry states where relevant.
- [x] 4.4 Record any structural findings deferred as advisory with rule, file, and rationale.

## 5. Guidance

- [x] 5.1 Update `docs/blueprint-validation.md` so promoted and advisory Blueprint lint rules match the script.
- [x] 5.2 Update AGENTS or `.agents/rules` guidance if the Blueprint lint policy wording changes there.
- [x] 5.3 Ensure contributor-facing command guidance still distinguishes blocking `make check-blueprint` from curated `make lint-blueprint`.

## 6. Validation

- [x] 6.1 Run `make check-blueprint`.
- [x] 6.2 Run `make lint-blueprint` and confirm promoted diagnostics are clean while accepted advisory exceptions are classified.
- [x] 6.3 Run generated UI template contract validation if not already covered by `make check-blueprint`.
- [x] 6.4 Run targeted widget or visual checks for any structural template changes.
- [x] 6.5 Run `git diff --check`.
- [x] 6.6 Run `openspec validate tighten-blueprint-lint-policy --strict`.
- [x] 6.7 Run `openspec validate --changes --strict`.
