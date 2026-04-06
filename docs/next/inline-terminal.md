# Inline Terminal Panel

## Status: Proposed

## Description
A toggleable bottom panel (`Ctrl+`` `) running the user's shell, scoped to the active
workspace root as the working directory. Ephemeral and unobtrusive — a scratchpad for
quick commands, not a full terminal emulator ambition. Keeps users in LushText instead
of alt-tabbing to GNOME Terminal for the 30% of tasks that need a quick command.

## Current State
- No terminal integration exists
- The window layout is: HeaderBar → TabBar → Paned(sidebar, content) → StatusBar
- No bottom panel infrastructure

## Motivation
Running a script, checking `git status`, grepping output, piping a command — these are
tasks that interrupt the editing flow when they require switching to a separate terminal.
An inline terminal that appears instantly, runs a command, and disappears keeps the user
in context. The key differentiator from an IDE terminal: it's ephemeral. No persistent
session management, no tabs within tabs, no tmux complexity.

## Implementation Plan

### Phase 1: VTE Integration
1. Add `vte4` crate dependency (GTK4 bindings for the VTE terminal emulator library)
   - VTE is the same terminal backend used by GNOME Terminal, Tilix, and Console
   - `vte4` crate version must align with the gtk-rs 0.11 series
2. New `LushtextTerminalPanel` widget:
   - `GtkBox(vertical)` containing a `GtkRevealer` with a `vte4::Terminal`
   - Separator line at the top (matching sidebar separators)
   - Small header bar with: workspace name, close button, maximize button

### Phase 2: Window Layout Integration
1. Wrap the existing content area and the new terminal panel in a vertical `GtkPaned`
   - Top: existing `GtkPaned` (sidebar + content)
   - Bottom: `LushtextTerminalPanel`
2. Toggle action: `win.toggle-terminal` with keyboard shortcut `` Ctrl+` ``
3. Animation: use `GtkRevealer` with `SlideUp` transition (or `AdwTimedAnimation` for
   consistency with sidebar toggle)
4. Default height: 1/4 of window height, resizable via paned handle
5. Persist terminal panel height in GSettings (`terminal-height` key)

### Phase 3: Workspace-Aware Working Directory
1. On terminal open, set `cwd` to the active workspace's first root directory
2. If no workspace is active, use the directory of the currently focused file
3. If no file is open, use `$HOME`
4. Switching workspaces does NOT change the terminal's `cwd` (would be confusing mid-command)
5. A "cd to workspace" button in the terminal header resets the directory

### Phase 4: Editor ↔ Terminal Integration
1. "Open Terminal Here" in the sidebar file context menu — opens terminal with `cwd` set
   to the right-clicked directory
2. Terminal output link detection: clickable file paths in terminal output open the file
   in a new tab (stretch goal — VTE supports custom regex matching)
3. `Ctrl+Shift+C` / `Ctrl+Shift+V` for terminal copy/paste (standard VTE bindings)

### Phase 5: Lifecycle
1. Terminal shell process spawns on first toggle (not on window creation)
2. Closing the panel hides it but keeps the shell alive (resume where you left off)
3. Window close kills the terminal shell process
4. No persistent terminal sessions across app restarts — always starts fresh
5. Optional: multiple terminal instances (stretch goal, deferred)

## Architecture Considerations
- VTE is a well-established library (used by GNOME Terminal since 2001) but adding it
  is a significant dependency. The `vte4` crate provides GTK4 bindings. Ensure version
  compatibility with the gtk-rs 0.11 series.
- The nested `GtkPaned` layout (horizontal for sidebar, vertical for terminal) is the
  standard pattern used by gedit, GNOME Builder, and Kate. It works well but adds
  complexity to the sidebar position clamping logic — the outer paned's width is now
  shared between three regions.
- Terminal font should follow the editor's monospace font setting (via the `.monospace`
  CSS class or explicit `vte::Terminal::set_font()`).
- The terminal panel must not steal focus on open unless the user explicitly focuses it.
  Opening via keyboard shortcut should focus the terminal; opening via menu should not.

## Dependencies
- `vte4` crate (GTK4 VTE bindings)
- VTE system library (`vte-2.91-gtk4` or newer)
- Flatpak: VTE is included in the GNOME runtime, so no additional SDK extension needed
- Window layout refactoring for nested paned

## Risks
- VTE's GTK4 port (`vte-2.91-gtk4`) may not be available in all distributions yet.
  Fedora 43 and Ubuntu 24.04+ have it; older distros may not. This could be a
  compile-time optional feature gated behind a cargo feature flag.
- The `vte4` Rust crate may lag behind VTE releases. Check maintenance status and
  version availability before committing.
- Adding a terminal blurs the line between "text editor" and "IDE." The design must stay
  minimal — one terminal, no tabs, no split terminals, no session persistence. Scope
  creep here would undermine the app's identity.
