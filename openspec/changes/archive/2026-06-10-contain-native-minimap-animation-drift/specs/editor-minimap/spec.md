## ADDED Requirements

### Requirement: Native minimap highlight remains stable during sidebar animation frames
The native minimap viewport highlight SHALL remain rendered-pixel stable while
workspace-sidebar show and hide animations are in progress. The implementation
MUST preserve the existing native `GtkSourceMap` highlight effect, styling,
interaction behavior, marker layering, and final settled geometry. The
implementation MUST NOT replace the highlight with an app-owned drawing, restyle
or recolor the highlight, or treat final settled correctness as sufficient when
sampled animation frames show drift. A temporary freeze layer MAY show a
snapshot of the last already-rendered native map pixels during a detected
width-reflow burst, provided it is removed after the settle repair and never
introduces a new highlight appearance.

#### Scenario: Sidebar show preserves native highlight rows during animation
- **WHEN** the minimap is enabled for a supported document
- **AND** the active editor is scrolled to the top of the document
- **AND** word wrap and the current window width make the workspace-sidebar show animation change editor width
- **THEN** every sampled animation frame keeps the native minimap viewport top edge within the declared row tolerance
- **AND** the final settled frame still satisfies the existing minimap top-content and viewport-overlay requirements

#### Scenario: Sidebar hide preserves native highlight rows during animation
- **WHEN** the minimap is enabled for a supported document
- **AND** the active editor is scrolled to the top of the document
- **AND** word wrap and the current window width make the workspace-sidebar hide animation change editor width
- **THEN** every sampled animation frame keeps the native minimap viewport top edge within the declared row tolerance
- **AND** the final settled frame still satisfies the existing minimap top-content and viewport-overlay requirements

#### Scenario: App geometry cannot excuse rendered native drift
- **WHEN** Automation1 reports stable or expected minimap geometry during a sidebar animation
- **AND** screenshot-derived pixel anchors show the native minimap highlight or another scenario-declared anchor drifting outside tolerance
- **THEN** the minimap animation invariant fails
- **AND** the failure preserves bounded app-vs-rendered diagnostics for review

#### Scenario: Native effect remains unchanged
- **WHEN** the animation-frame minimap fix is active
- **THEN** the minimap continues to use the native `GtkSourceMap` viewport highlight for visible presentation
- **AND** any temporary freeze during width reflow is a copy of the native rendered map pixels rather than a replacement drawing
- **AND** minimap navigation, read-only behavior, marker layering, focus behavior, and final settled appearance remain unchanged from the existing native effect
