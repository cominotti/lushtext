## Context

Kani already checks three crates' worth of code: the `gtk-lush-widgets` slice
geometry, the draft `journal_core`, and `write_protocol`. It builds
`lushtext-core` today: both core shards compile the whole crate, GTK
dependencies included, under Kani's nightly. So harnesses over `ui/**` pure
modules need no new build path. The one thing `ui/**` adds is the
workflow-boundaries gate. The previous change, `harden-kani-lane-and-draft-token`
(D9), teaches that gate to recognise a `cfg(kani)`-gated `kani_proofs.rs`, and
excludes that file from the mutation scope.

These are the five targets. Every one is GTK-free, I/O-free, and scalar:

| Target | Module | Owning row | Spec |
|---|---|---|---|
| Budget, hysteresis, LRU, ledger | `model/editor_memory.rs` | `WFR-EDITOR-MEMORY` (cross-cutting) | `live-editor-memory-budget` |
| Minimap fit functions | `ui/editor_page/minimap/policy.rs` | `WFR-MINIMAP` | `editor-minimap` |
| Shell layout and breakpoint | `ui/window/geometry/policy.rs` | `WFR-SHELL-GEOMETRY` | `adaptive-editor-geometry` |
| Width presets | `ui/sidebar/width_preset.rs` | `WFR-SHELL-GEOMETRY` (cross-cutting value) | `workspace-sidebar-width-policy` |
| Preview width clamp | `ui/window/preview.rs` (a GTK module) | `WFR-MARKDOWN-PREVIEW` (called presentation surface) | none yet; the rule lives in AGENTS.md and rustdoc |

`clamped_preview_width` computes this, with `PREVIEW_MIN_WIDTH_SP = 1.0` and
`PREVIEW_MAX_WIDTH_FRACTION = 1.0 / 3.0`:

```
max(preferred, MIN).min(max(floor(max(available, 1) * 1/3), MIN))
```

For an available width of 2 or less, `floor(available / 3)` is 0 and the
floor wins. The result is then 1 sp, which is more than one third of the
available width. That is the recorded violation. With MIN at 1 sp it is
reachable only when the content box is allocated 1–2 px wide, or before the
first allocation, when the fallbacks produce a width of 1.

## Goals / Non-Goals

**Goals:**

- Harnesses over the **real** functions for all five targets, each with a
  stated domain, run in the Kani lane within its measured budget.
- Resolve the preview "≤ 1/3" rule with a recorded decision, and write the rule
  into a spec for the first time.
- Fix, failing-first, every defect the harnesses find. One is predicted:
  `from_fraction` on non-finite input.

**Non-Goals:**

- The breakpoint feedback loop, allocated width → layout → breakpoint →
  allocated width. That is N3, and it needs new ledger axioms.
- Modelling GTK or Adwaita here. Every harness is over pure inputs.
- Moving `width_preset.rs` into a `policy.rs`. It stays a prose-classified,
  cross-cutting value. Its ledger entry is updated (D7).
- Proving the incremental enforcement scheduling of the memory budget.
  Scheduling is GTK coordination in `ui/window/editor_memory_eviction/`, not
  pure policy.

## Decisions

### D1. Harness placement: a `kani_proofs.rs` child of the checked module

Each checked module gets `#[cfg(kani)] mod kani_proofs;`, and the harness goes in
the child directory:

- `model/editor_memory/kani_proofs.rs`
- `ui/editor_page/minimap/policy/kani_proofs.rs`
- `ui/window/geometry/policy/kani_proofs.rs`
- `ui/sidebar/width_preset/kani_proofs.rs`
- `ui/markdown_preview/policy/kani_proofs.rs`

A child module can reach its parent's private items. That matters here, because
`expanded_to_min_height`, `fixed_fraction`, and
`properties_breakpoint_max_width_sp` are private, and the harnesses call them
directly instead of widening their visibility.

*Alternatives:*

- A sibling `kani_proofs.rs` next to `policy.rs`. It can reach only `pub(super)`
  items, so the private fit functions would have to be widened just for proofs.
- Inline `#[cfg(kani)] mod kani_proofs { … }` inside `policy.rs`. That bloats
  files of 800–2,400 lines. It also puts `cfg(kani)` code in a file inside the
  `ui/**/policy.rs` mutation glob, which the previous change's
  `**/kani_proofs.rs` exclusion cannot reach.

### D2. Domains: prove general `f64` where it holds, whole pixels where it does not

This follows the slice-geometry precedent (programme record, phase 2) and the
`formal-verification-kani` "state their input domain" requirement.

- **No-panic harnesses** use unrestricted `kani::any::<f64>()`, including NaN and
  ±inf, and `kani::any::<i32>()`. They cover every fit function,
  `native_slider_estimate_from_inputs`, and the shell-geometry functions. A
  `f64::clamp` with `lower > upper` or a NaN bound is the panic they rule out.
