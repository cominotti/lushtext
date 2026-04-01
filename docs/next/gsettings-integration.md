# GSettings Integration

## Status: Next priority

## Description
Wire up the preferences dialog to GSettings for persistent user preferences.
Currently the preferences dialog UI exists but values are not persisted.

## Implementation Plan
1. Create GSettings schema: `data/dev.cominotti.lushtext.gschema.xml`
2. Define keys: `font-name`, `use-system-font`, `tab-width`, `insert-spaces`,
   `show-line-numbers`, `highlight-current-line`, `color-scheme`, `sidebar-width`
3. Bind preferences dialog rows to GSettings via `gio::Settings::bind()`
4. Propagate changes to all open editor pages via GSettings `changed` signals
5. Install schema in Flatpak via Meson
6. For dev builds: compile schema locally with `glib-compile-schemas`
