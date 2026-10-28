## ADDED Requirements

### Requirement: External file modifications are covered in preview-only mode
The test suite SHALL cover the live file-monitor path while Markdown preview-only mode is active, from an on-disk external modification through the preview-surface alert and the reload action.

#### Scenario: External modification shows the warning on the preview surface
- **WHEN** a Markdown document is open and selected in preview-only mode
- **AND** another process modifies the backing file on disk
- **THEN** the test waits on an observable predicate with a timeout until the preview surface shows "File Has Changed on Disk"
- **AND** the test does not call the notification reducer directly

#### Scenario: Reload from the preview surface refreshes the rendered preview
- **WHEN** the preview surface shows the external-change warning
- **AND** the test activates "Discard Changes and Reload" on the preview surface
- **THEN** the editor buffer equals the new disk bytes
- **AND** the rendered Markdown preview text contains the new content
- **AND** the warning is cleared on both surfaces
