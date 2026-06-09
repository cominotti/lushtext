## 1. Shell Dismissal Model

- [x] 1.1 Add a window-level helper that closes only the topmost visible dismissible shell surface and reports whether it handled the request.
- [x] 1.2 Route normal-window Escape handling through the shared helper so the command palette closes even when focus is outside the palette.
- [x] 1.3 Reuse the same helper from Focus Mode Escape handling so topmost-surface priority is not duplicated or divergent.
- [x] 1.4 Keep real modal dialogs, file choosers, destructive confirmations, menu popovers, dropdown popups, and child popups ahead of shell-level dismissal.

## 2. Command Palette Pointer Dismissal

- [x] 2.1 Add outside-pointer activation handling at the window shell or overlay boundary for the visible command palette.
- [x] 2.2 Classify pointer targets so clicks inside the palette, result list, mode selector, scrollbars, and child popups do not dismiss the palette.
- [x] 2.3 Ensure outside-click dismissal calls `close_command_palette()` exactly like Escape and result activation.
- [x] 2.4 Verify hidden or dismissed outside-click plumbing does not block later editor, sidebar, status-bar, or tab-strip interaction.

## 3. Regression Coverage

- [x] 3.1 Add widget coverage proving Escape closes the command palette after focus moves to the editor.
- [x] 3.2 Add widget coverage proving Escape closes the command palette after focus moves to the sidebar or another non-palette shell widget.
- [x] 3.3 Add widget coverage proving outside click closes the command palette and consumes/restores saved focus through the existing close path.
- [x] 3.4 Add widget coverage proving inside clicks on palette controls or non-activated rows keep the palette visible.
- [x] 3.5 Add widget coverage proving result activation still runs the selected file/command path and closes the palette once.
- [x] 3.6 Add widget coverage proving one Escape closes only the topmost eligible surface when the command palette is above the workspace search panel or Focus Mode.
- [x] 3.7 Cover no-tab/no-workspace, populated, dense or awkward-result, and constrained-geometry palette states relevant to the dismissal contract.

## 4. Agent Guidance

- [x] 4.1 Update `AGENTS.md` to describe the command palette as a shell-owned transient overlay whose Escape/outside-click dismissal is focus-independent and uses the saved-focus close path.
- [x] 4.2 Update `.agents/rules/widget-wiring.md` so close/dismiss wiring requires topmost-surface Escape, outside-click behavior where appropriate, child-popup guardrails, and focus restoration.
- [x] 4.3 Update `.agents/rules/ui.md` with transient overlay guidance covering command palette-style surfaces, inside/outside click classification, and state-extreme visibility checks.
- [x] 4.4 Update `.agents/skills/gtk-testing/SKILL.md` so GTK widget-test planning includes focus-independent Escape, outside-click dismissal, topmost-surface ordering, and child-popup guardrail coverage.
- [x] 4.5 Audit other GTK interaction skills touched by this workflow and update any that already discuss overlays, modals, popovers, or Escape handling so they do not contradict the new rules.
- [x] 4.6 Clarify that modal dialogs, including the empty Notes browser, keep explicit Close/Escape or response-based dismissal by default instead of inheriting command-palette click-away behavior.

## 5. Validation

- [x] 5.1 Run focused command-palette/window widget tests covering the new dismissal behavior.
- [x] 5.2 Run `make test-widget-headless` or the narrowest accepted widget harness that covers the affected window/palette tests.
- [x] 5.3 Run `make check-blueprint` if any Blueprint template changes are made. No Blueprint templates changed.
- [x] 5.4 Run Rust formatting and Clippy gates required by the touched Rust/UI files.
- [x] 5.5 Run `openspec validate --change stabilize-transient-surface-dismissal --strict`.
- [x] 5.6 Run `openspec validate --all --strict`.
- [x] 5.7 Run `git diff --check`.
