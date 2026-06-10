## ADDED Requirements

### Requirement: Shell animations do not expose stale toolkit-owned editor effects
Adaptive shell transitions SHALL avoid presenting mapped editor or minimap
widgets at geometry epochs where toolkit-owned rendered effects are known to be
stale. When a workspace-sidebar animation would repeatedly reallocate the active
editor width while a native minimap is visible, each presented frame MUST either
use a synchronized editor/minimap allocation for that frame or use a
native-pixel freeze plus settle-once repair that prevents stale
intermediate-width native geometry from being visible. This requirement MUST
NOT be satisfied by replacing, restyling, recoloring, or drawing over the native
minimap highlight.

#### Scenario: Sidebar show stages editor width safely
- **WHEN** the workspace sidebar is requested open from a settled hidden state
- **AND** the active editor has a visible native minimap whose rendered effect depends on editor or source-map width
- **THEN** the shell transition does not present a frame where the editor/minimap allocation has advanced but the native minimap highlight still paints from stale slider geometry
- **AND** the transition ends with the requested sidebar visible and the editor/minimap using final settled geometry

#### Scenario: Sidebar hide stages editor width safely
- **WHEN** the workspace sidebar is requested hidden from a settled visible state
- **AND** the active editor has a visible native minimap whose rendered effect depends on editor or source-map width
- **THEN** the shell transition does not present a frame where the editor/minimap allocation has advanced but the native minimap highlight still paints from stale slider geometry
- **AND** the transition ends with the requested sidebar hidden and the editor/minimap using final settled geometry

#### Scenario: Unsupported same-frame sync uses native-pixel freeze instead of visual replacement
- **WHEN** public GTK or GtkSourceView APIs cannot prove that the native source-map highlight is synchronized before the next painted animation frame
- **THEN** the editor may temporarily present a snapshot of the previous native map pixels while the width-reflow burst is active
- **AND** the product does not switch to an app-owned minimap highlight, re-skin the native highlight, or leave the freeze visible after the settled repair

#### Scenario: Layout containment preserves requested visibility intent
- **WHEN** the shell stages a sidebar transition to protect native minimap rendering
- **THEN** workspace-sidebar requested visibility, compact secondary-surface arbitration, document-properties presentation, focus mode suppression, and saved visibility preferences remain governed by the existing adaptive layout intent
- **AND** only the presentation timing of the transition is affected
