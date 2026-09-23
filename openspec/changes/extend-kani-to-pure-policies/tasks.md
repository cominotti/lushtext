## 0. Preconditions

- [ ] 0.1 Confirm that `harden-kani-lane-and-draft-token` has landed: the shard table carries gate and budget fields, `kani-shards.py run --measure` exists, `**/kani_proofs.rs` is excluded from the mutation scope, and `check-workflow-boundaries.py` recognises a `cfg(kani)`-gated `kani_proofs.rs`. `make check-kani-shards` and `make check-workflow-boundaries` pass
- [ ] 0.2 Record the baseline mutation counts from `make mutants-list` for `ui/markdown_preview/policy.rs`, `ui/editor_page/minimap/policy.rs`, `ui/window/geometry/policy.rs`, and `model/editor_memory.rs`

## 1. Editor-memory budget harnesses

- [ ] 1.1 Add `#[cfg(kani)] mod kani_proofs;` to `model/editor_memory.rs` and create `model/editor_memory/kani_proofs.rs`. Its module doc states the domain: every `u64` estimate and generation, distinct ids in `0..3`, up to 3 pages, and `unwind(4)` (design D2)
- [ ] 1.2 Harnesses `estimate_is_bookkeeping_when_evicted_and_floored_by_file_size_otherwise`, `budget_never_selects_protected_or_bookkeeping_pages`, `budget_selects_least_recently_used_first`, `budget_stops_at_the_lower_watermark`, `budget_outcome_matches_the_projected_total`, and `within_budget_selects_nothing`, with `kani::cover!` for `WithinBudget`, `Converged`, and `NoProgress`
- [ ] 1.3 Harnesses `ledger_totals_match_a_recomputation` and `ledger_crossing_flag_is_exact`, over at most 3 upsert or remove operations on ids `{0, 1}`, plus agreement with `evaluate_editor_memory_budget`'s total. If the harness cannot meet the D7 budget, drop to 2 operations and state that in the doc comment
- [ ] 1.4 Run `cargo kani -p lushtext-core --harness model::editor_memory::kani_proofs::` locally. Every harness proves, or a counterexample is triaged: a code defect is fixed failing-first with a unit test in `editor_memory.rs`; a wrong property is corrected and the correction recorded. Record per-harness times
- [ ] 1.5 Add a sentence to the rustdoc of `evaluate_editor_memory_budget` and `EditorResidencyLedger` naming the Kani-proved properties and their domain

## 2. Minimap fit harnesses

- [ ] 2.1 Add `#[cfg(kani)] mod kani_proofs;` to `ui/editor_page/minimap/policy.rs` and create `policy/kani_proofs.rs`. Its module doc states both domains: any `f64` for no-panic, and whole pixels with magnitude at most 2^20 and minimum heights in 0..=2^16 for containment
- [ ] 2.2 No-panic harnesses over any `f64` and `i32`, for `fit_native_slider_to_source_map_bounds`, `fit_marker_bounds`, `fit_projected_bounds` (both `ProjectedBoundsFit` values), `expanded_to_min_height`, and `native_slider_estimate_from_inputs`. Each also asserts that any returned coordinate is finite
- [ ] 2.3 Containment and minimum-height harnesses, first over finite `f64` (design D2). For each, record whether Kani proves it. For each counterexample, keep it as `should_panic` (for example `marker_min_height_fails_for_general_f64`) and add a whole-pixel twin that proves it (for example `marker_bounds_stay_in_content_on_whole_pixels`)
- [ ] 2.4 Harnesses for `RejectOutside` (a span entirely outside the band returns `None`) and for the native slider (x and width preserved, vertically inside the source-map bounds), with `kani::cover!` for clamped-top, clamped-bottom, and expanded
- [ ] 2.5 Run the minimap harnesses locally. Triage every counterexample as in task 1.4. A production fix needs a failing-first unit test in `policy.rs`, plus `make visual-geometry-smoke` where the host supports it. Record per-harness times
- [ ] 2.6 Update the rustdoc of each fit function with its proved domain. No rustdoc may claim more than was proved

