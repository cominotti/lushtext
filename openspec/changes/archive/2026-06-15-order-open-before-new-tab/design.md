## Context

The completed `match-gnome-open-popover` change gives LushText a GNOME Text Editor-style Open menu button with a recent-document popover. The current header template still places `new_tab_button` before `open_menu_button`, which preserves LushText's old action order but leaves the new primary Open workflow visually second.

GNOME Text Editor presents Open before New. For LushText, that ordering also matches the new semantic weight: Open now gathers recent search, the normal file chooser, and duplicate-safe document activation, while New File remains a single direct creation action.

## Goals / Non-Goals

**Goals:**
- Put the Open menu button before New File/New Tab on the header bar's start side.
- Preserve the Open popover's behavior, actions, shortcuts, accessibility names, and compact icon breakpoint.
- Prove the ordering in wide and constrained header states without weakening existing Open popover coverage.

**Non-Goals:**
- Do not change `win.open-file`, `win.open-recent`, `win.new-tab`, or their shortcuts.
- Do not alter recent-document persistence, search, activation, or row rendering.
- Do not change end-side header controls such as document properties, Notes, or the primary menu.

## Decisions

### Reorder Template Children Instead Of Rebinding Actions

Move the existing `open_menu_button` block ahead of `new_tab_button` in `resources/ui/window.blp`, then regenerate `resources/ui/window.ui` and `resources/ui/template-contract.json`. This keeps object IDs, actions, shortcuts, accessible metadata, and automation anchors stable.

Alternative considered: leave the template order alone and use CSS or layout packing tricks. That would make the rendered order harder to reason about and could drift from TemplateChild/order tests.

### Keep Open First In Both Wide And Compact Presentations

The Open button should remain first whether it renders as `Open` plus chevron or as the compact folder icon. The breakpoint changes presentation only; it should not change action priority or keyboard/accessibility meaning.

Alternative considered: show Open first only in wide layouts while keeping the older icon order in compact mode. That would make constrained geometry feel inconsistent and would hide the exact state where visual order matters most.

### Verify Order At The Header Surface

Add focused widget coverage that inspects the start-side sibling order and verifies the Open menu button precedes New File while both controls remain reachable. Extend or reuse visual geometry proof so a rendered header state captures the Open-first ordering in the GNOME-style surface.

Alternative considered: rely on Blueprint drift checks alone. Blueprint checks prove generation consistency, but not the intended user-facing order.

## Risks / Trade-offs

- Existing LushText muscle memory expects New first -> This is a small visual-order change aligned with the larger Open popover redesign and GNOME Text Editor precedent.
- Header order tests can become too template-fragile -> Assert only the user-facing relative order of Open and New, not unrelated child counts or end-side controls.
- Compact geometry could regress silently -> Include constrained or visual proof coverage where the Open control is in its folder-icon presentation.

## Migration Plan

1. Move the existing Open menu button before the New File button in the Blueprint template.
2. Regenerate generated UI and template contract artifacts.
3. Add focused widget/visual coverage for Open-before-New ordering in wide and constrained presentations.
4. If rollback is needed, restore the previous template order without changing action IDs or persistence data.

## Open Questions

None. The intended order is Open first, then New File/New Tab.
