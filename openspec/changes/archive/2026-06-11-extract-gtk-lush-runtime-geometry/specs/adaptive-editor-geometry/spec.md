## ADDED Requirements

### Requirement: Reusable clipping and render-hold preserve shell contracts
After the Phase 3 migration, reusable GTK Lush widget abstractions SHALL
preserve LushText's existing adaptive editor geometry contracts. `ClipBin`
MUST keep flexible content yielding before persistent chrome, and
`RenderHoldOverlay` MUST prevent stale toolkit-rendered minimap frames without
changing requested sidebar/document-properties visibility, focus-mode
suppression, compact-surface arbitration, or final settled layout intent.

#### Scenario: ClipBin preserves persistent bottom chrome
- **WHEN** the main editor shell is constrained to the normal-mode minimum
  supported height after migrating from `LushtextShrinkableBin` to `ClipBin`
- **THEN** the status bar, tab strip, and fixed chrome retain nonzero visible
  allocations
- **AND** optional content yields, clips, or scrolls in its own region instead
  of pushing persistent chrome outside the window

#### Scenario: ClipBin does not add root scrolling
- **WHEN** the shell contains open side surfaces, search results, minimap,
  inline alerts, and awkward editor content inside constrained geometry
- **THEN** no unintended root-level scrollbar is introduced by the reusable
  clipping wrapper
- **AND** GTK and Libadwaita allocation warnings remain absent

#### Scenario: Render hold preserves requested layout intent
- **WHEN** a workspace-sidebar show or hide animation is staged while the
  native minimap is visible
- **THEN** `RenderHoldOverlay` may temporarily present captured native pixels
  for the minimap child
- **AND** workspace requested visibility, document-properties requested
  visibility, compact secondary-surface arbitration, focus mode, and saved
  preferences remain governed by the existing adaptive layout state

#### Scenario: Render hold ends with synchronized live widgets
- **WHEN** a render hold is cleared after the reflow settle repair or revealed
  early because of user interaction
- **THEN** the live editor and minimap widgets are visible, synchronized with
  the final layout, and warning-free
- **AND** no stale cover remains mapped over the final shell