## 3. Adaptive-shell geometry harnesses

- [ ] 3.1 Add `#[cfg(kani)] mod kani_proofs;` to `ui/window/geometry/policy.rs` and create `policy/kani_proofs.rs`. Its domain: every `i32` width, every preset, every intent combination, and workspace widths in [0, 440] sp for the breakpoint harness
- [ ] 3.2 Harnesses `focus_mode_renders_no_secondary_surface`, `layout_never_renders_an_unrequested_surface`, `compact_layout_renders_at_most_one_surface`, `wide_layout_renders_every_requested_surface`, and `sheet_presentation_matches_the_breakpoint`, with `kani::cover!` for sheet and pane
- [ ] 3.3 Harnesses `breakpoint_is_monotone_and_bounded` (over `properties_breakpoint_max_width_sp`), `fractions_are_finite_and_in_unit_interval` (over `fixed_fraction`, `desired_properties_fraction`, `effective_properties_fraction`, and `effective_workspace_sidebar_fraction`), and `shell_policy_never_panics`
- [ ] 3.4 Run locally, triage as in task 1.4, and record times. Record in the harness module doc that independence from rendered state holds by construction (design D3)

## 4. Width presets: harnesses and the non-finite fix

- [ ] 4.1 Add `#[cfg(kani)] mod kani_proofs;` to `ui/sidebar/width_preset.rs` and create `width_preset/kani_proofs.rs`. Its domain: every `i32` width, every `f64` fraction, and every `u32` index
- [ ] 4.2 Harnesses `clamp_matches_the_spec_formula_and_bounds`, `clamp_is_monotone_in_window_width`, `effective_fraction_is_in_unit_interval`, `index_round_trips`, and `fraction_round_trips`
- [ ] 4.3 Failing first: add the harness `from_fraction_picks_the_nearest_preset` (non-finite input goes to `DEFAULT`, finite input to within `f64::EPSILON` of the nearest, with the tie order Comfy, Small, Large) and the unit test `non_finite_stored_fraction_resolves_to_default` (NaN, `-inf`, and `+inf` give `Comfy`). Run both against the unchanged code. Record the Kani counterexample, and confirm that the unit test fails because NaN and `-inf` give `Large`
- [ ] 4.4 Fix `from_fraction`: `if !fraction.is_finite() { return Self::DEFAULT; }`. The harness and the unit test now pass, and so do the existing `0.25 → Comfy` spec test and the width-policy tests
- [ ] 4.5 Update the reason of the `PROSE_CLASSIFIED_UNMUTATED` entry for `width_preset.rs` in `scripts/check-workflow-boundaries.py`: the module now holds the `from_fraction` decision, and it is Kani-proved but outside the mutation scope. `make check-workflow-boundaries` passes

## 5. Preview width: relocate, prove, state the exception

- [ ] 5.1 Failing first: in `ui/window/preview.rs`, as a local scratch step, write a plain `#[kani::proof]` of the unconditional rule `3 * clamped_preview_width(p, a) <= max(a, 1)`. Run it, record Kani's counterexample (available ≤ 2 sp), and delete the scratch harness
- [ ] 5.2 Move `clamped_preview_width`, `preferred_preview_width`, `PREVIEW_MIN_WIDTH_SP`, `PREVIEW_DEFAULT_WIDTH_SP`, and `PREVIEW_MAX_WIDTH_FRACTION` unchanged into `ui/markdown_preview/policy.rs` (design D6). Update the imports in `ui/window/preview.rs`, `ui/window/imp.rs`, and `ui/window/geometry/execution.rs`. `policy.rs` still has no GTK-family import, and `make check-workflow-boundaries` passes
- [ ] 5.3 Add the unit test `preview_width_floor_wins_below_three_sp`, where `(500, 2)` gives `1.0` and `(500, 900)` gives at most `300.0`. Rewrite the `clamped_preview_width` rustdoc to state the rule and its 3 sp exception
- [ ] 5.4 Add `#[cfg(kani)] mod kani_proofs;` to `ui/markdown_preview/policy.rs` and create `policy/kani_proofs.rs` (domain: every `i32` preferred and available width). Its harnesses: `preview_width_respects_the_floor`, `preview_width_is_at_most_a_third_above_three_sp` (exact comparison `3 * result <= available`), `preview_width_is_the_floor_below_three_sp`, `preview_width_keeps_an_in_band_preference`, `preview_width_is_monotone_in_preference`, and the `should_panic` harness `preview_width_is_not_always_a_third`. All pass as intended
- [ ] 5.5 Re-run `make mutants-list` and record the new mutant count of `ui/markdown_preview/policy.rs` as a gain from zero over the task 0.2 baseline. The other three baselines are unchanged, and no `kani_proofs.rs` mutant is listed

