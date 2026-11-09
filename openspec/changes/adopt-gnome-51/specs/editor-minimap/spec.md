## MODIFIED Requirements

### Requirement: A live minimap is never detached while its update tick can fire
The system SHALL NOT clear the `view` of a `GtkSourceMap` that may have an update tick armed. This holds on every toolkit, including GtkSourceView 5.22 and later, which no longer logs on such a detach (ledger A32): the retire path stays while any supported runtime predates GtkSourceView 5.22, and because retirement also releases the map. When the minimap must stop projecting a document, the system SHALL retire the map instance: hide its overlay, remove it from the widget tree, and drop it. Removal unrealizes the map and disconnects it from the frame clock, so an armed tick never runs; that toolkit behaviour SHALL be a GTK axiom ledger entry with a probe (A45). A later re-attach SHALL construct a fresh map with its view already set, with LushText's established map configuration, and with the reflow freeze and adjustment guard re-pointed at the new instance. On GtkSourceView earlier than 5.22, a regression test SHALL reproduce the toolkit CRITICAL against the previous detach path before the fix lands. On every toolkit, the regression test SHALL fail on any `Gtk-CRITICAL`, `Gtk-WARNING` or `GLib-GObject-CRITICAL` emitted by the minimap lifecycle.

#### Scenario: Unavailable transition after scrolling emits no CRITICAL
- **WHEN** the user scrolls the editor so that the map arms its update tick, and the minimap becomes unavailable in the same frame
- **THEN** no `Gtk-CRITICAL` or `GLib-GObject-CRITICAL` is emitted during that or any later frame

#### Scenario: Load suspension retires and restores the map
- **WHEN** a large load suspends projections and later completes for a document whose policy allows the minimap
- **THEN** the map used during suspension was retired, not detached in place
- **AND** a fresh map with the editor view attached is shown after completion, with the established minimap geometry invariants intact

#### Scenario: Preference toggled rapidly
- **WHEN** the minimap preference is turned off and on repeatedly while the editor scrolls
- **THEN** every transition completes without toolkit warnings, and at most one map instance is attached to the view

#### Scenario: Retirement on GtkSourceView 5.22
- **WHEN** the editor runs on GtkSourceView 5.22 or later and the minimap becomes unavailable after a scroll
- **THEN** the map is still retired, not detached in place
- **AND** no `Gtk-WARNING` about snapshotting the map's slider without an allocation, and no CRITICAL, is emitted during that or any later frame

## ADDED Requirements

### Requirement: The native minimap draws no unallocated slider
On every supported toolkit, the native `GtkSourceMap` and its slider SHALL be allocated in every frame in which they are snapshotted, while the map is shown, hidden, retired, re-attached, or reflowed. GTK SHALL never log `Trying to snapshot GtkSourceMapSlider … without a current allocation` for a LushText editor. If a toolkit change lets the slider be snapshotted unallocated, the change that adopts that toolkit SHALL isolate the behaviour as a ledger axiom with a probe before fixing LushText at the cause. Warning filters SHALL NOT be widened instead.

#### Scenario: Minimap-on editor tests on GTK 4.24
- **WHEN** the widget suite runs on GTK 4.24 / GtkSourceView 5.22 with the minimap enabled
- **THEN** no `GtkSourceMapSlider` snapshot warning appears in any shard's output
