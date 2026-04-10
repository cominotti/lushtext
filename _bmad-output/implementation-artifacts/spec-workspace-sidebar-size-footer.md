---
title: 'Workspace Sidebar Footer Size Presets'
type: 'feature'
created: '2026-04-10'
status: 'done'
baseline_commit: 'ddf20a76b93f543019a4bff21d3656e5affb1d98'
review_decision_pending: false
context: ['AGENTS.md', '.agents/rules/ui.md', '_bmad-output/project-context.md']
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** The left workspace sidebar currently has only the fixed top "New Workspace" affordance and a horizontally scrollable middle section. That makes the bottom edge of the sidebar feel unfinished, forces the panel controls to move away while scrolling, and gives users no direct way to choose a denser or roomier sidebar width.

**Approach:** Add a fixed bottom footer that visually matches the top affordance row and contains three centered width-preset buttons: `Small` (20%), `Comfy (30%)`, and `Large` (40%). Those presets become the only supported total-window width targets for the left pane, while the outer sidebar horizontal scrollbar is removed and the nested split-view math stays coherent when the right properties pane is also visible.

## Boundaries & Constraints

**Always:** Keep the existing top "New Workspace" row pinned above the scroll area; add an equally fixed bottom footer below the scroll area with the same row height and overall visual treatment; center the three preset controls vertically and horizontally within that footer; make the selected preset visibly active; persist the chosen left-pane width through the existing split-view state so the same preset restores on restart; treat the selected left-pane preset as a total-window fraction, not a local nested-split fraction; recompute the right-pane inner fraction and the properties-collapse breakpoint from the active left preset so the editor column keeps the current protected minimum width on medium and wide windows; remove the sidebar's horizontal scrollbar entirely; keep the change free of new GTK sizing or focus warnings.

**Ask First:** If implementing the no-horizontal-scroll requirement exposes a broader long-label redesign need beyond simple viewport clipping; if the requested preset labels, order, or percentages need to change; if protecting the editor column requires changing the right-pane default target rather than only its derived inner fraction and collapse threshold.

**Never:** Do not move the workspace creation affordance into the scrollable region; do not add freeform draggable widths or extra width controls outside the sidebar footer; do not reintroduce horizontal scrolling through a different container; do not change the right properties toggle placement or the existing status-bar toggle model.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Restore preset | App starts with stored left sidebar width preference of `0.2`, `0.3`, or `0.4`, or with an older quarter-width value such as `0.25` | The matching footer button is active and the visible left pane restores to the corresponding total-window fraction; fresh installs default to `Comfy (30%)` | Clamp unexpected stored values to the nearest supported preset, with midpoint ties resolving to `Comfy (30%)` so existing quarter-width installs do not shrink unexpectedly |
| Change width on wide shell | User clicks `Large` while both sidebars are visible on a wide window | Left pane expands to 40% of the total window width, the right pane recomputes its inner fraction from the remaining width, and the properties breakpoint updates so the editor still collapses the right pane before becoming too narrow | If the window is already below the recalculated breakpoint, the properties pane overlays/collapses rather than forcing an undersized editor |
| Fixed footer while scrolling | Sidebar contains enough workspaces to scroll vertically | The top "New Workspace" row and the new bottom preset footer stay fixed while only the workspace list scrolls between them | N/A |
| No horizontal scroll | Long workspace or file labels extend past the visible sidebar width | No horizontal scrollbar is shown anywhere in the left sidebar; overflow stays clipped to the visible viewport instead of enabling sideways scrolling | N/A |

</frozen-after-approval>

## Code Map

- `resources/ui/sidebar.ui` -- sidebar template; add the fixed bottom footer, its separator, and the three preset buttons while removing the outer horizontal scrollbar policy.
- `crates/lushtext-core/src/ui/sidebar/imp.rs` -- template children and button wiring for the new footer controls.
- `crates/lushtext-core/src/ui/sidebar/mod.rs` -- sidebar API surface for preset-selection callbacks and active-button synchronization from window state.
- `crates/lushtext-core/src/ui/window/imp.rs` -- source of truth for left-pane fractions, nested right-pane math, dynamic breakpoint updates, restore/persist behavior, and size-allocation resync.
- `data/dev.cominotti.lushtext.gschema.xml` -- set the left-pane default to the `Comfy (30%)` preset while keeping the right-pane defaults unchanged.
- `crates/lushtext/tests/widget/sidebar.rs` -- widget coverage for fixed footer placement, row-height parity, centered controls, and the removed horizontal scrollbar.
- `crates/lushtext/tests/widget/window.rs` -- shell-level coverage for preset restoration, split-view fraction math, and properties-collapse behavior under each supported left-pane preset.

## Tasks & Acceptance

**Execution:**
- [x] `resources/ui/sidebar.ui`, `crates/lushtext-core/src/ui/sidebar/imp.rs`, `crates/lushtext-core/src/ui/sidebar/mod.rs` -- add a fixed footer row with three mutually exclusive preset buttons, remove sidebar horizontal scrolling, and expose a clean callback/update API between the sidebar widget and the window -- keeps the control surface local to the sidebar without letting the sidebar own split-view geometry.
- [x] `crates/lushtext-core/src/ui/window/imp.rs`, `data/dev.cominotti.lushtext.gschema.xml` -- replace the hardcoded left quarter-width normalization with the supported `20% / 30% / 40%` preset set, default fresh installs to `Comfy (30%)`, snap older quarter-width values to that middle preset, persist and restore the chosen preset, and recalculate the right-pane fraction plus properties breakpoint from the active left preset -- preserves the window-shell invariants after introducing a wider 40% left pane.
- [x] `crates/lushtext/tests/widget/sidebar.rs`, `crates/lushtext/tests/widget/window.rs` -- add widget and shell tests for fixed footer placement, selected preset state, no-horizontal-scroll behavior, and editor-safe two-sidebar geometry -- keeps this change deterministic and regression-resistant.

