## MODIFIED Requirements

### Requirement: Minimap availability adapts to high-cost documents without hiding enabled supported tabs
The system SHALL keep the minimap visible on supported editor pages whenever the user's minimap preference is enabled, including when the full document already fits inside the editor viewport. The system SHALL suppress the per-tab minimap instance only in two cases:
- the document scale policy disables the minimap for the active document, because of total size, paragraph length or batch density;
- another unsupported editor state prevents a safe minimap presentation.

Availability MUST be decided from the document's paragraph profile before any `GtkSourceMap` is attached to the document's view. It MUST NOT wait for a post-attach layout analysis. Suppressing the minimap for one document MUST NOT silently disable the user's saved minimap preference for other eligible documents.

#### Scenario: Fully visible document still keeps the minimap visible
- **WHEN** the active document fits entirely inside the current editor viewport and the minimap preference is enabled
- **THEN** the editor page still renders the minimap for that document view
- **AND** the saved minimap preference remains enabled for other supported documents

#### Scenario: Large document keeps the preference but disables the minimap
- **WHEN** the user opens or focuses a document whose scale policy disables the minimap
- **THEN** the editor page does not render the minimap for that document
- **AND** the user receives feedback that the minimap is unavailable for the current document, naming size or line length as the reason

#### Scenario: Giant paragraph never reaches a map
- **WHEN** a small document whose longest paragraph exceeds the minimap paragraph threshold is opened with the minimap preference enabled
- **THEN** no `GtkSourceMap` is attached to its view at any point during load or publication, on the chunked installation path and on the direct path below the synchronous installation threshold alike

#### Scenario: Edit makes the minimap unavailable
- **WHEN** an edit creates a paragraph longer than the minimap paragraph threshold in a document whose minimap is visible
- **THEN** the map instance is retired before the edit's signal emission returns, so it never lays out the new paragraph
- **AND** it is not re-attached for the rest of the document session

### Requirement: Wrapped-layout admission uses conservative live bytes
Minimap wrapped-layout analysis SHALL use the existing O(1) conservative live-buffer estimate: the greater of known file bytes and character count multiplied by four, with saturating arithmetic. It MUST NOT scan or copy buffer text merely to classify size. The exact 2 MiB budget MUST remain eligible, and an estimate one byte above the budget MUST enter bounded long-line analysis when wrapping is enabled. The analysis result MUST only feed marker collection. Minimap availability MUST come from the document scale policy alone.

#### Scenario: Multibyte or untitled content exceeds the threshold
- **WHEN** a modified or untitled buffer has no sufficiently large file-size floor but its character count multiplied by four exceeds 2 MiB
- **THEN** wrapped-layout admission starts bounded long-line analysis
- **AND** it does not treat Unicode scalar count as byte count

#### Scenario: Known file size is the conservative floor
- **WHEN** known file bytes exceed the character-derived estimate
- **THEN** the known file size controls wrapped-layout admission
- **AND** the calculation remains O(1)

#### Scenario: Estimate is exactly at the threshold
- **WHEN** the conservative estimate equals the 2 MiB budget
- **THEN** the ordinary eligible path remains available
- **AND** only an estimate above the budget triggers the large-buffer analysis policy

#### Scenario: Arithmetic would overflow
- **WHEN** the character-count estimate cannot be multiplied by four without overflow
- **THEN** saturating arithmetic classifies it conservatively as large
- **AND** no text scan or allocation is introduced

#### Scenario: Wrapping is disabled or generation becomes stale
- **WHEN** wrapping is disabled, the editor is evicted, or the minimap generation changes
- **THEN** existing disabled, eviction, cancellation, and stale-result behavior remains in force
- **AND** no obsolete analysis result changes the minimap

#### Scenario: Analysis does not decide availability
- **WHEN** wrapped-layout analysis is still running or has reported a long wrapped layout
- **THEN** the minimap's visibility is unchanged by that analysis
- **AND** only marker collection uses its result

## ADDED Requirements

### Requirement: A live minimap is never detached while its update tick can fire
The system SHALL NOT clear the `view` of a `GtkSourceMap` that may have an update tick armed. When the minimap must stop projecting a document, the system SHALL retire the map instance: hide its overlay, remove it from the widget tree and drop it. Removal unrealizes the map and disconnects it from the frame clock, so an armed tick never runs; that toolkit behaviour SHALL be a GTK axiom ledger entry with a probe. A later re-attach SHALL construct a fresh map with its view already set, with LushText's established map configuration, and with the reflow freeze and adjustment guard re-pointed at the new instance. A regression test SHALL reproduce the toolkit CRITICAL against the previous detach path before the fix lands, and it SHALL fail on any `Gtk-CRITICAL` or `GLib-GObject-CRITICAL` emitted by the minimap lifecycle.

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
