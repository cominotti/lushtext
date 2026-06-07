## Context

The completed filesystem-boundary work leaves production code routed through `services::filesystem`, with `rustix` contained in the private backend and content search documented as the approved read-only engine adapter. The remaining polish is smaller: a handful of tests still express existence checks through rich `file_facts()` probes, and sidecar workflows have similar scan/delete/migration scaffolding even though their domain rules remain different.

This cleanup is about making the final boundary easy to copy. New contributors and agents should see lightweight status helpers for existence assertions, fixture helpers for disk setup, and intentionally shared sidecar mechanics only where sharing reduces drift.

## Goals / Non-Goals

**Goals:**

- Replace test-only rich metadata probes with `metadata::exists` or `metadata::path_status` where the caller only needs presence or kind.
- Preserve `file_facts()` in tests or production code when canonical path, size, mtime, or kind together are part of the assertion.
- Evaluate sidecar service duplication and extract only a small reusable helper if it removes real repeated filesystem mechanics without hiding workflow-specific identity and filtering rules.
- Extend the no-leftovers audit so test status-probe drift and any new stale sidecar helper surface are caught deterministically.
- Keep repository guidance aligned with the final cleanup rules.

**Non-Goals:**

- No new filesystem trait, virtual filesystem abstraction, or second backend.
- No changes to the completed rustix backend adoption or direct `libc` removal.
- No user-facing behavior changes.
- No rewrite of bookmark, document-note, workspace-note, or local-history domain logic.
- No attempt to route the content-search ripgrep/ignore engine through `filesystem::tree` or `filesystem::read`.

## Decisions

### Decision: Treat test assertions as part of the boundary teaching surface

Tests are not production callers, but they are examples future code copies. Existence-only assertions should use `metadata::exists` or `path_status`, while rich `file_facts()` assertions should remain only where the test needs richer metadata.

Alternative considered: leave tests alone because the production audit already catches drift. Rejected because the root contract explicitly says tests and benches use the boundary, and tests that call full-facts probes for simple existence keep teaching the weaker pattern.

### Decision: Keep the sidecar abstraction small and evidence-driven

The shared helper should only cover repeated filesystem mechanics such as listing JSON sidecars in a directory, removing an optional sidecar path with context, or iterating sidecars under a visible-directory scan. Identity rebasing, workspace-root filtering, empty-document deletion policy, retention, and merge behavior stay in their owning services.

Alternative considered: create a general sidecar service framework. Rejected because bookmark, document-note, workspace-note, and local-history workflows have different domain contracts. A broad framework would obscure those differences and add indirection without a second backend or runtime polymorphism need.

Alternative considered: avoid extraction entirely. Acceptable if implementation shows the remaining duplication is clearer than the helper; the important part is that the decision is explicit and no unused helper surface remains.

### Decision: Strengthen audits around the actual polish risks

The existing boundary audit already catches raw filesystem imports, backend leaks, durable-write implementation imports, engine exceptions, and controlled backend dependency leftovers. This change should add narrow checks for `file_facts(...).is_ok()`/`.is_err()` style status probes in tests as well as production, while allowing cases where facts are immediately inspected. If a new sidecar helper module or function is introduced, the audit or final search evidence must prove it has callers.

Alternative considered: rely on code review for these final cleanup items. Rejected because the prior filesystem work deliberately made no-leftovers checks deterministic.

### Decision: Guidance updates stay narrow

Guidance should mention the final test-status and sidecar-helper rules only where it prevents future drift. This change should not restate the whole rustix migration or churn every filesystem-sensitive skill.

Alternative considered: broad guidance rewrites. Rejected because the boundary is already documented; this is a focused cleanup.

## Risks / Trade-offs

- [Risk] A status-probe audit could flag tests that truly inspect rich metadata. Mitigation: keep the pattern narrow and allow `file_facts()` when the returned facts are used.
- [Risk] A shared sidecar helper could blur domain-specific behavior. Mitigation: extract only filesystem mechanics and keep identity/filtering/retention decisions in the owning services.
- [Risk] Cleanup could expand into another architectural refactor. Mitigation: limit implementation to polish evidence from the previous exploration and avoid new abstractions unless they remove repeated code with active callers.
- [Risk] Guidance updates could churn unrelated skill files. Mitigation: update only files that currently teach or enforce the affected cleanup rules.

## Migration Plan

1. Inventory all remaining `file_facts(...).is_ok()` and `.is_err()` status probes across production, tests, benches, and guidance.
2. Replace existence-only probes with `metadata::exists` or `path_status`; keep rich probes where facts are actually needed.
3. Compare bookmark, document-note, workspace-note, and local-history sidecar filesystem mechanics and either extract a tiny helper with active callers or document that workflow-specific helpers remain clearer.
4. Extend `scripts/check-filesystem-boundary.sh` for test status-probe drift and any stale sidecar helper surface introduced by this cleanup.
5. Refresh guidance only if the final audit contract or helper choice needs to be taught.
6. Run the filesystem-boundary audit, targeted sidecar/filesystem tests, formatting, Rust validation, and strict OpenSpec validation.

Rollback is a normal revert before release because no persisted data format or user-facing behavior changes are expected.

## Open Questions

- During implementation, should the sidecar cleanup extract a helper or explicitly preserve the current workflow-specific shape? The decision should follow the code after inventory, with no unused helper left behind either way.
