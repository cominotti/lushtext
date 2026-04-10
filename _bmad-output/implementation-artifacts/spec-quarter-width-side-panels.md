---
title: 'Quarter-width side panels with mirrored bottom toggles'
type: 'feature'
created: '2026-04-10'
status: 'done'
baseline_commit: 'b041fea62ef894bdba39de1df6a8b9039b4833bd'
review_decision_pending: false
context:
  - '.agents/rules/ui.md'
  - '.agents/rules/widget-wiring.md'
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** The current dual-sidebar shell still treats the left and right panels as independently clamped utility panes instead of a balanced layout. The left sidebar also truncates long content, keeps the "New Workspace" affordance at the bottom, and the right-pane toggle lives in the header bar instead of mirroring the left toggle in the status bar.

**Approach:** Rework the shell so both visible side panes resolve to a fixed quarter-width layout, move the right-pane toggle into the status bar as the far-right control, and restructure the left sidebar so "New Workspace" leads the panel while long labels stay fully readable via horizontal scrolling instead of ellipsis.

## Boundaries & Constraints

**Always:** Keep the nested `AdwOverlaySplitView` shell and the current collapse order, but enforce an exact total-window `25% / 50% / 25%` layout whenever both side panes are visible in split mode, even if the nested right split view needs a different internal fraction to achieve it. Remove left-sidebar truncation in workspace headers, footer affordances, and file-tree rows, and make horizontal overflow scroll instead of clipping. Move the "New Workspace" affordance from the bottom footer to the top of the left panel. Keep the right-pane toggle wired to `win.toggle-properties`, but render it in the bottom status bar as the rightmost control, visually mirroring the existing left toggle.

**Ask First:** If an exact quarter-width rule proves impossible with `AdwOverlaySplitView` on a supported breakpoint without introducing GTK layout warnings, or if keeping fully untruncated sidebar content would force a broader design change than horizontal scrolling.

**Never:** Reintroduce custom `GtkPaned` choreography for the side panels, keep a duplicate properties toggle in the header bar, preserve persisted custom sidebar widths as a compatibility goal, or solve long sidebar labels by re-adding ellipsis.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Balanced desktop layout | Wide window with one or both side panels visible | Each visible side panel occupies one quarter of the total window width, leaving the center host with the remaining space | Width persistence normalizes to the fixed quarter fraction instead of drifting from stale saved values |
| Long sidebar content | Workspace names or file names exceed the visible pane width | Labels remain fully rendered and the left panel exposes a horizontal scrollbar so content can be inspected without truncation | Overflow must not crash or create clipped invisible text; the panel should stay scrollable |
| Bottom-bar toggle symmetry | User toggles the properties pane from the status bar | The rightmost status-bar button drives `win.toggle-properties`, mirrors the left toggle placement, and the header bar no longer shows a competing properties toggle | Focus restoration after pane close must still land on the editor or clear safely when no editor exists |

</frozen-after-approval>

## Code Map

- `resources/ui/window.ui` -- Remove the header-bar properties toggle and keep the shell rooted in the same split-view structure
- `resources/ui/status-bar.ui` -- Add the right-side properties toggle and rebalance the metadata/toggle layout for visual symmetry
- `crates/lushtext-core/src/ui/status_bar/imp.rs` and `crates/lushtext-core/src/ui/status_bar/mod.rs` -- Expose the new toggle widget to tests and preserve metadata/message behavior
- `crates/lushtext-core/src/ui/window/imp.rs` and `crates/lushtext-core/src/ui/window/mod.rs` -- Enforce the fixed quarter-width sidebar fractions, normalize restored settings, and keep split-view action state/focus handling intact
- `resources/ui/sidebar.ui` and `crates/lushtext-core/src/ui/sidebar/imp.rs` -- Move the "New Workspace" affordance to the top of the sidebar and update template children accordingly
- `resources/ui/workspace-section.ui` and `crates/lushtext-core/src/ui/sidebar/workspace_section/imp.rs` -- Stop ellipsizing workspace/file labels and allow horizontal overflow to participate in scrolling
- `crates/lushtext/tests/widget/status_bar.rs`, `crates/lushtext/tests/widget/sidebar.rs`, and `crates/lushtext/tests/widget/window.rs` -- Replace layout assumptions with coverage for mirrored toggles, top-mounted new-workspace controls, and fixed quarter-width panes
- `AGENTS.md` and `.agents/rules/ui.md` -- Sync the documented window/sidebar/status-bar hierarchy with the new panel behavior

## Tasks & Acceptance

**Execution:**
- [x] `resources/ui/window.ui` -- remove the header-bar properties toggle so the bottom bar becomes the single visible entry point for that action
- [x] `resources/ui/status-bar.ui`, `crates/lushtext-core/src/ui/status_bar/imp.rs`, and `crates/lushtext-core/src/ui/status_bar/mod.rs` -- add a rightmost `win.toggle-properties` button and keep metadata laid out between the message area and that mirrored control
- [x] `crates/lushtext-core/src/ui/window/imp.rs` and `crates/lushtext-core/src/ui/window/mod.rs` -- replace min/max-driven sidebar width restoration with fixed quarter-width enforcement while preserving visibility persistence, breakpoints, and focus restoration
- [x] `resources/ui/sidebar.ui`, `crates/lushtext-core/src/ui/sidebar/imp.rs`, and `crates/lushtext-core/src/ui/sidebar/mod.rs` -- move the "New Workspace" affordance above the scrollable workspace list without breaking existing create-workspace wiring
- [x] `resources/ui/workspace-section.ui` and `crates/lushtext-core/src/ui/sidebar/workspace_section/imp.rs` -- remove sidebar label ellipsizing and enable horizontal scrolling so long workspace and file names remain fully readable
- [x] `crates/lushtext/tests/widget/status_bar.rs`, `crates/lushtext/tests/widget/sidebar.rs`, and `crates/lushtext/tests/widget/window.rs` -- add regression coverage for the new toggle placement, top-mounted new-workspace affordance, and quarter-width split-view behavior
- [x] `AGENTS.md` and `.agents/rules/ui.md` -- update the contributor-facing shell/sidebar/status-bar documentation to match the final layout

