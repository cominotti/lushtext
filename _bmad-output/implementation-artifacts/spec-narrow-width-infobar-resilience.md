---
title: 'Narrow-width infobar resilience with earlier center-width protection'
type: 'bugfix'
created: '2026-04-10'
status: 'done'
baseline_commit: '7bde7bf8818e1c1b1894965374028df9c7e07f6c'
review_decision_pending: false
context:
  - 'AGENTS.md'
  - '.agents/rules/ui.md'
  - '.agents/rules/widget-wiring.md'
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** When the window is resized horizontally small, the editor infobar becomes unstable: action buttons and message text briefly disappear, especially in restored-document and draft-restore states while both side panes are still consuming width. The current shell also keeps the right properties pane visible long enough that the center editor area enters a cramped layout zone before the existing breakpoint takes effect.

**Approach:** Make the infobar itself resilient on narrow widths in the same spirit as GNOME Text Editor by allowing its text and action labels to wrap instead of vanishing, then tighten the shell’s center-width protection so the properties pane collapses sooner and stops starving the editor column before the infobar reaches that unstable range.

## Boundaries & Constraints

**Always:** Preserve the existing draft-restore, external-change, Save, Save As, Discard, and Retry behaviors; this is a layout and presentation fix, not a flow rewrite. Keep `GtkInfoBar` as the inline notification vehicle and preserve the current quarter-width contract on wide windows. Follow GNOME Text Editor’s responsive infobar pattern by improving label wrapping and action presentation instead of hiding actions or truncating the message body. Keep the current collapse order where the properties pane collapses before the workspace pane, and only change the breakpoint behavior enough to protect the center content width earlier.

**Ask First:** If fixing the narrow-width instability would require a new large global minimum window width, replacing `GtkInfoBar` with a custom banner system, changing the quarter-width desktop contract, or collapsing the workspace pane materially earlier than the current design intent.

**Never:** Do not solve this primarily by raising the whole app’s minimum width. Do not regress the mirrored side-pane toggle model, draft safety, Save As cleanup, or focus restoration. Do not hide infobar actions behind a menu or remove the inline close affordance just to make the layout fit.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Untitled restore on narrow width | Restored untitled document shows the warning infobar while the window is horizontally constrained | The title/subtitle remain readable, the Save As action remains visible, and the bar reflows instead of dropping text or buttons | If width becomes extremely small, wrapping may grow the bar vertically, but actions must stay reachable |
| File-backed restore on narrow width | Restored file-backed document shows warning infobar with both Discard and Save actions while the window is constrained | Both actions stay visible and the message remains legible without transient disappearance during resize | Layout may stack or wrap within the infobar, but the action wiring must remain unchanged |
| Narrow dual-pane shell | Both side panes are visible and the window shrinks toward the cramped center-content range | The properties pane collapses before the editor column becomes too narrow, leaving the workspace pane behavior and collapse ordering intact | If the window continues shrinking, the existing workspace breakpoint still takes over cleanly |
| Wide desktop shell | Wide window with one or both side panes visible | The exact total-window `25% / 50% / 25%` behavior still holds and the infobar still looks normal at comfortable widths | No special handling needed beyond preserving existing wide-layout behavior |

</frozen-after-approval>

## Code Map

- `resources/ui/info-bar.ui` -- Current infobar template; needs narrow-safe title/subtitle wrapping and action presentation updates
- `crates/lushtext-core/src/ui/info_bar/imp.rs` -- Best place to mirror GNOME Text Editor’s button-label wrapping setup during widget construction
- `crates/lushtext-core/src/ui/info_bar/mod.rs` -- Public infobar wrapper; should keep the same notification API while exposing the improved layout behavior
- `crates/lushtext-core/src/ui/window/imp.rs` -- Owns split-view min widths, breakpoint installation, and width synchronization for earlier center-width protection
- `resources/ui/window.ui` -- Current top-level window contract, including the existing global `width-request` that should remain a secondary guardrail rather than the primary fix
- `crates/lushtext/tests/widget/window.rs` -- Existing shell regression suite already covers split-view fractions and breakpoint collapse order
- `crates/lushtext/tests/widget/editor_page.rs` -- Best fit for narrow infobar regression coverage if widget-level assertions are needed around the editor page

## Tasks & Acceptance

