# adaptive-editor-geometry Specification

## Purpose
Ensure the editor shell, adaptive secondary surfaces, and persistent chrome settle into stable geometry across compact, narrow, and short-window layouts.

## Requirements
### Requirement: Adaptive secondary surfaces settle from stable layout intent
The system SHALL derive the main window's workspace-sidebar and document-properties presentation from stable layout intent: current window width, selected workspace width preset, explicit requested workspace visibility, explicit requested document-properties visibility, focus-mode suppression, and fixed editor-content minimums. The document-properties pane-or-sheet decision MUST NOT depend on temporary rendered workspace visibility caused by compact mutual exclusion, overlay animation, or a previous pass through the same allocation. Applying the derived state MUST be idempotent while those inputs remain unchanged.

#### Scenario: Medium width with both surfaces requested settles once
- **WHEN** the workspace sidebar and document properties are both explicitly requested open
- **AND** the selected workspace width preset is `Comfy`
- **AND** the window width is greater than the no-workspace document-properties guard and less than or equal to the default workspace-aware document-properties guard
- **THEN** document properties render in the compact bottom-sheet presentation
- **AND** the workspace sidebar is temporarily suppressed as the inactive compact secondary surface
- **AND** the shell does not alternate between right-pane and bottom-sheet presentations while the width and requested visibility inputs remain unchanged

#### Scenario: Spacious width restores both requested surfaces
- **WHEN** the workspace sidebar and document properties are both explicitly requested open
- **AND** the window becomes wide enough for the workspace sidebar, editor content, and document-properties pane to consume layout width together
- **THEN** the workspace sidebar renders as a consuming side surface
- **AND** document properties render as a right-side pane
- **AND** the transition does not discard either requested visibility state

#### Scenario: Settled adaptive state stays quiet
- **WHEN** the shell has applied the derived adaptive state for the current window width and requested surface inputs
- **THEN** the workspace split view, properties split view, properties layout, and bottom sheet do not continue emitting state changes caused by the same allocation
- **AND** another state change occurs only after a user action, focus-mode change, preset change, or real window-size change updates the inputs

### Requirement: Persistent bottom chrome remains allocated at supported short heights
The system SHALL preserve the normal-mode bottom status bar and quick editor-state controls at every supported interactive window height. Fixed chrome, the tab strip, the status bar, and a minimal editor viewport MUST fit within the advertised normal-mode minimum height. Optional surfaces such as search results, workspace content, document properties, and editor overlays MUST yield space before persistent bottom chrome is clipped. Focus Mode MAY suppress the status bar according to its existing focused-writing contract.

#### Scenario: Normal minimum height keeps the status bar visible
- **WHEN** the user resizes the main window to the normal-mode minimum supported height
- **THEN** the status bar has a nonzero visible allocation
- **AND** the editor still has a usable viewport
- **AND** GTK and Libadwaita do not report that the root window content exceeds the allocated height

#### Scenario: Search results yield to persistent chrome
- **WHEN** the search panel is open in a short normal-mode window
- **AND** the available height cannot satisfy the search results' comfortable height together with persistent chrome
- **THEN** the search results area shrinks or collapses within the remaining content budget
- **AND** the status bar remains visible and usable

#### Scenario: Optional side surfaces do not force bottom clipping
- **WHEN** the workspace sidebar, document properties, minimap, or editor inline overlays are visible in a short normal-mode window
- **THEN** those optional surfaces do not cause the status bar to disappear below the bottom allocation
- **AND** any necessary truncation, scrolling, or compacting happens inside the optional surface or editor content area

### Requirement: Narrow workspace layouts preserve the editor left edge
The system SHALL keep the active editor's gutter and line starts visible after passive narrow-width transitions. Crossing the workspace split-view collapse threshold MUST NOT leave the workspace sidebar covering the editor's left edge unless the user explicitly opens the compact workspace overlay. Layout-induced horizontal adjustment changes MUST be clamped so they do not masquerade as intentional user horizontal scrolling.

#### Scenario: Passive shrink does not obscure line starts
- **WHEN** the workspace sidebar is requested open in a side-by-side layout
- **AND** the user passively narrows the window past the workspace collapse threshold without activating the sidebar toggle
- **THEN** the editor gutter and beginning of visible text lines remain visible
- **AND** the workspace sidebar does not remain as an unintended overlay covering the editor content

#### Scenario: Explicit compact workspace overlay is distinguishable
- **WHEN** the window is compact and the user explicitly opens the workspace sidebar
- **THEN** any overlay presentation is treated as the active compact secondary surface
- **AND** the user can dismiss or replace that overlay through the existing workspace and document-properties controls
- **AND** the shell does not persist an overlay-obscured editor state as the result of a passive resize alone

#### Scenario: Passive resize does not preserve stale rightward scroll
- **WHEN** the active editor has no explicit user horizontal-scroll intent
- **AND** a passive layout change reduces the editor viewport width
- **THEN** the horizontal adjustment is clamped to the left edge after layout settles
- **AND** the gutter and line starts remain visible even for long-line documents

### Requirement: Shell width budgets match actual widget minima
The system SHALL base split-view fractions, adaptive guards, and side-surface width reservations on the same minimum sizes advertised by the rendered GTK widgets. Shell constants MUST NOT under-budget a surface relative to its template width request or measured minimum. Guard calculations that include the workspace sidebar MUST use the effective clamped workspace width for the relevant requested layout rather than a stale rendered state from compact suppression.

#### Scenario: Properties pane guard budgets the rendered properties surface
- **WHEN** document properties are eligible for right-pane presentation
- **THEN** the right-pane guard reserves at least the document-properties panel's actual minimum width
- **AND** the resulting editor content allocation does not rely on a smaller hard-coded width than the widget advertises

#### Scenario: Workspace preset changes recompute stable guards
- **WHEN** the user changes the workspace sidebar width preset while document properties are requested open
- **THEN** the adaptive guard is recomputed from the selected preset's effective clamped workspace width
- **AND** the shell settles to the correct pane or sheet presentation without oscillating through rendered sidebar visibility

#### Scenario: Guard boundary captures are warning-free
- **WHEN** the window is captured at widths immediately below, at, and above the no-workspace and workspace-aware document-properties guards
- **THEN** the shell presents a stable secondary-surface state at each width
- **AND** GTK and Libadwaita do not report allocation warnings caused by mismatched shell budgets

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
