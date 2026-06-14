## ADDED Requirements

### Requirement: Workspace sidebar shares the side-rail tone while preserving navigation behavior
The system SHALL render the workspace sidebar on the same coordinated side-rail tone used by the document properties surface. The sidebar MUST keep its navigation-tree presentation: the fixed workspace selector row remains visible, workspace section headers remain the workspace-level control surface, file-tree rows keep their navigation sidebar behavior, dense folder lists scroll only below the fixed selector, and the sidebar MUST NOT gain horizontal scrolling or inspector-style grouped cards as part of this styling change.

#### Scenario: No-workspace and empty-workspace states keep controls reachable
- **WHEN** the app starts with no workspaces or with a workspace that has zero folders
- **THEN** the workspace sidebar uses the shared side-rail tone
- **AND** the fixed workspace selector and new-workspace control remain visible
- **AND** any empty folder-set state remains readable without introducing a fake folder row

#### Scenario: Representative workspace tree keeps navigation styling
- **WHEN** the sidebar shows a workspace with one or more configured folders
- **THEN** top-level folder rows and descendant file rows keep their existing navigation sidebar presentation
- **AND** disclosure, selection, context-menu, file activation, file peek, focus-folder, and inline rename interactions remain visually reachable
- **AND** the rows are not restyled as document-properties inspector cards

#### Scenario: Dense and awkward workspace names preserve constrained geometry
- **WHEN** the sidebar contains many folders, long workspace names, long folder names, duplicate basenames, or overlapping folder paths
- **THEN** the sidebar preserves its no-horizontal-scrollbar contract
- **AND** dense rows scroll only in the workspace-section list area below the fixed selector
- **AND** labels, drag handles, disclosure controls, add-folder, refresh, and workspace header actions do not overlap

#### Scenario: Reorder and hover states remain legible
- **WHEN** workspace folder reorder handles, insertion indicators, row hover states, row selection states, or drop shields are visible
- **THEN** those states remain legible against the shared side-rail tone
- **AND** the styling change does not introduce filled row drop highlights, duplicate insertion feedback, or drop-into-folder cues

#### Scenario: Sidebar and document properties feel coordinated when both are visible
- **WHEN** the window is spacious enough to show the workspace sidebar, editor content, and document properties pane at the same time
- **THEN** the two side surfaces use coordinated side-rail styling
- **AND** the editor content remains visually distinct from the side rails
- **AND** each side surface keeps its own content idiom: workspace as navigation, document properties as inspector
