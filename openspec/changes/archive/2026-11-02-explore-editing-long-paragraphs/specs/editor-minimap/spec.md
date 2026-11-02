## ADDED Requirements

### Requirement: A retired native map is released
When the editor retires its native `GtkSourceMap` (the minimap is unavailable, hidden, or suspended for loading), nothing LushText owns SHALL keep the retired map alive: once retirement returns and the main loop has run, the map SHALL be finalized and SHALL no longer be attached to the document's buffer. A retired map SHALL NOT validate, lay out or draw document text afterwards. Retirement SHALL NOT clear a live map's `view` (ledger A32); it unparents the map, which unrealizes it (ledger A45), and drops it. Any helper that refers to the map, such as the reflow freeze, SHALL refer to it weakly.

#### Scenario: Default preference
- **WHEN** an editor is constructed with the minimap preference off and the user types in a long paragraph
- **THEN** no retired map is alive
- **AND** no validation idle runs for a map

#### Scenario: Retired after an edit
- **WHEN** the minimap is retired because an edit made a paragraph too long
- **THEN** a weak reference to the retired map is cleared
- **AND** no `Gtk-CRITICAL` or `GLib-GObject-CRITICAL` is logged
