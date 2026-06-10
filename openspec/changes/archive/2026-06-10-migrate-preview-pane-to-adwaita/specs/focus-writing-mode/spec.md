## ADDED Requirements

### Requirement: Focus Mode preview behavior is shell-independent
Focus Mode SHALL preserve its Markdown preview behavior regardless of whether
the normal preview shell is implemented with a paned widget, an Adwaita split
view, or an Adwaita layout-view slot. Entering Focus Mode MUST suppress
side-by-side preview, remember whether that state should be restored, and keep
`Alt+P` available for focused preview-only mode.

#### Scenario: Side-by-side preview suppresses without leaking an overlay
- **WHEN** side-by-side Markdown preview is visible through the Adwaita-native preview shell
- **AND** the user enters Focus Mode
- **THEN** side-by-side preview is temporarily hidden
- **AND** no collapsed preview overlay, split-view sidebar, or utility pane remains over the focused writing surface
- **AND** the previous side-by-side preview request is remembered for possible restoration

#### Scenario: Focused preview-only mode uses the full writing surface
- **WHEN** Focus Mode is active on a Markdown document in source editing view
- **AND** the user activates Markdown preview-only mode
- **THEN** the rendered Markdown preview fills the focused content area
- **AND** the source text-origin guide is not rendered over the preview
- **AND** Focus Mode remains active

#### Scenario: Exit restores side-by-side preview only when appropriate
- **WHEN** Focus Mode was entered while side-by-side preview was visible
- **AND** the user did not change preview state while focused
- **AND** the user exits Focus Mode
- **THEN** the normal shell restores side-by-side preview through the Adwaita-native preview presentation
- **AND** preview-only mode is not left active accidentally
