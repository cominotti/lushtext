## MODIFIED Requirements

### Requirement: Workspace sections refresh automatically for external filesystem changes
The system SHALL keep each workspace section's visible folder trees aligned with files and directories inside the sidebar's currently materialized scope when those paths are created, removed, renamed, or moved outside the LushText sidebar workflow. Automatic watching MUST prefer the visible top-level workspace folder rows and expanded directories needed to keep the rendered tree current, rather than recursively watching every descendant under every broad configured folder at startup.

#### Scenario: External file creation appears in the tree
- **WHEN** a new file is created on disk under a workspace folder that is currently visible in the sidebar
- **THEN** the corresponding workspace section shows the new file without requiring the user to remove and re-add the folder or reopen the workspace

#### Scenario: External removal clears stale rows
- **WHEN** a file or directory that is currently shown in a workspace section is removed on disk outside LushText
- **THEN** the workspace section removes the stale row after refresh processing settles
- **AND** the tree no longer exposes actions for the removed path

#### Scenario: External rename updates the visible tree
- **WHEN** a visible file or directory inside a workspace folder is renamed outside LushText
- **THEN** the workspace section stops showing the old path
- **AND** the workspace section shows the renamed path in the correct sorted position

#### Scenario: Broad folder with unreadable deep descendants does not block startup
- **WHEN** a workspace folder points at a broad directory such as the user's home folder and some deep descendant paths are unreadable to the watcher backend
- **THEN** the workspace section still renders its visible tree without waiting for a recursive watch across every descendant
- **AND** automatic refresh covers the currently materialized folder rows and expanded directories
- **AND** the user can still use the manual `Refresh` control for broader reloads

#### Scenario: Zero-folder workspace starts no folder watchers
- **WHEN** a workspace section contains zero folders
- **THEN** automatic refresh does not attempt to watch a fake folder
- **AND** the workspace section remains usable for adding folders

### Requirement: Workspace sections expose a manual refresh control
The system SHALL show a `Refresh` button in each workspace-section header as the rightmost header-control button, and invoking it MUST refresh that workspace section using the same tree-reload behavior as automatic refresh for each configured workspace folder. The refresh control MUST remain available without any adjacent replace-root control.

#### Scenario: Refresh button placement in the header
- **WHEN** a workspace section header is rendered
- **THEN** it shows a `Refresh` control in the rightmost header-control position
- **AND** no replace-root control appears to the right of it

#### Scenario: Manual refresh reloads stale content across folders
- **WHEN** the user activates the `Refresh` control for a workspace section whose folder trees are stale
- **THEN** that workspace section reloads visible tree data for its configured folders
- **AND** newly added, removed, or renamed paths appear in the refreshed result

#### Scenario: Manual refresh of an empty workspace is harmless
- **WHEN** the user activates `Refresh` for a workspace with zero folders
- **THEN** no filesystem tree reload is attempted
- **AND** the section remains visible with its add-folder action available

### Requirement: Manual refresh remains visually stable
The system SHALL keep manual refresh visually stable. Triggering the `Refresh` control MUST NOT blank, flash, collapse, or reconstruct unchanged visible rows in the workspace tree when the currently materialized folder trees can be reconciled in place.

#### Scenario: Manual refresh keeps unchanged folder rows mounted
- **WHEN** the user triggers manual refresh for a workspace whose visible top-level folder rows still represent the same paths after the reload
- **THEN** the workspace section keeps the existing tree models mounted where possible
- **AND** unchanged visible rows remain visually stable while refreshed data is applied

#### Scenario: Expanded workspace refresh avoids subtree blanking
- **WHEN** the user triggers manual refresh while a workspace folder or nested directory is expanded
- **THEN** refreshed child rows appear without first blanking the existing subtree contents
- **AND** the refresh preserves expansion and selection for unchanged paths

#### Scenario: Reordered folders keep stable refreshed content
- **WHEN** folders were reordered before a manual refresh
- **THEN** refresh preserves the persisted folder order
- **AND** it does not restore the old order from stale tree state

### Requirement: Refresh preserves section context when possible
The system SHALL preserve the current drill-down scope, expanded rows, selected row, and top-level folder order across a refresh whenever the corresponding paths still exist after the refreshed tree is applied.

#### Scenario: Expanded rows stay expanded after refresh
- **WHEN** a workspace section refreshes and an expanded directory still exists afterward
- **THEN** that directory remains expanded in the refreshed tree

#### Scenario: Selection is restored when the path still exists
- **WHEN** the selected file or directory still exists after a workspace-section refresh
- **THEN** the refreshed tree restores selection to that same path

#### Scenario: Removed selection is cleared safely
- **WHEN** the selected path no longer exists after a workspace-section refresh
- **THEN** the refresh completes without leaving a broken selection pointing at a missing path

#### Scenario: Folder order survives refresh
- **WHEN** a workspace has folders A, B, and C in persisted order
- **AND** the workspace section refreshes
- **THEN** the top-level folder trees remain ordered A, B, C

### Requirement: Automatic refresh remains visually stable
The system SHALL keep automatic workspace refresh visually stable. Refreshing visible rows MUST NOT blank, flash, collapse, or otherwise visibly re-render unchanged portions of any folder tree because of watcher noise or subtree reload mechanics.

#### Scenario: Access-only watcher noise is ignored
- **WHEN** the watcher backend emits access or open events that do not change the visible tree shape
- **THEN** the workspace section does not trigger a tree refresh from those events

#### Scenario: Real subtree refresh keeps unchanged rows mounted
- **WHEN** a visible directory refreshes because a child was created, removed, or renamed
- **THEN** unchanged rows in that directory remain visually stable
- **AND** the workspace section does not blank the directory contents before showing the updated result

#### Scenario: Overlapping folders refresh independently
- **WHEN** a file under `/repo/src` is visible through both `/repo` and `/repo/src` workspace folder trees
- **AND** that file is changed outside LushText
- **THEN** automatic refresh may update both visible tree locations
- **AND** it does not collapse either folder tree merely because both rows point at the same canonical file

### Requirement: Refresh failures surface recoverable feedback
The system SHALL surface lightweight user-visible feedback when automatic watching cannot keep a workspace section current or when a manual refresh fails to reload the latest tree state for one or more workspace folders.

#### Scenario: Watcher startup or runtime failure
- **WHEN** automatic workspace refresh cannot start or later stops because the watcher backend fails for a workspace folder
- **THEN** the user receives feedback that automatic refresh is unavailable for that workspace section or folder
- **AND** the manual `Refresh` control remains available

#### Scenario: Manual refresh cannot complete
- **WHEN** the user triggers a manual refresh and the workspace section cannot reload the latest tree state
- **THEN** the user receives feedback that the refresh failed
- **AND** the previously rendered tree remains in a usable state