- **Containment and minimum-height harnesses** are first written over finite
  `f64`. If Kani proves one, the general domain is kept and stated. If Kani finds
  a rounding counterexample (the likely case for `y + height <= upper` and for
  the centred expansion), that harness is kept as a `should_panic`
  counterexample. A twin then proves the property on whole pixels:
  - integers with magnitude at most 2^20 for coordinates, and 0..=2^16 for
    minimum heights;
  - on that domain every sum, difference, midpoint (a half-integer), and
    half-height is exact in `f64`, so the property is a real theorem there;
  - the function's rustdoc states the domain.
- **Shell geometry** uses every `i32` width, every preset, and every
  `bool`/`Option` intent. The breakpoint monotonicity harness takes any finite
  `f64` workspace width in [0, 440] sp, which is the reachable range: 0 when the
  workspace does not consume width, or up to the Large maximum.
- **Editor memory** uses every `u64` estimate and generation, so saturation is
  exercised. Distinct `usize` editor ids come from `0..3`. A snapshot is a
  `[EditorResidency; 3]` array with a nondeterministic length in 0..=3, passed as
  a slice. The unwind bound is 4, covering the fold, filter, sort, and selection
  loops. The ledger harness uses at most 3 operations over ids `{0, 1}`. If
  `BTreeMap` makes that too expensive to meet the budget (D6), the bound drops
  to 2 operations and is stated.
- **Width presets** use every `i32` width and every `f64` for `from_fraction`.
- **Preview width** uses every `i32` preferred and available width. The
  one-third bound is proved as the exact integer comparison
  `3 * result <= max(available, 1)` in `i64`; since D8 the clamp itself is
  integer-only.

Each harness's doc comment names its domain, and a `kani::cover!` confirms that
the interesting branches are reachable. Those branches are:

- **fit functions:** clamped at top, clamped at bottom, and expanded;
- **memory budget:** `Converged`, `NoProgress`, and `WithinBudget`;
- **shell layout:** sheet and pane;
- **preview width:** the floor-wins branch.

### D3. The properties per target

The properties are those in the delta specs. The less obvious ones:

- **Memory LRU prefix and hysteresis.**
  - Every eligible, non-selected page with an estimate above the bookkeeping
    figure has an (access, id) key greater than every selected key, or else
    selection stopped at the watermark.
  - Minimality: `projected + last.reclaimable > LOWER_WATER`.

  Together these are "least recently used first, no more than needed", and
  that is the scenario "ordinary estimate noise near the upper threshold does
  not cause immediate repeated eviction".
- **Ledger agreement.** After each operation the harness recomputes the
  saturating total and the protected total from `snapshot()`, and compares them
  with `total_bytes()` and `protected_bytes()`. It also checks
  `crossed_upper_threshold`, and it checks that the ledger and
  `evaluate_editor_memory_budget` compute the same total from the same records.
  That checks the spec's "incremental accounting" against the full scan.
- **Compact exclusivity.**
  `properties_presentation == Sheet ⇒ !(render_workspace && render_properties)`.
  This is the machine-checked half of "Adaptive secondary surfaces settle from
  stable layout intent". The feedback-loop half is N3.
- **Shell purity from rendered state.** This holds by construction:
  `AdaptiveShellInputs` has no rendered-visibility field. It is recorded in the
  harness doc comment, not proved.

### D4. `from_fraction` on non-finite input: fix it, failing-first

`from_fraction` compares each preset's delta against the minimum delta with
`abs() < EPSILON`:

- for NaN, every delta is NaN, every comparison is false, and the function
  returns `Large`;
- for `-inf`, every delta is `+inf`, `inf − inf` is NaN, and again the function
  returns `Large`.

The key `workspace-sidebar-width-fraction` has no `<range>` in the schema, and
GVariant text syntax accepts `nan` and `inf`, so
`gsettings set dev.cominotti.lushtext workspace-sidebar-width-fraction nan`
reaches this code.

*Decision:* a non-finite value resolves to `Self::DEFAULT` (`Comfy`), which is
what a missing or corrupt setting should mean. The order is:

1. The harness `from_fraction_picks_the_nearest_preset` runs first. Its failure
   is recorded.
2. A unit test (`non_finite_stored_fraction_resolves_to_default`) fails for
   NaN and `-inf`.
3. The fix goes in.

`+inf` also becomes `Comfy`. That is a deliberate simplification over "nearest
end", because no finite nearest exists.

*Alternative:* add `<range min="0" max="1">` to the schema. GSettings would then
reject out-of-range writes, but a range on an existing key rejects any stored
value outside it, and users' stored values would fall back silently. It also
leaves the pure function wrong for other callers. The function fix is smaller
and total.

### D5. The preview "≤ 1/3" rule: state the exception, do not change behaviour

