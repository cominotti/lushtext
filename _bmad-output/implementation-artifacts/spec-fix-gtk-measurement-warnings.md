---
title: 'Fix GTK measurement warnings via pre-clamping and architectural defenses'
type: 'bugfix'
created: '2026-04-09'
status: 'done'
baseline_commit: '5e08327'
context:
  - '.agents/rules/ui.md'
  - '.agents/rules/widget-wiring.md'
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** GTK emits "Trying to measure GtkBox for width of X, but it needs at least Y" warnings because `GtkPaned.measure()` distributes width based on the current `position`, which can be stale during the measure phase (runs BEFORE our `size_allocate` clamp). The most common triggers: (1) startup — position restored from GSettings may be too large for the actual window width; (2) fast window resize — position from the previous frame's width hasn't been clamped yet when the new frame's measure runs. The existing `clamp_sidebar_position` in `size_allocate` fixes the allocation but fires too late to prevent the measure-time warning.

**Approach:** Multi-layer defense: (1) pre-clamp the sidebar position in `constructed()` against the minimum window width (640px) AND the restored default width, storing the original saved value separately for use when the window is wider; (2) replace the hardcoded 16px separator buffer with the actual measured handle overhead; (3) set an explicit `width-request` on `content_box` equal to the stack's measured minimum, making the paned's minimum constraint visible in the widget tree; (4) add widget tests asserting the content-box invariant holds through panel lifecycle transitions; (5) update rules to codify the paned sizing defense pattern for future UI work.

## Boundaries & Constraints

