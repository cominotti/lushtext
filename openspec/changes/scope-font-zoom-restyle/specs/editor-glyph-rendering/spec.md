## ADDED Requirements

### Requirement: Every font-following surface takes the zoom through its class
Each widget that follows the editor font SHALL be registered when it is
created and SHALL then carry the class of the current zoom level, so that a
widget created after a zoom step (a new tab, a search-result row realized
later, a Markdown preview rendered later, the file peek) shows the current
size. The computed font family, size and editor line height of every
registered widget SHALL equal what the display-wide rules produce on `main`
for the same font, zoom and system-font state; with the system font at 100 %,
auxiliary `.monospace` widgets SHALL keep GTK's default font. The same SHALL
hold under the desktop text scale and the high-contrast style, and the Focus
Mode readable-column margins, which derive from the editor's Pango character
width, SHALL equal `main`'s at every zoom level. Application code SHALL NOT
add `.monospace` or enable `GtkTextView:monospace` (in Rust or in Blueprint)
except through the registering helper of `ui/font_zoom.rs`, or, for a
Blueprint widget, with a registration in the owning widget's construction that
the policy check maps to it. The workspace file tree uses the default font and does
not follow the editor font.

#### Scenario: A widget created after a zoom step takes the current size
- **WHEN** the user zooms to 130 % and then runs a workspace search
- **THEN** the result rows' line labels use the zoomed monospace size, the same
  as on `main` at 130 %

#### Scenario: The default state is unchanged
- **WHEN** the system font is used at 100 %
- **THEN** search results, the file peek, the minimap and Markdown code blocks
  use GTK's default monospace font, and the editor uses the system monospace
  family with its line-height guard

#### Scenario: Focus Mode margins follow the zoom as on main
- **WHEN** Focus Mode is active and the user zooms from 100 % to 130 %
- **THEN** the editor's readable-column margins equal those `main` computes at
  130 % for the same width and target columns

#### Scenario: No monospace widget is left unregistered at run time
- **WHEN** a window with a populated search panel, an open file peek, a
  Markdown preview with a code block and an image fallback, the notes browser,
  the local-history browser and the Line Inspector is walked
- **THEN** every widget carrying `.monospace` or `GtkTextView:monospace` is
  registered

#### Scenario: An unregistered monospace widget fails the policy check
- **WHEN** application code adds the `monospace` class to a widget without
  registering it
- **THEN** `make check-policy` fails and names the call site