**Options:**

- *(a) Fix it.* Let the one-third bound win below 3 sp, which permits a 0 sp
  width.
- *(b) State it.* The 1 sp floor wins below 3 sp, and the spec says so.

**Decision: (b).** The reasons:

1. `PREVIEW_MIN_WIDTH_SP` is documented as a "tiny non-zero floor", and
   `geometry/execution.rs` sets both split-view bounds to it before the first
   width sync. Option (a) would introduce a zero sidebar constraint that nothing
   has exercised. That is a new state for an Adwaita container, with no probe
   behind it.
2. The violating domain is available widths of 2 sp or less. That is reachable
   only in a transient pre-allocation state, or with a 1–2 px content box,
   where no preview is readable either way. Fixing it changes nothing a user
   can see.
3. A rule that states its floor is the honest shape the programme already uses:
   the slice geometry proves "on whole pixels" and keeps the general-`f64`
   counterexample.

**What lands:**

- `clamped_preview_width` gets a new rustdoc and a new requirement in
  `adaptive-editor-geometry`.
- Proved harnesses cover the floor, the one-third bound for available widths of
  3 sp or more, preservation of the preference inside the band, and
  monotonicity in the preferred width.
- A `should_panic` harness, `preview_width_is_not_always_a_third`, finds the
  counterexample (available ≤ 2).
- The failing-first step: before the rustdoc and spec are corrected, the
  harness is written as a plain proof of the unconditional rule, and the
  implementer records Kani's counterexample. The unit test
  `preview_width_floor_wins_below_three_sp` pins `(preferred = 500,
  available = 2) → 1.0`.

### D6. Relocate the preview clamp into `ui/markdown_preview/policy.rs`

The `formal-verification-kani` spec wants the checked code GTK-free, and
`ui/window/preview.rs` imports GTK and Adwaita. So `clamped_preview_width`,
`preferred_preview_width`, `PREVIEW_MIN_WIDTH_SP`, `PREVIEW_DEFAULT_WIDTH_SP`,
and `PREVIEW_MAX_WIDTH_FRACTION` move into `ui/markdown_preview/policy.rs`, the
single `policy.rs` of `WFR-MARKDOWN-PREVIEW`, which owns `preview.rs`.
`preview.rs`, `imp.rs`, and `geometry/execution.rs` import them from there.

*Alternatives:*

- `ui/window/geometry/policy.rs`. It is pure, but it belongs to another row, and
  the preview pane is not one of `WFR-SHELL-GEOMETRY`'s surfaces.
- A new `preview_width.rs`. It would sit outside the `ui/**/policy.rs` mutation
  glob and would need a prose role. The convention forbids a workflow-prefixed
  policy file for exactly this reason.

*Consequences:*

- The relocation is a material restructure of `WFR-MARKDOWN-PREVIEW`. Its row
  re-derives its measured cells: size, pure-policy consumers, and seam counts.
- The mutation count of `ui/markdown_preview/policy.rs` rises. That rise is
  reported as a **gain from zero**, never as parity: `preview.rs` had 0 mutants
  in scope.

### D7. Lane: one shard, measured; the gate follows the budget rules

A new shard, `core-pure-policies` (package `lushtext-core`), takes the five
filters:

- `model::editor_memory::kani_proofs::`
- `ui::editor_page::minimap::policy::kani_proofs::`
- `ui::window::geometry::policy::kani_proofs::`
- `ui::sidebar::width_preset::kani_proofs::`
- `ui::markdown_preview::policy::kani_proofs::`

It is measured with the previous change's measurement mode on a dispatched
runner job, and its budget fields are filled from that run.

- **Gate.** It is gated `pull-request` if its measured cold wall time is at most
  15 minutes. These policies have the visual-bug history that justified gating
  `widgets-geometry`. Otherwise it is `scheduled`.
- **Split.** If it breaks 25 minutes or 12 GiB, it is split, with the `f64`-heavy
  minimap harnesses the likely candidate for a separate `core-minimap-policy`
  shard. Bounds are reduced only as the previous change's D3 allows.
- **Ledger entry.** `PROSE_CLASSIFIED_UNMUTATED` keeps `width_preset.rs`, but its
  reason is updated: the module now holds decision logic (the `from_fraction`
  fix), it is Kani-proved, and it is still outside the mutation scope.

### D8. Whole pixels at the policy boundary (maintainer decision, during implementation)

Pure geometry and budget policies in this change take and return **whole
pixels** (integers), and the conversion to `f64` happens once, at the GTK
adapter boundary, wherever the value is not inherently fractional.

