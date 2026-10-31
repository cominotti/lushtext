## ADDED Requirements

### Requirement: A retired native map releases the document
When the editor retires its native `GtkSourceMap` (the minimap is unavailable, hidden, or suspended for loading), the retired map SHALL no longer be attached to the document's buffer once retirement returns, and SHALL be finalized unless the toolkit itself holds the last reference. A retired map SHALL NOT validate, lay out or draw document text afterwards. Retirement SHALL NOT clear a live map's `view` (ledger A32) and SHALL detach the buffer only after the map is unparented and unrealized (ledger A45).

#### Scenario: Default preference
- **WHEN** an editor is constructed with the minimap preference off and the user types in a long paragraph
- **THEN** no `GtkSourceMap` has the document buffer
- **AND** no validation idle runs for a map

#### Scenario: Retired after an edit
- **WHEN** the minimap is retired because an edit made a paragraph too long
- **THEN** a weak reference to the retired map is cleared, or, when the toolkit keeps the map alive, the map's buffer is no longer the document buffer
- **AND** no `Gtk-CRITICAL` or `GLib-GObject-CRITICAL` is logged