**Acceptance Criteria:**
- Given the sidebar is visible, when the user scrolls a long workspace list, then the top "New Workspace" affordance and the new bottom size-preset footer both remain fixed while only the middle workspace list scrolls.
- Given the user selects `Small`, `Comfy (30%)`, or `Large`, when the left pane is visible, then it uses exactly `20%`, `30%`, or `40%` of the total window width respectively and restores the same preset after restart.
- Given both sidebars are visible, when the left preset changes, then the right pane still derives its nested split fraction from the remaining width and collapses early enough to preserve the editor column's existing minimum-safe width.
- Given long workspace or file names exceed the visible sidebar width, when the sidebar renders, then no horizontal scrollbar appears in the left pane.

## Design Notes

The risky part of this change is not the footer itself; it is the geometry contract. The left pane can no longer be modeled as a single hardcoded fraction, so the window layer must own a small preset model and drive all split-view calculations from it.

The cleanest boundary is:

```text
sidebar footer buttons -> sidebar callback -> window preset update
window preset update -> split-view fractions + breakpoint condition + persisted state
window state -> sidebar active button sync
```

That keeps the sidebar responsible for presenting controls and the window responsible for shell layout rules.

## Verification

**Commands:**
- `cargo fmt --all` -- expected: formatting succeeds with no diffs left behind
- `cargo clippy --all-targets -- -D warnings` -- expected: lint-clean build
- `xvfb-run -a make test-widget` -- expected: widget test suite passes, including the updated sidebar and window shell coverage

## Review Findings Log

### 2026-04-10T16:22:51-03:00

- Reviewers used: Blind hunter via `bmad-review-adversarial-general`, Edge case hunter via `bmad-review-edge-case-hunter`, Acceptance auditor against the approved spec and context docs
- `intent_gap`: none
- `bad_spec`: none
- `patch`: none
- `defer`: none
- `reject`: none
- Highest-priority category: `none`
- Required human decision: `[C] Continue` | `[S] Stop for later`

### 2026-04-10T16:32:54-03:00

- Reviewers used: Blind hunter via `bmad-review-adversarial-general`, Edge case hunter via `bmad-review-edge-case-hunter`, Acceptance auditor against the approved spec and context docs
- `intent_gap`: none
- `bad_spec`: none
- `patch`:
  - `crates/lushtext-core/src/ui/window/imp.rs` -- the properties breakpoint is still too permissive when the workspace pane is hidden because it assumes the right pane can stay at a quarter-width target even in the width range where the 260sp minimum dominates; the guard needs to protect the editor minimum width in both the hidden-workspace and visible-workspace cases
  - `crates/lushtext/tests/widget/window.rs` -- several shell tests still seed the old `0.25` geometry assumptions and need to be realigned to the supported preset set plus the corrected breakpoint math
  - `resources/ui/sidebar.ui` -- the new footer button labels need `translatable=\"yes\"` so the new visible controls do not regress localization
- `defer`: none
- `reject`:
  - `crates/lushtext-core/src/ui/window/imp.rs` -- the reported right-pane inner-fraction regression was not accepted because `effective_properties_fraction()` still converts the total-width target into the nested inner fraction of the remaining width
- Highest-priority category: `patch`
- Required human decision: `[A] Apply the classified patch findings` | `[S] Stop for later`

## Suggested Review Order

**Preset Surface**

- See the fixed footer and no-scroll sidebar contract first.
  [`sidebar.ui:49`](../../resources/ui/sidebar.ui#L49)

- Follow how the sidebar turns button clicks into a preset model.
  [`mod.rs:25`](../../crates/lushtext-core/src/ui/sidebar/mod.rs#L25)

- Check the template wiring that keeps one preset active.
  [`imp.rs:87`](../../crates/lushtext-core/src/ui/sidebar/imp.rs#L87)

**Layout Enforcement**

- Start at the shell sync path that locks width and fan-outs updates.
  [`imp.rs:864`](../../crates/lushtext-core/src/ui/window/imp.rs#L864)

- This helper makes the left pane truly fixed against content growth.
  [`imp.rs:773`](../../crates/lushtext-core/src/ui/window/imp.rs#L773)

- This is the corrected breakpoint guard for both hidden and visible left-pane cases.
  [`imp.rs:727`](../../crates/lushtext-core/src/ui/window/imp.rs#L727)

- Fresh installs now land on the middle preset.
  [`dev.cominotti.lushtext.gschema.xml:82`](../../data/dev.cominotti.lushtext.gschema.xml#L82)

**Regression Coverage**

- These shell tests prove button clicks lock real widths, not just settings values.
  [`window.rs:351`](../../crates/lushtext/tests/widget/window.rs#L351)

- These collapse tests cover wide, large-preset, and hidden-workspace geometry.
  [`window.rs:383`](../../crates/lushtext/tests/widget/window.rs#L383)

- These sidebar tests pin the fixed footer and the removed horizontal scrollbar.
  [`sidebar.rs:51`](../../crates/lushtext/tests/widget/sidebar.rs#L51)

**Doc Sync**

- The UI rulebook now documents the footer and fixed-width sidebar behavior.
  [`ui.md:21`](../../.agents/rules/ui.md#L21)

- The architecture guide now matches the live sidebar shell.
  [`AGENTS.md:96`](../../AGENTS.md#L96)