**Execution:**
- [x] `resources/ui/info-bar.ui`, `crates/lushtext-core/src/ui/info_bar/imp.rs`, and `crates/lushtext-core/src/ui/info_bar/mod.rs` -- make the infobar narrow-safe by wrapping message labels and action labels, keeping action visibility stable during horizontal resize, and preserving existing callback semantics
- [x] `crates/lushtext-core/src/ui/window/imp.rs` -- introduce earlier center-width protection so the properties pane collapses before the editor column enters the unstable cramped range, while preserving quarter-width wide layouts and current collapse ordering
- [x] `crates/lushtext/tests/widget/window.rs` and `crates/lushtext/tests/widget/editor_page.rs` -- add deterministic regression coverage for the earlier properties breakpoint and the narrow-width infobar behavior without relying on brittle manual resize timing
- [x] `AGENTS.md` and `.agents/rules/ui.md` -- document the responsive infobar expectation and the updated breakpoint intent so future shell/layout work stays aligned

**Acceptance Criteria:**
- Given a restored untitled document on a narrow window, when the warning infobar renders, then the message remains readable and the Save As action stays visible instead of disappearing during resize
- Given a restored file-backed document on a narrow window, when the warning infobar renders with both Discard and Save actions, then both actions remain available and the bar reflows rather than dropping controls
- Given a wide window with both side panes visible, when the shell lays out its split views, then the wide-screen quarter-width contract still holds unchanged
- Given the window narrows while both side panes are visible, when breakpoints re-evaluate, then the properties pane collapses before the editor center becomes overly cramped and before the workspace pane collapse takes over
- Given existing draft/notification flows trigger Retry, Save, Save As, or Discard from the infobar, when the responsive layout changes are applied, then the underlying actions still perform exactly the same operations as before

## Spec Change Log

## Review Findings Log

### 2026-04-10T12:40:25-03:00

- Reviewers used: blind hunter, edge case hunter, acceptance auditor
- patch:
  - `crates/lushtext-core/src/ui/window/imp.rs` — The new properties breakpoint is derived from a fixed center-width target but does not yet account for shell overhead such as split-view separators and related chrome, so the editor column can still end up narrower than intended near the cutoff.
  - `crates/lushtext-core/src/ui/window/imp.rs` — Auto-collapsing the properties pane at the new threshold can leave focus stranded in the hidden pane because focus restoration is currently tied to explicit pane-close flows rather than breakpoint-driven collapse.
  - `crates/lushtext/tests/widget/editor_page.rs` — The new infobar tests assert wrap and visibility flags on an unrealized widget, but they do not present a constrained window or prove the real resize-time disappearance regression is gone, including the access-error path.
  - `crates/lushtext/tests/widget/window.rs` — The new breakpoint test does not put the properties pane into the “visible while both panes are shown” scenario from the acceptance criteria, and it does not cover the computed cutoff or adjacent widths where rounding mistakes would hide.
- reject:
  - Duplicate variants of the same test-coverage concern from multiple reviewers were merged into the two patch findings above.
- Highest-priority category: `patch`
- Human decision required next: `[A] Apply the classified patch findings` or `[S] Stop for later`.

## Verification

**Commands:**
- `cargo test -p lushtext --test widget -- --nocapture` -- expected: window and editor widget regressions stay green with the new infobar and breakpoint behavior
- `cargo fmt --check` -- expected: formatting remains clean
- `cargo clippy --workspace --all-targets -- -D warnings` -- expected: no new warnings

**Manual checks (if no CLI):**
- `make run` -- expected: shrinking the window no longer causes the restored-document infobar text or actions to disappear, and the properties pane overlays/collapses early enough to keep the center editor area comfortable

## Suggested Review Order

**Adaptive Shell**

- Start where the earlier breakpoint budget is defined and applied.
  [`imp.rs:34`](../../crates/lushtext-core/src/ui/window/imp.rs#L34)

- Then review the breakpoint-driven focus handoff when the pane auto-collapses.
  [`mod.rs:909`](../../crates/lushtext-core/src/ui/window/mod.rs#L909)

**Info Bar Layout**

- See the GNOME-style action-label wrapping hook that keeps buttons visible.
  [`imp.rs:32`](../../crates/lushtext-core/src/ui/info_bar/imp.rs#L32)

- Then inspect the template-level label wrapping and balanced action widths.
  [`info-bar.ui:19`](../../resources/ui/info-bar.ui#L19)

**Verification**

- Check the real narrow-window warning/error allocation regressions in the widget shell.
  [`window.rs:366`](../../crates/lushtext/tests/widget/window.rs#L366)

- Review the exact breakpoint-math guardrails added to the window internals.
  [`imp.rs:757`](../../crates/lushtext-core/src/ui/window/imp.rs#L757)

- Keep the widget-level wrap invariants as supporting coverage for the infobar API.
  [`editor_page.rs:239`](../../crates/lushtext/tests/widget/editor_page.rs#L239)

**Docs**

- Confirm the contributor-facing shell contract now explains the earlier properties collapse.
  [`AGENTS.md:108`](../../AGENTS.md#L108)

- Confirm the canonical UI rules now require narrow-safe infobars.
  [`ui.md:109`](../../.agents/rules/ui.md#L109)
