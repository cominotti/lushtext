## Why

Phase 2 of the formal-verification programme listed four more pure policies
for Kani, and the consolidation left them all open:

- the editor-memory budget;
- the minimap fit functions;
- the adaptive-shell geometry;
- the workspace-sidebar width presets.

It also left a known violation of a documented rule: `clamped_preview_width`
exceeds its "≤ 1/3 of the content width" rule when the available width is
below 3·MIN. These functions have the same shape as the slice geometry Kani
already proves: pure arithmetic on pixels or byte counts, with no I/O. So
harness cost is low. The minimap and shell geometry have the longest visual-bug
history in the project, and the memory budget guards user work. This is
candidate N2 of `docs/next/formal-verification-next.md`, step 4 of its
suggested order. It builds on `harden-kani-lane-and-draft-token`, which
supplies the shard budgets, the runner measurement mode, and `kani_proofs.rs`
recognition.

## What Changes

- **Editor-memory budget harnesses** (`model/editor_memory/kani_proofs.rs`).
  They prove, over every `u64` estimate and generation and up to three pages:
  - `evaluate_editor_memory_budget` never selects a protected page or one at
    or below the bookkeeping floor;
  - it selects least recently used first, and stops as soon as the low
    watermark is reached (hysteresis);
  - its outcome agrees with the projected total;
  - `EditorResidencyLedger` totals agree with a saturating recomputation over
    bounded upsert and remove sequences;
  - `crossed_upper_threshold` is exact.
- **Minimap fit harnesses** (`ui/editor_page/minimap/policy/kani_proofs.rs`).
  - No panic for any `f64` in any of these: `fit_native_slider_to_source_map_bounds`,
    `fit_marker_bounds`, `fit_projected_bounds`, `expanded_to_min_height`, and
    `native_slider_estimate_from_inputs`.
  - Containment in the rendered-content band, and the minimum-height guarantee,
    proved on a stated whole-pixel domain.
  - A kept `should_panic` counterexample wherever the general-`f64` form fails,
    following the slice-geometry precedent.
- **Adaptive-shell harnesses** (`ui/window/geometry/policy/kani_proofs.rs`),
  over every `i32` window width, preset, and intent:
  - `derive_adaptive_shell_layout` renders nothing in Focus Mode;
  - it never renders an unrequested surface;
  - it renders at most one secondary surface in the compact presentation;
  - it keeps both requested surfaces in the wide presentation;
  - its sheet or pane choice matches the breakpoint threshold;
  - `properties_breakpoint_max_width_sp` is monotone and bounded;
  - `fixed_fraction` and `effective_properties_fraction` stay finite and in
    (0, 1].
- **Width-preset harnesses** (`ui/sidebar/width_preset/kani_proofs.rs`):
  - `clamped_width_sp` stays inside the preset bounds for every `i32` width,
    equals the spec formula, and is monotone;
  - `effective_fraction` stays in (0, 1];
  - the index and fraction round-trips hold, and `from_fraction` picks the
    nearest preset.
- **Fix `from_fraction` for non-finite input.** The harness is expected to find
  that a stored `NaN` or `-inf` resolves to `Large`. The workspace-sidebar key
  has no range in the schema, and GVariant text parses `nan` and `inf`. After
  the fix, a non-finite stored value resolves to the default preset, `Comfy`.
  The fix goes in failing-first.
- **Preview width: relocate, prove, and state the exception.** Move
  `clamped_preview_width`, `preferred_preview_width`, and their constants out
  of the GTK module `ui/window/preview.rs` into `ui/markdown_preview/policy.rs`,
  the pure-policy home of the `WFR-MARKDOWN-PREVIEW` row that owns
  `preview.rs`. There, harnesses prove the rule as it actually holds:
  - with at least 3 sp available, the width is at most one third of the
    available width;
  - below that, the 1 sp floor wins.

  A `should_panic` harness pins the unconditional form as false. The rule is
  written into the `adaptive-editor-geometry` spec for the first time. Behaviour
  is unchanged (design D5).
- **Whole pixels at the policy boundary, enforced mechanically** (maintainer
  decisions during implementation, design D8 and D9). The preview-width clamp
  becomes integer-only, converted to `f64` once at the split view. Every pure
  geometry or budget policy module carries `#![deny(clippy::float_arithmetic)]`,
  admitting a genuinely fractional value only through a reasoned
  function-level `#[expect]`, and `make check-workflow-boundaries` fails when a
  listed module or a new geometry `policy.rs` lacks the deny. New requirement
  in `workflow-readability-boundaries`.
- **Add a Kani shard.** The shard `core-pure-policies` enters the shard table
  with a runner measurement inside the budget. It is gated `pull-request` only
  if its measured cold wall time is at most 15 minutes. Otherwise it is
  `scheduled`, or split if it breaks a margin.
- **State every harness's input domain** in the harness and in the checked
  function's rustdoc.
- **Update the documentation.** Update:
  - the programme record (phase 2 status, results table, the preview decision);
  - the next-candidates doc (N2 done);
  - the workflow-readability matrix: the `WFR-MARKDOWN-PREVIEW` row gains
    policy, so its cells are re-derived; `WFR-SHELL-GEOMETRY` and
    `WFR-MINIMAP` gain harness modules;
  - `AGENTS.md`, `.agents/rules/build.md`, and README.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `live-editor-memory-budget`: new requirement. The budget, hysteresis, LRU,
  and ledger decisions are machine-checked on a stated domain.
- `editor-minimap`: new requirement. The minimap fit functions never panic,
  and they keep projections inside the rendered-content band on a stated
  domain.
- `adaptive-editor-geometry`: two new requirements. The shell-layout
  invariants are machine-checked. The side-by-side preview width is at most
  one third of the content width above its floor; this rule was previously
  documented only in AGENTS.md and rustdoc, and it now carries its 3 sp
  exception.
- `workflow-readability-boundaries`: new requirement. Pure geometry and budget
  policies do whole-pixel arithmetic, enforced by Clippy and the boundary
  check.
- `workspace-sidebar-width-policy`: one modified requirement and one new one.
  The modified "deterministic and persistent" requirement adds that a
  non-finite stored value resolves to the default preset. The new requirement
  says that the preset clamps and round-trips are machine-checked.

## Impact

- **Code:**
  - `crates/lushtext-core/src/model/editor_memory.rs`;
  - `ui/editor_page/minimap/policy.rs`;
  - `ui/window/geometry/policy.rs`;
  - `ui/sidebar/width_preset.rs` (the `from_fraction` fix);
  - `ui/markdown_preview/policy.rs` (it gains the preview-width clamp);
  - `ui/window/preview.rs`, `ui/window/imp.rs`, and
    `ui/window/geometry/execution.rs` (imports only);
  - new `kani_proofs.rs` children under each of the five modules.

  Each checked module gets only a `#[cfg(kani)] mod kani_proofs;` line and
  rustdoc domain statements. The only behaviour change is `from_fraction` on
  non-finite input.
- **Lane:** `scripts/kani-shards.py` gets one or two new shards, measured on
  the runner.
- **Mutation scope:** `ui/markdown_preview/policy.rs` gains the preview-width
  functions. This is a gain from zero and is reported as a gain.
  `kani_proofs.rs` files stay excluded by the previous change's glob.
- **Documentation:** `docs/next/formal-verification.md`,
  `docs/next/formal-verification-next.md`,
  `docs/workflow-readability-matrix.md`, `AGENTS.md`,
  `.agents/rules/build.md`, and `README.md`.
- **Dependency:** lands after `harden-kani-lane-and-draft-token`.
