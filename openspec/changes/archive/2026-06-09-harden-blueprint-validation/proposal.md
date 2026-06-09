## Why

The Blueprint migration preserved the UI contract, but its review artifacts, warning output, and visual-proof tooling still need a more durable home before publication. This change hardens the validation layer so future template edits stay clean without committing bulky proof output or normalizing noisy gates.

## What Changes

- Keep Blueprint validation and visual-smoke proof artifacts out of ordinary Git status through targeted artifact hygiene, without hiding existing tracked smoke artifacts.
- Promote the before/after visual comparison from a one-off `build/` artifact into a reusable script with explicit baseline, artifact, and state-matrix inputs.
- Make `make check-blueprint` robust against compiler warning drift by allowing only documented known warnings and failing on any new Blueprint compiler warning class.
- Add an advisory Blueprint lint workflow that records and classifies current diagnostics before any lint rule becomes blocking.
- Harden the headless Mutter capture helper so short `XDG_RUNTIME_DIR` handling avoids PipeWire socket-length failures while preserving useful failure diagnostics.
- Update contributor and agent guidance so generated `.ui` drift checks, warning policy, lint triage, and visual-proof artifacts are handled consistently.

## Capabilities

### New Capabilities

- `blueprint-validation-hardening`: Defines the contract for Blueprint validation artifact hygiene, reusable visual comparison, compiler warning policy, advisory lint triage, and capture-helper diagnostic preservation.

### Modified Capabilities

None.

## Impact

- Affected scripts: `scripts/blueprint-templates.sh`, `scripts/check-ui-template-contract.py` if needed for warning summaries, the visual-smoke or new visual-comparison script, and `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py`.
- Affected build and policy files: `Makefile`, `.gitignore`, `.github/workflows/ci.yml` only if CI wiring changes, and any relevant validation helper documentation.
- Affected UI templates: no intended user-visible UI changes; `.blp` edits are limited to lint triage where they preserve generated `.ui` semantics or are explicitly regenerated and verified.
- Affected docs and guidance: README, AGENTS, `.agents/rules/build.md`, `.agents/rules/ui.md`, and the UI subtree guidance if they mention Blueprint validation or visual proof.
- Dependency impact: uses existing `blueprint-compiler` and host visual-smoke tooling; no end-user runtime dependency is added.
