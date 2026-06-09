## Why

Blueprint lint is now available and useful, but the raw output mixes easy text/accessibility cleanup with structural suggestions that could break layout if applied mechanically. This change tightens the policy so safe lint findings are fixed, noisy or compiler-limited findings stay explicitly classified, and future Blueprint edits get a stronger signal without sacrificing UI fidelity.

## What Changes

- Fix the low-risk Blueprint lint findings that are genuinely source hygiene, such as Unicode ellipses, clearly static translatable strings, and verified accessibility metadata.
- Define which Blueprint lint rules are promoted to blocking, which remain advisory, and which require visual or widget proof before structural fixes are accepted.
- Update the advisory lint workflow and documentation so new unclassified Blueprint lint findings fail policy checks, while accepted advisory exceptions remain narrow and justified.
- Preserve the existing generated `.ui` drift, template-contract, and visual-proof requirements for any `.blp` edits.
- Do not chase a zero-warning raw `blueprint-compiler lint` run by changing compact technical labels, removing useful adjustment behavior, or restructuring geometry-sensitive custom widgets without proof.

## Capabilities

### New Capabilities

- `blueprint-lint-policy`: Defines curated Blueprint lint promotion, safe cleanup expectations, advisory exception handling, and proof requirements for geometry-sensitive lint suggestions.

### Modified Capabilities

- None.

## Impact

- Affected scripts: `scripts/blueprint-templates.sh` and related Makefile targets for Blueprint lint/check behavior.
- Affected docs and guidance: `docs/blueprint-validation.md`, AGENTS/rules guidance if lint policy is described there, and any contributor-facing command documentation.
- Affected UI templates: targeted `.blp` and generated `.ui` files only for safe text/accessibility fixes or explicitly proven structural changes.
- Validation impact: `make check-blueprint`, `make lint-blueprint`, generated UI contract checks, visual comparison where structural template changes occur, `git diff --check`, and strict OpenSpec validation.
- Runtime impact: no intended user-visible layout or behavior change except intentional text/accessibility cleanup.
