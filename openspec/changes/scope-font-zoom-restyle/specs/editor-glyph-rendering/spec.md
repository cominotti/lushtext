## ADDED Requirements

### Requirement: Every font-following surface takes the zoom through its class
Each widget that follows the editor font SHALL be registered when it is
created and SHALL then carry the class of the current zoom level, so that a
widget created after a zoom step (a new tab, a search-result row realized
later, a Markdown preview rendered later, the file peek) shows the current
size. The computed font family, size and editor line height of every
registered widget SHALL equal what the display-wide rules produce on `main`
for the same font, zoom and system-font state; with the system font at 100 %,
auxiliary `.monospace` widgets SHALL keep GTK's default font. Application code
SHALL NOT add `.monospace` or enable `GtkTextView:monospace` without
registering the widget. The workspace file tree uses the default font and does
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

#### Scenario: An unregistered monospace widget fails the policy check
- **WHEN** application code adds the `monospace` class to a widget without
  registering it
- **THEN** `make check-policy` fails and names the call site