- **Preview width: converted.** `clamped_preview_width` is integer-only
  (`available.max(1) / 3`, then the floor and the preference) and returns
  `i32`; `ui/window/preview.rs` converts it with `f64::from` where it sets the
  split view's constraints, and `PREVIEW_MIN_WIDTH_SP` is an `i32` that
  `geometry/execution.rs` converts the same way. The old
  `floor(available * (1.0 / 3.0))` was suspected of flooring `3k` to `k - 1`.
  It did not in the `i32` domain: `3k * (1/3)` rounds back to `k` for
  `k < 2^31`, which the in-band harness had already proved on the `f64` form,
  so the off-by-one is recorded as unreachable rather than as a defect. The
  integer form removed the `f64` multiply and floor that cost the preview
  harnesses up to 12 minutes each.
- **Inherently fractional, kept in `f64` with the domain stated:** the
  workspace-sidebar preset width (a hint fraction times the window width, fed
  back as a split fraction); the adaptive-shell fractions and the breakpoint's
  workspace-width input (derived from that preset width); and the minimap fit
  functions, whose inputs are GTK widget coordinates and scroll-adjustment
  values that are fractional under scaling and smooth scrolling. Each states
  its proved domain in its rustdoc and in the delta spec, as the slice-bin
  geometry does.
- **Editor memory** was integer arithmetic already.

### D9. Enforce the whole-pixel rule mechanically (maintainer decision, during implementation)

D8 is a rule a reviewer would otherwise have to remember, so it is held in two
layers:

- **Compiler.** Every whole-pixel policy module carries
  `#![deny(clippy::float_arithmetic)]` after its module documentation. A
  genuinely fractional value is admitted by a function-level
  `#[expect(clippy::float_arithmetic, reason = "...")]` naming the value and
  its domain; `expect` rather than `allow` because it fails once the function
  stops doing float arithmetic. The listed modules are the four this change
  proves (`ui/window/geometry/policy.rs`, `ui/editor_page/minimap/policy.rs`,
  `ui/markdown_preview/policy.rs`, `ui/sidebar/width_preset.rs`),
  `model/editor_memory.rs` (already integer; the deny keeps it so), and,
  because convention amendments apply retroactively, the two other migrated
  policies holding geometry: `ui/window/local_history/policy.rs` (viewer
  size, two admitted functions) and `ui/window/focus_mode/policy.rs` (a
  comparison only, no admission needed). The markdown-preview and
  editor-memory modules need no admission at all.
- **Policy check.** Rule 10 of `scripts/check-workflow-boundaries.py` declares
  `WHOLE_PIXEL_POLICY_MODULES` and `GEOMETRY_ROLE_HOMES` and fails on a missing
  deny, any `allow` of the lint, a module-wide `expect`, an `expect` without a
  reason, or a listed module that no longer exists; a new `policy.rs` under a
  geometry role home is covered without editing the list. Proved failing
  first: removing the deny from `ui/window/geometry/policy.rs` made the check
  fail naming the module, and reintroducing the old `f64` preview arithmetic
  made Clippy fail with `float_arithmetic`.
- **Not listed: `gtk-lush-widgets`' `slice_geometry.rs` and
  `scroll_request.rs`.** Every function in both is arithmetic on
  `GtkAdjustment` values, fractional under smooth scrolling, so the deny would
  need an `expect` on every function and would enforce nothing. Their
  whole-pixel domain is already stated and Kani-proved.
- **Normative home:** the "Whole-pixel geometry policy" section of
  `.agents/rules/workflow-convention.md`; `rust.md` and `AGENTS.md` point to
  it.

## Risks / Trade-offs

- **`f64` harnesses can be slow in CBMC.** `slice_covers_…` took 176.9 s on
  whole pixels. Mitigation: whole-pixel twins bound the search, the ledger
  harness has a fallback bound, and the shard is split if it is over budget.
- **A proved property can still be the wrong property.** Mitigation: each
  harness mirrors a spec sentence, and `kani::cover!` checks keep vacuous proofs
  visible.
- **Relocation churn in a migrated row.** Mitigation: move by value, with no
  signature change, and run `make check-workflow-boundaries`. Re-derive the
  row's cells from the tree, not from the census.
- **An unexpected counterexample in production code** (for example in
  `fit_native_slider_to_source_map_bounds`). Mitigation: the
  pre-existing-blockers rule applies. Write a failing-first unit test, fix the
  code, and record the finding in the programme record. If the fix is visual,
  also run the minimap visual-proof predicates (`make visual-geometry-smoke`
  where the host supports it).
- **`+inf` resolving to `Comfy` instead of `Large`** is a judgement call.
  Mitigation: it is spelled out in the spec.

## Migration Plan

No data migration and no persisted-format change. The only behaviour change is
`from_fraction` on non-finite stored values, which previously gave `Large` and
now gives `Comfy`. Rollback is to revert the change. The shard table entry goes
with it.

## Open Questions

None blocking. Whether `core-pure-policies` joins the pull-request gate is
decided by its measurement (D7), not in advance.