**Acceptance Criteria:**
- Given a wide window with the left sidebar visible, when the shell lays out its split views, then the workspace sidebar width resolves to one quarter of the total window width instead of the old clamped band
- Given a wide window with both side panels visible, when the properties pane opens, then the left and right panes each occupy one quarter of the window and the center content keeps the remaining half
- Given long workspace names or file-tree labels, when they exceed the pane width, then they remain fully readable and the left sidebar provides horizontal scrolling instead of ellipsizing them
- Given the status bar is rendered, when the user looks at its outer edges, then the left sidebar toggle remains at the far left and the properties toggle appears at the far right beside the metadata area with no duplicate header-bar toggle
- Given the window crosses the existing split-view breakpoints, when the panes collapse into overlay mode and are closed again, then the properties pane still collapses before the workspace pane and focus restoration remains correct

## Spec Change Log

- 2026-04-10T04:34:05-03:00 — Review clarification replaced the frozen "`0.25` on both split views" wording with the actual user requirement: exact total-window `25% / 50% / 25%` when both panes are visible. This avoids the nested-split under-sizing that would leave the right pane at roughly `18.75%` of the full window while preserving the mirrored bottom toggles, top-mounted New Workspace affordance, and non-truncating horizontal-scroll behavior.

## Review Findings Log

### 2026-04-10T04:34:05-03:00

- Reviewers used: blind hunter, edge case hunter, acceptance auditor
- intent_gap:
  - The approved frozen guidance says to enforce a `0.25` fraction on both split views, but the acceptance audit found that the nested `properties_split_view` interprets `0.25` relative to the outer content area, producing about `18.75%` of the total window when both panes are visible. The human intent needs confirmation: exact total-window `25% / 50% / 25%`, not literal `0.25` on both split views.
- patch:
  - The Save As blocker fix now performs draft cleanup synchronously on the UI thread in `crates/lushtext-core/src/ui/window/dialogs.rs:97-104`, which can stall the main loop on slow storage.
- reject:
  - Blind hunter reported no real findings.
- Highest-priority category: `intent_gap`
- Human decision required next: Confirm whether the required layout is exact total-window `25% / 50% / 25%` even if the nested split views need different internal fractions to achieve it. If confirmed, revert the current code, refresh the frozen spec through step 2, and re-derive the implementation. After that, decide whether to apply the Save As cleanup patch finding or stop for later.

## Verification

**Commands:**
- `cargo test -p lushtext --test widget status_bar sidebar window -- --nocapture` -- expected: layout, toggle, and sidebar regressions stay green
- `cargo test -p lushtext --test widget` -- expected: the wider widget suite still passes with the new shell/layout behavior
- `cargo fmt --check` -- expected: formatting is clean
- `cargo clippy --all-targets -- -D warnings` -- expected: no new warnings

**Manual checks (if no CLI):**
- `make run` -- expected: both visible side panes settle at quarter width on wide windows, long left-sidebar labels stay readable via horizontal scrolling, "New Workspace" is at the top, the properties toggle lives only in the status bar, and stderr stays free of GTK layout warnings

## Suggested Review Order

**Shell Layout**

- Compute the right pane from the remaining width, not the nested split's raw fraction.
  [`imp.rs:320`](../../crates/lushtext-core/src/ui/window/imp.rs#L320)

- Keep the split fractions synchronized during resize, breakpoint, and toggle churn.
  [`imp.rs:493`](../../crates/lushtext-core/src/ui/window/imp.rs#L493)

**Bottom Bar**

- Move the properties control into the mirrored far-right slot of the status bar.
  [`status-bar.ui:45`](../../resources/ui/status-bar.ui#L45)

- Expose the new right toggle to the widget layer without changing status-bar behavior.
  [`imp.rs:9`](../../crates/lushtext-core/src/ui/status_bar/imp.rs#L9)

**Sidebar Overflow**

- Pin the New Workspace affordance above the scroller and enable horizontal overflow.
  [`sidebar.ui:13`](../../resources/ui/sidebar.ui#L13)

- Remove file-row ellipsis so long names stay readable instead of clipped.
  [`imp.rs:156`](../../crates/lushtext-core/src/ui/sidebar/workspace_section/imp.rs#L156)

**Verification**

- Assert true total-window quarter widths instead of the inner split's raw fraction.
  [`window.rs:201`](../../crates/lushtext/tests/widget/window.rs#L201)

- Cover the fixed top affordance and outer horizontal scroller contract.
  [`sidebar.rs:22`](../../crates/lushtext/tests/widget/sidebar.rs#L22)

- Record the updated shell contract for future contributors and reviewers.
  [`AGENTS.md:96`](../../AGENTS.md#L96)

- Mirror the same hierarchy and panel rules in the canonical UI rulebook.
  [`ui.md:21`](../../.agents/rules/ui.md#L21)
