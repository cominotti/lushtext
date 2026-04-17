# workspace-tree-refresh Specification

## Purpose
Keep each workspace section's visible tree aligned with on-disk changes while preserving the section's current browsing context and avoiding visually disruptive rebuilds.

## Requirements

### Requirement: Workspace sections refresh automatically for external filesystem changes
The system SHALL keep each workspace section's visible tree aligned with files and directories inside the sidebar's currently materialized scope when those paths are created, removed, renamed, or moved outside the LushText sidebar workflow. Automatic watching MUST prefer the visible root rows and expanded directories needed to keep the rendered tree current, rather than recursively watching every descendant under a broad configured root at startup.

#### Scenario: External file creation appears in the tree
- **WHEN** a new file is created on disk under a workspace root that is currently visible in the sidebar
- **THEN** the corresponding workspace section shows the new file without requiring the user to replace the root or reopen the workspace

#### Scenario: External removal clears stale rows
- **WHEN** a file or directory that is currently shown in a workspace section is removed on disk outside LushText
- **THEN** the workspace section removes the stale row after refresh processing settles
- **AND** the tree no longer exposes actions for the removed path

#### Scenario: External rename updates the visible tree
- **WHEN** a visible file or directory inside a workspace root is renamed outside LushText
- **THEN** the workspace section stops showing the old path
- **AND** the workspace section shows the renamed path in the correct sorted position

#### Scenario: Broad root with unreadable deep descendants does not block startup
- **WHEN** a workspace root points at a broad directory such as the user's home folder and some deep descendant paths are unreadable to the watcher backend
- **THEN** the workspace section still renders its visible tree without waiting for a recursive watch across every descendant
- **AND** automatic refresh covers the currently materialized root rows and expanded directories
- **AND** the user can still use the manual `Refresh` control for broader reloads

### Requirement: Workspace sections expose a manual refresh control
The system SHALL show a `Refresh` button in each workspace-section header immediately to the left of the existing `Replace Workspace Root` button, and invoking it MUST refresh that workspace section using the same tree-reload behavior as automatic refresh.

#### Scenario: Refresh button placement in the header
- **WHEN** a workspace section header is rendered
- **THEN** it shows a `Refresh` control immediately to the left of the replace-root control
- **AND** the existing replace-root control remains available to the right of it

#### Scenario: Manual refresh reloads stale content
- **WHEN** the user activates the `Refresh` control for a workspace section whose tree is stale
- **THEN** that workspace section reloads its tree from disk
- **AND** newly added, removed, or renamed paths appear in the refreshed result

### Requirement: Manual refresh remains visually stable
The system SHALL keep manual refresh visually stable. Triggering the `Refresh` control MUST NOT blank, flash, collapse, or reconstruct unchanged visible rows in the workspace tree when the currently materialized scope can be reconciled in place.

#### Scenario: Manual refresh keeps unchanged root rows mounted
- **WHEN** the user triggers manual refresh for a workspace whose visible root rows still represent the same paths after the reload
- **THEN** the workspace section keeps the existing root tree model mounted
- **AND** unchanged visible rows remain visually stable while refreshed data is applied

#### Scenario: Expanded workspace refresh avoids subtree blanking
- **WHEN** the user triggers manual refresh while a workspace root or nested directory is expanded
- **THEN** refreshed child rows appear without first blanking the existing subtree contents
- **AND** the refresh preserves expansion and selection for unchanged paths

### Requirement: Refresh preserves section context when possible
The system SHALL preserve the current drill-down scope, expanded rows, and selected row across a refresh whenever the corresponding paths still exist after the refreshed tree is applied.

#### Scenario: Expanded rows stay expanded after refresh
- **WHEN** a workspace section refreshes and an expanded directory still exists afterward
- **THEN** that directory remains expanded in the refreshed tree

#### Scenario: Selection is restored when the path still exists
- **WHEN** the selected file or directory still exists after a workspace-section refresh
- **THEN** the refreshed tree restores selection to that same path

#### Scenario: Removed selection is cleared safely
- **WHEN** the selected path no longer exists after a workspace-section refresh
- **THEN** the refresh completes without leaving a broken selection pointing at a missing path

### Requirement: Automatic refresh remains visually stable
The system SHALL keep automatic workspace refresh visually stable. Refreshing visible rows MUST NOT blank, flash, collapse, or otherwise visibly re-render unchanged portions of the file tree because of watcher noise or subtree reload mechanics.

#### Scenario: Access-only watcher noise is ignored
- **WHEN** the watcher backend emits access or open events that do not change the visible tree shape
- **THEN** the workspace section does not trigger a tree refresh from those events

#### Scenario: Real subtree refresh keeps unchanged rows mounted
- **WHEN** a visible directory refreshes because a child was created, removed, or renamed
- **THEN** unchanged rows in that directory remain visually stable
- **AND** the workspace section does not blank the directory contents before showing the updated result

### Requirement: Refresh failures surface recoverable feedback
The system SHALL surface lightweight user-visible feedback when automatic watching cannot keep a workspace section current or when a manual refresh fails to reload the latest tree state.

#### Scenario: Watcher startup or runtime failure
- **WHEN** automatic workspace refresh cannot start or later stops because the watcher backend fails
- **THEN** the user receives feedback that automatic refresh is unavailable for that workspace section
- **AND** the manual `Refresh` control remains available

#### Scenario: Manual refresh cannot complete
- **WHEN** the user triggers a manual refresh and the workspace section cannot reload the latest tree state
- **THEN** the user receives feedback that the refresh failed
- **AND** the previously rendered tree remains in a usable state
