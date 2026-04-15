## Why

The workspace sidebar currently treats `Small`, `Comfy`, and `Large` as exact fractions of the full window width. That stays deterministic, but it produces awkwardly wide sidebars on large and ultrawide displays, and it keeps a layout preference permanently visible in the sidebar chrome instead of placing it with the rest of the workspace settings.

## What Changes

- Move the workspace sidebar width choice from the fixed sidebar footer into `Preferences > Workspace`.
- Replace the raw fraction-only preset behavior with adaptive preset widths that clamp each preset to a comfortable sidebar width range on large windows while still shrinking on smaller windows.
- Keep the three user-facing presets (`Small`, `Comfy`, `Large`) and preserve deterministic, non-draggable split-view sizing.
- Recompute the left-pane width and dependent right-pane breakpoint math from the selected adaptive preset so the editor column remains protected.
- Remove the sidebar footer width buttons once the preference-driven control is in place.

## Capabilities

### New Capabilities
- `workspace-sidebar-width-policy`: Defines the user-visible workspace sidebar width preference, adaptive preset sizing behavior, and the window-shell contract for applying the selected preset.

### Modified Capabilities

## Impact

- Affected code:
  - `crates/lushtext-core/src/ui/preferences/*`
  - `resources/ui/preferences.ui`
  - `resources/ui/sidebar.ui`
  - `crates/lushtext-core/src/ui/sidebar/*`
  - `crates/lushtext-core/src/ui/window/imp.rs`
  - `data/dev.cominotti.lushtext.gschema.xml`
  - `crates/lushtext/tests/widget/window.rs`
- Affected systems:
  - Workspace preference persistence
  - Split-view width and breakpoint calculations
  - Sidebar chrome and GNOME-style preferences UX
