## Why

LushText currently offers no way to soften the document surface against the desktop background, while Fedora users now encounter a polished transparency control in Ptyxis and expect a similarly modern GNOME-style option. Previous attempts in this codebase blurred overlay surfaces instead of deliberately scoping transparency to document content, so the feature now needs a spec that locks down exactly which areas become translucent and which must remain opaque.

## What Changes

- Add a global `Transparency` preference in the editor appearance settings that is always visible and controls document-surface opacity.
- Use a GNOME-style control patterned after modern Fedora Ptyxis: a preferences row with a live percentage readout and a popover-hosted slider.
- Apply the selected transparency to the main editor document background and Markdown preview background.
- Keep the minimap, workspace sidebar, header bar, tab bar chrome, search panel chrome, info bars, properties panel, and status bar opaque.
- Define a GTK-native rendering contract that avoids whole-window opacity and avoids reintroducing the bleed-through problems seen in earlier overlay experiments.

## Capabilities

### New Capabilities
- `tab-content-transparency`: Defines the transparency preference, its user-visible control model, persistence, and the rendering boundaries for which tab content surfaces do and do not become translucent.

### Modified Capabilities

## Impact

- Affected code:
  - `crates/lushtext-core/src/ui/preferences/*`
  - `resources/ui/preferences.ui`
  - `data/dev.cominotti.lushtext.gschema.xml`
  - `crates/lushtext-core/src/ui/editor_page/*`
  - `crates/lushtext-core/src/ui/markdown_preview/*`
  - `resources/ui/editor-page.ui`
  - `resources/ui/markdown-preview.ui`
  - `resources/gtksourceview/styles/*`
  - `resources/style/style.css`
  - relevant widget tests under `crates/lushtext/tests/widget/`
- Affected systems:
  - Editor appearance preferences and GSettings persistence
  - GtkSourceView-backed document rendering
  - Markdown preview surface rendering
  - UI contracts that keep shell chrome opaque while tab content backgrounds become translucent