## 6. Lane and shard

- [ ] 6.1 Add the shard `core-pure-policies` to `scripts/kani-shards.py` with the five filters (design D7). `make check-kani-shards` fails until its budget fields are filled
- [ ] 6.2 Run `make kani KANI_SHARD=core-pure-policies` locally and record the wall time and peak memory
- [ ] 6.3 With the maintainer's go-ahead, push the branch and dispatch `gh workflow run kani.yml --ref <branch>`. Fetch the measurement artifact with `gh run download`, and fill `ci_minutes`, `ci_peak_gib`, and `measured_in`. If the shard breaks 25 minutes or 12 GiB, split it (the minimap harnesses become `core-minimap-policy`) and re-measure
- [ ] 6.4 Set the gate to `pull-request` if the measured cold wall time is at most 15 minutes, otherwise `scheduled`. `make check-kani-shards` passes. If the shard is gated `pull-request`, confirm on the change's pull request through `gh pr checks` that it ran and passed
- [ ] 6.5 `make kani` passes locally across every shard, and a final dispatched run passes every shard within `timeout-minutes: 30`

## 7. Documentation sync

- [ ] 7.1 `docs/next/formal-verification.md`, phase 2: replace "the editor-memory, minimap, window-geometry, and `clamped_preview_width` harnesses … remain open candidates" with a results table (harness, domain, result, local and CI time). Record the preview decision (D5), the `from_fraction` finding and fix, any other counterexample and its triage, and the new shard and its gate. Update the lane headline figures (harness count, shard count)
- [ ] 7.2 `docs/next/formal-verification-next.md`: mark N2 done, with a pointer to this change, and update §3's suggested order (N3 is next)
- [ ] 7.3 `docs/workflow-readability-matrix.md`: re-derive the `WFR-MARKDOWN-PREVIEW` row's measured cells (size, pure-policy consumers, seam counts), row-scoped and excluding `#[cfg(test)]` and `cfg(kani)` code, now that `policy.rs` owns the preview-width clamp. Note the harness modules in the `WFR-MINIMAP`, `WFR-SHELL-GEOMETRY`, and `WFR-EDITOR-MEMORY` rows. Run `make check-workflow-boundaries`
- [ ] 7.4 `AGENTS.md`: fix the Markdown preview bullet ("clamped to max 1/3 of the current content width") so it states the 1 sp floor below 3 sp and the new policy home. Note the Kani coverage in the live-editor memory, adaptive-shell, and minimap bullets, the `from_fraction` default for non-finite values in the sidebar-width text, and `ui/markdown_preview/policy.rs`'s new responsibility in the Module Layout
- [ ] 7.5 `.agents/rules/build.md` (Formal Verification): add `lushtext-core` `ui/**` policies to the placement list, and state the `policy/kani_proofs.rs` child-directory convention (design D1)
- [ ] 7.6 `README.md`: extend the formal-verification paragraph with the newly proved policies
- [ ] 7.7 Run `openspec validate extend-kani-to-pure-policies --strict`

## 8. Final verification

- [ ] 8.1 `make check`, `make check-policy`, and `make test` pass. Run `make test-widget` because `from_fraction` feeds the window and Preferences, and confirm that the sidebar width widget tests pass
- [ ] 8.2 The change's pull-request CI is green, including the Kani pull-request job or jobs
