## ADDED Requirements

### Requirement: Markdown preview activates supported links
The system SHALL render supported Markdown links in the native read-only preview as activatable content and MUST route activation through the desktop's default external handler instead of in-preview navigation.

#### Scenario: Activate a link in preview body text
- **WHEN** the user activates a supported Markdown link rendered in normal preview text
- **THEN** the system launches the target externally with the default desktop handler
- **AND** the preview remains read-only and keeps the surrounding document flow unchanged

#### Scenario: Activate a link in another preview text context
- **WHEN** a supported Markdown link appears inside another rendered preview text context such as a footnote definition or alert callout
- **THEN** the preview renders it as activatable content there as well
- **AND** activation uses the same external launch path as links in body text
