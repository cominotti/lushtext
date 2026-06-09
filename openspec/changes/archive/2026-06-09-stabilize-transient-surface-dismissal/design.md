## Context

`LushtextCommandPalette` is currently a `GtkRevealer` overlay in `LushtextWindow`, not a `GtkPopover` or `AdwDialog`. Opening the palette saves the current window focus, reveals the overlay, clears/searches the palette, and focuses the palette search entry. Closing clears the widget, hides the revealer, and restores saved focus.

The failure comes from dismissal ownership. Escape is currently observed through the palette `SearchEntry::stop-search` path, so it works only while the search entry or its direct key handling owns focus. Clicking outside the overlay can move focus elsewhere without closing the palette, and a later Escape goes to the newly focused widget instead of the palette. Focus Mode already has a local "close topmost transient first" idea, and search-panel code already treats the command palette as the topmost overlay when both are visible. This change makes that rule explicit and shared for the normal window shell.

## Goals / Non-Goals

**Goals:**
- Make Escape close the topmost visible dismissible shell surface even when focus has left that surface.
- Make outside pointer activation close the command palette while preserving every inside-palette interaction.
- Keep command-palette closing on one shared close path so search cleanup and focus restoration stay consistent.
- Preserve child popup/dialog semantics: modal dialogs, file choosers, destructive confirmations, menu popovers, dropdown popups, and palette child popups must get first chance to handle dismissal.
- Encode the resulting guardrails in repo rules and GTK testing skills.

**Non-Goals:**
- Replacing the command palette with `GtkPopover`, `AdwDialog`, or a new modal framework.
- Changing command-palette result grouping, command search, fuzzy matching, or activation semantics.
- Making one Escape close every visible secondary surface at once.
- Changing save/close confirmation behavior or bypassing destructive-dialog response handling.

## Decisions

### Add a window-level transient close path

Introduce a window-shell helper that answers "did we close the topmost dismissible shell surface?" and use it from the window's Escape handling. The helper should prioritize the command palette, then existing lower-priority surfaces that already have close contracts, without forcing everything to close in one key press.

Alternative considered: keep Escape inside each widget's focused child. That is the current failure mode; it breaks as soon as focus moves to a different widget while the surface remains visible.

Alternative considered: make every transient surface modal. That would overstate the command palette's role, risks blocking normal app interaction, and would still require careful child-popup handling.

### Keep the command palette close path single

All palette dismissal paths should call `close_command_palette()`: result activation, `SearchEntry::stop-search`, global Escape, and outside click. That method already clears the search entry/results, hides the revealer, and restores saved focus through `saved_focus -> active_editor -> no focus`.

The helper should be idempotent enough that double delivery from a click gesture and focus/event propagation does not consume focus twice or leave stale `saved_focus` state.

### Classify outside clicks at the window overlay boundary

Install pointer handling at the window shell level, not inside the palette widget. The handler should close the palette only when a primary pointer activation lands outside the visible command-palette widget and outside any active child popup. Inside-palette clicks must proceed so the search entry, mode selector, scrollbars, rows, and result activation keep their normal behavior.

If GTK hit testing through `Widget::pick()` or ancestry checks is available and stable for this overlay, prefer that over coordinate-only rectangle math. If an explicit scrim/event target is simpler, it must stay visually inert, must not sit above the palette, and must not block child popup interactions.

### Respect child popup and real modal ownership

The shell-level closer must not preempt active child surfaces that already own Escape or outside-click behavior. For this change, that means at least:
- `GtkDropDown` popup from the palette mode selector;
- menu popovers opened from window chrome;
- modal dialogs and alert dialogs;
- file chooser dialogs;
- destructive confirmations.

The implementation may be conservative: if there is uncertainty that a child popup owns the event, proceed with GTK's child behavior rather than forcing palette dismissal.

Outside-click dismissal is a shell-overlay or popover-style behavior, not a universal rule for modal dialogs. Browser-style dialogs such as the empty Notes browser and Local History keep explicit Close/Escape dismissal by default, while destructive, response-oriented, file chooser, and preferences dialogs require an explicit dialog response or toolkit-provided close gesture. Any future dialog that wants click-away behavior should opt in with a surface-specific requirement instead of inheriting it from the command-palette overlay contract.

### Add focused widget coverage plus guidance updates

Tests should prove the user-reported failure directly:
- open the palette, move focus to editor/sidebar/status bar, press Escape, and assert the palette closes;
- open the palette, click outside, and assert the palette closes through the same focus-restoring close path;
- click inside the palette, including the mode selector/result area, and assert the palette stays visible unless a result is activated;
- show command palette above search panel or Focus Mode and assert only the topmost surface closes per Escape.

Implementation should update:
- root `AGENTS.md` focus restoration / overlay behavior note;
- `.agents/rules/widget-wiring.md` close/dismiss and focus-restoration sections;
- `.agents/rules/ui.md` transient-surface guidance for overlays;
- relevant GTK testing skills, especially `gtk-testing`, so future UI work covers focus-independent dismissal and child-popup guardrails.

## Risks / Trade-offs

[Risk] A capture-phase key controller could steal Escape from a modal or child popup. -> Mitigation: scope the global Escape handler to the application window's non-modal shell, check for visible child/modal ownership first, and keep child-popup behavior ahead of shell dismissal.

[Risk] An outside-click gesture could close the palette before a legitimate inside click activates a result or opens the mode dropdown. -> Mitigation: classify target ancestry before closing, keep inside clicks proceeding, and cover result activation and mode-selector clicks in widget tests.

[Risk] A visually inert scrim could block editor/sidebar interactions after closing the palette. -> Mitigation: hide or make the scrim untargetable whenever the palette is hidden, and test outside click both closes the palette and leaves the app able to focus/edit afterward.

[Risk] Focus restoration may race with the outside click's own focus assignment. -> Mitigation: use the same close path as existing palette dismissal and, if needed, defer restoration to an idle tick only when it preserves the existing saved-focus contract.

[Risk] Widget tests can miss live shell event propagation. -> Mitigation: add direct widget tests for controllers/visibility and include at least one headless or live GTK smoke pass that exercises real pointer/key behavior in a presented window.