**Always:**
- Keep `hhomogeneous=true` on `content_stack` (required for stable stack transitions)
- Keep `shrink-end-child=false` on `main_paned`
- The clamp must only REDUCE position, never increase it (user's saved preference is a ceiling, not a target)
- All existing clamp widget tests must pass unchanged
- Pre-clamp logic must not destroy the user's saved sidebar position at wider window widths

**Ask First:**
- If the pre-clamp approach requires changing the GSettings restore order in `constructed()`
- If widget test count exceeds 5 new tests

**Never:**
- Override `measure()` on the window (causes position ratcheting — documented in imp.rs:502-509)
- Set `hhomogeneous=false` on `content_stack`
- Use `notify::default-width` for clamping (fires before allocation — stale values)
- Hardcode the `AdwStatusPage` minimum width as a constant (must be queried dynamically)

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Startup, saved pos fits default width | GSettings pos=250, default_width=1200 | Position stays 250, no warning | N/A |
| Startup, saved pos too large for min width | GSettings pos=250, actual width=640 | Pre-clamp to safe value, no first-frame warning | N/A |
| Fast resize 1200→640 | pos=250 valid at 1200, stale at 640 | Clamp catches it in size_allocate; pre-clamp at min width prevents measure-time warning for startup case | One-frame warning possible during runtime resize (GTK limitation) |
| Sidebar show animation | pos animates 1→250 | Each tick clamped via notify::position | N/A |
| Sidebar hidden | sidebar_visible=false | Clamp skipped, content grows (no violation) | N/A |

</frozen-after-approval>

## Code Map

- `crates/lushtext-core/src/ui/window/imp.rs` -- `constructed()` (GSettings restore + pre-clamp), `size_allocate()` (existing clamp), `clamp_sidebar_position()` (separator buffer fix + content_box width-request)
- `resources/ui/window.ui` -- content_box, content_stack, main_paned template properties
- `crates/lushtext/tests/widget/window.rs` -- existing clamp tests + new invariant tests
- `.agents/rules/ui.md` -- add paned sizing defense pattern
- `.agents/rules/widget-wiring.md` -- add measure-before-allocate documentation

## Tasks & Acceptance

**Execution:**
- [x] `crates/lushtext-core/src/ui/window/imp.rs` -- In `constructed()`, after restoring sidebar position from GSettings (line 236), add pre-clamp logic: query `content_stack.measure(Horizontal, -1).0` for `stack_min`, compute `safe_max = min(default_width / 3, default_width - stack_min - handle_overhead)` where `default_width = settings.int(WINDOW_WIDTH)`, clamp `saved_pos` to `safe_max`. Store original unclamped pos in `saved_sidebar_pos` for show-animation target at wider widths.
- [x] `crates/lushtext-core/src/ui/window/imp.rs` -- In `clamp_sidebar_position()`, replace `16` with actual handle overhead: `main_paned.measure(Horizontal, -1).0 - sidebar.measure(Horizontal, -1).0 - content_stack.measure(Horizontal, -1).0`, cached in a `Cell<i32>` on the imp struct to avoid re-measuring every frame.
- [x] `crates/lushtext-core/src/ui/window/imp.rs` -- In `constructed()`, after pre-clamp, set `content_box.set_width_request(stack_min)` to make the paned minimum constraint explicit in the widget tree.
- [x] `crates/lushtext/tests/widget/window.rs` -- Add test: `test_content_box_width_request_matches_stack_min` — asserts `content_box.width_request() >= content_stack.measure(H, -1).0` after construction.
- [x] `crates/lushtext/tests/widget/window.rs` -- Add test: `test_pre_clamp_safe_for_narrow_window` — constructs window, verifies `main_paned.position()` leaves at least `stack_min + handle_overhead` for content when default width is narrow.
- [x] `crates/lushtext/tests/widget/window.rs` -- Add test: `test_pre_clamp_preserves_wide_position` — constructs window with default width 1200 and pos 250, verifies position is not reduced.
- [x] `.agents/rules/ui.md` -- Add "Paned Sizing Defense" section documenting: (1) the measure-before-allocate timing gap, (2) the pre-clamp pattern for position restore, (3) the content_box width-request invariant, (4) why 16px was replaced with dynamic measurement.
- [x] `.agents/rules/widget-wiring.md` -- Add "GtkPaned Position Constraints" section: any code that sets a paned position must ensure it's valid for the current allocation width; document the pre-clamp pattern for GSettings restore.

**Acceptance Criteria:**
- Given a fresh launch with default GSettings (pos=250, width=1200), when the window opens, then no "Trying to measure" warnings appear in stderr
- Given a window restored at narrow width (640px) with saved pos=250, when the window opens, then position is pre-clamped and no first-frame measurement warning appears
- Given a window at 1200px with sidebar at 250, when the user hides sidebar, shows it again, then position returns to 250 (saved_sidebar_pos preserved)
- Given `make test-widget`, when clamp tests run, then all existing + new tests pass

## Verification

**Commands:**
- `make test-widget` -- expected: all widget tests pass (existing + 3 new)
- `make check` -- expected: no clippy warnings, fmt clean
- `make run 2>&1 | grep -i "trying to measure"` -- expected: no output during normal startup and sidebar toggle

## Suggested Review Order

**Core fix — pre-clamp and dynamic handle overhead**

- Entry point: pre-clamp logic after GSettings restore, handle overhead computation, content_box width-request
  [`imp.rs:240`](../../crates/lushtext-core/src/ui/window/imp.rs#L240)

- Runtime clamp now uses cached handle_overhead instead of hardcoded 16
  [`imp.rs:631`](../../crates/lushtext-core/src/ui/window/imp.rs#L631)

- Sidebar visibility restore no longer overwrites saved_sidebar_pos with clamped value
  [`imp.rs:318`](../../crates/lushtext-core/src/ui/window/imp.rs#L318)

**Rules and documentation**

- New "Paned Sizing Defense" section documenting the three-layer pattern
  [`ui.md:122`](../../.agents/rules/ui.md#L122)

- New "GtkPaned Position Constraints" section for future widget work
  [`widget-wiring.md:38`](../../.agents/rules/widget-wiring.md#L38)

- AGENTS.md updated sidebar position constraint description
  [`AGENTS.md:91`](../../.agents/AGENTS.md#L91)

**Tests**

- content_box width-request invariant test
  [`window.rs:1922`](../../crates/lushtext/tests/widget/window.rs#L1922)

- Pre-clamp safe for narrow window (with hard assertions on preconditions)
  [`window.rs:1938`](../../crates/lushtext/tests/widget/window.rs#L1938)

- Pre-clamp preserves position at wide window
  [`window.rs:1968`](../../crates/lushtext/tests/widget/window.rs#L1968)
