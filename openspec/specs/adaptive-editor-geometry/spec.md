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
