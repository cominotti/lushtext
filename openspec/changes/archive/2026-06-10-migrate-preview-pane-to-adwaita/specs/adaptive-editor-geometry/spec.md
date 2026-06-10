## ADDED Requirements

### Requirement: Markdown preview uses Adwaita-native presentation
The Markdown preview shell SHALL present editor-only, side-by-side preview, and
preview-only mode through Adwaita-native layout containers instead of an
app-owned `GtkPaned` divider animation. The side-by-side preview MUST be an
explicitly requested end secondary surface for the editor content, and
preview-only mode MUST render the Markdown preview as the focused content area.
The implementation MUST NOT rely on manually animating paned positions,
temporarily changing paned shrink flags, or exposing zero-width paned states to
make preview transitions work.

#### Scenario: Side-by-side preview opens as an end secondary surface
- **WHEN** an active Markdown tab is open in normal editing mode
- **AND** the side-by-side preview target-state action requests visibility
- **THEN** the rendered Markdown preview appears as an end secondary surface for the editor content
- **AND** the editor tab view remains present as the primary content surface
- **AND** no preview-specific `GtkPaned` position animation or shrink-child toggle is required for the state to settle

#### Scenario: Preview-only mode fills the content area
- **WHEN** an active Markdown tab is open
- **AND** the user activates Markdown preview-only mode through `Alt+P`, the primary menu, or the target-state action
- **THEN** the rendered Markdown preview fills the editor content area
- **AND** the side-by-side preview requested state is cleared
- **AND** normal header, tab, status, workspace, and document-properties behavior follows the existing shell contracts for that mode

#### Scenario: Compact side-by-side preview remains explicit
- **WHEN** the window is compact enough that the preview secondary surface cannot consume side-by-side width comfortably
- **AND** the user or automation explicitly requests side-by-side preview
- **THEN** any collapsed or overlay presentation is treated as the active requested preview surface
- **AND** passive window resizing alone does not persist an overlay-obscured editor state
- **AND** persistent chrome, reachable preview dismissal, and the editor left edge remain governed by the adaptive shell contracts

#### Scenario: Preview geometry settles for visual proof
- **WHEN** a visual or widget scenario toggles editor-only, side-by-side preview, or preview-only mode with workspace sidebar, document properties, compact width, or short height states present
- **THEN** readiness waits until the preview layout, relevant Adwaita split or layout-view state, editor allocation refresh, and Markdown embedded widget layout repair have settled
- **AND** the boundary state is free of unexpected GTK, Libadwaita, GDK, renderer, and accessibility warnings
