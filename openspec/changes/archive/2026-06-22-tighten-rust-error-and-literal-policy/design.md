## Context

LushText already has a curated Rust linting policy: the blocking gate runs all workspace targets and features, broad Clippy groups stay advisory, and `scripts/lint-advisory-policy.toml` classifies non-blocking findings. The exploration found that this policy is sound, but it does not yet give future agents and reviewers a crisp rule for two recurring readability risks:

- error types whose names are clear in their defining module but ambiguous after re-export or cross-layer use;
- numeric literals whose meaning is behavioral policy rather than a simple index, identity, or test datum.

The current codebase has good examples to preserve. `services/file_limits.rs` keeps large-file thresholds as named, documented service-layer policy. `DraftReadError`, `WorkspaceWatchError`, `BookmarkEditError`, `JsonFormatError`, and `DurableWriteError` show useful workflow-specific names. The work should tighten the weak edges without creating a global constant bucket or enabling noisy Clippy policies.

## Goals / Non-Goals

**Goals:**

- Establish a durable naming policy for Rust error types that cross module, service, UI, or crate boundaries.
- Establish a numeric-literal policy that distinguishes semantic policy values from harmless inline literals.
- Promote only low-noise numeric literal Clippy lints that are clean or can be cleaned without suppressions.
- Keep `make lint-advisory` useful for discovery of noisy numeric lints such as `default_numeric_fallback`, `float_arithmetic`, and `integer_division`.
- Update `.agents/rules` and relevant skills so future Rust architecture, comment, lint, and review work repeats the policy consistently.

**Non-Goals:**

- Do not rename every existing error type mechanically.
- Do not ban all numeric literals or create a root-level `constants.rs` dumping ground.
- Do not enable `clippy::restriction`, `clippy::pedantic`, `clippy::nursery`, or `clippy::cargo` as blanket blocking groups.
- Do not create `clippy.toml` unless a global path-insensitive ban is proven safe.
- Do not change runtime behavior except where naming or constant extraction exposes an already intended policy.

## Decisions

### Error Types Name the Failing Workflow at Boundaries

Error types used only inside a tiny private helper may remain short, but public, `pub(crate)`, re-exported, service-facing, or UI-facing errors should include the workflow or domain they belong to. This keeps call sites searchable and avoids collisions as modules grow.

Examples of the target shape:

- `EditorLoadError` or `DocumentLoadError` is clearer than a cross-layer `LoadError`.
- `EditorSaveError` or `DocumentSaveError` is clearer than a re-exported `SaveError`.
- `ReplaceWriteError` is clearer than a private `AtomicWriteError` when Replace All needs distinct user-facing recovery behavior.
- `ProofValidationError` or `SchemaValidationError` is clearer than `ValidationError` if the validation type grows beyond one local model module.

Alternative considered: enforce a suffix-only rule such as every type must end in `Error`. Rejected because the existing code already does that where it matters; the real issue is domain specificity, not suffix presence.

### Numeric Literals Are Classified by Meaning, Not by Size Alone

The implementation should treat numeric literals as one of these categories:

```text
numeric literal
├─ identity/index/sentinel       inline is usually fine
├─ protocol or format constant   named const near protocol owner
├─ user-visible behavior policy  named typed const or policy value
├─ UI geometry/timing            local named const near widget/workflow owner
└─ test fixture data             inline is fine unless mirroring production policy
```

This policy keeps good local constants such as file-size thresholds and minimap budgets, while avoiding names that add no information, such as `ONE` or `ZERO`.

Alternative considered: ban numeric literals through custom scripts. Rejected because a ban would either need too many exceptions or force meaningless constants into tests, GTK geometry, generated code, and parser fixtures.

### Keep Constants Near the Owner

Named constants should live beside the workflow or domain policy that owns them. A service policy belongs in `services/` or `model/`; widget-only geometry belongs in the UI module; proof-tool artifact limits belong in `cargo-gtk-proof`. Shared constants should move inward only when multiple callers genuinely share one policy.

Alternative considered: centralize constants in one module. Rejected because it hides ownership and encourages unrelated policy values to change together.

### Curate Numeric Clippy Lints Individually

The exploration found no suitable direct Clippy `magic_numbers` lint. It also found that `default_numeric_fallback`, `float_arithmetic`, and `integer_division` produce high-volume advisory output in normal GTK, generated widget-test, and proof-tool code. Those should remain advisory unless a future cleanup proves a narrower path.

Low-noise literal-format lints may become blocking after cleanup. The implementation should evaluate at least:

- `clippy::decimal_literal_representation`;
- `clippy::large_digit_groups`;
- `clippy::decimal_bitwise_operands`;
- `clippy::lossy_float_literal`;
- `clippy::unused_rounding`;
- `clippy::mixed_case_hex_literals`;
- `clippy::zero_prefixed_literal`;
- `clippy::unusual_byte_groupings`;
- `clippy::inconsistent_digit_grouping`.

Alternative considered: promote every numeric lint in one change. Rejected because the noisy lints would obscure the actual policy and invite broad suppressions.

### Guidance Updates Are Part of the Change

The implementation must update `.agents/rules/rust.md` with the new error-naming and numeric-literal policy. It must update `.agents/rules/build.md` only if the advisory command or validation language changes. It must inspect relevant skills and update those whose triggers or review checklists would otherwise keep missing this policy, especially Rust architecture/comment/review guidance. If a `.agents/rules/*.md` file changes materially, the root `AGENTS.md` rules index must remain accurate.

Alternative considered: only update OpenSpec. Rejected because future agent behavior is driven by the rule and skill files as much as by canonical specs.

## Risks / Trade-offs

- Over-renaming stable public symbols -> Keep renames scoped to cross-boundary ambiguity and preserve compatibility where names are private enough not to matter.
- Constant extraction churn -> Extract only semantic policy values; leave simple indexes, counts, and fixture data inline.
- Clippy noise blocks useful work -> Promote only lints that are clean under the standard all-targets/all-features gate after cleanup.
- Guidance drift -> Run `make check-agent-docs` and `openspec validate --all --strict` before completion.
- Hidden behavior change from refactoring literals -> Require targeted tests when a literal extraction touches behavior, limits, UI geometry, or persistence policy.
