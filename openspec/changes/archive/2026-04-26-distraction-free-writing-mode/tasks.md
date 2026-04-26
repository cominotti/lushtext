## 1. Settings, Actions, and Public Entrypoints

- [x] 1.1 Add Focus Mode GSettings keys for target column width and typewriter scrolling in `data/dev.cominotti.lushtext.gschema.xml` and `crates/lushtext-core/src/config.rs`.
- [x] 1.2 Add Focus Mode controls to `resources/ui/preferences.ui` and wire them in the preferences implementation.
- [x] 1.3 Register `win.toggle-focus-mode` as a stateful per-window action without changing the existing fullscreen actions.
- [x] 1.4 Add `Ctrl+Shift+F11` to the shortcut controller, shortcuts help, and command palette metadata while keeping `F11` as ordinary fullscreen and `Alt+P` as Markdown preview-only.

## 2. Focus Shell State and Chrome Suppression

- [x] 2.1 Add a `ui/window/focus_mode.rs` workflow module and window state for active Focus Mode, previous fullscreen state, preview restoration hints, and focused preview changes.
- [x] 2.2 Implement Focus Mode entry and exit so the window enters fullscreen on entry and restores only the fullscreen state it owns on exit.
- [x] 2.3 Hide the ordinary header bar, tab bar, and status bar while Focus Mode is active, then restore them on exit.
- [x] 2.4 Integrate Focus Mode suppression with the existing workspace sidebar and document-properties requested-versus-rendered layout path without writing persisted visibility preferences.
- [x] 2.5 Add a minimal overlaid Focus Mode affordance to the window overlay with a Leave Focus Mode action and top-edge or keyboard-accessible reveal behavior.
- [x] 2.6 Implement `Escape` handling so command palette, search surfaces, dialogs, popovers, and other transient surfaces retain priority before Focus Mode exits.

## 3. Preview, Editor Column, and Minimap Behavior

- [x] 3.1 Suppress side-by-side Markdown preview on Focus Mode entry and restore it on exit only when the user did not make a conflicting preview choice while focused.
- [x] 3.2 Preserve `Alt+P` preview-only behavior inside Focus Mode so it toggles focused source editing and focused rendered Markdown without exiting Focus Mode.
- [x] 3.3 Implement a reusable readable-column calculation based on allocated width, measured font metrics, configured target columns, and clamped minimum margins.
- [x] 3.4 Apply Focus Mode readable-column margins to `GtkSourceView` editor pages and restore normal editor margins when Focus Mode exits.
- [x] 3.5 Apply Focus Mode readable-column margins to rendered Markdown preview and restore normal preview margins outside Focus Mode.
- [x] 3.6 Suppress the editor minimap while Focus Mode is active without changing the saved minimap preference, and restore normal minimap availability on exit.
- [x] 3.7 Implement opt-in Focus Mode typewriter scrolling for source editing only, defaulting off and avoiding changes to session restore semantics.
- [x] 3.8 Add a subtle source-editor text-origin guide that is visible only in Focus Mode source editing, tracks the active readable-column left margin, and remains non-interactive.

## 4. Regression Coverage

- [x] 4.1 Add widget tests proving `Ctrl+Shift+F11` toggles Focus Mode while `F11` still toggles ordinary fullscreen.
- [x] 4.2 Add widget tests for Focus Mode entry and exit restoring header bar, tab bar, status bar, fullscreen state, workspace sidebar rendering, and document-properties rendering.
- [x] 4.3 Add widget tests proving `F9` changes document-properties requested state while Focus Mode suppresses the rendered surface.
- [x] 4.4 Add widget tests proving `Alt+P` works inside Focus Mode and side-by-side Markdown preview is suppressed/restored according to the spec.
- [x] 4.5 Add widget tests for readable-column margins on wide and narrow editor allocations, including restoration after Focus Mode exit.
- [x] 4.6 Add widget tests for rendered Markdown readable-column margins and normal preview margin restoration.
- [x] 4.7 Add widget tests proving minimap preference is preserved while Focus Mode temporarily hides minimap rendering.
- [x] 4.8 Add focused tests for typewriter scrolling default-off behavior and enabled cursor-centering behavior where deterministic in the widget harness.
- [x] 4.9 Add an integration or widget-level regression for `Escape` priority so an active overlay closes before Focus Mode exits.
- [x] 4.10 Add widget coverage proving the Focus Mode text-origin guide is hidden outside Focus Mode, visible in source editing while focused, and tracks readable-column margin changes.

## 5. Documentation and Verification

- [x] 5.1 Update `docs/next/distraction-free-mode.md` so it reflects the approved Focus Mode contract, current fullscreen support, `F9` ownership, and `Alt+P` Markdown preview behavior.
- [x] 5.2 Update root and nested `AGENTS.md`/README module maps only if the implementation adds or materially reorganizes modules.
- [x] 5.3 Run formatting and targeted widget tests for Focus Mode, preview, document properties, minimap, shortcuts, and preferences.
- [x] 5.4 Run `cargo check --workspace --all-targets`.
- [x] 5.5 Run `openspec status --change distraction-free-writing-mode --json` and confirm all implementation tasks are tracked before archive.
