## ADDED Requirements

### Requirement: Shell transitions preserve editor visual anchors
Adaptive shell transitions SHALL preserve the active editor's top and left visual anchors unless the user has explicitly scrolled away from those anchors. Width-only layout changes, workspace sidebar visibility changes, document-properties pane/sheet changes, compact secondary-surface arbitration, and maximization-like allocation changes MUST NOT create stale scroll adjustments that clip line starts or top content.

#### Scenario: Width-only sidebar transition preserves top and left anchors
- **WHEN** the active editor is scrolled to the top-left origin
- **AND** the workspace sidebar is shown or hidden without changing editor height
- **THEN** the editor remains anchored to the top-left origin after layout settles
- **AND** the minimap top content and viewport overlay use the refreshed editor geometry

#### Scenario: Properties transition does not disturb editor top anchor
- **WHEN** the active editor is scrolled to the top of the document
- **AND** document properties switch between hidden, right-pane, or bottom-sheet presentations
- **THEN** the editor's top visible line remains anchored unless the properties surface intentionally consumes vertical viewport space
- **AND** any intended vertical viewport change is represented in visual geometry state

#### Scenario: Explicit user scroll is respected
- **WHEN** the user has intentionally scrolled horizontally or vertically away from an anchor
- **THEN** shell transition clamping does not force the editor back to the origin
- **AND** the resulting scroll position remains internally consistent with the new adjustment range

### Requirement: Adaptive geometry exposes settled visual state for smoke proof
The adaptive shell SHALL expose enough bounded settled state for smoke helpers to determine whether sidebar, properties, bottom sheet, preview, search panel, status bar, tab strip, and editor content allocations are ready for visual comparison.

#### Scenario: Readiness waits for adaptive layout work
- **WHEN** a visual smoke scenario toggles workspace sidebar or document properties
- **THEN** the visual geometry readiness predicate waits until split-view state, compact-surface state, relevant animations, editor allocation refresh, minimap refresh, and status-bar allocation have settled
- **AND** a timeout reports the first blocker rather than falling back to a blind sleep

#### Scenario: Settled state includes surface rectangles
- **WHEN** Automation1 visual geometry state is requested after adaptive layout settles
- **THEN** it includes bounded rectangles and visibility state for workspace sidebar, document properties, bottom sheet, tab strip, editor viewport, minimap, and status bar when present
- **AND** absent surfaces are represented as intentionally hidden, not omitted ambiguously

### Requirement: Adaptive geometry remains warning-free at visual invariant boundaries
Adaptive shell geometry SHALL remain free of unexpected GTK, Libadwaita, GDK, renderer, and accessibility warnings at visual invariant boundary states.

#### Scenario: Boundary captures fail on geometry warnings
- **WHEN** visual smoke captures widths immediately below, at, and above workspace or properties layout boundaries
- **THEN** unexpected GTK or Libadwaita allocation warnings fail the scenario
- **AND** the warning scan preserves logs with the matching screenshot and geometry state
