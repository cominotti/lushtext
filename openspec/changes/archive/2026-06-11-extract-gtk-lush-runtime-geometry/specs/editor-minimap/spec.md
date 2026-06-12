## MODIFIED Requirements

### Requirement: Native minimap highlight remains stable during sidebar animation frames
The native minimap viewport highlight SHALL remain rendered-pixel stable while
workspace-sidebar show and hide animations are in progress. The implementation
MUST preserve the existing native `GtkSourceMap` highlight effect, styling,
interaction behavior, marker layering, and final settled geometry. The
implementation MUST NOT replace the highlight with an app-owned drawing,
restyle or recolor the highlight, or treat final settled correctness as
sufficient when sampled animation frames show drift. During a detected
width-reflow burst, LushText MAY use `gtk-lush-widgets::RenderHoldOverlay` or a
documented compatibility adapter to show a snapshot of the last already
rendered native map pixels, provided the hold is removed after the settle
repair or early user reveal, restores the live source map on every exit path,
and never introduces a new highlight appearance.

#### Scenario: Sidebar show preserves native highlight rows during animation
- **WHEN** the minimap is enabled for a supported document
- **AND** the active editor is scrolled to the top of the document
- **AND** word wrap and the current window width make the workspace-sidebar
  show animation change editor width
- **THEN** every sampled animation frame keeps the native minimap viewport top
  edge within the declared row tolerance
- **AND** sampled frames keep the first rendered minimap content row within the
  declared tolerance
- **AND** the final settled frame still satisfies the existing minimap
  top-content and viewport-overlay requirements

#### Scenario: Sidebar hide preserves native highlight rows during animation
- **WHEN** the minimap is enabled for a supported document
- **AND** the active editor is scrolled to the top of the document
- **AND** word wrap and the current window width make the workspace-sidebar
  hide animation change editor width
- **THEN** every sampled animation frame keeps the native minimap viewport top
  edge within the declared row tolerance
- **AND** sampled frames keep the first rendered minimap content row within the
  declared tolerance
- **AND** the final settled frame still satisfies the existing minimap
  top-content and viewport-overlay requirements

#### Scenario: App geometry cannot excuse rendered native drift
- **WHEN** Automation1 reports stable or expected minimap geometry during a
  sidebar animation
- **AND** screenshot-derived pixel anchors show the native minimap highlight or
  another scenario-declared anchor drifting outside tolerance
- **THEN** the minimap animation invariant fails
- **AND** the failure preserves bounded app-vs-rendered diagnostics for review

#### Scenario: Native effect remains unchanged
- **WHEN** the animation-frame minimap fix is active
- **THEN** the minimap continues to use the native `GtkSourceMap` viewport
  highlight for visible presentation
- **AND** any temporary hold during width reflow is a copy of the native
  rendered map pixels rather than a replacement drawing
- **AND** minimap navigation, read-only behavior, marker layering, focus
  behavior, and final settled appearance remain unchanged from the existing
  native effect

#### Scenario: Animation stability does not depend on marker recomputation
- **WHEN** semantic minimap markers are present while the workspace sidebar
  animates
- **THEN** lightweight native source-map geometry stays synchronized for the
  rendered viewport highlight during sampled frames
- **AND** expensive semantic marker recomputation MAY remain debounced if
  markers settle correctly and do not obscure or contradict the native
  highlight contract

#### Scenario: Render hold restores the live source map
- **WHEN** a native minimap render hold is captured, warmed, revealed,
  superseded, cancelled, or dropped because the editor tab closes
- **THEN** the live source map opacity and visibility are restored
- **AND** no stale captured cover remains visible over the minimap
- **AND** automation-visible minimap state can distinguish an intentional
  in-progress hold from a stuck invisible source map

#### Scenario: User scroll reveals held minimap promptly
- **WHEN** the user scrolls, drags, or clicks the minimap or editor while a
  render hold is waiting for the post-settle reveal
- **THEN** the hold is revealed or cleared promptly
- **AND** the live `GtkSourceMap` handles navigation and viewport updates
  through its normal path
