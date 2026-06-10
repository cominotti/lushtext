## ADDED Requirements

### Requirement: Native minimap highlight remains stable during sidebar animation
The minimap SHALL preserve the existing native `GtkSourceMap` viewport highlight
effect during workspace-sidebar show/hide animation frames. While the active
editor is top-anchored and the sidebar animation changes editor width, the
rendered native viewport top edge and first rendered minimap content row MUST
remain within the visual invariant's declared per-frame tolerance. The system
MUST NOT satisfy this requirement by replacing, hiding, restyling, or cloning the
native highlight.

#### Scenario: Sidebar hide animation does not show transient highlight drift
- **WHEN** the minimap is enabled for a supported document at the top of the file
- **AND** word wrap is enabled
- **AND** the workspace sidebar is hidden from a fully shown state
- **THEN** sampled animation frames keep the screenshot-derived native viewport top edge within tolerance
- **AND** sampled animation frames keep the first rendered minimap content row within tolerance
- **AND** the native highlight remains the existing `GtkSourceMap` effect

#### Scenario: Sidebar show animation does not show transient highlight drift
- **WHEN** the minimap is enabled for a supported document at the top of the file
- **AND** word wrap is enabled
- **AND** the workspace sidebar is shown from a fully hidden state
- **THEN** sampled animation frames keep the screenshot-derived native viewport top edge within tolerance
- **AND** sampled animation frames keep the first rendered minimap content row within tolerance
- **AND** final settled geometry still satisfies the existing final-state minimap invariant

#### Scenario: Animation stability does not depend on marker recomputation
- **WHEN** semantic minimap markers are present while the workspace sidebar animates
- **THEN** lightweight native source-map geometry stays synchronized for the rendered viewport highlight during sampled frames
- **AND** expensive semantic marker recomputation MAY remain debounced if markers settle correctly and do not obscure or contradict the native highlight contract

### Requirement: Native minimap animation sync stays responsive
The minimap SHALL keep animation-frame source-map synchronization lightweight
enough that sidebar animation and editor interaction remain responsive. Any work
performed on the frame path MUST avoid full document scans, unbounded text
snapshots, synchronous filesystem work, or repeated marker rebuilds.

#### Scenario: Frame-path sync avoids expensive document work
- **WHEN** a sidebar animation produces repeated editor width allocations
- **THEN** the minimap frame path synchronizes only bounded native source-map geometry and adjustment state
- **AND** long-line scans, search marker collection, bookmark marker rebuilds, and modified-line marker rebuilds remain debounced or otherwise bounded

#### Scenario: Rapid sidebar toggles do not accumulate stale frame callbacks
- **WHEN** the user toggles the workspace sidebar repeatedly before a previous animation fully settles
- **THEN** stale minimap animation-frame callbacks are ignored by generation or visibility checks
- **AND** the final native highlight and editor scroll state remain correct
