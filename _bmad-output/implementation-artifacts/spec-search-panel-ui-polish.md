---
title: 'Search panel UI polish: visual distinction, spacing, count label, background bug'
type: 'bugfix'
created: '2026-04-08'
status: 'done'
baseline_commit: 'f0de790'
context: []
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** The search panel is visually indistinct from the editor area (same background, no border), the match count label is small and can overlap with scrollbar, vertical spacing between options elements is inconsistent, and closing the panel with Ctrl+Shift+F leaves the editor area with a wrong background color due to the `slide-up` revealer transition clipping into the editor during collapse.

**Approach:** Add a distinct background and top border to the search panel via CSS, increase match count label prominence, normalize spacing in the options area to match the header, and fix the revealer transition type from `slide-up` to `slide-down` to eliminate the background color bug.

## Boundaries & Constraints

**Always:** Use Adwaita semantic color tokens (`@headerbar_bg_color`, `@borders`) for panel styling — must adapt to light/dark mode. Keep the status bar's existing visual language as reference.

**Ask First:** If the fix requires changing the panel's position in the widget hierarchy (e.g., moving from box child to overlay).

**Never:** Add custom hard-coded colors. Change panel functionality or behavior. Add new dependencies.

</frozen-after-approval>

## Code Map

- `resources/style/style.css` -- CSS rules for the search panel background and border
- `resources/ui/search-panel.ui` -- spacing values, count label styling
- `resources/ui/window.ui` -- revealer transition-type fix (line 132)
- `crates/lushtext-core/src/ui/search_panel/imp.rs` -- CSS class application in constructed()
- `crates/lushtext-core/src/ui/search_panel/mod.rs` -- count label update logic

## Tasks & Acceptance

**Execution:**
- [x] `resources/style/style.css` -- Add `.search-panel` CSS rule with `background-color: @headerbar_bg_color` and `border-top: 1px solid @borders` to visually separate panel from editor
- [x] `resources/ui/search-panel.ui` -- Add `<class name="search-panel"/>` to the template's style; change `count_label` from `.caption` to `.heading` class for larger text; normalize `options_box` margins from `margin-top=4, margin-bottom=4` to `margin-top=6, margin-bottom=6` to match `header_box`; change `options_box` spacing from `4` to `6` to match header
- [x] `resources/ui/window.ui` -- Change `search_panel_revealer` transition-type from `slide-up` to `slide-down` (line 132)
- [x] `crates/lushtext/tests/widget/search_panel.rs` -- Add test asserting the `.search-panel` CSS class is present; add test asserting revealer transition type is `slide-down`

**Acceptance Criteria:**
- Given the search panel is visible, when looking at the boundary between editor and panel, then a clear visual separation (distinct background + border) is visible in both light and dark modes
- Given the search panel has results, when the results scrollbar appears, then the match count label is clearly legible and does not overlap with the scrollbar
- Given the options revealer is expanded, when comparing spacing between elements, then vertical spacing is consistent (6px throughout)
- Given the search panel is open, when closing it with Ctrl+Shift+F, then the editor area background color is unchanged after the panel disappears

## Verification

**Commands:**
- `make check` -- expected: clippy + fmt pass
- `make test-widget` -- expected: all widget tests pass including new assertions
- `make run` -- expected: panel has distinct background, no GTK warnings on panel toggle

**Manual checks:**
- Toggle panel open/closed 5 times watching stderr — no pixman or GTK warnings
- Verify panel looks distinct in both light and dark mode
- Verify match count is readable when scrollbar is visible

## Spec Change Log

- **Review patch**: count_label changed from `.heading` to default body text (no class) — `.heading` was too dominant for secondary footer metadata. Test updated to assert absence of both `.caption` and `.heading`.
- **Review patch**: `footer_box` margins normalized from 4px to 6px for consistency.
- **Review patch**: `error_label` bottom margin normalized from 4px to 6px for consistency.

## Suggested Review Order

**Visual distinction (CSS + template)**

- Panel background and border — the core visual fix, mirrors `.status-bar` pattern
  [`style.css:36`](../../resources/style/style.css#L36)

- CSS class applied to panel template root element
  [`search-panel.ui:7`](../../resources/ui/search-panel.ui#L7)

**Background color bug fix**

- Revealer transition changed from `slide-up` to `slide-down` — prevents clip overlap with editor during hide animation
  [`window.ui:132`](../../resources/ui/window.ui#L132)

**Spacing normalization**

- Options box margins/spacing unified to 6px (was 4px)
  [`search-panel.ui:89`](../../resources/ui/search-panel.ui#L89)

- Footer box margins unified to 6px (was 4px)
  [`search-panel.ui:170`](../../resources/ui/search-panel.ui#L170)

- Error label bottom margin unified to 6px (was 4px)
  [`search-panel.ui:73`](../../resources/ui/search-panel.ui#L73)

- Count label class removed (was `.caption`, briefly `.heading`, now default body text)
  [`search-panel.ui:175`](../../resources/ui/search-panel.ui#L175)

**Tests**

- CSS class, revealer transition, and count label class assertions
  [`search_panel.rs:1095`](../../crates/lushtext/tests/widget/search_panel.rs#L1095)
