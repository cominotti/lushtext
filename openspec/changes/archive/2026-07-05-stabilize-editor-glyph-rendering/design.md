## Context

The main editor is a `GtkSourceView` hosted in `LushtextEditorPage`. Before this change, the editor used `GtkTextView:monospace=true` and the display-wide font provider targeted GTK's broad `.monospace` class. In a live `make run` session, typing `[` into a blank document clipped the top horizontal stroke while the line was active. Pressing Enter redrew the previous line correctly.

The old proposal assumed this needed broad visual-geometry, minimap, Automation1, and repaint-generation work. The live repro disproved that plan: the first line still clipped after those changes. The fix that passed the repro was narrower: stop using the main editor's built-in monospace mode, apply the selected monospace family through the editor's own surface selector, and add editor-only line-height headroom.

## Goals / Non-Goals

**Goals:**

- Keep active-line `[` top ink visible while the user is still typing, before Enter, focus loss, or unrelated redraws.
- Preserve the user's existing editor font and zoom preferences.
- Keep minimap, sidebar, and other `.monospace` surfaces from inheriting the editor-only line-height guard.
- Add a live key-event smoke lane that reproduces the original failure and fails on missing top ink.
- Repair the resource loading/rebuild path discovered during investigation so future bundled resource edits are not silently stale under `make run`.

**Non-Goals:**

- Replacing GtkSourceView or drawing editor glyphs manually.
- Adding Automation1 document-text diagnostics or broad visual-geometry pixel policy.
- Changing minimap behavior, file formats, session data, or app-data migrations.
- Keeping the discarded style-scheme foreground/background experiment; it was not required for the passing repro.

## Decisions

### Decision: Move the main editor off GtkTextView monospace mode

The main editor template sets `monospace=false`. The selected editor font is still applied by `theme.rs`, but through the editor's `tab-content-editor-surface` selector instead of relying on the built-in monospace mode. This avoids the live active-line clipping path while keeping the editor visually monospace.

Alternative considered: keep `monospace=true` and add repaint or margin changes. Repaint, bracket matching, current-line toggles, IM module changes, renderer changes, and small line-spacing changes did not fix the live repro.

### Decision: Add editor-only line-height headroom

The font CSS adds `line-height: 1.18` only to the main editor surface and its text node. This was the smallest tested value that made the active-line bracket top stroke pass with Adwaita Mono at 11pt while keeping normal row spacing tight.

Alternative considered: change the GtkSourceView style-scheme base `text` style. Reverting that experiment still passed the live smoke, so it is not kept.

### Decision: Prove the bug with real key events before Enter

The prevention lane launches an isolated Xvfb + D-Bus session, opens an empty fixture, types `[[[[[[[[` through `xdotool`, captures before pressing Enter, and checks the active-line crop for upper horizontal ink. It captures a post-Enter reference too, but the critical assertion is the before-Enter active line.

Alternative considered: rely on settled visual geometry screenshots. The reported failure can repair after later redraws, so settled-only proof can miss it.

### Decision: Keep the resource rebuild correction

The GResource XML prefix is corrected to match the path registered in `theme.rs`, and `build.rs` now tells Cargo to rerun when the resource XML or bundled style files change. This is not the direct glyph fix, but it prevents future style/resource fixes from appearing to fail because `make run` used stale compiled resources.

## Risks / Trade-offs

[Risk] `line-height: 1.18` could feel slightly taller than the old editor rows. Mitigation: the value is editor-only, tested with the exact live bracket repro, and intentionally modest.

[Risk] Pixel detection can be sensitive to fonts and renderers. Mitigation: the smoke pins a known fixture, theme, font, zoom, window size, and Xvfb/cairo path, and writes crops plus threshold artifacts for diagnosis.

[Risk] Xvfb lacks a window manager and focus can be brittle. Mitigation: the smoke focuses the window directly, tolerates unsupported `_NET_ACTIVE_WINDOW`, clicks two editor-body positions, and fails with artifacts instead of silently passing without typed text.
